use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
    pub created: String,
    pub ports: Vec<PortMapping>,
    pub mounts: Vec<MountInfo>,
    pub network: NetworkInfo,
    pub resources: ResourceInfo,
    pub process_info: Option<ProcessInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMapping {
    pub host_port: String,
    pub container_port: String,
    pub protocol: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountInfo {
    pub source: String,
    pub destination: String,
    pub mode: String,
    pub rw: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInfo {
    pub ip_address: String,
    pub gateway: String,
    pub mac_address: String,
    pub network_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceInfo {
    pub cpu_shares: u64,
    pub memory_limit: u64,
    pub memory_usage: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: i32,
    pub uid: u32,
    pub cmd: String,
}