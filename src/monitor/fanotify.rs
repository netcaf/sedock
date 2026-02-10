use crate::monitor::{event, process};
use crate::utils::{EventType, Result, SedockerError};
use nix::sys::stat::Mode;
use std::os::unix::io::RawFd;

const FAN_CLASS_NOTIF: u32 = 0x00000000;
const FAN_MARK_ADD: u32 = 0x00000001;
const FAN_OPEN: u64 = 0x00000020;
const FAN_ACCESS: u64 = 0x00000001;
const FAN_MODIFY: u64 = 0x00000002;
const FAN_EVENT_ON_CHILD: u64 = 0x08000000;

#[repr(C)]
struct FanotifyEventMetadata {
    event_len: u32,
    vers: u8,
    reserved: u8,
    metadata_len: u16,
    mask: u64,
    fd: i32,
    pid: i32,
}

extern "C" {
    fn fanotify_init(flags: u32, event_f_flags: u32) -> i32;
    fn fanotify_mark(
        fanotify_fd: i32,
        flags: u32,
        mask: u64,
        dirfd: i32,
        pathname: *const libc::c_char,
    ) -> i32;
}

pub fn start_monitoring(directory: &str, show_container: bool, format: &str) -> Result<()> {
    // 初始化 fanotify
    let fan_fd = unsafe { fanotify_init(FAN_CLASS_NOTIF, libc::O_RDONLY as u32) };
    if fan_fd < 0 {
        return Err(SedockerError::Fanotify(
            "Failed to initialize fanotify. Are you running as root?".to_string()
        ));
    }
    
    // 添加监控标记
    let dir_cstring = std::ffi::CString::new(directory)
        .map_err(|e| SedockerError::System(format!("Invalid directory path: {}", e)))?;
    
    let mark_result = unsafe {
        fanotify_mark(
            fan_fd,
            FAN_MARK_ADD,
            FAN_OPEN | FAN_ACCESS | FAN_MODIFY | FAN_EVENT_ON_CHILD,
            libc::AT_FDCWD,
            dir_cstring.as_ptr(),
        )
    };
    
    if mark_result < 0 {
        return Err(SedockerError::Fanotify(
            format!("Failed to mark directory: {}", directory)
        ));
    }
    
    // 打印表头
    if format == "text" {
        println!("{:<7} {:<6} {:<5} {:<5} {:<25} {:<15} {}",
                 "EVENT", "PID", "UID", "GID", "PROCESS_PATH", "CONTAINER", "FILE_PATH");
        println!("{}", "-".repeat(120));
    }
    
    // 事件去重器
    let mut dedup = event::EventDeduplicator::new();
    
    // 事件循环
    let mut buffer = vec![0u8; 4096];
    loop {
        let len = unsafe {
            libc::read(fan_fd, buffer.as_mut_ptr() as *mut libc::c_void, buffer.len())
        };
        
        if len <= 0 {
            continue;
        }
        
        let mut offset = 0;
        while offset < len as usize {
            let metadata = unsafe {
                &*(buffer.as_ptr().add(offset) as *const FanotifyEventMetadata)
            };
            
            if metadata.vers != 3 {
                eprintln!("Unsupported fanotify version");
                break;
            }
            
            // 获取文件路径
            let file_path = get_path_from_fd(metadata.fd);
            
            // 去重检查
            if !dedup.is_duplicate(metadata.pid, metadata.mask, &file_path) {
                // 处理事件
                if let Err(e) = handle_event(metadata, &file_path, show_container, format) {
                    eprintln!("Error handling event: {}", e);
                }
            }
            
            // 关闭文件描述符
            unsafe { libc::close(metadata.fd); }
            
            offset += metadata.event_len as usize;
        }
    }
}

fn handle_event(
    metadata: &FanotifyEventMetadata,
    file_path: &str,
    show_container: bool,
    format: &str,
) -> Result<()> {
    // 确定事件类型
    let event_type = if metadata.mask & FAN_MODIFY != 0 {
        EventType::Write
    } else if metadata.mask & FAN_OPEN != 0 {
        EventType::Open
    } else {
        EventType::Read
    };
    
    // 获取进程信息
    let proc_info = process::get_process_info(metadata.pid)?;
    
    // 获取容器信息（如果需要）
    let container_id = if show_container {
        process::get_container_id(metadata.pid)
    } else {
        None
    };
    
    // 创建事件
    let event = event::create_event(
        event_type,
        metadata.pid,
        proc_info.uid,
        proc_info.gid,
        proc_info.exe,
        file_path.to_string(),
        container_id.clone(),
    );
    
    // 输出事件
    if format == "json" {
        println!("{}", serde_json::to_string(&event).unwrap());
    } else {
        println!("[{:<5}] {:<6} {:<5} {:<5} {:<25} {:<15} {}",
                 event.event_type,
                 event.pid,
                 event.uid,
                 event.gid,
                 truncate_string(&event.process_path, 25),
                 container_id.as_deref().unwrap_or("-"),
                 event.file_path);
    }
    
    Ok(())
}

fn get_path_from_fd(fd: RawFd) -> String {
    let link_path = format!("/proc/self/fd/{}", fd);
    match std::fs::read_link(&link_path) {
        Ok(path) => path.to_string_lossy().into_owned(),
        Err(_) => "unknown".to_string(),
    }
}

fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("...{}", &s[s.len().saturating_sub(max_len - 3)..])
    }
}