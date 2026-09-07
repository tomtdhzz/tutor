//! The study loop over "the things you don't know" (不会的).
//!
//! The organising idea, in the user's own words:
//!
//! - 预习是找出不会的   — Preview surfaces what you don't know.
//! - 听课是解决不会的   — Class solves what you don't know.
//! - 作业是检验不会的   — Homework tests what you don't know.
//! - 复习是死磕不会的   — Review grinds what you don't know.
//! - 改错是消灭不会的   — Correcting errors eliminates what you don't know.
//!
//! Every mined item ("Unknown") is a single 不会的 travelling through those five
//! stages. `Review` is where consolidation (沉淀) lives, and it is the only stage
//! with a spaced-repetition schedule.

use std::time::{Duration, SystemTime};

/// The five stages of the loop, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopStage {
    /// 预习 — surfaced, not yet addressed.
    Preview,
    /// 听课 — actively being solved.
    Class,
    /// 作业 — being tested / applied.
    Homework,
    /// 复习 — consolidated; grind on a spaced schedule.
    Review,
    /// 改错 — resolved / eliminated.
    Correct,
}

impl LoopStage {
    pub const ALL: [LoopStage; 5] = [
        LoopStage::Preview,
        LoopStage::Class,
        LoopStage::Homework,
        LoopStage::Review,
        LoopStage::Correct,
    ];

    /// Stable index, also used as the on-disk encoding.
    pub fn index(self) -> usize {
        match self {
            LoopStage::Preview => 0,
            LoopStage::Class => 1,
            LoopStage::Homework => 2,
            LoopStage::Review => 3,
            LoopStage::Correct => 4,
        }
    }

    pub fn from_index(i: usize) -> LoopStage {
        LoopStage::ALL[i.min(4)]
    }

    /// Lowercase ascii key accepted from LLM mining output.
    pub fn from_key(s: &str) -> Option<LoopStage> {
        match s.trim().to_ascii_lowercase().as_str() {
            "preview" | "预习" => Some(LoopStage::Preview),
            "class" | "听课" => Some(LoopStage::Class),
            "homework" | "作业" => Some(LoopStage::Homework),
            "review" | "复习" => Some(LoopStage::Review),
            "correct" | "改错" | "resolved" => Some(LoopStage::Correct),
            _ => None,
        }
    }
}

/// One 不会的: a knowledge point / gap mined from sessions.
#[derive(Clone, Debug)]
pub struct Unknown {
    /// Stable id derived from the normalized topic.
    pub id: String,
    pub topic: String,
    pub detail: String,
    pub project: String,
    pub stage: LoopStage,
    /// How many distinct occurrences fed this card.
    pub times_seen: u32,
    /// How many spaced reviews have happened (Review stage only).
    pub reviews: u32,
    pub first_seen: SystemTime,
    pub last_seen: SystemTime,
    pub next_review: SystemTime,
}

impl Unknown {
    /// Spaced-repetition intervals (days) indexed by review count.
    const INTERVAL_DAYS: [u64; 5] = [1, 3, 7, 14, 30];

    fn interval(reviews: u32) -> Duration {
        let d = Self::INTERVAL_DAYS[(reviews as usize).min(Self::INTERVAL_DAYS.len() - 1)];
        Duration::from_secs(d * 86_400)
    }

    /// A fresh card seeded from a mined occurrence, scheduled for its first review.
    pub fn seed(
        topic: &str,
        detail: &str,
        project: &str,
        stage: LoopStage,
        at: SystemTime,
    ) -> Unknown {
        Unknown {
            id: normalize_id(topic),
            topic: topic.trim().to_string(),
            detail: detail.trim().to_string(),
            project: project.to_string(),
            stage,
            times_seen: 1,
            reviews: 0,
            first_seen: at,
            last_seen: at,
            next_review: at + Self::interval(0),
        }
    }

    /// Whether this card wants attention now:
    /// anything not yet eliminated that is either freshly surfaced (Preview) or
    /// whose spaced review has come due.
    pub fn is_due(&self, now: SystemTime) -> bool {
        match self.stage {
            LoopStage::Correct => false,
            LoopStage::Preview => true,
            _ => self.next_review <= now,
        }
    }

