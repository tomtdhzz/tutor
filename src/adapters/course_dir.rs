//! A course lives inside its own directory: `<dir>/roadmap.md` (the editable
//! syllabus) and `<dir>/.tutor/deck.json` (progress). This adapter owns that
//! on-disk layout; the deck itself reuses [`FileDeckStore`](super::FileDeckStore).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::cache_file::FileDeckStore;

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
}
