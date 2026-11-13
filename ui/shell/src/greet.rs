//! The sign-in screen: who is sitting down, and under what mode.
//!
//! No password. One principal per row, `return` to enter, `g` to flip the
//! mode. Sway starts this instead of the desktop shell; on confirm the
//! greeter records the session, spawns the desktop pieces, and exits.

use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use gtk_layer_shell::{Edge, KeyboardMode, Layer};

use crate::{icons, single, system, theme};

/// Stylesheets owned by this module, in cascade order.
pub const STYLE_SHEETS: &[&str] = &[include_str!("../css/greet.css")];

const WALLPAPERS: [&str; 2] = [
    "/usr/share/tiaki/wallpapers/photo-03-mist-pines.jpg",
    "/usr/share/tiaki/wallpapers/drawn-01-ink-field.jpg",
];

struct Principal {
    name: &'static str,
    id: &'static str,
}

// v1: one row. More principals arrive as more entries; the up/down handling
// below already walks the list.
const PRINCIPALS: [Principal; 1] = [Principal {
    name: "Ada",
    id: "user:ada",
}];

pub fn run() {
    // First boot belongs to setup. It returns here when the machine knows
    // its owner.
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/desktop".to_string());
    if !std::path::Path::new(&format!("{home}/.config/tiaki/setup-done")).exists() {
        crate::setup::run();
    }

    let Some(_instance) = single::Instance::acquire("tiaki-greet", "greet", false) else {
        return;
    };

    let window = theme::base_window("greet-window");
    theme::setup_layer(
        &window,
        "tiaki-greet",
        Layer::Overlay,
        KeyboardMode::Exclusive,
        0,
    );
    for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        theme::anchor(&window, edge);
    }
    // Anchored to all edges the layer surface already fills the screen;
    // a toplevel fullscreen() would unmap it instead.

    let backdrop = gtk::Overlay::new();
    backdrop.add(&wallpaper());
    backdrop.add_overlay(&veil());

    let state = std::rc::Rc::new(std::cell::RefCell::new(State {
        selected: 0,
        guarded: true,
    }));

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.set_halign(gtk::Align::Fill);
    content.set_valign(gtk::Align::Fill);
    content.set_vexpand(true);
    content.set_hexpand(true);

    let clock = gtk::Label::new(Some(&time_text()));
    clock.set_halign(gtk::Align::Center);
    clock.set_margin_top(96);
    theme::class(&clock, "greet-clock");
    content.pack_start(&clock, false, false, 0);

    let date = gtk::Label::new(Some(&date_text()));
    date.set_halign(gtk::Align::Center);
    date.set_margin_top(10);
    theme::class(&date, "greet-date");
    content.pack_start(&date, false, false, 0);

    let clock_clone = clock.clone();
    let date_clone = date.clone();
    glib::timeout_add_seconds_local(1, move || {
        clock_clone.set_text(&time_text());
        date_clone.set_text(&date_text());
        glib::ControlFlow::Continue
    });

    let spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    spacer.set_vexpand(true);
    content.pack_start(&spacer, true, true, 0);

    let rows = gtk::Box::new(gtk::Orientation::Vertical, 0);
    rows.set_halign(gtk::Align::Center);
    theme::class(&rows, "greet-row-wrap");
    content.pack_start(&rows, false, false, 0);
    draw_rows(&rows, &state.borrow());

    let mode = gtk::Label::new(Some(&mode_text(true)));
    mode.set_halign(gtk::Align::Center);
    mode.set_margin_top(12);
    theme::class(&mode, "greet-mode");
    content.pack_start(&mode, false, false, 0);

    let rule = gtk::Separator::new(gtk::Orientation::Horizontal);
    theme::class(&rule, "greet-rule");
    rule.set_margin_start(48);
    rule.set_margin_end(48);
    rule.set_margin_top(24);
    content.pack_start(&rule, false, false, 0);

    let bottom = gtk::Box::new(gtk::Orientation::Vertical, 0);
    bottom.set_vexpand(true);
    content.pack_start(&bottom, true, true, 0);

    let foot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    foot.set_margin_start(48);
    foot.set_margin_end(48);
    foot.set_margin_bottom(32);
    let generation = gtk::Label::new(Some("gen a"));
    generation.set_halign(gtk::Align::Start);
    generation.set_hexpand(true);
    theme::class(&generation, "greet-foot");
    foot.pack_start(&generation, true, true, 0);
    let power = gtk::Label::new(Some("sleep · shut down"));
    theme::class(&power, "greet-foot");
    foot.pack_end(&power, false, false, 0);
    content.pack_start(&foot, false, false, 0);

    let overlay = gtk::Overlay::new();
    overlay.add(&content);
    scrim_add(&backdrop, &overlay);
    window.add(&backdrop);

    let window_for_keys = window.clone();
    let rows_for_keys = rows.clone();
    let mode_for_keys = mode.clone();
    let state_for_keys = state.clone();
    window.connect_key_press_event(move |_, event| {
        let mut state = state_for_keys.borrow_mut();
        match event.keyval() {
            gdk::keys::constants::Return | gdk::keys::constants::KP_Enter => {
                confirm(&state);
                window_for_keys.close();
                gtk::glib::Propagation::Stop
            }
            gdk::keys::constants::g | gdk::keys::constants::G => {
                state.guarded = !state.guarded;
                mode_for_keys.set_text(&mode_text(state.guarded));
                gtk::glib::Propagation::Stop
            }
            gdk::keys::constants::Up => {
                if state.selected > 0 {
                    state.selected -= 1;
                    draw_rows(&rows_for_keys, &state);
                }
                gtk::glib::Propagation::Stop
            }
            gdk::keys::constants::Down => {
                if state.selected + 1 < PRINCIPALS.len() {
                    state.selected += 1;
                    draw_rows(&rows_for_keys, &state);
                }
                gtk::glib::Propagation::Stop
            }
            gdk::keys::constants::s | gdk::keys::constants::S => {
                system::spawn("systemctl suspend");
                gtk::glib::Propagation::Stop
            }
            gdk::keys::constants::Delete => {
                system::spawn("systemctl poweroff");
                gtk::glib::Propagation::Stop
            }
            _ => gtk::glib::Propagation::Proceed,
        }
    });

    window.connect_destroy(|_| gtk::main_quit());
    window.show_all();
    window.present();
    gtk::main();
}

