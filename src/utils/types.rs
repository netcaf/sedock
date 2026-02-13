use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: i32,
    pub uid: u32,
    pub gid: u32,
    pub container_pid: Option<i32>,
    pub comm: String,
    pub exe: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
    Open,
    Read,
    Write,
    #[allow(dead_code)]
    Modify,
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventType::Open => write!(f, "OPEN"),
            EventType::Read => write!(f, "READ"),
            EventType::Write => write!(f, "WRITE"),
            EventType::Modify => write!(f, "MODIFY"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAccessEvent {
    pub event_type: String,
    pub timestamp: String,
    pub pid: i32,
    pub container_pid: Option<i32>,
    pub uid: u32,
    pub gid: u32,
    pub process_path: String,
    pub file_path: String,
    pub container_id: Option<String>,
}