//! A menu bar dropdown as its own layer surface.
//!
//! The bar itself is only 30px tall, so a GTK popover would be clipped by the
//! layer surface. Menus are separate overlay surfaces positioned under the
//! button that opened them. They are fully drawn by us, so the dark popup and
//! shortcut layout match the rest of the shell.

use gtk::gdk;
use gtk::prelude::*;
use gtk_layer_shell::{Edge, KeyboardMode, Layer};

use crate::model::MenuEntry;
use crate::{model, single, system, theme};

/// Stylesheets owned by this module, in cascade order.
/// Add a file to `ui/shell/css/` and list it here to grow them.
pub const STYLE_SHEETS: &[&str] = &[include_str!("../css/menu.css")];

pub fn run(root: &agentic_ui_tree::Node, menu_id: &str, x: i32, app_id: &str) {
    let Some(_instance) = single::Instance::acquire("tiaki-menu", "menu", true) else {
        return;
    };
    let Some(menu) = model::menu_by_id(root, menu_id) else {
        return;
    };
    // Same File/Edit/View label, different items per focused app.
    let menu = model::filter_menu(&menu, app_id);

    let window = theme::base_window("menu-window");
    theme::setup_layer(
        &window,
        "tiaki-menu",
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
    panel.set_size_request(232, -1);
    panel.set_halign(gtk::Align::Start);
    panel.set_valign(gtk::Align::Start);
    theme::class(&panel, "menu-popover");

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
    let left = x.clamp(8, (screen_width - 240).max(8));
    panel.set_margin_start(left);
    panel.set_margin_top(2);

    let list = gtk::Box::new(gtk::Orientation::Vertical, 1);
    theme::class(&list, "menu-list");

    for item in menu.items {
        match item {
            MenuEntry::Separator => {
                let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
                theme::class(&separator, "menu-sep");
                list.pack_start(&separator, false, false, 0);
            }
            MenuEntry::Item(item) => {
                let entry = gtk::Button::new();
                entry.set_relief(gtk::ReliefStyle::None);
                theme::class(&entry, "menu-entry");

                let row = gtk::Box::new(gtk::Orientation::Horizontal, 18);
                let label = gtk::Label::new(Some(&item.label));
                label.set_xalign(0.0);
                label.set_hexpand(true);
                theme::class(&label, "menu-label");
                row.pack_start(&label, true, true, 0);
                if !item.shortcut.is_empty() {
                    let shortcut = gtk::Label::new(Some(&item.shortcut));
                    shortcut.set_xalign(1.0);
                    theme::class(&shortcut, "menu-shortcut");
                    row.pack_start(&shortcut, false, false, 0);
                }
                entry.add(&row);

                let action = item.action.clone();
                let window_clone = window.clone();
                entry.connect_clicked(move |_| {
                    window_clone.close();
                    system::run_action(&action);
                });
                list.pack_start(&entry, false, false, 0);
            }
        }
    }

    panel.add(&list);
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
