use std::net::TcpStream;

use chrono::{DateTime, Utc};

pub enum LockedProject {
    WithListener(TcpStream),
    WithoutListener(DateTime<Utc>),
}
