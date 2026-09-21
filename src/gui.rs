use tiny_http::{Server, Response, Header, Method, SslConfig};
use std::io::Read;

const CONFIG_PATH: &str = "/root/.config/vzdumper/config.yaml";
const STATE_PATH: &str = "/run/vzdumper/state.json";
const LISTEN_ADDR: &str = "0.0.0.0:8889";

fn read_config() -> (Vec<String>, Vec<String>, u32) {
    let mut disks = Vec::new();
    let mut targets = vec!["all".to_string()];
    let mut keep_last: u32 = 5;

    if let Ok(content) = std::fs::read_to_string(CONFIG_PATH) {
        let v: serde_yaml::Value = serde_yaml::from_str(&content).unwrap_or_default();
        if let Some(seq) = v.get("disks").and_then(|d| d.as_sequence()) {
            disks = seq.iter()
                .filter_map(|item| item.as_str().map(|s| s.to_string()))
                .filter(|s| !s.is_empty())
                .collect();
        }
        if let Some(t) = v.get("targets") {
            if let Some(s) = t.as_str() {
                targets = vec![s.to_string()];
            } else if let Some(seq) = t.as_sequence() {
                targets = seq.iter()
                    .filter_map(|item| item.as_str().map(|s| s.to_string()))
                    .filter(|s| !s.is_empty())
                    .collect();
            }
        }
        if let Some(k) = v.get("keep_last").and_then(|k| k.as_u64()) {
            keep_last = k as u32;
        }
    }
    (disks, targets, keep_last)
}

fn write_config(disks: &[String], targets: &[String], keep_last: u32) -> Result<(), String> {
    let dir = std::path::Path::new("/root/.config/vzdumper");
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut yaml = String::from("disks:\n");
    for d in disks {
        yaml.push_str(&format!("  - \"{}\"\n", d));
    }
    yaml.push_str("targets:\n");
    for t in targets {
        yaml.push_str(&format!("  - \"{}\"\n", t));
    }
    yaml.push_str(&format!("keep_last: {}\n", keep_last));
    std::fs::write(CONFIG_PATH, yaml).map_err(|e| e.to_string())
}

fn get_available_targets() -> Vec<String> {
    let mut result = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/etc/pve/qemu-server") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(id) = name.split('.').next() {
                if id.chars().all(|c| c.is_ascii_digit()) {
                    result.push(id.to_string());
                }
            }
        }
    }
    if let Ok(entries) = std::fs::read_dir("/etc/pve/containers") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.chars().all(|c| c.is_ascii_digit()) {
                result.push(name);
            }
        }
    }
    result.sort();
    result
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

fn state_color(state: &str) -> &'static str {
    match state {
        "backing_up" => "#e65100",
        "waiting_for_device" => "#1565c0",
        "waiting_for_removal" => "#2e7d32",
        "waiting_for_config" => "#757575",
        "backup_complete" => "#2e7d32",
        "error" => "#d32f2f",
        _ => "#424242",
    }
}

fn state_icon(state: &str) -> &'static str {
    match state {
        "backing_up" => "⏳",
        "waiting_for_device" => "💽",
        "waiting_for_removal" => "✅",
        "waiting_for_config" => "⚙️",
        "backup_complete" => "🎉",
        "error" => "⚠️",
        _ => "❓",
    }
}

fn time_ago(time: u64) -> String {
    if time == 0 { return "—".to_string(); }
    let dt = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap().as_secs().saturating_sub(time);
    if dt < 60 { format!("{}s ago", dt) }
    else if dt < 3600 { format!("{}m ago", dt / 60) }
    else { format!("{}h ago", dt / 3600) }
}

