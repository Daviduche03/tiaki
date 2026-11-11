//! Home-screen launcher.
//!
//! Dot-matrix clock, search pill, fixed app grid, real system/calendar widgets,
//! inference/ledger status, page dots, and a dock — over the current wallpaper.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use gtk_layer_shell::{Edge, KeyboardMode, Layer};

use crate::{clock, icons, model, single, system, theme};

const COLUMNS: usize = 6;

/// Stylesheets owned by this module, in cascade order.
/// Add a file to `ui/shell/css/` and list it here to grow them.
pub const STYLE_SHEETS: &[&str] = &[include_str!("../css/launcher.css")];

pub fn run(root: &agentic_ui_tree::Node) {
    let Some(_instance) = single::Instance::acquire("tiaki-launcher", "launcher", false) else {
        return;
    };

    let apps = model::launcher_apps(root);
    let filtered: Rc<RefCell<Vec<model::AppItem>>> = Rc::new(RefCell::new(apps.clone()));
    let cells: Rc<RefCell<Vec<gtk::EventBox>>> = Rc::new(RefCell::new(Vec::new()));
    let selected: Rc<RefCell<usize>> = Rc::new(RefCell::new(0));

    let window = theme::base_window("launcher-window");
    // The bar is layered below this overlay, so the launcher sees through to
    // the desktop while it is open.
    theme::setup_layer(
        &window,
        "tiaki-launcher",
        Layer::Top,
        KeyboardMode::Exclusive,
        0,
    );
    for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        theme::anchor(&window, edge);
    }

    let screen_width = gdk::Display::default()
        .and_then(|display| display.primary_monitor())
        .map(|monitor| monitor.geometry().width())
        .unwrap_or(1440);

    let scrim = gtk::EventBox::new();
    scrim.set_visible_window(true);
    theme::class(&scrim, "home-scrim");

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.set_halign(gtk::Align::Center);
    content.set_valign(gtk::Align::Fill);
    content.set_hexpand(false);
    content.set_size_request((screen_width - 160).max(960), -1);

    let top_spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    top_spacer.set_vexpand(true);
    content.pack_start(&top_spacer, true, true, 0);

    let clock = clock::label_clock();
    clock.set_halign(gtk::Align::Center);
    content.pack_start(&clock, false, false, 0);

    let date = gtk::Label::new(Some(&date_text()));
    date.set_xalign(0.5);
    theme::class(&date, "home-date");
    content.pack_start(&date, false, false, 2);

    let date_clone = date.clone();
    glib::timeout_add_seconds_local(30, move || {
        date_clone.set_text(&date_text());
        glib::ControlFlow::Continue
    });

    let search_pill = gtk::EventBox::new();
    search_pill.set_visible_window(true);
    search_pill.set_size_request(480, 48);
    search_pill.set_halign(gtk::Align::Center);
    search_pill.set_margin_top(18);
    theme::class(&search_pill, "home-search-pill");
    let search_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    search_row.set_margin_start(20);
    search_row.set_margin_end(20);
    search_row.set_valign(gtk::Align::Center);
    let search_icon = icons::status_icon("search", 19);
    search_icon.set_valign(gtk::Align::Center);
    search_row.pack_start(&search_icon, false, false, 0);
    let entry = gtk::Entry::new();
    entry.set_has_frame(false);
    entry.set_placeholder_text(Some("Search Apps"));
    entry.set_hexpand(true);
    entry.set_valign(gtk::Align::Center);
    theme::class(&entry, "home-search-entry");
    search_row.pack_start(&entry, true, true, 0);
    search_pill.add(&search_row);
    content.pack_start(&search_pill, false, false, 0);

    let grid = gtk::Grid::new();
    grid.set_row_spacing(6);
    grid.set_column_spacing(8);
    grid.set_column_homogeneous(true);
    grid.set_halign(gtk::Align::Center);
    grid.set_valign(gtk::Align::Start);
    theme::class(&grid, "home-grid");

    let left = gtk::Box::new(gtk::Orientation::Vertical, 16);
    left.set_size_request(260, -1);
    left.set_valign(gtk::Align::Start);
    left.set_hexpand(false);
    left.pack_start(&system_widget(), false, false, 0);
    left.pack_start(&calendar_widget(), false, false, 0);

    let right = gtk::Box::new(gtk::Orientation::Vertical, 16);
    right.set_size_request(260, -1);
    right.set_valign(gtk::Align::Start);
    right.set_hexpand(false);
    right.pack_start(&inference_widget(), false, false, 0);
    right.pack_start(&ledger_widget(root), false, false, 0);

    let home_body = gtk::Box::new(gtk::Orientation::Horizontal, 24);
    home_body.set_halign(gtk::Align::Center);
    home_body.set_margin_top(22);
    home_body.set_valign(gtk::Align::Start);
    home_body.set_hexpand(false);
    home_body.pack_start(&left, false, false, 0);
    home_body.pack_start(&grid, false, false, 0);
    home_body.pack_start(&right, false, false, 0);
    content.pack_start(&home_body, false, false, 0);

    let spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    spacer.set_vexpand(true);
    content.pack_start(&spacer, true, true, 0);

    let dots = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    dots.set_halign(gtk::Align::Center);
    dots.set_margin_bottom(12);
    for index in 0..3 {
        let dot = gtk::EventBox::new();
        dot.set_visible_window(true);
        dot.set_size_request(7, 7);
        theme::class(&dot, "home-dot");
        if index == 0 {
            theme::class(&dot, "active");
        }
        dots.pack_start(&dot, false, false, 0);
    }
    content.pack_start(&dots, false, false, 0);

    let dock = home_dock(root, &window);
    dock.set_margin_bottom(20);
    content.pack_start(&dock, false, false, 0);

    let hint = gtk::Label::new(Some("↑↓ navigate    ↵ open    esc close"));
    hint.set_xalign(0.5);
    hint.set_margin_top(10);
    theme::class(&hint, "home-hint");
    content.pack_start(&hint, false, false, 0);

    let backdrop = gtk::Overlay::new();
    backdrop.add(&wallpaper_image(screen_width));
    scrim.add(&content);
    backdrop.add_overlay(&scrim);
    window.add(&backdrop);

    let all_apps = apps.clone();
    let grid_for_search = grid.clone();
    let filtered_for_search = filtered.clone();
    let cells_for_search = cells.clone();
    let selected_for_search = selected.clone();
    let window_for_search = window.clone();
    entry.connect_changed(move |entry| {
        let items = filter(&all_apps, &entry.text().to_string());
        rebuild(
            &grid_for_search,
            items,
            &filtered_for_search,
            &cells_for_search,
            &selected_for_search,
            &window_for_search,
        );
    });

    let window_for_keys = window.clone();
    let filtered_for_keys = filtered.clone();
    let cells_for_keys = cells.clone();
    let selected_for_keys = selected.clone();
    entry.connect_key_press_event(move |_, event| match event.keyval() {
        gdk::keys::constants::Escape => {
            window_for_keys.close();
            glib::Propagation::Stop
        }
        gdk::keys::constants::Left => {
            step_selection(&cells_for_keys, &selected_for_keys, -1);
            glib::Propagation::Stop
        }
        gdk::keys::constants::Right => {
            step_selection(&cells_for_keys, &selected_for_keys, 1);
            glib::Propagation::Stop
        }
        gdk::keys::constants::Up => {
            step_selection(&cells_for_keys, &selected_for_keys, -(COLUMNS as i32));
            glib::Propagation::Stop
        }
        gdk::keys::constants::Down => {
            step_selection(&cells_for_keys, &selected_for_keys, COLUMNS as i32);
            glib::Propagation::Stop
        }
        gdk::keys::constants::Return | gdk::keys::constants::KP_Enter => {
            let index = *selected_for_keys.borrow();
            if let Some(app) = filtered_for_keys.borrow().get(index).cloned() {
                open(&app, &window_for_keys);
            }
            glib::Propagation::Stop
        }
        _ => glib::Propagation::Proceed,
    });

    let scrim_for_click = scrim.clone();
    let window_for_click = window.clone();
    scrim.connect_button_press_event(move |_, event| {
        if event.window() == scrim_for_click.window() {
            window_for_click.close();
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });

    window.connect_destroy(|_| gtk::main_quit());

    rebuild(&grid, apps, &filtered, &cells, &selected, &window);
    window.show_all();
    entry.grab_focus();
    window.present();
    gtk::main();
}

