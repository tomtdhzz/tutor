//! A course lives inside its own directory: `<dir>/roadmap.md` (the editable
//! syllabus) and `<dir>/.tutor/deck.json` (progress). This adapter owns that
//! on-disk layout; the deck itself reuses [`FileDeckStore`](super::FileDeckStore).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::cache_file::FileDeckStore;

/// Per-course preferences persisted at `<dir>/.tutor/config.json`.
#[derive(Default, Clone, Serialize, Deserialize)]
pub struct CourseConfig {
    /// Preferred programming language for lesson code (e.g. "rust"); `None` lets
    /// the model choose.
    #[serde(default)]
    pub code_language: Option<String>,
}

pub struct CourseDir {
    dir: PathBuf,
}

impl CourseDir {
    pub fn new(dir: impl Into<PathBuf>) -> CourseDir {
        CourseDir { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn roadmap_path(&self) -> PathBuf {
        self.dir.join("roadmap.md")
    }

    pub fn deck_path(&self) -> PathBuf {
        self.dir.join(".tutor").join("deck.json")
    }

    /// The Markdown file caching the lesson for a topic id (`normalize_id`).
    pub fn lesson_path(&self, id: &str) -> PathBuf {
        self.dir
            .join(".tutor")
            .join("lessons")
            .join(format!("{}.md", sanitize_id(id)))
    }

    /// True when a lesson has already been drafted and cached for this topic id.
    pub fn has_lesson(&self, id: &str) -> bool {
        self.lesson_path(id).is_file()
    }

    pub fn read_lesson(&self, id: &str) -> Result<Option<String>> {
        match fs::read_to_string(self.lesson_path(id)) {
            Ok(s) => Ok(Some(s)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).context("read lesson.md"),
        }
    }

    pub fn write_lesson(&self, id: &str, md: &str) -> Result<()> {
        let path = self.lesson_path(id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create lessons dir {}", parent.display()))?;
        }
        let body = if md.ends_with('\n') {
            md.to_string()
        } else {
            format!("{md}\n")
        };
        fs::write(&path, body).context("write lesson.md")?;
        Ok(())
    }

    /// True if this directory already holds a course (has a roadmap).
    pub fn exists(&self) -> bool {
        self.roadmap_path().is_file()
    }

    /// Create `<dir>` and `<dir>/.tutor` if needed.
    pub fn ensure(&self) -> Result<()> {
        fs::create_dir_all(self.dir.join(".tutor"))
            .with_context(|| format!("create course dir {}", self.dir.display()))?;
        Ok(())
    }

    pub fn read_roadmap(&self) -> Result<Option<String>> {
        match fs::read_to_string(self.roadmap_path()) {
            Ok(s) => Ok(Some(s)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).context("read roadmap.md"),
        }
    }

    pub fn write_roadmap(&self, md: &str) -> Result<()> {
        self.ensure()?;
        let body = if md.ends_with('\n') {
            md.to_string()
        } else {
            format!("{md}\n")
        };
        fs::write(self.roadmap_path(), body).context("write roadmap.md")?;
        Ok(())
    }

    /// The deck store for this course's progress file.
    pub fn deck_store(&self) -> Result<FileDeckStore> {
        self.ensure()?;
        Ok(FileDeckStore::at(self.deck_path()))
    }

    pub fn config_path(&self) -> PathBuf {
        self.dir.join(".tutor").join("config.json")
    }

    /// Read the course config, defaulting to empty when absent or unreadable.
    pub fn read_config(&self) -> CourseConfig {
        fs::read_to_string(self.config_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn write_config(&self, cfg: &CourseConfig) -> Result<()> {
        self.ensure()?;
        let body = serde_json::to_string_pretty(cfg).context("serialize course config")?;
        fs::write(self.config_path(), body).context("write config.json")?;
        Ok(())
    }
}

/// Make a topic id safe as a single filename: keep alphanumerics (incl. CJK),
/// collapse every other run into a single `-`, and cap the length.
fn sanitize_id(id: &str) -> String {
    let mut out = String::with_capacity(id.len());
    let mut prev_dash = false;
    for c in id.chars() {
        if c.is_alphanumeric() {
            out.push(c);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    let capped: String = trimmed.chars().take(80).collect();
    if capped.is_empty() {
        "lesson".to_string()
    } else {
        capped
    }
}
