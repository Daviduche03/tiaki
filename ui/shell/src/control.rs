//! Control Center: the utilities you reach for while working — network,
//! bluetooth, inference, sandbox mode, display, sound, power.
//!
//! Same material as the bar: an ink panel, paper text, one lamp accent for
//! what is on. No glass, no white badges, no translucent tiles.

use gtk::gdk;
use gtk::prelude::*;
use gtk_layer_shell::{Edge, KeyboardMode, Layer};

use crate::glyphs;
use crate::{single, system, theme};

/// Stylesheets owned by this module, in cascade order.
pub const STYLE_SHEETS: &[&str] = &[include_str!("../css/control.css")];

const PAPER: (f64, f64, f64) = (0.902, 0.878, 0.831);
const PAPER_DIM: (f64, f64, f64) = (0.56, 0.54, 0.51);

pub fn run(_x: i32) {
    // Opening one popup closes the other.
    single::Instance::dismiss("tiaki-notify", "notifications");
    let Some(_instance) = single::Instance::acquire("tiaki-control", "control", false) else {
        return;
    };

    let window = theme::base_window("control-window");
    theme::setup_layer(
        &window,
        "tiaki-control",
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
    panel.set_size_request(380, -1);
    panel.set_halign(gtk::Align::End);
    panel.set_valign(gtk::Align::Start);
    theme::class(&panel, "cc-panel");

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

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let title = gtk::Label::new(Some("Control Center"));
    title.set_xalign(0.0);
    title.set_hexpand(true);
    theme::class(&title, "cc-title");
    header.pack_start(&title, true, true, 0);

    let gear = gtk::Button::new();
    gear.set_relief(gtk::ReliefStyle::None);
    theme::class(&gear, "cc-icon-button");
    gear.add(&glyphs::status_icon_tinted("gear", 16, PAPER_DIM));
    gear.set_tooltip_text(Some("Open settings"));
    let window_for_gear = window.clone();
    gear.connect_clicked(move |_| {
        window_for_gear.close();
        system::run_action("settings.open");
    });
    header.pack_end(&gear, false, false, 0);
    content.pack_start(&header, false, false, 0);

    let rule = gtk::Separator::new(gtk::Orientation::Horizontal);
    theme::class(&rule, "cc-rule");
    rule.set_margin_top(10);
    content.pack_start(&rule, false, false, 0);

    let grid = gtk::Grid::new();
    grid.set_row_spacing(8);
    grid.set_column_spacing(8);
    grid.set_column_homogeneous(true);
    grid.set_margin_top(12);

    grid.attach(&tile(&window, "wifi", true, "Wi-Fi", "connected", "settings.page.network"), 0, 0, 1, 1);
    grid.attach(&tile(&window, "bluetooth", true, "Bluetooth", "on", "settings.page.bluetooth"), 1, 0, 1, 1);
    grid.attach(&tile(&window, "spark", true, "Inference", "local broker", "agents.open"), 0, 1, 1, 1);
    grid.attach(&tile(&window, "lock", true, "Sandbox", "guarded", "tiaki:mode.guarded"), 1, 1, 1, 1);

    content.pack_start(&grid, false, false, 0);
    content.pack_start(&slider("brightness", "Display", 82.0), false, false, 8);
    content.pack_start(&slider("audio", "Sound", 64.0), false, false, 0);
    content.pack_start(&power(&window), false, false, 8);
    content.pack_start(&footer(), false, false, 0);

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

fn tile(window: &gtk::Window, icon: &str, active: bool, title: &str, subtitle: &str, action: &str) -> gtk::EventBox {
    let card = gtk::EventBox::new();
    card.set_visible_window(true);
    theme::class(&card, "cc-tile");
    card.set_hexpand(true);
    // Tiles are buttons in behaviour: click navigates or toggles, then the
    // panel closes like the gear does.
    card.set_can_focus(true);
    let window_for_tile = window.clone();
    let action = action.to_string();
    card.connect_button_press_event(move |_, _| {
        window_for_tile.close();
        system::run_action(&action);
        gtk::glib::Propagation::Stop
    });

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.set_margin_top(11);
    row.set_margin_bottom(11);
    row.set_margin_start(12);
    row.set_margin_end(12);
    row.set_valign(gtk::Align::Center);

    let color = if active { PAPER } else { PAPER_DIM };
    row.pack_start(&glyphs::status_icon_tinted(icon, 17, color), false, false, 0);

    let text = gtk::Box::new(gtk::Orientation::Vertical, 1);
    text.set_valign(gtk::Align::Center);
    let title_label = gtk::Label::new(Some(title));
    title_label.set_xalign(0.0);
    theme::class(&title_label, "cc-tile-title");
    let sub_label = gtk::Label::new(Some(subtitle));
    sub_label.set_xalign(0.0);
    theme::class(&sub_label, "cc-tile-sub");
    text.pack_start(&title_label, false, false, 0);
    text.pack_start(&sub_label, false, false, 0);
    row.pack_start(&text, true, true, 0);

    card.add(&row);
    card
}

fn slider(icon: &str, label: &str, value: f64) -> gtk::EventBox {
    let card = gtk::EventBox::new();
    card.set_visible_window(true);
    theme::class(&card, "cc-card");
    card.set_hexpand(true);

    let body = gtk::Box::new(gtk::Orientation::Vertical, 8);
    body.set_margin_top(11);
    body.set_margin_bottom(12);
    body.set_margin_start(12);
    body.set_margin_end(12);

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.pack_start(&glyphs::status_icon_tinted(icon, 15, PAPER_DIM), false, false, 0);
    let label_widget = gtk::Label::new(Some(label));
    label_widget.set_xalign(0.0);
    label_widget.set_hexpand(true);
    theme::class(&label_widget, "cc-slider-title");
    row.pack_start(&label_widget, true, true, 0);
    let percent = gtk::Label::new(Some(&format!("{value:.0}%")));
    percent.set_xalign(1.0);
    theme::class(&percent, "cc-value");
    row.pack_end(&percent, false, false, 0);
    body.pack_start(&row, false, false, 0);

    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    scale.set_value(value);
    scale.set_draw_value(false);
    scale.set_hexpand(true);
    theme::class(&scale, "cc-scale");
    let percent_clone = percent.clone();
    let backend = if label == "Display" {
        "brightnessctl set {v}% 2>/dev/null || light -S {v} 2>/dev/null; true"
    } else {
        "pactl set-sink-volume @DEFAULT_SINK@ {v}% 2>/dev/null || pamixer --set-volume {v} 2>/dev/null; true"
    }
    .to_string();
    scale.connect_value_changed(move |scale| {
        let v = scale.value().round() as i64;
        percent_clone.set_text(&format!("{v}%"));
        let command = backend.replace("{v}", &v.to_string());
        system::spawn(&command);
    });
    body.pack_start(&scale, false, false, 0);

    card.add(&body);
    card
}

fn power(window: &gtk::Window) -> gtk::EventBox {
    let card = gtk::EventBox::new();
    card.set_visible_window(true);
    theme::class(&card, "cc-card");
    card.set_hexpand(true);

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.set_margin_top(12);
    row.set_margin_bottom(12);
    row.set_margin_start(12);
    row.set_margin_end(12);
    row.pack_start(&glyphs::status_icon_tinted("battery", 17, PAPER_DIM), false, false, 0);
    let label = gtk::Label::new(Some("Power"));
    label.set_xalign(0.0);
    label.set_hexpand(true);
    theme::class(&label, "cc-slider-title");
    row.pack_start(&label, true, true, 0);
    let value = gtk::Label::new(Some("AC · 100%"));
    value.set_xalign(1.0);
    theme::class(&value, "cc-value");
    row.pack_end(&value, false, false, 0);
    card.add(&row);
    card.set_can_focus(true);
    let window_for_power = window.clone();
    card.connect_button_press_event(move |_, _| {
        window_for_power.close();
        system::run_action("settings.page.power");
        gtk::glib::Propagation::Stop
    });
    card
}

fn footer() -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.set_margin_top(10);
    row.set_margin_start(2);
    let generation = gtk::Label::new(Some("gen a · guarded"));
    generation.set_xalign(0.0);
    generation.set_hexpand(true);
    theme::class(&generation, "cc-meta");
    row.pack_start(&generation, true, true, 0);
    row
}
