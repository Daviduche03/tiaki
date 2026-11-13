//! Mission Control overview: displays all windows across workspaces in a grid.
//!
//! Pressing Super+Up (or invoking `tiaki-shell overview`) opens a full-screen
//! overlay showing window cards. Clicking or hitting Return focuses that window,
//! bringing it to the active workspace. Escape dismisses the overview.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::gdk;
use gtk::prelude::*;
use gtk_layer_shell::{Edge, KeyboardMode, Layer};

use crate::windows::{self, View};
use crate::{icons, single, theme};

/// Stylesheets owned by this module, in cascade order.
pub const STYLE_SHEETS: &[&str] = &[include_str!("../css/overview.css")];

pub fn run() {
    let Some(_instance) = single::Instance::acquire("tiaki-overview", "overview", true) else {
        return;
    };

    let all_windows = windows::views();
    if all_windows.is_empty() {
        return;
    }

    let window = theme::base_window("overview-window");
    theme::setup_layer(
        &window,
        "tiaki-overview",
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
    theme::class(&scrim, "overview-scrim");

    let root_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root_box.set_halign(gtk::Align::Center);
    root_box.set_valign(gtk::Align::Center);
    root_box.set_margin_start(48);
    root_box.set_margin_end(48);
    root_box.set_margin_top(48);
    root_box.set_margin_bottom(48);

    // Top header
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    theme::class(&header, "overview-header-bar");
    let title = gtk::Label::new(Some("Mission Control"));
    theme::class(&title, "overview-title-main");
    header.pack_start(&title, false, false, 0);

    let count_label = gtk::Label::new(Some(&format!("{} windows open", all_windows.len())));
    theme::class(&count_label, "overview-count");
    header.pack_start(&count_label, false, false, 0);
    root_box.pack_start(&header, false, false, 0);

    // Grid of window cards
    let grid = gtk::Grid::new();
    grid.set_column_spacing(20);
    grid.set_row_spacing(20);
    grid.set_halign(gtk::Align::Center);

    let cards: Rc<RefCell<Vec<(i64, gtk::EventBox)>>> = Rc::new(RefCell::new(Vec::new()));
    let cols = if all_windows.len() <= 2 {
        all_windows.len()
    } else if all_windows.len() <= 6 {
        3
    } else {
        4
    };

    for (index, view) in all_windows.iter().enumerate() {
        let card = gtk::EventBox::new();
        card.set_visible_window(true);
        theme::class(&card, "overview-card");

        let card_box = gtk::Box::new(gtk::Orientation::Vertical, 0);

        // Header row: app icon + title + workspace badge
        let card_header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        theme::class(&card_header, "overview-card-header");

        let icon_name = art_for(view);
        card_header.pack_start(&icons::app_icon(&icon_name, 20), false, false, 0);

        let win_title = gtk::Label::new(Some(&windows::title(view)));
        win_title.set_xalign(0.0);
        win_title.set_hexpand(true);
        win_title.set_max_width_chars(24);
        win_title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        theme::class(&win_title, "overview-card-title");
        card_header.pack_start(&win_title, true, true, 0);

        let badge = gtk::Label::new(Some(&format!("ID {}", view.id)));
        theme::class(&badge, "overview-card-badge");
        card_header.pack_end(&badge, false, false, 0);
        card_box.pack_start(&card_header, false, false, 0);

        // Thumbnail canvas / mock representation
        let thumb = gtk::Box::new(gtk::Orientation::Vertical, 0);
        theme::class(&thumb, "overview-thumbnail");
        card_box.pack_start(&thumb, true, true, 0);

        card.add(&card_box);

        let id = view.id;
        let window_for_click = window.clone();
        card.connect_button_press_event(move |_, _| {
            focus(id);
            window_for_click.close();
            gtk::glib::Propagation::Stop
        });

        let col = (index % cols) as i32;
        let row = (index / cols) as i32;
        grid.attach(&card, col, row, 1, 1);
        cards.borrow_mut().push((id, card));
    }

    root_box.pack_start(&grid, true, true, 0);
    scrim.add(&root_box);
    window.add(&scrim);

    // Select the first card initially
    let selected = Rc::new(RefCell::new(0usize));
    if let Some((_, first_card)) = cards.borrow().first() {
        first_card.style_context().add_class("selected");
    }

    let cards_for_key = cards.clone();
    let selected_for_key = selected.clone();
    let window_for_key = window.clone();
    window.connect_key_press_event(move |_, event| {
        let key = event.keyval();
        if key == gdk::keys::constants::Escape {
            window_for_key.close();
            return gtk::glib::Propagation::Stop;
        }
        if key == gdk::keys::constants::Return || key == gdk::keys::constants::KP_Enter {
            if let Some((id, _)) = cards_for_key.borrow().get(*selected_for_key.borrow()) {
                focus(*id);
            }
            window_for_key.close();
            return gtk::glib::Propagation::Stop;
        }

        let total = cards_for_key.borrow().len();
        if total == 0 {
            return gtk::glib::Propagation::Proceed;
        }

        let current = *selected_for_key.borrow();
        let next = match key {
            gdk::keys::constants::Right | gdk::keys::constants::Tab => (current + 1) % total,
            gdk::keys::constants::Left => {
                if current == 0 {
                    total - 1
                } else {
                    current - 1
                }
            }
            gdk::keys::constants::Down => (current + cols).min(total - 1),
            gdk::keys::constants::Up => current.saturating_sub(cols),
            _ => return gtk::glib::Propagation::Proceed,
        };

        if let Some((_, old_card)) = cards_for_key.borrow().get(current) {
            old_card.style_context().remove_class("selected");
        }
        if let Some((_, new_card)) = cards_for_key.borrow().get(next) {
            new_card.style_context().add_class("selected");
        }
        *selected_for_key.borrow_mut() = next;
        gtk::glib::Propagation::Stop
    });

    let window_for_scrim = window.clone();
    scrim.connect_button_press_event(move |_, _| {
        window_for_scrim.close();
        gtk::glib::Propagation::Stop
    });

    window.connect_destroy(|_| gtk::main_quit());
    window.show_all();
    gtk::main();
}

fn focus(id: i64) {
    // Bring out of scratchpad if needed and focus
    windows::sway_plain(&format!("[con_id={id}] scratchpad show"));
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
