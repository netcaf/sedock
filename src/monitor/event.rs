use crate::utils::{EventType, FileAccessEvent};
use chrono::Local;

pub struct EventDeduplicator {
    last_pid: i32,
    last_mask: u64,
    last_path: String,
}

impl EventDeduplicator {
    pub fn new() -> Self {
        Self {
            last_pid: 0,
            last_mask: 0,
            last_path: String::new(),
        }
    }
    
    pub fn is_duplicate(&mut self, pid: i32, mask: u64, path: &str) -> bool {
        let is_dup = pid == self.last_pid && mask == self.last_mask && path == self.last_path;
        
        self.last_pid = pid;
        self.last_mask = mask;
        self.last_path = path.to_string();
        
        is_dup
    }
}

pub fn create_event(
    event_type: EventType,
    pid: i32,
    container_pid: Option<i32>,
    uid: u32,
    gid: u32,
    process_path: String,
    file_path: String,
    container_id: Option<String>,
) -> FileAccessEvent {
    FileAccessEvent {
        event_type: event_type.to_string(),
        timestamp: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        pid,
        container_pid,
        uid,
        gid,
        process_path,
        file_path,
        container_id,
    }
}