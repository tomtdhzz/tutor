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
                let stage = if t.done {
                    LoopStage::Correct
                } else {
                    LoopStage::Preview
                };
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

/// Fine-grained course progress, counted by *problems*, not whole topics.
///
/// Each topic contributes its lesson's solved/total problems; a topic without a
/// lesson yet counts as a single unsolved problem (solved only if mastered by a
/// roadmap `- [x]` / the `.` key). So the bar reflects how many problems you have
/// actually worked through, and partly-done topics move it fractionally.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CourseProgress {
    /// Topics fully done (all problems solved, or mastered when lesson-less).
    pub mastered: usize,
    /// Total topic cards.
    pub total: usize,
    /// Problems marked done across the course.
    pub solved_problems: usize,
    /// Total problems across the course (lesson-less topics count as 1).
    pub total_problems: usize,
}

impl CourseProgress {
    pub fn of(deck: &StudyDeck) -> CourseProgress {
        let mut mastered = 0;
        let mut solved_problems = 0;
        let mut total_problems = 0;
        for c in &deck.cards {
            let n = c.problems_total();
            if n > 0 {
                let done = c.solved_count();
                solved_problems += done;
                total_problems += n;
                if done == n {
                    mastered += 1;
                }
            } else {
                total_problems += 1;
                if c.stage == LoopStage::Correct {
                    solved_problems += 1;
                    mastered += 1;
                }
            }
        }
        CourseProgress {
            mastered,
            total: deck.cards.len(),
            solved_problems,
            total_problems,
        }
    }
    pub fn pct(&self) -> u8 {
        (self.solved_problems * 100 + self.total_problems / 2)
            .checked_div(self.total_problems)
            .map_or(0, |v| v.min(100) as u8)
    }
}

/// Whether `path` is `base` or lives beneath it. Both should be canonicalized by
/// the caller; comparison is a boundary-aware string prefix.
pub fn is_under(path: &str, base: &str) -> bool {
    let p = path.trim_end_matches('/');
    let b = base.trim_end_matches('/');
    !b.is_empty() && (p == b || p.starts_with(&format!("{b}/")))
}

/// Distinctive tokens of a topic title: alphanumeric words of >=5 chars,
/// lowercased. Used to match a topic against session/file signals.
pub fn topic_tokens(title: &str) -> Vec<String> {
    title
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 5)
        .map(|w| w.to_lowercase())
        .collect()
}

/// Auto-advance roadmap topics you've started touching: a `Preview` card whose
/// project matches `subject` (i.e. a roadmap topic, not a session-mined cue) and
/// any of whose distinctive tokens appears in a signal is promoted one stage
/// (→ Class). Conservative by design: it never advances past Class and never
/// downgrades, so mastery stays a deliberate act (`- [x]` or the `.` key).
/// Signals must be lowercased by the caller. Returns the number advanced.
pub fn advance_roadmap(
    deck: &mut StudyDeck,
    subject: &str,
    signals: &[String],
    now: SystemTime,
) -> usize {
    let mut advanced = 0;
    for card in deck.cards.iter_mut() {
        if card.project != subject || card.stage != LoopStage::Preview {
            continue;
        }
        let tokens = topic_tokens(&card.topic);
        let touched = tokens
            .iter()
            .any(|tok| signals.iter().any(|s| s.contains(tok)));
        if touched {
            card.promote(now); // Preview -> Class
            advanced += 1;
        }
    }
    advanced
}

/// The prompt handed to `omp -p` to draft a roadmap, grounded on public paths.
/// `lang` is a natural-language instruction (from the presentation layer) telling
/// the model which language to write section/topic names in.
pub fn roadmap_prompt(subject: &str, lang: &str) -> String {
    format!(
        "Create a practical, ordered learning roadmap for the subject: \"{subject}\".\n\
         Ground it in well-known public learning paths for this subject (for algorithms/DSA: \
         NeetCode 150, roadmap.sh, and common LeetCode pattern lists; for other subjects, the \
         canonical community curricula). Order sections from fundamentals to advanced.\n\
         {lang}\n\n\
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

    #[test]
    fn problem_completion_drives_stage_and_progress() {
        let mut deck = StudyDeck::default();
        deck.upsert(Unknown::seed(
            "二分查找",
            "d",
            "algo",
            LoopStage::Preview,
            t(),
        ));
        let id = crate::domain::study::normalize_id("二分查找");

        let card = deck.get_mut(&id).unwrap();
        card.sync_problems(4);
        assert_eq!(card.problems_total(), 4);

        // One problem done → topic is in progress (Class), 1/4 of its problems.
        card.toggle_solved(0, t());
        card.sync_stage_from_problems(t());
        assert_eq!(card.stage, LoopStage::Class);
        let p = CourseProgress::of(&deck);
        assert_eq!((p.solved_problems, p.total_problems), (1, 4));
        assert_eq!(p.pct(), 25);

        // All problems done → topic mastered (Correct), progress full.
        let card = deck.get_mut(&id).unwrap();
        for i in 1..4 {
            card.toggle_solved(i, t());
        }
        card.sync_stage_from_problems(t());
        assert_eq!(card.stage, LoopStage::Correct);
        let p = CourseProgress::of(&deck);
        assert_eq!(p.mastered, 1);
        assert_eq!(p.pct(), 100);
    }

    #[test]
    fn is_under_respects_boundaries() {
        assert!(is_under("/a/b/algo", "/a/b/algo"));
        assert!(is_under("/a/b/algo/sub", "/a/b/algo"));
        assert!(!is_under("/a/b/algorithms", "/a/b/algo")); // not a path boundary
        assert!(!is_under("/a/b", "/a/b/algo"));
        assert!(!is_under("/a/b/algo", ""));
    }

    #[test]
    fn topic_tokens_extracts_significant_words() {
        assert_eq!(
            topic_tokens("Recursion and call stack"),
            vec!["recursion", "stack"]
        );
        assert_eq!(topic_tokens("Big-O, big-Θ notation"), vec!["notation"]);
        assert!(topic_tokens("A x y").is_empty()); // nothing >= 5 chars
    }

    #[test]
    fn advance_only_touched_roadmap_previews() {
        let s = Syllabus::parse(MD, "algorithms");
        let mut deck = StudyDeck::default();
        s.merge_into(&mut deck, t());
        // A session-mined cue card (different project) must never be advanced here.
        deck.upsert(Unknown::seed(
            "panic somewhere",
            "d",
            "you2php",
            LoopStage::Preview,
            t(),
        ));
        let signals = vec!["i was working on valid palindrome today".to_string()];
        let n = advance_roadmap(&mut deck, &s.subject, &signals, t());
        assert_eq!(n, 1); // "Valid Palindrome" (roadmap, Preview) → Class
        assert_eq!(
            deck.get_mut(&crate::domain::study::normalize_id("Valid Palindrome"))
                .unwrap()
                .stage,
            LoopStage::Class
        );
        // untouched roadmap topic stays Preview; the cue card stays Preview
        assert_eq!(
            deck.get_mut(&crate::domain::study::normalize_id(
                "Two Sum (hashmap complement)"
            ))
            .unwrap()
            .stage,
            LoopStage::Preview
        );
        assert_eq!(
            deck.get_mut(&crate::domain::study::normalize_id("panic somewhere"))
                .unwrap()
                .stage,
            LoopStage::Preview
        );
    }
}
