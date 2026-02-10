use crate::utils::{ProcessInfo, Result, SedockerError};
use std::fs;
use std::path::PathBuf;

/// 从 PID 获取 UID 和 GID
pub fn get_ids_from_pid(pid: i32) -> Result<(u32, u32)> {
    let status_path = format!("/proc/{}/status", pid);
    let content = fs::read_to_string(&status_path)
        .map_err(|e| SedockerError::System(format!("Cannot read {}: {}", status_path, e)))?;
    
    let mut uid = 0u32;
    let mut gid = 0u32;
    
    for line in content.lines() {
        if line.starts_with("Uid:") {
            uid = line.split_whitespace()
                .nth(1)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
        } else if line.starts_with("Gid:") {
            gid = line.split_whitespace()
                .nth(1)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
        }
    }
    
    Ok((uid, gid))
}

/// 获取进程的可执行文件路径
pub fn get_process_path(pid: i32) -> Result<String> {
    let exe_link = format!("/proc/{}/exe", pid);
    match fs::read_link(&exe_link) {
        Ok(path) => Ok(path.to_string_lossy().into_owned()),
        Err(_) => {
            // 回退：从 cmdline 获取
            let cmdline_path = format!("/proc/{}/cmdline", pid);
            match fs::read_to_string(&cmdline_path) {
                Ok(content) => {
                    let cmd = content.split('\0').next().unwrap_or("unknown");
                    Ok(cmd.to_string())
                }
                Err(_) => Ok("unknown".to_string()),
            }
        }
    }
}

/// 获取进程名称
pub fn get_process_comm(pid: i32) -> Result<String> {
    let comm_path = format!("/proc/{}/comm", pid);
    match fs::read_to_string(&comm_path) {
        Ok(content) => Ok(content.trim().to_string()),
        Err(_) => Ok("unknown".to_string()),
    }
}

/// 检查进程是否在容器中
pub fn get_container_id(pid: i32) -> Option<String> {
    let cgroup_path = format!("/proc/{}/cgroup", pid);
    let content = fs::read_to_string(&cgroup_path).ok()?;
    
    for line in content.lines() {
        if line.contains("docker") || line.contains("containerd") {
            // 提取容器 ID
            if let Some(id) = extract_container_id(line) {
                return Some(id);
            }
        }
    }
    
    None
}

fn extract_container_id(line: &str) -> Option<String> {
    // 从 cgroup 行中提取容器 ID
    // 格式: 12:pids:/docker/1234567890abcdef...
    if let Some(pos) = line.rfind('/') {
        let id = &line[pos + 1..];
        let id = id.trim();
        
        // 取前 12 个字符（短 ID）
        if id.len() >= 12 {
            return Some(id[..12].to_string());
        } else if !id.is_empty() {
            return Some(id.to_string());
        }
    }
    
    None
}

/// 获取完整的进程信息
pub fn get_process_info(pid: i32) -> Result<ProcessInfo> {
    let (uid, gid) = get_ids_from_pid(pid)?;
    let exe = get_process_path(pid)?;
    let comm = get_process_comm(pid)?;
    
    Ok(ProcessInfo {
        pid,
        uid,
        gid,
        comm,
        exe,
    })
}