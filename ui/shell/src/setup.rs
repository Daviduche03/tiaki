//! First-run setup: who owns this machine, how much it trusts agents, and
//! where thinking happens. Five screens, one question each, keyboard-first.
//!
//! Runs inside the greet flow when `~/.config/tiaki/setup-done` is missing.
//! Finishing writes one ledger transaction per answer, then hands over to the
//! sign-in screen. No password: identity here is a choice, not a secret.

use gtk::gdk;
use gtk::prelude::*;
use gtk_layer_shell::{Edge, KeyboardMode, Layer};

use crate::{icons, single, theme};

/// Stylesheets owned by this module, in cascade order.
pub const STYLE_SHEETS: &[&str] = &[include_str!("../css/setup.css")];

const WALLPAPERS: [&str; 2] = [
    "/usr/share/tiaki/wallpapers/photo-03-mist-pines.jpg",
    "/usr/share/tiaki/wallpapers/drawn-01-ink-field.jpg",
];

const STEPS: [&str; 5] = [
    "Welcome",
    "Principal",
    "Default mode",
    "Inference",
    "Review & finish",
];

#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Welcome,
    Principal,
    Mode,
    Inference,
    Review,
    Done,
}

impl Screen {
    fn index(self) -> usize {
        match self {
            Screen::Welcome => 0,
            Screen::Principal => 1,
            Screen::Mode => 2,
            Screen::Inference => 3,
            Screen::Review => 4,
            Screen::Done => 4,
        }
    }
    fn next(self) -> Option<Screen> {
        match self {
            Screen::Welcome => Some(Screen::Principal),
            Screen::Principal => Some(Screen::Mode),
            Screen::Mode => Some(Screen::Inference),
            Screen::Inference => Some(Screen::Review),
            Screen::Review => None,
            Screen::Done => None,
        }
    }
    fn prev(self) -> Option<Screen> {
        match self {
            Screen::Welcome => None,
            Screen::Principal => Some(Screen::Welcome),
            Screen::Mode => Some(Screen::Principal),
            Screen::Inference => Some(Screen::Mode),
            Screen::Review => Some(Screen::Inference),
            Screen::Done => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Inference {
    Local,
    Remote,
    Later,
}

struct Answers {
    name: String,
    guarded: bool,
    inference: Inference,
    remote_url: String,
    recorded: usize,
    total: usize,
}

struct Ui {
    window: gtk::Window,
    body: gtk::Box,
    contents: gtk::Box,
    screen: Screen,
    answers: Answers,
    card_selected: usize,
    name_entry: Option<gtk::Entry>,
    url_entry: Option<gtk::Entry>,
    finish_button: Option<gtk::Button>,
}

pub fn run() {
    let Some(_instance) = single::Instance::acquire("tiaki-setup", "setup", false) else {
        return;
    };

    let window = theme::base_window("setup-window");
    theme::setup_layer(
        &window,
        "tiaki-setup",
        Layer::Overlay,
        KeyboardMode::Exclusive,
        0,
    );
    for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        theme::anchor(&window, edge);
    }

    let backdrop = gtk::Overlay::new();
    backdrop.add(&wallpaper());
    backdrop.add_overlay(&veil());

    let columns = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    columns.set_hexpand(true);
    columns.set_vexpand(true);

    let left = gtk::Box::new(gtk::Orientation::Vertical, 0);
    left.set_hexpand(true);
    left.set_margin_start(96);
    left.set_margin_end(64);
    left.set_margin_top(48);
    left.set_margin_bottom(40);

    let brand = gtk::Box::new(gtk::Orientation::Horizontal, 11);
    brand.pack_start(&icons::logo_mark(20), false, false, 0);
    let word = gtk::Label::new(Some("tiaki"));
    theme::class(&word, "su-brand");
    brand.pack_start(&word, false, false, 0);
    left.pack_start(&brand, false, false, 0);

    let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
    body.set_vexpand(true);
    left.pack_start(&body, true, true, 0);

    let foot = gtk::Label::new(None);
    foot.set_halign(gtk::Align::Start);
    theme::class(&foot, "su-foot");
    left.pack_start(&foot, false, false, 0);

    let right = gtk::EventBox::new();
    right.set_visible_window(true);
    right.set_size_request(400, -1);
    theme::class(&right, "su-contents");
    let contents = gtk::Box::new(gtk::Orientation::Vertical, 0);
    contents.set_margin_top(48);
    contents.set_margin_start(40);
    contents.set_margin_end(40);
    right.add(&contents);

    columns.pack_start(&left, true, true, 0);
    columns.pack_start(&right, false, false, 0);

    let shell = gtk::Overlay::new();
    shell.add(&columns);
    backdrop.add_overlay(&shell);
    window.add(&backdrop);

    let ui = std::rc::Rc::new(std::cell::RefCell::new(Ui {
        window: window.clone(),
        body,
        contents,
        screen: Screen::Welcome,
        answers: Answers {
            name: "Ada".to_string(),
            guarded: true,
            inference: Inference::Local,
            remote_url: String::new(),
            recorded: 0,
            total: 0,
        },
        card_selected: 0,
        name_entry: None,
        url_entry: None,
        finish_button: None,
    }));
    draw(&ui);

    let ui_for_keys = ui.clone();
    window.connect_key_press_event(move |_, event| {
        on_key(&ui_for_keys, event.keyval());
        gtk::glib::Propagation::Stop
    });

    window.connect_destroy(|_| gtk::main_quit());
    window.show_all();
    window.present();
    gtk::main();
}

fn on_key(ui: &std::rc::Rc<std::cell::RefCell<Ui>>, key: gdk::keys::Key) {
    use gdk::keys::constants::*;
    let mut borrowed = ui.borrow_mut();
    let screen = borrowed.screen;
    if key == Return || key == KP_Enter {
        if screen == Screen::Done {
            borrowed.window.close();
            return;
        }
        drop(borrowed);
        primary(ui);
        return;
    }
    if key == Escape {
        if let Some(prev) = screen.prev() {
            borrowed.screen = prev;
            borrowed.card_selected = 0;
            drop(borrowed);
            draw(ui);
        }
        return;
    }
    if screen == Screen::Mode || screen == Screen::Inference {
        let count = if screen == Screen::Mode { 2 } else { 3 };
        if key == Up {
            if borrowed.card_selected > 0 {
                borrowed.card_selected -= 1;
                drop(borrowed);
                draw(ui);
            }
            return;
        }
        if key == Down {
            if borrowed.card_selected + 1 < count {
                borrowed.card_selected += 1;
                drop(borrowed);
                draw(ui);
            }
            return;
        }
        let digit = match key {
            pressed if pressed == gdk::keys::Key::from_name("1") => Some(0),
            pressed if pressed == gdk::keys::Key::from_name("2") => Some(1),
            pressed if pressed == gdk::keys::Key::from_name("3") => Some(2),
            _ => None,
        };
        if let Some(index) = digit {
            if index < count {
                borrowed.card_selected = index;
                drop(borrowed);
                draw(ui);
            }
        }
    }
}

fn primary(ui: &std::rc::Rc<std::cell::RefCell<Ui>>) {
    let screen = ui.borrow().screen;
    match screen {
        Screen::Review => finish(ui),
        Screen::Done => ui.borrow().window.close(),
        _ => {
            if screen == Screen::Principal {
                collect_entries(ui);
            }
            if screen == Screen::Mode {
                let mut borrowed = ui.borrow_mut();
                borrowed.answers.guarded = borrowed.card_selected == 0;
            }
            if screen == Screen::Inference {
                collect_entries(ui);
                let mut borrowed = ui.borrow_mut();
                borrowed.answers.inference = match borrowed.card_selected {
                    1 => Inference::Remote,
                    2 => Inference::Later,
                    _ => Inference::Local,
                };
            }
            if let Some(next) = screen.next() {
                let mut borrowed = ui.borrow_mut();
                borrowed.screen = next;
                borrowed.card_selected = 0;
                drop(borrowed);
                draw(ui);
            }
        }
    }
}

fn collect_entries(ui: &std::rc::Rc<std::cell::RefCell<Ui>>) {
    let mut borrowed = ui.borrow_mut();
    if let Some(entry) = borrowed.name_entry.as_ref() {
        let text = entry.text().to_string().trim().to_string();
        if !text.is_empty() {
            borrowed.answers.name = text;
        }
    }
    if let Some(entry) = borrowed.url_entry.as_ref() {
        borrowed.answers.remote_url = entry.text().to_string().trim().to_string();
    }
}

fn finish(ui: &std::rc::Rc<std::cell::RefCell<Ui>>) {
    collect_entries(ui);
    let answers = {
        let borrowed = ui.borrow();
        (
            borrowed.answers.name.clone(),
            borrowed.answers.guarded,
            borrowed.answers.inference,
            borrowed.answers.remote_url.clone(),
        )
    };
    let (name, guarded, inference, remote_url) = answers;
    let user = name.to_lowercase();
    let mode = if guarded { "guarded" } else { "full" };
    let (backend, backend_detail) = match inference {
        Inference::Local => ("local".to_string(), "broker starts with the session".to_string()),
        Inference::Remote => (
            if remote_url.is_empty() {
                "remote".to_string()
            } else {
                format!("remote:{remote_url}")
            },
            "every call is logged to the ledger".to_string(),
        ),
        Inference::Later => ("later".to_string(), "agents will ask when they need one".to_string()),
    };

    let mut recorded = 0;
    let writes = [
        (format!("user/{user}/profile/name"), name.clone(), format!("Create principal {name}")),
        (
            format!("user/{user}/session/mode"),
            mode.to_string(),
            format!("Default mode set to {mode}"),
        ),
        (
            format!("user/{user}/inference/backend"),
            backend.clone(),
            format!("Inference backend: {backend}"),
        ),
        (
            format!("user/{user}/setup/generation"),
            "a".to_string(),
            "Keep generation A as last-good".to_string(),
        ),
    ];
    let _ = backend_detail;
    let total = writes.len();
    for (key, value, intent) in &writes {
        if apply(key, value, intent) {
            recorded += 1;
        }
    }

    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/desktop".to_string());
    let dir = format!("{home}/.config/tiaki");
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(format!("{dir}/setup-done"), "done\n");

    {
        let mut borrowed = ui.borrow_mut();
        borrowed.answers.recorded = recorded;
        borrowed.answers.total = total;
        borrowed.screen = Screen::Done;
        drop(borrowed);
        draw(ui);
    }
}

/// One ledger transaction. Returns true when the control plane commits it.
fn apply(key: &str, value: &str, intent: &str) -> bool {
    let principal = "user:ada";
    let output = std::process::Command::new("/usr/local/bin/agentd")
        .args(["--principal", principal, "apply", key, value, "--intent", intent])
        .env(
            "TIAKI_HOME",
            std::env::var("TIAKI_HOME").unwrap_or_else(|_| "/var/lib/tiaki".to_string()),
        )
        .output();
    match output {
        Ok(output) => {
            let text = String::from_utf8_lossy(&output.stdout);
            output.status.success() && text.contains("committed")
        }
        Err(_) => false,
    }
}

fn draw(ui: &std::rc::Rc<std::cell::RefCell<Ui>>) {
    let extracted = {
        let borrowed = ui.borrow();
        (
            borrowed.body.clone(),
            borrowed.contents.clone(),
            borrowed.screen,
            borrowed.answers.name.clone(),
            borrowed.answers.guarded,
            borrowed.answers.inference,
            borrowed.answers.remote_url.clone(),
            borrowed.answers.recorded,
            borrowed.answers.total,
            borrowed.card_selected,
        )
    };
    let (body, contents, screen, name, guarded, inference, remote_url, recorded, total, selected) =
        extracted;

    for child in body.children() {
        body.remove(&child);
    }
    for child in contents.children() {
        contents.remove(&child);
    }

    let mut ui_mut = ui.borrow_mut();
    ui_mut.name_entry = None;
    ui_mut.url_entry = None;
    ui_mut.finish_button = None;
    drop(ui_mut);

    match screen {
        Screen::Welcome => {
            title(&body, "A computer that\nremembers what\nit changed.");
            lede(&body, "Every app, agent, and setting is a transaction in a ledger you can read, undo, and hand to someone else.");
            primary_button(ui, &body, "Begin");
            footer(ui, "setup writes 4 transactions · all reversible");
        }
        Screen::Principal => {
            title(&body, "Who owns\nthis machine?");
            lede(&body, "This principal signs every transaction. Others can be added later, each with their own authority.");
            let entry = field(&body, "Name", &name);
            ui.borrow_mut().name_entry = Some(entry);
            primary_button(ui, &body, "Continue");
            footer(ui, "the name becomes user:<name> · owner");
        }
        Screen::Mode => {
            title(&body, "How much should\nthe machine trust agents?");
            card(
                ui,
                &body,
                0,
                selected == 0,
                "Guarded",
                Some("recommended"),
                "Agents run sandboxed. Privileged changes wait for your approval.",
            );
            card(
                ui,
                &body,
                1,
                selected == 1,
                "Full access",
                None,
                "No prompts inside this session. Every action is still recorded.",
            );
            let _ = guarded;
            primary_button(ui, &body, "Continue");
            footer(ui, "switchable per session, never silently");
        }
        Screen::Inference => {
            title(&body, "Where should\nthinking happen?");
            card(
                ui,
                &body,
                0,
                selected == 0,
                "On this machine",
                None,
                "Local model, nothing leaves the device.",
            );
            card(
                ui,
                &body,
                1,
                selected == 1,
                "Remote endpoint",
                None,
                "Point at a provider. Every call is logged.",
            );
            card(
                ui,
                &body,
                2,
                selected == 2,
                "Not now",
                None,
                "Agents will ask when they need a backend.",
            );
            let _ = inference;
            let _ = remote_url;
            if selected == 1 {
                let entry = field(&body, "Endpoint URL", "");
                ui.borrow_mut().url_entry = Some(entry);
            }
            primary_button(ui, &body, "Continue");
            footer(ui, "inference is an OS service, not an app setting");
        }
        Screen::Review => {
            title(&body, "Four changes,\nall reversible.");
            lede(&body, "This is the whole of setup, written the way the system will write it.");
            let mode_word = if guarded { "Guarded" } else { "Full access" };
            let backend_word = match inference {
                Inference::Local => "local",
                Inference::Remote => "remote",
                Inference::Later => "later",
            };
            change(&body, &format!("Create principal {name}"), "owner of this machine");
            change(&body, &format!("Default mode set to {mode_word}"), "switchable per session");
            change(&body, &format!("Inference backend: {backend_word}"), "broker starts with the session");
            change(&body, "Keep generation A as last-good", "B stays the rollback target");
            let button = primary_button(ui, &body, "Finish setup");
            ui.borrow_mut().finish_button = Some(button);
            footer(ui, "4 transactions · 4 rollback points");
        }
        Screen::Done => {
            title(&body, "Tiaki is ready.");
            lede(
                &body,
                &format!("{recorded} of {total} transactions recorded. Nothing left this machine."),
            );
            primary_button(ui, &body, "Sign in");
            footer(ui, "return to sign in");
        }
    }

    draw_contents(ui, screen);
    ui.borrow().window.show_all();
}

fn draw_contents(ui: &std::rc::Rc<std::cell::RefCell<Ui>>, screen: Screen) {
    let contents = ui.borrow().contents.clone();
    let head = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let title = gtk::Label::new(Some("Contents"));
    title.set_halign(gtk::Align::Start);
    title.set_hexpand(true);
    theme::class(&title, "su-toc-title");
    head.pack_start(&title, true, true, 0);
    let count = gtk::Label::new(Some("5 steps"));
    theme::class(&count, "su-toc-count");
    head.pack_end(&count, false, false, 0);
    contents.pack_start(&head, false, false, 0);

    let rule = gtk::Separator::new(gtk::Orientation::Horizontal);
    theme::class(&rule, "su-rule");
    contents.pack_start(&rule, false, false, 0);

    let here = screen.index();
    for (index, label) in STEPS.iter().enumerate() {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row.set_margin_top(14);
        row.set_margin_bottom(14);
        row.set_margin_start(2);
        let number = gtk::Label::new(Some(&format!("{:02}", index + 1)));
        theme::class(&number, "su-toc-num");
        row.pack_start(&number, false, false, 0);
        let name = gtk::Label::new(Some(label));
        name.set_halign(gtk::Align::Start);
        name.set_hexpand(true);
        theme::class(
            &name,
            if index == here {
                "su-toc-now"
            } else {
                "su-toc-item"
            },
        );
        if index < here || screen == Screen::Done {
            theme::class(&name, "su-toc-done");
        }
        row.pack_start(&name, true, true, 0);
        let state = gtk::Label::new(Some(if index < here || screen == Screen::Done {
            "done"
        } else if index == here {
            "here"
        } else {
            "—"
        }));
        theme::class(&state, "su-toc-state");
        row.pack_end(&state, false, false, 0);
        contents.pack_start(&row, false, false, 0);

        let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
        theme::class(&separator, "su-rule");
        contents.pack_start(&separator, false, false, 0);
    }
}

fn title(body: &gtk::Box, text: &str) {
    let label = gtk::Label::new(Some(text));
    label.set_halign(gtk::Align::Start);
    label.set_valign(gtk::Align::Start);
    label.set_line_wrap(true);
    label.set_margin_top(64);
    theme::class(&label, "su-title");
    body.pack_start(&label, false, false, 0);
}

fn lede(body: &gtk::Box, text: &str) {
    let label = gtk::Label::new(Some(text));
    label.set_halign(gtk::Align::Start);
    label.set_line_wrap(true);
    label.set_max_width_chars(46);
    label.set_margin_top(22);
    theme::class(&label, "su-lede");
    body.pack_start(&label, false, false, 0);
}

fn field(body: &gtk::Box, label_text: &str, value: &str) -> gtk::Entry {
    let label = gtk::Label::new(Some(label_text));
    label.set_halign(gtk::Align::Start);
    label.set_margin_top(30);
    theme::class(&label, "su-label");
    body.pack_start(&label, false, false, 0);

    let entry = gtk::Entry::new();
    entry.set_text(value);
    entry.set_hexpand(false);
    entry.set_size_request(420, 44);
    entry.set_halign(gtk::Align::Start);
    entry.set_margin_top(6);
    theme::class(&entry, "su-field");
    theme::class(&entry, "su-entry");
    body.pack_start(&entry, false, false, 0);
    entry
}

fn card(
    ui: &std::rc::Rc<std::cell::RefCell<Ui>>,
    body: &gtk::Box,
    index: usize,
    selected: bool,
    title_text: &str,
    tag: Option<&str>,
    detail: &str,
) {
    let card = gtk::EventBox::new();
    card.set_visible_window(true);
    card.set_size_request(560, -1);
    card.set_halign(gtk::Align::Start);
    card.set_margin_top(if index == 0 { 30 } else { 12 });
    theme::class(&card, "su-card");
    if selected {
        theme::class(&card, "on");
    }

    let inner = gtk::Box::new(gtk::Orientation::Vertical, 6);
    inner.set_margin_top(16);
    inner.set_margin_bottom(16);
    inner.set_margin_start(18);
    inner.set_margin_end(18);

    let head = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let title = gtk::Label::new(Some(title_text));
    title.set_halign(gtk::Align::Start);
    theme::class(&title, "su-card-title");
    head.pack_start(&title, false, false, 0);
    if let Some(tag) = tag {
        let pill = gtk::Label::new(Some(tag));
        theme::class(&pill, "su-card-tag");
        head.pack_start(&pill, false, false, 0);
    }
    inner.pack_start(&head, false, false, 0);

    let body_label = gtk::Label::new(Some(detail));
    body_label.set_halign(gtk::Align::Start);
    body_label.set_line_wrap(true);
    body_label.set_max_width_chars(52);
    theme::class(&body_label, "su-card-body");
    inner.pack_start(&body_label, false, false, 0);

    card.add(&inner);
    let ui_for_click = ui.clone();
    card.connect_button_press_event(move |_, _| {
        {
            let mut borrowed = ui_for_click.borrow_mut();
            borrowed.card_selected = index;
        }
        draw(&ui_for_click);
        gtk::glib::Propagation::Stop
    });
    body.pack_start(&card, false, false, 0);
}

fn change(body: &gtk::Box, what: &str, sub: &str) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    row.set_margin_top(14);
    row.set_margin_bottom(14);

    let marker = gtk::EventBox::new();
    marker.set_visible_window(true);
    marker.set_size_request(6, 6);
    marker.set_valign(gtk::Align::Start);
    marker.set_margin_top(6);
    theme::class(&marker, "su-marker");
    row.pack_start(&marker, false, false, 0);

    let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
    let title = gtk::Label::new(Some(what));
    title.set_halign(gtk::Align::Start);
    theme::class(&title, "su-change");
    text.pack_start(&title, false, false, 0);
    let detail = gtk::Label::new(Some(sub));
    detail.set_halign(gtk::Align::Start);
    theme::class(&detail, "su-change-sub");
    text.pack_start(&detail, false, false, 0);
    row.pack_start(&text, true, true, 0);

    body.pack_start(&row, false, false, 0);

    let rule = gtk::Separator::new(gtk::Orientation::Horizontal);
    theme::class(&rule, "su-rule");
    body.pack_start(&rule, false, false, 0);
}

fn primary_button(
    ui: &std::rc::Rc<std::cell::RefCell<Ui>>,
    body: &gtk::Box,
    label: &str,
) -> gtk::Button {
    let button = gtk::Button::with_label(label);
    button.set_halign(gtk::Align::Start);
    button.set_size_request(0, 44);
    button.set_margin_top(30);
    theme::class(&button, "su-stamp");
    let ui_for_click = ui.clone();
    button.connect_clicked(move |_| primary(&ui_for_click));
    body.pack_start(&button, false, false, 0);
    button
}

fn footer(ui: &std::rc::Rc<std::cell::RefCell<Ui>>, text: &str) {
    let label = gtk::Label::new(Some(text));
    label.set_halign(gtk::Align::Start);
    label.set_margin_top(26);
    theme::class(&label, "su-foot");
    ui.borrow().body.pack_start(&label, false, false, 0);
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

fn screen_size() -> (i32, i32) {
    gdk::Display::default()
        .and_then(|display| display.primary_monitor())
        .map(|monitor| {
            let rect = monitor.geometry();
            (rect.width(), rect.height())
        })
        .unwrap_or((1440, 900))
}