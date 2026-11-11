//! Live machine readers behind every Settings control.
use std::path::PathBuf;
use std::process::Command;

use agentic_ui_tree::Node;

use crate::{model, system};

pub(crate) fn set_theme(mode: &str) {
    if live_theme() != mode {
        system::run_action("theme.toggle");
    }
}

// ----------------------------------------------------------------- system

pub(crate) fn wallpaper_dir() -> PathBuf {
    // Rigs installed before the rename keep wallpapers under /usr/share/tiaki.
    for prefix in ["tiaki"] {
        let dir = PathBuf::from(format!("/usr/share/{prefix}/wallpapers"));
        if dir.is_dir() {
            return dir;
        }
    }
    PathBuf::from("/usr/share/tiaki/wallpapers")
}

pub(crate) fn wallpaper_names() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(wallpaper_dir())
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().to_string())
                .filter(|name| name.ends_with(".jpg"))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

pub(crate) fn current_wallpaper() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/desktop".to_string());
    std::fs::read_to_string(format!("{home}/.config/tiaki/wallpaper"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "drawn-01-ink-field.jpg".to_string())
}

pub(crate) fn set_wallpaper(name: &str) {
    system::spawn(&format!("/usr/local/bin/tiaki-wallpaper {name}"));
}

pub(crate) fn live_theme() -> String {
    model::http_get(
        "http://127.0.0.1:8787/tree.json",
        std::time::Duration::from_millis(700),
    )
    .and_then(|body| agentic_json::parse(&body).ok())
    .and_then(|value| {
        value
            .as_object()?
            .get("theme")?
            .as_str()
            .map(str::to_string)
    })
    .unwrap_or_else(|| "dark".to_string())
}

pub(crate) fn live_clock() -> String {
    let Some(body) = model::http_get(
        "http://127.0.0.1:8787/tree.json",
        std::time::Duration::from_millis(700),
    ) else {
        return "12h".to_string();
    };
    let Ok(value) = agentic_json::parse(&body) else {
        return "12h".to_string();
    };
    let Some(tree) = value.as_object().and_then(|object| object.get("tree")) else {
        return "12h".to_string();
    };
    let Ok(node) = Node::from_json(tree) else {
        return "12h".to_string();
    };
    model::find_by_id(&node, "bar.clock")
        .and_then(|node| node.prop_str("format"))
        .unwrap_or("12h")
        .to_string()
}

pub(crate) fn clock_format(root: &Node) -> String {
    model::find_by_id(root, "bar.clock")
        .and_then(|node| node.prop_str("format"))
        .unwrap_or("12h")
        .to_string()
}

// ------------------------------------------------------------- summaries

pub(crate) fn display_summary() -> String {
    let output = sway_output();
    format!("{} × {}", output.width, output.height)
}

pub(crate) fn power_summary() -> String {
    let rows = power_rows();
    if rows.is_empty() {
        return "AC · no battery".to_string();
    }
    rows.first()
        .map(|(key, value)| format!("{key} · {value}"))
        .unwrap_or_else(|| "AC".to_string())
}

pub(crate) fn storage_summary() -> String {
    disk_rows()
        .into_iter()
        .find(|(key, _)| key == "Used")
        .map(|(_, value)| format!("{value} used"))
        .unwrap_or_else(|| "Unknown".to_string())
}

pub(crate) fn network_summary() -> String {
    network_interfaces()
        .into_iter()
        .find(|interface| interface.connected)
        .map(|interface| format!("{} · {}", interface.state, interface.address))
        .unwrap_or_else(|| "No network".to_string())
}

// --------------------------------------------------------------- network

pub(crate) struct Interface {
    pub(crate) name: String,
    pub(crate) state: String,
    pub(crate) address: String,
    pub(crate) wireless: bool,
    pub(crate) connected: bool,
}

