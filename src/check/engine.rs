//! Docker 引擎层信息收集
//! 来源：docker version, docker info, daemon.json, journald/syslog

use serde::{Deserialize, Serialize};
use std::process::Command;
use crate::utils::{Result, SedockerError};

// ── 数据结构 ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineInfo {
    pub version: VersionInfo,
    pub runtime: RuntimeInfo,
    pub daemon_config: DaemonConfig,
    pub daemon_logs: Vec<String>,     // 最近的 warning/error
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    pub server_version: String,
    pub api_version: String,
    pub go_version: String,
    pub os_arch: String,
    pub build_time: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeInfo {
    pub storage_driver: String,
    pub cgroup_driver: String,       // systemd / cgroupfs
    pub cgroup_version: String,
    pub root_dir: String,
    pub total_containers: u64,
    pub running_containers: u64,
    pub paused_containers: u64,
    pub stopped_containers: u64,
    pub total_images: u64,
    pub memory_limit: bool,
    pub swap_limit: bool,
    pub kernel_memory: bool,
    pub oom_kill_disable: bool,
    pub ipv4_forwarding: bool,
    pub bridge_nf_iptables: bool,
    pub default_runtime: String,
    pub log_driver: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    pub config_file: String,         // daemon.json 路径
    pub raw: Option<serde_json::Value>, // 原始内容（若存在）
}

// ── 收集入口 ────────────────────────────────────────────────────────────────

pub fn collect(verbose: bool) -> Result<EngineInfo> {
    let version = collect_version()?;
    let runtime = collect_runtime()?;
    let daemon_config = collect_daemon_config();
    let daemon_logs = if verbose {
        collect_daemon_logs(50)
    } else {
        collect_daemon_logs(20)
    };

    Ok(EngineInfo { version, runtime, daemon_config, daemon_logs })
}

// ── docker version ──────────────────────────────────────────────────────────

fn collect_version() -> Result<VersionInfo> {
    let output = Command::new("docker")
        .args(&["version", "-f", "json"])
        .output()
        .map_err(|e| SedockerError::Docker(format!("docker version failed: {}", e)))?;

    if !output.status.success() {
        return Err(SedockerError::Docker(
            "docker version command failed — is Docker running?".to_string()
        ));
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| SedockerError::Parse(format!("docker version JSON: {}", e)))?;

    let server = &json["Server"];

    Ok(VersionInfo {
        server_version: str_val(&server["Version"]),
        api_version:    str_val(&server["ApiVersion"]),
        go_version:     str_val(&server["GoVersion"]),
        os_arch:        format!("{}/{}", str_val(&server["Os"]), str_val(&server["Arch"])),
        build_time:     str_val(&server["BuildTime"]),
    })
}

// ── docker info ─────────────────────────────────────────────────────────────

fn collect_runtime() -> Result<RuntimeInfo> {
    let output = Command::new("docker")
        .args(&["info", "--format", "{{json .}}"])
        .output()
        .map_err(|e| SedockerError::Docker(format!("docker info failed: {}", e)))?;

    if !output.status.success() {
        return Err(SedockerError::Docker("docker info command failed".to_string()));
    }

    let j: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| SedockerError::Parse(format!("docker info JSON: {}", e)))?;

    Ok(RuntimeInfo {
        storage_driver:      str_val(&j["Driver"]),
        cgroup_driver:       str_val(&j["CgroupDriver"]),
        cgroup_version:      str_val(&j["CgroupVersion"]),
        root_dir:            str_val(&j["DockerRootDir"]),
        total_containers:    j["Containers"].as_u64().unwrap_or(0),
        running_containers:  j["ContainersRunning"].as_u64().unwrap_or(0),
        paused_containers:   j["ContainersPaused"].as_u64().unwrap_or(0),
        stopped_containers:  j["ContainersStopped"].as_u64().unwrap_or(0),
        total_images:        j["Images"].as_u64().unwrap_or(0),
        memory_limit:        j["MemoryLimit"].as_bool().unwrap_or(false),
        swap_limit:          j["SwapLimit"].as_bool().unwrap_or(false),
        kernel_memory:       j["KernelMemory"].as_bool().unwrap_or(false),
        oom_kill_disable:    j["OomKillDisable"].as_bool().unwrap_or(false),
        ipv4_forwarding:     j["IPv4Forwarding"].as_bool().unwrap_or(false),
        bridge_nf_iptables:  j["BridgeNfIptables"].as_bool().unwrap_or(false),
        default_runtime:     str_val(&j["DefaultRuntime"]),
        log_driver:          str_val(&j["LoggingDriver"]),
    })
}

// ── daemon.json ─────────────────────────────────────────────────────────────

fn collect_daemon_config() -> DaemonConfig {
    let paths = ["/etc/docker/daemon.json", "/etc/docker/daemon.json.d/daemon.json"];

    for path in &paths {
        if let Ok(content) = std::fs::read_to_string(path) {
            let raw = serde_json::from_str(&content).ok();
            return DaemonConfig {
                config_file: path.to_string(),
                raw,
            };
        }
    }

    DaemonConfig {
        config_file: "not found".to_string(),
        raw: None,
    }
}

// ── daemon logs ─────────────────────────────────────────────────────────────

fn collect_daemon_logs(lines: usize) -> Vec<String> {
    // 方法1: journalctl
    if let Ok(o) = Command::new("journalctl")
        .args(&[
            "-u", "docker",
            "--no-pager",
            "-n", &lines.to_string(),
            "-p", "warning",   // warning 以上
            "--output", "short-iso",
        ])
        .output()
    {
        if o.status.success() {
            let out = String::from_utf8_lossy(&o.stdout);
            let result: Vec<String> = out.lines()
                .map(|l| l.to_string())
                .collect();
            if !result.is_empty() {
                return result;
            }
        }
    }

    // 方法2: /var/log/docker.log
    if let Ok(content) = std::fs::read_to_string("/var/log/docker.log") {
        return content.lines()
            .rev()
            .take(lines)
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
    }

    vec!["daemon logs unavailable".to_string()]
}

// ── 工具 ────────────────────────────────────────────────────────────────────

fn str_val(v: &serde_json::Value) -> String {
    v.as_str().unwrap_or("").to_string()
}
