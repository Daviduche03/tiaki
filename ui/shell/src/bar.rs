//! The top bar: brand, menus, the approval signal, machine facts, status, and
//! the clock.
//!
//! Ink chrome on every wallpaper. One lamp signal, and it means something:
//! the approval chip exists only while an approval is outstanding. Every
//! label, menu, and status item still comes from the UI tree.

use gtk::glib;
use gtk::prelude::*;
use gtk_layer_shell::{Edge, KeyboardMode, Layer};

use crate::{icons, model, notify, single, system, theme};

/// Stylesheets owned by this module, in cascade order.
pub const STYLE_SHEETS: &[&str] = &[include_str!("../css/bar.css")];

const BAR_HEIGHT: i32 = 36;

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

    // A windowed surface: GtkBox backgrounds do not composite reliably on a
    // non-focusable layer surface, but an EventBox does.
    let shell = gtk::EventBox::new();
    shell.set_visible_window(true);
    shell.set_size_request(-1, BAR_HEIGHT);
    theme::class(&shell, "bar");
    // Painted here as well as styled in CSS: the bar must stay opaque even if a
    // theme or state drops the box background.
    shell.connect_draw(|widget, cr| {
        let width = widget.allocated_width() as f64;
        let height = widget.allocated_height() as f64;
        cr.set_source_rgb(0.082, 0.078, 0.074); // #151413
        let _ = cr.paint();
        cr.set_source_rgba(0.902, 0.878, 0.831, 0.14);
        cr.set_line_width(1.0);
        cr.move_to(0.0, height - 0.5);
        cr.line_to(width, height - 0.5);
        let _ = cr.stroke();
        gtk::glib::Propagation::Proceed
    });

    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    bar.set_size_request(-1, BAR_HEIGHT);

    // brand: the beacon and the name, one hit target
    bar.pack_start(&brand_button(bar_node), false, true, 0);
    bar.pack_start(&divider(), false, true, 0);
    bar.pack_start(&workspace_indicator(), false, true, 0);
    bar.pack_start(&divider(), false, true, 0);

    // App menus belong to the focused app, not the desktop. With no
    // window focused they hide, leaving brand + system cluster.
    let mut menu_buttons: Vec<gtk::Button> = Vec::new();
    for menu in model::menus_in(bar_node) {
        let button = menu_button(menu, &window);
        bar.pack_start(&button, false, true, 0);
        menu_buttons.push(button);
    }
    let menus_for_focus = menu_buttons.clone();
    glib::timeout_add_seconds_local(1, move || {
        let focused = !crate::windows::focused_app_id().is_empty();
        for button in &menus_for_focus {
            if focused {
                button.show();
            } else {
                button.hide();
            }
        }
        glib::ControlFlow::Continue
    });

    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    bar.pack_start(&spacer, true, true, 0);

    // the only lamp on screen, when there is an approval waiting
    let (chip, chip_label) = approval_chip();
    bar.pack_start(&chip, false, true, 0);

    bar.pack_start(&generation_label(bar_node), false, true, 0);
    bar.pack_start(&divider(), false, true, 0);

    for item in model::status_items(bar_node) {
        if let Some(widget) = status_item(&item) {
            bar.pack_start(&widget, false, true, 0);
        }
    }
    bar.pack_start(&divider(), false, true, 0);

    let (bell, badge) = notify::bell_button(&window);
    bar.pack_start(&bell, false, true, 0);

    if let Some(node) = bar_node
        .children
        .iter()
        .find(|child| child.id == "bar.control")
    {
        bar.pack_start(&status_button(node, &window), false, true, 0);
    }

    bar.pack_start(&divider(), false, true, 0);
    bar.pack_start(&clock_label(bar_node), false, true, 0);

    shell.add(&bar);
    window.add(&shell);
    window.show_all();

    // The store is a directory on disk, so the bar watches it rather than
    // being told about changes. Two seconds is cheap and never wrong.
    let chip_for_timer = chip.clone();
    let label_for_timer = chip_label.clone();
    let badge_for_timer = badge.clone();
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
        if notify::count() == 0 {
            badge_for_timer.set_no_show_all(true);
            badge_for_timer.hide();
        } else {
            badge_for_timer.set_no_show_all(false);
            badge_for_timer.show_all();
        }
        glib::ControlFlow::Continue
    });

    gtk::main();
}

