# vzdumper

Automatic Proxmox VE backup tool that triggers backups when a specific removable disk is inserted.

## Overview

`vzdumper` watches for a configured USB/external disk (by UUID), mounts it when detected, runs `vzdump` to back up selected Proxmox VMs and containers, prunes old backups, and waits for the disk to be removed before returning to standby. It includes a web-based GUI for configuration and real-time status monitoring.

## Features

- **Disk-triggered backups**: Automatically starts backups when a configured disk is inserted
- **Flexible targeting**: Back up all VMs/containers or select specific ones via the GUI
- **Automatic pruning**: Keeps only the last N backups (configurable)
- **Web GUI**: Material Design interface for configuration and live status updates (HTTPS with Proxmox certificates if available)
- **Systemd integration**: Runs as a daemon with automatic restart
- **Config migration**: Automatically migrates old `disk_uuid` format to new `disks` array format

## Requirements

- Proxmox VE host
- `vzdump` installed and available in PATH
- `findmnt`, `mount`, `umount` utilities
- Rust toolchain (for building)
- Root privileges (for mounting, writing to system paths)

## Installation

1. Build the binary:
```bash
cargo build --release
```

2. Install as root (copies binary to `/usr/local/bin/vzdumper`, creates systemd services):
```bash
sudo ./target/release/vzdumper --install
```

This will:
- Create config directory at `/root/.config/vzdumper/`
- Generate default config if none exists
- Install the binary to `/usr/local/bin/vzdumper`
- Create and enable `vzdumper.service` (daemon) and `vzdumper-gui.service` (web GUI)
- Start both services

## Configuration

Config file: `/root/.config/vzdumper/config.yaml`

```yaml
disks:
  - "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
targets:
  - "all"  # or specific VM/CT IDs like "100", "101"
keep_last: 5
```

- `disks`: List of disk UUIDs to watch for (from `/dev/disk/by-uuid/`)
- `targets`: `["all"]` to backup everything, or list specific VM/CT IDs
- `keep_last`: Number of backup snapshots to retain (default: 5)

You can also configure via the web GUI at `https://<proxmox-ip>:8889` (or `http://` if TLS certificates aren't found).

## Usage

### As a daemon (normal operation)
```bash
/usr/local/bin/vzdumper
```
The daemon polls for the configured disk every 5 seconds. When found, it mounts to `/mnt/proxmox-backup`, runs `vzdump --mode snapshot`, prunes old backups, unmounts, waits for disk removal, and repeats.

### Web GUI
```bash
/usr/local/bin/vzdumper --gui
```
Or use the `vzdumper-gui.service`. Provides:
- Real-time backup status with icons and colors
- Configure disk UUIDs
- Select backup targets (auto-discovers VMs from `/etc/pve/qemu-server/` and containers from `/etc/pve/containers/`)
- Set number of backups to keep

### Installation mode
```bash
vzdumper --install
```

## How it works

1. Waits for valid config
2. Polls for any configured disk to appear at `/dev/disk/by-uuid/<uuid>`
3. Mounts the disk to `/mnt/proxmox-backup`
4. Runs `vzdump` with `--dumpdir /mnt/proxmox-backup --mode snapshot` for selected targets
5. Prunes backups keeping only the last `keep_last` directories matching `vzdump-*`
6. Unmounts the disk
7. Waits for disk removal before restarting the cycle

## State tracking

Status is written to `/run/vzdumper/state.json` with states: `waiting_for_config`, `waiting_for_device`, `backing_up`, `backup_complete`, `waiting_for_removal`, `error`.

## License

See [LICENSE](LICENSE) file.
