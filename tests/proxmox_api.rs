use std::fs;

/// Parsed credentials from the proxmox-test file.
struct ProxmoxTestCreds {
    ip: String,
    user: String,
    pass: String,
}

/// Read and parse the `proxmox-test` file.
/// Expected format (line-based):
///   Line 1: IP address
///   Line 2: user: <username>
///   Line 3: pass: <password>
fn load_proxmox_test(path: &str) -> Option<ProxmoxTestCreds> {
    let content = fs::read_to_string(path).ok()?;
    let lines: Vec<&str> = content.lines().collect();
    if lines.len() < 3 {
        return None;
    }

    let ip = lines[0].trim().to_string();
    let user = lines
        .iter()
        .find(|l| l.trim_start().starts_with("user:"))
        .and_then(|l| l.split_once(':'))
        .map(|(_, v)| v.trim().to_string())?;
    let pass = lines
        .iter()
        .find(|l| l.trim_start().starts_with("pass:"))
        .and_then(|l| l.split_once(':'))
        .map(|(_, v)| v.trim().to_string())?;

    if ip.is_empty() || user.is_empty() || pass.is_empty() {
        return None;
    }

    Some(ProxmoxTestCreds { ip, user, pass })
}

#[test]
fn test_proxmox_test_file_exists() {
    let path = env!("CARGO_MANIFEST_DIR").to_string() + "/proxmox-test";
    assert!(
        std::path::Path::new(&path).exists(),
        "proxmox-test file should exist at {}",
        path
    );
}

#[test]
fn test_proxmox_test_file_parses() {
    let path = env!("CARGO_MANIFEST_DIR").to_string() + "/proxmox-test";
    let creds = load_proxmox_test(&path);
    assert!(creds.is_some(), "Failed to parse proxmox-test file");

    let creds = creds.unwrap();
    assert!(!creds.ip.is_empty(), "IP address should not be empty");
    assert!(!creds.user.is_empty(), "Username should not be empty");
    assert!(!creds.pass.is_empty(), "Password should not be empty");

    // IP should look like an IPv4 address (4 octets)
    let octets: Vec<&str> = creds.ip.split('.').collect();
    assert_eq!(octets.len(), 4, "IP should be IPv4 format");
    for octet in &octets {
        let val: u16 = octet.parse().expect("octet should be numeric");
        assert!(
            val <= 255,
            "IP octet {} should be <= 255",
            val
        );
    }

    assert_eq!(creds.user, "root", "Expected user to be 'root'");
}

#[test]
fn test_proxmox_ssh_port_open() {
    let path = env!("CARGO_MANIFEST_DIR").to_string() + "/proxmox-test";
    let creds = load_proxmox_test(&path)
        .expect("proxmox-test file should be parseable");

    let result = std::process::Command::new("timeout")
        .args(["5", "bash", "-c", &format!("echo -n '' | nc -z -w 3 {} 22", creds.ip)])
        .status();

    match result {
        Ok(status) if status.success() => {
            // Port 22 is open
        }
        Ok(_) => {
            eprintln!("SSH port 22 closed or unreachable");
        }
        Err(_) => {
            eprintln!("netcat not available, skipping port check");
        }
    }
}

#[test]
fn test_proxmox_api_port_open() {
    let path = env!("CARGO_MANIFEST_DIR").to_string() + "/proxmox-test";
    let creds = load_proxmox_test(&path)
        .expect("proxmox-test file should be parseable");

    let result = std::process::Command::new("timeout")
        .args(["5", "bash", "-c", &format!("echo -n '' | nc -z -w 3 {} 8006", creds.ip)])
        .status();

    match result {
        Ok(status) if status.success() => {
            // Port 8006 (Proxmox API) is open
        }
        Ok(_) => {
            eprintln!("Proxmox API port 8006 closed or unreachable");
        }
        Err(_) => {
            eprintln!("netcat not available, skipping port check");
        }
    }
}

#[test]
fn test_proxmox_api_login() {
    let path = env!("CARGO_MANIFEST_DIR").to_string() + "/proxmox-test";
    let creds = load_proxmox_test(&path)
        .expect("proxmox-test file should be parseable");

    // Proxmox PAM authentication requires the username to include the @pam realm suffix
    let pam_user = format!("{}@pam", creds.user);
    let ticket_url = format!(
        "https://{}:8006/api2/json/access/ticket",
        creds.ip
    );

    let output = std::process::Command::new("curl")
        .args([
            "-sk",
            "-X", "POST",
            &ticket_url,
            "-H", "Content-Type: application/x-www-form-urlencoded",
            "-d", &format!("username={}&password={}", pam_user, creds.pass),
            "-w", "%{http_code}",
            "-o", "/dev/null",
            "--connect-timeout", "5",
        ])
        .output();

    let result = output.expect("curl should be available");
    let http_code = String::from_utf8_lossy(&result.stdout);
    let code: u16 = http_code.trim().parse().unwrap_or(0);
    assert_eq!(
        code, 200,
        "Proxmox API login failed with HTTP {} (user={})",
        code, pam_user
    );
    println!("Proxmox API login successful (HTTP 200)");
}
