//! File-backed `DeckStore`: persists the study deck as JSON under the XDG cache.
//!
//! The domain deliberately holds `SystemTime` and no serde derives; this adapter
//! owns the on-disk shape and maps instants to/from epoch-millis.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::app::DeckStore;
use crate::domain::study::LoopStage;
use crate::domain::{StudyDeck, Unknown};

pub struct FileDeckStore {
    path: PathBuf,
}

impl FileDeckStore {
    /// `$XDG_CACHE_HOME/tutor/deck.json`, falling back to `~/.cache/...`.
    pub fn new() -> Result<Self> {
        let base = if let Ok(x) = std::env::var("XDG_CACHE_HOME") {
            PathBuf::from(x)
        } else {
            let home = std::env::var("HOME").context("HOME not set")?;
            PathBuf::from(home).join(".cache")
        };
        let dir = base.join("tutor");
        fs::create_dir_all(&dir).with_context(|| format!("create cache dir {}", dir.display()))?;
        Ok(FileDeckStore {
            path: dir.join("deck.json"),
        })
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Construct at an explicit `deck.json` path (used in tests).
    pub fn at(path: PathBuf) -> Self {
        FileDeckStore { path }
    }
}

impl DeckStore for FileDeckStore {
    fn load(&self) -> Result<StudyDeck> {
        let bytes = match fs::read(&self.path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(StudyDeck::default()),
            Err(e) => return Err(e).context("read deck cache")?,
        };
        let dto: DeckDto = serde_json::from_slice(&bytes).context("parse deck cache")?;
        Ok(StudyDeck::new(
            dto.cards.into_iter().map(Into::into).collect(),
        ))
    }

    fn save(&self, deck: &StudyDeck) -> Result<()> {
        let dto = DeckDto {
            version: 1,
            cards: deck.cards.iter().map(CardDto::from).collect(),
        };
        let json = serde_json::to_vec_pretty(&dto).context("serialize deck")?;
        // Write via temp + rename for atomicity.
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, &json).with_context(|| format!("write {}", tmp.display()))?;
        fs::rename(&tmp, &self.path).context("commit deck cache")?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct DeckDto {
    version: u32,
    cards: Vec<CardDto>,
}

#[derive(Serialize, Deserialize)]
struct CardDto {
    topic: String,
    detail: String,
    project: String,
    stage: usize,
    times_seen: u32,
    reviews: u32,
    first_seen_ms: u64,
    last_seen_ms: u64,
    next_review_ms: u64,
}

fn to_ms(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as u64
}
fn from_ms(ms: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(ms)
}

impl From<&Unknown> for CardDto {
    fn from(u: &Unknown) -> Self {
        CardDto {
            topic: u.topic.clone(),
            detail: u.detail.clone(),
            project: u.project.clone(),
            stage: u.stage.index(),
            times_seen: u.times_seen,
            reviews: u.reviews,
            first_seen_ms: to_ms(u.first_seen),
            last_seen_ms: to_ms(u.last_seen),
            next_review_ms: to_ms(u.next_review),
        }
    }
}

impl From<CardDto> for Unknown {
    fn from(c: CardDto) -> Self {
        Unknown {
            id: crate::domain::study::normalize_id(&c.topic),
            topic: c.topic,
            detail: c.detail,
            project: c.project,
            stage: LoopStage::from_index(c.stage),
            times_seen: c.times_seen,
            reviews: c.reviews,
            first_seen: from_ms(c.first_seen_ms),
            last_seen: from_ms(c.last_seen_ms),
            next_review: from_ms(c.next_review_ms),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn roundtrip_persists_cards() {
        let dir = std::env::temp_dir().join(format!("tutor-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let store = FileDeckStore::at(dir.join("deck.json"));

        // Missing file loads as empty.
        assert!(store.load().unwrap().is_empty());

        let at = UNIX_EPOCH + Duration::from_secs(1_000);
        let mut deck = StudyDeck::default();
        deck.upsert(Unknown::seed(
            "Rust lifetimes",
            "elision",
            "tutor",
            LoopStage::Review,
            at,
        ));
        store.save(&deck).unwrap();

        let loaded = store.load().unwrap();
        assert_eq!(loaded.cards.len(), 1);
        let c = &loaded.cards[0];
        assert_eq!(c.topic, "Rust lifetimes");
        assert_eq!(c.stage, LoopStage::Review);
        assert_eq!(c.last_seen, at);
        let _ = fs::remove_dir_all(&dir);
    }
}
