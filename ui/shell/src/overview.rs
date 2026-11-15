//! Mission Control overview: displays all windows across workspaces in a grid.
//!
//! Pressing Super+Up (or invoking `tiaki-shell overview`) opens a full-screen
//! overlay showing window cards. Clicking or hitting Return focuses that window,
//! bringing it to the active workspace. Escape dismisses the overview.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::gdk;
use gtk::gdk::prelude::GdkContextExt;
use gtk::prelude::*;
use gtk_layer_shell::{Edge, KeyboardMode, Layer};

use crate::windows::{self, View};
use crate::{icons, single, theme};

/// Stylesheets owned by this module, in cascade order.
pub const STYLE_SHEETS: &[&str] = &[include_str!("../css/overview.css")];

/// Space kept clear around the grid: the bar above, the dock below.
const MARGIN_SIDE: i32 = 64;
const MARGIN_TOP: i32 = theme::BAR_HEIGHT + 40;
const MARGIN_BOTTOM: i32 = 130;
const GAP: i32 = 28;
/// Height of the icon and title row under each thumbnail.
const LABEL_HEIGHT: i32 = 32;

pub fn run() {
    let Some(_instance) = single::Instance::acquire("tiaki-overview", "overview", true) else {
        return;
    };

    let all_windows = windows::views();
    if all_windows.is_empty() {
        return;
    }

    // One capture of the screen before the overlay exists, cropped per window
    // below. Sway offers no per-window capture, so a window another covers
    // shows what is on top of it, which is what you would see.
    let screen = capture_screen();

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

    let (screen_w, screen_h) = windows::output_size();

    // Fit every window at one shared scale, never above 1:1. One scale keeps
    // relative sizes honest: a terminal does not look as big as a browser.
    let count = all_windows.len() as i32;
    let cols = (count as f64).sqrt().ceil().max(1.0) as i32;
    let rows = ((count + cols - 1) / cols).max(1);
    let cell_w = (screen_w - 2 * MARGIN_SIDE - (cols - 1) * GAP) / cols;
    let cell_h = (screen_h - MARGIN_TOP - MARGIN_BOTTOM - (rows - 1) * GAP) / rows - LABEL_HEIGHT;
    let scale = all_windows
        .iter()
        .map(|view| {
            (cell_w as f64 / view.width.max(1) as f64)
                .min(cell_h as f64 / view.height.max(1) as f64)
        })
        .fold(1.0_f64, f64::min)
        .max(0.05);

    let grid = gtk::Grid::new();
    grid.set_column_spacing(GAP as u32);
    grid.set_row_spacing(GAP as u32);
    grid.set_column_homogeneous(true);
    grid.set_row_homogeneous(true);
    grid.set_halign(gtk::Align::Center);
    grid.set_valign(gtk::Align::Center);
    grid.set_margin_top(MARGIN_TOP - theme::BAR_HEIGHT);
    grid.set_margin_bottom(MARGIN_BOTTOM - 60);

    let cards: Rc<RefCell<Vec<(i64, gtk::EventBox)>>> = Rc::new(RefCell::new(Vec::new()));
    let cols = cols as usize;

    for (index, view) in all_windows.iter().enumerate() {
        let card = gtk::EventBox::new();
        card.set_visible_window(true);
        card.set_halign(gtk::Align::Center);
        // Centre, not fill: rows are homogeneous, so a filling card would draw
        // its selection ring down to the bottom of a taller row.
        card.set_valign(gtk::Align::Center);
        theme::class(&card, "overview-card");
        // Cards arrive as a cascade, not as one block.
        crate::motion::enter_after(&card, crate::motion::stagger_ms(index.min(12), 25));

        let card_box = gtk::Box::new(gtk::Orientation::Vertical, 0);

        let thumb_w = ((view.width as f64 * scale).round() as i32).max(120);
        let thumb_h = ((view.height as f64 * scale).round() as i32).max(80);
        let covered = !view.focused && covered_fraction(view, &all_windows) > 0.08;
        card_box.pack_start(
            &thumbnail(&screen, view, thumb_w, thumb_h, covered),
            false,
            false,
            0,
        );

        // Icon and name under the thumbnail.
        let label_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        label_row.set_halign(gtk::Align::Center);
        theme::class(&label_row, "overview-card-label");
        label_row.pack_start(&icons::app_icon(&art_for(view), 16), false, false, 0);
        let win_title = gtk::Label::new(Some(&windows::title(view)));
        win_title.set_max_width_chars(((thumb_w - 40) / 8).max(10));
        win_title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        theme::class(&win_title, "overview-card-title");
        label_row.pack_start(&win_title, false, false, 0);
        card_box.pack_start(&label_row, false, false, 0);

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

    scrim.add(&grid);
    window.add(&scrim);

    // Select the focused window first, falling back to the first card.
    let start = all_windows
        .iter()
        .position(|view| view.focused)
        .unwrap_or(0);
    let selected = Rc::new(RefCell::new(start));
    if let Some((_, card)) = cards.borrow().get(start) {
        card.style_context().add_class("selected");
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

/// The whole screen as a pixbuf, or None when `grim` is unavailable.
fn capture_screen() -> Option<gtk::gdk_pixbuf::Pixbuf> {
    let output = std::process::Command::new("grim").arg("-").output().ok()?;
    if !output.status.success() || output.stdout.is_empty() {
        return None;
    }
    let loader = gtk::gdk_pixbuf::PixbufLoader::new();
    loader.write(&output.stdout).ok()?;
    loader.close().ok()?;
    loader.pixbuf()
}

/// One window's picture, drawn to fit with the frame's corner radius and a
/// hairline. Falls back to the app icon when the window is off screen
/// (minimized, on another workspace) and there is nothing to crop.
fn thumbnail(
    screen: &Option<gtk::gdk_pixbuf::Pixbuf>,
    view: &View,
    width: i32,
    height: i32,
    covered: bool,
) -> gtk::DrawingArea {
    // A window another covers cannot be captured: the screen shows the
    // window on top of it. Those get the icon card, never someone else's
    // picture.
    let crop = screen.as_ref().filter(|_| !covered).and_then(|screen| {
        let x = view.x.max(0);
        let y = view.y.max(0);
        let w = (view.x + view.width).min(screen.width()) - x;
        let h = (view.y + view.height).min(screen.height()) - y;
        (w > 16 && h > 16).then(|| screen.new_subpixbuf(x, y, w, h))
    });
    let art = art_for(view);

    let area = gtk::DrawingArea::new();
    area.set_size_request(width, height);
    area.connect_draw(move |widget, cr| {
        use std::f64::consts::{FRAC_PI_2, PI};
        let w = widget.allocated_width() as f64;
        let h = widget.allocated_height() as f64;
        let r = 10.0;
        let (ir, ig, ib) = theme::INK;
        let (pr, pg, pb) = theme::PAPER;

        let outline = |cr: &gtk::cairo::Context| {
            cr.new_sub_path();
            cr.arc(w - r - 0.5, r + 0.5, r, -FRAC_PI_2, 0.0);
            cr.arc(w - r - 0.5, h - r - 0.5, r, 0.0, FRAC_PI_2);
            cr.arc(r + 0.5, h - r - 0.5, r, FRAC_PI_2, PI);
            cr.arc(r + 0.5, r + 0.5, r, PI, 1.5 * PI);
            cr.close_path();
        };

        let _ = cr.save();
        outline(cr);
        cr.clip();
        cr.set_source_rgb(ir, ig, ib);
        let _ = cr.paint();
        match &crop {
            Some(pixbuf) => {
                let sx = w / pixbuf.width() as f64;
                let sy = h / pixbuf.height() as f64;
                cr.scale(sx, sy);
                cr.set_source_pixbuf(pixbuf, 0.0, 0.0);
                let _ = cr.paint();
            }
            None => {
                if let Some(icon) = crate::icons::app_icon_pixbuf(&art, 64) {
                    cr.set_source_pixbuf(&icon, (w - 64.0) / 2.0, (h - 64.0) / 2.0);
                    let _ = cr.paint();
                }
            }
        }
        let _ = cr.restore();

        outline(cr);
        cr.set_source_rgba(pr, pg, pb, 0.16);
        cr.set_line_width(1.0);
        let _ = cr.stroke();
        gtk::glib::Propagation::Proceed
    });
    area
}

/// How much of `view` other windows overlap, as a fraction of its area.
fn covered_fraction(view: &View, all: &[View]) -> f64 {
    let area = (view.width.max(1) as f64) * (view.height.max(1) as f64);
    let mut covered = 0.0;
    for other in all.iter().filter(|other| other.id != view.id) {
        let w = (view.x + view.width).min(other.x + other.width) - view.x.max(other.x);
        let h = (view.y + view.height).min(other.y + other.height) - view.y.max(other.y);
        if w > 0 && h > 0 {
            covered += w as f64 * h as f64;
        }
    }
    (covered / area).min(1.0)
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
        if title.contains("files") || title.contains("trash") {
            return "files".to_string();
        }
        return "launcher".to_string();
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
