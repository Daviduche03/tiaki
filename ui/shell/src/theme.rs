//! Design tokens, window helpers, and the stylesheet registry.
//!
//! One palette, one type ramp, one radius scale. The styles themselves live
//! in `ui/shell/css/`, one file per module, embedded at compile time: an
//! agent restyles the desktop by transacting on CSS files without touching
//! Rust. Each shell module declares its own `STYLE_SHEETS` list; `install`
//! concatenates the base sheet with every module's sheets in cascade order.

use gtk::prelude::*;
use gtk_layer_shell::LayerShell;

use crate::{
    bar, chrome, control, dock, files, greet, launcher, menu, notify, overview, settings, setup,
    switcher,
};

/// Height of the top bar and its exclusive zone. The titlebar overlay and
/// window zoom position themselves against this, so it lives in one place.
pub const BAR_HEIGHT: i32 = 30;

/// Brand colours as cairo RGB, for the few surfaces painted in code rather
/// than CSS. Values mirror `css/tokens.css`.
pub const INK: (f64, f64, f64) = (0.082, 0.078, 0.075);
pub const PAPER: (f64, f64, f64) = (0.95, 0.96, 0.98);
pub const ACCENT: (f64, f64, f64) = (0.243, 0.651, 1.0); // #3ea6ff (Zorin Blue default)

/// Active themeable accent color from ~/.config/tiaki/accent or system defaults.
pub fn accent_color() -> (f64, f64, f64) {
    if let Ok(path) = std::env::var("XDG_CONFIG_HOME")
        .map(|p| format!("{p}/tiaki/accent"))
        .or_else(|_| std::env::var("HOME").map(|h| format!("{h}/.config/tiaki/accent")))
    {
        if let Ok(hex) = std::fs::read_to_string(path) {
            let hex = hex.trim().trim_start_matches('#');
            if hex.len() == 6 {
                if let (Ok(r), Ok(g), Ok(b)) = (
                    u8::from_str_radix(&hex[0..2], 16),
                    u8::from_str_radix(&hex[2..4], 16),
                    u8::from_str_radix(&hex[4..6], 16),
                ) {
                    return (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
                }
            }
        }
    }
    ACCENT
}

/// Colour names, the type ramp, spacing and radius steps. Loaded first.
const TOKENS_CSS: &str = include_str!("../css/tokens.css");

/// The shared foundation every module's sheets layer on top of.
const BASE_CSS: &str = include_str!("../css/base.css");

/// All module stylesheets in cascade order. This must stay in the order the
/// old monolithic blob had: base, bar, menu, dock, launcher, chrome,
/// settings, control.
fn all_sheets() -> Vec<&'static str> {
    let mut sheets = vec![TOKENS_CSS, BASE_CSS];
    sheets.extend_from_slice(bar::STYLE_SHEETS);
    sheets.extend_from_slice(menu::STYLE_SHEETS);
    sheets.extend_from_slice(dock::STYLE_SHEETS);
    sheets.extend_from_slice(greet::STYLE_SHEETS);
    sheets.extend_from_slice(launcher::STYLE_SHEETS);
    sheets.extend_from_slice(chrome::STYLE_SHEETS);
    sheets.extend_from_slice(settings::STYLE_SHEETS);
    sheets.extend_from_slice(control::STYLE_SHEETS);
    sheets.extend_from_slice(files::STYLE_SHEETS);
    sheets.extend_from_slice(notify::STYLE_SHEETS);
    sheets.extend_from_slice(setup::STYLE_SHEETS);
    sheets.extend_from_slice(switcher::STYLE_SHEETS);
    sheets.extend_from_slice(overview::STYLE_SHEETS);
    sheets
}

pub fn install() {
    let accent = accent_color();
    let ar = (accent.0 * 255.0).round() as u8;
    let ag = (accent.1 * 255.0).round() as u8;
    let ab = (accent.2 * 255.0).round() as u8;
    let dynamic_tokens = format!(
        "@define-color accent rgb({ar}, {ag}, {ab});\n\
         @define-color accent_hover rgba({ar}, {ag}, {ab}, 0.85);\n\
         @define-color accent_glow rgba({ar}, {ag}, {ab}, 0.40);\n"
    );
    let mut css: String = dynamic_tokens;
    css.push_str(&all_sheets().join("\n"));
    let provider = gtk::CssProvider::new();
    if let Err(error) = provider.load_from_data(css.as_bytes()) {
        eprintln!("tiaki-shell: stylesheet error: {error}");
    }
    if let Some(screen) = gtk::gdk::Screen::default() {
        gtk::StyleContext::add_provider_for_screen(
            &screen,
            &provider,
            // Above the user sheet: ~/.config/gtk-3.0/gtk.css themes
            // third-party apps and must not restyle shell controls.
            gtk::STYLE_PROVIDER_PRIORITY_USER + 1,
        );
    }
}

pub fn class(widget: &impl IsA<gtk::Widget>, name: &str) {
    widget.style_context().add_class(name);
}

/// A layer-shell window with the transparent visual the shell needs.
pub fn base_window(app_class: &str) -> gtk::Window {
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_decorated(false);
    window.set_resizable(false);
    window.set_app_paintable(true);
    window.set_skip_taskbar_hint(true);
    window.set_skip_pager_hint(true);
    window.style_context().add_class(app_class);
    if let Some(screen) = gtk::prelude::WidgetExt::screen(&window) {
        if let Some(visual) = screen.rgba_visual() {
            window.set_visual(Some(&visual));
        }
    }
    window
}

pub fn setup_layer(
    window: &gtk::Window,
    namespace: &str,
    layer: gtk_layer_shell::Layer,
    keyboard: gtk_layer_shell::KeyboardMode,
    exclusive: i32,
) {
    window.init_layer_shell();
    window.set_namespace(namespace);
    window.set_layer(layer);
    window.set_keyboard_mode(keyboard);
    window.set_exclusive_zone(exclusive);
}

pub fn anchor(window: &gtk::Window, edge: gtk_layer_shell::Edge) {
    window.set_anchor(edge, true);
}
