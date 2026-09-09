//! Integration tests: exercise the real `OmpSessions` adapter against an on-disk
//! fixture omp store, the full `Tutor` pipeline over it, and the actual `tutor`
//! binary end-to-end. These cover the anti-corruption layer (breadcrumb + JSONL
//! parsing) and the app wiring that the in-crate unit tests only touch in pieces.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tutor::adapters::OmpSessions;
use tutor::app::{Clock, SessionSource, Tutor};
use tutor::domain::study::LoopStage;
use tutor::domain::WorkState;

// 2026-09-07T00:00:00.000Z
const T0_MS: u64 = 1_788_739_200_000;

struct FixedClock(SystemTime);
impl Clock for FixedClock {
    fn now(&self) -> SystemTime {
        self.0
    }
}
fn now_clock() -> FixedClock {
    // one hour after the fixture's activity → "recent"
    FixedClock(UNIX_EPOCH + Duration::from_millis(T0_MS + 3_600_000))
}

/// Build a throwaway `~/.omp/agent`-shaped fixture and return its root.
///
/// Layout:
///   <root>/terminal-sessions/ttyTEST      (line1 cwd, line2 session path)
///   <root>/sessions/-proj-acme/<file>.jsonl
fn build_fixture() -> PathBuf {
    build_fixture_at(T0_MS)
}

/// Like [`build_fixture`] but stamps every entry at `ms` (epoch-ms). The e2e
/// binary test uses the real `SystemClock`, so it must build a fixture that is
/// recent relative to *now*, not a fixed calendar date.
fn build_fixture_at(ms: u64) -> PathBuf {
    let uniq = format!(
        "tutor-it-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let root = std::env::temp_dir().join(uniq);
    let bucket = root.join("sessions").join("-proj-acme");
    let crumbs = root.join("terminal-sessions");
    fs::create_dir_all(&bucket).unwrap();
    fs::create_dir_all(&crumbs).unwrap();

    let ts = iso_ms(ms);
    let session_path = bucket.join(format!("{}_abc123.jsonl", ts.replace([':', '.'], "-")));
    let lines = [
        r#"{"type":"title","v":1,"title":"Ship the parser","source":"auto"}"#.to_string(),
        format!(
            r#"{{"type":"session","version":3,"id":"abc123","timestamp":"{ts}","cwd":"/proj/acme","title":"Ship the parser"}}"#
        ),
        format!(
            r#"{{"type":"message","id":"m1","timestamp":"{ts}","message":{{"role":"user","content":[{{"type":"text","text":"为什么会 panic 在这里"}}],"timestamp":{ms}}}}}"#
        ),
        format!(
            r#"{{"type":"message","id":"m2","timestamp":"{ts}","message":{{"role":"user","content":"thanks looks good","timestamp":{ms}}}}}"#
        ),
        format!(
            r#"{{"type":"custom","id":"c1","timestamp":"{ts}","customType":"user_todo_edit","data":{{"phases":[{{"items":[{{"status":"completed"}},{{"status":"in_progress"}},{{"status":"pending"}}]}}]}}}}"#
        ),
        format!(
            r#"{{"type":"custom","id":"c2","timestamp":"{ts}","customType":"tool_execution_start"}}"#
        ),
        format!(
            r#"{{"type":"custom","id":"c3","timestamp":"{ts}","customType":"tool_execution_start"}}"#
        ),
        format!(
            r#"{{"type":"custom","id":"c4","timestamp":"{ts}","customType":"session_exit","data":{{"kind":"normal"}}}}"#
        ),
    ];
    fs::write(&session_path, lines.join("\n")).unwrap();

    // Breadcrumb binds a live terminal to the session.
    fs::write(
        crumbs.join("ttyTEST"),
        format!("/proj/acme\n{}\n", session_path.display()),
    )
    .unwrap();

    root
}

/// Format epoch-ms as `YYYY-MM-DDTHH:MM:SS.mmmZ` (UTC), matching omp's JSONL.
fn iso_ms(ms: u64) -> String {
    let secs = (ms / 1000) as i64;
    let millis = ms % 1000;
    let (h, m, s) = {
        let sod = secs.rem_euclid(86_400);
        (sod / 3600, (sod % 3600) / 60, sod % 60)
    };
    let (y, mo, d) = civil_from_days(secs.div_euclid(86_400));
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}.{millis:03}Z")
}