    /// Record a completed review and push out the next one.
    pub fn reschedule(&mut self, now: SystemTime) {
        self.reviews = self.reviews.saturating_add(1);
        self.last_seen = now;
        self.next_review = now + Self::interval(self.reviews);
    }
}

/// Normalize a topic into a stable id: lowercase, collapse whitespace.
pub fn normalize_id(topic: &str) -> String {
    topic
        .to_ascii_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The deck of mined unknowns.
#[derive(Clone, Debug, Default)]
pub struct StudyDeck {
    pub cards: Vec<Unknown>,
}

impl StudyDeck {
    pub fn new(cards: Vec<Unknown>) -> StudyDeck {
        StudyDeck { cards }
    }

    /// Merge a freshly-mined card. If an id already exists, bump its recency and
    /// occurrence count and adopt the newer stage/detail; otherwise insert.
    pub fn upsert(&mut self, incoming: Unknown) {
        if let Some(existing) = self.cards.iter_mut().find(|c| c.id == incoming.id) {
            existing.times_seen = existing.times_seen.saturating_add(1);
            existing.last_seen = existing.last_seen.max(incoming.last_seen);
            existing.stage = incoming.stage;
            if !incoming.detail.is_empty() {
                existing.detail = incoming.detail;
            }
        } else {
            self.cards.push(incoming);
        }
    }

    /// Cards wanting attention now, most-recently-seen first.
    pub fn due(&self, now: SystemTime) -> Vec<&Unknown> {
        let mut v: Vec<&Unknown> = self.cards.iter().filter(|c| c.is_due(now)).collect();
        v.sort_by_key(|c| std::cmp::Reverse(c.last_seen));
        v
    }

    pub fn by_stage(&self, stage: LoopStage) -> Vec<&Unknown> {
        let mut v: Vec<&Unknown> = self.cards.iter().filter(|c| c.stage == stage).collect();
        v.sort_by_key(|c| std::cmp::Reverse(c.last_seen));
        v
    }

    /// Count of cards per stage, indexed by [`LoopStage::index`].
    pub fn counts(&self) -> [usize; 5] {
        let mut c = [0usize; 5];
        for card in &self.cards {
            c[card.stage.index()] += 1;
        }
        c
    }

    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
    }

    #[test]
    fn preview_is_always_due() {
        let u = Unknown::seed("borrow checker", "d", "p", LoopStage::Preview, t(0));
        assert!(u.is_due(t(0)));
    }

    #[test]
    fn correct_is_never_due() {
        let u = Unknown::seed("x", "d", "p", LoopStage::Correct, t(0));
        assert!(!u.is_due(t(10_000_000)));
    }

    #[test]
    fn review_due_after_interval() {
        let mut u = Unknown::seed("x", "d", "p", LoopStage::Review, t(0));
        // seeded next_review = 0 + 1 day
        assert!(!u.is_due(t(3600)));
        assert!(u.is_due(t(86_400)));
        u.reschedule(t(86_400)); // reviews -> 1, interval 3d
        assert!(!u.is_due(t(86_400 + 3600)));
        assert!(u.is_due(t(86_400 + 3 * 86_400)));
    }

    #[test]
    fn upsert_merges_by_normalized_topic() {
        let mut deck = StudyDeck::default();
        deck.upsert(Unknown::seed(
            "Borrow  Checker",
            "a",
            "p",
            LoopStage::Preview,
            t(1),
        ));
        deck.upsert(Unknown::seed(
            "borrow checker",
            "b",
            "p",
            LoopStage::Class,
            t(5),
        ));
        assert_eq!(deck.cards.len(), 1);
        assert_eq!(deck.cards[0].times_seen, 2);
        assert_eq!(deck.cards[0].stage, LoopStage::Class);
        assert_eq!(deck.cards[0].detail, "b");
    }

    #[test]
    fn counts_by_stage() {
        let mut deck = StudyDeck::default();
        deck.upsert(Unknown::seed("a", "", "p", LoopStage::Preview, t(1)));
        deck.upsert(Unknown::seed("b", "", "p", LoopStage::Review, t(1)));
        deck.upsert(Unknown::seed("c", "", "p", LoopStage::Review, t(1)));
        let c = deck.counts();
        assert_eq!(c[LoopStage::Preview.index()], 1);
        assert_eq!(c[LoopStage::Review.index()], 2);
    }
}
