//! The dock: a low ink shelf, centred at the bottom of the screen.
//!
//! Pinned applications and the separator come straight from the `dock` panel
//! in the tree; running dots come from the live window list. Icons stand on
//! the shelf and grow upward out of it under the pointer, so the surface
//! never resizes while the pointer moves.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use gtk::gdk;
use gtk::gdk::prelude::GdkContextExt;
use gtk::prelude::*;
use gtk_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::motion::Spring;
use crate::{model, single, system, theme, windows};

/// Stylesheets owned by this module, in cascade order.
pub const STYLE_SHEETS: &[&str] = &[include_str!("../css/dock.css")];

const BASE_ICON_SIZE: f64 = 40.0;
const PEAK_ICON_SIZE: f64 = 52.0;
const MAGNIFY_RADIUS: f64 = 96.0;
/// Width of one icon slot. Wider than the base icon so neighbours have air,
/// and wide enough that a magnified icon never touches the next one.
const SLOT_WIDTH: i32 = 52;
/// How far a launching icon hops.
const BOUNCE_HEIGHT: f64 = 12.0;
/// Height of the icon canvas: a magnified icon plus the hop above it.
const CANVAS_HEIGHT: i32 = (PEAK_ICON_SIZE + BOUNCE_HEIGHT) as i32;
/// Shelf padding above the resting icons, and the shelf's corner radius.
const SHELF_PAD: f64 = 8.0;
const SHELF_RADIUS: f64 = 16.0;
const BOUNCE_SECONDS: f64 = 0.3;

struct AppItemWidget {
    app: model::AppItem,
    button: gtk::Button,
    drawing_area: gtk::DrawingArea,
    dot: gtk::Box,
    pixbuf: gtk::gdk_pixbuf::Pixbuf,
    size: Cell<Spring>,
    bounce_start: Cell<Option<Instant>>,
}

