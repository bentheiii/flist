use std::fs;
use std::path::{Path, PathBuf};

use crate::config::Entry;
use crate::config::FlistConfig;

#[derive(Debug)]
pub struct Project {
    pub root: PathBuf,
    pub config: FlistConfig,
    pub entries: Vec<Entry>,
    pub archive: Vec<Entry>,
}

impl Project {
    pub fn new(
        root: PathBuf,
        config: FlistConfig,
        entries: Vec<Entry>,
        archive: Vec<Entry>,
    ) -> Self {
        Self {
            root,
            config,
            entries,
            archive,
        }
    }

    pub fn from_dir(root: &Path, config: FlistConfig) -> Self {
        let entries_path = root.join("entries.json");
        let archive_path = root.join("archive.json");
        let entries = if entries_path.exists() {
            serde_json::from_str(
                &std::fs::read_to_string(&entries_path).expect("Failed to read entries file"),
            )
            .expect("Failed to parse entries file")
        } else {
            vec![]
        };
        let archive = if archive_path.exists() {
            serde_json::from_str(
                &std::fs::read_to_string(&archive_path).expect("Failed to read archive file"),
            )
            .expect("Failed to parse archive file")
        } else {
            vec![]
        };
        Self::new(root.to_path_buf(), config, entries, archive)
    }

    pub fn insert_entry(&mut self, entry: Entry) {
        self.entries.insert(0, entry)
    }

    pub fn archive_entry(&mut self, entry_idx: usize) {
        let entry = self.entries.remove(entry_idx);
        self.archive.insert(0, entry);
        if self.archive.len() > self.config.max_archive {
            self.archive.pop();
        }
    }

    pub fn remove_from_archive(&mut self, entry_idx: usize) {
        self.archive.remove(entry_idx);
    }

    pub fn restore_from_archive(&mut self, entry_idx: usize) {
        let entry = self.archive.remove(entry_idx);
        self.entries.insert(0, entry);
    }

    pub fn move_entry(&mut self, from: usize, to: usize) {
        if from == to {
            return;
        }
        let entry = self.entries.remove(from);
        self.entries.insert(to, entry);
    }

    pub fn save(&self) {
        let entries_path = self.root.join("entries.json");
        let archive_path = self.root.join("archive.json");
        let entries = serde_json::to_string(&self.entries).expect("Failed to serialize entries");
        let archive = serde_json::to_string(&self.archive).expect("Failed to serialize archive");
        fs::write(entries_path, entries).expect("Failed to write entries file");
        fs::write(archive_path, archive).expect("Failed to write archive file");
    }
}
