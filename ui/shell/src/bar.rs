//! The top bar: one ink surface with a hairline under it.
//! Left: the Tiaki mark, the focused app's name, then its menus.
//! Right: pending approvals (only when there are some), then whatever
//! hardware the machine really has, search, Control Center, and the clock.
//! Nothing here shows a value it did not read from the system.

use gtk::glib;
use gtk::prelude::*;
use gtk_layer_shell::{Edge, KeyboardMode, Layer};

use crate::{icons, model, notify, single, system, theme};

/// Stylesheets owned by this module, in cascade order.
pub const STYLE_SHEETS: &[&str] = &[include_str!("../css/bar.css")];

const BAR_HEIGHT: i32 = theme::BAR_HEIGHT;

pub fn run(root: &agentic_ui_tree::Node) {
    // One bar process; a relaunch replaces it rather than stacking two.
    let Some(_instance) = single::Instance::acquire("tiaki-bar", "bar", true) else {
        return;
    };

    let bar_node = model::find_by_id(root, "bar")
        .or_else(|| model::find_by_role(root, "bar"))
        .unwrap_or(root);

    let window = theme::base_window("bar-window");
    theme::setup_layer(
        &window,
        "tiaki-bar",
        Layer::Top,
        KeyboardMode::None,
        BAR_HEIGHT,
    );
    theme::anchor(&window, Edge::Top);
    theme::anchor(&window, Edge::Left);
    theme::anchor(&window, Edge::Right);
    window.set_size_request(-1, BAR_HEIGHT);

    // Translucent frosted glass with specular bottom hairline
    window.connect_draw(|widget, cr| {
        let width = widget.allocated_width() as f64;
        let height = widget.allocated_height() as f64;

        cr.set_operator(gtk::cairo::Operator::Source);
        let pat = gtk::cairo::LinearGradient::new(0.0, 0.0, 0.0, height);
        pat.add_color_stop_rgba(0.0, 0.12, 0.15, 0.18, 0.70);
        pat.add_color_stop_rgba(1.0, 0.07, 0.09, 0.11, 0.78);
        let _ = cr.set_source(&pat);
        let _ = cr.paint();
        cr.set_operator(gtk::cairo::Operator::Over);

        let stroke_pat = gtk::cairo::LinearGradient::new(0.0, 0.0, width, 0.0);
        stroke_pat.add_color_stop_rgba(0.0, 1.0, 1.0, 1.0, 0.06);
        stroke_pat.add_color_stop_rgba(0.5, 1.0, 1.0, 1.0, 0.18);
        stroke_pat.add_color_stop_rgba(1.0, 1.0, 1.0, 1.0, 0.06);
        let _ = cr.set_source(&stroke_pat);
        cr.rectangle(0.0, height - 1.0, width, 1.0);
        let _ = cr.fill();

        gtk::glib::Propagation::Proceed
    });

    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    bar.set_size_request(-1, BAR_HEIGHT);
    theme::class(&bar, "bar");

    // Left cluster: Tiaki logo, active app title, and application menus
    let left_cluster = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    left_cluster.set_valign(gtk::Align::Center);

    // 1. Tiaki brand mark button (beacon icon)
    left_cluster.pack_start(&tiaki_brand_button(bar_node), false, false, 0);

    // 2. Active application title in bold
    let (app_title_widget, app_title_label) = active_app_title();
    left_cluster.pack_start(&app_title_widget, false, false, 0);

    // 3. Persistent application menus: File, Edit, View, Window, etc.
    for menu in model::menus_in(bar_node) {
        let button = menu_button(menu, &window);
        left_cluster.pack_start(&button, false, false, 0);
    }
    bar.pack_start(&left_cluster, false, false, 0);

    // Dynamic focus tracker for app title
    let title_for_timer = app_title_label;
    glib::timeout_add_seconds_local(1, move || {
        let focused = crate::windows::focused_app_id();
        let name = match focused.as_str() {
            "foot" => "Terminal",
            "mousepad" => "Editor",
            "thunar" => "Files",
            "epiphany" => "Browser",
            "btop" => "Monitor",
            "tiaki-shell" => "Settings",
            other if !other.is_empty() => other,
            _ => "Tiaki",
        };
        title_for_timer.set_text(name);
        glib::ControlFlow::Continue
    });

    // Center cluster: Horizontally centered clock and date (matching Zorin OS)
    let center_cluster = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    center_cluster.set_valign(gtk::Align::Center);
    let clock = clock_button(bar_node);
    center_cluster.pack_start(&clock, false, false, 0);
    bar.set_center_widget(Some(&center_cluster));

    // Right cluster: privacy approvals, hardware status, search, control center
    let right_cluster = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    right_cluster.set_valign(gtk::Align::Center);

    // a. Privacy / Agent approval orange capsule
    let (chip, chip_label) = approval_chip();
    right_cluster.pack_start(&chip, false, false, 0);

    // b. Radios and battery, each only if the hardware exists.
    if has_bluetooth() {
        let bt = icon_spawn_button("bluetooth", 16, "tiaki-shell settings bluetooth", "Bluetooth");
        right_cluster.pack_start(&bt, false, false, 0);
    }
    if has_wifi() {
        let wifi = icon_spawn_button("wifi", 16, "tiaki-shell settings wifi", "Wi-Fi");
        right_cluster.pack_start(&wifi, false, false, 0);
    }
    if let Some(battery) = battery_widget() {
        right_cluster.pack_start(&battery, false, false, 0);
    }

    // c. Search
    let search = icon_action_button("search", 16, "palette.open", "Search (⌘Space)");
    right_cluster.pack_start(&search, false, false, 0);

    // d. Control Center
    let control = control_center_button(bar_node, &window);
    right_cluster.pack_start(&control, false, false, 0);

    bar.pack_end(&right_cluster, false, false, 0);

    window.add(&bar);
    window.show_all();

    // Check approvals periodically
    let chip_for_timer = chip.clone();
    let label_for_timer = chip_label.clone();
    glib::timeout_add_seconds_local(2, move || {
        let approvals = notify::pending_approvals();
        if approvals == 0 {
            chip_for_timer.set_no_show_all(true);
            chip_for_timer.hide();
        } else {
            label_for_timer.set_text(&if approvals == 1 {
                "1 approval".to_string()
            } else {
                format!("{approvals} approvals")
            });
            chip_for_timer.set_no_show_all(false);
            chip_for_timer.show_all();
        }
        glib::ControlFlow::Continue
    });

    gtk::main();
}

