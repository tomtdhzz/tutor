//! Per-window work: an omp terminal pane/tab, its inferred proposition, and the
//! state it sits in on the board. All classification here is rule-based and
//! LLM-free so the board renders instantly.

use std::time::{Duration, SystemTime};

/// How much of a session's declared plan is complete. `total == 0` means the
/// session never declared a todo list, so progress is unknown (not zero).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    pub done: usize,
    pub in_progress: usize,
    pub total: usize,
}

impl Progress {
    pub fn is_known(&self) -> bool {
        self.total > 0
    }
    /// Completion in percent, or `None` when no todo list exists.
    pub fn pct(&self) -> Option<u8> {
        if self.total == 0 {
            return None;
        }
        Some(((self.done * 100 + self.total / 2) / self.total).min(100) as u8)
    }
    pub fn all_done(&self) -> bool {
        self.total > 0 && self.done >= self.total
    }
}

/// Raw activity volume, a coarse "how much happened here" signal used when a
/// session declared no todos.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Activity {
    pub messages: usize,
    pub tool_starts: usize,
}

/// How a session's last turn ended, read from its tail.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lifecycle {
    /// Still live, or ended mid-turn / interrupted.
    Active,
    /// Exited normally.
    Complete,
    /// Interrupted / aborted / errored tail.
    Interrupted,
    #[default]
    Unknown,
}

/// The three board columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkState {
    Todo,
    Doing,
    Done,
}

/// One card on the board: a window/tab and everything the tutor knows about it.
#[derive(Clone, Debug)]
pub struct WorkItem {
    /// Terminal id the pane is bound to (e.g. `ttys004`), if it is a live pane.
    pub terminal_id: Option<String>,
    pub session_id: String,
    pub cwd: String,
    /// Auto-inferred proposition (the session title), falling back to the first
    /// user prompt.
    pub proposition: String,
    pub first_prompt: String,
    pub progress: Progress,
    pub activity: Activity,
    pub lifecycle: Lifecycle,
    pub created: SystemTime,
    pub last_activity: SystemTime,
}

impl WorkItem {
    /// Short project label: the last path segment of `cwd`.
    pub fn project(&self) -> &str {
        self.cwd
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or(&self.cwd)
    }

    /// A 0–100 engagement proxy from message volume, used as the gauge fill when
    /// a session declared no todos. ~40 messages reads as "full".
    pub fn activity_level(&self) -> u8 {
        let m = self.activity.messages.min(40) as u32;
        ((m * 100 + 20) / 40).min(100) as u8
    }

    /// Gauge fill: real todo completion when known, else the activity proxy.
    pub fn gauge_pct(&self) -> u8 {
        self.progress.pct().unwrap_or_else(|| self.activity_level())
    }

    /// Rule-based column classification.
    ///
    /// - `Done`   — todos all complete, or a normal exit that has been idle a while.
    /// - `Doing`  — touched recently, or a partially-complete todo list, or a live pane.
    /// - `Todo`   — everything else (barely started, or all-pending).
    pub fn state(&self, now: SystemTime, active_window: Duration, stale: Duration) -> WorkState {
        if self.progress.all_done() {
            return WorkState::Done;
        }
        let idle = now
            .duration_since(self.last_activity)
            .unwrap_or(Duration::ZERO);
        let recent = idle <= active_window;

        if self.progress.is_known() {
            // A declared, unfinished plan is in progress unless long abandoned.
            return if self.progress.done > 0 || recent {
                WorkState::Doing
            } else {
                WorkState::Todo
            };
        }

        // No declared plan: fall back to recency + lifecycle + volume.
        if recent || self.terminal_id.is_some() {
            WorkState::Doing
        } else if self.lifecycle == Lifecycle::Complete && idle >= stale {
            WorkState::Done
        } else if self.activity.messages <= 2 {
            WorkState::Todo
        } else if idle >= stale {
            WorkState::Done
        } else {
            WorkState::Doing
        }
    }
}

/// One rendered column and its items, sorted most-recent first.
#[derive(Clone, Debug)]
pub struct Column {
    pub state: WorkState,
    pub items: Vec<WorkItem>,
}

/// The full board: three columns built from a set of work items.
#[derive(Clone, Debug)]
pub struct Board {
    pub todo: Column,
    pub doing: Column,
    pub done: Column,
}

