//! The floating dock with macOS-style physical magnification and spring bounce.
//!
//! Pinned applications, running indicators, and the separator come straight
//! from the `dock` panel in the tree. The dock floats above windows with smooth
//! fisheye Gaussian magnification on hover, decaying spring launch bounce, and
//! a floating label badge above the hovered icon.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use gtk::gdk;
use gtk::prelude::*;
use gtk_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::{icons, model, single, system, theme, windows};

/// Stylesheets owned by this module, in cascade order.
pub const STYLE_SHEETS: &[&str] = &[include_str!("../css/dock.css")];

const BASE_ICON_SIZE: f64 = 36.0;
const PEAK_ICON_SIZE: f64 = 52.0;
const MAGNIFY_RADIUS: f64 = 110.0;

struct AppItemWidget {
    app: model::AppItem,
    button: gtk::Button,
    drawing_area: gtk::DrawingArea,
    pixbuf: gtk::gdk_pixbuf::Pixbuf,
    current_size: Cell<f64>,
    target_size: Cell<f64>,
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
    window.set_layer_shell_margin(Edge::Bottom, 4);

    let container = gtk::Box::new(gtk::Orientation::Vertical, 4);
    container.set_valign(gtk::Align::End);
    container.set_halign(gtk::Align::Center);

    // Floating label pill above the dock
    let label_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    label_box.set_halign(gtk::Align::Center);
    let floating_label = gtk::Label::new(None);
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

    let panel = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    theme::class(&panel, "dock");
    panel.set_valign(gtk::Align::End);

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
                panel.pack_start(
                    &icons::separator(gtk::Orientation::Vertical),
                    false,
                    false,
                    0,
                );
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
            let cur_size = item_clone.current_size.get();
            let alloc_w = widget.allocated_width() as f64;
            let alloc_h = widget.allocated_height() as f64;

            let mut y_bounce = 0.0;
            if let Some(start) = item_clone.bounce_start.get() {
                let elapsed = start.elapsed().as_secs_f64();
                if elapsed < 0.85 {
                    let decay = (-3.2 * elapsed).exp();
                    let bounce = (elapsed * 18.0).sin().abs() * decay;
                    y_bounce = -20.0 * bounce;
                }
            }

            let _ = cr.save();
            let x_off = (alloc_w - cur_size) / 2.0;
            let y_off = (alloc_h - cur_size) + y_bounce;

            cr.translate(x_off, y_off);
            let pb_w = item_clone.pixbuf.width() as f64;
            if pb_w > 0.0 {
                let scale = cur_size / pb_w;
                cr.scale(scale, scale);
                gdk::cairo_set_source_pixbuf(cr, &item_clone.pixbuf, 0.0, 0.0);
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
            gtk::glib::timeout_add_local(std::time::Duration::from_millis(16), move || {
                let mut still_animating = false;

                for item in items.iter() {
                    let cur = item.current_size.get();
                    let tgt = item.target_size.get();
                    let diff = tgt - cur;
                    if diff.abs() > 0.25 {
                        let new_size = cur + diff * 0.35;
                        item.current_size.set(new_size);
                        item.drawing_area.queue_draw();
                        item.button
                            .set_size_request((new_size + 10.0) as i32, (PEAK_ICON_SIZE + 14.0) as i32);
                        still_animating = true;
                    } else if (cur - tgt).abs() > 0.01 {
                        item.current_size.set(tgt);
                        item.drawing_area.queue_draw();
                        item.button
                            .set_size_request((tgt + 10.0) as i32, (PEAK_ICON_SIZE + 14.0) as i32);
                    }

                    if let Some(start) = item.bounce_start.get() {
                        let elapsed = start.elapsed().as_secs_f64();
                        if elapsed < 0.85 {
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

    // Track mouse motion across dock for continuous Gaussian magnification
    {
        let items = items_rc.clone();
        let start_tick = start_animation_tick.clone();
        let flabel = floating_label.clone();
        event_box.connect_motion_notify_event(move |_, event| {
            let (mouse_x, _) = event.position();
            let mut closest_label: Option<&str> = None;
            let mut min_dist = f64::MAX;

            for item in items.iter() {
                let alloc = item.button.allocation();
                let center_x = alloc.x() as f64 + (alloc.width() as f64) / 2.0;
                let dist = (mouse_x - center_x).abs();

                let target = if dist < MAGNIFY_RADIUS {
                    let factor = ((dist / MAGNIFY_RADIUS) * std::f64::consts::FRAC_PI_2).cos().powi(2);
                    BASE_ICON_SIZE + (PEAK_ICON_SIZE - BASE_ICON_SIZE) * factor
                } else {
                    BASE_ICON_SIZE
                };
                item.target_size.set(target);

                if dist < min_dist {
                    min_dist = dist;
                    closest_label = Some(&item.app.label);
                }
            }

            if min_dist < 40.0 {
                if let Some(lbl) = closest_label {
                    flabel.set_text(lbl);
                    flabel.set_opacity(1.0);
                }
            } else {
                flabel.set_opacity(0.0);
            }

            start_tick();
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
                item.target_size.set(BASE_ICON_SIZE);
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
            let app_windows: Vec<_> = windows::views()
                .into_iter()
                .filter(|view| view.app_id == app_id)
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

fn create_dock_item(app: &model::AppItem) -> Rc<AppItemWidget> {
    let button = gtk::Button::new();
    button.set_relief(gtk::ReliefStyle::None);
    theme::class(&button, "dock-item");
    button.set_size_request((BASE_ICON_SIZE + 10.0) as i32, (PEAK_ICON_SIZE + 14.0) as i32);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.set_valign(gtk::Align::End);

    // High resolution pixbuf for smooth Cairo scaling
    let path = format!("/usr/share/tiaki/icons/{}.svg", app.art);
    let pixbuf = gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(&path, 96, 96, true)
        .unwrap_or_else(|_| {
            gtk::gdk_pixbuf::Pixbuf::new(gtk::gdk_pixbuf::Colorspace::Rgb, true, 8, 96, 96)
                .expect("failed to create fallback pixbuf")
        });

    let drawing_area = gtk::DrawingArea::new();
    drawing_area.set_size_request(-1, (PEAK_ICON_SIZE + 4.0) as i32);
    drawing_area.set_valign(gtk::Align::End);
    content.pack_start(&drawing_area, true, true, 0);

    let dot = gtk::Label::new(Some("•"));
    theme::class(&dot, "run-dot");
    if !app.running {
        theme::class(&dot, "hidden");
    }
    content.pack_start(&dot, false, false, 0);

    button.add(&content);

    Rc::new(AppItemWidget {
        app: app.clone(),
        button,
        drawing_area,
        pixbuf,
        current_size: Cell::new(BASE_ICON_SIZE),
        target_size: Cell::new(BASE_ICON_SIZE),
        bounce_start: Cell::new(None),
    })
}
