//! Window switcher: the ⌘-Tab equivalent, and the dock's "show this app's
//! windows" list.
//!
//! A layer overlay lists every open window with its app mark and title.
//! Arrows/Tab move the selection, Enter focuses, Escape closes. Optional
//! filter arguments:
//!
//!   tiaki-shell switcher            all windows
//!   tiaki-shell switcher focused    windows of the focused app
//!   tiaki-shell switcher foot       windows of one app id

use std::cell::RefCell;
use std::rc::Rc;

use gtk::gdk;
use gtk::prelude::*;
use gtk_layer_shell::{Edge, KeyboardMode, Layer};

use crate::windows::{self, View};
use crate::{icons, single, theme};

/// Stylesheets owned by this module, in cascade order.
pub const STYLE_SHEETS: &[&str] = &[include_str!("../css/switcher.css")];

pub fn run(args: &[String]) {
    let Some(_instance) = single::Instance::acquire("tiaki-switcher", "switcher", true) else {
        return;
    };

    let all = windows::views();
    let filter = args.first().map(String::as_str);
    let focused_app = all
        .iter()
        .find(|view| view.focused)
        .map(|view| view.app_id.clone());
    let windows: Vec<View> = match filter {
        None | Some("all") => all,
        Some("focused") => {
            let app = focused_app.unwrap_or_default();
            all.into_iter().filter(|view| view.app_id == app).collect()
        }
        Some(app) => all.into_iter().filter(|view| view.app_id == app).collect(),
    };
    if windows.is_empty() {
        return;
    }

    let window = theme::base_window("switcher-window");
    theme::setup_layer(
        &window,
        "tiaki-switcher",
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

    let panel = gtk::EventBox::new();
    panel.set_visible_window(true);
    panel.set_halign(gtk::Align::Center);
    panel.set_valign(gtk::Align::Center);
    panel.set_size_request(560, -1);
    theme::class(&panel, "switcher");

    let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
    list.set_margin_top(10);
    list.set_margin_bottom(10);
    list.set_margin_start(10);
    list.set_margin_end(10);

    let rows: Rc<RefCell<Vec<(i64, gtk::EventBox)>>> = Rc::new(RefCell::new(Vec::new()));
    for view in &windows {
        let row = gtk::EventBox::new();
        row.set_visible_window(true);
        theme::class(&row, "switcher-row");

        let line = gtk::Box::new(gtk::Orientation::Horizontal, 14);
        line.set_margin_start(12);
        line.set_margin_end(12);
        line.set_margin_top(8);
        line.set_margin_bottom(8);
        line.pack_start(&icons::app_icon(&art_for(view), 30), false, false, 0);

        let text = gtk::Box::new(gtk::Orientation::Vertical, 0);
        text.set_valign(gtk::Align::Center);
        let title = gtk::Label::new(Some(&windows::title(view)));
        title.set_xalign(0.0);
        theme::class(&title, "switcher-title");
        text.pack_start(&title, false, false, 0);
        line.pack_start(&text, true, true, 0);

        let hint = gtk::Label::new(Some(&app_label(&view.app_id)));
        theme::class(&hint, "switcher-app");
        line.pack_end(&hint, false, false, 0);

        row.add(&line);
        let id = view.id;
        let window_for_click = window.clone();
        row.connect_button_press_event(move |_, _| {
            focus(id);
            window_for_click.close();
            gtk::glib::Propagation::Stop
        });
        list.pack_start(&row, false, false, 0);
        rows.borrow_mut().push((id, row));
    }

    panel.add(&list);
    scrim.add(&panel);
    window.add(&scrim);

    // First row selected.
    let selected = Rc::new(RefCell::new(0usize));
    select(&rows, &selected, 0);

    let rows_for_key = rows.clone();
    let selected_for_key = selected.clone();
    let window_for_key = window.clone();
    window.connect_key_press_event(move |_, event| {
        let key = event.keyval();
        if key == gdk::keys::constants::Escape {
            window_for_key.close();
            return gtk::glib::Propagation::Stop;
        }
        if key == gdk::keys::constants::Return || key == gdk::keys::constants::KP_Enter {
            if let Some((id, _)) = rows_for_key.borrow().get(*selected_for_key.borrow()) {
                focus(*id);
            }
            window_for_key.close();
            return gtk::glib::Propagation::Stop;
        }
        if key == gdk::keys::constants::Down || key == gdk::keys::constants::Tab {
            let count = rows_for_key.borrow().len();
            let next = (*selected_for_key.borrow() + 1) % count.max(1);
            select(&rows_for_key, &selected_for_key, next);
            return gtk::glib::Propagation::Stop;
        }
        if key == gdk::keys::constants::Up || key == gdk::keys::constants::ISO_Left_Tab {
            let count = rows_for_key.borrow().len();
            let current = *selected_for_key.borrow();
            let next = (current + count.max(1) - 1) % count.max(1);
            select(&rows_for_key, &selected_for_key, next);
            return gtk::glib::Propagation::Stop;
        }
        gtk::glib::Propagation::Proceed
    });

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

    window.connect_destroy(|_| gtk::main_quit());
    window.show_all();
    window.present();
    scrim.grab_focus();
    gtk::main();
}

fn select(
    rows: &Rc<RefCell<Vec<(i64, gtk::EventBox)>>>,
    selected: &Rc<RefCell<usize>>,
    index: usize,
) {
    let rows = rows.borrow();
    for (position, (_, row)) in rows.iter().enumerate() {
        if position == index {
            row.style_context().add_class("selected");
        } else {
            row.style_context().remove_class("selected");
        }
    }
    *selected.borrow_mut() = index;
}

fn focus(id: i64) {
    windows::sway_plain(&format!("[con_id={id}] focus"));
}

fn art_for(view: &View) -> String {
    if view.app_id == "tiaki-shell" {
        let title = view.name.to_lowercase();
        if title.contains("settings") {
            return "settings".to_string();
        }
        if title.contains("agents") {
            return "agents".to_string();
        }
        return "app".to_string();
    }
    match view.app_id.as_str() {
        "foot" => "terminal".to_string(),
        "mousepad" => "editor".to_string(),
        "thunar" => "files".to_string(),
        "epiphany" => "browser".to_string(),
        "btop" => "monitor".to_string(),
        other => other.to_string(),
    }
}

fn app_label(app_id: &str) -> String {
    match app_id {
        "foot" => "Terminal".to_string(),
        "mousepad" => "Editor".to_string(),
        "thunar" => "Files".to_string(),
        "epiphany" => "Browser".to_string(),
        "btop" => "Monitor".to_string(),
        "tiaki-shell" => "Tiaki".to_string(),
        other => other.to_string(),
    }
}