fn divider() -> gtk::Widget {
    let rule = gtk::Separator::new(gtk::Orientation::Vertical);
    rule.set_size_request(1, 14);
    rule.set_valign(gtk::Align::Center);
    theme::class(&rule, "bar-divider");
    rule.upcast()
}

fn brand_button(bar: &agentic_ui_tree::Node) -> gtk::Button {
    let node = bar.children.iter().find(|child| child.id == "bar.logo");
    let action = node
        .and_then(|node| node.prop_str("action"))
        .unwrap_or("palette.open")
        .to_string();
    let value = node
        .and_then(|node| node.prop_str("label"))
        .unwrap_or("Tiaki");

    let button = gtk::Button::new();
    button.set_relief(gtk::ReliefStyle::None);
    button.set_valign(gtk::Align::Center);
    button.set_tooltip_text(Some("Tiaki — open the launcher"));
    theme::class(&button, "logo-button");

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.set_valign(gtk::Align::Center);
    row.pack_start(&icons::logo_mark(16), false, false, 0);
    let wordmark = gtk::Label::new(Some(value));
    wordmark.set_valign(gtk::Align::Center);
    theme::class(&wordmark, "bar-wordmark");
    row.pack_start(&wordmark, false, false, 0);
    button.add(&row);

    button.connect_clicked(move |_| system::run_action(&action));
    button
}

fn workspace_indicator() -> gtk::Widget {
    let box_widget = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    box_widget.set_valign(gtk::Align::Center);
    theme::class(&box_widget, "bar-workspaces");

    let initial_active = crate::windows::focused_workspace();
    let mut dots = Vec::new();
    for n in 1..=5 {
        let dot = gtk::EventBox::new();
        dot.set_visible_window(true);
        dot.set_size_request(8, 8);
        dot.set_valign(gtk::Align::Center);
        theme::class(&dot, "bar-ws");
        if n == initial_active {
            dot.style_context().add_class("bar-ws-on");
        }
        dot.set_tooltip_text(Some(&format!("Workspace {n}")));
        dot.connect_button_press_event(move |_, event| {
            if event.button() == 1 {
                crate::windows::sway_plain(&format!("workspace number {n}"));
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        box_widget.pack_start(&dot, false, false, 0);
        dots.push((n, dot));
    }

    let dots_for_timer = dots;
    glib::timeout_add_seconds_local(1, move || {
        let active = crate::windows::focused_workspace();
        for (n, dot) in &dots_for_timer {
            if *n == active {
                dot.style_context().add_class("bar-ws-on");
            } else {
                dot.style_context().remove_class("bar-ws-on");
            }
        }
        glib::ControlFlow::Continue
    });

    box_widget.upcast()
}

fn approval_chip() -> (gtk::Button, gtk::Label) {
    let button = gtk::Button::new();
    button.set_relief(gtk::ReliefStyle::None);
    button.set_valign(gtk::Align::Center);
    button.set_tooltip_text(Some("Approve or decline the waiting change"));
    theme::class(&button, "bar-chip");
    button.set_no_show_all(true);
    button.hide();

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 7);
    row.set_valign(gtk::Align::Center);
    let dot = gtk::EventBox::new();
    dot.set_visible_window(true);
    dot.set_size_request(6, 6);
    theme::class(&dot, "bar-chip-dot");
    row.pack_start(&dot, false, false, 0);

    let label = gtk::Label::new(Some("1 approval"));
    label.set_valign(gtk::Align::Center);
    theme::class(&label, "bar-chip-label");
    row.pack_start(&label, false, false, 0);

    button.add(&row);
    button.connect_clicked(|_| system::run_action("notifications.open"));
    (button, label)
}

fn generation_label(bar: &agentic_ui_tree::Node) -> gtk::Label {
    let value = bar
        .children
        .iter()
        .find(|child| child.id == "bar.generation")
        .and_then(|node| node.prop_str("value"))
        .unwrap_or("gen a");
    let label = gtk::Label::new(Some(value));
    label.set_valign(gtk::Align::Center);
    theme::class(&label, "bar-meta");
    label.set_tooltip_text(Some("The generation this session booted from"));
    label
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
        // The app under focus owns the menu contents; read it at click
        // time so the popup filters File/Edit/View for that app.
        let app = crate::windows::focused_app_id();
        launch_popup("menu", &format!("{menu_id} {x} {app}"));
    });
    button
}

