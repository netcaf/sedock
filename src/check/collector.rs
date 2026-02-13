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
        // 根据 verbose 模式决定日志行数
        let log_lines = if verbose { "all" } else { "10" };
        info.log_tail       = fetch_logs(id, log_lines);
    } else {
        // exited 容器也拿日志，有助于排障
        let log_lines = if verbose { "all" } else { "10" };
        info.log_tail = fetch_logs(id, log_lines);
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
    let id: String = c["Id"].as_str().unwrap_or("").chars().take(12).collect();
    let name = c["Name"].as_str().unwrap_or("")
        .trim_start_matches('/').to_string();
    let image    = str_val(c, &["Config", "Image"]);
    let image_id = c["Image"].as_str().unwrap_or("").to_string();
    let cmd = c["Config"]["Cmd"].as_array()
        .map(|a| a.iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.to_string())
            .collect::<Vec<String>>()
            .join(" "))
        .unwrap_or_default();
    let entrypoint = c["Config"]["Entrypoint"].as_array()
        .map(|a| a.iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.to_string())
            .collect::<Vec<String>>()
            .join(" "))
        .unwrap_or_default();
    let working_dir = str_val(c, &["Config", "WorkingDir"]);
    let user = str_val(c, &["Config", "User"]);

    let status      = str_val(c, &["State", "Status"]);
    let exit_code   = c["State"]["ExitCode"].as_i64().unwrap_or(0);
    let oom_killed  = c["State"]["OOMKilled"].as_bool().unwrap_or(false);
    let created     = str_val(c, &["Created"]);
    let started_at  = str_val(c, &["State", "StartedAt"]);
    let finished_at = str_val(c, &["State", "FinishedAt"]);

    let restart_policy = str_val(c, &["HostConfig", "RestartPolicy", "Name"]);
    let restart_count  = c["RestartCount"].as_i64().unwrap_or(0);

    let env = c["Config"]["Env"].as_array()
        .map(|a| a.iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.to_string())
            .collect())
        .unwrap_or_default();

    let ports        = parse_ports(c);
    let networks     = parse_networks(c);
    let network_mode = str_val(c, &["HostConfig", "NetworkMode"]);
    let mounts       = parse_mounts(c);
    let resource_config = parse_resource_config(c);
    let security_config = parse_security_config(c);
    let processes = parse_process_info(c).unwrap_or_default();

    // Collect users and groups from container (always, for normal mode display)
    let users_groups = collect_users_groups(id.as_str()).unwrap_or_default();

    Ok(ContainerInfo {
        id, name, image, image_id,
        status, exit_code, oom_killed,
        created, started_at, finished_at,
        restart_policy, restart_count, env,
        cmd, entrypoint, working_dir, user,
        security: security_config,
        ports, networks, network_mode, mounts,
        resource_config,
        resource_usage: None,
        log_tail: None,
        processes,
        users_groups,
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

fn parse_process_info(c: &serde_json::Value) -> Option<Vec<ProcessInfo>> {
    let host_pid = c["State"]["Pid"].as_i64()? as i32;
    if host_pid <= 0 { return None; }

    // Get container ID from inspect JSON
    let container_id = c["Id"].as_str()?;
    let short_id = container_id.chars().take(12).collect::<String>();
    
    // Use docker top to get all processes in the container
    let mut processes = collect_container_processes(&short_id)?;
    
    // Try to identify the main process (PID 1 in container)
    // We can check if any process has PPID = 0 (orphaned) or is the entrypoint/cmd
    if let Some(main_pid) = get_container_main_pid(&short_id, host_pid) {
        for process in &mut processes {
            if process.pid == main_pid {
                // Mark this as the main process
                // We'll add a flag or special handling in display
            }
        }
    }
    
    Some(processes)
}

fn get_container_main_pid(_container_id: &str, host_pid: i32) -> Option<i32> {
    // The main container process is the one with PID 1 in the container namespace
    // We can try to get this from /proc/<host_pid>/status which shows NSpid
    let status_path = format!("/proc/{}/status", host_pid);
    if let Ok(content) = std::fs::read_to_string(&status_path) {
        for line in content.lines() {
            if line.starts_with("NSpid:") {
                // NSpid shows PID in different namespaces
                // Format: NSpid:  <host_pid>    <container_pid> ...
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    if let Ok(container_ns_pid) = parts[2].parse::<i32>() {
                        if container_ns_pid == 1 {
                            return Some(host_pid);
                        }
                    }
                }
            }
        }
    }
    
    // Fallback: the process with PPID = 0 (init process) is often the main one
    None
}

fn collect_container_processes(container_id: &str) -> Option<Vec<ProcessInfo>> {
    use std::process::Command;
    
    // Run docker top to get PIDs and commands
    let output = Command::new("docker")
        .args(&["top", container_id, "-eo", "pid,ppid,cmd"])
        .output()
        .ok()?;
    
    if !output.status.success() {
        return None;
    }
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    
    // Skip header line
    if lines.len() < 2 {
        return Some(Vec::new());
    }
    
    let mut processes = Vec::new();
    
    for line in lines.iter().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }
        
        let pid = parts[0].parse().unwrap_or(0);
        let ppid = parts[1].parse().unwrap_or(0);
        
        // cmd might contain spaces, so join remaining parts
        let cmd = parts[2..].join(" ");
        
        // Get uid/gid from /proc
        let (uid, gid) = get_process_uid_gid(pid);
        
        // Get user and group names from container filesystem
        let (user, group) = get_container_user_group(container_id, uid, gid);
        
        // Try to get executable path from /proc
        let exe_path = get_process_exe_path(pid);
        let cwd = get_process_cwd(pid);
        
        processes.push(ProcessInfo {
            pid,
            ppid,
            uid,
            gid,
            user,
            group,
            cmd,
            exe_path,
            cwd,
        });
    }
    
    Some(processes)
}

