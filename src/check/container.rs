use serde::{Deserialize, Serialize};

// ── 顶层容器信息 ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerInfo {
    // 基本标识
    pub id: String,
    pub name: String,
    pub image: String,
    pub image_id: String,

    // 状态
    pub status: String,
    pub exit_code: i64,
    pub oom_killed: bool,
    pub created: String,
    pub started_at: String,
    pub finished_at: String,

    // 配置
    pub restart_policy: String,
    pub restart_count: i64,
    pub env: Vec<String>,         // verbose 下才填充
    pub cmd: String,
    pub entrypoint: String,
    pub working_dir: String,
    pub user: String,

    // 安全配置
    pub security: SecurityConfig,

    // 网络
    pub ports: Vec<PortMapping>,
    pub networks: Vec<NetworkEntry>,
    pub network_mode: String,

    // 存储
    pub mounts: Vec<MountInfo>,

    // 资源配置（来自 inspect）
    pub resource_config: ResourceConfig,

    // 资源使用（来自 docker stats，仅 running 容器）
    pub resource_usage: Option<ResourceUsage>,

    // 日志 tail
    pub log_tail: Option<Vec<String>>,

    // 进程信息（verbose，来自 docker top）
    pub processes: Vec<ProcessInfo>,

    // 用户和组信息
    pub users_groups: Vec<UserGroupInfo>,
}

// ── 网络 ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMapping {
    pub host_ip: String,
    pub host_port: String,
    pub container_port: String,
    pub protocol: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkEntry {
    pub network_name: String,
    pub ip_address: String,
    pub gateway: String,
    pub mac_address: String,
}

// ── 存储 ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountInfo {
    pub mount_type: String,   // bind / volume / tmpfs
    pub source: String,
    pub destination: String,
    pub mode: String,
    pub rw: bool,
    pub permissions: Vec<PathPermission>,  // uid/gid for all files under mount
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathPermission {
    pub path: String,
    pub uid: u32,
    pub gid: u32,
    pub mode: u32,
}

// ── 资源 ────────────────────────────────────────────────────────────────────

/// 来自 inspect HostConfig（静态配置）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceConfig {
    pub cpu_shares: u64,
    pub cpu_period: u64,
    pub cpu_quota: i64,    // -1 = unlimited
    pub memory_limit: u64, // 0 = unlimited
    pub memory_swap: i64,  // -1 = unlimited
    pub pids_limit: i64,   // 0 = unlimited
}

/// 来自 docker stats（运行时实际用量）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub cpu_percent: f64,
    pub memory_usage: u64,
    pub memory_limit: u64,
    pub memory_percent: f64,
    pub block_read: u64,
    pub block_write: u64,
    pub net_rx: u64,
    pub net_tx: u64,
    pub pids: u64,
}

// ── 安全配置 ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub privileged: bool,
    pub capabilities: Vec<String>,
    pub seccomp_profile: String,
    pub apparmor_profile: String,
    pub read_only_rootfs: bool,
    pub no_new_privileges: bool,
}

// ── 用户和组信息 ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserGroupInfo {
    pub username: String,
    pub user_id: u32,
    pub group_name: String,
    pub group_id: u32,
    pub home_dir: Option<String>,
    pub shell: Option<String>,
}

// ── 进程 ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: i32,
    pub ppid: i32,
    pub uid: u32,
    pub gid: u32,
    pub user: String,
    pub group: String,
    pub cmd: String,
    pub exe_path: Option<String>,
    pub cwd: Option<String>,
}