fn tiaki_brand_button(bar: &agentic_ui_tree::Node) -> gtk::Button {
    let node = bar.children.iter().find(|child| child.id == "bar.logo");
    let action = node
        .and_then(|node| node.prop_str("action"))
        .unwrap_or("palette.open")
        .to_string();

    let button = gtk::Button::new();
    button.set_relief(gtk::ReliefStyle::None);
    button.set_valign(gtk::Align::Center);
    button.set_tooltip_text(Some("Tiaki"));
    theme::class(&button, "brand-button");

    button.add(&icons::logo_mark(18));
    button.connect_clicked(move |_| system::run_action(&action));
    button
}

fn active_app_title() -> (gtk::Widget, gtk::Label) {
    let label = gtk::Label::new(Some("Tiaki"));
    label.set_valign(gtk::Align::Center);
    theme::class(&label, "bar-app-title");
    (label.clone().upcast(), label)
}

fn menu_button(menu: model::MenuDef, window: &gtk::Window) -> gtk::Button {
    let button = gtk::Button::with_label(&menu.label);
    button.set_relief(gtk::ReliefStyle::None);
    button.set_valign(gtk::Align::Center);
    theme::class(&button, "menu-button");

    let menu_id = menu.id.clone();
    let window = window.clone();
    button.connect_clicked(move |button| {
        let x = button
            .translate_coordinates(&window, 0, 0)
            .map(|(x, _)| x)
            .unwrap_or(0);
        let app = crate::windows::focused_app_id();
        launch_popup("menu", &format!("{menu_id} {x} {app}"));
    });
    button
}

fn launch_popup(mode: &str, args: &str) {
    system::spawn(&format!("{} {mode} {args}", system::self_exe()));
}

fn approval_chip() -> (gtk::Button, gtk::Label) {
    let button = gtk::Button::new();
    button.set_relief(gtk::ReliefStyle::None);
    button.set_valign(gtk::Align::Center);
    button.set_tooltip_text(Some("Agent actions waiting for your approval"));
    theme::class(&button, "bar-chip");
    button.set_no_show_all(true);
    button.hide();

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    row.set_valign(gtk::Align::Center);

    let label = gtk::Label::new(Some("1 approval"));
    label.set_valign(gtk::Align::Center);
    theme::class(&label, "bar-chip-label");
    row.pack_start(&label, false, false, 0);

    button.add(&row);
    button.connect_clicked(|_| system::run_action("notifications.open"));
    (button, label)
}

fn icon_action_button(icon: &str, size: i32, action: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::new();
    button.set_relief(gtk::ReliefStyle::None);
    button.set_valign(gtk::Align::Center);
    button.set_tooltip_text(Some(tooltip));
    theme::class(&button, "bar-icon-button");
    button.add(&icons::status_icon(icon, size));

    let action = action.to_string();
    button.connect_clicked(move |_| system::run_action(&action));
    button
}

fn icon_spawn_button(icon: &str, size: i32, cmd: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::new();
    button.set_relief(gtk::ReliefStyle::None);
    button.set_valign(gtk::Align::Center);
    button.set_tooltip_text(Some(tooltip));
    theme::class(&button, "bar-icon-button");
    button.add(&icons::status_icon(icon, size));

    let cmd = cmd.to_string();
    button.connect_clicked(move |_| system::spawn(&cmd));
    button
}

