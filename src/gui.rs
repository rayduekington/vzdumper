use tiny_http::{Server, Response, Header, Method};

const CONFIG_PATH: &str = "/root/.config/vzdumper/config.yaml";
const LISTEN_ADDR: &str = "0.0.0.0:8888";

fn read_uuid() -> String {
    match std::fs::read_to_string(CONFIG_PATH) {
        Ok(content) => {
            let yaml: serde_yaml::Value = serde_yaml::from_str(&content).unwrap_or_default();
            yaml.get("disk_uuid")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        }
        Err(_) => String::new(),
    }
}

fn write_uuid(uuid: &str) -> Result<(), String> {
    let dir = std::path::Path::new("/root/.config/vzdumper");
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    std::fs::write(CONFIG_PATH, format!("disk_uuid: \"{}\"\n", uuid))
        .map_err(|e| e.to_string())
}

fn read_state() -> (String, String, u64) {
    match std::fs::read_to_string("/run/vzdumper/state.json") {
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

fn page(current_uuid: &str, message: &str) -> String {
    let (state, detail, time) = read_state();

    let state_color = match state.as_str() {
        "backing_up" => "#e65100",
        "waiting_for_device" => "#1565c0",
        "waiting_for_removal" => "#2e7d32",
        "waiting_for_config" => "#666",
        _ => "#333",
    };

    let time_str = if time > 0 {
        let dt = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .saturating_sub(time);
        format!("{}s ago", dt)
    } else {
        "—".to_string()
    };

    format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta http-equiv="refresh" content="5">
<title>vzdumper</title>
<style>
body {{ font-family: system-ui, sans-serif; max-width: 520px; margin: 60px auto; padding: 0 20px; }}
h1 {{ font-size: 1.4rem; }}
label {{ display: block; margin-bottom: 6px; font-weight: 600; }}
input[type=text] {{ width: 100%; padding: 10px; font-size: 1rem; font-family: monospace; box-sizing: border-box; }}
button {{ margin-top: 14px; padding: 10px 24px; font-size: 1rem; cursor: pointer; }}
.msg {{ margin-top: 12px; color: #2e7d32; }}
.state {{ margin-top: 24px; padding: 12px 16px; border-radius: 8px; background: #f5f5f5; }}
.state .label {{ font-size: .8rem; color: #888; text-transform: uppercase; }}
.state .value {{ font-size: 1.1rem; font-weight: 600; color: {state_color}; }}
.state .detail {{ font-size: .85rem; color: #666; margin-top: 4px; font-family: monospace; }}
.state .time {{ font-size: .8rem; color: #999; margin-top: 4px; }}
</style>
</head>
<body>
<h1>vzdumper</h1>
<div class="state">
<div class="label">Status</div>
<div class="value">{state}</div>
<div class="detail">{detail}</div>
<div class="time">updated {time_str}</div>
</div>
<form method="POST" action="/" style="margin-top:24px">
<label for="uuid">Disk UUID</label>
<input type="text" id="uuid" name="uuid" value="{current_uuid}" placeholder="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx" required>
<br>
<button type="submit">Save</button>
</form>
{message}
<p style="margin-top:24px;color:#888;font-size:.85rem">Page auto-refreshes every 5 s.</p>
</body>
</html>"#)
}   

pub fn run() {
    let server = Server::http(LISTEN_ADDR).expect("failed to bind");
    println!("GUI listening on http://{} (Ctrl+C to stop)", LISTEN_ADDR);

    for mut request in server.incoming_requests() {
        let method = request.method().clone();

        if method == Method::Get {
            let uuid = read_uuid();
            let html = page(&uuid, "");
            let _ = request.respond(Response::from_string(html).with_header(
                Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap()
            ));
        } else if method == Method::Post {
            let mut body = String::new();
            let _ = request.as_reader().read_to_string(&mut body);

            // Parse form body: uuid=...
            let uuid = body
                .split('&')
                .find_map(|pair| {
                    let mut kv = pair.splitn(2, '=');
                    let k = kv.next()?;
                    let v = kv.next()?;
                    if k == "uuid" { Some(v.replace('+', " ").trim().to_string()) } else { None }
                })
                .unwrap_or_default();

            if uuid.is_empty() {
                let html = page("", "Error: empty UUID");
                let _ = request.respond(Response::from_string(html));
            } else if let Err(e) = write_uuid(&uuid) {
                let html = page(&uuid, &format!("Error: {}", e));
                let _ = request.respond(Response::from_string(html));
            } else {
                println!("UUID saved: {}", uuid);
                let html = page(&uuid, "Saved ✓");
                let _ = request.respond(Response::from_string(html));
            }
        }
    }
}   