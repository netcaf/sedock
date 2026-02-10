//! 输出层：接收 CheckReport，渲染 text 或 json

use crate::check::report::CheckReport;
use crate::check::container::ContainerInfo;
use crate::utils::{Result, SedockerError};

pub fn display(report: &CheckReport, format: &str, verbose: bool) -> Result<()> {
    match format {
        "json" => display_json(report),
        "text" => display_text(report, verbose),
        other  => Err(SedockerError::System(format!("unknown format: {}", other))),
    }
}

// ── JSON ────────────────────────────────────────────────────────────────────

fn display_json(report: &CheckReport) -> Result<()> {
    let json = serde_json::to_string_pretty(report)
        .map_err(|e| SedockerError::System(format!("JSON serialize: {}", e)))?;
    println!("{}", json);
    Ok(())
}

// ── Text ────────────────────────────────────────────────────────────────────

fn display_text(report: &CheckReport, verbose: bool) -> Result<()> {
    print_section("REPORT");
    println!("  Collected at : {}", report.collected_at);

    // ── Host ──────────────────────────────────────────────────────────────
    print_section("HOST");
    let h = &report.host;
    println!("  Hostname     : {}", h.os.hostname);
    println!("  OS           : {}", h.os.os_release);
    println!("  Kernel       : {}", h.os.kernel);
    println!("  Arch         : {}", h.os.arch);
    println!("  Uptime       : {}", format_uptime(h.os.uptime_seconds));

    println!("  CPU          : {} ({} cores)", h.cpu.model, h.cpu.logical_cores);
    println!("  Load avg     : {:.2}  {:.2}  {:.2}  (1/5/15 min)",
        h.cpu.load_avg_1, h.cpu.load_avg_5, h.cpu.load_avg_15);

    let m = &h.memory;
    println!("  Memory       : {} used / {} total  ({:.1}%)",
        fmt_kb(m.used_kb), fmt_kb(m.total_kb), m.used_percent);
    if m.swap_total_kb > 0 {
        println!("  Swap         : {} used / {}", fmt_kb(m.swap_used_kb), fmt_kb(m.swap_total_kb));
    } else {
        println!("  Swap         : disabled");
    }

    if !h.disk.is_empty() {
        println!("  Disk:");
        for d in &h.disk {
            let warn = if d.used_percent > 85.0 || d.inode_used_percent > 85.0 { " ⚠" } else { "" };
            println!("    {:<20} {:<12}  {:.1}% used  inode {:.1}%{}",
                d.mount, d.filesystem, d.used_percent, d.inode_used_percent, warn);
        }
    }

    println!("  cgroup       : {}", h.cgroup_version);
    println!("  SELinux      : {}", h.security.selinux);
    println!("  AppArmor     : {}", h.security.apparmor);
    println!("  Time         : {}  NTP synced: {}", h.time.system_time,
        if h.time.ntp_synced { "yes" } else { "no ⚠" });

    // ── Engine ────────────────────────────────────────────────────────────
    print_section("DOCKER ENGINE");
    let e = &report.engine;
    println!("  Version      : {}", e.version.server_version);
    println!("  API version  : {}", e.version.api_version);
    println!("  Go version   : {}", e.version.go_version);
    println!("  OS/Arch      : {}", e.version.os_arch);
    println!("  Build time   : {}", e.version.build_time);
    println!("  Storage drv  : {}", e.runtime.storage_driver);
    println!("  cgroup drv   : {}", e.runtime.cgroup_driver);
    println!("  cgroup ver   : {}", e.runtime.cgroup_version);
    println!("  Log driver   : {}", e.runtime.log_driver);
    println!("  Root dir     : {}", e.runtime.root_dir);
    println!("  Containers   : {} total  {} running  {} paused  {} stopped",
        e.runtime.total_containers, e.runtime.running_containers,
        e.runtime.paused_containers, e.runtime.stopped_containers);
    println!("  Images       : {}", e.runtime.total_images);

    // kernel capability warnings
    if !e.runtime.memory_limit {
        println!("  ⚠  memory limit support not available in kernel");
    }
    if !e.runtime.swap_limit {
        println!("  ⚠  swap limit support not available in kernel");
    }

    println!("  daemon.json  : {}", e.daemon_config.config_file);
    if verbose && !e.daemon_logs.is_empty() {
        println!("  Daemon logs (recent warnings):");
        for line in &e.daemon_logs {
            println!("    {}", line);
        }
    }

    // ── Containers ────────────────────────────────────────────────────────
    print_section(&format!("CONTAINERS ({})", report.containers.len()));
    for c in &report.containers {
        display_container_text(c, verbose);
    }

    // ── Events ────────────────────────────────────────────────────────────
    if !report.events.is_empty() {
        print_section(&format!("RECENT EVENTS ({})", report.events.len()));
        for ev in &report.events {
            println!("  {}  [{:<12}] {:<10} {}",
                ev.timestamp, ev.actor_name, ev.event_type, ev.action);
        }
    }

    Ok(())
}

