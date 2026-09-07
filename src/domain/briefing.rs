//! The daily briefing (每日快报): a rule-based rollup of the day, optionally
//! narrated by an LLM. The structured facts are computed here; the prose is
//! attached later by the app/delivery layer.

use std::time::{Duration, SystemTime};

use super::pane::{Board, WorkItem};
use super::study::StudyDeck;

/// A single line item in the briefing.
#[derive(Clone, Debug)]
pub struct BriefLine {
    pub proposition: String,
    pub project: String,
    pub detail: String,
}

/// The structured daily briefing.
#[derive(Clone, Debug, Default)]
pub struct DailyBriefing {
    /// Windows touched within the reporting window.
    pub active: Vec<BriefLine>,
    /// Work that finished within the reporting window.
    pub finished: Vec<BriefLine>,
    /// Windows started but idle / waiting.
    pub waiting: Vec<BriefLine>,
    /// Knowledge points due for attention today.
    pub to_study: Vec<BriefLine>,
    /// Optional LLM-written narration.
    pub prose: Option<String>,
}

impl DailyBriefing {
    /// Compose the structured facts from a board and study deck. `window` is the
    /// look-back horizon for "today" (e.g. 24h).
    pub fn compose(
        board: &Board,
        deck: &StudyDeck,
        now: SystemTime,
        window: Duration,
    ) -> DailyBriefing {
        let touched = |it: &WorkItem| {
            now.duration_since(it.last_activity)
                .map(|d| d <= window)
                .unwrap_or(false)
        };
        let line = |it: &WorkItem| BriefLine {
            proposition: it.proposition.clone(),
            project: it.project().to_string(),
            detail: match it.progress.pct() {
                Some(p) => format!("{}% ({}/{})", p, it.progress.done, it.progress.total),
                None => format!("{} msgs", it.activity.messages),
            },
        };

        let active = board
            .doing
            .items
            .iter()
            .filter(|it| touched(it))
            .map(line)
            .collect();
        let finished = board
            .done
            .items
            .iter()
            .filter(|it| touched(it))
            .map(line)
            .collect();
        let waiting = board.todo.items.iter().map(line).collect();

        let to_study = deck
            .due(now)
            .into_iter()
            .take(8)
            .map(|u| BriefLine {
                proposition: u.topic.clone(),
                project: u.project.clone(),
                detail: u.detail.clone(),
            })
            .collect();

        DailyBriefing {
            active,
            finished,
            waiting,
            to_study,
            prose: None,
        }
    }

    /// Render the structured facts into a compact prompt for an LLM narrator.
    pub fn to_prompt(&self, date: &str) -> String {
        let mut s = String::new();
        s.push_str("You are my personal work/study tutor. Write a short, warm daily briefing in the SAME language as the material below (Chinese if it is Chinese). 4-8 sentences, concrete, no preamble, no markdown headers. Cover: what moved today, what got finished, what is waiting on me, and what to review. Material:\n\n");
        s.push_str(&format!("Date: {date}\n"));
        let sect = |s: &mut String, title: &str, lines: &[BriefLine]| {
            s.push_str(&format!("\n[{title}]\n"));
            if lines.is_empty() {
                s.push_str("(none)\n");
            }
            for l in lines {
                s.push_str(&format!(
                    "- {} · {} · {}\n",
                    l.proposition, l.project, l.detail
                ));
            }
        };
        sect(&mut s, "In progress today", &self.active);
        sect(&mut s, "Finished today", &self.finished);
        sect(&mut s, "Waiting / not started", &self.waiting);
        sect(&mut s, "To review (things you don't know)", &self.to_study);
        s
    }

    pub fn is_empty(&self) -> bool {
        self.active.is_empty()
            && self.finished.is_empty()
            && self.waiting.is_empty()
            && self.to_study.is_empty()
    }
}
