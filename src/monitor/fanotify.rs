use crate::monitor::{event, process};
use crate::utils::{EventType, Result, SedockerError};
use lru::LruCache;
use nix::sys::stat::Mode;
use std::num::NonZeroUsize;
use std::os::unix::io::RawFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const FAN_CLASS_NOTIF: u32 = 0x00000000;
const FAN_MARK_ADD: u32 = 0x00000001;
const FAN_OPEN: u64 = 0x00000020;
const FAN_ACCESS: u64 = 0x00000001;
const FAN_MODIFY: u64 = 0x00000002;
const FAN_EVENT_ON_CHILD: u64 = 0x08000000;

/// 进程路径缓存，用于捕获短暂进程的完整路径
struct ProcessCache {
    cache: LruCache<i32, String>,
}

impl ProcessCache {
    fn new() -> Self {
        Self {
            cache: LruCache::new(NonZeroUsize::new(1000).unwrap()),
        }
    }
    
    /// 获取进程路径，优先从缓存读取
    fn get_or_fetch(&mut self, pid: i32) -> String {
        // 先查缓存
        if let Some(path) = self.cache.get(&pid) {
            return path.clone();
        }
        
        // 缓存未命中，尝试读取当前进程路径
        if let Ok(path) = process::get_process_path(pid) {
            // 只缓存有效路径（非 [pid] 格式）
            if !path.starts_with('[') {
                self.cache.put(pid, path.clone());
                return path;
            }
        }
        
        // 无法获取，返回 pid 格式
        format!("[{}]", pid)
    }
}


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

pub fn start_monitoring(directory: &str, format: &str, verbose: bool) -> Result<()> {
    // 设置 Ctrl+C 处理
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
        eprintln!("\nCtrl+C received, exiting...");
        std::process::exit(0);
    }).expect("Error setting Ctrl-C handler");
    
    // 初始化 fanotify (使用 O_NONBLOCK 提高响应速度)
    let fan_fd = unsafe { 
        fanotify_init(
            FAN_CLASS_NOTIF, 
            (libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NONBLOCK) as u32
        ) 
    };
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
        println!("{:<7} {:<13} {:<5} {:<5} {:<25} {:<15} {}",
                 "EVENT", "PID(H/C)", "UID", "GID", "PROCESS_PATH", "CONTAINER", "FILE_PATH");
        println!("{}", "-".repeat(130));
    }
    
    // 事件去重器（可选）
    let mut dedup = if verbose {
        None
    } else {
        Some(event::EventDeduplicator::new())
    };
    
    // 进程路径缓存（用于捕获短暂进程）
    let mut proc_cache = ProcessCache::new();

    
    // 事件循环（使用更大的缓冲区处理快速事件）
    let mut buffer = vec![0u8; 16384]; // 4x增大，减少read()调用次数
    while running.load(Ordering::SeqCst) {
        let len = unsafe {
            libc::read(fan_fd, buffer.as_mut_ptr() as *mut libc::c_void, buffer.len())
        };
        
        if len < 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::EAGAIN) || err.raw_os_error() == Some(libc::EWOULDBLOCK) {
                // 非阻塞模式下没有数据，短暂休眠避免CPU空转
                std::thread::sleep(std::time::Duration::from_micros(100));
                continue;
            }
            eprintln!("Read error: {}", err);
            continue;
        }
        
        if len == 0 {
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
            
            // **FIX: 立即读取进程信息，避免竞态条件**
            // 快速命令(cat/tail/head)可能在处理前就退出
            let proc_info = match process::get_process_info(metadata.pid) {
                Ok(info) => {
                    // 成功读取，同时填充缓存
                    if !info.exe.starts_with('[') {
                        proc_cache.cache.put(metadata.pid, info.exe.clone());
                    }
                    Some(info)
                }
                Err(SedockerError::ProcessGone(_)) => {
                    // 进程已退出，仍输出基本信息
                    None
                }
                Err(e) => {
                    eprintln!("Error reading process info: {}", e);
                    unsafe { libc::close(metadata.fd); }
                    offset += metadata.event_len as usize;
                    continue;
                }
            };
            
            // 获取容器信息
            let container_id = process::get_container_id(metadata.pid);
            
            // 条件去重检查
            let should_process = if let Some(ref mut d) = dedup {
                !d.is_duplicate(metadata.pid, metadata.mask, &file_path)
            } else {
                true  // 禁用去重，处理所有事件
            };
            
            if should_process {
                // 处理事件（传入已读取的进程信息和路径缓存）
                if let Err(e) = handle_event(metadata, &file_path, format, proc_info, container_id, &mut proc_cache) {
                    eprintln!("Error handling event: {}", e);
                }
            }
            
            // 关闭文件描述符
            unsafe { libc::close(metadata.fd); }
            
            offset += metadata.event_len as usize;
        }
    }
    
    // 清理
    unsafe { libc::close(fan_fd); }
    if format == "text" {
        eprintln!("\nMonitoring stopped.");
    }
    
    Ok(())
}

fn handle_event(
    metadata: &FanotifyEventMetadata,
    file_path: &str,
    format: &str,
    proc_info: Option<crate::utils::ProcessInfo>,
    container_id: Option<String>,
    proc_cache: &mut ProcessCache,
) -> Result<()> {
    // 确定事件类型
    let event_type = if metadata.mask & FAN_MODIFY != 0 {
        EventType::Write
    } else if metadata.mask & FAN_OPEN != 0 {
        EventType::Open
    } else {
        EventType::Read
    };
    
    // 处理进程信息
    let (container_pid, uid, gid, exe) = if let Some(info) = proc_info {
        (info.container_pid, info.uid, info.gid, info.exe)
    } else {
        // 进程已退出，从缓存获取路径
        (None, 0, 0, proc_cache.get_or_fetch(metadata.pid))
    };
    
    // 创建事件
    let event = event::create_event(
        event_type,
        metadata.pid,
        container_pid,
        uid,
        gid,
        exe,
        file_path.to_string(),
        container_id.clone(),
    );
    
    // 输出事件
    if format == "json" {
        println!("{}", serde_json::to_string(&event).unwrap());
    } else {
        // 格式化 PID 显示
        let pid_display = if let Some(cpid) = event.container_pid {
            format!("{}/{}", event.pid, cpid)
        } else {
            format!("{}", event.pid)
        };
        
        println!("[{:<5}] {:<13} {:<5} {:<5} {:<25} {:<15} {}",
                 event.event_type,
                 pid_display,
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