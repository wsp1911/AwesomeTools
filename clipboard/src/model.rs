//! Clipboard history model and its on-disk representation.
//!
//! The store keeps pinned entries first (most recently pinned on top), then the
//! unpinned entries by recency. Only unpinned entries count against the limit.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// How many unpinned entries are kept before the oldest ones are dropped.
pub const DEFAULT_LIMIT: usize = 50;
/// Entries larger than this are ignored; the file would grow without bound.
pub const MAX_TEXT_BYTES: usize = 512 * 1024;

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default()
}

/// A single clipboard record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub id: u64,
    pub text: String,
    #[serde(default)]
    pub pinned: bool,
    /// Unix milliseconds, refreshed when the same text is copied again.
    pub created_at: u64,
    /// Unix milliseconds of the last pin action; 0 while unpinned.
    #[serde(default)]
    pub pinned_at: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ignored {
    Empty,
    TooLarge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recorded {
    /// A brand new entry was appended.
    Added(u64),
    /// The text already existed and was moved to the top.
    Refreshed(u64),
    /// Nothing was recorded.
    Ignored(Ignored),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Store {
    pub entries: Vec<Entry>,
    pub limit: usize,
    #[serde(default)]
    next_id: u64,
}

impl Default for Store {
    fn default() -> Self {
        Self::new(DEFAULT_LIMIT)
    }
}

impl Store {
    pub fn new(limit: usize) -> Self {
        Self {
            entries: Vec::new(),
            limit: limit.max(1),
            next_id: 1,
        }
    }

    pub fn get(&self, id: u64) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    pub fn pinned_count(&self) -> usize {
        self.entries.iter().filter(|entry| entry.pinned).count()
    }

    pub fn unpinned_count(&self) -> usize {
        self.entries.iter().filter(|entry| !entry.pinned).count()
    }

    /// Records `text`, de-duplicating by exact content.
    pub fn record(&mut self, text: &str, now: u64) -> Recorded {
        if text.trim().is_empty() {
            return Recorded::Ignored(Ignored::Empty);
        }
        if text.len() > MAX_TEXT_BYTES {
            return Recorded::Ignored(Ignored::TooLarge);
        }

        if let Some(existing) = self.entries.iter_mut().find(|entry| entry.text == text) {
            existing.created_at = now;
            let id = existing.id;
            self.normalize();
            return Recorded::Refreshed(id);
        }

        let id = self.next_id;
        self.next_id += 1;
        self.entries.push(Entry {
            id,
            text: text.to_owned(),
            pinned: false,
            created_at: now,
            pinned_at: 0,
        });
        self.normalize();
        Recorded::Added(id)
    }

    pub fn toggle_pin(&mut self, id: u64, now: u64) {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) {
            entry.pinned = !entry.pinned;
            entry.pinned_at = if entry.pinned { now } else { 0 };
            self.normalize();
        }
    }

    pub fn remove(&mut self, id: u64) {
        self.entries.retain(|entry| entry.id != id);
    }

    pub fn clear_unpinned(&mut self) {
        self.entries.retain(|entry| entry.pinned);
    }

    /// Sorts pinned-then-recent and drops the oldest unpinned entries over the
    /// limit. Pinned entries never count against the limit and are never evicted.
    fn normalize(&mut self) {
        self.entries.sort_by(|a, b| match (a.pinned, b.pinned) {
            (true, true) => b.pinned_at.cmp(&a.pinned_at),
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            (false, false) => b.created_at.cmp(&a.created_at),
        });

        if self.next_id <= self.entries.iter().map(|entry| entry.id).max().unwrap_or(0) {
            self.next_id = self.entries.iter().map(|entry| entry.id).max().unwrap_or(0) + 1;
        }

        while self.unpinned_count() > self.limit {
            let oldest = self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, entry)| !entry.pinned)
                .min_by_key(|(_, entry)| entry.created_at)
                .map(|(index, _)| index);
            match oldest {
                Some(index) => {
                    self.entries.remove(index);
                }
                None => break,
            }
        }
    }
}

pub fn storage_path() -> PathBuf {
    if let Some(directory) = std::env::var_os("APPDATA") {
        PathBuf::from(directory)
            .join("AwesomeTools")
            .join("clipboard")
            .join("history.json")
    } else {
        std::env::current_dir()
            .unwrap_or_default()
            .join("clipboard-history.json")
    }
}

/// Reads the history, falling back to an empty store when the file is missing
/// or unreadable so a corrupt file never blocks startup.
pub fn load(path: &Path) -> Store {
    let Ok(raw) = fs::read_to_string(path) else {
        return Store::default();
    };
    match serde_json::from_str::<Store>(&raw) {
        Ok(mut store) => {
            store.limit = store.limit.max(1);
            store.normalize();
            store
        }
        Err(_) => Store::default(),
    }
}