pub fn run(root: &agentic_ui_tree::Node) {
    let Some(_instance) = single::Instance::acquire("tiaki-dock", "dock", true) else {
        return;
    };

    let dock = model::find_by_id(root, "dock")
        .or_else(|| model::find_by_role(root, "dock"))
        .unwrap_or(root);

    let window = theme::base_window("dock-window");
    theme::setup_layer(&window, "tiaki-dock", Layer::Top, KeyboardMode::None, 0);
    theme::anchor(&window, Edge::Bottom);
    window.set_layer_shell_margin(Edge::Bottom, 8);

    let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
    container.set_valign(gtk::Align::End);
    container.set_halign(gtk::Align::Center);

    // Floating label pill above the dock
    let label_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    label_box.set_size_request(-1, 24);
    let floating_label = gtk::Label::new(None);
    floating_label.set_halign(gtk::Align::Start);
    theme::class(&floating_label, "dock-label-pill");
    floating_label.set_opacity(0.0);
    label_box.pack_start(&floating_label, false, false, 0);
    container.pack_start(&label_box, false, false, 0);

    // Dock pill panel wrapped in EventBox for hover tracking
    let event_box = gtk::EventBox::new();
    event_box.set_visible_window(false);
    event_box.add_events(
        gdk::EventMask::POINTER_MOTION_MASK
            | gdk::EventMask::LEAVE_NOTIFY_MASK
            | gdk::EventMask::ENTER_NOTIFY_MASK,
    );

    let panel = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    theme::class(&panel, "dock");
    panel.set_valign(gtk::Align::End);

    // The shelf: ink with one hairline, painted behind the icons. It starts
    // SHELF_PAD above the resting icons, so magnified icons rise out of it.
    panel.connect_draw(|widget, cr| {
        let width = widget.allocated_width() as f64;
        let height = widget.allocated_height() as f64;
        let top = CANVAS_HEIGHT as f64 - BASE_ICON_SIZE - SHELF_PAD;
        let (ir, ig, ib) = theme::INK;
        let (pr, pg, pb) = theme::PAPER;

        rounded_rect(cr, 0.5, top + 0.5, width - 1.0, height - top - 1.0, SHELF_RADIUS);
        cr.set_source_rgba(ir, ig, ib, 0.88);
        let _ = cr.fill_preserve();
        cr.set_source_rgba(pr, pg, pb, 0.14);
        cr.set_line_width(1.0);
        let _ = cr.stroke();
        gtk::glib::Propagation::Proceed
    });

    let mut items: Vec<Rc<AppItemWidget>> = Vec::new();

    for child in &dock.children {
        match child.kind.as_str() {
            "app" => {
                if let Some(app) = model::apps_in(&dock)
                    .into_iter()
                    .find(|app| app.id == child.id)
                {
                    let item = create_dock_item(&app);
                    panel.pack_start(&item.button, false, false, 0);
                    items.push(item);
                }
            }
            "separator" => {
                panel.pack_start(&shelf_rule(), false, false, 8);
            }
            _ => {}
        }
    }

    event_box.add(&panel);
    container.pack_start(&event_box, false, false, 0);
    window.add(&container);

    let items_rc = Rc::new(items);
    let is_animating = Rc::new(Cell::new(false));

    // Connect draw handlers to each drawing area
    for item in items_rc.iter() {
        let item_clone = item.clone();
        item.drawing_area.connect_draw(move |widget, cr| {
            let cur_size = item_clone.size.get().value;
            let alloc_w = widget.allocated_width() as f64;
            let alloc_h = widget.allocated_height() as f64;

            // Launch hop: a damped bounce that settles inside 300ms.
            let mut y_bounce = 0.0;
            if let Some(start) = item_clone.bounce_start.get() {
                let elapsed = start.elapsed().as_secs_f64();
                if elapsed < BOUNCE_SECONDS {
                    let decay = (-9.0 * elapsed).exp();
                    y_bounce = -BOUNCE_HEIGHT * (elapsed * 21.0).sin().abs() * decay;
                }
            }

            let _ = cr.save();
            let pb_w = item_clone.pixbuf.width() as f64;
            let pb_h = item_clone.pixbuf.height() as f64;

            if pb_w > 0.0 && pb_h > 0.0 {
                let scale = cur_size / pb_w.max(pb_h);
                let drawn_w = pb_w * scale;
                let drawn_h = pb_h * scale;

                // Icons stand on the canvas floor and grow upward.
                let x_off = (alloc_w - drawn_w) / 2.0;
                let y_off = alloc_h - drawn_h + y_bounce;

                cr.translate(x_off, y_off);
                cr.scale(scale, scale);
                cr.set_source_pixbuf(&item_clone.pixbuf, 0.0, 0.0);
                let _ = cr.paint();
            }
            let _ = cr.restore();

            gtk::glib::Propagation::Proceed
        });
    }

    // Motion animation tick driver
    let start_animation_tick = {
        let items = items_rc.clone();
        let is_animating = is_animating.clone();
        move || {
            if is_animating.get() {
                return;
            }
            is_animating.set(true);

            let items = items.clone();
            let is_animating = is_animating.clone();
            let mut last = Instant::now();
            gtk::glib::timeout_add_local(std::time::Duration::from_millis(8), move || {
                let now = Instant::now();
                let dt = (now - last).as_secs_f64();
                last = now;
                let mut still_animating = false;

                for item in items.iter() {
                    let mut spring = item.size.get();
                    let before = spring.value;
                    if spring.step(dt) {
                        still_animating = true;
                    }
                    item.size.set(spring);
                    if (spring.value - before).abs() > 0.0 {
                        item.drawing_area.queue_draw();
                    }

                    if let Some(start) = item.bounce_start.get() {
                        let elapsed = start.elapsed().as_secs_f64();
                        if elapsed < BOUNCE_SECONDS {
                            still_animating = true;
                            item.drawing_area.queue_draw();
                        } else {
                            item.bounce_start.set(None);
                            item.drawing_area.queue_draw();
                        }
                    }
                }

                if still_animating {
                    gtk::glib::ControlFlow::Continue
                } else {
                    is_animating.set(false);
                    gtk::glib::ControlFlow::Break
                }
            });
        }
    };

    // Shared magnification updater
    let update_magnification = {
        let items = items_rc.clone();
        let start_tick = start_animation_tick.clone();
        let flabel = floating_label.clone();
        let last_x = Cell::new(f64::NAN);
        move |mouse_x: f64| {
            let prev = last_x.get();
            if !prev.is_nan() && (mouse_x - prev).abs() < 1.0 {
                return;
            }
            last_x.set(mouse_x);

            let mut closest_label: Option<&str> = None;
            let mut closest_center = 0.0;
            let mut min_dist = f64::MAX;

            for item in items.iter() {
                let alloc = item.button.allocation();
                let center_x = alloc.x() as f64 + (alloc.width() as f64) / 2.0;
                let dist = (mouse_x - center_x).abs();

                let target = if dist < MAGNIFY_RADIUS {
                    let factor = 0.5 * (1.0 + ((dist / MAGNIFY_RADIUS) * std::f64::consts::PI).cos());
                    BASE_ICON_SIZE + (PEAK_ICON_SIZE - BASE_ICON_SIZE) * factor
                } else {
                    BASE_ICON_SIZE
                };
                let mut spring = item.size.get();
                spring.target = target;
                item.size.set(spring);

                if dist < min_dist {
                    min_dist = dist;
                    closest_center = center_x;
                    closest_label = Some(&item.app.label);
                }
            }

            if min_dist < SLOT_WIDTH as f64 / 2.0 {
                if let Some(lbl) = closest_label {
                    // Centre the name over its icon.
                    flabel.set_text(lbl);
                    let (_, width) = flabel.preferred_width();
                    let start = (closest_center - width as f64 / 2.0).max(0.0);
                    flabel.set_margin_start(start as i32);
                    flabel.set_opacity(1.0);
                }
            } else {
                flabel.set_opacity(0.0);
            }

            start_tick();
        }
    };

    // Track mouse motion across event_box
    {
        let update_mag = update_magnification.clone();
        event_box.connect_motion_notify_event(move |_, event| {
            let (mouse_x, _) = event.position();
            update_mag(mouse_x);
            gtk::glib::Propagation::Proceed
        });
    }

    // Also connect motion notify on every button so child hover propagates seamlessly
    for item in items_rc.iter() {
        let panel_ref = panel.clone();
        let update_mag = update_magnification.clone();
        item.button.add_events(gdk::EventMask::POINTER_MOTION_MASK);
        item.button.connect_motion_notify_event(move |btn, event| {
            let (bx, by) = event.position();
            if let Some((px, _)) = btn.translate_coordinates(&panel_ref, bx as i32, by as i32) {
                update_mag(px as f64);
            }
            gtk::glib::Propagation::Proceed
        });
    }

    // When pointer leaves the dock, reset all target sizes back to base
    {
        let items = items_rc.clone();
        let start_tick = start_animation_tick.clone();
        let flabel = floating_label.clone();
        event_box.connect_leave_notify_event(move |_, _| {
            for item in items.iter() {
                let mut spring = item.size.get();
                spring.target = BASE_ICON_SIZE;
                item.size.set(spring);
            }
            flabel.set_opacity(0.0);
            start_tick();
            gtk::glib::Propagation::Proceed
        });
    }

    // Connect clicked to launch & bounce
    for item in items_rc.iter() {
        let item_clone = item.clone();
        let start_tick = start_animation_tick.clone();
        let command = item.app.command.clone();
        item.button.connect_clicked(move |_| {
            // Trigger bouncy spring launch animation
            item_clone.bounce_start.set(Some(Instant::now()));
            start_tick();

            let app_id = windows::app_id_for_command(&command);
            let label = item_clone.app.label.clone();
            let app_windows: Vec<_> = windows::views()
                .into_iter()
                .filter(|view| owns(&app_id, &label, view))
                .collect();
            match app_windows.len() {
                0 => {
                    windows::sway_plain(&format!("[app_id=\"{app_id}\"] scratchpad show"));
                    if windows::is_action(&command) {
                        system::run_action(command.trim_start_matches("tiaki:"));
                    } else {
                        system::open_command(&command);
                    }
                }
                1 => {
                    windows::sway_plain(&format!("[con_id={}] scratchpad show", app_windows[0].id));
                    windows::sway_plain(&format!("[con_id={}] focus", app_windows[0].id));
                }
                _ => system::open_switcher(Some(&app_id)),
            }
        });
    }

    window.show_all();

    // Running dots follow the real window list, not the manifest.
    let refresh_dots = {
        let items = items_rc.clone();
        move || {
            let views = windows::views();
            for item in items.iter() {
                let app_id = windows::app_id_for_command(&item.app.command);
                let running = views.iter().any(|view| owns(&app_id, &item.app.label, view));
                let context = item.dot.style_context();
                if running {
                    context.remove_class("hidden");
                } else {
                    context.add_class("hidden");
                }
            }
        }
    };
    refresh_dots();
    gtk::glib::timeout_add_seconds_local(2, move || {
        refresh_dots();
        gtk::glib::ControlFlow::Continue
    });

    // The launcher has its own dock; hide the desktop dock while it is open.
    let window_for_visibility = window.clone();
    gtk::glib::timeout_add_local(std::time::Duration::from_millis(250), move || {
        if launcher_open() {
            window_for_visibility.hide();
        } else {
            window_for_visibility.show();
        }
        gtk::glib::ControlFlow::Continue
    });

    gtk::main();
}

