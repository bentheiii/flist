use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{link::Link, requests::InsertRequest};

pub const DEFAULT_MAX_ARCHIVE: usize = 100;

fn default_max_archive() -> usize {
    DEFAULT_MAX_ARCHIVE
}

fn is_default_max_archive(max_archive: &usize) -> bool {
    *max_archive == DEFAULT_MAX_ARCHIVE
}

#[derive(Debug, Deserialize, Serialize)]
pub struct FlistConfig {
    #[serde(
        default = "default_max_archive",
        skip_serializing_if = "is_default_max_archive"
    )]
    pub max_archive: usize,
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub preferred_suffixes: Vec<Vec<String>>,
}

impl Default for FlistConfig {
    fn default() -> Self {
        Self {
            max_archive: default_max_archive(),
            preferred_suffixes: Vec::new(),
        }
    }
}

impl FlistConfig {
    pub fn new(max_archive: usize, preferred_suffixes: Vec<Vec<String>>) -> Self {
        Self {
            max_archive,
            preferred_suffixes,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Entry {
    pub name: String,
    pub link: Link,
    pub time_added: DateTime<Utc>,
    pub metadata: Vec<String>,
}

impl From<InsertRequest> for Entry {
    fn from(req: InsertRequest) -> Self {
        Self {
            name: req.name,
            link: req.link,
            time_added: Utc::now(),
            metadata: req.metadata,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Lock {
    WithListener(LockedWithListener),
    WithoutListener(LockedWithoutListener),
}

impl Lock {
    pub fn without_listener() -> Self {
        Self::WithoutListener(LockedWithoutListener {
            time_locked: Utc::now(),
        })
    }

    pub fn with_listener(hostname: String, listener_port: u16) -> Self {
        Self::WithListener(LockedWithListener {
            hostname,
            listener_port,
        })
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LockedWithListener {
    pub hostname: String,
    pub listener_port: u16,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LockedWithoutListener {
    pub time_locked: DateTime<Utc>,
}
