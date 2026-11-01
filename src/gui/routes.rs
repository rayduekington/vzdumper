use crate::gui::api;
use crate::gui::disk::{read_config, get_available_targets, write_config};
use crate::gui::templates::page;
use crate::gui::utils::url_decode;
use std::collections::HashMap;
use tiny_http::{Request, Response, Header, Method, SslConfig};

pub fn handle_request(mut request: Request, ssl_config: &Option<SslConfig>) {
    let method = request.method().clone();

    if method == Method::Get {
        let url = request.url().to_string();

        if url == "/state" {
            api::handle_state(request);
            return;
        }

        if url == "/tls" {
            api::handle_tls(request, ssl_config);
            return;
        }

        if url == "/disks" {
            api::handle_disks_scan(request);
            return;
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
        let url = request.url().to_string();

        if url == "/disks/use" {
            api::handle_disks_use(request);
            return;
        }

        if url == "/disks/format" {
            api::handle_disks_format(request);
            return;
        }

        let mut disk_uuids = Vec::new();
        let mut disk_labels = Vec::new();
        let mut disk_use_global = Vec::new();
        let mut disk_paused = Vec::new();
        let mut disk_target_lists: HashMap<usize, Vec<String>> = HashMap::new();
        let mut targets = Vec::new();
        let mut keep_last: u32 = 5;
        for pair in body.split('&') {
            if pair.is_empty() { continue; }
            let mut kv = pair.splitn(2, '=');
            let k = kv.next().unwrap_or("");
            let v = kv.next().unwrap_or("");
            let v = url_decode(&v.replace('+', " ")).trim().to_string();
            if k == "disk" {
                disk_uuids.push(v);
            } else if k.starts_with("disk_label_") {
                if let Ok(idx) = k.trim_start_matches("disk_label_").parse::<usize>() {
                    while disk_labels.len() <= idx { disk_labels.push(String::new()); }
                    disk_labels[idx] = v;
                }
            } else if k.starts_with("disk_paused_") {
                if let Ok(idx) = k.trim_start_matches("disk_paused_").parse::<usize>() {
                    while disk_paused.len() <= idx { disk_paused.push(false); }
                    disk_paused[idx] = true;
                }
            } else if k.starts_with("disk_use_global_") {
                if let Ok(idx) = k.trim_start_matches("disk_use_global_").parse::<usize>() {
                    while disk_use_global.len() <= idx { disk_use_global.push(false); }
                    if v == "1" || v == "on" || v == "true" {
                        disk_use_global[idx] = true;
                    } else {
                        disk_use_global[idx] = false;
                    }
                }
            } else if k.starts_with("disk_target_") {
                if let Some(idx_str) = k.strip_prefix("disk_target_") {
                    if let Ok(idx) = idx_str.parse::<usize>() {
                        if !v.is_empty() {
                            disk_target_lists.entry(idx).or_default().push(v);
                        }
                    }
                }
            } else if k == "target" && !v.is_empty() {
                targets.push(v);
            } else if k == "keep_last" {
                if let Ok(n) = v.parse::<u32>() {
                    keep_last = n.clamp(1, 99);
                }
            }
        }

        let mut disks = Vec::new();
        for (i, uuid) in disk_uuids.iter().enumerate() {
            if uuid.is_empty() { continue; }
            let label = disk_labels.get(i).cloned().filter(|l| !l.is_empty());
            let use_global = disk_use_global.get(i).cloned().unwrap_or(true);
            let paused = disk_paused.get(i).cloned().unwrap_or(false);
            let disk_targets = if use_global {
                None
            } else {
                let mut dt = disk_target_lists.get(&i).cloned().unwrap_or_default();
                if dt.is_empty() { dt.push("all".to_string()); }
                Some(dt)
            };
            disks.push((uuid.clone(), label, disk_targets, paused));
        }

        let available = get_available_targets();
        if disks.is_empty() {
            let (d, t, k) = read_config();
            let html = page(&d, &t, k, &available, "Error: at least one disk required");
            let _ = request.respond(
                Response::from_string(html).with_header(
                    Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap(),
                ),
            );
        } else if let Err(e) = write_config(&disks, &targets, keep_last) {
            let html = page(&disks, &targets, keep_last, &available, &format!("Error: {}", e));
            let _ = request.respond(
                Response::from_string(html).with_header(
                    Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap(),
                ),
            );
        } else {
            println!("Config saved: disks={:?}, targets={:?}, keep_last={}", disks, targets, keep_last);
            let _ = request.respond(
                Response::empty(303)
                    .with_header(Header::from_bytes(&b"Location"[..], &b"/"[..]).unwrap()),
            );
        }
    }
}
