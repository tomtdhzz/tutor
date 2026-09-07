//! `SessionSource` over the on-disk omp agent store.
//!
//! Layout (see omp `session.md`):
//! - `~/.omp/agent/terminal-sessions/<terminal-id>` — breadcrumb: line 1 = cwd,
//!   line 2 = absolute session file path. Binds a live pane/tab to a session.
//! - `~/.omp/agent/sessions/<encoded-cwd>/<ts>_<id>.jsonl` — the transcript. The
//!   physical file starts with a `type:"title"` slot line, then a `type:"session"`
//!   header, then append-only entries.
//!
//! This is the anti-corruption layer: all omp JSON shapes stay here and are
//! mapped to [`ScannedSession`]. Malformed lines/files are skipped, never fatal.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde_json::Value;

use crate::app::{ScannedSession, SessionSource, Snippet};
use crate::domain::{Activity, Lifecycle, Progress};

/// Cue tokens that mark a user line as a candidate 不会的 (question / trouble).
const CUES: &[&str] = &[
    "报错",
    "错误",
    "为什么",
    "怎么",
    "如何",
    "不会",
    "不懂",
    "不知道",
    "失败",
    "卡住",
    "求助",
    "error",
    "failed",
    "fail ",
    "panic",
    "exception",
    "traceback",
    "why ",
    "how do",
    "how to",
    "cannot",
    "can't",
    "couldn't",
    "doesn't work",
    "not working",
    "stuck",
    "unexpected",
];
/// Keep at most this many mined snippets per session (newest first).
const SNIPPETS_PER_SESSION: usize = 6;

pub struct OmpSessions {
    root: PathBuf,
}

impl OmpSessions {
    /// Default root `~/.omp/agent`, overridable via `TUTOR_OMP_DIR`.
    pub fn new() -> Result<Self> {
        let root = if let Ok(d) = std::env::var("TUTOR_OMP_DIR") {
            PathBuf::from(d)
        } else {
            let home = std::env::var("HOME").context("HOME not set")?;
            PathBuf::from(home).join(".omp").join("agent")
        };
        Ok(OmpSessions { root })
    }

    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        OmpSessions { root: root.into() }
    }

    /// path -> terminal id, for every live breadcrumb.
    fn live_panes(&self) -> HashMap<PathBuf, String> {
        let mut map = HashMap::new();
        let dir = self.root.join("terminal-sessions");
        let Ok(entries) = fs::read_dir(&dir) else {
            return map;
        };
        for e in entries.flatten() {
            let tid = e.file_name().to_string_lossy().to_string();
            let Ok(text) = fs::read_to_string(e.path()) else {
                continue;
            };
            if let Some(path) = text.lines().nth(1) {
                let p = PathBuf::from(path.trim());
                if !p.as_os_str().is_empty() {
                    map.insert(canonical(&p), tid);
                }
            }
        }
        map
    }
}

impl SessionSource for OmpSessions {
    fn collect(&self) -> Result<Vec<ScannedSession>> {
        let live = self.live_panes();
        let mut out = Vec::new();
        let sessions_dir = self.root.join("sessions");
        let Ok(buckets) = fs::read_dir(&sessions_dir) else {
            return Ok(out);
        };
        for bucket in buckets.flatten() {
            if !bucket.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let Ok(files) = fs::read_dir(bucket.path()) else {
                continue;
            };
            for f in files.flatten() {
                let path = f.path();
                if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    continue;
                }
                let tid = live.get(&canonical(&path)).cloned();
                if let Some(s) = parse_session(&path, tid) {
                    out.push(s);
                }
            }
        }
        Ok(out)
    }
}

