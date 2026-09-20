use tiny_http::{Server, Response, Header, Method};

const CONFIG_PATH: &str = "/root/.config/vzdumper/config.yaml";
const STATE_PATH: &str = "/run/vzdumper/state.json";
const LISTEN_ADDR: &str = "0.0.0.0:8888";

fn read_disks() -> Vec<String> {
    match std::fs::read_to_string(CONFIG_PATH) {
        Ok(content) => {
            let v: serde_yaml::Value = serde_yaml::from_str(&content).unwrap_or_default();
            v.get("disks")
                .and_then(|d| d.as_sequence())
                .map(|seq| {
                    seq.iter()
                        .filter_map(|item| item.as_str().map(|s| s.to_string()))
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default()
        }
        Err(_) => Vec::new(),
    }
}

fn write_disks(disks: &[String]) -> Result<(), String> {
    let dir = std::path::Path::new("/root/.config/vzdumper");
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut yaml = String::from("disks:\n");
    for d in disks {
        yaml.push_str(&format!("  - \"{}\"\n", d));
    }
    std::fs::write(CONFIG_PATH, yaml).map_err(|e| e.to_string())
}

fn read_state() -> (String, String, u64) {
    match std::fs::read_to_string(STATE_PATH) {
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

fn page(disks: &[String], message: &str) -> String {
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

    let mut disk_rows = String::new();
for (_i, d) in disks.iter().enumerate() {   
            disk_rows.push_str(&format!(
            r#"<div class="disk-row"><input type="text" name="disk" value="{}" placeholder="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"><button type="button" onclick="this.parentElement.remove()">✕</button></div>"#,
            d
        ));
    }

    let msg_html = if message.is_empty() {
        String::new()
    } else {
        format!("<div class=\"msg\">{}</div>", message)
    };

    format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta http-equiv="refresh" content="5">
<title>vzdumper</title>
<style>
body {{ font-family: system-ui, sans-serif; max-width: 560px; margin: 60px auto; padding: 0 20px; }}
h1 {{ font-size: 1.4rem; }}
label {{ display: block; margin-bottom: 6px; font-weight: 600; }}
input[type=text] {{ width: 100%; padding: 10px; font-size: 1rem; font-family: monospace; box-sizing: border-box; }}
button[type=submit] {{ margin-top: 14px; padding: 10px 24px; font-size: 1rem; cursor: pointer; }}
button[type=button] {{ padding: 8px 12px; margin-left: 8px; cursor: pointer; border: 1px solid #ccc; background: #fff; border-radius: 4px; }}
.msg {{ margin-top: 12px; color: #2e7d32; }}
.state {{ margin-top: 24px; padding: 12px 16px; border-radius: 8px; background: #f5f5f5; }}
.state .label {{ font-size: .8rem; color: #888; text-transform: uppercase; }}
.state .value {{ font-size: 1.1rem; font-weight: 600; color: {state_color}; }}
.state .detail {{ font-size: .85rem; color: #666; margin-top: 4px; font-family: monospace; word-break: break-all; }}
.state .time {{ font-size: .8rem; color: #999; margin-top: 4px; }}
.disk-row {{ display: flex; align-items: center; margin-bottom: 8px; }}
.disk-row input {{ flex: 1; }}
.add-btn {{ margin-top: 8px; padding: 8px 16px; cursor: pointer; border: 1px dashed #999; background: #fafafa; border-radius: 4px; }}
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
<label>Backup Disks</label>
<div id="disks">{disk_rows}</div>
<button type="button" class="add-btn" onclick="addDisk()">+ Add disk</button>
<br>
<button type="submit">Save</button>
</form>
{msg_html}
<p style="margin-top:24px;color:#888;font-size:.85rem">Page auto-refreshes every 5 s. Daemon picks up changes within ~10 s.</p>
<script>
function addDisk() {{
  const div = document.createElement('div');
  div.className = 'disk-row';
  div.innerHTML = '<input type="text" name="disk" placeholder="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"><button type="button" onclick="this.parentElement.remove()">✕</button>';
  document.getElementById('disks').appendChild(div);
}}
</script>
</body>
</html>"#)
}

pub fn run() {
    let server = Server::http(LISTEN_ADDR).expect("failed to bind");
    println!("GUI listening on http://{} (Ctrl+C to stop)", LISTEN_ADDR);

    for mut request in server.incoming_requests() {
        let method = request.method().clone();

        if method == Method::Get {
            let disks = read_disks();
            let html = page(&disks, "");
            let _ = request.respond(
                Response::from_string(html).with_header(
                    Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap(),
                ),
            );
        } else if method == Method::Post {
            let mut body = String::new();
            let _ = request.as_reader().read_to_string(&mut body);

            let disks: Vec<String> = body
                .split('&')
                .filter_map(|pair| {
                    let mut kv = pair.splitn(2, '=');
                    let k = kv.next()?;
                    let v = kv.next()?;
                    if k == "disk" {
                        let v = v.replace('+', " ").trim().to_string();
                        if !v.is_empty() { Some(v) } else { None }
                    } else { None }
                })
                .collect();

            if disks.is_empty() {
                let html = page(&[], "Error: at least one disk required");
                let _ = request.respond(Response::from_string(html));
            } else if let Err(e) = write_disks(&disks) {
                let html = page(&disks, &format!("Error: {}", e));
                let _ = request.respond(Response::from_string(html));
            } else {
                println!("Disks saved: {:?}", disks);
                let html = page(&disks, "Saved ✓");
                let _ = request.respond(Response::from_string(html));
            }
        }
    }
}   