fn home_dock(root: &agentic_ui_tree::Node, window: &gtk::Window) -> gtk::Box {
    let dock = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    dock.set_halign(gtk::Align::Center);
    theme::class(&dock, "home-dock");

    for app in model::dock_apps(root) {
        if app.art == "trash" {
            let separator = gtk::Separator::new(gtk::Orientation::Vertical);
            theme::class(&separator, "dock-sep");
            dock.pack_start(&separator, false, false, 0);
        }
        let button = gtk::Button::new();
        button.set_relief(gtk::ReliefStyle::None);
        theme::class(&button, "home-dock-item");
        button.set_tooltip_text(Some(&app.label));
        button.add(&icons::app_icon(&app.art, 50));
        let command = app.command.clone();
        let window = window.clone();
        button.connect_clicked(move |_| {
            system::open_command(&command);
            window.close();
        });
        dock.pack_start(&button, false, false, 0);
    }
    dock
}

fn widget_shell(title: &str, icon: &str) -> (gtk::EventBox, gtk::Box) {
    let card = gtk::EventBox::new();
    card.set_visible_window(true);
    card.set_size_request(260, 126);
    theme::class(&card, "home-widget");

    let body = gtk::Box::new(gtk::Orientation::Vertical, 6);
    body.set_margin_top(16);
    body.set_margin_bottom(16);
    body.set_margin_start(18);
    body.set_margin_end(18);

    let heading = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    heading.pack_start(&icons::status_icon(icon, 15), false, false, 0);
    let title = gtk::Label::new(Some(title));
    title.set_xalign(0.0);
    theme::class(&title, "widget-title");
    heading.pack_start(&title, true, true, 0);
    body.pack_start(&heading, false, false, 0);

    card.add(&body);
    (card, body)
}