fn launcher_open() -> bool {
    let base = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    let pid: Option<u32> = std::fs::read_to_string(format!("{base}/tiaki-launcher.lock/pid"))
        .ok()
        .and_then(|text| text.trim().parse().ok());
    let Some(pid) = pid else {
        return false;
    };
    std::fs::read_to_string(format!("/proc/{pid}/cmdline"))
        .map(|command| {
            let mut args = command.split('\0');
            let exe = args.next().unwrap_or_default();
            let is_shell =
                exe.ends_with("tiaki-shell") || exe.ends_with("tiaki-shell (deleted)");
            is_shell && command.split('\0').any(|arg| arg == "launcher")
        })
        .unwrap_or(false)
}

/// Whether a window belongs to a dock item. The shell's own apps (Files,
/// Settings, Agents) share one app_id, so those are told apart by title.
fn owns(app_id: &str, label: &str, view: &windows::View) -> bool {
    if view.app_id != app_id {
        return false;
    }
    if app_id != "tiaki-shell" {
        return true;
    }
    view.name.to_lowercase().contains(&label.to_lowercase())
}

fn rounded_rect(cr: &gtk::cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    use std::f64::consts::{FRAC_PI_2, PI};
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -FRAC_PI_2, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, FRAC_PI_2);
    cr.arc(x + r, y + h - r, r, FRAC_PI_2, PI);
    cr.arc(x + r, y + r, r, PI, 1.5 * PI);
    cr.close_path();
}

