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
        "waiting_for_config" => "#757575",
        "backup_complete" => "#2e7d32",
        "error" => "#d32f2f",
        _ => "#424242",
    };

    let state_icon = match state.as_str() {
        "backing_up" => "⏳",
        "waiting_for_device" => "💽",
        "waiting_for_removal" => "✅",
        "waiting_for_config" => "⚙️",
        "backup_complete" => "🎉",
        "error" => "⚠️",
        _ => "❓",
    };

    let time_str = if time > 0 {
        let dt = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .saturating_sub(time);
        if dt < 60 { format!("{}s ago", dt) }
        else if dt < 3600 { format!("{}m ago", dt / 60) }
        else { format!("{}h ago", dt / 3600) }
    } else {
        "—".to_string()
    };

    let mut disk_rows = String::new();
    for d in disks.iter() {
        disk_rows.push_str(&format!(
            r#"<div class="disk-row"><div class="input-container"><span class="input-label">UUID</span><input type="text" name="disk" value="{}" placeholder="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"><div class="input-underline"></div></div><button type="button" class="icon-btn" onclick="this.closest('.disk-row').remove()" title="Remove">✕</button></div>"#,
            d
        ));
    }

    let msg_html = if message.is_empty() {
        String::new()
    } else {
        format!("<div class=\"snackbar\">{}</div>", message)
    };

    format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="refresh" content="5">
<title>vzdumper</title>
<link href="https://fonts.googleapis.com/css2?family=Roboto:wght@400;500;700&family=Roboto+Mono&display=swap" rel="stylesheet">
<style>
* {{ box-sizing: border-box; margin: 0; padding: 0; }}

:root {{
  --bg: #fafafa;
  --card-bg: #fff;
  --text: #212121;
  --text-secondary: #757575;
  --text-muted: #9e9e9e;
  --app-bar-bg: #1976d2;
  --app-bar-text: #fff;
  --input-border: #e0e0e0;
  --input-focus: #1976d2;
  --btn-primary-bg: #1976d2;
  --btn-primary-text: #fff;
  --btn-text-color: #1976d2;
  --btn-text-border: #90caf9;
  --btn-text-hover: #e3f2fd;
  --icon-btn-hover: #eeeeee;
  --icon-btn-danger: #d32f2f;
  --snackbar-bg: #323232;
  --snackbar-text: #fff;
  --shadow: rgba(0,0,0,.12);
  --shadow-hover: rgba(0,0,0,.15);
}}
@media (prefers-color-scheme: dark) {{
  :root {{
    --bg: #121212;
    --card-bg: #1e1e1e;
    --text: #e0e0e0;
    --text-secondary: #9e9e9e;
    --text-muted: #757575;
    --app-bar-bg: #1565c0;
    --app-bar-text: #fff;
    --input-border: #424242;
    --input-focus: #64b5f6;
    --btn-primary-bg: #1976d2;
    --btn-primary-text: #fff;
    --btn-text-color: #64b5f6;
    --btn-text-border: #1976d2;
    --btn-text-hover: #1a2a3a;
    --icon-btn-hover: #2c2c2c;
    --icon-btn-danger: #ef5350;
    --snackbar-bg: #e0e0e0;
    --snackbar-text: #212121;
    --shadow: rgba(0,0,0,.3);
    --shadow-hover: rgba(0,0,0,.4);
  }}
}}
[data-theme="dark"] {{
  --bg: #121212;
  --card-bg: #1e1e1e;
  --text: #e0e0e0;
  --text-secondary: #9e9e9e;
  --text-muted: #757575;
  --app-bar-bg: #1565c0;
  --app-bar-text: #fff;
  --input-border: #424242;
  --input-focus: #64b5f6;
  --btn-primary-bg: #1976d2;
  --btn-primary-text: #fff;
  --btn-text-color: #64b5f6;
  --btn-text-border: #1976d2;
  --btn-text-hover: #1a2a3a;
  --icon-btn-hover: #2c2c2c;
  --icon-btn-danger: #ef5350;
  --snackbar-bg: #e0e0e0;
  --snackbar-text: #212121;
  --shadow: rgba(0,0,0,.3);
  --shadow-hover: rgba(0,0,0,.4);
}}
[data-theme="light"] {{
  --bg: #fafafa;
  --card-bg: #fff;
  --text: #212121;
  --text-secondary: #757575;
  --text-muted: #9e9e9e;
  --app-bar-bg: #1976d2;
  --app-bar-text: #fff;
  --input-border: #e0e0e0;
  --input-focus: #1976d2;
  --btn-primary-bg: #1976d2;
  --btn-primary-text: #fff;
  --btn-text-color: #1976d2;
  --btn-text-border: #90caf9;
  --btn-text-hover: #e3f2fd;
  --icon-btn-hover: #eeeeee;
  --icon-btn-danger: #d32f2f;
  --snackbar-bg: #323232;
  --snackbar-text: #fff;
  --shadow: rgba(0,0,0,.12);
  --shadow-hover: rgba(0,0,0,.15);
}}