fn get_container_user_group(container_id: &str, uid: u32, gid: u32) -> (String, String) {
    use std::process::Command;
    
    // Try to get user name from container's /etc/passwd
    let user_output = Command::new("docker")
        .args(&["exec", container_id, "getent", "passwd", &uid.to_string()])
        .output();
    
    let user = match user_output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout)
                .split(':')
                .nth(0)
                .unwrap_or(&uid.to_string())
                .to_string()
        }
        _ => uid.to_string(),
    };
    
    // Try to get group name from container's /etc/group
    let group_output = Command::new("docker")
        .args(&["exec", container_id, "getent", "group", &gid.to_string()])
        .output();
    
    let group = match group_output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout)
                .split(':')
                .nth(0)
                .unwrap_or(&gid.to_string())
                .to_string()
        }
        _ => gid.to_string(),
    };
    
    (user, group)
}

fn get_process_uid_gid(pid: i32) -> (u32, u32) {
    if pid <= 0 {
        return (0, 0);
    }
    
    let status_path = format!("/proc/{}/status", pid);
    if let Ok(content) = std::fs::read_to_string(&status_path) {
        let mut uid = 0;
        let mut gid = 0;
        
        for line in content.lines() {
            if line.starts_with("Uid:") {
                if let Some(uid_str) = line.split_whitespace().nth(1) {
                    uid = uid_str.parse().unwrap_or(0);
                }
            } else if line.starts_with("Gid:") {
                if let Some(gid_str) = line.split_whitespace().nth(1) {
                    gid = gid_str.parse().unwrap_or(0);
                }
            }
        }
        
        return (uid, gid);
    }
    
    (0, 0)
}

fn get_process_exe_path(pid: i32) -> Option<String> {
    if pid <= 0 {
        return None;
    }
    
    let exe_path = format!("/proc/{}/exe", pid);
    match std::fs::read_link(&exe_path) {
        Ok(path) => Some(path.to_string_lossy().to_string()),
        Err(_) => None,
    }
}

