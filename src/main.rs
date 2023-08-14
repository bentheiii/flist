mod args;
mod config;
mod errors;
mod gui;
mod link;
mod lock;
mod project;
mod requests;

use std::net::TcpListener;

use args::MainArgs;
use chrono::{DateTime, Local};
use clap::Parser;
use errors::LockedProject;
use lock::LockFile;
use project::Project;

fn main() {
    let args = MainArgs::parse();
    let config = args.get_config();

    match config {
        Ok(config) => {
            let lockfile = LockFile::new(&args.project_root);
            let mut project = Project::from_dir(&args.project_root, config);
            let apply_results = args.apply(&mut project);
            if apply_results.should_exit {
                return;
            }
            let listener = TcpListener::bind(("127.0.0.1", 0)).expect("Failed to bind to port");
            let addr = listener.local_addr().expect("Failed to get local addr");
            lockfile.set_listener(addr.ip().to_string(), addr.port());
            gui::main(project, listener, lockfile)
        }
        Err(LockedProject::WithListener(stream)) => args.on_locked(stream),
        Err(LockedProject::WithoutListener(time)) => {
            let time: DateTime<Local> = time.into();
            panic!(
                "Project is locked, last lock was at {}",
                time.format("%Y-%m-%d %H:%M:%S")
            );
        }
    }
}
