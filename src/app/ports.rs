//! Ports: the boundary the app depends on. Adapters implement these; the app and
//! domain never name a concrete infrastructure type.

use std::time::SystemTime;

use anyhow::Result;

use crate::domain::{Activity, Lifecycle, Progress, StudyDeck};

/// A candidate "不会的" pulled verbatim from a session — a user question or an
/// error the assistant surfaced. Topic distillation happens later (heuristic or
/// LLM); this is the raw material.
#[derive(Clone, Debug)]
pub struct Snippet {
    pub text: String,
    pub at: SystemTime,
}

/// Everything the tutor extracts from one session file. Produced by a
/// [`SessionSource`]; consumed by the [`Tutor`](super::tutor::Tutor).
#[derive(Clone, Debug)]
pub struct ScannedSession {
    /// Terminal id if this session is bound to a live pane/tab, else `None`.
    pub terminal_id: Option<String>,
    pub session_id: String,
    pub cwd: String,
    /// Auto-inferred proposition: the session title.
    pub title: String,
    pub first_prompt: String,
    pub progress: Progress,
    pub activity: Activity,
    pub lifecycle: Lifecycle,
    pub created: SystemTime,
    pub last_activity: SystemTime,
    /// Candidate unknowns mined heuristically from the transcript.
    pub snippets: Vec<Snippet>,
}

/// Reads omp sessions (live panes + saved transcripts) into [`ScannedSession`]s.
pub trait SessionSource {
    fn collect(&self) -> Result<Vec<ScannedSession>>;
}

/// The "brain": runs a one-shot completion. Backed by `omp -p`. `Send + Sync` so
/// the TUI can drive it on a background thread without freezing the event loop.
pub trait Summarizer: Send + Sync {
    fn run(&self, prompt: &str) -> Result<String>;
}

/// A wall clock (injected so the domain stays testable).
pub trait Clock {
    fn now(&self) -> SystemTime;
}

/// Persists the study deck across runs.
pub trait DeckStore {
    fn load(&self) -> Result<StudyDeck>;
    fn save(&self, deck: &StudyDeck) -> Result<()>;
}