struct State {
    selected: usize,
    guarded: bool,
}

fn mode_text(guarded: bool) -> String {
    if guarded {
        "guarded · g for full access".to_string()
    } else {
        "full access · g for guarded".to_string()
    }
}

fn draw_rows(rows: &gtk::Box, state: &State) {
    for child in rows.children() {
        rows.remove(&child);
    }
    for (index, principal) in PRINCIPALS.iter().enumerate() {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        row.set_halign(gtk::Align::Center);
        row.set_valign(gtk::Align::Center);

        row.pack_start(&icons::logo_mark(34), false, false, 0);

        let name = gtk::Label::new(Some(principal.name));
        theme::class(&name, "greet-name");
        row.pack_start(&name, false, false, 0);

        if index == state.selected {
            let hint = gtk::Label::new(Some("return"));
            theme::class(&hint, "greet-kbd");
            row.pack_start(&hint, false, false, 0);
        }

        rows.pack_start(&row, false, false, 0);
    }
    rows.show_all();
}

fn confirm(state: &State) {
    let principal = &PRINCIPALS[state.selected];
    let mode = if state.guarded { "guarded" } else { "full" };

    // The session contract: who sat down, under what mode.
    let runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    let _ = std::fs::write(format!("{runtime}/tiaki-session"), format!("{} {mode}\n", principal.id));

    // Best effort: sign the sign-in itself into the ledger. The key is
    // user-scoped: a principal can write its own session state, not the
    // system scope.
    let _ = std::process::Command::new("/usr/local/bin/agentd")
        .args([
            "--principal",
            principal.id,
            "apply",
            &format!("user/{}/session/mode", principal.name.to_lowercase()),
            mode,
            "--intent",
            &format!("{} signed in ({mode})", principal.name),
        ])
        .env(
            "TIAKI_HOME",
            std::env::var("TIAKI_HOME").unwrap_or_else(|_| "/var/lib/tiaki".to_string()),
        )
        .status();

    // What Sway used to autostart directly now starts behind the sign-in.
    for (command, class) in [
        ("tiaki-shell bar", "foreground"),
        ("tiaki-shell dock", "foreground"),
        ("tiaki-shell chrome", "foreground"),
        (
            "foot -c /etc/tiaki/foot.ini -e /usr/share/tiaki/terminal-demo.sh",
            "foreground",
        ),
        ("mousepad /home/desktop/demo.rs", "foreground"),
    ] {
        system::spawn_app(command, class);
    }
}

fn wallpaper() -> gtk::Image {
    let (width, height) = screen_size();
    let path = WALLPAPERS
        .iter()
        .find(|path| std::path::Path::new(path).exists());
    let image = match path {
        Some(path) => gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(path, width, height, false)
            .map(|pixbuf| gtk::Image::from_pixbuf(Some(&pixbuf)))
            .unwrap_or_else(|_| gtk::Image::new()),
        None => gtk::Image::new(),
    };
    image.set_hexpand(true);
    image.set_vexpand(true);
    image.set_halign(gtk::Align::Fill);
    image.set_valign(gtk::Align::Fill);
    image
}

fn veil() -> gtk::EventBox {
    let veil = gtk::EventBox::new();
    veil.set_visible_window(true);
    veil.set_hexpand(true);
    veil.set_vexpand(true);
    veil.set_halign(gtk::Align::Fill);
    veil.set_valign(gtk::Align::Fill);
    theme::class(&veil, "greet-veil");
    veil
}

fn scrim_add(backdrop: &gtk::Overlay, overlay: &gtk::Overlay) {
    backdrop.add_overlay(overlay);
}

fn screen_size() -> (i32, i32) {
    gdk::Display::default()
        .and_then(|display| display.primary_monitor())
        .map(|monitor| {
            let rect = monitor.geometry();
            (rect.width(), rect.height())
        })
        .unwrap_or((1440, 900))
}

fn time_text() -> String {
    match glib::DateTime::now_local() {
        Ok(now) => format!("{:02}:{:02}", now.hour(), now.minute()),
        Err(_) => "--:--".to_string(),
    }
}

fn date_text() -> String {
    const WEEKDAYS: [&str; 7] = [
        "sunday",
        "monday",
        "tuesday",
        "wednesday",
        "thursday",
        "friday",
        "saturday",
    ];
    const MONTHS: [&str; 12] = [
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
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