/// Writes through a temporary file so an interrupted write cannot truncate the
/// history.
pub fn save(store: &Store, path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let encoded = serde_json::to_vec_pretty(store).map_err(io::Error::other)?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, encoded)?;
    fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_file(tag: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "clipboard-test-{}-{tag}-{unique}.json",
            std::process::id()
        ))
    }

    fn texts(store: &Store) -> Vec<&str> {
        store
            .entries
            .iter()
            .map(|entry| entry.text.as_str())
            .collect()
    }

    #[test]
    fn records_new_entries_newest_first() {
        let mut store = Store::new(5);
        store.record("alpha", 1_000);
        store.record("beta", 2_000);
        assert_eq!(texts(&store), vec!["beta", "alpha"]);
        assert_eq!(store.entries.len(), 2);
    }

    #[test]
    fn ignores_blank_and_oversized_text() {
        let mut store = Store::new(5);
        assert_eq!(store.record("   \n ", 1), Recorded::Ignored(Ignored::Empty));
        let huge = "x".repeat(MAX_TEXT_BYTES + 1);
        assert_eq!(store.record(&huge, 2), Recorded::Ignored(Ignored::TooLarge));
        assert!(store.entries.is_empty());
    }

    #[test]
    fn refreshes_duplicates_instead_of_appending() {
        let mut store = Store::new(5);
        store.record("alpha", 1_000);
        store.record("beta", 2_000);
        let id = store.entries[1].id;
        assert_eq!(store.record("alpha", 3_000), Recorded::Refreshed(id));
        assert_eq!(texts(&store), vec!["alpha", "beta"]);
        assert_eq!(store.entries[0].created_at, 3_000);
    }

    #[test]
    fn evicts_only_the_oldest_unpinned_entries() {
        let mut store = Store::new(2);
        store.record("old", 1_000);
        let kept = store.entries[0].id;
        store.toggle_pin(kept, 1_100);
        store.record("second", 2_000);
        store.record("third", 3_000);
        store.record("fourth", 4_000);

        assert_eq!(texts(&store), vec!["old", "fourth", "third"]);
        assert!(store.get(kept).is_some_and(|entry| entry.pinned));
    }

    #[test]
    fn pinned_entries_group_by_pin_time() {
        let mut store = Store::new(10);
        store.record("one", 1_000);
        store.record("two", 2_000);
        let first = store.entries[1].id;
        let second = store.entries[0].id;
        store.toggle_pin(first, 5_000);
        store.toggle_pin(second, 6_000);
        assert_eq!(texts(&store), vec!["two", "one"]);

        store.toggle_pin(second, 7_000);
        assert_eq!(texts(&store), vec!["one", "two"]);
        assert_eq!(store.pinned_count(), 1);
        assert_eq!(store.unpinned_count(), 1);
    }

    #[test]
    fn removes_and_clears_only_unpinned_entries() {
        let mut store = Store::new(10);
        store.record("keep", 1_000);
        store.record("drop", 2_000);
        store.record("pin", 3_000);
        let pinned = store.entries[0].id;
        store.toggle_pin(pinned, 4_000);

        let dropped = store.entries[1].id;
        store.remove(dropped);
        assert_eq!(texts(&store), vec!["pin", "keep"]);

        store.clear_unpinned();
        assert_eq!(texts(&store), vec!["pin"]);
    }

    #[test]
    fn save_and_load_roundtrip() {
        let path = temp_file("roundtrip");
        let mut store = Store::new(3);
        store.record("alpha", 1_000);
        store.record("beta", 2_000);
        let pinned = store.entries[0].id;
        store.toggle_pin(pinned, 3_000);

        save(&store, &path).expect("save");
        let loaded = load(&path);
        assert_eq!(texts(&loaded), texts(&store));
        assert_eq!(loaded.limit, store.limit);
        assert!(loaded.entries[0].pinned);

        // Recording after a reload keeps fresh ids.
        let mut loaded = loaded;
        loaded.record("gamma", 4_000);
        assert_eq!(loaded.entries.len(), 3);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn missing_or_corrupt_file_falls_back_to_default() {
        let path = temp_file("missing");
        assert!(load(&path).entries.is_empty());

        fs::write(&path, b"{ not json").expect("write");
        let loaded = load(&path);
        assert!(loaded.entries.is_empty());
        assert_eq!(loaded.limit, DEFAULT_LIMIT);
        let _ = fs::remove_file(&path);
    }
}
