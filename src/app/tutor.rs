//! The `Tutor` use case: turn scanned sessions into a work board, keep the
//! study deck fed, and compose/narrate the daily briefing.
//!
//! Everything works with zero LLM calls (rule-based). The `Summarizer` port only
//! *enriches*: it distills better study cards and narrates the briefing prose.

use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use serde::Deserialize;

use super::ports::{Clock, ScannedSession, SessionSource, Snippet, Summarizer};
use crate::domain::study::LoopStage;
use crate::domain::{Board, DailyBriefing, StudyDeck, Unknown, WorkItem};

/// How far back "today" reaches for the briefing.
const BRIEFING_WINDOW: Duration = Duration::from_secs(24 * 3600);
/// Cap on heuristic cards seeded per refresh, newest first.
const HEURISTIC_CARD_CAP: usize = 40;
/// Cap on snippets handed to the LLM in one mining pass.
const MINE_SNIPPET_CAP: usize = 60;

pub struct Tutor<'a> {
    source: &'a dyn SessionSource,
    clock: &'a dyn Clock,
}

impl<'a> Tutor<'a> {
    pub fn new(source: &'a dyn SessionSource, clock: &'a dyn Clock) -> Tutor<'a> {
        Tutor { source, clock }
    }

    pub fn now(&self) -> SystemTime {
        self.clock.now()
    }

    /// Scan every session once; callers reuse the result for board + study +
    /// briefing to avoid re-reading the disk three times.
    pub fn scan(&self) -> Result<Vec<ScannedSession>> {
        self.source.collect()
    }

    /// Build the work board from an already-collected scan.
    pub fn board(&self, scans: &[ScannedSession]) -> Board {
        let now = self.now();
        let items = scans.iter().map(to_item).collect();
        Board::build(items, now)
    }

    /// Seed the deck from heuristic snippet mining (offline, always available).
    /// New cards enter at `Preview` (预习 — 找出不会的).
    pub fn seed_heuristic(&self, scans: &[ScannedSession], base: StudyDeck) -> StudyDeck {
        let mut deck = base;
        let mut snippets: Vec<(&Snippet, &str)> = Vec::new();
        for s in scans {
            for sn in &s.snippets {
                snippets.push((sn, s.project_label()));
            }
        }
        snippets.sort_by_key(|(sn, _)| std::cmp::Reverse(sn.at));
        for (sn, project) in snippets.into_iter().take(HEURISTIC_CARD_CAP) {
            let topic = topic_of(&sn.text);
            deck.upsert(Unknown::seed(
                &topic,
                &sn.text,
                project,
                LoopStage::Preview,
                sn.at,
            ));
        }
        deck
    }

    /// LLM mining pass: distill recent snippets into well-formed cards with a
    /// stage, and merge them into the deck. Refines the heuristic seeds.
    pub fn mine(
        &self,
        summarizer: &dyn Summarizer,
        scans: &[ScannedSession],
        base: StudyDeck,
    ) -> Result<StudyDeck> {
        let now = self.now();
        let mut snippets: Vec<(&Snippet, &str)> = Vec::new();
        for s in scans {
            for sn in &s.snippets {
                snippets.push((sn, s.project_label()));
            }
        }
        snippets.sort_by_key(|(sn, _)| std::cmp::Reverse(sn.at));
        snippets.truncate(MINE_SNIPPET_CAP);
        if snippets.is_empty() {
            return Ok(base);
        }

        let mut prompt = String::from(
            "From the raw excerpts below (a developer's questions and errors from coding sessions), \
             extract the distinct knowledge gaps — 'things they don't yet know'. Merge duplicates. \
             For each, output a concise topic (<=8 words, same language as the excerpt), a one-line \
             detail, and a stage from this loop:\n\
             preview (just surfaced), class (being solved), homework (being tested), \
             review (understood, worth consolidating), correct (resolved).\n\
             Respond with ONLY a JSON array like \
             [{\"topic\":\"...\",\"detail\":\"...\",\"stage\":\"preview\"}]. No prose.\n\nExcerpts:\n",
        );
        for (sn, project) in &snippets {
            prompt.push_str(&format!("- [{}] {}\n", project, one_line(&sn.text, 240)));
        }

        let raw = summarizer
            .run(&prompt)
            .context("mining summarizer call failed")?;
        let cards = parse_mined(&raw).context("could not parse mined cards from model output")?;

        let mut deck = base;
        for c in cards {
            let stage = LoopStage::from_key(&c.stage).unwrap_or(LoopStage::Preview);
            deck.upsert(Unknown::seed(&c.topic, &c.detail, "mined", stage, now));
        }
        Ok(deck)
    }

    /// Compose the structured daily briefing.
    pub fn briefing(&self, board: &Board, deck: &StudyDeck) -> DailyBriefing {
        DailyBriefing::compose(board, deck, self.now(), BRIEFING_WINDOW)
    }

    /// Attach LLM-written narration to a briefing.
    pub fn narrate(
        &self,
        summarizer: &dyn Summarizer,
        briefing: &mut DailyBriefing,
        date: &str,
    ) -> Result<()> {
        if briefing.is_empty() {
            return Ok(());
        }
        let prose = summarizer
            .run(&briefing.to_prompt(date))
            .context("briefing narrator call failed")?;
        let prose = prose.trim().to_string();
        if !prose.is_empty() {
            briefing.prose = Some(prose);
        }
        Ok(())
    }
}