/// Howard Hinnant's civil-from-days (days since 1970-01-01) → (year, month, day).
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[test]
fn omp_sessions_parses_fixture_into_scanned_session() {
    let root = build_fixture();
    let source = OmpSessions::with_root(&root);
    let scans = source.collect().expect("collect");

    assert_eq!(scans.len(), 1, "one session expected");
    let s = &scans[0];
    assert_eq!(s.session_id, "abc123");
    assert_eq!(s.cwd, "/proj/acme");
    assert_eq!(s.title, "Ship the parser"); // proposition source
    assert_eq!(s.terminal_id.as_deref(), Some("ttyTEST")); // breadcrumb binding
    assert_eq!(s.progress.done, 1);
    assert_eq!(s.progress.in_progress, 1);
    assert_eq!(s.progress.total, 3);
    assert_eq!(s.lifecycle, tutor::domain::Lifecycle::Complete); // session_exit kind=normal
    assert_eq!(s.activity.messages, 2);
    assert_eq!(s.activity.tool_starts, 2);
    // Only the cue-bearing line ("panic") is mined; "thanks looks good" is not.
    assert_eq!(s.snippets.len(), 1);
    assert!(s.snippets[0].text.contains("panic"));

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn pipeline_builds_board_study_and_briefing() {
    let root = build_fixture();
    let source = OmpSessions::with_root(&root);
    let clock = now_clock();
    let tutor = Tutor::new(&source, &clock);

    let scans = tutor.scan().expect("scan");

    // Board: a partially-complete, terminal-bound session sits in Doing.
    let board = tutor.board(&scans);
    assert_eq!(board.total(), 1);
    assert_eq!(board.doing.items.len(), 1);
    let item = &board.doing.items[0];
    assert_eq!(item.proposition, "Ship the parser");
    assert_eq!(
        item.state(
            clock.now(),
            tutor::domain::Board::ACTIVE_WINDOW,
            tutor::domain::Board::STALE
        ),
        WorkState::Doing
    );
    assert_eq!(item.progress.pct(), Some(33));

    // Study: the heuristic seeds one Preview card from the "panic" cue.
    let deck = tutor.seed_heuristic(&scans, Default::default());
    assert_eq!(deck.cards.len(), 1);
    assert_eq!(deck.cards[0].stage, LoopStage::Preview);
    assert!(deck.cards[0].detail.contains("panic"));
    // Preview is always due.
    assert_eq!(deck.due(clock.now()).len(), 1);

    // Briefing: today's rollup carries the active window and the due card.
    let brief = tutor.briefing(&board, &deck);
    assert_eq!(brief.active.len(), 1);
    assert_eq!(brief.active[0].proposition, "Ship the parser");
    assert_eq!(brief.to_study.len(), 1);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn cli_board_renders_fixture_end_to_end() {
    // The binary runs under the real system clock, so the fixture must be recent
    // (within the briefing's 24h window) — stamp it one hour ago.
    let recent_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
        - 3_600_000;
    let root = build_fixture_at(recent_ms);
    let bin = env!("CARGO_BIN_EXE_tutor");

    let out = Command::new(bin)
        .args(["board", "--lang", "en"])
        .env("TUTOR_OMP_DIR", &root)
        .output()
        .expect("run tutor");
    assert!(out.status.success(), "tutor board should exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Doing"),
        "board should show a Doing column:\n{stdout}"
    );
    assert!(
        stdout.contains("Ship the parser"),
        "board should show the proposition:\n{stdout}"
    );
    assert!(
        stdout.contains("33%"),
        "board should show todo progress:\n{stdout}"
    );

    // Briefing (rule-based, no LLM) over the same fixture.
    let brief = Command::new(bin)
        .args(["briefing", "--no-llm", "--lang", "en"])
        .env("TUTOR_OMP_DIR", &root)
        .output()
        .expect("run tutor briefing");
    assert!(brief.status.success());
    let btext = String::from_utf8_lossy(&brief.stdout);
    assert!(
        btext.contains("Ship the parser"),
        "briefing should mention the window:\n{btext}"
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn course_folds_in_sessions_and_advances_topics() {
    // A course folder with a roadmap of two topics.
    let uniq = format!("tutor-course-it-{}", std::process::id());
    let base = std::env::temp_dir().join(uniq);
    let course_dir = base.join("algo");
    fs::create_dir_all(&course_dir).unwrap();
    fs::write(
        course_dir.join("roadmap.md"),
        "# algorithms\n## Patterns\n- [ ] Sliding window technique\n- [ ] Binary search\n",
    )
    .unwrap();
    let course_canon = fs::canonicalize(&course_dir).unwrap();

    // A fixture omp store with one session whose cwd IS the course folder.
    let omp = base.join("omp");
    let bucket = omp.join("sessions").join("-algo");
    fs::create_dir_all(&bucket).unwrap();
    let ts = "2026-09-07T00:00:00.000Z";
    let title = r#"{"type":"title","title":"Debugging sliding window"}"#;
    let header = format!(
        r#"{{"type":"session","version":3,"id":"c1","timestamp":"{ts}","cwd":"{}","title":"Debugging sliding window"}}"#,
        course_canon.display()
    );
    let msg = format!(
        r#"{{"type":"message","id":"m1","timestamp":"{ts}","message":{{"role":"user","content":[{{"type":"text","text":"为什么 sliding window 会报错"}}],"timestamp":{T0_MS}}}}}"#
    );
    let session = format!("{title}\n{header}\n{msg}\n");
    fs::write(bucket.join("2026-09-07T00-00-00-000Z_c1.jsonl"), session).unwrap();

    let source = OmpSessions::with_root(&omp);
    let clock = FixedClock(UNIX_EPOCH + Duration::from_millis(T0_MS + 3_600_000));
    let tutor = Tutor::new(&source, &clock);

    let md = fs::read_to_string(course_dir.join("roadmap.md")).unwrap();
    let deck = tutor.reconcile_course(&course_dir, "algorithms", &md, Default::default(), &[]);

    // The session touched "sliding window" → that topic advanced to Class.
    let sliding = deck
        .cards
        .iter()
        .find(|c| c.topic == "Sliding window technique")
        .expect("sliding topic present");
    assert_eq!(sliding.stage, LoopStage::Class);
    // The untouched topic stays Preview.
    let binary = deck
        .cards
        .iter()
        .find(|c| c.topic == "Binary search")
        .unwrap();
    assert_eq!(binary.stage, LoopStage::Preview);
    // The session's cue ("报错") was mined into an extra card (project != subject).
    assert!(deck.cards.iter().any(|c| c.project != "algorithms"));

    let _ = fs::remove_dir_all(&base);
}
