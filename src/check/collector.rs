//! 容器信息收集
//! 来源：docker inspect / docker stats / docker logs / /proc

use crate::check::container::*;
use crate::utils::{Result, SedockerError};
use std::process::Command;

const LOG_TAIL_LINES: &str = "50";

// ── 公开接口 ────────────────────────────────────────────────────────────────

pub fn collect_all(verbose: bool) -> Result<Vec<ContainerInfo>> {
    let ids = list_container_ids()?;
    let mut containers = Vec::new();

    for id in &ids {
        match collect_one(id, verbose) {
            Ok(info) => containers.push(info),
            Err(e)   => eprintln!("warn: skipping {}: {}", id, e),
        }
    }

    Ok(containers)
}

pub fn collect_one(id: &str, verbose: bool) -> Result<ContainerInfo> {
    let json = docker_inspect(id)?;
    let mut info = parse_inspect(&json, verbose)?;

    // 仅 running 容器才有 stats
    if info.status == "running" {
        info.resource_usage = fetch_stats(id);
        info.log_tail       = fetch_logs(id, LOG_TAIL_LINES);
    } else {
        // exited 容器也拿日志，有助于排障
        info.log_tail = fetch_logs(id, LOG_TAIL_LINES);
    }

    Ok(info)
}

// ── docker ps / inspect ─────────────────────────────────────────────────────

fn list_container_ids() -> Result<Vec<String>> {
    let out = Command::new("docker")
        .args(&["ps", "-a", "--format", "{{.ID}}"])
        .output()
        .map_err(|e| SedockerError::Docker(format!("docker ps failed: {}", e)))?;

    if !out.status.success() {
        return Err(SedockerError::Docker(
            "docker ps failed — is Docker running?".to_string()
        ));
    }

    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect())
}

fn docker_inspect(id: &str) -> Result<serde_json::Value> {
    let out = Command::new("docker")
        .args(&["inspect", id])
        .output()
        .map_err(|e| SedockerError::Docker(format!("docker inspect failed: {}", e)))?;

    if !out.status.success() {
        return Err(SedockerError::Docker(format!("container {} not found", id)));
    }

    let arr: serde_json::Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| SedockerError::Parse(format!("inspect JSON: {}", e)))?;

    arr.as_array()
        .and_then(|a| a.first())
        .cloned()
        .ok_or_else(|| SedockerError::Parse("empty inspect result".to_string()))
}

// ── inspect パーサー ─────────────────────────────────────────────────────────

fn parse_inspect(c: &serde_json::Value, verbose: bool) -> Result<ContainerInfo> {
    let id = c["Id"].as_str().unwrap_or("").chars().take(12).collect();
    let name = c["Name"].as_str().unwrap_or("")
        .trim_start_matches('/').to_string();
    let image    = str_val(c, &["Config", "Image"]);
    let image_id = c["Image"].as_str().unwrap_or("").chars().take(19).collect();

    let status      = str_val(c, &["State", "Status"]);
    let exit_code   = c["State"]["ExitCode"].as_i64().unwrap_or(0);
    let oom_killed  = c["State"]["OOMKilled"].as_bool().unwrap_or(false);
    let created     = str_val(c, &["Created"]);
    let started_at  = str_val(c, &["State", "StartedAt"]);
    let finished_at = str_val(c, &["State", "FinishedAt"]);

    let restart_policy = str_val(c, &["HostConfig", "RestartPolicy", "Name"]);
    let restart_count  = c["RestartCount"].as_i64().unwrap_or(0);
    let privileged     = c["HostConfig"]["Privileged"].as_bool().unwrap_or(false);

    let env = if verbose {
        c["Config"]["Env"].as_array()
            .map(|a| a.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect())
            .unwrap_or_default()
    } else {
        vec![]
    };

    let ports        = parse_ports(c);
    let networks     = parse_networks(c);
    let network_mode = str_val(c, &["HostConfig", "NetworkMode"]);
    let mounts       = parse_mounts(c);
    let resource_config = parse_resource_config(c);

    let process_info = if verbose {
        parse_process_info(c)
    } else {
        None
    };

    Ok(ContainerInfo {
        id, name, image, image_id,
        status, exit_code, oom_killed,
        created, started_at, finished_at,
        restart_policy, restart_count, privileged, env,
        ports, networks, network_mode, mounts,
        resource_config,
        resource_usage: None,
        log_tail: None,
        process_info,
    })
}

