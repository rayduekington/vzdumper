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

fn page(current_uuid: &str, message: &str) -> String {
    format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>vzdumper config</title>
<style>
body {{ font-family: system-ui, sans-serif; max-width: 480px; margin: 60px auto; padding: 0 20px; }}
h1 {{ font-size: 1.4rem; }}
label {{ display: block; margin-bottom: 6px; font-weight: 600; }}
input[type=text] {{ width: 100%; padding: 10px; font-size: 1rem; font-family: monospace; box-sizing: border-box; }}
button {{ margin-top: 14px; padding: 10px 24px; font-size: 1rem; cursor: pointer; }}
.msg {{ margin-top: 12px; color: #2e7d32; }}
</style>
</head>
<body>
<h1>vzdumper</h1>
<form method="POST" action="/">
<label for="uuid">Disk UUID</label>
<input type="text" id="uuid" name="uuid" value="{current_uuid}" placeholder="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx" required>
<br>
<button type="submit">Save</button>
</form>
{message}
<p style="margin-top:24px;color:#888;font-size:.85rem">The daemon picks up changes automatically (~10 s).</p>
</body>
</html>"#,
        current_uuid = current_uuid,
        message = if message.is_empty() { String::new() } else { format!("<div class=\"msg\">{}</div>", message) }
    )
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