fn display_container_text(c: &ContainerInfo, verbose: bool) {
    let status_icon = match c.status.as_str() {
        "running" => "●",
        "exited"  => "○",
        "paused"  => "⏸",
        _         => "?",
    };
    let exit_info = if c.status != "running" {
        format!("  exit={}{}", c.exit_code,
            if c.oom_killed { "  ⚠ OOM-killed" } else { "" })
    } else {
        String::new()
    };

    println!("  {} {} [{}]{}",
        status_icon, c.name, c.status, exit_info);
    println!("      ID         : {}", c.id);
    println!("      Image      : {}  ({})", c.image, c.image_id);
    println!("      Created    : {}", c.created);
    println!("      Started    : {}", c.started_at);
    if c.status != "running" {
        println!("      Finished   : {}", c.finished_at);
    }
    println!("      Restart    : {}  (count: {})", c.restart_policy, c.restart_count);
    if c.privileged {
        println!("      ⚠ Privileged mode");
    }

    if !c.ports.is_empty() {
        println!("      Ports:");
        for p in &c.ports {
            println!("        {}:{} -> {}/{}", p.host_ip, p.host_port, p.container_port, p.protocol);
        }
    }

    if !c.networks.is_empty() {
        println!("      Networks:");
        for n in &c.networks {
            println!("        {} — IP: {}  GW: {}  MAC: {}",
                n.network_name, n.ip_address, n.gateway, n.mac_address);
        }
    }
    println!("      Net mode   : {}", c.network_mode);

    if !c.mounts.is_empty() {
        println!("      Mounts:");
        for m in &c.mounts {
            println!("        [{}] {} → {}  {} {}",
                m.mount_type, m.source, m.destination, m.mode,
                if m.rw { "rw" } else { "ro" });
            
            if !m.permissions.is_empty() {
                println!("          Permissions (uid:gid mode):");
                for p in &m.permissions {
                    println!("            {:o} {}:{} {}",
                        p.mode & 0o7777, p.uid, p.gid, p.path);
                }
                println!("          ⚠ Please review permissions for security implications");
            }
        }
    }

    // 资源配置
    let rc = &c.resource_config;
    let mem_lim = if rc.memory_limit == 0 {
        "unlimited".to_string()
    } else {
        fmt_bytes(rc.memory_limit)
    };
    println!("      Res config : cpu_shares={}  cpu_quota={}  mem_limit={}  pids={}",
        rc.cpu_shares, rc.cpu_quota, mem_lim, rc.pids_limit);

    // 资源用量（stats）
    if let Some(u) = &c.resource_usage {
        println!("      Res usage  : CPU {:.2}%  MEM {} / {} ({:.1}%)  PIDs {}",
            u.cpu_percent,
            fmt_bytes(u.memory_usage), fmt_bytes(u.memory_limit),
            u.memory_percent, u.pids);
        println!("                   Net rx={} tx={}  Blk r={} w={}",
            fmt_bytes(u.net_rx), fmt_bytes(u.net_tx),
            fmt_bytes(u.block_read), fmt_bytes(u.block_write));
    }

    // verbose: env
    if verbose && !c.env.is_empty() {
        println!("      Env:");
        for e in &c.env {
            println!("        {}", e);
        }
    }

    // verbose: 进程
    if !c.processes.is_empty() {
        println!("      Processes  :");
        for p in &c.processes {
            let exe_info = p.exe_path.as_ref()
                .map(|path| format!(" → {}", path))
                .unwrap_or_default();
            let cwd_info = p.cwd.as_ref()
                .map(|cwd| format!(" (cwd: {})", cwd))
                .unwrap_or_default();
            
            println!("        PID {} (PPID {})  {}:{}  {}{}{}",
                p.pid, p.ppid, p.uid, p.gid, p.cmd, exe_info, cwd_info);
        }
    }

    // 日志 tail
    if let Some(logs) = &c.log_tail {
        if !logs.is_empty() {
            println!("      Logs (last {}):", logs.len());
            for line in logs {
                println!("        {}", line);
            }
        }
    }

    println!();
}

// ── 格式化工具 ───────────────────────────────────────────────────────────────

fn print_section(title: &str) {
    println!("\n{}", "─".repeat(60));
    println!("  {}", title);
    println!("{}", "─".repeat(60));
}

fn fmt_kb(kb: u64) -> String {
    if kb >= 1024 * 1024 {
        format!("{:.1}GiB", kb as f64 / 1024.0 / 1024.0)
    } else if kb >= 1024 {
        format!("{:.1}MiB", kb as f64 / 1024.0)
    } else {
        format!("{}KiB", kb)
    }
}

fn fmt_bytes(b: u64) -> String {
    if b >= 1 << 30 {
        format!("{:.1}GiB", b as f64 / (1u64 << 30) as f64)
    } else if b >= 1 << 20 {
        format!("{:.1}MiB", b as f64 / (1u64 << 20) as f64)
    } else if b >= 1 << 10 {
        format!("{:.1}KiB", b as f64 / (1u64 << 10) as f64)
    } else {
        format!("{}B", b)
    }
}

fn format_uptime(seconds: u64) -> String {
    let d = seconds / 86400;
    let h = (seconds % 86400) / 3600;
    let m = (seconds % 3600) / 60;
    if d > 0 {
        format!("{}d {}h {}m", d, h, m)
    } else if h > 0 {
        format!("{}h {}m", h, m)
    } else {
        format!("{}m", m)
    }
}
