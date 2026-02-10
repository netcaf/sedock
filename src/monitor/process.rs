use crate::utils::{ProcessInfo, Result, SedockerError};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// 启动时扫描常见 bin 目录，构建 name→path 查找表
/// 事件处理时只需 O(1) HashMap 查找，零磁盘 I/O
pub struct BinPathCache {
    map: HashMap<String, String>,
}

impl BinPathCache {
    pub fn new() -> Self {
        let mut dirs: Vec<String> = vec![
            "/usr/bin".into(),
            "/bin".into(),
            "/usr/sbin".into(),
            "/sbin".into(),
            "/usr/local/bin".into(),
            "/usr/local/sbin".into(),
        ];
        // 追加 PATH 中的额外目录（如 /opt/xxx/bin, /home/xxx/.local/bin 等）
        if let Ok(path_env) = std::env::var("PATH") {
            for p in path_env.split(':') {
                if !p.is_empty() && !dirs.iter().any(|d| d == p) {
                    dirs.push(p.to_string());
                }
            }
        }
        let mut map = HashMap::with_capacity(2048);
        for dir in &dirs {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    if let Some(name) = entry.file_name().to_str() {
                        map.entry(name.to_string())
                            .or_insert_with(|| entry.path().to_string_lossy().into_owned());
                    }
                }
            }
        }
        map.shrink_to_fit();
        Self { map }
    }

    /// O(1) 查找，找不到返回 None
    pub fn resolve(&self, name: &str) -> Option<&str> {
        self.map.get(name).map(|s| s.as_str())
    }
}

// Deref so callers get transparent HashMap access
impl std::ops::Deref for BinPathCache {
    type Target = HashMap<String, String>;
    fn deref(&self) -> &Self::Target {
        &self.map
    }
}

/// 从 PID 获取 UID 和 GID
pub fn get_ids_from_pid(pid: i32) -> Result<(u32, u32)> {
    let status_path = format!("/proc/{}/status", pid);
    let content = fs::read_to_string(&status_path)
        .map_err(|e| {
            // 检查是否是因为进程已退出
            // ENOENT (2): No such file or directory - /proc/{pid} doesn't exist
            // ESRCH (3): No such process - process exited during read
            use std::io::ErrorKind;
            match e.kind() {
                ErrorKind::NotFound => SedockerError::ProcessGone(pid),
                _ => {
                    // Check raw OS error code for ESRCH (3)
                    if let Some(3) = e.raw_os_error() {
                        SedockerError::ProcessGone(pid)
                    } else {
                        SedockerError::System(format!("Cannot read {}: {}", status_path, e))
                    }
                }
            }
        })?;
    
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

/// 获取进程的可执行文件路径（优化版）
pub fn get_process_path(pid: i32) -> Result<String> {
    // 方法1: 读取 /proc/{pid}/exe 符号链接（最快且最准确）
    let exe_link = format!("/proc/{}/exe", pid);
    if let Ok(path) = fs::read_link(&exe_link) {
        let path_str = path.to_string_lossy().into_owned();
        // 移除 " (deleted)" 后缀
        return Ok(path_str.trim_end_matches(" (deleted)").to_string());
    }
    
    // 方法2: 从 cmdline 获取（exe失败时）
    let cmdline_path = format!("/proc/{}/cmdline", pid);
    if let Ok(content) = fs::read_to_string(&cmdline_path) {
        if let Some(cmd) = content.split('\0').next() {
            if !cmd.is_empty() {
                // 绝对路径直接返回
                if cmd.starts_with('/') {
                    return Ok(cmd.to_string());
                }
                // 相对路径：只检查最常见的bin目录
                for prefix in &["/usr/bin/", "/bin/"] {
                    let full_path = format!("{}{}", prefix, cmd);
                    if PathBuf::from(&full_path).exists() {
                        return Ok(full_path);
                    }
                }
                return Ok(cmd.to_string());
            }
        }
    }
    
    // 方法3: 使用 comm（最后手段，返回短名称，由调用方解析完整路径）
    let comm_path = format!("/proc/{}/comm", pid);
    if let Ok(content) = fs::read_to_string(&comm_path) {
        let name = content.trim();
        if !name.is_empty() {
            return Ok(name.to_string());
        }
    }

    Ok(format!("[{}]", pid))
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

/// 获取进程在容器内的 PID（如果在容器中）
/// 
/// 通过读取 /proc/{pid}/status 的 NSpid 字段
/// NSpid 格式: "NSpid:  <host_pid> <container_pid>"
pub fn get_container_pid(host_pid: i32) -> Option<i32> {
    let status_path = format!("/proc/{}/status", host_pid);
    let content = fs::read_to_string(&status_path).ok()?;
    
    for line in content.lines() {
        if line.starts_with("NSpid:") {
            // 解析 "NSpid:  2399439 1"
            let pids: Vec<&str> = line.split_whitespace().skip(1).collect();
            
            // 如果有多个 PID，说明在命名空间中
            if pids.len() >= 2 {
                // 最后一个是最内层命名空间的 PID（容器内 PID）
                return pids.last().and_then(|s| s.parse().ok());
            }
        }
    }
    
    None
}

/// 获取完整的进程信息（优化版：只读取一次status）
pub fn get_process_info(pid: i32, bin_cache: &BinPathCache) -> Result<ProcessInfo> {
    // 一次性读取 status 文件，获取多个字段
    let status_path = format!("/proc/{}/status", pid);
    let status_content = fs::read_to_string(&status_path)
        .map_err(|e| {
            use std::io::ErrorKind;
            match e.kind() {
                ErrorKind::NotFound => SedockerError::ProcessGone(pid),
                _ => {
                    if let Some(3) = e.raw_os_error() {
                        SedockerError::ProcessGone(pid)
                    } else {
                        SedockerError::System(format!("Cannot read {}: {}", status_path, e))
                    }
                }
            }
        })?;
    
    // 从 status 中解析 uid, gid, container_pid, name
    let mut uid = 0u32;
    let mut gid = 0u32;
    let mut container_pid = None;
    let mut comm = String::from("unknown");
    
    for line in status_content.lines() {
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
        } else if line.starts_with("NSpid:") {
            let pids: Vec<&str> = line.split_whitespace().skip(1).collect();
            if pids.len() >= 2 {
                container_pid = pids.last().and_then(|s| s.parse().ok());
            }
        } else if line.starts_with("Name:") {
            if let Some(name) = line.split_whitespace().nth(1) {
                comm = name.to_string();
            }
        }
    }
    
    // 获取 exe 路径（仍需单独读取）
    let exe = get_process_path(pid)?;
    // 如果 exe 只拿到短名称或 [pid]，通过 BinPathCache O(1) 查找完整路径
    let exe = if !exe.contains('/') {
        // 短名称（来自 comm），尝试解析
        bin_cache.resolve(&exe).unwrap_or(&exe).to_string()
    } else if exe.starts_with('[') && comm != "unknown" {
        // [pid] 格式，用 status 中的 Name 字段解析
        bin_cache.resolve(&comm).unwrap_or(&comm).to_string()
    } else {
        exe
    };

    Ok(ProcessInfo {
        pid,
        uid,
        gid,
        comm,
        exe,
        container_pid,
    })
}