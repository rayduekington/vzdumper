use serde::Deserialize;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;

const CONFIG_PATH: &str = "/root/.config/vzdumper/config.yaml";
const MOUNT_POINT: &str = "/mnt/proxmox-backup";
const POLL_INTERVAL: Duration = Duration::from_secs(5);
const BINARY_PATH: &str = "/usr/local/bin/vzdumper";
const SERVICE_PATH: &str = "/etc/systemd/system/vzdumper.service";

#[derive(Deserialize)]
struct Config {
    disk_uuid: String,
}

fn load_config() -> Option<Config> {
    let content = match fs::read_to_string(CONFIG_PATH) {
        Ok(c) => c,
        Err(_) => return None,
    };
    match serde_yaml::from_str::<Config>(&content) {
        Ok(cfg) if !cfg.disk_uuid.is_empty() => Some(cfg),
        _ => None,
    }
}

fn wait_for_device(uuid: &str) -> String {
    let path = format!("/dev/disk/by-uuid/{}", uuid);
    println!("Waiting for device {} ...", path);
    loop {
        if Path::new(&path).exists() {
            println!("Device found: {}", path);
            return path;
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn wait_for_removal(uuid: &str) {
    let path = format!("/dev/disk/by-uuid/{}", uuid);
    println!("Waiting for device to be removed ...");
    loop {
        if !Path::new(&path).exists() {
            println!("Device removed.");
            return;
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn do_backup(device_path: &str) {
    fs::create_dir_all(MOUNT_POINT).expect("failed to create mount point");

    let mount = Command::new("mount")
        .arg(device_path)
        .arg(MOUNT_POINT)
        .status()
        .expect("failed to execute mount");
    if !mount.success() {
        eprintln!("mount failed");
        return;
    }
    println!("Mounted at {}", MOUNT_POINT);

    let vzdump = Command::new("vzdump")
        .arg("--all")
        .arg("--dumpdir")
        .arg(MOUNT_POINT)
        .arg("--mode")
        .arg("snapshot")
        .arg("--prune")
        .arg("daily,5")
        .status()
        .expect("failed to execute vzdump");

    let _ = Command::new("umount").arg(MOUNT_POINT).status();

    if vzdump.success() {
        println!("Backup complete.");
    } else {
        eprintln!("vzdump failed.");
    }
}

pub fn run() {
    println!("vzdumper started. Waiting for valid config ...");
    let config = loop {
        if let Some(cfg) = load_config() {
            println!("Config loaded.");
            break cfg;
        }
        thread::sleep(Duration::from_secs(10));
    };

    loop {
        let device = wait_for_device(&config.disk_uuid);
        do_backup(&device);
        wait_for_removal(&config.disk_uuid);
    }
}

pub fn install() {
    // 1. Config
    let config_dir = Path::new("/root/.config/vzdumper");
    fs::create_dir_all(config_dir).unwrap_or_else(|e| {
        eprintln!("Failed to create config dir: {}", e);
        std::process::exit(1);
    });
    let config_file = config_dir.join("config.yaml");
    if !config_file.exists() {
        fs::write(&config_file, "disk_uuid: \"\"\n").unwrap_or_else(|e| {
            eprintln!("Failed to write config: {}", e);
            std::process::exit(1);
        });
        println!("[install] Created config at {}", config_file.display());
        println!("[install]   → Edit it to set your disk_uuid.");
    } else {
        println!("[install] Config already exists at {}", config_file.display());
    }

    // 2. Copy binary
    let self_path = std::env::current_exe().unwrap_or_else(|e| {
        eprintln!("Failed to get self path: {}", e);
        std::process::exit(1);
    });
    println!("[install] Copying {} → {}", self_path.display(), BINARY_PATH);
    fs::copy(&self_path, BINARY_PATH).unwrap_or_else(|e| {
        eprintln!("Failed to copy binary: {}", e);
        std::process::exit(1);
    });
    fs::set_permissions(BINARY_PATH, std::os::unix::fs::PermissionsExt::from_mode(0o755))
        .unwrap_or_else(|e| {
            eprintln!("Failed to chmod: {}", e);
            std::process::exit(1);
        });
    println!("[install] Binary installed at {}", BINARY_PATH);

    // 3. Service
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
        eprintln!("Failed to write service: {}", e);
        std::process::exit(1);
    });
    println!("[install] Service written to {}", SERVICE_PATH);

    // 4. Reload & start
    let r = Command::new("systemctl").args(["daemon-reload"]).status();
    println!("[install] daemon-reload: {:?}", r);

    let r = Command::new("systemctl").args(["enable", "vzdumper.service"]).status();
    println!("[install] enable: {:?}", r);

    let r = Command::new("systemctl").args(["start", "vzdumper.service"]).status();
    println!("[install] start: {:?}", r);

    // 5. Verify
    thread::sleep(Duration::from_secs(2));
    let status = Command::new("systemctl")
        .args(["is-active", "vzdumper.service"])
        .output()
        .expect("failed to run systemctl is-active");
    let active = String::from_utf8_lossy(&status.stdout).trim().to_string();
    if active == "active" {
        println!("[install] ✓ Service is running.");
    } else {
        eprintln!("[install] ✗ Service is NOT running (state: {}). Check: journalctl -u vzdumper", active);
    }
}   