/// The battery readout, or nothing on a machine without one.
fn battery_widget() -> Option<gtk::Widget> {
    let pct = battery_capacity()?;

    let container = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    container.set_valign(gtk::Align::Center);
    theme::class(&container, "bar-battery-box");

    let label = gtk::Label::new(Some(&format!("{pct}%")));
    label.set_valign(gtk::Align::Center);
    theme::class(&label, "bar-battery-pct");
    container.pack_start(&label, false, false, 0);

    let icon = icons::status_icon("battery", 20);
    icon.set_valign(gtk::Align::Center);
    container.pack_start(&icon, false, false, 0);

    glib::timeout_add_seconds_local(30, move || {
        if let Some(pct) = battery_capacity() {
            label.set_text(&format!("{pct}%"));
        }
        glib::ControlFlow::Continue
    });

    Some(container.upcast())
}

fn battery_capacity() -> Option<u32> {
    let entries = std::fs::read_dir("/sys/class/power_supply").ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let kind = std::fs::read_to_string(path.join("type")).unwrap_or_default();
        if kind.trim() != "Battery" {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(path.join("capacity")) {
            if let Ok(pct) = text.trim().parse::<u32>() {
                return Some(pct);
            }
        }
    }
    None
}

fn has_wifi() -> bool {
    std::fs::read_dir("/sys/class/net")
        .map(|entries| {
            entries
                .flatten()
                .any(|entry| entry.path().join("wireless").exists())
        })
        .unwrap_or(false)
}

fn has_bluetooth() -> bool {
    std::fs::read_dir("/sys/class/bluetooth")
        .map(|mut entries| entries.next().is_some())
        .unwrap_or(false)
}

fn control_center_button(bar: &agentic_ui_tree::Node, window: &gtk::Window) -> gtk::Button {
    let _ = bar;
    let button = gtk::Button::new();
    button.set_relief(gtk::ReliefStyle::None);
    button.set_valign(gtk::Align::Center);
    button.set_tooltip_text(Some("Control Center"));
    theme::class(&button, "bar-icon-button");
    button.add(&icons::status_icon("control", 16));

    let window = window.clone();
    button.connect_clicked(move |button| {
        let x = button
            .translate_coordinates(&window, 0, 0)
            .map(|(x, _)| x)
            .unwrap_or(0);
        launch_popup("control", &x.to_string());
    });
    button
}

fn clock_button(bar: &agentic_ui_tree::Node) -> gtk::Button {
    let node = model::clock_node(bar);
    let format = node
        .and_then(|node| node.prop_str("format"))
        .unwrap_or("24h")
        .to_string();
    let show_date = node
        .and_then(|node| node.prop_bool("show_date"))
        .unwrap_or(true);
    let show_seconds = node
        .and_then(|node| node.prop_bool("show_seconds"))
        .unwrap_or(false);

    let button = gtk::Button::new();
    button.set_relief(gtk::ReliefStyle::None);
    button.set_valign(gtk::Align::Center);
    button.set_tooltip_text(Some("Notification Center"));
    theme::class(&button, "bar-clock-button");

    let label = gtk::Label::new(Some(&clock_text(&format, show_date, show_seconds)));
    label.set_xalign(0.5);
    label.set_valign(gtk::Align::Center);
    theme::class(&label, "bar-clock");
    button.add(&label);

    let label_clone = label.clone();
    glib::timeout_add_seconds_local(1, move || {
        label_clone.set_text(&clock_text(&format, show_date, show_seconds));
        glib::ControlFlow::Continue
    });

    button.connect_clicked(|_| system::run_action("notifications.open"));
    button
}

fn clock_text(format: &str, show_date: bool, show_seconds: bool) -> String {
    let now = match glib::DateTime::now_local() {
        Ok(now) => now,
        Err(_) => return "--".to_string(),
    };
    let hour = now.hour();
    let minute = now.minute();
    let second = now.second();

    let (hour, suffix) = if format == "12h" {
        let suffix = if hour >= 12 { "PM" } else { "AM" };
        let hour = if hour % 12 == 0 { 12 } else { hour % 12 };
        (hour, suffix)
    } else {
        (hour, "")
    };

    let time = if show_seconds {
        if suffix.is_empty() {
            format!("{hour:02}:{minute:02}:{second:02}")
        } else {
            format!("{hour}:{minute:02}:{second:02} {suffix}")
        }
    } else if suffix.is_empty() {
        format!("{hour:02}:{minute:02}")
    } else {
        format!("{hour}:{minute:02} {suffix}")
    };

    if !show_date {
        return time;
    }

    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let month = MONTHS[((now.month() as usize).saturating_sub(1)) % 12];
    format!("{month} {:02}  {time}", now.day_of_month())
}
