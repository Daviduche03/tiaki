//! Tiaki Shell — a tree-driven macOS-like desktop shell.
//!
//! The bar, dock, and launcher all render nodes from the same `/tree.json`
//! that the control plane owns. The tree is the desktop: transactions can
//! restyle, rearrange, add, or remove shell elements and every change is
//! attributable and reversible.
//!
//! Subcommands:
//!   tiaki-shell bar       top menu bar
//!   tiaki-shell dock      floating application dock
//!   tiaki-shell launcher  Spotlight-style launcher

mod agents;
mod bar;
mod chrome;
mod clock;
mod control;
mod dock;
mod files;
mod greet;
mod glyphs;
mod icons;
mod launcher;
mod menu;
mod model;
pub mod motion;
mod notify;
mod overview;
mod settings;
mod setup;
mod single;
mod switcher;
mod system;
mod theme;
mod windows;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().cloned().unwrap_or_else(|| "bar".to_string());

    if let Err(error) = gtk::init() {
        eprintln!("tiaki-shell: cannot start GTK: {error}");
        return ExitCode::FAILURE;
    }

    if !gtk_layer_shell::is_supported() {
        eprintln!("tiaki-shell: the Wayland compositor does not support wlr-layer-shell");
        return ExitCode::FAILURE;
    }

    theme::install();
    let tree = model::load_tree();

    match mode.as_str() {
        "bar" => bar::run(&tree),
        "chrome" => chrome::run(),
        "dock" => dock::run(&tree),
        "files" => {
            let start = args.get(1).map(String::as_str);
            files::run(start)
        }
        "greet" => greet::run(),
        "launcher" => launcher::run(&tree),
        "menu" => {
            let id = args.get(1).cloned().unwrap_or_default();
            let x = args
                .get(2)
                .and_then(|value| value.parse().ok())
                .unwrap_or(0);
            let app = args.get(3).cloned().unwrap_or_default();
            menu::run(&tree, &id, x, &app)
        }
        "setup" => setup::run(),
        "notifications" => {
            let x = args.get(1).and_then(|value| value.parse().ok()).unwrap_or(0);
            notify::run(x)
        }
        "control" => {
            let x = args
                .get(1)
                .and_then(|value| value.parse().ok())
                .unwrap_or(0);
            control::run(x)
        }
        "agents" => agents::run(&tree),
        "settings" => {
            let page = args.get(1).cloned().unwrap_or_default();
            settings::run(&tree, (!page.is_empty()).then_some(page.as_str()))
        }
        "zoom" => {
            chrome::zoom_focused();
            return ExitCode::SUCCESS;
        }
        "switcher" => {
            let rest = args.get(1..).unwrap_or(&[]);
            switcher::run(rest)
        }
        "overview" | "mission" => {
            overview::run();
        }
        "version" => {
            println!("tiaki-shell {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        other => {
            eprintln!("tiaki-shell: unknown mode '{other}' (bar, dock, launcher, menu, control, agents, settings, switcher, chrome, files)");
            return ExitCode::FAILURE;
        }
    }

    ExitCode::SUCCESS
}