fn system_widget() -> gtk::EventBox {
    let (card, body) = widget_shell("SYSTEM", "gear");
    let value = gtk::Label::new(Some(&cpu_load()));
    value.set_xalign(0.0);
    theme::class(&value, "widget-big");
    body.pack_start(&value, false, false, 0);
    let label = gtk::Label::new(Some("load average"));
    label.set_xalign(0.0);
    theme::class(&label, "widget-sub");
    body.pack_start(&label, false, false, 0);

    let memory = gtk::ProgressBar::new();
    memory.set_fraction(memory_used() / 100.0);
    memory.set_show_text(false);
    theme::class(&memory, "home-meter");
    let memory_label = gtk::Label::new(Some(&format!("memory {:.0}%", memory_used())));
    memory_label.set_xalign(0.0);
    theme::class(&memory_label, "widget-sub");
    body.pack_start(&memory_label, false, false, 0);
    body.pack_start(&memory, false, false, 0);
    card
}

fn calendar_widget() -> gtk::EventBox {
    let (card, body) = widget_shell("CALENDAR", "clock");
    let (day, sub) = calendar_text();
    let value = gtk::Label::new(Some(&day));
    value.set_xalign(0.0);
    theme::class(&value, "widget-big");
    body.pack_start(&value, false, false, 0);
    let label = gtk::Label::new(Some(&sub));
    label.set_xalign(0.0);
    theme::class(&label, "widget-sub");
    body.pack_start(&label, false, false, 0);
    card
}

fn inference_widget() -> gtk::EventBox {
    let (card, body) = widget_shell("INFERENCE", "inference");
    let value = gtk::Label::new(Some("local"));
    value.set_xalign(0.0);
    theme::class(&value, "widget-big");
    body.pack_start(&value, false, false, 0);
    let label = gtk::Label::new(Some("broker · guarded"));
    label.set_xalign(0.0);
    theme::class(&label, "widget-sub");
    body.pack_start(&label, false, false, 0);
    card
}

fn ledger_widget(root: &agentic_ui_tree::Node) -> gtk::EventBox {
    let (card, body) = widget_shell("LEDGER", "lock");
    let activity = model::find_by_id(root, "act.1")
        .and_then(|node| node.prop_str("value"))
        .unwrap_or("committed");
    let value = gtk::Label::new(Some("committed"));
    value.set_xalign(0.0);
    theme::class(&value, "widget-big");
    body.pack_start(&value, false, false, 0);
    let label = gtk::Label::new(Some(if activity.contains("user:ada") {
        "user:ada · tree"
    } else {
        "reversible · attributable"
    }));
    label.set_xalign(0.0);
    theme::class(&label, "widget-sub");
    body.pack_start(&label, false, false, 0);
    card
}

