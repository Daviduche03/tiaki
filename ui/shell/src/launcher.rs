//! Applications overlay launcher, matching macOS Applications modal style.
//!
//! Filter tabs, live search, 7-column squircle app grid, keyboard navigation,
//! and trackpad gesture integration.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use gtk_layer_shell::{Edge, KeyboardMode, Layer};

use crate::{glyphs, icons, model, single, system, theme};

const COLUMNS: usize = 7;

/// Stylesheets owned by this module, in cascade order.
pub const STYLE_SHEETS: &[&str] = &[include_str!("../css/launcher.css")];

pub fn run(root: &agentic_ui_tree::Node) {
    let Some(_instance) = single::Instance::acquire("tiaki-launcher", "launcher", false) else {
        return;
    };

    let all_apps = model::launcher_apps(root);
    let current_cat: Rc<RefCell<String>> = Rc::new(RefCell::new("All".to_string()));
    let search_query: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));
    let filtered: Rc<RefCell<Vec<model::AppItem>>> = Rc::new(RefCell::new(all_apps.clone()));
    let cells: Rc<RefCell<Vec<gtk::EventBox>>> = Rc::new(RefCell::new(Vec::new()));
    let selected: Rc<RefCell<usize>> = Rc::new(RefCell::new(0));

    let window = theme::base_window("launcher-window");
    theme::setup_layer(
        &window,
        "tiaki-launcher",
        Layer::Overlay,
        KeyboardMode::Exclusive,
        0,
    );
    for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        theme::anchor(&window, edge);
    }

    let scrim = gtk::EventBox::new();
    scrim.set_visible_window(true);
    theme::class(&scrim, "home-scrim");

    let center_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    center_box.set_halign(gtk::Align::Center);
    center_box.set_valign(gtk::Align::Center);

    // Modal Applications Window
    let modal = gtk::Box::new(gtk::Orientation::Vertical, 0);
    modal.set_size_request(1040, 680);
    theme::class(&modal, "app-window");

    // 1. Header Bar
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    header.set_valign(gtk::Align::Center);
    theme::class(&header, "app-header");

    let title_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    title_box.set_valign(gtk::Align::Center);
    let app_icon = glyphs::status_icon_tinted("apps", 20, (0.902, 0.878, 0.831));
    app_icon.set_valign(gtk::Align::Center);
    title_box.pack_start(&app_icon, false, false, 0);

    let title_lbl = gtk::Label::new(Some("Applications"));
    title_lbl.set_valign(gtk::Align::Center);
    theme::class(&title_lbl, "app-title");
    title_box.pack_start(&title_lbl, false, false, 0);
    header.pack_start(&title_box, false, false, 0);

    // Category Tabs
    let tabs_box = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    tabs_box.set_valign(gtk::Align::Center);
    theme::class(&tabs_box, "app-tabs");

    let categories = [
        "All",
        "Developer Tools",
        "Productivity & Finance",
        "Social",
        "Utilities",
        "Creativity",
        "Information & Reading",
    ];

    let tab_buttons: Rc<RefCell<Vec<(String, gtk::Button)>>> = Rc::new(RefCell::new(Vec::new()));

    // Search entry on the right
    let search_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    search_box.set_valign(gtk::Align::Center);
    search_box.set_size_request(180, 32);
    theme::class(&search_box, "app-search-box");

    let search_ico = glyphs::status_icon_tinted("search", 13, (0.6, 0.58, 0.55));
    search_box.pack_start(&search_ico, false, false, 0);

    let entry = gtk::Entry::new();
    entry.set_has_frame(false);
    entry.set_placeholder_text(Some("Search Apps"));
    entry.set_hexpand(true);
    entry.set_valign(gtk::Align::Center);
    theme::class(&entry, "app-search-entry");
    search_box.pack_start(&entry, true, true, 0);

    header.pack_start(&tabs_box, true, true, 0);
    header.pack_end(&search_box, false, false, 0);
    modal.pack_start(&header, false, false, 0);

    // 2. Scrolled Grid Body
    let scroll = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_hexpand(true);
    scroll.set_vexpand(true);
    theme::class(&scroll, "app-scroll");

    let grid = gtk::Grid::new();
    grid.set_row_spacing(16);
    grid.set_column_spacing(16);
    grid.set_column_homogeneous(true);
    grid.set_halign(gtk::Align::Center);
    grid.set_valign(gtk::Align::Start);
    theme::class(&grid, "app-grid");
    scroll.add(&grid);
    modal.pack_start(&scroll, true, true, 0);

    // 3. Footer
    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    theme::class(&footer, "app-footer");
    let hint = gtk::Label::new(Some("↑↓←→ navigate    ↵ open    esc close"));
    hint.set_xalign(0.5);
    theme::class(&hint, "app-hint");
    footer.pack_start(&hint, true, true, 0);
    modal.pack_start(&footer, false, false, 0);

    center_box.pack_start(&modal, false, false, 0);
    scrim.add(&center_box);
    window.add(&scrim);

    // Filtering logic helper
    let apply_filters = {
        let all_apps = all_apps.clone();
        let grid = grid.clone();
        let filtered = filtered.clone();
        let cells = cells.clone();
        let selected = selected.clone();
        let window = window.clone();
        let current_cat = current_cat.clone();
        let search_query = search_query.clone();
        move || {
            let cat = current_cat.borrow().clone();
            let query = search_query.borrow().to_lowercase();
            let items: Vec<model::AppItem> = all_apps
                .iter()
                .filter(|app| {
                    let matches_cat = if cat == "All" {
                        true
                    } else {
                        app.category.eq_ignore_ascii_case(&cat)
                    };
                    let matches_query = if query.is_empty() {
                        true
                    } else {
                        app.label.to_lowercase().contains(&query)
                            || app.description.to_lowercase().contains(&query)
                    };
                    matches_cat && matches_query
                })
                .cloned()
                .collect();
            rebuild(&grid, items, &filtered, &cells, &selected, &window);
        }
    };

    // Build Category Tabs
    for category in categories {
        let btn = gtk::Button::with_label(category);
        btn.set_relief(gtk::ReliefStyle::None);
        theme::class(&btn, "app-tab");
        if category == "All" {
            btn.style_context().add_class("active");
        }
        let category_str = category.to_string();
        let current_cat = current_cat.clone();
        let tab_buttons_clone = tab_buttons.clone();
        let apply_filters_clone = apply_filters.clone();
        btn.connect_clicked(move |_| {
            *current_cat.borrow_mut() = category_str.clone();
            for (name, b) in tab_buttons_clone.borrow().iter() {
                if *name == category_str {
                    b.style_context().add_class("active");
                } else {
                    b.style_context().remove_class("active");
                }
            }
            apply_filters_clone();
        });
        tabs_box.pack_start(&btn, false, false, 0);
        tab_buttons.borrow_mut().push((category.to_string(), btn));
    }

    // Search entry listener
    let search_query_clone = search_query.clone();
    let apply_filters_for_search = apply_filters.clone();
    entry.connect_changed(move |e| {
        *search_query_clone.borrow_mut() = e.text().to_string();
        apply_filters_for_search();
    });

    // Keyboard navigation
    let window_for_keys = window.clone();
    let cells_for_keys = cells.clone();
    let selected_for_keys = selected.clone();
    let filtered_for_keys = filtered.clone();
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

    // Click outside modal on scrim closes window
    let window_for_click = window.clone();
    scrim.connect_button_press_event(move |widget, event| {
        if event.window() == widget.window() {
            window_for_click.close();
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });

    window.connect_destroy(|_| gtk::main_quit());

    // Initial build
    apply_filters();

    window.show_all();
    entry.grab_focus();
    window.present();
    gtk::main();
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

    for (index, app) in filtered.borrow().iter().enumerate() {
        let item = gtk::EventBox::new();
        item.set_visible_window(true);
        item.set_size_request(118, 108);
        theme::class(&item, "app-cell");

        let cell = gtk::Box::new(gtk::Orientation::Vertical, 6);
        cell.set_halign(gtk::Align::Center);
        cell.set_valign(gtk::Align::Center);

        let icon = icons::app_icon(&app.art, 56);
        icon.set_halign(gtk::Align::Center);
        theme::class(&icon, "app-icon-wrap");
        cell.pack_start(&icon, false, false, 0);

        let label = gtk::Label::new(Some(&app.label));
        label.set_xalign(0.5);
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        label.set_width_chars(12);
        label.set_max_width_chars(12);
        theme::class(&label, "app-label");
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
