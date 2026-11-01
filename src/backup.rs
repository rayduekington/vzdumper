use serde::{Deserialize, Serialize};
use std::fs;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;

const CONFIG_PATH: &str = "/root/.config/vzdumper/config.yaml";
const MOUNT_POINT: &str = "/mnt/proxmox-backup";
const POLL_INTERVAL: Duration = Duration::from_secs(5);
const BINARY_PATH: &str = "/usr/local/bin/vzdumper";
const SERVICE_PATH: &str = "/etc/systemd/system/vzdumper.service";
const GUI_SERVICE_PATH: &str = "/etc/systemd/system/vzdumper-gui.service";
const STATE_PATH: &str = "/run/vzdumper/state.json";

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(untagged)]
enum DiskEntry {
    Uuid(String),
    WithLabel { 
        uuid: String, 
        #[serde(default, skip_serializing_if = "Option::is_none")] label: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")] _label: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")] targets: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")] paused: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")] disabled: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")] pause: Option<bool>,
    },
}

#[derive(Deserialize, Serialize, Clone)]
struct Config {
    #[serde(default)]
    disks: Vec<DiskEntry>,
    #[serde(default = "default_targets")]
    targets: Vec<String>,
    #[serde(default = "default_keep_last")]
    keep_last: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_backed_up_uuid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    backup_in_progress: Option<bool>,
}

impl Config {
    fn get_disk_uuids(&self) -> Vec<String> {
        self.disks
            .iter()
            .filter_map(|d| match d {
                DiskEntry::Uuid(s) => {
                    if s.is_empty() { None } else { Some(s.clone()) }
                }
                DiskEntry::WithLabel { uuid, paused, disabled, pause, .. } => {
                    if uuid.is_empty() { None } else if paused.unwrap_or(false) || disabled.unwrap_or(false) || pause.unwrap_or(false) { None } else { Some(uuid.clone()) }
                }
            })
            .collect()
    }

    fn get_targets_for_uuid(&self, uuid: &str) -> Vec<String> {
        for disk in &self.disks {
            match disk {
                DiskEntry::Uuid(s) if s == uuid => {
                    if self.targets.is_empty() { return vec!["all".to_string()]; }
                    return self.targets.clone();
                }
                DiskEntry::WithLabel { uuid: u, targets: disk_targets, .. } if u == uuid => {
                    if let Some(t) = disk_targets {
                        if !t.is_empty() { return t.clone(); }
                    }
                    if self.targets.is_empty() { return vec!["all".to_string()]; }
                    return self.targets.clone();
                }
                _ => {}
            }
        }
        if self.targets.is_empty() { vec!["all".to_string()] } else { self.targets.clone() }
    }

    fn get_label_for_uuid(&self, uuid: &str) -> Option<String> {
        for disk in &self.disks {
            match disk {
                DiskEntry::WithLabel { uuid: u, _label, .. } if u == uuid => {
                    return _label.clone();
                }
                _ => {}
            }
        }
        None
    }
}

fn default_targets() -> Vec<String> {
    vec!["all".to_string()]
}

fn default_keep_last() -> u32 {
    5
}

fn load_config() -> Option<Config> {
    let content = match fs::read_to_string(CONFIG_PATH) {
        Ok(c) => c,
        Err(_) => return None,
    };
    match serde_yaml::from_str::<Config>(&content) {
        Ok(cfg) => {
            if cfg.disks.is_empty() { None } else { Some(cfg) }
        }
        _ => None,
    }
}

fn save_config(config: &Config) {
    if let Ok(yaml) = serde_yaml::to_string(config) {
        let _ = fs::write(CONFIG_PATH, yaml);
    }
}

fn write_state(state: &str, detail: &str) {
    let _ = fs::create_dir_all("/run/vzdumper");
    let json = format!(
        "{{\"state\":\"{}\",\"detail\":\"{}\",\"time\":{}}}",
        state,
        detail,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    );
    let _ = fs::write(STATE_PATH, json);
}

fn is_block_device(path: &str) -> bool {
    match fs::metadata(path) {
        Ok(meta) => meta.file_type().is_block_device(),
        Err(_) => false,
    }
}

fn wait_for_any_device(uuids: &[String]) -> Option<String> {
    for uuid in uuids {
        let path = format!("/dev/disk/by-uuid/{}", uuid);
        if is_block_device(&path) {
            println!("Device found: {}", path);
            return Some(path);
        }
    }
    None
}

