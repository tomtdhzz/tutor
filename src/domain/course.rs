//! Subject-scoped courses: point the tutor at a directory, give it a subject, and
//! it works a learning roadmap (a syllabus of topics) instead of only mining
//! unknowns from sessions.
//!
//! The roadmap is a plain, hand-editable Markdown file (`<dir>/roadmap.md`): a
//! title, an optional note, `## Section` groups, and `- [ ]` / `- [x]` topics.
//! Each topic becomes an [`Unknown`] card — unchecked → `Preview` (default: not
//! yet known), checked → `Correct` (already known). The deck then drives the same
//! study loop and a course kanban.

use std::time::SystemTime;

use super::study::{LoopStage, StudyDeck, Unknown};

/// One roadmap item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Topic {
    pub title: String,
    pub section: String,
    pub done: bool,
}

/// A parsed learning roadmap.
#[derive(Clone, Debug)]
pub struct Syllabus {
    pub subject: String,
    pub topics: Vec<Topic>,
}

impl Syllabus {
    /// Parse a roadmap Markdown document. `## headings` set the current section;
    /// `- [ ]` / `- [x]` lines (or plain `- ` bullets) are topics. The first
    /// `# Title` provides a subject fallback. Everything else is ignored, so the
    /// file can carry freeform notes.
    pub fn parse(md: &str, subject_fallback: &str) -> Syllabus {
        let mut subject = subject_fallback.trim().to_string();
        let mut section = String::new();
        let mut topics = Vec::new();
        let mut seen_title = false;

        for raw in md.lines() {
            let line = raw.trim();
            if let Some(rest) = line.strip_prefix("## ") {
                section = rest.trim().to_string();
            } else if let Some(rest) = line.strip_prefix("# ") {
                if !seen_title {
                    // "# Algorithms — learning roadmap" → subject "Algorithms".
                    let t = rest.trim();
                    let head = t.split(['—', '-', '·', ':']).next().unwrap_or(t).trim();
                    if !head.is_empty() {
                        subject = head.to_string();
                    }
                    seen_title = true;
                }
            } else if let Some(item) = parse_bullet(line) {
                let (done, title) = item;
                if !title.is_empty() {
                    topics.push(Topic {
                        title: title.to_string(),
                        section: section.clone(),
                        done,
                    });
                }
            }
        }
        Syllabus { subject, topics }
    }

    /// Merge this syllabus into a deck. New topics are added; existing topics keep
    /// their in-progress stage, except a roadmap `- [x]` is authoritative and marks
    /// the card `Correct`. A `- [ ]` never resets progress. Returns topics added.
    pub fn merge_into(&self, deck: &mut StudyDeck, now: SystemTime) -> usize {
        let mut added = 0;
        for t in &self.topics {
            let detail = if t.section.is_empty() {
                self.subject.clone()
            } else {
                t.section.clone()
            };
            let id = crate::domain::study::normalize_id(&t.title);
            if let Some(card) = deck.get_mut(&id) {
                if t.done && card.stage != LoopStage::Correct {
                    card.stage = LoopStage::Correct;
                    card.last_seen = now;
                }
            } else {
                let stage = if t.done { LoopStage::Correct } else { LoopStage::Preview };
                deck.add_if_absent(Unknown::seed(&t.title, &detail, &self.subject, stage, now));
                added += 1;
            }
        }
        added
    }
}

/// `- [ ] topic`, `- [x] topic`, or plain `- topic` → (done, title).
fn parse_bullet(line: &str) -> Option<(bool, &str)> {
    let rest = line
        .strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))
        .or_else(|| line.strip_prefix("+ "))?;
    if let Some(r) = rest.strip_prefix("[ ] ") {
        Some((false, r.trim()))
    } else if let Some(r) = rest
        .strip_prefix("[x] ")
        .or_else(|| rest.strip_prefix("[X] "))
    {
        Some((true, r.trim()))
    } else {
        Some((false, rest.trim()))
    }
}

/// The three course-kanban buckets, mapping the loop onto To do / Doing / Done.
/// Preserves deck (roadmap) order within each column.
pub fn columns(deck: &StudyDeck) -> [Vec<&Unknown>; 3] {
    let mut todo = Vec::new();
    let mut doing = Vec::new();
    let mut done = Vec::new();
    for c in &deck.cards {
        match c.stage {
            LoopStage::Preview => todo.push(c),
            LoopStage::Correct => done.push(c),
            _ => doing.push(c),
        }
    }
    [todo, doing, done]
}