/// The hairline between apps and the trash, standing on the shelf.
fn shelf_rule() -> gtk::DrawingArea {
    let rule = gtk::DrawingArea::new();
    rule.set_size_request(1, CANVAS_HEIGHT);
    rule.set_valign(gtk::Align::Start);
    rule.connect_draw(|widget, cr| {
        let height = widget.allocated_height() as f64;
        let (pr, pg, pb) = theme::PAPER;
        cr.set_source_rgba(pr, pg, pb, 0.14);
        cr.rectangle(0.0, height - BASE_ICON_SIZE + 4.0, 1.0, BASE_ICON_SIZE - 8.0);
        let _ = cr.fill();
        gtk::glib::Propagation::Proceed
    });
    rule
}

fn create_dock_item(app: &model::AppItem) -> Rc<AppItemWidget> {
    let button = gtk::Button::new();
    button.set_relief(gtk::ReliefStyle::None);
    theme::class(&button, "dock-item");

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.set_halign(gtk::Align::Center);

    // High resolution pixbuf for smooth Cairo scaling
    let path = format!("/usr/share/tiaki/icons/{}.svg", app.art);
    let pixbuf = gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(&path, 128, 128, true)
        .unwrap_or_else(|_| {
            gtk::gdk_pixbuf::Pixbuf::new(gtk::gdk_pixbuf::Colorspace::Rgb, true, 8, 128, 128)
                .expect("failed to create fallback pixbuf")
        });

    let drawing_area = gtk::DrawingArea::new();
    drawing_area.set_size_request(SLOT_WIDTH, CANVAS_HEIGHT);
    content.pack_start(&drawing_area, false, false, 0);

    let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    dot.set_size_request(4, 4);
    dot.set_halign(gtk::Align::Center);
    theme::class(&dot, "run-dot");
    theme::class(&dot, "hidden");
    content.pack_start(&dot, false, false, 0);

    button.add(&content);

    Rc::new(AppItemWidget {
        app: app.clone(),
        button,
        drawing_area,
        dot,
        pixbuf,
        size: Cell::new(Spring::snappy(BASE_ICON_SIZE)),
        bounce_start: Cell::new(None),
    })
}
