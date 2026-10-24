//! Transcript history stored as JSON lines in the app data dir.

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HistoryKind {
    Dictation,
    Meeting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SegmentSource {
    Mic,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingSegment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    pub source: SegmentSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub kind: HistoryKind,
    pub created_at: String,
    pub text: String,
    pub duration_ms: u64,
    pub model_id: Option<String>,
    pub title: Option<String>,
    pub app: Option<String>,
    pub segments: Option<Vec<MeetingSegment>>,
    pub saved_path: Option<String>,
}

impl HistoryEntry {
    pub fn dictation(text: String, duration_ms: u64, model_id: Option<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            kind: HistoryKind::Dictation,
            created_at: chrono::Local::now().to_rfc3339(),
            text,
            duration_ms,
            model_id,
            title: None,
            app: None,
            segments: None,
            saved_path: None,
        }
    }
}

const MAX_ENTRIES: usize = 5000;

pub struct HistoryStore {
    path: PathBuf,
    entries: Mutex<Vec<HistoryEntry>>,
}

impl HistoryStore {
    pub fn load(path: PathBuf) -> Self {
        let mut entries: Vec<HistoryEntry> = std::fs::read_to_string(&path)
            .map(|s| s.lines().filter_map(|l| serde_json::from_str(l).ok()).collect())
            .unwrap_or_default();
        if entries.len() > MAX_ENTRIES {
            entries.drain(0..entries.len() - MAX_ENTRIES);
        }
        Self { path, entries: Mutex::new(entries) }
    }

    pub fn append(&self, entry: HistoryEntry) {
        {
            let mut entries = self.entries.lock();
            entries.push(entry.clone());
            if entries.len() > MAX_ENTRIES {
                let overflow = entries.len() - MAX_ENTRIES;
                entries.drain(0..overflow);
            }
        }
        if let Err(e) = self.append_line(&entry) {
            log::error!("history append failed: {e:#}");
        }
    }

    /// Replace an existing entry (used when a meeting finishes) or append it.
    pub fn upsert(&self, entry: HistoryEntry) {
        let replaced = {
            let mut entries = self.entries.lock();
            if let Some(slot) = entries.iter_mut().find(|e| e.id == entry.id) {
                *slot = entry.clone();
                true
            } else {
                entries.push(entry.clone());
                false
            }
        };
        let res = if replaced { self.rewrite() } else { self.append_line(&entry) };
        if let Err(e) = res {
            log::error!("history upsert failed: {e:#}");
        }
    }

    pub fn query(&self, query: &str, limit: usize, offset: usize) -> Vec<HistoryEntry> {
        let q = query.trim().to_lowercase();
        let entries = self.entries.lock();
        entries
            .iter()
            .rev()
            .filter(|e| {
                q.is_empty()
                    || e.text.to_lowercase().contains(&q)
                    || e.title.as_deref().map(|t| t.to_lowercase().contains(&q)).unwrap_or(false)
                    || e.app.as_deref().map(|t| t.to_lowercase().contains(&q)).unwrap_or(false)
            })
            .skip(offset)
            .take(limit)
            .cloned()
            .collect()
    }

    pub fn delete(&self, id: &str) {
        self.entries.lock().retain(|e| e.id != id);
        if let Err(e) = self.rewrite() {
            log::error!("history rewrite failed: {e:#}");
        }
    }

    pub fn clear(&self) {
        self.entries.lock().clear();
        if let Err(e) = self.rewrite() {
            log::error!("history rewrite failed: {e:#}");
        }
    }

    fn append_line(&self, entry: &HistoryEntry) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&self.path)?;
        writeln!(f, "{}", serde_json::to_string(entry)?)?;
        Ok(())
    }

    fn rewrite(&self) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let entries = self.entries.lock();
        let mut out = String::new();
        for e in entries.iter() {
            out.push_str(&serde_json::to_string(e)?);
            out.push('\n');
        }
        let tmp = self.path.with_extension("jsonl.tmp");
        std::fs::write(&tmp, out)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}
