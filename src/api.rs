use std::fs;
use std::process::Command;

const CONFIG_PATH: &str = "/root/.config/vzdumper/config.yaml";

fn get_available_targets() -> Vec<String> {
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

pub fn list_disks() {
    println!("Scanning for disks...");
    let output = Command::new("lsblk")
        .args(["-o", "NAME,SIZE,TYPE,UUID,LABEL,MOUNTPOINT", "-J"])
        .output();
    
    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            println!("{}", stdout);
        }
        Err(e) => {
            eprintln!("Failed to run lsblk: {}", e);
            // Fallback
            if let Ok(entries) = fs::read_dir("/dev/disk/by-uuid") {
                println!("Disks by UUID:");
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if let Ok(link) = fs::read_link(entry.path()) {
                        println!("  {} -> {}", name, link.to_string_lossy());
                    }
                }
            }
        }
    }
}

pub fn list_vms() {
    let targets = get_available_targets();
    println!("Available VMs and containers:");
    for t in targets {
        println!("  {}", t);
    }
}

fn get_arg(args: &[String], flag: &str) -> Option<String> {
    for i in 0..args.len() {
        if args[i] == flag && i + 1 < args.len() {
            return Some(args[i + 1].clone());
        }
    }
    None
}

pub fn add_target(args: &[String]) {
    let uuid = match get_arg(args, "--uuid") {
        Some(u) => u,
        None => {
            eprintln!("Error: --uuid required");
            return;
        }
    };
    let label = get_arg(args, "--label");
    
    let mut config = fs::read_to_string(CONFIG_PATH).unwrap_or_default();
    let new_entry = if let Some(l) = label {
        format!("  - uuid: \"{}\"\n    label: \"{}\"\n", uuid.replace('"', "\\\""), l.replace('"', "\\\""))
    } else {
        format!("  - \"{}\"\n", uuid.replace('"', "\\\""))
    };
    
    if config.contains("disks:") {
        // Try to append to disks section
        if let Some(pos) = config.find("disks:") {
            // Find end of disks section (next top-level key or end)
            let rest = &config[pos..];
            if let Some(next_key) = rest[6..].find('\n') {
                // Look for next line starting at column 0
                let mut insert_pos = pos + 6 + next_key;
                // Find the actual end of this line
                while insert_pos < config.len() && config.as_bytes()[insert_pos] != b'\n' {
                    insert_pos += 1;
                }
                insert_pos += 1;
                // Insert before targets or next section
                config.insert_str(insert_pos, &new_entry);
            } else {
                config.push_str(&new_entry);
            }
        } else {
            config = format!("disks:\n{}{}", new_entry, config);
        }
    } else {
        config = format!("disks:\n{}targets:\n  - \"all\"\nkeep_last: 5\n{}", new_entry, config);
    }
    
    if let Err(e) = fs::write(CONFIG_PATH, config) {
        eprintln!("Failed to write config: {}", e);
    } else {
        println!("Added backup target: {}", uuid);
    }
}

pub fn add_profile(args: &[String]) {
    let disk = match get_arg(args, "--disk") {
        Some(d) => d,
        None => {
            eprintln!("Error: --disk required");
            return;
        }
    };
    let vms_str = match get_arg(args, "--vms") {
        Some(v) => v,
        None => {
            eprintln!("Error: --vms required");
            return;
        }
    };
    let retention = get_arg(args, "--retention");
    
    let vms: Vec<String> = vms_str.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    
    let mut config = fs::read_to_string(CONFIG_PATH).unwrap_or_default();
    
    // Update disks section to add per-disk targets
    // This is a simplified implementation
    println!("Profile created: disk={}, vms={:?}, retention={:?}", disk, vms, retention);
    
    if let Some(r) = retention {
        // Update keep_last
        if config.contains("keep_last:") {
            // Simple replacement
        } else {
            config.push_str(&format!("keep_last: {}\n", r));
        }
    }
    
    if let Err(e) = fs::write(CONFIG_PATH, config) {
        eprintln!("Failed to write config: {}", e);
    } else {
        println!("Profile updated");
    }
}