fn parse_ports(c: &serde_json::Value) -> Vec<PortMapping> {
    let mut ports = Vec::new();
    if let Some(bindings) = c["HostConfig"]["PortBindings"].as_object() {
        for (container_port, bindings_arr) in bindings {
            let (cport, proto) = container_port
                .split_once('/')
                .map(|(p, r)| (p.to_string(), r.to_string()))
                .unwrap_or_else(|| (container_port.clone(), "tcp".to_string()));

            if let Some(arr) = bindings_arr.as_array() {
                for b in arr {
                    ports.push(PortMapping {
                        host_ip:        b["HostIp"].as_str().unwrap_or("0.0.0.0").to_string(),
                        host_port:      b["HostPort"].as_str().unwrap_or("").to_string(),
                        container_port: cport.clone(),
                        protocol:       proto.clone(),
                    });
                }
            }
        }
    }
    ports
}

fn parse_networks(c: &serde_json::Value) -> Vec<NetworkEntry> {
    let mut result = Vec::new();
    if let Some(networks) = c["NetworkSettings"]["Networks"].as_object() {
        for (name, n) in networks {
            result.push(NetworkEntry {
                network_name: name.clone(),
                ip_address:   n["IPAddress"].as_str().unwrap_or("").to_string(),
                gateway:      n["Gateway"].as_str().unwrap_or("").to_string(),
                mac_address:  n["MacAddress"].as_str().unwrap_or("").to_string(),
            });
        }
    }
    result
}

fn parse_mounts(c: &serde_json::Value) -> Vec<MountInfo> {
    c["Mounts"].as_array()
        .map(|arr| arr.iter().map(|m| {
            let source = m["Source"].as_str().unwrap_or("").to_string();
            let permissions = if !source.is_empty() && std::path::Path::new(&source).exists() {
                collect_path_permissions(&source)
            } else {
                vec![]
            };
            
            MountInfo {
                mount_type:  m["Type"].as_str().unwrap_or("").to_string(),
                source,
                destination: m["Destination"].as_str().unwrap_or("").to_string(),
                mode:        m["Mode"].as_str().unwrap_or("").to_string(),
                rw:          m["RW"].as_bool().unwrap_or(false),
                permissions,
            }
        }).collect())
        .unwrap_or_default()
}

fn collect_path_permissions(path: &str) -> Vec<crate::check::container::PathPermission> {
    use std::os::unix::fs::MetadataExt;
    use std::fs;
    
    let mut permissions = Vec::new();
    
    if let Ok(metadata) = fs::metadata(path) {
        permissions.push(crate::check::container::PathPermission {
            path: path.to_string(),
            uid: metadata.uid(),
            gid: metadata.gid(),
            mode: metadata.mode(),
        });
    }
    
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                permissions.push(crate::check::container::PathPermission {
                    path: entry.path().to_string_lossy().to_string(),
                    uid: metadata.uid(),
                    gid: metadata.gid(),
                    mode: metadata.mode(),
                });
                
                if metadata.is_dir() {
                    permissions.extend(collect_path_permissions(&entry.path().to_string_lossy()));
                }
            }
        }
    }
    
    permissions
}

fn parse_resource_config(c: &serde_json::Value) -> ResourceConfig {
    let hc = &c["HostConfig"];
    ResourceConfig {
        cpu_shares:   hc["CpuShares"].as_u64().unwrap_or(0),
        cpu_period:   hc["CpuPeriod"].as_u64().unwrap_or(0),
        cpu_quota:    hc["CpuQuota"].as_i64().unwrap_or(0),
        memory_limit: hc["Memory"].as_u64().unwrap_or(0),
        memory_swap:  hc["MemorySwap"].as_i64().unwrap_or(0),
        pids_limit:   hc["PidsLimit"].as_i64().unwrap_or(0),
    }
}

fn parse_process_info(c: &serde_json::Value) -> Option<ProcessInfo> {
    let host_pid = c["State"]["Pid"].as_i64()? as i32;
    if host_pid <= 0 { return None; }

    let status_path = format!("/proc/{}/status", host_pid);
    let uid = std::fs::read_to_string(&status_path).ok()
        .and_then(|s| s.lines()
            .find(|l| l.starts_with("Uid:"))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse().ok()))
        .unwrap_or(0);

    let cmd = std::fs::read_to_string(format!("/proc/{}/cmdline", host_pid)).ok()
        .map(|s| s.replace('\0', " ").trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    Some(ProcessInfo { host_pid, uid, cmd })
}