fn page(disks: &[String], targets: &[String], keep_last: u32, available: &[String], message: &str) -> String {
    let (state, detail, time) = read_state();
    let s_color = state_color(&state);
    let s_icon = state_icon(&state);
    let t_str = time_ago(time);
    let all_selected = if targets.iter().any(|t| t == "all") { "selected" } else { "" };

    let mut disk_rows = String::new();
    for d in disks.iter() {
        disk_rows.push_str(&format!(
            r#"<div class="disk-row"><div class="input-container"><span class="input-label">UUID</span><input type="text" name="disk" value="{}" placeholder="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"><div class="input-underline"></div></div><button type="button" class="icon-btn" onclick="this.closest('.disk-row').remove()" title="Remove">✕</button></div>"#,
            d
        ));
    }

    let mut available_chips = String::new();
    for a in available.iter() {
        let selected = targets.iter().any(|t| t == a);
        let cls = if selected { "chip selected" } else { "chip" };
        available_chips.push_str(&format!(
            r#"<span class="{}" onclick="toggleTarget(this, '{}')">{}</span>"#,
            cls, a, a
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
<title>vzdumper</title>
<link href="https://fonts.googleapis.com/css2?family=Roboto:wght@400;500;700&family=Roboto+Mono&display=swap" rel="stylesheet">
<style>
* {{ box-sizing: border-box; margin: 0; padding: 0; }}
:root {{
  --bg: #fafafa; --card-bg: #fff; --text: #212121; --text-secondary: #757575;
  --text-muted: #9e9e9e; --app-bar-bg: #1976d2; --app-bar-text: #fff;
  --input-border: #e0e0e0; --input-focus: #1976d2; --btn-primary-bg: #1976d2;
  --btn-primary-text: #fff; --btn-text-color: #1976d2; --btn-text-border: #90caf9;
  --btn-text-hover: #e3f2fd; --icon-btn-hover: #eeeeee; --icon-btn-danger: #d32f2f;
  --snackbar-bg: #323232; --snackbar-text: #fff; --shadow: rgba(0,0,0,.12);
  --shadow-hover: rgba(0,0,0,.15); --chip-bg: #e0e0e0; --chip-selected: #1976d2;
}}
@media (prefers-color-scheme: dark) {{
  :root {{
    --bg: #121212; --card-bg: #1e1e1e; --text: #e0e0e0; --text-secondary: #9e9e9e;
    --text-muted: #757575; --app-bar-bg: #1565c0; --input-border: #424242;
    --input-focus: #64b5f6; --btn-text-color: #64b5f6; --btn-text-border: #1976d2;
    --btn-text-hover: #1a2a3a; --icon-btn-hover: #2c2c2c; --icon-btn-danger: #ef5350;
    --snackbar-bg: #e0e0e0; --snackbar-text: #212121; --shadow: rgba(0,0,0,.3);
    --shadow-hover: rgba(0,0,0,.4); --chip-bg: #333; --chip-selected: #1976d2;
  }}
}}
[data-theme="dark"] {{
  --bg: #121212; --card-bg: #1e1e1e; --text: #e0e0e0; --text-secondary: #9e9e9e;
  --text-muted: #757575; --app-bar-bg: #1565c0; --input-border: #424242;
  --input-focus: #64b5f6; --btn-text-color: #64b5f6; --btn-text-border: #1976d2;
  --btn-text-hover: #1a2a3a; --icon-btn-hover: #2c2c2c; --icon-btn-danger: #ef5350;
  --snackbar-bg: #e0e0e0; --snackbar-text: #212121; --shadow: rgba(0,0,0,.3);
  --shadow-hover: rgba(0,0,0,.4); --chip-bg: #333; --chip-selected: #1976d2;
}}
[data-theme="light"] {{
  --bg: #fafafa; --card-bg: #fff; --text: #212121; --text-secondary: #757575;
  --text-muted: #9e9e9e; --app-bar-bg: #1976d2; --input-border: #e0e0e0;
  --input-focus: #1976d2; --btn-text-color: #1976d2; --btn-text-border: #90caf9;
  --btn-text-hover: #e3f2fd; --icon-btn-hover: #eeeeee; --icon-btn-danger: #d32f2f;
  --snackbar-bg: #323232; --snackbar-text: #fff; --shadow: rgba(0,0,0,.12);
  --shadow-hover: rgba(0,0,0,.15); --chip-bg: #e0e0e0; --chip-selected: #1976d2;
}}
body {{ font-family: 'Roboto', system-ui, sans-serif; background: var(--bg); color: var(--text); min-height: 100vh; padding: 24px; }}
.container {{ max-width: 560px; margin: 0 auto; }}
.app-bar {{ position: relative; background: var(--app-bar-bg); color: var(--app-bar-text); padding: 16px 24px; border-radius: 12px; margin-bottom: 24px; box-shadow: 0 2px 4px var(--shadow); }}
.app-bar h1 {{ font-size: 1.3rem; font-weight: 500; }}
.app-bar p {{ font-size: .85rem; opacity: .8; margin-top: 4px; }}
.theme-toggle {{ position: absolute; top: 16px; right: 16px; width: 36px; height: 36px; border: none; border-radius: 50%; background: rgba(255,255,255,.15); color: #fff; font-size: 1.2rem; cursor: pointer; transition: background .2s; }}
.theme-toggle:hover {{ background: rgba(255,255,255,.25); }}
.card {{ background: var(--card-bg); border-radius: 12px; padding: 24px; margin-bottom: 16px; box-shadow: 0 1px 3px var(--shadow); transition: box-shadow .2s; }}
.card:hover {{ box-shadow: 0 4px 8px var(--shadow-hover); }}
.card-title {{ font-size: .8rem; font-weight: 500; color: var(--text-secondary); text-transform: uppercase; letter-spacing: .5px; margin-bottom: 16px; }}
.state-row {{ display: flex; align-items: center; gap: 16px; }}
.state-icon {{ font-size: 2rem; }}
.state-info {{ flex: 1; }}
.state-value {{ font-size: 1.1rem; font-weight: 500; color: {s_color}; }}
.state-detail {{ font-size: .8rem; color: var(--text-secondary); margin-top: 4px; font-family: 'Roboto Mono', monospace; word-break: break-all; }}
.state-time {{ font-size: .75rem; color: var(--text-muted); margin-top: 4px; }}
.input-container {{ position: relative; margin-bottom: 20px; }}
.input-label {{ position: absolute; left: 0; top: -8px; font-size: .7rem; color: var(--input-focus); background: var(--card-bg); padding: 0 4px; }}
.input-container input {{ width: 100%; padding: 12px 0; border: none; border-bottom: 2px solid var(--input-border); font-size: 1rem; font-family: 'Roboto Mono', monospace; background: transparent; outline: none; color: var(--text); transition: border-color .2s; }}
.input-container input:focus {{ border-bottom-color: var(--input-focus); }}
.input-underline {{ height: 2px; background: var(--input-focus); transform: scaleX(0); transition: transform .2s; }}
.input-container input:focus ~ .input-underline {{ transform: scaleX(1); }}
.disk-row {{ display: flex; align-items: flex-end; gap: 8px; }}
.disk-row .input-container {{ flex: 1; }}
.btn {{ display: inline-block; padding: 10px 24px; border: none; border-radius: 8px; font-size: .9rem; font-weight: 500; cursor: pointer; transition: background .2s, box-shadow .2s; }}
.btn-primary {{ background: var(--btn-primary-bg); color: var(--btn-primary-text); box-shadow: 0 2px 4px rgba(25,118,210,.3); }}
.btn-primary:hover {{ background: #1565c0; box-shadow: 0 4px 8px rgba(25,118,210,.4); }}
.btn-text {{ background: transparent; color: var(--btn-text-color); border: 1px dashed var(--btn-text-border); margin-top: 12px; }}
.btn-text:hover {{ background: var(--btn-text-hover); }}
.icon-btn {{ width: 36px; height: 36px; border: none; border-radius: 50%; background: transparent; color: var(--text-secondary); font-size: 1rem; cursor: pointer; transition: background .2s; flex-shrink: 0; }}
.icon-btn:hover {{ background: var(--icon-btn-hover); color: var(--icon-btn-danger); }}
.snackbar {{ margin-top: 16px; padding: 12px 16px; background: var(--snackbar-bg); color: var(--snackbar-text); border-radius: 8px; font-size: .85rem; box-shadow: 0 3px 6px rgba(0,0,0,.2); }}
.footer {{ text-align: center; font-size: .75rem; color: var(--text-muted); margin-top: 24px; }}
.chips {{ display: flex; flex-wrap: wrap; gap: 8px; margin-bottom: 16px; }}
.chip {{ padding: 6px 14px; border-radius: 16px; background: var(--chip-bg); color: var(--text); font-size: .8rem; font-family: 'Roboto Mono', monospace; cursor: pointer; transition: background .2s, color .2s; }}
.chip:hover {{ opacity: .8; }}
.chip.selected {{ background: var(--chip-selected); color: #fff; }}
.section-divider {{ border: none; border-top: 1px solid var(--input-border); margin: 20px 0; }}
.keep-last-row {{ display: flex; align-items: center; gap: 12px; margin-top: 16px; }}
.keep-last-row label {{ font-size: .85rem; color: var(--text-secondary); font-weight: 500; }}
.keep-last-row input {{ width: 60px; padding: 8px; border: 1px solid var(--input-border); border-radius: 6px; font-size: 1rem; font-family: 'Roboto Mono', monospace; text-align: center; background: transparent; color: var(--text); outline: none; }}
.keep-last-row input:focus {{ border-color: var(--input-focus); }}
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
    <div class="state-icon">{s_icon}</div>
    <div class="state-info">
      <div class="state-value">{state}</div>
      <div class="state-detail">{detail}</div>
      <div class="state-time">updated {t_str}</div>
    </div>
  </div>
</div>

<div class="card">
  <div class="card-title">Backup Disks</div>
  <form method="POST" action="/" id="mainForm">
    <div id="disks">{disk_rows}</div>
    <button type="button" class="btn btn-text" onclick="addDisk()">+ Add disk</button>

    <hr class="section-divider">

    <div class="card-title" style="margin-top:8px">Backup Targets</div>
    <p style="font-size:.8rem;color:var(--text-muted);margin-bottom:12px">Click a chip to toggle. Use "all" to backup everything.</p>
    <div class="chips" id="chips">
      <span class="chip {all_selected}" onclick="toggleTarget(this, 'all')">all</span>
      {available_chips}
    </div>

    <hr class="section-divider">

    <div class="keep-last-row">
      <label for="keep_last">Keep last</label>
      <input type="number" id="keep_last" name="keep_last" value="{keep_last}" min="1" max="99">
      <span style="font-size:.8rem;color:var(--text-muted)">backups</span>
    </div>

    <br><br>
    <button type="submit" class="btn btn-primary">Save</button>
  </form>
</div>

{msg_html}

<div class="footer">Status updates every 5 s · Daemon picks up changes within ~10 s</div>

</div>
<script>
function addDisk() {{
  const div = document.createElement('div');
  div.className = 'disk-row';
  div.innerHTML = '<div class="input-container"><span class="input-label">UUID</span><input type="text" name="disk" placeholder="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"><div class="input-underline"></div></div><button type="button" class="icon-btn" onclick="this.closest(\'.disk-row\').remove()">✕</button>';
  document.getElementById('disks').appendChild(div);
}}
function toggleTarget(el, id) {{
  el.classList.toggle('selected');
  const chips = document.querySelectorAll('#chips .chip');
  if (id === 'all' && el.classList.contains('selected')) {{
    chips.forEach(c => {{ if (c !== el) c.classList.remove('selected'); }});
  }} else if (id !== 'all') {{
    const allChip = document.querySelector('#chips .chip:first-child');
    if (allChip) allChip.classList.remove('selected');
  }}
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

document.getElementById('mainForm').addEventListener('submit', function(e) {{
  this.querySelectorAll('input[name="target"][data-chip]').forEach(el => el.remove());
  const selected = document.querySelectorAll('#chips .chip.selected');
  const allSelected = selected.length > 0 && selected[0].textContent === 'all';
  const chips = allSelected ? [selected[0]] : Array.from(selected);
  if (chips.length === 0) {{
    const input = document.createElement('input');
    input.type = 'hidden'; input.name = 'target'; input.value = 'all';
    input.setAttribute('data-chip', '1');
    this.appendChild(input);
  }} else {{
    chips.forEach(chip => {{
      const input = document.createElement('input');
      input.type = 'hidden'; input.name = 'target'; input.value = chip.textContent;
      input.setAttribute('data-chip', '1');
      this.appendChild(input);
    }});
  }}
}});

let lastState = '{s_icon}|{state}|{detail}|{t_str}';
setInterval(async () => {{
  try {{
    const r = await fetch('/state');
    const d = await r.json();
    const key = d.icon + '|' + d.state + '|' + d.detail + '|' + d.time;
    if (key !== lastState) {{
      lastState = key;
      document.querySelector('.state-icon').textContent = d.icon;
      document.querySelector('.state-value').textContent = d.state;
      document.querySelector('.state-value').style.color = d.color;
      document.querySelector('.state-detail').textContent = d.detail;
      document.querySelector('.state-time').textContent = 'updated ' + d.time;
    }}
  }} catch(e) {{}}
}}, 5000);
</script>
</body>
</html>"#)
}

fn load_ssl_config() -> Option<SslConfig> {
    let (cert_path, key_path) = if std::path::Path::new("/etc/pve/local/pveproxy-ssl.pem").exists() {
        ("/etc/pve/local/pveproxy-ssl.pem", "/etc/pve/local/pveproxy-ssl.key")
    } else if std::path::Path::new("/etc/pve/local/pve-ssl.pem").exists() {
        ("/etc/pve/local/pve-ssl.pem", "/etc/pve/local/pve-ssl.key")
    } else {
        eprintln!("No TLS certificate found. Falling back to plain HTTP.");
        return None;
    };

    let certificate = match std::fs::read(cert_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to read cert {}: {}. Falling back to plain HTTP.", cert_path, e);
            return None;
        }
    };
    let private_key = match std::fs::read(key_path) {
        Ok(k) => k,
        Err(e) => {
            eprintln!("Failed to read key {}: {}. Falling back to plain HTTP.", key_path, e);
            return None;
        }
    };

    println!("Using TLS cert: {}", cert_path);
    Some(SslConfig { certificate, private_key })
}

pub fn run() {
    let ssl_config = load_ssl_config();
    let server = match &ssl_config {
        Some(config) => Server::https(LISTEN_ADDR, config.clone())
            .expect("failed to bind HTTPS server"),
        None => Server::http(LISTEN_ADDR)
            .expect("failed to bind HTTP server"),
    };
    let scheme = if ssl_config.is_some() { "https" } else { "http" };
    println!("[install] GUI available at http(s)://<proxmox-ip>:8889");
    for mut request in server.incoming_requests() {
        let method = request.method().clone();

        if method == Method::Get {
            let url = request.url().to_string();

            if url == "/state" {
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
                continue;
            }

            let (disks, targets, keep_last) = read_config();
            let available = get_available_targets();
            let html = page(&disks, &targets, keep_last, &available, "");
            let _ = request.respond(
                Response::from_string(html).with_header(
                    Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap(),
                ),
            );
        } else if method == Method::Post {
            let mut body = String::new();
            let _ = request.as_reader().read_to_string(&mut body);

            let mut disks = Vec::new();
            let mut targets = Vec::new();
            let mut keep_last: u32 = 5;
            for pair in body.split('&') {
                let mut kv = pair.splitn(2, '=');
                let k = kv.next().unwrap_or("");
                let v = kv.next().unwrap_or("");
                let v = v.replace('+', " ").trim().to_string();
                if k == "disk" && !v.is_empty() {
                    disks.push(v);
                } else if k == "target" && !v.is_empty() {
                    targets.push(v);
                } else if k == "keep_last" {
                    if let Ok(n) = v.parse::<u32>() {
                        keep_last = n.clamp(1, 99);
                    }
                }
            }

            let available = get_available_targets();
            let html = if disks.is_empty() {
                let (d, t, k) = read_config();
                page(&d, &t, k, &available, "Error: at least one disk required")
            } else if let Err(e) = write_config(&disks, &targets, keep_last) {
                page(&disks, &targets, keep_last, &available, &format!("Error: {}", e))
            } else {
                println!("Config saved: disks={:?}, targets={:?}, keep_last={}", disks, targets, keep_last);
                page(&disks, &targets, keep_last, &available, "Saved ✓")
            };

            let _ = request.respond(
                Response::from_string(html).with_header(
                    Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap(),
                ),
            );
        }
    }
}