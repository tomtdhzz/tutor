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

    /// The next stage toward mastery (Correct is terminal).
    pub fn next(self) -> LoopStage {
        LoopStage::from_index((self.index() + 1).min(4))
    }

    /// The previous stage (Preview is the floor).
    pub fn prev(self) -> LoopStage {
        LoopStage::from_index(self.index().saturating_sub(1))
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
    /// Per-problem completion for this topic's lesson (empty until a lesson is
    /// opened). Length = number of problems; each `true` = a problem the learner
    /// has worked through and marked 已掌握. Drives fine-grained course progress.
    pub solved: Vec<bool>,
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
            solved: Vec::new(),
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

    /// Move one stage toward mastery, stamping recency. Entering Review schedules
    /// the first spaced review from `now`.
    pub fn promote(&mut self, now: SystemTime) {
        let next = self.stage.next();
        if next == LoopStage::Review && self.stage != LoopStage::Review {
            self.reviews = 0;
            self.next_review = now + Self::interval(0);
        }
        self.stage = next;
        self.last_seen = now;
    }

    /// Move one stage back toward Preview, stamping recency.
    pub fn demote(&mut self, now: SystemTime) {
        self.stage = self.stage.prev();
        self.last_seen = now;
    }

    /// Resize the per-problem completion vector to `n`, preserving existing marks.
    /// Called when a lesson is opened/generated so the deck knows how many
    /// problems the topic has. Never changes the stage on its own.
    pub fn sync_problems(&mut self, n: usize) {
        if self.solved.len() != n {
            self.solved.resize(n, false);
        }
    }

    /// Toggle problem `i`'s 已掌握 mark, stamping recency. No-op if out of range.
    pub fn toggle_solved(&mut self, i: usize, now: SystemTime) {
        if let Some(b) = self.solved.get_mut(i) {
            *b = !*b;
            self.last_seen = now;
        }
    }

    /// How many of this topic's problems are marked done.
    pub fn solved_count(&self) -> usize {
        self.solved.iter().filter(|b| **b).count()
    }

    /// Number of problems in this topic's lesson (0 until a lesson is opened).
    pub fn problems_total(&self) -> usize {
        self.solved.len()
    }

    /// Re-derive the kanban stage from problem completion — for lesson-bearing
    /// topics, problem progress *is* the stage: none → Preview, some → Class, all
    /// → Correct. Topics without a lesson keep their manually-set stage.
    pub fn sync_stage_from_problems(&mut self, now: SystemTime) {
        let n = self.solved.len();
        if n == 0 {
            return;
        }
        let done = self.solved_count();
        let next = if done == n {
            LoopStage::Correct
        } else if done > 0 {
            LoopStage::Class
        } else {
            LoopStage::Preview
        };
        if next != self.stage {
            self.stage = next;
            self.last_seen = now;
        }
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

    /// Insert a card only if its id is not already present. Used when re-parsing a
    /// roadmap so hand edits add new topics without resetting progress on existing.
    pub fn add_if_absent(&mut self, card: Unknown) -> bool {
        if self.cards.iter().any(|c| c.id == card.id) {
            return false;
        }
        self.cards.push(card);
        true
    }

    /// Mutable access to a card by id (for manual stage promotion in the TUI).
    pub fn get_mut(&mut self, id: &str) -> Option<&mut Unknown> {
        self.cards.iter_mut().find(|c| c.id == id)
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

/// One entry on the dated review agenda: a Review-stage card and how many whole
/// days remain until its next spaced review (negative = overdue).
#[derive(Clone, Copy, Debug)]
pub struct ReviewEntry<'a> {
    pub card: &'a Unknown,
    pub days_until: i64,
}

/// The spaced-repetition agenda (复习计划), bucketed by due date. Consolidation
/// (沉淀) lives in the Review stage, so only Review-stage cards carry a real
/// schedule and only they appear here — a fresh course has an empty plan until a
/// topic is walked into Review.
#[derive(Clone, Debug, Default)]
pub struct ReviewPlan<'a> {
    pub overdue: Vec<ReviewEntry<'a>>,
    pub today: Vec<ReviewEntry<'a>>,
    pub week: Vec<ReviewEntry<'a>>,
    pub later: Vec<ReviewEntry<'a>>,
}

impl<'a> ReviewPlan<'a> {
    /// Bucket every Review-stage card by whole days until its next review.
    pub fn of(deck: &'a StudyDeck, now: SystemTime) -> ReviewPlan<'a> {
        let mut plan = ReviewPlan::default();
        let mut entries: Vec<ReviewEntry<'a>> = deck
            .cards
            .iter()
            .filter(|c| c.stage == LoopStage::Review)
            .map(|c| ReviewEntry {
                card: c,
                days_until: days_between(now, c.next_review),
            })
            .collect();
        entries.sort_by_key(|e| (e.days_until, std::cmp::Reverse(e.card.last_seen)));
        for e in entries {
            match e.days_until {
                d if d < 0 => plan.overdue.push(e),
                0 => plan.today.push(e),
                1..=7 => plan.week.push(e),
                _ => plan.later.push(e),
            }
        }
        plan
    }

    pub fn total(&self) -> usize {
        self.overdue.len() + self.today.len() + self.week.len() + self.later.len()
    }

    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }
}

/// Whole calendar days from `now`'s date to `then`'s date (UTC epoch-day basis).
/// Date-based (not raw seconds) so it matches the `YYYY-MM-DD` shown in
/// `course state` and is stable across sub-second drift between processes: a
/// review scheduled for tomorrow always reads `1`, an instant earlier today `0`,
/// and yesterday `-1`.
pub fn days_between(now: SystemTime, then: SystemTime) -> i64 {
    fn epoch_day(t: SystemTime) -> i64 {
        t.duration_since(std::time::UNIX_EPOCH)
            .map(|d| (d.as_secs() / 86_400) as i64)
            .unwrap_or(0)
    }
    epoch_day(then) - epoch_day(now)
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
    fn promote_and_demote_walk_the_loop() {
        let mut u = Unknown::seed("x", "d", "p", LoopStage::Preview, t(0));
        u.promote(t(10)); // Preview -> Class
        assert_eq!(u.stage, LoopStage::Class);
        u.promote(t(20)); // Class -> Homework
        u.promote(t(30)); // Homework -> Review: first review scheduled from now
        assert_eq!(u.stage, LoopStage::Review);
        assert_eq!(u.reviews, 0);
        assert_eq!(u.next_review, t(30) + Duration::from_secs(86_400));
        u.promote(t(40)); // Review -> Correct
        u.promote(t(50)); // Correct is terminal
        assert_eq!(u.stage, LoopStage::Correct);
        u.demote(t(60)); // Correct -> Review
        assert_eq!(u.stage, LoopStage::Review);
        assert_eq!(LoopStage::Preview.prev(), LoopStage::Preview); // floor
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

    #[test]
    fn days_between_is_calendar_based() {
        let day = 86_400;
        assert_eq!(days_between(t(0), t(0)), 0);
        assert_eq!(days_between(t(0), t(day + 3600)), 1); // tomorrow, +1h → 1
        assert_eq!(days_between(t(day), t(0)), -1); // yesterday
        assert_eq!(days_between(t(3600), t(day - 60)), 0); // same UTC day → 0
    }

    #[test]
    fn review_plan_buckets_only_review_cards_by_due_date() {
        let day = 86_400;
        let now = t(30 * day); // day 30
        let mut deck = StudyDeck::default();
        // Not in Review → excluded from the plan.
        deck.upsert(Unknown::seed("preview", "", "p", LoopStage::Preview, now));
        deck.upsert(Unknown::seed("homework", "", "p", LoopStage::Homework, now));
        // Review cards with hand-set next_review dates.
        let mut over = Unknown::seed("overdue", "", "p", LoopStage::Review, now);
        over.next_review = t(29 * day); // yesterday
        deck.upsert(over);
        let mut today = Unknown::seed("today", "", "p", LoopStage::Review, now);
        today.next_review = t(30 * day + 3600); // later today
        deck.upsert(today);
        let mut week = Unknown::seed("week", "", "p", LoopStage::Review, now);
        week.next_review = t(33 * day); // +3d
        deck.upsert(week);
        let mut later = Unknown::seed("later", "", "p", LoopStage::Review, now);
        later.next_review = t(60 * day); // +30d
        deck.upsert(later);

        let plan = ReviewPlan::of(&deck, now);
        assert_eq!(plan.total(), 4); // preview/homework excluded
        assert_eq!(plan.overdue.len(), 1);
        assert_eq!(plan.overdue[0].card.topic, "overdue");
        assert_eq!(plan.overdue[0].days_until, -1);
        assert_eq!(plan.today.len(), 1);
        assert_eq!(plan.today[0].days_until, 0);
        assert_eq!(plan.week.len(), 1);
        assert_eq!(plan.week[0].days_until, 3);
        assert_eq!(plan.later.len(), 1);
        assert_eq!(plan.later[0].days_until, 30);
    }

    #[test]
    fn empty_review_plan_when_nothing_reached_review() {
        let mut deck = StudyDeck::default();
        deck.upsert(Unknown::seed("a", "", "p", LoopStage::Preview, t(0)));
        deck.upsert(Unknown::seed("b", "", "p", LoopStage::Class, t(0)));
        assert!(ReviewPlan::of(&deck, t(0)).is_empty());
    }
}