fn launch_popup(mode: &str, args: &str) {
    system::spawn(&format!("{} {mode} {args}", system::self_exe()));
}

fn status_item(item: &str) -> Option<gtk::Widget> {
    // Read-only indicators. Only the sliders button opens Control Center
    // and only the bell opens Notifications — wifi/battery opening a
    // panel was a wrong shortcut.
    match item {
        "wifi" | "network" => Some(wrap_status(&icons::status_icon("wifi", 22))),
        "audio" => Some(wrap_status(&icons::status_icon("audio", 22))),
        "battery" => Some(wrap_status(&icons::status_icon("battery", 22))),
        _ => None,
    }
}

fn wrap_status(widget: &impl IsA<gtk::Widget>) -> gtk::Widget {
    let holder = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    holder.set_size_request(-1, 26);
    holder.set_valign(gtk::Align::Center);
    holder.set_halign(gtk::Align::Center);
    theme::class(&holder, "status-item");
    if let Some(widget) = widget.dynamic_cast_ref::<gtk::Widget>() {
        widget.set_valign(gtk::Align::Center);
    }
    holder.pack_start(widget, false, false, 0);
    holder.upcast()
}

fn status_button(node: &agentic_ui_tree::Node, window: &gtk::Window) -> gtk::Widget {
    let action = node
        .prop_str("action")
        .unwrap_or("control.open")
        .to_string();

    let button = gtk::Button::new();
    button.set_relief(gtk::ReliefStyle::None);
    button.set_valign(gtk::Align::Center);
    button.add(&icons::status_icon("control", 22));
    theme::class(&button, "bar-icon-button");

    if action == "control.open" {
        button.set_tooltip_text(Some("Control Center"));
        let window = window.clone();
        button.connect_clicked(move |button| {
            let x = button
                .translate_coordinates(&window, 0, 0)
                .map(|(x, _)| x)
                .unwrap_or(0);
            launch_popup("control", &x.to_string());
        });
    } else {
        button.connect_clicked(move |_| system::run_action(&action));
    }

    button.upcast()
}

fn clock_label(bar: &agentic_ui_tree::Node) -> gtk::Label {
    let node = model::clock_node(bar);
    let format = node
        .and_then(|node| node.prop_str("format"))
        .unwrap_or("24h")
        .to_string();
    let show_date = node
        .and_then(|node| node.prop_bool("show_date"))
        .unwrap_or(false);
    let show_seconds = node
        .and_then(|node| node.prop_bool("show_seconds"))
        .unwrap_or(false);

    let label = gtk::Label::new(Some(&clock_text(&format, show_date, show_seconds)));
    label.set_xalign(1.0);
    label.set_valign(gtk::Align::Center);
    theme::class(&label, "bar-clock");

    let label_clone = label.clone();
    glib::timeout_add_seconds_local(1, move || {
        label_clone.set_text(&clock_text(&format, show_date, show_seconds));
        glib::ControlFlow::Continue
    });

    label
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
    format!("{weekday} {} {month}  {time}", now.day_of_month())
}