pub(crate) fn network_interfaces() -> Vec<Interface> {
    let Ok(entries) = std::fs::read_dir("/sys/class/net") else {
        return Vec::new();
    };
    let mut interfaces = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "lo" {
            continue;
        }
        let dir = entry.path();
        let operstate = read_trim(dir.join("operstate")).unwrap_or_else(|| "unknown".to_string());
        let mac = read_trim(dir.join("address")).unwrap_or_default();
        let wireless = dir.join("wireless").exists() || name.starts_with("wl");
        let connected = operstate == "up";
        let address = ipv4_for(&name).unwrap_or(mac);
        interfaces.push(Interface {
            name: name.to_uppercase(),
            state: if connected {
                "Connected".to_string()
            } else {
                "Disconnected".to_string()
            },
            address,
            wireless,
            connected,
        });
    }
    interfaces.sort_by(|left, right| {
        right
            .connected
            .cmp(&left.connected)
            .then(left.name.cmp(&right.name))
    });
    interfaces
}

pub(crate) fn ipv4_for(name: &str) -> Option<String> {
    let output = Command::new("ip")
        .args(["-4", "-o", "addr", "show", "dev", name])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let words: Vec<&str> = text.split_whitespace().collect();
    let position = words.iter().position(|word| *word == "inet")?;
    words
        .get(position + 1)
        .map(|value| value.split('/').next().unwrap_or(value).to_string())
}

pub(crate) fn network_properties() -> Vec<(String, String)> {
    let mut rows = vec![("Hostname".to_string(), hostname())];
    if let Some(interface) = network_interfaces().into_iter().next() {
        rows.push(("Interface".to_string(), interface.name));
        rows.push(("Address".to_string(), interface.address));
    }
    rows.push(("DNS".to_string(), dns_servers()));
    rows
}

pub(crate) fn dns_servers() -> String {
    std::fs::read_to_string("/etc/resolv.conf")
        .ok()
        .map(|text| {
            let servers: Vec<String> = text
                .lines()
                .filter_map(|line| line.strip_prefix("nameserver "))
                .map(|value| value.trim().to_string())
                .collect();
            if servers.is_empty() {
                "—".to_string()
            } else {
                servers.join(", ")
            }
        })
        .unwrap_or_else(|| "—".to_string())
}

// ------------------------------------------------------------- devices

pub(crate) fn usb_devices() -> Vec<String> {
    let Ok(entries) = std::fs::read_dir("/sys/bus/usb/devices") else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|entry| read_trim(entry.path().join("product")))
        .filter(|name| !name.contains("Host Controller") && !name.contains("Root Hub"))
        .collect();
    names.sort();
    names.dedup();
    names
}

pub(crate) fn input_devices() -> Vec<String> {
    let Ok(text) = std::fs::read_to_string("/proc/bus/input/devices") else {
        return Vec::new();
    };
    let mut names: Vec<String> = text
        .lines()
        .filter_map(|line| line.strip_prefix("N: Name="))
        .map(|name| name.trim().trim_matches('"').to_string())
        .filter(|name| !name.is_empty())
        .collect();
    names.sort();
    names.dedup();
    names
}

pub(crate) fn bluetooth_present() -> bool {
    std::fs::read_dir("/sys/class/bluetooth")
        .map(|mut entries| entries.next().is_some())
        .unwrap_or(false)
}

// -------------------------------------------------------------- time

pub(crate) fn timezone() -> String {
    read_trim(PathBuf::from("/etc/timezone"))
        .or_else(|| {
            Command::new("date")
                .arg("+%Z")
                .output()
                .ok()
                .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        })
        .unwrap_or_else(|| "—".to_string())
}

pub(crate) fn now_string() -> String {
    Command::new("date")
        .arg("+%A, %B %-d, %Y · %-I:%M %p")
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_else(|| "—".to_string())
}

pub(crate) fn now_clock() -> String {
    Command::new("date")
        .arg("+%-I:%M %p")
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_else(|| "now".to_string())
}

pub(crate) fn timesyncd_active() -> bool {
    Command::new("systemctl")
        .args(["is-active", "systemd-timesyncd"])
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim() == "active")
        .unwrap_or(false)
}