fn to_item(s: &ScannedSession) -> WorkItem {
    let proposition = if s.title.trim().is_empty() {
        one_line(&s.first_prompt, 80)
    } else {
        s.title.clone()
    };
    WorkItem {
        terminal_id: s.terminal_id.clone(),
        session_id: s.session_id.clone(),
        cwd: s.cwd.clone(),
        proposition,
        first_prompt: s.first_prompt.clone(),
        progress: s.progress,
        activity: s.activity,
        lifecycle: s.lifecycle,
        created: s.created,
        last_activity: s.last_activity,
    }
}

impl ScannedSession {
    fn project_label(&self) -> &str {
        self.cwd
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or(&self.cwd)
    }
}

/// A crude topic for heuristic seeding: first sentence/line, clipped.
fn topic_of(text: &str) -> String {
    let first = text
        .split(['\n', '。', '.', '?', '？', '!', '！'])
        .map(str::trim)
        .find(|s| !s.is_empty())
        .unwrap_or(text.trim());
    one_line(first, 48)
}

fn one_line(text: &str, max: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        flat
    } else {
        let mut t: String = flat.chars().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

#[derive(Deserialize)]
struct MinedCard {
    topic: String,
    #[serde(default)]
    detail: String,
    #[serde(default)]
    stage: String,
}

/// Extract the JSON array from a model reply that may wrap it in prose/fences.
fn parse_mined(raw: &str) -> Result<Vec<MinedCard>> {
    let start = raw.find('[').context("no JSON array in model output")?;
    let end = raw.rfind(']').context("unterminated JSON array")?;
    if end < start {
        anyhow::bail!("malformed JSON array bounds");
    }
    let slice = &raw[start..=end];
    let cards: Vec<MinedCard> = serde_json::from_str(slice)?;
    Ok(cards
        .into_iter()
        .filter(|c| !c.topic.trim().is_empty())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topic_takes_first_clause() {
        assert_eq!(
            topic_of("为什么 borrow checker 报错？后面还有内容"),
            "为什么 borrow checker 报错"
        );
    }

    #[test]
    fn parse_mined_handles_fenced_prose() {
        let raw = "Sure!\n```json\n[{\"topic\":\"Rust lifetimes\",\"detail\":\"elision rules\",\"stage\":\"review\"}]\n```\n";
        let cards = parse_mined(raw).unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].topic, "Rust lifetimes");
        assert_eq!(cards[0].stage, "review");
    }

    #[test]
    fn parse_mined_rejects_no_array() {
        assert!(parse_mined("no json here").is_err());
    }
}
