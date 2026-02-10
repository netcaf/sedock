use crate::check::container::ContainerInfo;
use crate::utils::{Result, SedockerError};

pub fn display_containers(containers: &[ContainerInfo], format: &str) -> Result<()> {
    match format {
        "json" => display_json(containers),
        "text" => display_text(containers),
        _ => Err(SedockerError::System(
            format!("Unknown output format: {}", format)
        )),
    }
}

fn display_json(containers: &[ContainerInfo]) -> Result<()> {
    let json = serde_json::to_string_pretty(containers)
        .map_err(|e| SedockerError::System(format!("JSON serialization failed: {}", e)))?;
    println!("{}", json);
    Ok(())
}

fn display_text(containers: &[ContainerInfo]) -> Result<()> {
    for container in containers {
        println!("Container: {}", container.id);
        println!("  Name:   {}", container.name);
        println!("  Image:  {}", container.image);
        println!("  Status: {}", container.status);
        println!("  Created: {}", container.created);
        
        if !container.ports.is_empty() {
            println!("  Ports:");
            for port in &container.ports {
                println!("    {}:{} -> {}", 
                         port.host_port, port.protocol, port.container_port);
            }
        }
        
        if !container.mounts.is_empty() {
            println!("  Mounts:");
            for mount in &container.mounts {
                println!("    {} -> {} [{}{}]",
                         mount.source,
                         mount.destination,
                         mount.mode,
                         if mount.rw { ", rw" } else { ", ro" });
            }
        }
        
        println!("  Network:");
        println!("    IP:      {}", container.network.ip_address);
        println!("    Gateway: {}", container.network.gateway);
        println!("    Mode:    {}", container.network.network_mode);
        
        if let Some(ref proc) = container.process_info {
            println!("  Process:");
            println!("    PID: {}", proc.pid);
            println!("    UID: {}", proc.uid);
            println!("    CMD: {}", proc.cmd);
        }
        
        println!();
    }
    
    Ok(())
}