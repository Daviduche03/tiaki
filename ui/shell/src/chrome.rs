//! Window management overlay: one 30px titlebar per window, floating
//! placement, cascade, and macOS-style zoom.
//!
//! Sway has no titlebars, so the shell draws them. The traffic lights are
//! real: close kills the view, yellow moves it to the scratchpad, green
//! toggles fullscreen (macOS behaviour). Double-clicking the titlebar zooms
//! (macOS "zoom"), filling the workspace and restoring on the second click.
//!
//! Apps open as floating windows, cascaded like macOS. A window is placed
//! once, the first time it is seen, and the placement survives overlay
//! restarts. Tiled windows are still available through the Window menu.
//!
//! Visibility rule: the focused window always carries its bar. An unfocused
//! window keeps its bar only while no other window overlaps its band, so a
//! titlebar never paints on top of a window above it.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;
use gtk_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::windows::{self, View};
use crate::{single, theme};

const HEIGHT: i32 = 30;
const DEFAULT_W: i32 = 1100;
const DEFAULT_H: i32 = 700;
const CASCADE_X: i32 = 110;
const CASCADE_Y: i32 = 90;
const CASCADE_STEP: i32 = 26;
const CASCADE_WRAP: u32 = 4;
const MARGIN: i32 = 16;
const BAR: i32 = 30;

struct Chrome {
    window: gtk::Window,
    bar: gtk::EventBox,
    title: gtk::Label,
    visible: bool,
}

struct Placement {
    placed: BTreeSet<i64>,
    slot: u32,
    path: String,
}

impl Placement {
    fn load() -> Self {
        let base = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
        let path = format!("{base}/tiaki-wm-placed");
        let mut placed = BTreeSet::new();
        let mut slot = 0;
        if let Ok(text) = std::fs::read_to_string(&path) {
            let mut lines = text.lines();
            if let Some(first) = lines.next() {
                slot = first.trim().parse().unwrap_or(0);
            }
            for line in lines {
                for id in line.split(',') {
                    if let Ok(id) = id.trim().parse() {
                        placed.insert(id);
                    }
                }
            }
        }
        Self { placed, slot, path }
    }

    fn save(&self) {
        let ids: Vec<String> = self.placed.iter().map(i64::to_string).collect();
        let _ = std::fs::write(&self.path, format!("{}\n{}\n", self.slot, ids.join(",")));
    }
}

/// Stylesheets owned by this module, in cascade order.
/// Add a file to `ui/shell/css/` and list it here to grow them.
pub const STYLE_SHEETS: &[&str] = &[include_str!("../css/chrome.css")];

pub fn run() {
    // One overlay process. A second launch replaces the first instead of
    // leaving two sets of titlebars fighting over the same bands.
    let Some(_instance) = single::Instance::acquire("tiaki-chrome", "chrome", true) else {
        return;
    };

    let windows: Rc<RefCell<BTreeMap<i64, Chrome>>> = Rc::new(RefCell::new(BTreeMap::new()));
    let placement = Rc::new(RefCell::new(Placement::load()));
    refresh(&windows, &placement);

    let windows_for_timer = windows.clone();
    let placement_for_timer = placement.clone();
    gtk::glib::timeout_add_local(Duration::from_millis(500), move || {
        refresh(&windows_for_timer, &placement_for_timer);
        gtk::glib::ControlFlow::Continue
    });

    gtk::main();
}

fn refresh(windows: &Rc<RefCell<BTreeMap<i64, Chrome>>>, placement: &Rc<RefCell<Placement>>) {
    let views = windows::views();
    let ids: BTreeSet<i64> = views.iter().map(|view| view.id).collect();

    let mut windows = windows.borrow_mut();
    let stale: Vec<i64> = windows
        .keys()
        .filter(|id| !ids.contains(id))
        .copied()
        .collect();
    for id in stale {
        if let Some(chrome) = windows.remove(&id) {
            chrome.window.close();
        }
    }

    for view in &views {
        place(placement, view);

        // A titlebar is a layer surface: it always paints above every window.
        // Showing one for a window that something else covers would draw the
        // bar on top of the other window, so only the focused window and
        // unobstructed windows keep theirs.
        let obstructed = views.iter().any(|other| {
            other.id != view.id
                && rects_overlap(
                    view.x,
                    view.y - HEIGHT,
                    view.width,
                    HEIGHT,
                    other.x,
                    other.y,
                    other.width,
                    other.height,
                )
        });
        let visible = view.focused || !obstructed;

        if let Some(chrome) = windows.get_mut(&view.id) {
            update(chrome, view);
            set_visible(chrome, visible);
        } else {
            let mut chrome = create(view);
            set_visible(&mut chrome, visible);
            windows.insert(view.id, chrome);
        }
    }
}

/// Float and cascade a window the first time it is seen.
fn place(placement: &Rc<RefCell<Placement>>, view: &View) {
    let mut state = placement.borrow_mut();
    if state.placed.contains(&view.id) {
        return;
    }
    // Windows that manage their own geometry (Settings, Monitor) arrive
    // floating with a size already chosen; leave them where they are.
    if !view.floating {
        let slot = state.slot % CASCADE_WRAP;
        let x = CASCADE_X + slot as i32 * CASCADE_STEP;
        let y = CASCADE_Y + slot as i32 * CASCADE_STEP;
        windows::sway(
            view.id,
            &format!("floating enable, resize set {DEFAULT_W} {DEFAULT_H}"),
        );
        windows::move_absolute(view.id, x, y);
        state.slot = state.slot.wrapping_add(1);
    }
    state.placed.insert(view.id);
    state.save();
}

