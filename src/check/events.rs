//! Docker 事件收集
//! 来源：docker events --since <duration>

use serde::{Deserialize, Serialize};
use std::process::Command;

const DEFAULT_SINCE: &str = "24h";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerEvent {
    pub timestamp: String,
    pub event_type: String,   // container / network / volume / image
    pub action: String,       // start / stop / die / kill / oom / ...
    pub actor_id: String,     // short container id or name
    pub actor_name: String,
    pub attributes: std::collections::HashMap<String, String>,
}

pub fn collect(since: &str) -> Vec<DockerEvent> {
    let out = match Command::new("docker")
        .args(&[
            "events",
            "--since", since,
            "--until", "0s",
            "--format", "{{json .}}",
        ])
        .output()
    {
        Ok(o) if o.status.success() => o,
        Ok(o) => {
            eprintln!("warn: docker events: {}", String::from_utf8_lossy(&o.stderr));
            return vec![];
        }
        Err(e) => {
            eprintln!("warn: docker events failed: {}", e);
            return vec![];
        }
    };

    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| parse_event_line(line))
        .collect()
}

pub fn collect_with_limit(since: &str, limit: usize) -> Vec<DockerEvent> {
    let out = match Command::new("docker")
        .args(&[
            "events",
            "--since", since,
            "--until", "0s",
            "--format", "{{json .}}",
        ])
        .output()
    {
        Ok(o) if o.status.success() => o,
        Ok(o) => {
            eprintln!("warn: docker events: {}", String::from_utf8_lossy(&o.stderr));
            return vec![];
        }
        Err(e) => {
            eprintln!("warn: docker events failed: {}", e);
            return vec![];
        }
    };

    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| parse_event_line(line))
        .take(limit)
        .collect()
}

fn parse_event_line(line: &str) -> Option<DockerEvent> {
    let j: serde_json::Value = serde_json::from_str(line).ok()?;

    // timestamp: unix nano → human readable
    let ts = j["time"].as_u64()
        .map(|t| {
            use std::time::{Duration, UNIX_EPOCH};
            let d = UNIX_EPOCH + Duration::from_secs(t);
            chrono::DateTime::<chrono::Local>::from(d)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_else(|| j["timeNano"].as_str().unwrap_or("").to_string());

    let event_type = j["Type"].as_str().unwrap_or("").to_string();
    let action     = j["Action"].as_str().unwrap_or("").to_string();
    let actor_id   = j["Actor"]["ID"].as_str()
        .unwrap_or("")
        .chars().take(12).collect::<String>();

    let attributes: std::collections::HashMap<String, String> = j["Actor"]["Attributes"]
        .as_object()
        .map(|obj| obj.iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
            .collect())
        .unwrap_or_default();

    let actor_name = attributes.get("name")
        .cloned()
        .unwrap_or_else(|| actor_id.clone());

    Some(DockerEvent {
        timestamp: ts,
        event_type,
        action,
        actor_id,
        actor_name,
        attributes,
    })
}

pub fn default_since() -> &'static str {
    DEFAULT_SINCE
}