body {{
  font-family: 'Roboto', system-ui, sans-serif;
  background: var(--bg);
  color: var(--text);
  min-height: 100vh;
  padding: 24px;
}}
.container {{ max-width: 520px; margin: 0 auto; }}

.app-bar {{
  position: relative;
  background: var(--app-bar-bg);
  color: var(--app-bar-text);
  padding: 16px 24px;
  border-radius: 12px;
  margin-bottom: 24px;
  box-shadow: 0 2px 4px var(--shadow);
}}
.app-bar h1 {{ font-size: 1.3rem; font-weight: 500; }}
.app-bar p {{ font-size: .85rem; opacity: .8; margin-top: 4px; }}

.theme-toggle {{
  position: absolute;
  top: 16px;
  right: 16px;
  width: 36px;
  height: 36px;
  border: none;
  border-radius: 50%;
  background: rgba(255,255,255,.15);
  color: #fff;
  font-size: 1.2rem;
  cursor: pointer;
  transition: background .2s;
}}
.theme-toggle:hover {{ background: rgba(255,255,255,.25); }}

.card {{
  background: var(--card-bg);
  border-radius: 12px;
  padding: 24px;
  margin-bottom: 16px;
  box-shadow: 0 1px 3px var(--shadow);
  transition: box-shadow .2s;
}}
.card:hover {{ box-shadow: 0 4px 8px var(--shadow-hover); }}
.card-title {{
  font-size: .8rem;
  font-weight: 500;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: .5px;
  margin-bottom: 16px;
}}

.state-row {{ display: flex; align-items: center; gap: 16px; }}
.state-icon {{ font-size: 2rem; }}
.state-info {{ flex: 1; }}
.state-value {{ font-size: 1.1rem; font-weight: 500; color: {state_color}; }}
.state-detail {{ font-size: .8rem; color: var(--text-secondary); margin-top: 4px; font-family: 'Roboto Mono', monospace; word-break: break-all; }}
.state-time {{ font-size: .75rem; color: var(--text-muted); margin-top: 4px; }}

.input-container {{ position: relative; margin-bottom: 20px; }}
.input-label {{
  position: absolute;
  left: 0;
  top: -8px;
  font-size: .7rem;
  color: var(--input-focus);
  background: var(--card-bg);
  padding: 0 4px;
}}
.input-container input {{
  width: 100%;
  padding: 12px 0;
  border: none;
  border-bottom: 2px solid var(--input-border);
  font-size: 1rem;
  font-family: 'Roboto Mono', monospace;
  background: transparent;
  outline: none;
  color: var(--text);
  transition: border-color .2s;
}}
.input-container input:focus {{ border-bottom-color: var(--input-focus); }}
.input-underline {{
  height: 2px;
  background: var(--input-focus);
  transform: scaleX(0);
  transition: transform .2s;
}}
.input-container input:focus ~ .input-underline {{ transform: scaleX(1); }}