pub(crate) fn language() -> String {
    let raw = std::env::var("LANG")
        .ok()
        .filter(|value| !value.is_empty() && value != "C")
        .unwrap_or_else(|| "en_US.UTF-8".to_string());
    let base = raw.split('.').next().unwrap_or("en_US");
    let mut parts = base.split('_');
    let language = parts.next().unwrap_or("en");
    let region = parts.next().unwrap_or("US");
    let language = match language {
        "en" => "English",
        "de" => "German",
        "fr" => "French",
        "es" => "Spanish",
        "ja" => "Japanese",
        "zh" => "Chinese",
        "it" => "Italian",
        "pt" => "Portuguese",
        other => other,
    };
    let region = match region {
        "US" => "United States",
        "GB" => "United Kingdom",
        "DE" => "Germany",
        "FR" => "France",
        "ES" => "Spain",
        "JP" => "Japan",
        "CN" => "China",
        other => other,
    };
    format!("{language} ({region})")
}

pub(crate) fn region() -> String {
    std::env::var("LANG")
        .ok()
        .and_then(|value| value.split('_').nth(1).map(|value| value.to_string()))
        .map(|value| value.split('.').next().unwrap_or(&value).to_string())
        .unwrap_or_else(|| "US".to_string())
}

// ------------------------------------------------------- accessibility

pub(crate) fn gsettings_get(schema: &str, key: &str) -> Option<String> {
    let output = Command::new("gsettings")
        .args(["get", schema, key])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .trim_matches('\'')
            .to_string(),
    )
}

pub(crate) fn text_scale() -> String {
    gsettings_get("org.gnome.desktop.interface", "text-scaling-factor")
        .and_then(|value| value.parse::<f64>().ok())
        .map(|value| format!("{}%", (value * 100.0).round() as u32))
        .unwrap_or_else(|| "100%".to_string())
}

pub(crate) fn cursor_size() -> String {
    gsettings_get("org.gnome.desktop.interface", "cursor-size")
        .map(|value| format!("{value} px"))
        .unwrap_or_else(|| "24 px".to_string())
}

pub(crate) fn command_exists(name: &str) -> bool {
    Command::new("sh")
        .args(["-c", &format!("command -v {name}")])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

// ------------------------------------------------------------ machine

pub(crate) fn read_trim(path: PathBuf) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(crate) fn hostname() -> String {
    read_trim(PathBuf::from("/etc/hostname"))
        .or_else(|| std::env::var("HOSTNAME").ok())
        .unwrap_or_else(|| "tiaki".to_string())
}

pub(crate) fn cpu_model() -> String {
    std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|text| {
            text.lines()
                .find(|line| line.starts_with("model name"))
                .and_then(|line| line.split(':').nth(1))
                .map(|value| value.trim().to_string())
        })
        .unwrap_or_else(|| "Unknown".to_string())
}

pub(crate) fn memory_total() -> String {
    std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|text| {
            text.lines()
                .find(|line| line.starts_with("MemTotal:"))
                .and_then(|line| line.split_whitespace().nth(1))
                .and_then(|value| value.parse::<f64>().ok())
                .map(|kb| format!("{:.1} GiB", kb / 1024.0 / 1024.0))
        })
        .unwrap_or_else(|| "—".to_string())
}

pub(crate) fn os_pretty() -> String {
    std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|text| {
            text.lines()
                .find(|line| line.starts_with("PRETTY_NAME="))
                .map(|line| {
                    line.trim_start_matches("PRETTY_NAME=")
                        .trim_matches('"')
                        .to_string()
                })
        })
        .unwrap_or_else(|| "Linux".to_string())
}

pub(crate) fn kernel() -> String {
    Command::new("uname")
        .arg("-r")
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_else(|| "—".to_string())
}

pub(crate) fn disk_rows() -> Vec<(String, String)> {
    let Ok(output) = Command::new("df").args(["-h", "/"]).output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let Some(line) = text.lines().nth(1) else {
        return Vec::new();
    };
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 6 {
        return Vec::new();
    }
    vec![
        ("Filesystem".to_string(), parts[0].to_string()),
        ("Total".to_string(), parts[1].to_string()),
        (
            "Used".to_string(),
            format!("{} ({}%)", parts[2], parts[4].trim_end_matches('%')),
        ),
        ("Free".to_string(), parts[3].to_string()),
    ]
}

