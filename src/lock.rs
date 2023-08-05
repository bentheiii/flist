use std::fs;
use std::path::{Path, PathBuf};

use crate::config::Lock;

pub struct LockFile {
    path: Option<PathBuf>,
}

impl LockFile {
    pub fn new(root: &Path) -> Self {
        let path = root.join("flist.lock");
        let lock = Lock::without_listener();
        let ret = Self { path: Some(path) };
        ret.write(lock);
        ret
    }

    pub fn set_listener(&self, hostname: String, listener_port: u16) {
        let lock = Lock::with_listener(hostname, listener_port);
        self.write(lock);
    }

    fn write(&self, lock: Lock) {
        let lock = serde_json::to_string(&lock).expect("Failed to serialize lock");
        fs::write(self.path.as_ref().unwrap(), lock).expect("Failed to write lock file");
    }
}

impl Drop for LockFile {
    fn drop(&mut self) {
        if let Some(path) = &self.path {
            fs::remove_file(path).expect("Failed to remove lock file");
        }
    }
}