.disk-row {{ display: flex; align-items: flex-end; gap: 8px; }}
.disk-row .input-container {{ flex: 1; }}

.btn {{
  display: inline-block;
  padding: 10px 24px;
  border: none;
  border-radius: 8px;
  font-size: .9rem;
  font-weight: 500;
  cursor: pointer;
  transition: background .2s, box-shadow .2s;
}}
.btn-primary {{
  background: var(--btn-primary-bg);
  color: var(--btn-primary-text);
  box-shadow: 0 2px 4px rgba(25,118,210,.3);
}}
.btn-primary:hover {{
  background: #1565c0;
  box-shadow: 0 4px 8px rgba(25,118,210,.4);
}}
.btn-text {{
  background: transparent;
  color: var(--btn-text-color);
  border: 1px dashed var(--btn-text-border);
  margin-top: 12px;
}}
.btn-text:hover {{ background: var(--btn-text-hover); }}

.icon-btn {{
  width: 36px;
  height: 36px;
  border: none;
  border-radius: 50%;
  background: transparent;
  color: var(--text-secondary);
  font-size: 1rem;
  cursor: pointer;
  transition: background .2s;
  flex-shrink: 0;
}}
.icon-btn:hover {{ background: var(--icon-btn-hover); color: var(--icon-btn-danger); }}

.snackbar {{
  margin-top: 16px;
  padding: 12px 16px;
  background: var(--snackbar-bg);
  color: var(--snackbar-text);
  border-radius: 8px;
  font-size: .85rem;
  box-shadow: 0 3px 6px rgba(0,0,0,.2);
}}

.footer {{
  text-align: center;
  font-size: .75rem;
  color: var(--text-muted);
  margin-top: 24px;
}}
</style>
</head>
<body>
<div class="container">

<div class="app-bar">
  <h1>vzdumper</h1>
  <p>Proxmox backup on disk insert</p>
  <button type="button" class="theme-toggle" onclick="toggleTheme()" title="Toggle theme">◐</button>
</div>

<div class="card">
  <div class="card-title">Status</div>
  <div class="state-row">
    <div class="state-icon">{state_icon}</div>
    <div class="state-info">
      <div class="state-value">{state}</div>
      <div class="state-detail">{detail}</div>
      <div class="state-time">updated {time_str}</div>
    </div>
  </div>
</div>

<div class="card">
  <div class="card-title">Backup Disks</div>
  <form method="POST" action="/">
    <div id="disks">{disk_rows}</div>
    <button type="button" class="btn btn-text" onclick="addDisk()">+ Add disk</button>
    <br><br>
    <button type="submit" class="btn btn-primary">Save</button>
  </form>
</div>

{msg_html}

<div class="footer">Auto-refreshes every 5 s · Daemon picks up changes within ~10 s</div>

</div>
<script>
function addDisk() {{
  const div = document.createElement('div');
  div.className = 'disk-row';
  div.innerHTML = '<div class="input-container"><span class="input-label">UUID</span><input type="text" name="disk" placeholder="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"><div class="input-underline"></div></div><button type="button" class="icon-btn" onclick="this.closest(\'.disk-row\').remove()">✕</button>';
  document.getElementById('disks').appendChild(div);
}}
function toggleTheme() {{
  const html = document.documentElement;
  const current = html.getAttribute('data-theme');
  const next = current === 'dark' ? 'light' : current === 'light' ? '' : 'dark';
  if (next) html.setAttribute('data-theme', next);
  else html.removeAttribute('data-theme');
  localStorage.setItem('vzdumper-theme', next || 'auto');
}}
(function() {{
  const saved = localStorage.getItem('vzdumper-theme');
  if (saved === 'dark') document.documentElement.setAttribute('data-theme', 'dark');
  else if (saved === 'light') document.documentElement.setAttribute('data-theme', 'light');
}})();
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