/// Best-effort canonicalization so breadcrumb and bucket paths compare equal.
fn canonical(p: &Path) -> PathBuf {
    fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

fn parse_session(path: &Path, terminal_id: Option<String>) -> Option<ScannedSession> {
    let text = fs::read_to_string(path).ok()?;

    let mut session_id = String::new();
    let mut cwd = String::new();
    let mut title = String::new();
    let mut first_prompt = String::new();
    let mut created: Option<SystemTime> = None;
    let mut last_activity: Option<SystemTime> = None;
    let mut messages = 0usize;
    let mut tool_starts = 0usize;
    let mut lifecycle = Lifecycle::Unknown;
    let mut progress: Option<Progress> = None;
    let mut snippets: Vec<Snippet> = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let ty = v.get("type").and_then(Value::as_str).unwrap_or("");
        let entry_ts = v
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(iso_to_time);
        if let Some(t) = entry_ts {
            last_activity = Some(last_activity.map_or(t, |c| c.max(t)));
        }

        match ty {
            "title" => {
                if title.is_empty() {
                    if let Some(t) = v.get("title").and_then(Value::as_str) {
                        title = t.trim().to_string();
                    }
                }
            }
            "session" => {
                session_id = v
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                cwd = v
                    .get("cwd")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                if let Some(t) = v.get("title").and_then(Value::as_str) {
                    if title.is_empty() {
                        title = t.trim().to_string();
                    }
                }
                created = v
                    .get("timestamp")
                    .and_then(Value::as_str)
                    .and_then(iso_to_time);
            }
            "message" => {
                messages += 1;
                let msg = v.get("message");
                let role = msg
                    .and_then(|m| m.get("role"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                // Prefer the message's own epoch-ms timestamp for recency.
                if let Some(ms) = msg.and_then(|m| m.get("timestamp")).and_then(Value::as_u64) {
                    let t = UNIX_EPOCH + Duration::from_millis(ms);
                    last_activity = Some(last_activity.map_or(t, |c| c.max(t)));
                }
                if role == "user" {
                    let text = extract_text(msg.and_then(|m| m.get("content")));
                    if !text.is_empty() {
                        if first_prompt.is_empty() {
                            first_prompt = text.clone();
                        }
                        let at = entry_ts.or(created).unwrap_or(UNIX_EPOCH);
                        if is_cue(&text) && snippets.len() < SNIPPETS_PER_SESSION {
                            snippets.push(Snippet {
                                text: clip(&text, 400),
                                at,
                            });
                        }
                    }
                }
            }
            "custom" => match v.get("customType").and_then(Value::as_str).unwrap_or("") {
                "tool_execution_start" => tool_starts += 1,
                "session_exit" => {
                    let kind = v
                        .get("data")
                        .and_then(|d| d.get("kind"))
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    lifecycle = if kind == "normal" {
                        Lifecycle::Complete
                    } else {
                        Lifecycle::Interrupted
                    };
                }
                "user_todo_edit" => {
                    if let Some(p) = v
                        .get("data")
                        .and_then(|d| d.get("phases"))
                        .and_then(parse_progress)
                    {
                        progress = Some(p);
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    // A file with only a header (no messages) and no live pane is not worth a card.
    if session_id.is_empty() {
        return None;
    }
    if messages == 0 && terminal_id.is_none() {
        return None;
    }

    let created = created.unwrap_or(UNIX_EPOCH);
    let last_activity = last_activity.unwrap_or(created);
    if terminal_id.is_some() && lifecycle == Lifecycle::Unknown {
        lifecycle = Lifecycle::Active;
    }

    Some(ScannedSession {
        terminal_id,
        session_id,
        cwd,
        title,
        first_prompt,
        progress: progress.unwrap_or_default(),
        activity: Activity {
            messages,
            tool_starts,
        },
        lifecycle,
        created,
        last_activity,
        snippets,
    })
}

/// Sum todo item statuses across all phases into a [`Progress`].
fn parse_progress(phases: &Value) -> Option<Progress> {
    let arr = phases.as_array()?;
    let (mut done, mut in_progress, mut total) = (0, 0, 0);
    for phase in arr {
        let Some(items) = phase.get("items").and_then(Value::as_array) else {
            continue;
        };
        for it in items {
            total += 1;
            match it
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("pending")
            {
                "completed" | "done" => done += 1,
                "in_progress" => in_progress += 1,
                _ => {}
            }
        }
    }
    if total == 0 {
        return None;
    }
    Some(Progress {
        done,
        in_progress,
        total,
    })
}

/// Join text blocks from a message `content` (string or array of blocks).
fn extract_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.trim().to_string(),
        Some(Value::Array(blocks)) => {
            let mut parts = Vec::new();
            for b in blocks {
                if b.get("type").and_then(Value::as_str) == Some("text") {
                    if let Some(t) = b.get("text").and_then(Value::as_str) {
                        parts.push(t.trim());
                    }
                }
            }
            parts.join(" ").trim().to_string()
        }
        _ => String::new(),
    }
}

fn is_cue(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    CUES.iter().any(|c| lower.contains(c))
}

fn clip(s: &str, max: usize) -> String {
    let flat = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        flat
    } else {
        let mut t: String = flat.chars().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

/// Parse `YYYY-MM-DDTHH:MM:SS(.fff)?Z` into a `SystemTime`. Dependency-free.
fn iso_to_time(s: &str) -> Option<SystemTime> {
    let b = s.as_bytes();
    if b.len() < 19 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' {
        return None;
    }
    let num = |lo: usize, hi: usize| -> Option<i64> { s.get(lo..hi)?.parse().ok() };
    let year = num(0, 4)?;
    let month = num(5, 7)?;
    let day = num(8, 10)?;
    let hour = num(11, 13)?;
    let min = num(14, 16)?;
    let sec = num(17, 19)?;
    let millis: i64 = if b.len() > 20 && b[19] == b'.' {
        // up to 3 fractional digits
        let frac: String = s[20..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .take(3)
            .collect();
        let mut m: i64 = frac.parse().ok()?;
        for _ in frac.len()..3 {
            m *= 10;
        }
        m
    } else {
        0
    };
    let days = days_from_civil(year, month, day);
    let secs = days * 86_400 + hour * 3_600 + min * 60 + sec;
    if secs < 0 {
        return None;
    }
    Some(UNIX_EPOCH + Duration::from_millis((secs * 1000 + millis) as u64))
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_epoch_is_zero() {
        assert_eq!(iso_to_time("1970-01-01T00:00:00.000Z"), Some(UNIX_EPOCH));
    }

    #[test]
    fn iso_known_instant() {
        // 2026-09-07T09:51:43.553Z == 1788767503553 ms
        let t = iso_to_time("2026-09-07T09:51:43.553Z").unwrap();
        let ms = t.duration_since(UNIX_EPOCH).unwrap().as_millis();
        assert_eq!(ms, 1_788_774_703_553);
    }

    #[test]
    fn iso_without_fraction() {
        let t = iso_to_time("2020-01-01T00:00:00Z").unwrap();
        let ms = t.duration_since(UNIX_EPOCH).unwrap().as_millis();
        assert_eq!(ms, 1_577_836_800_000);
    }

    #[test]
    fn cue_detection() {
        assert!(is_cue("为什么会报错"));
        assert!(is_cue("How to fix this panic"));
        assert!(!is_cue("thanks, looks good"));
    }

    #[test]
    fn extract_text_from_blocks() {
        let v: Value = serde_json::from_str(
            r#"[{"type":"text","text":"hi"},{"type":"image"},{"type":"text","text":"there"}]"#,
        )
        .unwrap();
        assert_eq!(extract_text(Some(&v)), "hi there");
    }

    #[test]
    fn progress_sums_statuses() {
        let v: Value = serde_json::from_str(
            r#"[{"items":[{"status":"completed"},{"status":"in_progress"},{"status":"pending"}]}]"#,
        )
        .unwrap();
        assert_eq!(
            parse_progress(&v),
            Some(Progress {
                done: 1,
                in_progress: 1,
                total: 3
            })
        );
    }
}
