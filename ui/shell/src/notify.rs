//! Notifications: the store, the counts the bar reads, and the panel.
//!
//! A notification is a small text file in
//! `$XDG_STATE_HOME/tiaki/notifications/` (default `~/.local/state/...`):
//!
//! ```text
//! <epoch>-<kind>.txt
//! first line = title
//! remaining lines = body
//! ```
//!
//! `kind` is `approval`, `error`, or `info`. The bar shows a lamp chip only
//! while an `approval` is outstanding; everything else is just the bell.

use std::path::PathBuf;

use gtk::gdk;
use gtk::prelude::*;
use gtk_layer_shell::{Edge, KeyboardMode, Layer};

use crate::{glyphs, single, system, theme};

/// Stylesheets owned by this module, in cascade order.
pub const STYLE_SHEETS: &[&str] = &[include_str!("../css/notify.css")];

#[derive(Clone)]
pub struct Notice {
    pub file: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub age: String,
}

pub fn dir() -> PathBuf {
    let base = std::env::var("XDG_STATE_HOME")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/home/desktop".to_string());
            format!("{home}/.local/state")
        });
    PathBuf::from(base).join("tiaki/notifications")
}

fn epoch_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn age_of(stamp: i64) -> String {
    if stamp <= 0 {
        return String::new();
    }
    let delta = (epoch_now() - stamp).max(0);
    if delta < 60 {
        format!("{delta}s")
    } else if delta < 3_600 {
        format!("{}m", delta / 60)
    } else if delta < 86_400 {
        format!("{}h", delta / 3_600)
    } else {
        format!("{}d", delta / 86_400)
    }
}

pub fn load() -> Vec<Notice> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir()) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("txt") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let mut lines = text.lines();
        let title = lines.next().unwrap_or("").trim().to_string();
        let body = lines.collect::<Vec<_>>().join(" ").trim().to_string();
        let file = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_string();
        let mut parts = file.splitn(2, '-');
        let stamp = parts
            .next()
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(0);
        let kind = parts
            .next()
            .unwrap_or("info")
            .trim_end_matches(".txt")
            .to_string();
        out.push(Notice {
            file,
            kind,
            title,
            body,
            age: age_of(stamp),
        });
    }
    out.sort_by(|a, b| b.file.cmp(&a.file));
    out
}

pub fn count() -> usize {
    load().len()
}

pub fn pending_approvals() -> usize {
    load().iter().filter(|notice| notice.kind == "approval").count()
}

pub fn clear_all() {
    if let Ok(entries) = std::fs::read_dir(dir()) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) == Some("txt") {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

/// The notification center. Anchored under the bar, top-left, like Control
/// Center — not a modal, because it is a list you scan and dismiss.
pub fn run(_x: i32) {
    // Opening one popup closes the other.
    single::Instance::dismiss("tiaki-control", "control");
    let Some(_instance) = single::Instance::acquire("tiaki-notify", "notifications", false)
    else {
        return;
    };

    let window = theme::base_window("notify-window");
    theme::setup_layer(
        &window,
        "tiaki-notify",
        Layer::Overlay,
        KeyboardMode::Exclusive,
        0,
    );
    for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        theme::anchor(&window, edge);
    }

    let scrim = gtk::EventBox::new();
    scrim.set_visible_window(true);
    scrim.set_can_focus(true);
    theme::class(&scrim, "menu-scrim");
    scrim.set_hexpand(true);
    scrim.set_vexpand(true);

    let panel = gtk::EventBox::new();
    panel.set_visible_window(true);
    panel.set_size_request(400, -1);
    panel.set_halign(gtk::Align::End);
    panel.set_valign(gtk::Align::Start);
    theme::class(&panel, "nt-panel");

    let (screen_width, screen_height) = gdk::Display::default()
        .and_then(|display| display.primary_monitor())
        .map(|monitor| {
            let geometry = monitor.geometry();
            (geometry.width(), geometry.height())
        })
        .unwrap_or((1440, 900));
    // The window must fill the output so the scrim spans the screen and
    // margin_start is measured from the screen edge, not from a window that
    // GTK keeps at content size and the compositor centres.
    window.set_size_request(screen_width, screen_height);
    // Pinned to the right edge, under the icon that opened it.
    // The icon x in bar pixels does not match compositor scale reliably,
    // so derive the position from the screen, not the click point.
    panel.set_margin_end(12);
    panel.set_margin_top(44);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.set_margin_top(14);
    content.set_margin_bottom(14);
    content.set_margin_start(14);
    content.set_margin_end(14);

    let notices = load();

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let title = gtk::Label::new(Some("Notifications"));
    title.set_xalign(0.0);
    title.set_hexpand(true);
    theme::class(&title, "nt-title");
    header.pack_start(&title, true, true, 0);

    let count = gtk::Label::new(Some(&format!("{}", notices.len())));
    theme::class(&count, "nt-count");
    header.pack_start(&count, false, false, 0);

    if !notices.is_empty() {
        let clear = gtk::Button::with_label("Clear");
        clear.set_relief(gtk::ReliefStyle::None);
        theme::class(&clear, "nt-text-button");
        let window_for_clear = window.clone();
        clear.connect_clicked(move |_| {
            clear_all();
            window_for_clear.close();
        });
        header.pack_end(&clear, false, false, 0);
    }
    content.pack_start(&header, false, false, 0);

    let rule = gtk::Separator::new(gtk::Orientation::Horizontal);
    theme::class(&rule, "nt-rule");
    rule.set_margin_top(12);
    content.pack_start(&rule, false, false, 0);

    if notices.is_empty() {
        let empty = gtk::Box::new(gtk::Orientation::Vertical, 4);
        empty.set_margin_top(26);
        empty.set_margin_bottom(22);
        empty.set_margin_start(2);
        let head = gtk::Label::new(Some("Nothing needs you."));
        head.set_xalign(0.0);
        theme::class(&head, "nt-empty-title");
        let sub = gtk::Label::new(Some(
            "Approvals and failed transactions will appear here.",
        ));
        sub.set_xalign(0.0);
        sub.set_line_wrap(true);
        theme::class(&sub, "nt-empty-body");
        empty.pack_start(&head, false, false, 0);
        empty.pack_start(&sub, false, false, 0);
        content.pack_start(&empty, false, false, 0);
    } else {
        for notice in &notices {
            content.pack_start(&notice_row(notice), false, false, 0);
        }
    }

    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    footer.set_margin_top(12);
    footer.set_margin_bottom(2);
    let ledger = gtk::Button::with_label("Review ledger");
    ledger.set_relief(gtk::ReliefStyle::None);
    theme::class(&ledger, "nt-text-button");
    let window_for_ledger = window.clone();
    ledger.connect_clicked(move |_| {
        window_for_ledger.close();
        system::run_action("transactions.open");
    });
    footer.pack_start(&ledger, false, false, 0);
    content.pack_start(&footer, false, false, 0);

    panel.add(&content);
    scrim.add(&panel);
    window.add(&scrim);

    let scrim_for_click = scrim.clone();
    let window_for_click = window.clone();
    scrim.connect_button_press_event(move |_, event| {
        if event.window() == scrim_for_click.window() {
            window_for_click.close();
            gtk::glib::Propagation::Stop
        } else {
            gtk::glib::Propagation::Proceed
        }
    });

    let window_for_key = window.clone();
    window.connect_key_press_event(move |_, event| {
        if event.keyval() == gdk::keys::constants::Escape {
            window_for_key.close();
            gtk::glib::Propagation::Stop
        } else {
            gtk::glib::Propagation::Proceed
        }
    });

    window.connect_destroy(|_| gtk::main_quit());
    window.show_all();
    panel.show_all();
    window.present();
    scrim.grab_focus();
    gtk::main();
}