// ── docker stats ─────────────────────────────────────────────────────────────

fn fetch_stats(id: &str) -> Option<ResourceUsage> {
    let out = Command::new("docker")
        .args(&[
            "stats", "--no-stream",
            "--format", "{{json .}}",
            id,
        ])
        .output()
        .ok()?;

    if !out.status.success() { return None; }

    let j: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;

    // docker stats json 格式：字段值为字符串，如 "1.5GiB / 3.8GiB"
    let memory_usage  = parse_stat_mem(j["MemUsage"].as_str().unwrap_or(""));
    let cpu_percent   = parse_stat_pct(j["CPUPerc"].as_str().unwrap_or(""));
    let mem_percent   = parse_stat_pct(j["MemPerc"].as_str().unwrap_or(""));
    let (net_rx, net_tx) = parse_stat_pair(j["NetIO"].as_str().unwrap_or(""));
    let (blk_r, blk_w)  = parse_stat_pair(j["BlockIO"].as_str().unwrap_or(""));
    let pids = j["PIDs"].as_str()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    Some(ResourceUsage {
        cpu_percent,
        memory_usage: memory_usage.0,
        memory_limit: memory_usage.1,
        memory_percent: mem_percent,
        block_read: blk_r,
        block_write: blk_w,
        net_rx,
        net_tx,
        pids,
    })
}

/// 解析 "1.5GiB / 3.8GiB" → (used_bytes, limit_bytes)
fn parse_stat_mem(s: &str) -> (u64, u64) {
    let parts: Vec<&str> = s.split('/').collect();
    let used  = parts.get(0).map(|v| parse_size_to_bytes(v.trim())).unwrap_or(0);
    let limit = parts.get(1).map(|v| parse_size_to_bytes(v.trim())).unwrap_or(0);
    (used, limit)
}

/// 解析 "1.5GiB" → bytes
fn parse_size_to_bytes(s: &str) -> u64 {
    let s = s.trim();
    if s == "0B" || s.is_empty() { return 0; }
    let (num_part, unit) = s.split_at(
        s.find(|c: char| c.is_alphabetic()).unwrap_or(s.len())
    );
    let num: f64 = num_part.trim().parse().unwrap_or(0.0);
    match unit.to_uppercase().trim_end_matches('B') {
        "KI" | "K" => (num * 1024.0) as u64,
        "MI" | "M" => (num * 1024.0 * 1024.0) as u64,
        "GI" | "G" => (num * 1024.0 * 1024.0 * 1024.0) as u64,
        "TI" | "T" => (num * 1024.0 * 1024.0 * 1024.0 * 1024.0) as u64,
        _ => num as u64,
    }
}

/// 解析 "1.5%" → f64
fn parse_stat_pct(s: &str) -> f64 {
    s.trim_end_matches('%').parse().unwrap_or(0.0)
}

/// 解析 "1.5MB / 2.3MB" → (left_bytes, right_bytes)
fn parse_stat_pair(s: &str) -> (u64, u64) {
    let parts: Vec<&str> = s.split('/').collect();
    let a = parts.get(0).map(|v| parse_size_to_bytes(v.trim())).unwrap_or(0);
    let b = parts.get(1).map(|v| parse_size_to_bytes(v.trim())).unwrap_or(0);
    (a, b)
}

// ── docker logs ─────────────────────────────────────────────────────────────

fn fetch_logs(id: &str, tail: &str) -> Option<Vec<String>> {
    let out = Command::new("docker")
        .args(&["logs", "--tail", tail, "--timestamps", id])
        .output()
        .ok()?;

    // docker logs 写 stderr
    let combined = [out.stdout.as_slice(), out.stderr.as_slice()].concat();
    let s = String::from_utf8_lossy(&combined);

    Some(s.lines().map(String::from).collect())
}

// ── 工具 ────────────────────────────────────────────────────────────────────

fn str_val(c: &serde_json::Value, path: &[&str]) -> String {
    let mut cur = c;
    for key in path {
        cur = &cur[key];
    }
    cur.as_str().unwrap_or("").to_string()
}
