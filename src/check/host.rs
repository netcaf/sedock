//! 宿主机层信息收集
//! 来源：/proc/*, /etc/os-release, /sys/fs/cgroup, 系统命令

use serde::{Deserialize, Serialize};
use std::fs;
use crate::utils::{Result, SedockerError};

// ── 数据结构 ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostInfo {
    pub os: OsInfo,
    pub cpu: CpuInfo,
    pub memory: MemoryInfo,
    pub disk: Vec<DiskInfo>,
    pub cgroup_version: String,   // "v1" / "v2"
    pub security: SecurityInfo,
    pub time: TimeInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsInfo {
    pub hostname: String,
    pub os_release: String,       // PRETTY_NAME
    pub kernel: String,           // uname -r
    pub arch: String,
    pub uptime_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuInfo {
    pub model: String,
    pub logical_cores: u32,
    pub load_avg_1: f64,
    pub load_avg_5: f64,
    pub load_avg_15: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryInfo {
    pub total_kb: u64,
    pub available_kb: u64,
    pub used_kb: u64,
    pub used_percent: f64,
    pub swap_total_kb: u64,
    pub swap_used_kb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskInfo {
    pub mount: String,
    pub filesystem: String,
    pub total_kb: u64,
    pub used_kb: u64,
    pub available_kb: u64,
    pub used_percent: f64,
    pub inode_used_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityInfo {
    pub selinux: String,     // "enforcing" / "permissive" / "disabled" / "unavailable"
    pub apparmor: String,    // "enabled" / "disabled" / "unavailable"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeInfo {
    pub system_time: String,
    pub ntp_synced: bool,
}

// ── 收集入口 ────────────────────────────────────────────────────────────────

pub fn collect() -> Result<HostInfo> {
    Ok(HostInfo {
        os:             collect_os()?,
        cpu:            collect_cpu()?,
        memory:         collect_memory()?,
        disk:           collect_disk()?,
        cgroup_version: detect_cgroup_version(),
        security:       collect_security(),
        time:           collect_time(),
    })
}

// ── OS ──────────────────────────────────────────────────────────────────────

fn collect_os() -> Result<OsInfo> {
    let hostname = fs::read_to_string("/proc/sys/kernel/hostname")
        .unwrap_or_default()
        .trim()
        .to_string();

    let os_release = parse_os_release();

    let kernel = fs::read_to_string("/proc/sys/kernel/osrelease")
        .unwrap_or_default()
        .trim()
        .to_string();

    let arch = std::process::Command::new("uname")
        .arg("-m")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    let uptime_seconds = fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| s.split_whitespace().next().map(|v| v.to_string()))
        .and_then(|v| v.parse::<f64>().ok())
        .map(|v| v as u64)
        .unwrap_or(0);

    Ok(OsInfo { hostname, os_release, kernel, arch, uptime_seconds })
}

fn parse_os_release() -> String {
    fs::read_to_string("/etc/os-release")
        .unwrap_or_default()
        .lines()
        .find(|l| l.starts_with("PRETTY_NAME="))
        .map(|l| l.trim_start_matches("PRETTY_NAME=").trim_matches('"').to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

// ── CPU ─────────────────────────────────────────────────────────────────────

fn collect_cpu() -> Result<CpuInfo> {
    let cpuinfo = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();

    let model = cpuinfo
        .lines()
        .find(|l| l.starts_with("model name"))
        .and_then(|l| l.splitn(2, ':').nth(1))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let logical_cores = cpuinfo
        .lines()
        .filter(|l| l.starts_with("processor"))
        .count() as u32;

    let (load_avg_1, load_avg_5, load_avg_15) = parse_loadavg();

    Ok(CpuInfo { model, logical_cores, load_avg_1, load_avg_5, load_avg_15 })
}

fn parse_loadavg() -> (f64, f64, f64) {
    let s = fs::read_to_string("/proc/loadavg").unwrap_or_default();
    let mut parts = s.split_whitespace();
    let v1 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let v5 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let v15 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
    (v1, v5, v15)
}

// ── Memory ──────────────────────────────────────────────────────────────────

fn collect_memory() -> Result<MemoryInfo> {
    let meminfo = fs::read_to_string("/proc/meminfo")
        .map_err(|e| SedockerError::Io(e))?;

    let get = |key: &str| -> u64 {
        meminfo.lines()
            .find(|l| l.starts_with(key))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    };

    let total_kb     = get("MemTotal:");
    let available_kb = get("MemAvailable:");
    let used_kb      = total_kb.saturating_sub(available_kb);
    let used_percent = if total_kb > 0 {
        used_kb as f64 / total_kb as f64 * 100.0
    } else { 0.0 };

    let swap_total_kb = get("SwapTotal:");
    let swap_free_kb  = get("SwapFree:");
    let swap_used_kb  = swap_total_kb.saturating_sub(swap_free_kb);

    Ok(MemoryInfo {
        total_kb,
        available_kb,
        used_kb,
        used_percent,
        swap_total_kb,
        swap_used_kb,
    })
}

// ── Disk ────────────────────────────────────────────────────────────────────

fn collect_disk() -> Result<Vec<DiskInfo>> {
    let output = std::process::Command::new("df")
        .args(&["-Pk"])   // POSIX, kB
        .output();

    let inode_output = std::process::Command::new("df")
        .args(&["-Pi"])   // inode
        .output();

    let mut disks = Vec::new();

    let out = match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => return Ok(disks),
    };

    // inode map: mount -> used%
    let inode_map = parse_inode_percents(&inode_output.ok());

    for line in out.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 6 { continue; }

        // 跳过 tmpfs / devtmpfs 等虚拟 fs，只保留真实挂载点
        let fs = parts[0];
        if fs.starts_with("tmpfs") || fs.starts_with("devtmpfs") || fs.starts_with("overlay") {
            continue;
        }

        let total_kb: u64     = parts[1].parse().unwrap_or(0);
        let used_kb: u64      = parts[2].parse().unwrap_or(0);
        let available_kb: u64 = parts[3].parse().unwrap_or(0);
        let used_percent: f64 = parts[4].trim_end_matches('%').parse().unwrap_or(0.0);
        let mount             = parts[5].to_string();

        let inode_used_percent = inode_map.get(&mount).copied().unwrap_or(0.0);

        disks.push(DiskInfo {
            mount,
            filesystem: fs.to_string(),
            total_kb,
            used_kb,
            available_kb,
            used_percent,
            inode_used_percent,
        });
    }

    Ok(disks)
}

fn parse_inode_percents(output: &Option<std::process::Output>) -> std::collections::HashMap<String, f64> {
    let mut map = std::collections::HashMap::new();
    if let Some(o) = output {
        if o.status.success() {
            for line in String::from_utf8_lossy(&o.stdout).lines().skip(1) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 6 {
                    let mount = parts[5].to_string();
                    let pct: f64 = parts[4].trim_end_matches('%').parse().unwrap_or(0.0);
                    map.insert(mount, pct);
                }
            }
        }
    }
    map
}

// ── cgroup ──────────────────────────────────────────────────────────────────

fn detect_cgroup_version() -> String {
    // cgroup v2: /sys/fs/cgroup/cgroup.controllers 存在
    if std::path::Path::new("/sys/fs/cgroup/cgroup.controllers").exists() {
        "v2".to_string()
    } else if std::path::Path::new("/sys/fs/cgroup/memory/memory.limit_in_bytes").exists() {
        "v1".to_string()
    } else {
        "unknown".to_string()
    }
}

// ── Security ────────────────────────────────────────────────────────────────

fn collect_security() -> SecurityInfo {
    let selinux = read_selinux_status();
    let apparmor = read_apparmor_status();
    SecurityInfo { selinux, apparmor }
}

fn read_selinux_status() -> String {
    // 先查 /sys/fs/selinux/enforce
    if let Ok(val) = fs::read_to_string("/sys/fs/selinux/enforce") {
        return match val.trim() {
            "1" => "enforcing".to_string(),
            "0" => "permissive".to_string(),
            _   => "unknown".to_string(),
        };
    }
    // 再尝试 getenforce 命令
    if let Ok(o) = std::process::Command::new("getenforce").output() {
        let s = String::from_utf8_lossy(&o.stdout).trim().to_lowercase();
        if !s.is_empty() { return s; }
    }
    "disabled".to_string()
}

fn read_apparmor_status() -> String {
    if std::path::Path::new("/sys/kernel/security/apparmor/profiles").exists() {
        "enabled".to_string()
    } else if std::path::Path::new("/sys/module/apparmor").exists() {
        "enabled".to_string()
    } else {
        "disabled".to_string()
    }
}

// ── Time ────────────────────────────────────────────────────────────────────

fn collect_time() -> TimeInfo {
    let system_time = chrono::Local::now().format("%Y-%m-%d %H:%M:%S %z").to_string();

    // timedatectl 检查 NTP，失败时回退到 /run/systemd/timesync/synchronized
    let ntp_synced = check_ntp_sync();

    TimeInfo { system_time, ntp_synced }
}

fn check_ntp_sync() -> bool {
    // 方法1: timedatectl
    if let Ok(o) = std::process::Command::new("timedatectl").output() {
        let out = String::from_utf8_lossy(&o.stdout);
        if out.contains("synchronized: yes") || out.contains("NTP synchronized: yes") {
            return true;
        }
        if out.contains("synchronized: no") || out.contains("NTP synchronized: no") {
            return false;
        }
    }
    // 方法2: systemd timesync sentinel 文件
    std::path::Path::new("/run/systemd/timesync/synchronized").exists()
}
