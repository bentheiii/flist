use serde::{Deserialize, Serialize};
use std::io;
use std::{path::Path, time::Duration};

use itertools::Itertools;
use std::process::Command;

#[derive(Debug, Clone)]
pub enum Link {
    File(String),
    Directory(String),
    Url(String),
}

impl From<&str> for Link {
    fn from(s: &str) -> Self {
        let pth = Path::new(s);
        if pth.is_absolute() {
            if pth.is_dir() {
                Self::Directory(s.to_string())
            } else {
                Self::File(s.to_string())
            }
        } else {
            Self::Url(s.to_string())
        }
    }
}

impl Link {
    pub fn infer_name(&self) -> String {
        match self {
            Self::File(s) => Path::new(s)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string(),
            Self::Directory(s) => Path::new(s)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string(),
            Self::Url(s) => {
                let Ok(Some(title)) = get_url_title(s) else { return s.to_string() };
                title
            }
        }
    }

    pub fn explore(&self) {
        match self {
            Self::File(s) => Provider::new().explore_at_file(s),
            Self::Directory(s) => Provider::new().open_dir(s),
            Self::Url(s) => Provider::new().open_url(s),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::File(s) => s.as_str(),
            Self::Directory(s) => s.as_str(),
            Self::Url(s) => s.as_str(),
        }
    }

    pub fn preferred_file<'a>(
        &self,
        preffered_suffixes: impl IntoIterator<Item = &'a Vec<String>>,
    ) -> io::Result<Option<PreferredFile>> {
        match self {
            Self::Directory(dir) => {
                let suffixes = Path::new(dir)
                    .read_dir()?
                    .map(|f| f.map(|f| f.path()))
                    .collect::<io::Result<Vec<_>>>()?
                    .into_iter()
                    .filter_map(|pth| {
                        pth.extension()
                            .and_then(|e| e.to_owned().into_string().ok())
                            .map(|e| (e, pth))
                    })
                    .into_group_map();
                for layer in preffered_suffixes {
                    match layer
                        .iter()
                        .filter_map(|suffix| {
                            suffixes
                                .get(suffix)
                                .map(|v| v.iter().map(move |p| (suffix, p)))
                        })
                        .flatten()
                        .exactly_one()
                    {
                        Ok((suf, pth)) => {
                            return Ok(Some(PreferredFile::new(
                                Link::from(pth.to_str().unwrap()),
                                Some(suf.clone()),
                            )))
                        }
                        Err(mut remained) => {
                            if remained.next().is_some() {
                                break;
                            } else {
                                continue;
                            }
                        }
                    }
                }
                Ok(None)
            }
            Self::File(_) => {
                let ext = Path::new(self.as_str())
                    .extension()
                    .and_then(|e| e.to_owned().into_string().ok());
                Ok(Some(PreferredFile::new(self.clone(), ext)))
            }
            _ => Ok(None),
        }
    }
}

impl<'de> Deserialize<'de> for Link {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Self::from(s.as_str()))
    }
}

impl Serialize for Link {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::File(s) => s.serialize(serializer),
            Self::Directory(s) => s.serialize(serializer),
            Self::Url(s) => s.serialize(serializer),
        }
    }
}

trait OsProvider {
    fn new() -> Self;
    fn open_file(&self, link: &str) {
        open::that_detached(link).expect("Failed to open file");
    }
    fn explore_at_file(&self, link: &str);
    fn open_dir(&self, link: &str);
    fn open_url(&self, link: &str) {
        open::that_detached(link).expect("Failed to open browser");
    }
}

#[derive(Debug)]
pub struct PreferredFile {
    pub file: Link,
    pub extension: Option<String>,
}

impl PreferredFile {
    fn new(file: Link, extension: Option<String>) -> Self {
        Self { file, extension }
    }

    pub fn open(&self) {
        Provider::new().open_file(self.file.as_str());
    }
}

struct WindowsProvider;

impl OsProvider for WindowsProvider {
    fn new() -> Self {
        Self
    }

    fn explore_at_file(&self, link: &str) {
        Command::new("explorer")
            .arg("/select,")
            .arg(link)
            .spawn()
            .expect("Failed to open explorer");
    }

    fn open_dir(&self, link: &str) {
        Command::new("explorer")
            .arg(link)
            .spawn()
            .expect("Failed to open explorer");
    }
}

struct LinuxProvider;

impl OsProvider for LinuxProvider {
    fn new() -> Self {
        Self
    }

    fn explore_at_file(&self, link: &str) {
        Command::new("xdg-open")
            .arg("--select")
            .arg(link)
            .spawn()
            .expect("Failed to open explorer");
    }

    fn open_dir(&self, link: &str) {
        Command::new("xdg-open")
            .arg(link)
            .spawn()
            .expect("Failed to open explorer");
    }
}

struct MacProvider;

impl OsProvider for MacProvider {
    fn new() -> Self {
        Self
    }

    fn explore_at_file(&self, link: &str) {
        Command::new("open")
            .arg("-R")
            .arg(link)
            .spawn()
            .expect("Failed to open explorer");
    }

    fn open_dir(&self, link: &str) {
        Command::new("open")
            .arg(link)
            .spawn()
            .expect("Failed to open explorer");
    }
}

#[cfg(target_os = "windows")]
type Provider = WindowsProvider;

#[cfg(target_os = "linux")]
type Provider = LinuxProvider;

#[cfg(target_os = "macos")]
type Provider = MacProvider;

use reqwest::blocking::Client;
use scraper::{Html, Selector};

const INFER_TIMEOUT: Duration = Duration::from_millis(1000);
const INFER_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/117.0.0.0 Safari/537.36";

fn get_url_title(url: &str) -> reqwest::Result<Option<String>> {
    let title_selectors = vec![
        Selector::parse("title").unwrap(),
        Selector::parse("head > title").unwrap(),
    ];

    let client = Client::builder()
        .user_agent(INFER_UA)
        .timeout(INFER_TIMEOUT)
        .build()
        .unwrap();

    let resp = client.get(url).send()?;
    let body = resp.text()?;

    let fragment = Html::parse_document(&body);

    Ok(title_selectors.iter().map(|s| fragment.select(s).map(|e| e.inner_html())).flatten().next())
}
