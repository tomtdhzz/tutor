//! tutor — a local omp tutor (私人教师).
//!
//! Three jobs, one read-only dashboard:
//! 1. Work board (看板): every omp terminal window/tab becomes a card with an
//!    auto-inferred proposition (session title), progress, recency, and state.
//! 2. Study loop (学习): auto-mine "the things you don't know" from recent
//!    sessions and cycle them through 预习→听课→作业→复习→改错, with spaced review.
//! 3. Daily briefing (快报): a rolled-up account of the day, optionally narrated
//!    by `omp -p`.
//!
//! Layered as a lightweight hexagon, mirroring its sibling `headroom`:
//! - [`domain`]: pure ubiquitous language (panes, work state, study loop, briefing).
//! - [`app`]: the `Tutor` use case and the ports it depends on.
//! - [`adapters`]: infrastructure (omp sessions on disk, `omp -p`, cache, clock).
//! - [`delivery`]: user-facing rendering (cli one-shot + interactive tui), i18n.

pub mod adapters;
pub mod app;
pub mod delivery;
pub mod domain;
