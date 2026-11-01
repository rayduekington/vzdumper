use tiny_http::{Server, SslConfig};
use std::time::Duration;

pub mod api;
pub mod disk;
pub mod routes;
pub mod templates;
pub mod utils;

pub const CONFIG_PATH: &str = "/root/.config/vzdumper/config.yaml";
pub const STATE_PATH: &str = "/run/vzdumper/state.json";
pub const LISTEN_ADDR: &str = "0.0.0.0:8889";

/// Returns (cert_path, key_path, cert_mtime) if certs are found.
pub fn find_cert_files() -> Option<(&'static str, &'static str, u64)> {
    let candidates = [
        ("/etc/pve/local/pveproxy-ssl.pem", "/etc/pve/local/pveproxy-ssl.key"),
        ("/etc/pve/local/pve-ssl.pem", "/etc/pve/local/pve-ssl.key"),
    ];
    for (cert, key) in &candidates {
        if std::path::Path::new(cert).exists() && std::path::Path::new(key).exists() {
            if let Ok(meta) = std::fs::metadata(cert) {
                if let Ok(time) = meta.modified() {
                    if let Ok(mtime) = time.duration_since(std::time::UNIX_EPOCH) {
                        return Some((cert, key, mtime.as_secs()));
                    }
                }
            }
        }
    }
    None
}

/// Like `find_cert_files`, but retries on failure.
fn find_cert_files_with_retry(attempts: usize, delay_secs: u64) -> Option<(&'static str, &'static str, u64)> {
    for attempt in 0..attempts {
        if let Some(result) = find_cert_files() {
            return Some(result);
        }
        if attempt + 1 < attempts {
            eprintln!("Cert files not found, retry {}/{} in {}s...",
                attempt + 1, attempts, delay_secs);
            std::thread::sleep(Duration::from_secs(delay_secs));
        }
    }
    None
}

fn load_ssl_config() -> Option<SslConfig> {
    let (cert_path, key_path) = match find_cert_files_with_retry(6, 5) {
        Some((cert, key, _)) => (cert, key),
        None => {
            eprintln!("No TLS certificate found. Falling back to plain HTTP.");
            return None;
        }
    };

    let certificate = match retry_read_file(cert_path, 6, 5) {
        Some(c) => c,
        None => {
            eprintln!("Failed to read cert {} after retries. Falling back to plain HTTP.", cert_path);
            return None;
        }
    };
    let private_key = match retry_read_file(key_path, 6, 5) {
        Some(k) => k,
        None => {
            eprintln!("Failed to read key {} after retries. Falling back to plain HTTP.", key_path);
            return None;
        }
    };

    println!("Using TLS cert: {}", cert_path);
    Some(SslConfig { certificate, private_key })
}

fn retry_read_file(path: &str, attempts: usize, delay_secs: u64) -> Option<Vec<u8>> {
    for attempt in 0..attempts {
        match std::fs::read(path) {
            Ok(data) => return Some(data),
            Err(e) => {
                eprintln!("Read attempt {} for {} failed: {}. Retrying in {}s...",
                    attempt + 1, path, e, delay_secs);
                std::thread::sleep(Duration::from_secs(delay_secs));
            }
        }
    }
    None
}

pub fn run() {
    let ssl_config = load_ssl_config();
    let cert_mtime = find_cert_files().map(|(_, _, m)| m);

    // Spawn a thread that watches for certificate file changes.
    if let Some(mtime) = cert_mtime {
        std::thread::spawn(move || {
            let mut last_mtime = mtime;
            loop {
                std::thread::sleep(Duration::from_secs(30));
                if let Some((_, _, new_mtime)) = find_cert_files() {
                    if new_mtime != last_mtime {
                        println!("Certificate file changed (mtime {} -> {}), restarting GUI...",
                            last_mtime, new_mtime);
                        std::process::exit(0);
                    }
                    last_mtime = new_mtime;
                }
            }
        });
    }

    let server = match &ssl_config {
        Some(config) => Server::https(LISTEN_ADDR, config.clone())
            .expect("failed to bind HTTPS server"),
        None => {
            eprintln!("WARNING: Running without TLS. Use a reverse proxy if exposing publicly.");
            Server::http(LISTEN_ADDR)
                .expect("failed to bind HTTP server")
        }
    };
    let scheme = if ssl_config.is_some() { "https" } else { "http" };
    println!("[install] GUI available at {}://<proxmox-ip>:8889", scheme);
    
    for request in server.incoming_requests() {
        routes::handle_request(request, &ssl_config);
    }
}
