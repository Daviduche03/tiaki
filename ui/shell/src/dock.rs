//! The floating dock.
//!
//! Pinned applications, running indicators, and the separator come straight
//! from the `dock` panel in the tree. The dock floats above windows instead of
//! reserving space, like the macOS Dock.

use gtk::prelude::*;
use gtk_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::{icons, model, single, system, theme, windows};

/// Stylesheets owned by this module, in cascade order.
/// Add a file to `ui/shell/css/` and list it here to grow them.
pub const STYLE_SHEETS: &[&str] = &[include_str!("../css/dock.css")];

pub fn run(root: &agentic_ui_tree::Node) {
    // One dock process; a relaunch replaces it rather than stacking two.
    let Some(_instance) = single::Instance::acquire("tiaki-dock", "dock", true) else {
        return;
    };

    let dock = model::find_by_id(root, "dock")
        .or_else(|| model::find_by_role(root, "dock"))
        .unwrap_or(root);

    let window = theme::base_window("dock-window");
    theme::setup_layer(&window, "tiaki-dock", Layer::Top, KeyboardMode::None, 0);
    theme::anchor(&window, Edge::Bottom);
    window.set_layer_shell_margin(Edge::Bottom, 4);

    let panel = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    theme::class(&panel, "dock");
    panel.set_margin_top(12);
    panel.set_margin_bottom(12);
    panel.set_margin_start(12);
    panel.set_margin_end(12);

    for child in &dock.children {
        match child.kind.as_str() {
            "app" => {
                if let Some(app) = model::apps_in(&dock)
                    .into_iter()
                    .find(|app| app.id == child.id)
                {
                    panel.pack_start(&dock_item(&app), false, false, 0);
                }
            }
            "separator" => {
                panel.pack_start(
                    &icons::separator(gtk::Orientation::Vertical),
                    false,
                    false,
                    0,
                );
            }
            _ => {}
        }
    }

    window.add(&panel);
    window.show_all();

    // The launcher has its own dock; hide the desktop dock while it is open.
    let window_for_visibility = window.clone();
    gtk::glib::timeout_add_local(std::time::Duration::from_millis(250), move || {
        if launcher_open() {
            window_for_visibility.hide();
        } else {
            window_for_visibility.show();
        }
        gtk::glib::ControlFlow::Continue
    });

    gtk::main();
}

/// True when a live launcher process holds its lock (stale locks ignored).
fn launcher_open() -> bool {
    let base = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    let pid: Option<u32> = std::fs::read_to_string(format!("{base}/tiaki-launcher.lock/pid"))
        .ok()
        .and_then(|text| text.trim().parse().ok());
    let Some(pid) = pid else {
        return false;
    };
    std::fs::read_to_string(format!("/proc/{pid}/cmdline"))
        .map(|command| {
            let mut args = command.split('\0');
            let exe = args.next().unwrap_or_default();
            let is_shell =
                exe.ends_with("tiaki-shell") || exe.ends_with("tiaki-shell (deleted)");
            is_shell && command.split('\0').any(|arg| arg == "launcher")
        })
        .unwrap_or(false)
}

fn dock_item(app: &model::AppItem) -> gtk::Button {
    let button = gtk::Button::new();
    button.set_relief(gtk::ReliefStyle::None);
    theme::class(&button, "dock-item");
    button.set_tooltip_text(Some(&format!("{} · {}", app.label, app.category)));

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let tile = icons::app_icon(&app.art, 34);
    content.pack_start(&tile, false, false, 0);

    let dot = gtk::Label::new(Some("•"));
    theme::class(&dot, "run-dot");
    if !app.running {
        theme::class(&dot, "hidden");
    }
    content.pack_start(&dot, false, false, 0);

    button.add(&content);
    let command = app.command.clone();
    button.connect_clicked(move |_| {
        // macOS: clicking a running app brings its windows forward; clicking
        // a stopped app launches it. Multiple windows open the switcher
        // filtered to that app.
        let app_id = windows::app_id_for_command(&command);
        let app_windows: Vec<_> = windows::views()
            .into_iter()
            .filter(|view| view.app_id == app_id)
            .collect();
        match app_windows.len() {
            0 => {
                // Try restoring from scratchpad in case window was hidden with Super+H
                windows::sway_plain(&format!("[app_id=\"{app_id}\"] scratchpad show"));
                if windows::is_action(&command) {
                    system::run_action(command.trim_start_matches("tiaki:"));
                } else {
                    system::open_command(&command);
                }
            }
            1 => {
                // Focus and bring forward; if in scratchpad, scratchpad show
                windows::sway_plain(&format!("[con_id={}] scratchpad show", app_windows[0].id));
                windows::sway_plain(&format!("[con_id={}] focus", app_windows[0].id));
            }
            _ => system::open_switcher(Some(&app_id)),
        }
    });
    button
}
