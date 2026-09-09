//! Domain layer: the ubiquitous language of a work/study tutor.
//!
//! Pure, IO-free, and independently testable. Nothing here knows about `omp`,
//! JSON, files, or terminals — those live in `adapters`/`delivery`.

pub mod briefing;
pub mod course;
pub mod pane;
pub mod study;

pub use briefing::DailyBriefing;
pub use course::{CourseProgress, Syllabus, Topic};
pub use pane::{Activity, Board, Column, Lifecycle, Progress, WorkItem, WorkState};
pub use study::{days_between, LoopStage, ReviewEntry, ReviewPlan, StudyDeck, Unknown};

use std::time::Duration;

/// Compact, human-readable duration such as `1d3h`, `47m`, or `12s`.
pub fn human_duration(d: Duration) -> String {
    let secs = d.as_secs();
    if secs >= 86_400 {
        let (days, hours) = (secs / 86_400, (secs % 86_400) / 3_600);
        if hours > 0 {
            format!("{days}d{hours}h")
        } else {
            format!("{days}d")
        }
    } else if secs >= 3_600 {
        let (hours, mins) = (secs / 3_600, (secs % 3_600) / 60);
        if mins > 0 {
            format!("{hours}h{mins}m")
        } else {
            format!("{hours}h")
        }
    } else if secs >= 60 {
        format!("{}m", secs / 60)
    } else {
        format!("{secs}s")
    }
}

/// "N ago" phrasing for a past instant relative to `now`; `now` for future/zero.
pub fn human_ago(then: std::time::SystemTime, now: std::time::SystemTime) -> String {
    match now.duration_since(then) {
        Ok(d) if d.as_secs() > 0 => human_duration(d),
        _ => "now".to_string(),
    }
}
