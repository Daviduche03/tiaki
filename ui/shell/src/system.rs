//! What the shell can do: launch apps, dispatch tree actions, query the
//! control plane. Nothing here is privileged; the authority layer still owns
//! every state change.

use std::path::Path;
use std::time::Duration;

use crate::{model, windows};

const ACTION_URL: &str = "http://127.0.0.1:8787/action?name=";
/// Present in the desktop image. When it is missing (host dev builds) apps
/// launch plainly rather than failing.
const LAUNCHER: &str = "/usr/local/bin/sched-launch";

pub fn spawn(command: &str) {
    let command = command.trim();
    if command.is_empty() {
        return;
    }
    if let Err(error) = gtk::glib::spawn_command_line_async(command) {
        eprintln!("tiaki-shell: cannot launch '{command}': {error}");
    }
}

fn launcher_available() -> bool {
    Path::new(LAUNCHER).exists()
}

/// Launch an app through the scheduler so it gets a cgroup and a share of the
/// machine. Control commands (swaymsg, shell popups) keep using `spawn`.
pub fn spawn_app(command: &str, class: &str) {
    let command = command.trim();
    if command.is_empty() {
        return;
    }
    if launcher_available() {
        spawn(&format!("{LAUNCHER} --class {class} -- {command}"));
    } else {
        spawn(command);
    }
}

/// The running binary's path, without the kernel's ` (deleted)` marker that
/// appears after the file was replaced on disk while still mapped.
pub fn self_exe() -> String {
    match std::env::current_exe() {
        Ok(path) => {
            let display = path.display().to_string();
            display
                .strip_suffix(" (deleted)")
                .unwrap_or(&display)
                .to_string()
        }
        Err(_) => "tiaki-shell".to_string(),
    }
}

fn launch_self(mode: &str) {
    spawn_app(&format!("{} {mode}", self_exe()), "foreground");
}

/// Ask the control plane to run a tree action. The ledger records it.
pub fn agentic_action(name: &str) -> Option<String> {
    let url = format!("{ACTION_URL}{name}");
    model::http_get(&url, Duration::from_secs(2))
}

/// Map a tree `action` to a shell behavior.
pub fn run_action(action: &str) {
    match action {
        "" | "noop" => {}
        "palette.open" => launch_self("launcher"),
        "notifications.open" => launch_self("notifications"),
        "notifications.clear" => crate::notify::clear_all(),
        "control.open" => launch_self("control"),
        "settings.open" => launch_self("settings"),
        other if other.starts_with("settings.page.") => {
            spawn_app(&format!("{} settings {other}", self_exe()), "foreground")
        }
        "terminal.new" => spawn_app("foot -c /etc/tiaki/foot.ini", "foreground"),
        "files.open" => launch_self("files"),
        "trash.open" => spawn_app(&format!("{} files trash:", self_exe()), "foreground"),
        "browser.open" => spawn_app("epiphany", "foreground"),
        // T3 Code is the agent app. The dev runner's ports are pinned in
        // tiaki-t3.sh, so this URL is stable for the life of the image.
        "t3.open" => spawn_app("tiaki-t3-open", "foreground"),
        "editor.open" => spawn_app("mousepad /home/desktop/demo.rs", "foreground"),
        "monitor.open" => spawn_app("foot -c /etc/tiaki/foot.ini -e btop", "foreground"),
        "update.check" => launch_self("settings"),
        "transactions.open" => spawn_app(
            "foot -c /etc/tiaki/foot.ini -e /usr/local/bin/agentd log",
            "foreground",
        ),
        "window.close" => spawn("swaymsg kill"),
        "window.floating" => spawn("swaymsg floating toggle"),
        "layout.splith" => spawn("swaymsg splith"),
        "layout.splitv" => spawn("swaymsg splitv"),
        "layout.tabbed" => spawn("swaymsg layout tabbed"),
        "layout.stacking" => spawn("swaymsg layout stacking"),
        "layout.toggle" => spawn("swaymsg layout toggle split"),
        "session.lock" => spawn("swaylock -f"),
        "window.restore" => spawn("swaymsg scratchpad show"),
        "session.exit" => spawn("swaymsg exit"),
        "view.fullscreen" => spawn("swaymsg fullscreen toggle"),
        "window.zoom" => launch_self("zoom"),
        "windows.list" => open_switcher(None),
        "windows.cycle" => open_switcher(Some("focused")),
        "wallpaper.next" => spawn_app("/usr/local/bin/tiaki-wallpaper next", "background"),
        // Global edit keys: forwarded to the focused Wayland window via
        // wtype's virtual keyboard. Foot also accepts Shift+Insert, so
        // paste sends both — one lands per app, the other is a no-op.
        // Without wtype installed these are best-effort no-ops.
        "edit.undo" => spawn("wtype -M ctrl z -m ctrl"),
        "edit.redo" => spawn("wtype -M ctrl -M shift z -m shift -m ctrl"),
        "edit.copy" => spawn("wtype -M ctrl c -m ctrl"),
        "edit.paste" => {
            spawn("wtype -M ctrl v -m ctrl");
            spawn("wtype -M shift Insert -m shift");
        }
        "theme.toggle" | "clock.toggle_format" | "ui.reload" | "rollback" | "mode.guarded" => {
            let _ = agentic_action(action);
        }
        other if other.starts_with("tiaki:") => {
            let _ = agentic_action(&other["tiaki:".len()..]);
        }
        other if other.starts_with("exec:") => spawn(&other["exec:".len()..]),
        _ => {}
    }
}

/// Open the window switcher, optionally filtered to one app id.
pub fn open_switcher(filter: Option<&str>) {
    let exe = self_exe();
    match filter {
        Some(app) => spawn_app(&format!("{exe} switcher {app}"), "foreground"),
        None => spawn_app(&format!("{exe} switcher"), "foreground"),
    }
}

/// Open an app node's command through the scheduler.
pub fn open_command(command: &str) {
    if let Some(action) = command.strip_prefix("tiaki:") {
        let _ = agentic_action(action);
        return;
    }
    match command {
        "" => {}
        "wallpaper.next" => run_action("wallpaper.next"),
        "session.exit" => run_action("session.exit"),
        "palette.open" => launch_self("launcher"),
        "notifications.open" => launch_self("notifications"),
        other if windows::is_action(other) => run_action(other),
        other => {
            let class = if other.contains("tiaki-wallpaper") {
                "background"
            } else {
                "foreground"
            };
            spawn_app(other, class);
        }
    }
}
