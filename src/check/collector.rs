use crate::check::container::*;
use crate::utils::{Result, SedockerError};
use std::process::Command;

pub fn collect_all_containers(verbose: bool) -> Result<Vec<ContainerInfo>> {
    // 获取所有容器 ID
    let output = Command::new("docker")
        .args(&["ps", "-a", "--format", "{{.ID}}"])
        .output()
        .map_err(|e| SedockerError::Docker(format!("Failed to run docker command: {}", e)))?;
    
    if !output.status.success() {
        return Err(SedockerError::Docker(
            "Docker command failed. Is Docker installed?".to_string()
        ));
    }
    
    let container_ids = String::from_utf8_lossy(&output.stdout);
    let mut containers = Vec::new();
    
    for id in container_ids.lines() {
        let id = id.trim();
        if !id.is_empty() {
            match collect_container_info(id, verbose) {
                Ok(info) => containers.push(info),
                Err(e) => eprintln!("Warning: Failed to collect info for {}: {}", id, e),
            }
        }
    }
    
    Ok(containers)
}

pub fn collect_container_info(container_id: &str, verbose: bool) -> Result<ContainerInfo> {
    // 使用 docker inspect 获取详细信息
    let output = Command::new("docker")
        .args(&["inspect", container_id])
        .output()
        .map_err(|e| SedockerError::Docker(format!("Failed to inspect container: {}", e)))?;
    
    if !output.status.success() {
        return Err(SedockerError::Docker(
            format!("Container {} not found", container_id)
        ));
    }
    
    let json_str = String::from_utf8_lossy(&output.stdout);
    parse_container_json(&json_str, verbose)
}

fn parse_container_json(json_str: &str, verbose: bool) -> Result<ContainerInfo> {
    let value: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| SedockerError::Parse(format!("Failed to parse JSON: {}", e)))?;
    
    let container = value.as_array()
        .and_then(|arr| arr.first())
        .ok_or_else(|| SedockerError::Parse("Empty JSON array".to_string()))?;
    
    // 提取基本信息
    let id = container["Id"]
        .as_str()
        .unwrap_or("unknown")
        .chars()
        .take(12)
        .collect();
    
    let name = container["Name"]
        .as_str()
        .unwrap_or("unknown")
        .trim_start_matches('/')
        .to_string();
    
    let image = container["Config"]["Image"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();
    
    let status = container["State"]["Status"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();
    
    let created = container["Created"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();
    
    // 提取端口映射
    let ports = extract_ports(container);
    
    // 提取挂载信息
    let mounts = extract_mounts(container);
    
    // 提取网络信息
    let network = extract_network(container);
    
    // 提取资源信息
    let resources = extract_resources(container);
    
    // 获取进程信息（如果 verbose）
    let process_info = if verbose {
        extract_process_info(container)
    } else {
        None
    };
    
    Ok(ContainerInfo {
        id,
        name,
        image,
        status,
        created,
        ports,
        mounts,
        network,
        resources,
        process_info,
    })
}

fn extract_ports(container: &serde_json::Value) -> Vec<PortMapping> {
    let mut ports = Vec::new();
    
    if let Some(port_bindings) = container["HostConfig"]["PortBindings"].as_object() {
        for (container_port, bindings) in port_bindings {
            if let Some(bindings_arr) = bindings.as_array() {
                for binding in bindings_arr {
                    ports.push(PortMapping {
                        host_port: binding["HostPort"]
                            .as_str()
                            .unwrap_or("0")
                            .to_string(),
                        container_port: container_port.to_string(),
                        protocol: if container_port.contains("/tcp") {
                            "tcp".to_string()
                        } else {
                            "udp".to_string()
                        },
                    });
                }
            }
        }
    }
    
    ports
}

fn extract_mounts(container: &serde_json::Value) -> Vec<MountInfo> {
    let mut mounts = Vec::new();
    
    if let Some(mounts_arr) = container["Mounts"].as_array() {
        for mount in mounts_arr {
            mounts.push(MountInfo {
                source: mount["Source"].as_str().unwrap_or("").to_string(),
                destination: mount["Destination"].as_str().unwrap_or("").to_string(),
                mode: mount["Mode"].as_str().unwrap_or("").to_string(),
                rw: mount["RW"].as_bool().unwrap_or(false),
            });
        }
    }
    
    mounts
}

fn extract_network(container: &serde_json::Value) -> NetworkInfo {
    let networks = &container["NetworkSettings"]["Networks"];
    
    // 获取第一个网络的信息
    let first_network = networks.as_object()
        .and_then(|obj| obj.values().next());
    
    NetworkInfo {
        ip_address: first_network
            .and_then(|n| n["IPAddress"].as_str())
            .unwrap_or("")
            .to_string(),
        gateway: first_network
            .and_then(|n| n["Gateway"].as_str())
            .unwrap_or("")
            .to_string(),
        mac_address: first_network
            .and_then(|n| n["MacAddress"].as_str())
            .unwrap_or("")
            .to_string(),
        network_mode: container["HostConfig"]["NetworkMode"]
            .as_str()
            .unwrap_or("default")
            .to_string(),
    }
}

fn extract_resources(container: &serde_json::Value) -> ResourceInfo {
    ResourceInfo {
        cpu_shares: container["HostConfig"]["CpuShares"]
            .as_u64()
            .unwrap_or(0),
        memory_limit: container["HostConfig"]["Memory"]
            .as_u64()
            .unwrap_or(0),
        memory_usage: None, // 需要额外的 stats 调用
    }
}

fn extract_process_info(container: &serde_json::Value) -> Option<ProcessInfo> {
    let pid = container["State"]["Pid"].as_i64()? as i32;
    
    if pid <= 0 {
        return None;
    }
    
    // 从 /proc 读取信息
    let status_path = format!("/proc/{}/status", pid);
    let uid = std::fs::read_to_string(&status_path)
        .ok()
        .and_then(|content| {
            content.lines()
                .find(|line| line.starts_with("Uid:"))
                .and_then(|line| line.split_whitespace().nth(1))
                .and_then(|s| s.parse().ok())
        })
        .unwrap_or(0);
    
    let cmdline_path = format!("/proc/{}/cmdline", pid);
    let cmd = std::fs::read_to_string(&cmdline_path)
        .ok()
        .map(|s| s.replace('\0', " "))
        .unwrap_or_else(|| "unknown".to_string());
    
    Some(ProcessInfo { pid, uid, cmd })
}