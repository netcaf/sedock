//! 顶层报告结构体

use serde::{Deserialize, Serialize};
use crate::check::container::ContainerInfo;
use crate::check::engine::EngineInfo;
use crate::check::events::DockerEvent;
use crate::check::host::HostInfo;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckReport {
    pub collected_at: String,
    pub host: HostInfo,
    pub engine: EngineInfo,
    pub containers: Vec<ContainerInfo>,
    pub events: Vec<DockerEvent>,
}
