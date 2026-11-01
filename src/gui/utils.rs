use crate::gui::STATE_PATH;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn url_decode(s: &str) -> String {
    let mut result = String::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = u8::from_str_radix(&s[i+1..i+3], 16) {
                result.push(hex as char);
                i += 3;
                continue;
            }
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    result
}

pub fn read_state() -> (String, String, u64) {
    match std::fs::read_to_string(STATE_PATH) {
        Ok(content) => {
            let v: serde_yaml::Value = serde_yaml::from_str(&content).unwrap_or_default();
            let state = v.get("state").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
            let detail = v.get("detail").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let time = v.get("time").and_then(|v| v.as_u64()).unwrap_or(0);
            (state, detail, time)
        }
        Err(_) => ("unknown".to_string(), String::new(), 0),
    }
}

pub fn state_color(state: &str) -> &'static str {
    match state {
        "backing_up" => "#e65100",
        "waiting_for_device" => "#1565c0",
        "waiting_for_removal" => "#2e7d32",
        "waiting_for_config" => "#757575",
        "backup_complete" => "#2e7d32",
        "error" => "#d32f2f",
        _ => "#424242",
    }
}

pub fn state_icon(state: &str) -> &'static str {
    match state {
        "backing_up" => "⏳",
        "waiting_for_device" => "💽",
        "waiting_for_removal" => "✅",
        "waiting_for_config" => "⚙️",
        "backup_complete" => "🎉",
        "error" => "⚠️",
        _ => "❓",
    }
}

pub fn time_ago(time: u64) -> String {
    if time == 0 { return "—".to_string(); }
    let dt = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap().as_secs().saturating_sub(time);
    if dt < 60 { format!("{}s ago", dt) }
    else if dt < 3600 { format!("{}m ago", dt / 60) }
    else { format!("{}h ago", dt / 3600) }
}

pub fn is_system_disk(device: &str) -> bool {
    // Check if device contains root or boot partition
    let output = Command::new("findmnt")
        .args(["-no", "SOURCE", "/"])
        .output();
    if let Ok(out) = output {
        let root_src = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if root_src.starts_with(device) {
            return true;
        }
    }
    // Check boot
    let output = Command::new("findmnt")
        .args(["-no", "SOURCE", "/boot"])
        .output();
    if let Ok(out) = output {
        let boot_src = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if boot_src.starts_with(device) {
            return true;
        }
    }
    // Check if it's the disk of root
    let output = Command::new("lsblk")
        .args(["-no", "PKNAME", device])
        .output();
    if let Ok(out) = output {
        let pkname = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !pkname.is_empty() {
            let parent = format!("/dev/{}", pkname);
            let output2 = Command::new("findmnt")
                .args(["-no", "SOURCE", "/"])
                .output();
            if let Ok(out2) = output2 {
                let root_src = String::from_utf8_lossy(&out2.stdout).trim().to_string();
                if root_src.starts_with(&parent) || root_src == device {
                    return true;
                }
            }
        }
    }
    false
}

pub fn is_mounted(device: &str) -> bool {
    let output = Command::new("findmnt")
        .args(["-rno", "TARGET", device])
        .output();
    if let Ok(out) = output {
        let out_str = String::from_utf8_lossy(&out.stdout);
        return !out_str.trim().is_empty();
    }
    false
}

pub fn is_device_busy(device: &str) -> bool {
    let output = Command::new("fuser")
        .args(["-s", device])
        .output();
    if let Ok(out) = output {
        return out.status.success();
    }
    false
}
