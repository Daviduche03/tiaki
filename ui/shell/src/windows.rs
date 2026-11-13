//! Live Sway window model shared by the window manager overlay, the switcher,
//! and the dock.
//!
//! Everything here is read-only observation plus a small set of `swaymsg`
//! commands. No compositor state is cached beyond what a caller asks for.

use std::process::Command;

use agentic_json::Json;

#[derive(Clone, Debug)]
pub struct View {
    pub id: i64,
    pub name: String,
    pub app_id: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub floating: bool,
    pub focused: bool,
}

pub fn sway_command() -> Command {
    let mut command = Command::new("swaymsg");
    if let Some(socket) = resolve_socket() {
        command.env("SWAYSOCK", socket);
    }
    command
}

/// The IPC socket: `SWAYSOCK` when it points at a live socket, otherwise the
/// newest `sway-ipc.*` socket in the runtime directory. Launchers do not
/// always inherit Sway's environment, so discovery must not depend on it.
fn resolve_socket() -> Option<std::path::PathBuf> {
    if let Some(value) = std::env::var_os("SWAYSOCK") {
        let path = std::path::PathBuf::from(value);
        if path.exists() {
            return Some(path);
        }
    }
    let runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/run/tiaki".to_string());
    let mut best: Option<(std::time::SystemTime, std::path::PathBuf)> = None;
    for entry in std::fs::read_dir(runtime).ok()?.flatten() {
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with("sway-ipc.") {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        if best
            .as_ref()
            .map(|(time, _)| modified > *time)
            .unwrap_or(true)
        {
            best = Some((modified, entry.path()));
        }
    }
    best.map(|(_, path)| path)
}

/// Run a swaymsg command on a specific container.
pub fn sway(id: i64, command: &str) {
    let _ = sway_command()
        .arg(format!("[con_id={id}] {command}"))
        .spawn();
}

/// Top-left of the workspace's content area. Sway's `move position` is
/// relative to this origin, not the output, because gaps and the bar shift
/// the workspace rect.
pub fn workspace_origin() -> (i32, i32) {
    let Some(value) = tree() else {
        return (0, 0);
    };
    fn find(node: &Json) -> Option<(i32, i32)> {
        let object = node.as_object()?;
        // The tree carries a hidden `__i3` scratch output whose workspace
        // sits at the origin. It is not the visible workspace.
        if object
            .get("name")
            .and_then(Json::as_str)
            .map(|name| name.starts_with("__i3"))
            .unwrap_or(false)
        {
            return None;
        }
        if object.get("type").and_then(Json::as_str) == Some("workspace") {
            let rect = object.get("rect")?.as_object()?;
            let x = rect.get("x")?.as_f64()? as i32;
            let y = rect.get("y")?.as_f64()? as i32;
            return Some((x, y));
        }
        for key in ["nodes", "floating_nodes"] {
            if let Some(children) = object.get(key).and_then(Json::as_array) {
                for child in children {
                    if let Some(origin) = find(child) {
                        return Some(origin);
                    }
                }
            }
        }
        None
    }
    find(&value).unwrap_or((0, 0))
}

/// Move a floating window to an absolute output position.
pub fn move_absolute(id: i64, x: i32, y: i32) {
    let (ox, oy) = workspace_origin();
    sway(id, &format!("move position {} {}", x - ox, y - oy));
}

pub fn sway_plain(command: &str) {
    let _ = sway_command().arg(command).spawn();
}

pub fn sway_query(query_type: &str) -> Option<Json> {
    let output = sway_command().args(["-t", query_type]).output().ok()?;
    agentic_json::parse(&String::from_utf8_lossy(&output.stdout)).ok()
}

/// Query the currently focused workspace number (defaults to 1).
pub fn focused_workspace() -> i32 {
    let Some(value) = sway_query("get_workspaces") else {
        return 1;
    };
    if let Some(workspaces) = value.as_array() {
        for ws in workspaces {
            if let Some(object) = ws.as_object() {
                if object.get("focused").and_then(Json::as_bool) == Some(true) {
                    if let Some(num) = object.get("num").and_then(Json::as_f64) {
                        if num > 0.0 {
                            return num as i32;
                        }
                    }
                    if let Some(name) = object.get("name").and_then(Json::as_str) {
                        if let Ok(num) = name.parse::<i32>() {
                            return num;
                        }
                    }
                }
            }
        }
    }
    1
}

/// Which app has focus, lowercased: `foot`, `mousepad`, `thunar`,
/// `epiphany`, or empty when the desktop has focus. The bar polls this;
/// menu popups read it once at click time.
pub fn focused_app_id() -> String {
    views()
        .iter()
        .find(|view| view.focused)
        .map(|view| view.app_id.to_lowercase())
        .unwrap_or_default()
}

fn tree() -> Option<Json> {
    sway_query("get_tree")
}

pub fn views() -> Vec<View> {
    let Some(value) = tree() else {
        return Vec::new();
    };
    let mut views = Vec::new();
    collect(&value, &mut views);
    views
}

/// Output size, so the window manager can compute zoom targets without
/// guessing the screen.
pub fn output_size() -> (i32, i32) {
    let Some(value) = tree() else {
        return (1440, 900);
    };
    fn find(node: &Json) -> Option<(i32, i32)> {
        let object = node.as_object()?;
        if object
            .get("name")
            .and_then(Json::as_str)
            .map(|name| name.starts_with("__i3"))
            .unwrap_or(false)
        {
            return None;
        }
        if object.get("type").and_then(Json::as_str) == Some("output") {
            let rect = object.get("rect")?.as_object()?;
            let width = rect.get("width")?.as_f64()? as i32;
            let height = rect.get("height")?.as_f64()? as i32;
            if width > 0 && height > 0 {
                return Some((width, height));
            }
        }
        for key in ["nodes", "floating_nodes"] {
            if let Some(children) = object.get(key).and_then(Json::as_array) {
                for child in children {
                    if let Some(size) = find(child) {
                        return Some(size);
                    }
                }
            }
        }
        None
    }
    find(&value).unwrap_or((1440, 900))
}

fn collect(value: &Json, views: &mut Vec<View>) {
    let Some(object) = value.as_object() else {
        return;
    };
    let kind = object.get("type").and_then(Json::as_str);
    if matches!(kind, Some("con") | Some("floating_con")) {
        let app_id = object.get("app_id").and_then(Json::as_str).or_else(|| {
            object
                .get("window_properties")
                .and_then(Json::as_object)
                .and_then(|properties| properties.get("class"))
                .and_then(Json::as_str)
        });
        if let (Some(app_id), Some(rect)) = (app_id, object.get("rect").and_then(Json::as_object)) {
            let name = object.get("name").and_then(Json::as_str).unwrap_or("");
            let id = object.get("id").and_then(Json::as_f64).unwrap_or(0.0) as i64;
            let x = rect.get("x").and_then(Json::as_f64).unwrap_or(0.0) as i32;
            let y = rect.get("y").and_then(Json::as_f64).unwrap_or(0.0) as i32;
            let width = rect.get("width").and_then(Json::as_f64).unwrap_or(0.0) as i32;
            let height = rect.get("height").and_then(Json::as_f64).unwrap_or(0.0) as i32;
            let visible = object
                .get("visible")
                .and_then(Json::as_bool)
                .unwrap_or(true);
            let fullscreen = object
                .get("fullscreen_mode")
                .and_then(Json::as_f64)
                .unwrap_or(0.0) as i32;
            let focused = object
                .get("focused")
                .and_then(Json::as_bool)
                .unwrap_or(false);
            let floating = object
                .get("floating")
                .and_then(Json::as_str)
                .map(|value| value != "auto_off")
                .unwrap_or(false);
            if visible && fullscreen == 0 && width > 0 && height > 0 {
                views.push(View {
                    id,
                    name: name.to_string(),
                    app_id: app_id.to_string(),
                    x,
                    y,
                    width,
                    height,
                    floating,
                    focused,
                });
            }
        }
    }

    for key in ["nodes", "floating_nodes"] {
        if let Some(children) = object.get(key).and_then(Json::as_array) {
            for child in children {
                collect(child, views);
            }
        }
    }
}

/// The display title for a window: its name, or a friendly app name.
pub fn title(view: &View) -> String {
    let name = view.name.trim();
    if name.is_empty() || name == view.app_id {
        match view.app_id.as_str() {
            "foot" => "Terminal".to_string(),
            "mousepad" => "Editor".to_string(),
            "btop" => "Monitor".to_string(),
            "thunar" => "Files".to_string(),
            "epiphany" => "Browser".to_string(),
            other => other.to_string(),
        }
    } else {
        name.to_string()
    }
}

/// Best-effort map from a launch command (or tree action) to the app_id the
/// window will carry.
pub fn app_id_for_command(command: &str) -> String {
    let command = command.strip_prefix("tiaki:").unwrap_or(command);
    match command {
        "terminal.new" => return "foot".to_string(),
        "files.open" => return "tiaki-shell".to_string(),
        "trash.open" => return "tiaki-shell".to_string(),
        "editor.open" => return "mousepad".to_string(),
        "monitor.open" => return "btop".to_string(),
        "settings.open" => return "tiaki-shell".to_string(),
        _ => {}
    }
    let first = command.split_whitespace().next().unwrap_or("");
    let base = first.rsplit('/').next().unwrap_or(first);
    match base {
        "tiaki-shell" => "tiaki-shell".to_string(),
        "foot" => "foot".to_string(),
        "thunar" => "thunar".to_string(),
        "mousepad" => "mousepad".to_string(),
        "epiphany" => "epiphany".to_string(),
        "btop" => "btop".to_string(),
        other => other.to_string(),
    }
}

/// True when the command names a tree action the shell knows how to run
/// (as opposed to a literal executable path).
pub fn is_action(command: &str) -> bool {
    matches!(
        command.strip_prefix("tiaki:").unwrap_or(command),
        "terminal.new"
            | "files.open"
            | "editor.open"
            | "monitor.open"
            | "settings.open"
            | "agents.open"
            | "transactions.open"
            | "palette.open"
            | "wallpaper.next"
            | "trash.open"
            | "session.exit"
    )
}