impl Board {
    /// Default classification windows: active within 6h, stale after 3d.
    pub const ACTIVE_WINDOW: Duration = Duration::from_secs(6 * 3600);
    pub const STALE: Duration = Duration::from_secs(3 * 86_400);

    pub fn build(mut items: Vec<WorkItem>, now: SystemTime) -> Board {
        // Newest activity first, everywhere.
        items.sort_by_key(|it| std::cmp::Reverse(it.last_activity));
        let (mut todo, mut doing, mut done) = (Vec::new(), Vec::new(), Vec::new());
        for it in items {
            match it.state(now, Self::ACTIVE_WINDOW, Self::STALE) {
                WorkState::Todo => todo.push(it),
                WorkState::Doing => doing.push(it),
                WorkState::Done => done.push(it),
            }
        }
        Board {
            todo: Column {
                state: WorkState::Todo,
                items: todo,
            },
            doing: Column {
                state: WorkState::Doing,
                items: doing,
            },
            done: Column {
                state: WorkState::Done,
                items: done,
            },
        }
    }

    pub fn columns(&self) -> [&Column; 3] {
        [&self.todo, &self.doing, &self.done]
    }

    pub fn total(&self) -> usize {
        self.todo.items.len() + self.doing.items.len() + self.done.items.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(
        progress: Progress,
        msgs: usize,
        life: Lifecycle,
        idle: Duration,
        live: bool,
    ) -> WorkItem {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        WorkItem {
            terminal_id: live.then(|| "ttys000".to_string()),
            session_id: "s".into(),
            cwd: "/a/b/proj".into(),
            proposition: "p".into(),
            first_prompt: "fp".into(),
            progress,
            activity: Activity {
                messages: msgs,
                tool_starts: msgs,
            },
            lifecycle: life,
            created: now - idle,
            last_activity: now - idle,
        }
    }
    fn now() -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000)
    }
    const AW: Duration = Board::ACTIVE_WINDOW;
    const ST: Duration = Board::STALE;

    #[test]
    fn all_done_todos_are_done() {
        let it = item(
            Progress {
                done: 3,
                in_progress: 0,
                total: 3,
            },
            20,
            Lifecycle::Complete,
            Duration::from_secs(10),
            false,
        );
        assert_eq!(it.state(now(), AW, ST), WorkState::Done);
    }

    #[test]
    fn partial_todos_recent_is_doing() {
        let it = item(
            Progress {
                done: 1,
                in_progress: 1,
                total: 4,
            },
            10,
            Lifecycle::Active,
            Duration::from_secs(60),
            false,
        );
        assert_eq!(it.state(now(), AW, ST), WorkState::Doing);
    }

    #[test]
    fn untouched_all_pending_is_todo() {
        let it = item(
            Progress {
                done: 0,
                in_progress: 0,
                total: 4,
            },
            1,
            Lifecycle::Active,
            Duration::from_secs(5 * 86_400),
            false,
        );
        assert_eq!(it.state(now(), AW, ST), WorkState::Todo);
    }

    #[test]
    fn live_pane_without_todos_is_doing() {
        let it = item(
            Progress::default(),
            1,
            Lifecycle::Active,
            Duration::from_secs(10 * 86_400),
            true,
        );
        assert_eq!(it.state(now(), AW, ST), WorkState::Doing);
    }

    #[test]
    fn stale_complete_without_todos_is_done() {
        let it = item(
            Progress::default(),
            40,
            Lifecycle::Complete,
            Duration::from_secs(5 * 86_400),
            false,
        );
        assert_eq!(it.state(now(), AW, ST), WorkState::Done);
    }

    #[test]
    fn barely_started_no_todos_is_todo() {
        let it = item(
            Progress::default(),
            1,
            Lifecycle::Active,
            Duration::from_secs(4 * 86_400),
            false,
        );
        assert_eq!(it.state(now(), AW, ST), WorkState::Todo);
    }

    #[test]
    fn progress_pct_rounds() {
        assert_eq!(
            Progress {
                done: 1,
                in_progress: 0,
                total: 3
            }
            .pct(),
            Some(33)
        );
        assert_eq!(
            Progress {
                done: 2,
                in_progress: 0,
                total: 3
            }
            .pct(),
            Some(67)
        );
        assert_eq!(Progress::default().pct(), None);
    }
}