/// Mastery progress across the whole course.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CourseProgress {
    pub mastered: usize,
    pub total: usize,
}

impl CourseProgress {
    pub fn of(deck: &StudyDeck) -> CourseProgress {
        let mastered = deck
            .cards
            .iter()
            .filter(|c| c.stage == LoopStage::Correct)
            .count();
        CourseProgress {
            mastered,
            total: deck.cards.len(),
        }
    }
    pub fn pct(&self) -> u8 {
        (self.mastered * 100 + self.total / 2)
            .checked_div(self.total)
            .map_or(0, |v| v.min(100) as u8)
    }
}

/// The prompt handed to `omp -p` to draft a roadmap, grounded on public paths.
pub fn roadmap_prompt(subject: &str) -> String {
    format!(
        "Create a practical, ordered learning roadmap for the subject: \"{subject}\".\n\
         Ground it in well-known public learning paths for this subject (for algorithms/DSA: \
         NeetCode 150, roadmap.sh, and common LeetCode pattern lists; for other subjects, the \
         canonical community curricula). Order sections from fundamentals to advanced.\n\n\
         Output ONLY GitHub-Flavored Markdown, nothing else, in exactly this shape:\n\
         # {subject} — learning roadmap\n\
         > AI-drafted starting point based on well-known public paths — edit freely.\n\n\
         ## <Section name>\n\
         - [ ] <concise topic, learnable in one sitting>\n\
         - [ ] <...>\n\n\
         Provide 6-12 sections, each with 4-10 topics. Keep topic names short (<= 8 words). \
         Do not add commentary outside this structure."
    )
}

/// A minimal offline starter roadmap, used when the LLM is unavailable.
pub fn starter_roadmap(subject: &str) -> String {
    format!(
        "# {subject} — learning roadmap\n\
         > Starter template (offline). Edit freely, then run `tutor course` on this folder.\n\n\
         ## Fundamentals\n\
         - [ ] Define the first thing you don't know about {subject}\n\
         - [ ] Add the next topic\n\n\
         ## Practice\n\
         - [ ] Add a topic you want to test yourself on\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    const MD: &str = "# Algorithms — learning roadmap\n\
> AI-drafted, edit freely.\n\n\
## Arrays & Hashing\n\
- [ ] Two Sum (hashmap complement)\n\
- [x] Contains Duplicate\n\
## Two Pointers\n\
- [ ] Valid Palindrome\n\
* Trapping Rain Water\n";

    fn t() -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1000)
    }

    #[test]
    fn parses_sections_and_checkboxes() {
        let s = Syllabus::parse(MD, "fallback");
        assert_eq!(s.subject, "Algorithms");
        assert_eq!(s.topics.len(), 4);
        assert_eq!(s.topics[0].section, "Arrays & Hashing");
        assert!(!s.topics[0].done);
        assert!(s.topics[1].done); // [x]
        assert_eq!(s.topics[2].section, "Two Pointers");
        assert_eq!(s.topics[3].title, "Trapping Rain Water"); // plain bullet
    }

    #[test]
    fn merge_seeds_stages_and_is_idempotent() {
        let s = Syllabus::parse(MD, "algorithms");
        let mut deck = StudyDeck::default();
        assert_eq!(s.merge_into(&mut deck, t()), 4);
        // checked topic → Correct, others Preview
        let dup = deck
            .get_mut(&super::super::study::normalize_id("Contains Duplicate"))
            .unwrap();
        assert_eq!(dup.stage, LoopStage::Correct);
        // re-merge adds nothing and does not reset progress
        deck.get_mut(&super::super::study::normalize_id(
            "Two Sum (hashmap complement)",
        ))
        .unwrap()
        .promote(t()); // Preview -> Class
        assert_eq!(s.merge_into(&mut deck, t()), 0);
        assert_eq!(
            deck.get_mut(&super::super::study::normalize_id(
                "Two Sum (hashmap complement)"
            ))
            .unwrap()
            .stage,
            LoopStage::Class
        );
    }

    #[test]
    fn columns_bucket_by_stage() {
        let s = Syllabus::parse(MD, "algorithms");
        let mut deck = StudyDeck::default();
        s.merge_into(&mut deck, t());
        let [todo, doing, done] = columns(&deck);
        assert_eq!(todo.len(), 3); // three unchecked
        assert_eq!(doing.len(), 0);
        assert_eq!(done.len(), 1); // Contains Duplicate
        assert_eq!(CourseProgress::of(&deck).pct(), 25);
    }
}
