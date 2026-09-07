//! The "brain": a `Summarizer` backed by `omp -p` (headless print mode).
//!
//! Read-only with respect to omp config — it only runs one-shot prompts against
//! the user's already-authenticated accounts, exactly as `headroom` shells out to
//! `omp usage`. `omp -p` prints progress noise to stderr and the answer to
//! stdout, so we take stdout only.

use std::process::Command;

use anyhow::{anyhow, Context, Result};

use crate::app::Summarizer;

pub struct OmpSummarizer {
    /// Optional model override (e.g. "smol" for cheap/fast mining).
    model: Option<String>,
}

impl OmpSummarizer {
    pub fn new() -> Self {
        OmpSummarizer { model: None }
    }

    pub fn with_model(model: impl Into<String>) -> Self {
        OmpSummarizer {
            model: Some(model.into()),
        }
    }
}

impl Default for OmpSummarizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Summarizer for OmpSummarizer {
    fn run(&self, prompt: &str) -> Result<String> {
        let mut cmd = Command::new("omp");
        // `--no-session` keeps the tutor's own brain calls ephemeral, so they are
        // never persisted and re-ingested as work/study items on the next scan.
        cmd.arg("-p").arg("--no-title").arg("--no-session");
        if let Some(m) = &self.model {
            cmd.arg("--model").arg(m);
        }
        cmd.arg("--").arg(prompt);

        let out = cmd
            .output()
            .context("failed to run `omp` — is it installed and on PATH?")?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            return Err(anyhow!("omp -p exited with {}: {}", out.status, err.trim()));
        }
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if text.is_empty() {
            return Err(anyhow!("omp -p returned empty output"));
        }
        Ok(text)
    }
}