fn notice_row(notice: &Notice) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.set_margin_top(12);
    row.set_margin_bottom(12);
    row.set_margin_start(2);
    row.set_margin_end(2);

    let marker = gtk::EventBox::new();
    marker.set_visible_window(true);
    marker.set_size_request(6, 6);
    marker.set_valign(gtk::Align::Start);
    marker.set_margin_top(5);
    theme::class(&marker, "nt-marker");
    theme::class(
        &marker,
        match notice.kind.as_str() {
            "approval" => "lamp",
            "error" => "refused",
            _ => "info",
        },
    );
    row.pack_start(&marker, false, false, 0);

    let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
    let title = gtk::Label::new(Some(&notice.title));
    title.set_xalign(0.0);
    title.set_line_wrap(true);
    theme::class(&title, "nt-row-title");
    text.pack_start(&title, false, false, 0);

    if !notice.body.is_empty() {
        let body = gtk::Label::new(Some(&notice.body));
        body.set_xalign(0.0);
        body.set_line_wrap(true);
        theme::class(&body, "nt-row-body");
        text.pack_start(&body, false, false, 0);
    }

    if !notice.age.is_empty() {
        let age = gtk::Label::new(Some(&notice.age));
        age.set_xalign(0.0);
        theme::class(&age, "nt-row-age");
        text.pack_start(&age, false, false, 0);
    }
    row.pack_start(&text, true, true, 0);
    row
}

/// The bell in the bar, with a lamp dot while anything is unread. Returns the
/// button and the dot so the bar's refresh loop can show or hide it.
pub fn bell_button(window: &gtk::Window) -> (gtk::Button, gtk::EventBox) {
    let button = gtk::Button::new();
    button.set_relief(gtk::ReliefStyle::None);
    button.set_valign(gtk::Align::Center);
    theme::class(&button, "bar-icon-button");
    button.set_tooltip_text(Some("Notifications"));

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    row.set_valign(gtk::Align::Center);
    row.pack_start(&glyphs::status_icon("bell", 22), false, false, 0);

    let dot = gtk::EventBox::new();
    dot.set_visible_window(true);
    dot.set_size_request(5, 5);
    dot.set_valign(gtk::Align::Center);
    theme::class(&dot, "bar-badge-dot");
    dot.set_no_show_all(true);
    dot.hide();
    row.pack_start(&dot, false, false, 0);

    button.add(&row);
    let window = window.clone();
    button.connect_clicked(move |button| {
        let x = button
            .translate_coordinates(&window, 0, 0)
            .map(|(x, _)| x)
            .unwrap_or(0);
        launch_popup("notifications", &x.to_string());
    });
    (button, dot)
}

fn launch_popup(mode: &str, x: &str) {
    system::spawn(&format!("{} {mode} {x}", system::self_exe()));
}