fn wait_for_removal(uuid: &str) {
    let path = format!("/dev/disk/by-uuid/{}", uuid);
    println!("Waiting for device {} to be removed ...", uuid);
    loop {
        if !Path::new(&path).exists() {
            println!("Device removed.");
            return;
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn do_backup(device_path: &str, targets: &[String], keep_last: u32) {
    fs::create_dir_all(MOUNT_POINT).expect("failed to create mount point");

    for attempt in 0..6 {
        if is_block_device(device_path) { break; }
        println!("Device not ready yet, retrying in 5s... ({})", attempt + 1);
        thread::sleep(POLL_INTERVAL);
    }

    if !is_block_device(device_path) {
        write_state("error", &format!("device {} not available", device_path));
        return;
    }

    let mount_output = Command::new("findmnt")
        .arg(MOUNT_POINT)
        .output()
        .expect("failed to run findmnt");
    let already_mounted = mount_output.status.success()
        && String::from_utf8_lossy(&mount_output.stdout).contains(device_path);

    if already_mounted {
        println!("Already mounted at {}", MOUNT_POINT);
    } else {
        let mount = Command::new("mount")
            .arg(device_path)
            .arg(MOUNT_POINT)
            .output()
            .expect("failed to execute mount");
        if !mount.status.success() {
            let err = String::from_utf8_lossy(&mount.stderr).trim().to_string();
            write_state("error", &format!("mount failed: {}", err));
            return;
        }
        println!("Mounted at {}", MOUNT_POINT);
    }

    write_state("backing_up", device_path);

    let mut cmd = Command::new("vzdump");
    if targets.iter().any(|t| t == "all") {
        cmd.arg("--all");
    } else {
        for t in targets {
            cmd.arg(t);
        }
    }
    cmd.arg("--dumpdir").arg(MOUNT_POINT).arg("--mode").arg("snapshot").arg("--prune-backups").arg(format!("keep-last={}", keep_last));

    use std::process::Stdio;
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("failed to execute vzdump");
    let status = child.wait().expect("failed to wait for vzdump");

    let _ = Command::new("umount").arg(MOUNT_POINT).status();

    if status.success() {
        write_state("backup_complete", device_path);
        println!("Backup complete. Kept last {} backups.", keep_last);
    } else {
        write_state("error", &format!("vzdump failed with status: {}", status));
        eprintln!("vzdump failed with status: {}", status);
    }
}

pub fn run() {
    write_state("waiting_for_config", "");
    println!("vzdumper started. Waiting for valid config ...");
    let mut config = loop {
        if let Some(cfg) = load_config() {
            let uuids = cfg.get_disk_uuids();
            println!("Config loaded: {} disk(s), targets: {:?}, keep_last: {}", uuids.len(), cfg.targets, cfg.keep_last);
            break cfg;
        }
        thread::sleep(Duration::from_secs(10));
    };

    loop {
        // Reload config to pick up changes
        if let Some(fresh_cfg) = load_config() {
            // Preserve our runtime state
            let last_backed_up_uuid = config.last_backed_up_uuid.clone();
            let backup_in_progress = config.backup_in_progress;
            config = fresh_cfg;
            config.last_backed_up_uuid = last_backed_up_uuid;
            config.backup_in_progress = backup_in_progress;
        }

        // If we just restarted and the last backed up disk is still present, wait for removal first
        if let Some(ref last_uuid) = config.last_backed_up_uuid.clone() {
            let path = format!("/dev/disk/by-uuid/{}", last_uuid);
            if Path::new(&path).exists() {
                println!("Resuming: last backed up disk {} still present, waiting for removal...", last_uuid);
                write_state("waiting_for_removal", last_uuid);
                wait_for_removal(last_uuid);
                // Clear the last backed up marker
                config.last_backed_up_uuid = None;
                config.backup_in_progress = Some(false);
                save_config(&config);
                continue;
            } else {
                // Disk was removed, clear marker
                config.last_backed_up_uuid = None;
                config.backup_in_progress = Some(false);
                save_config(&config);
            }
        }

        let disk_uuids = config.get_disk_uuids();
        write_state("waiting_for_device", &disk_uuids.join(", "));
        let device = loop {
            // Reload config while waiting
            if let Some(fresh_cfg) = load_config() {
                let last_backed_up_uuid = config.last_backed_up_uuid.clone();
                let backup_in_progress = config.backup_in_progress;
                config = fresh_cfg;
                config.last_backed_up_uuid = last_backed_up_uuid;
                config.backup_in_progress = backup_in_progress;
            }
            if let Some(d) = wait_for_any_device(&config.get_disk_uuids()) {
                break d;
            }
            thread::sleep(POLL_INTERVAL);
        };

        let uuid = device.rsplit('/').next().unwrap_or("");
        // Mark backup in progress
        config.last_backed_up_uuid = Some(uuid.to_string());
        config.backup_in_progress = Some(true);
        save_config(&config);

        let targets = config.get_targets_for_uuid(uuid);
        if let Some(label) = config.get_label_for_uuid(uuid) {
            write_state("backing_up", &format!("{} ({})", label, uuid));
        }
        do_backup(&device, &targets, config.keep_last);
        thread::sleep(Duration::from_secs(5));

        write_state("waiting_for_removal", uuid);
        wait_for_removal(uuid);
        // Clear after removal
        config.last_backed_up_uuid = None;
        config.backup_in_progress = Some(false);
        save_config(&config);
    }
}

pub fn install() {
    let config_dir = Path::new("/root/.config/vzdumper");
    fs::create_dir_all(config_dir).unwrap_or_else(|e| {
        eprintln!("[install] Failed to create config dir: {}", e);
        std::process::exit(1);
    });
    let config_file = config_dir.join("config.yaml");
    if !config_file.exists() {
        fs::write(&config_file, "disks:\n  - \"\"\ntargets:\n  - \"all\"\nkeep_last: 5\n").unwrap_or_else(|e| {
            eprintln!("[install] Failed to write config: {}", e);
            std::process::exit(1);
        });
        println!("[install] Created config at {}", config_file.display());
    } else {
        println!("[install] Config already exists at {}", config_file.display());
    }

    if config_file.exists() {
        let content = fs::read_to_string(&config_file).unwrap_or_default();
        if content.contains("disk_uuid:") && !content.contains("disks:") {
            let uuid = content
                .lines()
                .find(|l| l.trim_start().starts_with("disk_uuid:"))
                .and_then(|l| l.split_once(':').map(|(_, v)| v.trim().trim_matches('"').to_string()))
                .unwrap_or_default();
            let new_yaml = format!("disks:\n  - \"{}\"\ntargets:\n  - \"all\"\nkeep_last: 5\n", uuid);
            fs::write(&config_file, new_yaml).unwrap_or_else(|e| {
                eprintln!("[install] Failed to migrate config: {}", e);
                std::process::exit(1);
            });
            println!("[install] Migrated config to new format.");
        }
    }

    let self_path = std::env::current_exe().unwrap_or_else(|e| {
        eprintln!("[install] Failed to get self path: {}", e);
        std::process::exit(1);
    });
    println!("[install] Copying {} → {}", self_path.display(), BINARY_PATH);
    fs::copy(&self_path, BINARY_PATH).unwrap_or_else(|e| {
        eprintln!("[install] Failed to copy binary: {}", e);
        std::process::exit(1);
    });
    fs::set_permissions(BINARY_PATH, PermissionsExt::from_mode(0o755))
        .unwrap_or_else(|e| {
            eprintln!("[install] Failed to chmod: {}", e);
            std::process::exit(1);
        });
    println!("[install] Binary installed at {}", BINARY_PATH);

    let service = r#"[Unit]
Description=vzdumper - Proxmox backup on disk insert
After=local-fs.target

[Service]
Type=simple
ExecStart=/usr/local/bin/vzdumper
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
"#;
    fs::write(SERVICE_PATH, service).unwrap_or_else(|e| {
        eprintln!("[install] Failed to write service: {}", e);
        std::process::exit(1);
    });

    let gui_service = r#"[Unit]
Description=vzdumper web GUI
After=network.target
RequiresMountsFor=/etc/pve

[Service]
Type=simple
ExecStart=/usr/local/bin/vzdumper --gui
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
"#;
    fs::write(GUI_SERVICE_PATH, gui_service).unwrap_or_else(|e| {
        eprintln!("[install] Failed to write GUI service: {}", e);
        std::process::exit(1);
    });

    let r = Command::new("systemctl").args(["daemon-reload"]).status();
    println!("[install] daemon-reload: {:?}", r);
    let r = Command::new("systemctl").args(["enable", "vzdumper.service"]).status();
    println!("[install] enable daemon: {:?}", r);
    let r = Command::new("systemctl").args(["start", "vzdumper.service"]).status();
    println!("[install] start daemon: {:?}", r);
    let r = Command::new("systemctl").args(["enable", "vzdumper-gui.service"]).status();
    println!("[install] enable gui: {:?}", r);
    let r = Command::new("systemctl").args(["start", "vzdumper-gui.service"]).status();
    println!("[install] start gui: {:?}", r);

    thread::sleep(Duration::from_secs(2));
    let _ = fs::create_dir_all("/run/vzdumper");
    write_state("waiting_for_config", "");
    let _ = Command::new("systemctl").args(["restart", "vzdumper.service"]).status();
    let _ = Command::new("systemctl").args(["restart", "vzdumper-gui.service"]).status();
    thread::sleep(Duration::from_secs(2));
    for svc in ["vzdumper.service", "vzdumper-gui.service"] {
        let status = Command::new("systemctl")
            .args(["is-active", svc])
            .output()
            .expect("failed to check status");
        let active = String::from_utf8_lossy(&status.stdout).trim().to_string();
        if active == "active" {
            println!("[install] ✓ {} is running.", svc);
        } else {
            eprintln!("[install] ✗ {} is NOT running (state: {}).", svc, active);
        }
    }
    println!("[install] GUI available at https://<proxmox-ip>:8889");
}

pub fn update() {
    let self_path = std::env::current_exe().unwrap_or_else(|e| {
        eprintln!("[update] Failed to get self path: {}", e);
        std::process::exit(1);
    });
    println!("[update] Updating {} → {}", self_path.display(), BINARY_PATH);
    let _ = Command::new("systemctl").args(["stop", "vzdumper.service"]).status();
    let _ = Command::new("systemctl").args(["stop", "vzdumper-gui.service"]).status();
    fs::copy(&self_path, BINARY_PATH).unwrap_or_else(|e| {
        eprintln!("[update] Failed to copy binary: {}", e);
        std::process::exit(1);
    });
    fs::set_permissions(BINARY_PATH, PermissionsExt::from_mode(0o755))
        .unwrap_or_else(|e| {
            eprintln!("[update] Failed to chmod: {}", e);
            std::process::exit(1);
        });
    println!("[update] Binary updated at {}", BINARY_PATH);
    let _ = Command::new("systemctl").args(["daemon-reload"]).status();
    let _ = Command::new("systemctl").args(["start", "vzdumper.service"]).status();
    let _ = Command::new("systemctl").args(["start", "vzdumper-gui.service"]).status();
    thread::sleep(Duration::from_secs(2));
    for svc in ["vzdumper.service", "vzdumper-gui.service"] {
        let status = Command::new("systemctl")
            .args(["is-active", svc])
            .output()
            .expect("failed to check status");
        let active = String::from_utf8_lossy(&status.stdout).trim().to_string();
        if active == "active" {
            println!("[update] ✓ {} is running.", svc);
        } else {
            eprintln!("[update] ✗ {} is NOT running (state: {}).", svc, active);
        }
    }
    println!("[update] Update complete.");
}

pub fn uninstall() {
    use std::io::{self, Write};
    print!("Are you sure you want to uninstall vzdumper? [y/N]: ");
    io::stdout().flush().unwrap();
    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        println!("Uninstall cancelled.");
        return;
    }
    let input = input.trim().to_lowercase();
    if input != "y" && input != "yes" {
        println!("Uninstall cancelled.");
        return;
    }
    println!("[uninstall] Stopping services...");
    let _ = Command::new("systemctl").args(["stop", "vzdumper.service"]).status();
    let _ = Command::new("systemctl").args(["stop", "vzdumper-gui.service"]).status();
    let _ = Command::new("systemctl").args(["disable", "vzdumper.service"]).status();
    let _ = Command::new("systemctl").args(["disable", "vzdumper-gui.service"]).status();
    let _ = fs::remove_file(SERVICE_PATH);
    let _ = fs::remove_file(GUI_SERVICE_PATH);
    let _ = Command::new("systemctl").args(["daemon-reload"]).status();
    let _ = fs::remove_file(BINARY_PATH);
    let _ = fs::remove_dir_all("/root/.config/vzdumper");
    let _ = fs::remove_dir_all("/run/vzdumper");
    println!("[uninstall] vzdumper has been uninstalled.");
}