fn rects_overlap(ax: i32, ay: i32, aw: i32, ah: i32, bx: i32, by: i32, bw: i32, bh: i32) -> bool {
    ax < bx + bw && bx < ax + aw && ay < by + bh && by < ay + ah
}

fn set_visible(chrome: &mut Chrome, visible: bool) {
    if chrome.visible == visible {
        return;
    }
    chrome.visible = visible;
    if visible {
        chrome.window.show_all();
    } else {
        chrome.window.hide();
    }
}

fn create(view: &View) -> Chrome {
    let window = theme::base_window("chrome-window");
    theme::setup_layer(&window, "tiaki-chrome", Layer::Top, KeyboardMode::None, 0);
    theme::anchor(&window, Edge::Top);
    theme::anchor(&window, Edge::Left);
    window.set_size_request(view.width, HEIGHT);

    let bar = gtk::EventBox::new();
    bar.set_visible_window(true);
    theme::class(&bar, "chrome-bar");

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    row.set_margin_start(8);
    row.set_margin_end(10);
    row.set_valign(gtk::Align::Center);
    row.pack_start(
        &traffic_button("red", &format!("[con_id={}] kill", view.id), "Close"),
        false,
        false,
        0,
    );
    row.pack_start(
        &traffic_button(
            "yellow",
            &format!("[con_id={}] move scratchpad", view.id),
            "Minimize",
        ),
        false,
        false,
        0,
    );
    row.pack_start(
        &traffic_button(
            "green",
            &format!("[con_id={}] fullscreen toggle", view.id),
            "Enter full screen",
        ),
        false,
        false,
        0,
    );

    let title = gtk::Label::new(Some(&windows::title(view)));
    title.set_xalign(0.5);
    title.set_hexpand(true);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_max_width_chars(48);
    theme::class(&title, "chrome-title");
    row.pack_start(&title, true, true, 0);

    bar.add(&row);
    window.add(&bar);

    let id = view.id;
    bar.connect_button_press_event(move |_, event| {
        // A double click on the titlebar zooms the window, like macOS.
        if event.event_type() == gtk::gdk::EventType::DoubleButtonPress && event.button() == 1 {
            zoom(id);
        } else if event.button() == 1 {
            windows::sway_plain(&format!("[con_id={id}] focus"));
        }
        gtk::glib::Propagation::Proceed
    });

    let chrome = Chrome {
        window,
        bar,
        title,
        visible: true,
    };
    update_geometry(&chrome, view);
    chrome.window.show_all();
    chrome
}

/// Zoom the focused window (used by the Window menu and ⌘↑).
pub fn zoom_focused() {
    if let Some(view) = windows::views().into_iter().find(|view| view.focused) {
        zoom(view.id);
    }
}

/// macOS zoom: fill the workspace, restore the previous geometry on toggle.
/// The saved rectangle lives in a small side file so the overlay process and
/// the double-click handler agree even if the bar was rebuilt.
fn zoom(id: i64) {
    let base = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    let path = format!("{base}/tiaki-zoom-{id}");
    let (width, height) = windows::output_size();
    let target = (
        MARGIN,
        BAR + MARGIN,
        width - MARGIN * 2,
        height - BAR - MARGIN * 2,
    );

    if let Ok(text) = std::fs::read_to_string(&path) {
        // Second click: restore.
        let parts: Vec<i32> = text
            .split_whitespace()
            .filter_map(|part| part.parse().ok())
            .collect();
        if parts.len() == 4 {
            windows::sway(id, &format!("resize set {} {}", parts[2], parts[3]));
            windows::move_absolute(id, parts[0], parts[1]);
        }
        let _ = std::fs::remove_file(&path);
        return;
    }

    let current = windows::views().into_iter().find(|view| view.id == id);
    if let Some(view) = current {
        let _ = std::fs::write(
            &path,
            format!("{} {} {} {}\n", view.x, view.y, view.width, view.height),
        );
        windows::sway(
            id,
            &format!("floating enable, resize set {} {}", target.2, target.3),
        );
        windows::move_absolute(id, target.0, target.1);
    }
}

fn update(chrome: &mut Chrome, view: &View) {
    update_geometry(chrome, view);
    chrome.title.set_text(&windows::title(view));
    if view.focused {
        chrome.bar.style_context().add_class("focused");
        chrome.bar.style_context().remove_class("inactive");
    } else {
        chrome.bar.style_context().add_class("inactive");
        chrome.bar.style_context().remove_class("focused");
    }
}

fn update_geometry(chrome: &Chrome, view: &View) {
    // Layer surfaces are positioned below the bar's exclusive zone, so the
    // margin is relative to y=30, not the output origin.
    let top = (view.y - HEIGHT - BAR).max(0);
    chrome.window.set_layer_shell_margin(Edge::Left, view.x);
    chrome.window.set_layer_shell_margin(Edge::Top, top);
    chrome.window.resize(view.width, HEIGHT);
    chrome.window.set_size_request(view.width, HEIGHT);
}

fn traffic_button(color: &str, command: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::new();
    button.set_relief(gtk::ReliefStyle::None);
    button.set_size_request(12, 12);
    button.set_tooltip_text(Some(tooltip));
    theme::class(&button, "traffic");
    theme::class(&button, color);

    let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    dot.set_size_request(12, 12);
    theme::class(&dot, "traffic-dot");
    theme::class(&dot, color);
    button.add(&dot);

    let command = command.to_string();
    button.connect_clicked(move |_| windows::sway_plain(&command));
    button
}
