use crate::gui::utils::{is_device_busy, is_mounted, is_system_disk};
use std::process::Command;
use std::fs;

pub fn get_available_targets() -> Vec<String> {
    let mut result = Vec::new();
    if let Ok(entries) = fs::read_dir("/etc/pve/qemu-server") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(id) = name.split('.').next() {
                if id.chars().all(|c| c.is_ascii_digit()) {
                    result.push(id.to_string());
                }
            }
        }
    }
    if let Ok(entries) = fs::read_dir("/etc/pve/containers") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.chars().all(|c| c.is_ascii_digit()) {
                result.push(name);
            }
        }
    }
    result.sort();
    result
}

pub fn scan_disks() -> String {
    let output = Command::new("lsblk")
        .args(["-J", "-o", "NAME,SIZE,TYPE,TRAN,RM,RO,UUID,LABEL,MOUNTPOINT,FSTYPE,MODEL"])
        .output();
    
    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&stdout) {
                if let Some(blockdevices) = v.get("blockdevices").and_then(|b| b.as_array()) {
                    let mut result = Vec::new();
                    for dev in blockdevices {
                        let name = dev.get("name").and_then(|n| n.as_str()).unwrap_or("");
                        if name.is_empty() { continue; }
                        let dev_path = format!("/dev/{}", name);
                        let dtype = dev.get("type").and_then(|t| t.as_str()).unwrap_or("");
                        if dtype != "disk" { continue; }
                        let rm = dev.get("rm").and_then(|r| r.as_bool()).unwrap_or(false);
                        let ro = dev.get("ro").and_then(|r| r.as_bool()).unwrap_or(false);
                        if ro { continue; }
                        if is_system_disk(&dev_path) { continue; }
                        let mounted = is_mounted(&dev_path);
                        let mut has_mounted_child = false;
                        if let Some(children) = dev.get("children").and_then(|c| c.as_array()) {
                            for child in children {
                                let cname = child.get("name").and_then(|n| n.as_str()).unwrap_or("");
                                if cname.is_empty() { continue; }
                                let cpath = format!("/dev/{}", cname);
                                if is_mounted(&cpath) { has_mounted_child = true; break; }
                            }
                        }
                        if mounted || has_mounted_child { continue; }
                        if is_device_busy(&dev_path) { continue; }
                        
                        let size = dev.get("size").and_then(|s| s.as_str()).unwrap_or("");
                        let tran = dev.get("tran").and_then(|t| t.as_str()).unwrap_or("");
                        let uuid = dev.get("uuid").and_then(|u| u.as_str()).unwrap_or("");
                        let label = dev.get("label").and_then(|l| l.as_str()).unwrap_or("");
                        let fstype = dev.get("fstype").and_then(|f| f.as_str()).unwrap_or("");
                        let model = dev.get("model").and_then(|m| m.as_str()).unwrap_or("");
                        
                        result.push(serde_json::json!({
                            "device": dev_path,
                            "name": name,
                            "size": size,
                            "transport": tran,
                            "removable": rm,
                            "uuid": uuid,
                            "label": label,
                            "fstype": fstype,
                            "model": model,
                            "mounted": false,
                            "safe": true
                        }));
                    }
                    return serde_json::to_string(&result).unwrap_or("[]".to_string());
                }
            }
            "[]".to_string()
        }
        Err(_) => "[]".to_string()
    }
}

pub fn read_config() -> (Vec<(String, Option<String>, Option<Vec<String>>, bool)>, Vec<String>, u32) {
    let mut disks = Vec::new();
    let mut targets = vec!["all".to_string()];
    let mut keep_last: u32 = 5;

    if let Ok(content) = std::fs::read_to_string(crate::gui::CONFIG_PATH) {
        let v: serde_yaml::Value = serde_yaml::from_str(&content).unwrap_or_default();
        if let Some(seq) = v.get("disks").and_then(|d| d.as_sequence()) {
            for item in seq {
                if let Some(s) = item.as_str() {
                    if !s.is_empty() {
                        disks.push((s.to_string(), None, None, false));
                    }
                } else if let Some(obj) = item.as_mapping() {
                    let uuid = obj.get("uuid")
                        .and_then(|u| u.as_str())
                        .unwrap_or("")
                        .to_string();
                    if !uuid.is_empty() {
                        let label = obj.get("label")
                            .and_then(|l| l.as_str())
                            .map(|l| l.to_string())
                            .filter(|l| !l.is_empty());
                        let disk_targets = obj.get("targets").and_then(|t| {
                            if let Some(s) = t.as_str() {
                                Some(vec![s.to_string()])
                            } else if let Some(seq) = t.as_sequence() {
                                Some(seq.iter()
                                    .filter_map(|item| item.as_str().map(|s| s.to_string()))
                                    .filter(|s| !s.is_empty())
                                    .collect::<Vec<String>>())
                            } else {
                                None
                            }
                        }).filter(|v| !v.is_empty());
                        let paused = obj.get("paused").and_then(|p| p.as_bool()).unwrap_or(false)
                            || obj.get("disabled").and_then(|p| p.as_bool()).unwrap_or(false)
                            || obj.get("pause").and_then(|p| p.as_bool()).unwrap_or(false);
                        disks.push((uuid, label, disk_targets, paused));
                    }
                }
            }
        }
        if let Some(t) = v.get("targets") {
            if let Some(s) = t.as_str() {
                targets = vec![s.to_string()];
            } else if let Some(seq) = t.as_sequence() {
                targets = seq.iter()
                    .filter_map(|item| item.as_str().map(|s| s.to_string()))
                    .filter(|s| !s.is_empty())
                    .collect();
            }
        }
        if let Some(k) = v.get("keep_last").and_then(|k| k.as_u64()) {
            keep_last = k as u32;
        }
    }
    (disks, targets, keep_last)
}

pub fn write_config(disks: &[(String, Option<String>, Option<Vec<String>>, bool)], targets: &[String], keep_last: u32) -> Result<(), String> {
    let dir = std::path::Path::new("/root/.config/vzdumper");
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut yaml = String::from("disks:\n");
    for (uuid, label, disk_targets, paused) in disks {
        let has_obj = label.as_ref().map_or(false, |l| !l.is_empty()) || disk_targets.as_ref().map_or(false, |t| !t.is_empty()) || *paused;
        if has_obj {
            yaml.push_str(&format!("  - uuid: \"{}\"\n", uuid.replace('"', "\\\"")));
            if let Some(l) = label {
                if !l.is_empty() {
                    yaml.push_str(&format!("    label: \"{}\"\n", l.replace('"', "\\\"")));
                }
            }
            if let Some(dt) = disk_targets {
                if !dt.is_empty() {
                    yaml.push_str("    targets:\n");
                    for t in dt {
                        yaml.push_str(&format!("      - \"{}\"\n", t.replace('"', "\\\"")));
                    }
                }
            }
            if *paused {
                yaml.push_str("    paused: true\n");
            }
        } else {
            yaml.push_str(&format!("  - \"{}\"\n", uuid.replace('"', "\\\"")));
        }
    }
    yaml.push_str("targets:\n");
    for t in targets {
        yaml.push_str(&format!("  - \"{}\"\n", t.replace('"', "\\\"")));
    }
    yaml.push_str(&format!("keep_last: {}\n", keep_last));
    std::fs::write(crate::gui::CONFIG_PATH, yaml).map_err(|e| e.to_string())
}
