//! The top bar: authentic macOS frosted glass menu bar.
//! Left: Apple logo, Active App Title (bold), Menus (File, Edit, View, Window, Assistant)
//! Right: Orange privacy/agent chip, Sparkle AI Copilot, Bluetooth, Wi-Fi, Battery (61% [== ]),
//!        Spotlight Search, Control Center, and Clock (Fri Nov 13  15:01).

use gtk::glib;
use gtk::prelude::*;
use gtk_layer_shell::{Edge, KeyboardMode, Layer};

use crate::{icons, model, notify, single, system, theme};

/// Stylesheets owned by this module, in cascade order.
pub const STYLE_SHEETS: &[&str] = &[include_str!("../css/bar.css")];

const BAR_HEIGHT: i32 = 28;

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

    // Translucent frosted glass top bar
    window.connect_draw(|widget, cr| {
        let width = widget.allocated_width() as f64;
        let height = widget.allocated_height() as f64;

        // Clear to transparent so background is never opaque
        cr.set_operator(gtk::cairo::Operator::Source);
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
        let _ = cr.paint();
        cr.set_operator(gtk::cairo::Operator::Over);

        // Translucent frosted glass tint (showing ~60% wallpaper through with solid contrast)
        let pattern = gtk::cairo::LinearGradient::new(0.0, 0.0, 0.0, height);
        pattern.add_color_stop_rgba(0.0, 1.0, 1.0, 1.0, 0.22); // subtle top rim specular
        pattern.add_color_stop_rgba(0.15, 0.12, 0.10, 0.09, 0.35); // translucent glass body
        pattern.add_color_stop_rgba(1.0, 0.08, 0.07, 0.06, 0.48); // bottom glass depth
        let _ = cr.set_source(&pattern);
        let _ = cr.paint();

        // 1px bottom rim highlight
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.20);
        cr.set_line_width(1.0);
        cr.move_to(0.0, height - 0.5);
        cr.line_to(width, height - 0.5);
        let _ = cr.stroke();

        gtk::glib::Propagation::Proceed
    });

    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    bar.set_size_request(-1, BAR_HEIGHT);
    theme::class(&bar, "bar");

    // 1. Tiaki brand mark button (beacon icon)
    bar.pack_start(&tiaki_brand_button(bar_node), false, false, 0);

    // 2. Active application title in bold
    let (app_title_widget, app_title_label) = active_app_title();
    bar.pack_start(&app_title_widget, false, false, 0);

    // 3. Persistent application menus: File, Edit, View, Window, etc.
    for menu in model::menus_in(bar_node) {
        let button = menu_button(menu, &window);
        bar.pack_start(&button, false, false, 0);
    }

    // Dynamic focus tracker for app title
    let title_for_timer = app_title_label;
    glib::timeout_add_seconds_local(1, move || {
        let focused = crate::windows::focused_app_id();
        let name = match focused.as_str() {
            "foot" => "Terminal",
            "mousepad" => "Editor",
            "thunar" => "Files",
            "epiphany" => "Safari",
            "btop" => "Activity Monitor",
            "tiaki-shell" => "Settings",
            other if !other.is_empty() => other,
            _ => "Tiaki",
        };
        title_for_timer.set_text(name);
        glib::ControlFlow::Continue
    });

    // Flexible spacer pushing right cluster to the right
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    bar.pack_start(&spacer, true, true, 0);

    // Right cluster:
    // a. Privacy / Agent approval orange capsule
    let (chip, chip_label) = approval_chip();
    bar.pack_start(&chip, false, false, 0);

    // b. AI Copilot / Assistant Sparkle
    let spark = icon_action_button("spark", 16, "palette.open", "AI Copilot (⌘Space)");
    bar.pack_start(&spark, false, false, 0);

    // c. Bluetooth status
    let bt = icon_spawn_button("bluetooth", 16, "tiaki-shell settings bluetooth", "Bluetooth");
    bar.pack_start(&bt, false, false, 0);

    // d. Wi-Fi status
    let wifi = icon_spawn_button("wifi", 16, "tiaki-shell settings wifi", "Wi-Fi");
    bar.pack_start(&wifi, false, false, 0);

    // e. Battery percentage & horizontal icon
    let battery = battery_widget();
    bar.pack_start(&battery, false, false, 0);

    // f. Spotlight search (magnifying glass)
    let search = icon_action_button("search", 16, "palette.open", "Spotlight Search (⌘Space)");
    bar.pack_start(&search, false, false, 0);

    // g. Control Center
    let control = control_center_button(bar_node, &window);
    bar.pack_start(&control, false, false, 0);

    // h. Clock & Notification Center (Fri Nov 13  15:01)
    let clock = clock_button(bar_node);
    bar.pack_start(&clock, false, false, 0);

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
    button.set_tooltip_text(Some("Tiaki — System Menu"));
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
    button.set_tooltip_text(Some("Approvals & Agent Privacy"));
    theme::class(&button, "bar-chip");
    button.set_no_show_all(true);
    button.hide();

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 5);
    row.set_valign(gtk::Align::Center);
    row.pack_start(&icons::status_icon("mic", 13), false, false, 0);

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

fn battery_widget() -> gtk::Widget {
    let container = gtk::Box::new(gtk::Orientation::Horizontal, 3);
    container.set_valign(gtk::Align::Center);
    theme::class(&container, "bar-battery-box");
    container.set_tooltip_text(Some("Battery: 61% (Power Adapter Connected)"));

    let label = gtk::Label::new(Some("61%"));
    label.set_valign(gtk::Align::Center);
    theme::class(&label, "bar-battery-pct");
    container.pack_start(&label, false, false, 0);

    let icon = icons::status_icon("battery", 22);
    icon.set_valign(gtk::Align::Center);
    container.pack_start(&icon, false, false, 0);

    let label_timer = label;
    glib::timeout_add_seconds_local(10, move || {
        let pct = battery_capacity();
        label_timer.set_text(&format!("{pct}%"));
        glib::ControlFlow::Continue
    });

    container.upcast()
}

fn battery_capacity() -> u32 {
    for bat in &["BAT0", "BAT1", "macsmc-battery"] {
        let path = format!("/sys/class/power_supply/{bat}/capacity");
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(pct) = content.trim().parse::<u32>() {
                return pct;
            }
        }
    }
    61
}

fn control_center_button(bar: &agentic_ui_tree::Node, window: &gtk::Window) -> gtk::Button {
    let _ = bar;
    let button = gtk::Button::new();
    button.set_relief(gtk::ReliefStyle::None);
    button.set_valign(gtk::Align::Center);
    button.set_tooltip_text(Some("Control Center"));
    theme::class(&button, "bar-icon-button");
    button.add(&icons::status_icon("control", 18));

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
    label.set_xalign(1.0);
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

    const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let weekday = WEEKDAYS[(now.day_of_week() as usize) % 7];
    let month = MONTHS[((now.month() as usize).saturating_sub(1)) % 12];
    format!("{weekday} {month} {}  {time}", now.day_of_month())
}