fn rebuild(
    grid: &gtk::Grid,
    items: Vec<model::AppItem>,
    filtered: &Rc<RefCell<Vec<model::AppItem>>>,
    cells: &Rc<RefCell<Vec<gtk::EventBox>>>,
    selected: &Rc<RefCell<usize>>,
    window: &gtk::Window,
) {
    for child in grid.children() {
        grid.remove(&child);
    }
    cells.borrow_mut().clear();
    *filtered.borrow_mut() = items;

    // Reserve all six columns even when the filter returns fewer apps, so the
    // side widgets and the dock do not shift while typing.
    for column in 0..COLUMNS {
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_size_request(112, 0);
        grid.attach(&spacer, column as i32, 99, 1, 1);
    }

    for (index, app) in filtered.borrow().iter().enumerate() {
        let item = gtk::EventBox::new();
        item.set_visible_window(true);
        item.set_size_request(112, 116);
        theme::class(&item, "home-cell");

        let cell = gtk::Box::new(gtk::Orientation::Vertical, 7);
        cell.set_halign(gtk::Align::Center);
        cell.set_valign(gtk::Align::Center);
        let icon = icons::app_icon(&app.art, 80);
        icon.set_halign(gtk::Align::Center);
        cell.pack_start(&icon, false, false, 0);

        let label = gtk::Label::new(Some(&app.label));
        label.set_xalign(0.5);
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        label.set_width_chars(10);
        label.set_max_width_chars(10);
        theme::class(&label, "home-label");
        cell.pack_start(&label, false, false, 0);

        item.add(&cell);
        let command = app.command.clone();
        let window = window.clone();
        item.connect_button_press_event(move |_, event| {
            if event.button() == 1 {
                system::open_command(&command);
                window.close();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        grid.attach(
            &item,
            (index % COLUMNS) as i32,
            (index / COLUMNS) as i32,
            1,
            1,
        );
        cells.borrow_mut().push(item);
    }

    grid.show_all();
    *selected.borrow_mut() = 0;
    update_selection(cells, selected);
}

fn update_selection(cells: &Rc<RefCell<Vec<gtk::EventBox>>>, selected: &Rc<RefCell<usize>>) {
    let index = *selected.borrow();
    for (position, button) in cells.borrow().iter().enumerate() {
        if position == index {
            button.style_context().add_class("selected");
        } else {
            button.style_context().remove_class("selected");
        }
    }
}

fn step_selection(
    cells: &Rc<RefCell<Vec<gtk::EventBox>>>,
    selected: &Rc<RefCell<usize>>,
    delta: i32,
) {
    let length = cells.borrow().len();
    if length == 0 {
        return;
    }
    let next = (*selected.borrow() as i32 + delta).clamp(0, length as i32 - 1) as usize;
    *selected.borrow_mut() = next;
    update_selection(cells, selected);
}

fn open(app: &model::AppItem, window: &gtk::Window) {
    system::open_command(&app.command);
    window.close();
}

fn wallpaper_image(screen_width: i32) -> gtk::Image {
    let height = gdk::Display::default()
        .and_then(|display| display.primary_monitor())
        .map(|monitor| monitor.geometry().height())
        .unwrap_or(900);
    let path = wallpaper_path();
    let image =
        match gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(&path, screen_width, height, false) {
            Ok(pixbuf) => gtk::Image::from_pixbuf(Some(&pixbuf)),
            Err(_) => gtk::Image::new(),
        };
    image.set_hexpand(true);
    image.set_vexpand(true);
    image.set_halign(gtk::Align::Fill);
    image.set_valign(gtk::Align::Fill);
    image
}

fn wallpaper_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/desktop".to_string());
    let state = PathBuf::from(home).join(".config/tiaki/wallpaper");
    let name = std::fs::read_to_string(state)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "drawn-01-ink-field.jpg".to_string());
    let path = PathBuf::from("/usr/share/tiaki/wallpapers").join(name);
    if path.exists() {
        path
    } else {
        PathBuf::from("/usr/share/tiaki/wallpapers/drawn-01-ink-field.jpg")
    }
}

fn filter(apps: &[model::AppItem], query: &str) -> Vec<model::AppItem> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return apps.to_vec();
    }
    apps.iter()
        .filter(|app| {
            app.label.to_lowercase().contains(&query)
                || app.category.to_lowercase().contains(&query)
                || app.description.to_lowercase().contains(&query)
        })
        .cloned()
        .collect()
}

fn cpu_load() -> String {
    std::fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|text| {
            text.split_whitespace()
                .next()
                .and_then(|value| value.parse::<f64>().ok())
        })
        .map(|value| format!("{value:.2}"))
        .unwrap_or_else(|| "0.00".to_string())
}

fn memory_used() -> f64 {
    let text = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let mut total: f64 = 0.0;
    let mut available: f64 = 0.0;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("MemTotal:") {
            total = value
                .split_whitespace()
                .next()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.0);
        }
        if let Some(value) = line.strip_prefix("MemAvailable:") {
            available = value
                .split_whitespace()
                .next()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.0);
        }
    }
    if total > 0.0 {
        ((total - available) / total * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    }
}

fn calendar_text() -> (String, String) {
    const WEEKDAYS: [&str; 7] = [
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
    ];
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    match glib::DateTime::now_local() {
        Ok(now) => (
            now.day_of_month().to_string(),
            format!(
                "{} · {}",
                WEEKDAYS[(now.day_of_week() as usize) % 7],
                MONTHS[((now.month() as usize).saturating_sub(1)) % 12]
            ),
        ),
        Err(_) => ("--".to_string(), String::new()),
    }
}

fn date_text() -> String {
    const WEEKDAYS: [&str; 7] = [
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
    ];
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    match glib::DateTime::now_local() {
        Ok(now) => format!(
            "{}, {} {}",
            WEEKDAYS[(now.day_of_week() as usize) % 7],
            now.day_of_month(),
            MONTHS[((now.month() as usize).saturating_sub(1)) % 12]
        ),
        Err(_) => String::new(),
    }
}
