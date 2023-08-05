use std::path::Path;

pub fn open(link: &str) {
    let link = Path::new(link);
    assert!(link.is_absolute());
    if link.is_dir() {
        os::open_dir(link.to_str().unwrap());
    } else {
        os::open_select(link.to_str().unwrap());
    };
}

#[cfg(target_os = "windows")]
mod os {
    pub(super) fn open_dir(link: &str) {
        use std::process::Command;
        Command::new("explorer")
            .arg(link)
            .spawn()
            .expect("Failed to open explorer");
    }

    pub(super) fn open_select(link: &str) {
        use std::process::Command;
        Command::new("explorer")
            .arg("/select,")
            .arg(link)
            .spawn()
            .expect("Failed to open explorer");
    }
}

#[cfg(target_os = "linux")]
mod os {
    fn open_dir(link: &str) {
        use std::process::Command;
        Command::new("xdg-open")
            .arg(link)
            .spawn()
            .expect("Failed to open explorer");
    }

    fn open_select(link: &str) {
        use std::process::Command;
        Command::new("xdg-open")
            .arg("--select")
            .arg(link)
            .spawn()
            .expect("Failed to open explorer");
    }
}

#[cfg(target_os = "macos")]
mod os {
    fn open_dir(link: &str) {
        use std::process::Command;
        Command::new("open")
            .arg(link)
            .spawn()
            .expect("Failed to open explorer");
    }

    fn open_select(link: &str) {
        use std::process::Command;
        Command::new("open")
            .arg("-R")
            .arg(link)
            .spawn()
            .expect("Failed to open explorer");
    }
}