fn get_process_cwd(pid: i32) -> Option<String> {
    if pid <= 0 {
        return None;
    }
    
    let cwd_path = format!("/proc/{}/cwd", pid);
    match std::fs::read_link(&cwd_path) {
        Ok(path) => Some(path.to_string_lossy().to_string()),
        Err(_) => None,
    }
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
    let out = if tail == "all" {
        Command::new("docker")
            .args(&["logs", "--timestamps", id])
            .output()
            .ok()?
    } else {
        Command::new("docker")
            .args(&["logs", "--tail", tail, "--timestamps", id])
            .output()
            .ok()?
    };

    // docker logs 写 stderr
    let combined = [out.stdout.as_slice(), out.stderr.as_slice()].concat();
    let s = String::from_utf8_lossy(&combined);

    Some(s.lines().map(String::from).collect())
}

// ── 安全配置解析 ─────────────────────────────────────────────────────────────

fn parse_security_config(c: &serde_json::Value) -> SecurityConfig {
    let hc = &c["HostConfig"];
    
    // 解析 capabilities
    let capabilities = hc["CapAdd"].as_array()
        .map(|arr| arr.iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.to_string())
            .collect())
        .unwrap_or_default();
    
    // 解析 seccomp 和 apparmor 配置
    let seccomp_profile = hc["SecurityOpt"].as_array()
        .and_then(|opts| {
            opts.iter()
                .filter_map(|v| v.as_str())
                .find(|s| s.starts_with("seccomp="))
                .map(|s| s.trim_start_matches("seccomp=").to_string())
        })
        .unwrap_or_default();
    
    let apparmor_profile = hc["SecurityOpt"].as_array()
        .and_then(|opts| {
            opts.iter()
                .filter_map(|v| v.as_str())
                .find(|s| s.starts_with("apparmor="))
                .map(|s| s.trim_start_matches("apparmor=").to_string())
        })
        .unwrap_or_default();
    
    SecurityConfig {
        privileged: hc["Privileged"].as_bool().unwrap_or(false),
        capabilities,
        seccomp_profile,
        apparmor_profile,
        read_only_rootfs: hc["ReadonlyRootfs"].as_bool().unwrap_or(false),
        no_new_privileges: hc["NoNewPrivileges"].as_bool().unwrap_or(false),
    }
}

// ── 用户和组收集 ─────────────────────────────────────────────────────────────

fn collect_users_groups(container_id: &str) -> Result<Vec<UserGroupInfo>> {
    use std::process::Command;
    
    // 获取容器内的所有用户
    let users_output = Command::new("docker")
        .args(&["exec", container_id, "getent", "passwd"])
        .output()
        .map_err(|e| SedockerError::Docker(format!("Failed to get users: {}", e)))?;
    
    if !users_output.status.success() {
        return Ok(vec![]); // 容器可能没有 getent 或已停止
    }
    
    let users_content = String::from_utf8_lossy(&users_output.stdout);
    let mut users_groups = Vec::new();
    
    // 解析 /etc/passwd 格式: username:password:uid:gid:gecos:home:shell
    for line in users_content.lines() {
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() >= 7 {
            let username = parts[0].to_string();
            let user_id = parts[2].parse().unwrap_or(0);
            let group_id = parts[3].parse().unwrap_or(0);
            let home_dir = if !parts[5].is_empty() { Some(parts[5].to_string()) } else { None };
            let shell = if !parts[6].is_empty() { Some(parts[6].to_string()) } else { None };
            
            // 获取组名
            let group_name = get_group_name(container_id, group_id).unwrap_or_else(|| group_id.to_string());
            
            users_groups.push(UserGroupInfo {
                username,
                user_id,
                group_name,
                group_id,
                home_dir,
                shell,
            });
        }
    }
    
    Ok(users_groups)
}

fn get_group_name(container_id: &str, gid: u32) -> Option<String> {
    use std::process::Command;
    
    let output = Command::new("docker")
        .args(&["exec", container_id, "getent", "group", &gid.to_string()])
        .output()
        .ok()?;
    
    if !output.status.success() {
        return None;
    }
    
    let content = String::from_utf8_lossy(&output.stdout);
    content.split(':').next().map(|s| s.to_string())
}

// ── 工具 ────────────────────────────────────────────────────────────────────

fn str_val(c: &serde_json::Value, path: &[&str]) -> String {
    let mut cur = c;
    for key in path {
        cur = &cur[key];
    }
    cur.as_str().unwrap_or("").to_string()
}
