use chrono::Utc;
use clap::{Args, Parser, Subcommand};
use std::fs;
use std::fs::create_dir_all;
use std::io::{BufWriter, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use crate::config::{self, FlistConfig, Lock, LockedWithoutListener};
use crate::errors::LockedProject;
use crate::project::Project;
use crate::requests::InsertRequest;

const SECS_OF_GRACE_FOR_NONLISTENING_LOCK: u64 = 60;
const LOCK_CONNECTION_TIMEOUT_MS: u64 = 250;

#[derive(Debug)]
pub struct ArgsApplyResult {
    pub should_exit: bool,
}

#[derive(Debug, Parser)]
#[command(author, version)]
pub struct MainArgs {
    /// the path to a directory containing a flist.toml file. Defaults to the current directory.
    #[arg(value_name = "DIR", default_value = ".")]
    pub project_root: PathBuf,
    #[command(subcommand)]
    command: Option<Command>,
    /// exit after completing the command
    #[arg(short, long)]
    pub exit: bool,
}

impl MainArgs {
    pub fn on_locked(self, stream: TcpStream) {
        self.command.unwrap_or_default().on_locked(stream)
    }

    pub fn get_config(&self) -> Result<FlistConfig, LockedProject> {
        match self.command.as_ref() {
            Some(Command::New(new_args)) => {
                let config_path = self.project_root.join("flist.toml");
                let files_to_delete = if !self.project_root.exists() {
                    create_dir_all(&self.project_root).expect("Failed to create project directory");
                    vec![]
                } else if !self.project_root.is_dir() {
                    panic!("Project root is not a directory");
                } else {
                    if !new_args.force {
                        // dir already existed and we can't overwrite an existing toml, we need to check if the plint project exists
                        if config_path.exists() {
                            panic!("Project already exists, to overwrite use --force");
                        }
                    }

                    let mut files_to_delete = vec![];
                    for delete_candidate in ["flist.lock", "entries.json", "archive.json"] {
                        let delete_candidate = self.project_root.join(delete_candidate);
                        if delete_candidate.exists() {
                            files_to_delete.push(delete_candidate);
                        }
                    }
                    files_to_delete
                };
                let quick_launch = if let Some(quick_launch) = &new_args.quick_launch {
                    quick_launch
                        .split(',')
                        .map(|layer| layer.split('|').map(|suffix| suffix.to_string()).collect())
                        .collect()
                } else {
                    vec![]
                };
                let config = FlistConfig::new(
                    new_args.max_archive.unwrap_or(config::DEFAULT_MAX_ARCHIVE),
                    quick_launch,
                );

                fs::write(
                    config_path,
                    toml::to_string(&config).expect("Failed to serialize config"),
                )
                .expect("failed to write config file");

                if new_args.clear {
                    for file in files_to_delete {
                        fs::remove_file(file).expect("Failed to delete file");
                    }
                }
                Ok(config)
            }
            _ => {
                let lock_path = self.project_root.join("flist.lock");
                if lock_path.exists() {
                    // file is locked, we need to read the lock file, and attempt to establish a connection.
                    let lock: Lock = serde_json::from_str(
                        &fs::read_to_string(&lock_path).expect("Failed to read lock file"),
                    )
                    .expect("failed to read lock file");
                    match lock {
                        Lock::WithListener(listener) => {
                            let hostname = IpAddr::from_str(&listener.hostname)
                                .expect("Failed to parse hostname");
                            let stream = TcpStream::connect_timeout(
                                &SocketAddr::from((hostname, listener.listener_port)),
                                Duration::from_millis(LOCK_CONNECTION_TIMEOUT_MS),
                            );
                            if let Ok(stream) = stream {
                                return Err(LockedProject::WithListener(stream));
                            }
                            // if the connection failed, the lock can be deleted
                        }
                        Lock::WithoutListener(LockedWithoutListener { time_locked }) => {
                            let diff: u64 = (time_locked - Utc::now())
                                .num_seconds()
                                .try_into()
                                .unwrap_or_default();
                            if diff < SECS_OF_GRACE_FOR_NONLISTENING_LOCK {
                                // if the lock was created less than a minute ago, we can't delete it
                                return Err(LockedProject::WithoutListener(time_locked));
                            }
                        }
                    }
                    // if we made it this far, we can delete the lock
                    fs::remove_file(lock_path).expect("Failed to delete lock file");
                }
                let config_path = self.project_root.join("flist.toml");
                if !config_path.exists() {
                    panic!("No flist.toml found in project directory");
                }
                let config = fs::read_to_string(config_path).expect("Failed to read config file");
                Ok(toml::from_str(&config).expect("Failed to parse config file"))
            }
        }
    }

    pub fn apply(self, project: &mut Project) -> ArgsApplyResult {
        let should_exit = self.exit;
        self.command.unwrap_or_default().apply(project);
        ArgsApplyResult { should_exit }
    }
}

#[derive(Debug, Subcommand, Default)]
pub enum Command {
    /// Create a new flist project
    New(NewArgs),
    /// view the project
    #[default]
    View,
    /// adds a new entry to the project
    Add(AddArgs),
}

impl Command {
    fn on_locked(self, stream: TcpStream) {
        match self {
            Self::New(..) => unreachable!(),
            Self::View => {}
            Self::Add(args) => {
                let request = InsertRequest::from(args);
                let mut stream = BufWriter::new(stream);
                serde_json::to_writer(&mut stream, &request).expect("Failed to serialize request");
                stream.flush().expect("Failed to send request");
            }
        }
    }

    fn apply(self, project: &mut Project) {
        match self {
            Self::New(..) | Self::View => {}
            Self::Add(args) => {
                let request = InsertRequest::from(args).into();
                project.insert_entry(request);
                project.save();
            }
        }
    }
}

#[derive(Debug, Args)]
pub struct NewArgs {
    /// The maximum number of archives to keep.
    #[arg(short, long)]
    pub max_archive: Option<usize>,
    /// The prefferred file suffixes for quick launch, each layer is seperated by a comma, each entry in a layer is seperated by a pipe.
    #[arg(short, long)]
    pub quick_launch: Option<String>,
    /// whether to overwrite an existing project.
    #[arg(short, long)]
    pub force: bool,
    /// whether to clear existing flist files from the project directory.
    #[arg(short, long)]
    pub clear: bool,
}

#[derive(Debug, Args)]
pub struct AddArgs {
    /// the name of the entry
    pub name: String,
    /// the link to the entry
    pub link: String,
    /// metadata to add to the entry
    #[arg(short, long)]
    pub metadata: Vec<String>,
}