pub(crate) fn power_rows() -> Vec<(String, String)> {
    let mut rows = Vec::new();
    let Ok(entries) = std::fs::read_dir("/sys/class/power_supply") else {
        return rows;
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let kind = read_trim(dir.join("type")).unwrap_or_default();
        if kind == "Mains" {
            let online = read_trim(dir.join("online"))
                .map(|value| value == "1")
                .unwrap_or(false);
            rows.push((
                name,
                if online {
                    "Plugged in".to_string()
                } else {
                    "Unplugged".to_string()
                },
            ));
        } else if kind == "Battery" {
            let capacity = read_trim(dir.join("capacity"))
                .map(|value| format!("{value}%"))
                .unwrap_or_else(|| "—".to_string());
            let status = read_trim(dir.join("status")).unwrap_or_default();
            rows.push((name, format!("{capacity} · {status}")));
        }
    }
    rows
}

pub(crate) fn output_volume() -> Option<f64> {
    let output = Command::new("wpctl")
        .args(["get-volume", "@DEFAULT_AUDIO_SINK@"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.split_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<f64>().ok())
        .map(|value| (value * 100.0).clamp(0.0, 100.0))
}

pub(crate) fn backlight() -> Option<(PathBuf, f64, f64)> {
    let entries = std::fs::read_dir("/sys/class/backlight").ok()?;
    let dir: PathBuf = entries.flatten().map(|entry| entry.path()).next()?;
    let max: f64 = read_trim(dir.join("max_brightness"))?.parse().ok()?;
    let current: f64 = read_trim(dir.join("brightness"))?.parse().ok()?;
    Some((dir, current, max))
}

pub(crate) struct SwayOutput {
    pub(crate) name: String,
    pub(crate) width: i32,
    pub(crate) height: i32,
    pub(crate) scale: f64,
    pub(crate) modes: Vec<String>,
    pub(crate) orientation: String,
}

pub(crate) fn sway_output() -> SwayOutput {
    let fallback = SwayOutput {
        name: String::new(),
        width: 1440,
        height: 900,
        scale: 1.0,
        modes: vec!["1440x900".to_string()],
        orientation: "Landscape".to_string(),
    };
    let Ok(output) = Command::new("swaymsg").args(["-t", "get_outputs"]).output() else {
        return fallback;
    };
    let Ok(value) = agentic_json::parse(&String::from_utf8_lossy(&output.stdout)) else {
        return fallback;
    };
    let Some(first) = value
        .as_array()
        .and_then(|outputs| outputs.first())
        .and_then(|value| value.as_object())
    else {
        return fallback;
    };
    let name = first
        .get("name")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let rect = first.get("rect").and_then(|value| value.as_object());
    let width = rect
        .and_then(|rect| rect.get("width"))
        .and_then(|value| value.as_f64())
        .unwrap_or(1440.0) as i32;
    let height = rect
        .and_then(|rect| rect.get("height"))
        .and_then(|value| value.as_f64())
        .unwrap_or(900.0) as i32;
    let scale = first
        .get("scale")
        .and_then(|value| value.as_f64())
        .unwrap_or(1.0);
    let transform = first
        .get("transform")
        .and_then(|value| value.as_str())
        .unwrap_or("normal");
    let orientation = match transform {
        "90" | "270" | "flipped-90" | "flipped-270" => "Portrait",
        _ => "Landscape",
    }
    .to_string();

    let mut modes: Vec<String> = first
        .get("modes")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|mode| {
                    let mode = mode.as_object()?;
                    let width = mode.get("width")?.as_f64()? as i32;
                    let height = mode.get("height")?.as_f64()? as i32;
                    Some(format!("{width}x{height}"))
                })
                .collect()
        })
        .unwrap_or_default();
    modes.sort_by_key(|mode| {
        let mut parts = mode.split('x');
        let width: i32 = parts.next().unwrap_or("0").parse().unwrap_or(0);
        let height: i32 = parts.next().unwrap_or("0").parse().unwrap_or(0);
        (width, height)
    });
    modes.dedup();
    if modes.is_empty() {
        modes.push(format!("{width}x{height}"));
    }

    SwayOutput {
        name,
        width,
        height,
        scale,
        modes,
        orientation,
    }
}
