pub mod fanotify;
pub mod process;
pub mod event;

use crate::utils::Result;

pub fn run_monitor(directory: &str, show_container: bool, format: &str) -> Result<()> {
    // 验证目录存在
    if !std::path::Path::new(directory).exists() {
        return Err(crate::utils::SedockerError::System(
            format!("Directory does not exist: {}", directory)
        ));
    }
    
    // 检查权限
    if unsafe { libc::geteuid() } != 0 {
        return Err(crate::utils::SedockerError::Permission(
            "This tool requires root privileges".to_string()
        ));
    }
    
    println!("Starting file access monitor on: {}", directory);
    println!("Press Ctrl+C to stop\n");
    
    // 启动 fanotify 监控
    fanotify::start_monitoring(directory, show_container, format)
}