use serde::Deserialize;
use std::fs;
use std::process::Command;

const CONFIG_PATH: &str = "/root/.config/vzdumper/config.yaml";
const MOUNT_POINT: &str = "/mnt/proxmox-backup";

#[derive(Deserialize)]
struct Config {
    disk_uuid: String,
}

fn load_config() -> Config {
    let content = fs::read_to_string(CONFIG_PATH)
        .unwrap_or_else(|e| {
            eprintln!("Failed to read {}: {}", CONFIG_PATH, e);
            std::process::exit(1);
        });
    serde_yaml::from_str(&content)
        .unwrap_or_else(|e| {
            eprintln!("Failed to parse config: {}", e);
            std::process::exit(1);
        })
}

/*fn ensure_udev_rule(uuid: &str) {
    let rule = UDEV_RULE_TEMPLATE.replace("{}", uuid);

    let existing = fs::read_to_string(UDEV_RULE_PATH).unwrap_or_default();
    if existing.trim() == rule.trim() {
        return; // already up to date
    }

    fs::write(UDEV_RULE_PATH, &rule).unwrap_or_else(|e| {
        eprintln!("Failed to write udev rule: {}", e);
        std::process::exit(1);
    });
    println!("Udev rule written to {}", UDEV_RULE_PATH);

    // Reload udev rules
    Command::new("udevadm")
        .args(["control", "--reload-rules"])
        .status()
        .expect("failed to run udevadm");
    println!("Udev rules reloaded.");
}*/

const UDEV_RULE_PATH: &str = "/etc/udev/rules.d/zz-proxmox-backup.rules";

fn ensure_udev_rule(uuid: &str) {
    let rule = format!(
        "ENV{{ID_FS_UUID}}==\"{}\", TAG+=\"systemd\", ENV{{SYSTEMD_WANTS}}+=\"proxmox-backup.service\"",
        uuid
    );

    let existing = fs::read_to_string(UDEV_RULE_PATH).unwrap_or_default();
    if existing.trim() == rule.trim() {
        return;
    }

    fs::write(UDEV_RULE_PATH, &rule).unwrap_or_else(|e| {
        eprintln!("Failed to write udev rule: {}", e);
        std::process::exit(1);
    });
    println!("Udev rule written to {}", UDEV_RULE_PATH);

    Command::new("udevadm")
        .args(["control", "--reload-rules"])
        .status()
        .expect("failed to run udevadm");
    println!("Udev rules reloaded.");
} 

pub fn run() {
    let config = load_config();

    // 0. Ensure udev rule is in place
    ensure_udev_rule(&config.disk_uuid);

    // 1. Create mount point
    fs::create_dir_all(MOUNT_POINT).expect("failed to create mount point");

    // 2. Mount disk by UUID
    let mount = Command::new("mount")
        .arg(format!("/dev/disk/by-uuid/{}", config.disk_uuid))
        .arg(MOUNT_POINT)
        .status()
        .expect("failed to execute mount");
    if !mount.success() {
        eprintln!("mount failed");
        std::process::exit(1);
    }
    println!("Mounted disk {} at {}", config.disk_uuid, MOUNT_POINT);

    // 3. vzdump --all
    let vzdump = Command::new("vzdump")
        .arg("--all")
        .arg("--dumpdir")
        .arg(MOUNT_POINT)
        .arg("--mode")
        .arg("snapshot")
        .arg("--prune-backups")
        .arg("keep-last=5")
        .status()
        .expect("failed to execute vzdump");
    if !vzdump.success() {
        eprintln!("vzdump failed");
        std::process::exit(1);
    }
    println!("All VMs backed up.");

    // 4. Unmount
    if let Ok(s) = Command::new("umount").arg(MOUNT_POINT).status() {
        if !s.success() {
            eprintln!("umount failed");
        }
    }
}   