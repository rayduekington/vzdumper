use crate::gui::disk::{read_config, scan_disks, write_config};
use crate::gui::utils::{is_device_busy, is_mounted, is_system_disk, read_state, state_color, state_icon, time_ago};
use std::process::Command;
use std::time::Duration;
use tiny_http::{Request, Response, Header};

pub fn handle_state(request: Request) {
    let (state, detail, time) = read_state();
    let json = format!(
        "{{\"state\":\"{}\",\"detail\":\"{}\",\"time\":\"{}\",\"color\":\"{}\",\"icon\":\"{}\"}}",
        state, detail, time_ago(time), state_color(&state), state_icon(&state)
    );
    let _ = request.respond(
        Response::from_string(json).with_header(
            Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
        )
    );
}

pub fn handle_tls(request: Request, ssl_config: &Option<tiny_http::SslConfig>) {
    let tls_info = match ssl_config {
        Some(_) => {
            let cert = crate::gui::find_cert_files().map(|(p, _, _)| p).unwrap_or("unknown");
            format!(
                "{{\"tls_enabled\":true,\"cert_path\":\"{}\",\"scheme\":\"https\"}}",
                cert
            )
        }
        None => "{\"tls_enabled\":false,\"cert_path\":null,\"scheme\":\"http\"}".to_string(),
    };
    let _ = request.respond(
        Response::from_string(tls_info).with_header(
            Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
        )
    );
}

pub fn handle_disks_scan(request: Request) {
    let disks_json = scan_disks();
    let _ = request.respond(
        Response::from_string(disks_json).with_header(
            Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
        )
    );
}

pub fn handle_disks_use(mut request: Request) {
    let mut body = String::new();
    let _ = request.as_reader().read_to_string(&mut body);
    let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
    let device = v.get("device").and_then(|d| d.as_str()).unwrap_or("");
    let label = v.get("label").and_then(|l| l.as_str()).unwrap_or("");
    if device.is_empty() {
        let _ = request.respond(Response::from_string("{\"error\":\"device required\"}").with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()));
        return;
    }
    let output = Command::new("blkid")
        .args(["-s", "UUID", "-o", "value", device])
        .output();
    let uuid = match output {
        Ok(out) => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        Err(_) => String::new()
    };
    if uuid.is_empty() {
        let _ = request.respond(Response::from_string("{\"error\":\"no uuid found\"}").with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()));
        return;
    }
    if is_system_disk(device) || is_mounted(device) || is_device_busy(device) {
        let _ = request.respond(Response::from_string("{\"error\":\"unsafe disk\"}").with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()));
        return;
    }
    let (mut disks, targets, keep_last) = read_config();
    disks.push((uuid.clone(), if label.is_empty() { None } else { Some(label.to_string()) }, None, false));
    if let Err(e) = write_config(&disks, &targets, keep_last) {
        let _ = request.respond(Response::from_string(format!("{{\"error\":\"{}\"}}", e)).with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()));
    } else {
        let _ = Command::new("systemctl").args(["restart", "vzdumper.service"]).status();
        let _ = request.respond(Response::from_string("{\"success\":true}").with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()));
    }
}

pub fn handle_disks_format(mut request: Request) {
    let mut body = String::new();
    let _ = request.as_reader().read_to_string(&mut body);
    let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
    let device = v.get("device").and_then(|d| d.as_str()).unwrap_or("");
    let label = v.get("label").and_then(|l| l.as_str()).unwrap_or("");
    let confirm = v.get("confirm").and_then(|c| c.as_str()).unwrap_or("");
    if device.is_empty() {
        let _ = request.respond(Response::from_string("{\"error\":\"device required\"}").with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()));
        return;
    }
    if confirm != device {
        let _ = request.respond(Response::from_string("{\"error\":\"confirmation mismatch\"}").with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()));
        return;
    }
    if is_system_disk(device) || is_mounted(device) || is_device_busy(device) {
        let _ = request.respond(Response::from_string("{\"error\":\"unsafe disk\"}").with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()));
        return;
    }
    let _ = Command::new("bash").args(["-c", &format!("umount {}* 2>/dev/null || true", device)]).status();
    let wipe = Command::new("wipefs").args(["-a", device]).status();
    if wipe.is_err() {
        let _ = request.respond(Response::from_string("{\"error\":\"wipe failed\"}").with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()));
        return;
    }
    let mkfs = Command::new("mkfs.ext4").args(["-F", device]).status();
    if mkfs.is_err() || !mkfs.as_ref().unwrap().success() {
        let _ = request.respond(Response::from_string("{\"error\":\"format failed\"}").with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()));
        return;
    }
    std::thread::sleep(Duration::from_secs(3));
    let mut uuid = String::new();
    let output = Command::new("blkid")
        .args(["-s", "UUID", "-o", "value", device])
        .output();
    if let Ok(out) = output {
        uuid = String::from_utf8_lossy(&out.stdout).trim().to_string();
    }
    if uuid.is_empty() {
        std::thread::sleep(Duration::from_secs(2));
        let output = Command::new("blkid")
            .args(["-s", "UUID", "-o", "value", device])
            .output();
        if let Ok(out) = output {
            uuid = String::from_utf8_lossy(&out.stdout).trim().to_string();
        }
    }
    if uuid.is_empty() {
        let _ = request.respond(Response::from_string("{\"error\":\"uuid not found after format\"}").with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()));
        return;
    }
    let (mut disks, targets, keep_last) = read_config();
    disks.push((uuid.clone(), if label.is_empty() { None } else { Some(label.to_string()) }, None, false));
    if let Err(e) = write_config(&disks, &targets, keep_last) {
        let _ = request.respond(Response::from_string(format!("{{\"error\":\"{}\"}}", e)).with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()));
    } else {
        let _ = Command::new("systemctl").args(["restart", "vzdumper.service"]).status();
        let _ = request.respond(Response::from_string("{\"success\":true,\"uuid\":\"".to_string() + &uuid + "\"}").with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()));
    }
}
