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
    actions: gtk::Box,
    step: gtk::Label,
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

    // One sheet, centred over the wallpaper. A window-sized surface beats a
    // full-bleed page: the eye gets a single edge to follow and the wallpaper
    // stays a background instead of a competing panel.
    // A Box, not an EventBox: an EventBox hands its child its natural width,
    // which left the sheet under-allocated and every control short of its edge.
    let sheet = gtk::Box::new(gtk::Orientation::Vertical, 0);
    sheet.set_size_request(1040, 560);
    sheet.set_halign(gtk::Align::Center);
    sheet.set_valign(gtk::Align::Center);
    theme::class(&sheet, "su-sheet");
    sheet.connect_draw(|widget, cr| {
        let width = widget.allocated_width() as f64;
        let height = widget.allocated_height() as f64;
        cr.set_operator(gtk::cairo::Operator::Source);
        rounded_path(cr, 0.0, 0.0, width, height, 10.0);
        cr.set_source_rgba(0.082, 0.078, 0.075, 0.97);
        let _ = cr.fill();
        cr.set_operator(gtk::cairo::Operator::Over);
        cr.set_line_width(1.0);
        cr.set_source_rgba(0.902, 0.878, 0.831, 0.12);
        rounded_path(cr, 0.5, 0.5, width - 1.0, height - 1.0, 10.0);
        let _ = cr.stroke();
        gtk::glib::Propagation::Proceed
    });

    let columns = gtk::Grid::new();
    columns.set_hexpand(true);
    columns.set_vexpand(true);
    columns.set_margin_top(1);
    columns.set_margin_bottom(1);
    columns.set_margin_start(1);
    columns.set_margin_end(1);

    // Left: the question, its explanation, its controls, then the actions.
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.set_hexpand(true);
    content.set_valign(gtk::Align::Center);
    content.set_margin_start(56);
    content.set_margin_end(36);
    content.set_margin_top(44);
    content.set_margin_bottom(36);

    let head = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let brand = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    brand.pack_start(&icons::logo_mark(18), false, false, 0);
    let word = gtk::Label::new(Some("tiaki"));
    theme::class(&word, "su-brand");
    brand.pack_start(&word, false, false, 0);
    head.pack_start(&brand, false, false, 0);

    // Where you are belongs at the top, beside the name, read before the
    // question.
    let step = gtk::Label::new(None);
    step.set_halign(gtk::Align::Start);
    step.set_valign(gtk::Align::Center);
    step.set_margin_start(12);
    theme::class(&step, "su-step");
    head.pack_start(&step, false, false, 0);
    content.pack_start(&head, false, false, 0);

    let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
    body.set_hexpand(true);
    content.pack_start(&body, false, false, 0);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    actions.set_hexpand(true);
    actions.set_margin_top(36);
    content.pack_start(&actions, true, true, 0);

    // Right: the map. Inset, so the sheet keeps one silhouette.
    let rail = gtk::EventBox::new();
    rail.set_visible_window(true);
    rail.set_size_request(252, -1);
    rail.set_margin_top(24);
    rail.set_margin_bottom(24);
    rail.set_margin_end(24);
    theme::class(&rail, "su-rail");
    let contents = gtk::Box::new(gtk::Orientation::Vertical, 0);
    contents.set_margin_top(24);
    contents.set_margin_start(22);
    contents.set_margin_end(22);
    contents.set_margin_bottom(20);
    rail.add(&contents);

    columns.attach(&content, 0, 0, 1, 1);
    columns.attach(&rail, 1, 0, 1, 1);
    content.set_hexpand(true);
    rail.set_hexpand(false);
    sheet.pack_start(&columns, true, true, 0);

    let shell = gtk::Overlay::new();
    shell.add(&sheet);
    // Pin the layout to the screen so the sheet does not drift when a step's
    // text changes length.
    let (screen_width, screen_height) = screen_size();
    shell.set_size_request(screen_width, screen_height);
    shell.set_halign(gtk::Align::Fill);
    shell.set_valign(gtk::Align::Fill);
    backdrop.add_overlay(&shell);
    window.add(&backdrop);

    let ui = std::rc::Rc::new(std::cell::RefCell::new(Ui {
        window: window.clone(),
        body,
        contents,
        actions,
        step,
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
        Inference::Local => (
            "local".to_string(),
            "broker starts with the session".to_string(),
        ),
        Inference::Remote => (
            if remote_url.is_empty() {
                "remote".to_string()
            } else {
                format!("remote:{remote_url}")
            },
            "every call is logged to the ledger".to_string(),
        ),
        Inference::Later => (
            "later".to_string(),
            "agents will ask when they need one".to_string(),
        ),
    };

    let mut recorded = 0;
    let writes = [
        (
            format!("user/{user}/profile/name"),
            name.clone(),
            format!("Create principal {name}"),
        ),
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
        .args([
            "--principal",
            principal,
            "apply",
            key,
            value,
            "--intent",
            intent,
        ])
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
    let actions = ui.borrow().actions.clone();
    for child in actions.children() {
        actions.remove(&child);
    }
    ui.borrow()
        .step
        .set_text(&format!("Step {} of {}", screen.index() + 1, STEPS.len()));

    let mut ui_mut = ui.borrow_mut();
    ui_mut.name_entry = None;
    ui_mut.url_entry = None;
    ui_mut.finish_button = None;
    drop(ui_mut);

    match screen {
        Screen::Welcome => {
            title(&body, "A computer that\nremembers what\nit changed.");
            lede(&body, "Every app, agent, and setting is a transaction in a ledger you can read, undo, and hand to someone else.");
            action_row(ui, "Begin", "4 transactions · all reversible");
        }
        Screen::Principal => {
            title(&body, "Who owns\nthis machine?");
            lede(&body, "This principal signs every transaction. Others can be added later, each with their own authority.");
            let entry = field(&body, "Name", &name);
            // A focused GtkEntry consumes Return, so Enter inside the field
            // would otherwise do nothing; every field submits the step.
            let ui_for_enter = ui.clone();
            entry.connect_activate(move |_| primary(&ui_for_enter));
            ui.borrow_mut().name_entry = Some(entry);
            action_row(ui, "Continue", "the name becomes user:<name> · owner");
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
            action_row(ui, "Continue", "switchable per session, never silently");
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
                let ui_for_enter = ui.clone();
                entry.connect_activate(move |_| primary(&ui_for_enter));
                ui.borrow_mut().url_entry = Some(entry);
            }
            action_row(
                ui,
                "Continue",
                "inference is an OS service, not an app setting",
            );
        }
        Screen::Review => {
            title(&body, "Four changes,\nall reversible.");
            lede(
                &body,
                "This is the whole of setup, written the way the system will write it.",
            );
            let mode_word = if guarded { "Guarded" } else { "Full access" };
            let backend_word = match inference {
                Inference::Local => "local",
                Inference::Remote => "remote",
                Inference::Later => "later",
            };
            change(
                &body,
                &format!("Create principal {name}"),
                "owner of this machine",
            );
            change(
                &body,
                &format!("Default mode set to {mode_word}"),
                "switchable per session",
            );
            change(
                &body,
                &format!("Inference backend: {backend_word}"),
                "broker starts with the session",
            );
            change(
                &body,
                "Keep generation A as last-good",
                "B stays the rollback target",
            );
            if let Some(trailing) = body.children().last().cloned() {
                body.remove(&trailing);
            }
            let button = action_row(ui, "Finish setup", "4 transactions · 4 rollback points");
            ui.borrow_mut().finish_button = Some(button);
        }
        Screen::Done => {
            title(&body, "Tiaki is ready.");
            lede(
                &body,
                &format!("{recorded} of {total} transactions recorded. Nothing left this machine."),
            );
            action_row(ui, "Sign in", "return to sign in");
        }
    }

    draw_contents(ui, screen);
    ui.borrow().window.show_all();
}

fn draw_contents(ui: &std::rc::Rc<std::cell::RefCell<Ui>>, screen: Screen) {
    let contents = ui.borrow().contents.clone();
    let here = screen.index();
    let done = screen == Screen::Done;

    let head = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let title = gtk::Label::new(Some("SETUP"));
    title.set_halign(gtk::Align::Start);
    title.set_hexpand(true);
    theme::class(&title, "su-toc-title");
    head.pack_start(&title, true, true, 0);
    let count = gtk::Label::new(Some(&format!("{} / {}", here + 1, STEPS.len())));
    theme::class(&count, "su-toc-count");
    head.pack_end(&count, false, false, 0);
    contents.pack_start(&head, false, false, 0);

    // A dot per step, one column of names. The dot carries state; the label
    // carries the name. No rows, no rules, no numbers to read twice.
    let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    list.set_margin_top(20);
    contents.pack_start(&list, false, false, 0);

    for (index, label) in STEPS.iter().enumerate() {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row.set_margin_top(9);
        row.set_margin_bottom(9);

        let dot = gtk::EventBox::new();
        dot.set_visible_window(true);
        dot.set_size_request(6, 6);
        dot.set_valign(gtk::Align::Center);
        theme::class(
            &dot,
            if index == here && !done {
                "su-dot-now"
            } else if index < here || done {
                "su-dot-done"
            } else {
                "su-dot"
            },
        );
        row.pack_start(&dot, false, false, 0);

        let name = gtk::Label::new(Some(label));
        name.set_halign(gtk::Align::Start);
        theme::class(
            &name,
            if index == here && !done {
                "su-toc-now"
            } else if index < here || done {
                "su-toc-done"
            } else {
                "su-toc-item"
            },
        );
        row.pack_start(&name, false, false, 0);
        list.pack_start(&row, false, false, 0);
    }

    let note = gtk::Label::new(Some("Enter continue\nEsc back"));
    note.set_halign(gtk::Align::Start);
    note.set_valign(gtk::Align::End);
    note.set_vexpand(true);
    theme::class(&note, "su-toc-note");
    contents.pack_start(&note, true, true, 0);
}

fn title(body: &gtk::Box, text: &str) {
    let label = gtk::Label::new(Some(text));
    label.set_halign(gtk::Align::Start);
    label.set_valign(gtk::Align::Start);
    label.set_line_wrap(true);
    label.set_margin_top(40);
    theme::class(&label, "su-title");
    body.pack_start(&label, false, false, 0);
}

fn lede(body: &gtk::Box, text: &str) {
    let label = gtk::Label::new(Some(text));
    label.set_halign(gtk::Align::Start);
    label.set_line_wrap(true);
    label.set_max_width_chars(44);
    label.set_margin_top(14);
    theme::class(&label, "su-lede");
    body.pack_start(&label, false, false, 0);
}

fn field(body: &gtk::Box, label_text: &str, value: &str) -> gtk::Entry {
    let label = gtk::Label::new(Some(label_text));
    label.set_halign(gtk::Align::Start);
    label.set_margin_top(22);
    theme::class(&label, "su-label");
    body.pack_start(&label, false, false, 0);

    let entry = gtk::Entry::new();
    entry.set_text(value);
    entry.set_hexpand(true);
    entry.set_size_request(0, 40);
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
    card.set_halign(gtk::Align::Fill);
    card.set_margin_top(if index == 0 { 24 } else { 10 });
    theme::class(&card, "su-card");
    if selected {
        theme::class(&card, "on");
    }

    let inner = gtk::Box::new(gtk::Orientation::Vertical, 4);
    inner.set_margin_top(14);
    inner.set_margin_bottom(14);
    inner.set_margin_start(16);
    inner.set_margin_end(16);

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

fn action_row(
    ui: &std::rc::Rc<std::cell::RefCell<Ui>>,
    primary_label: &str,
    hint: &str,
) -> gtk::Button {
    let actions = ui.borrow().actions.clone();
    let has_back = ui.borrow().screen.prev().is_some();

    if has_back {
        let back = gtk::Button::with_label("Back");
        back.set_size_request(0, 36);
        back.set_valign(gtk::Align::Center);
        theme::class(&back, "su-back");
        let ui_for_click = ui.clone();
        back.connect_clicked(move |_| {
            let previous = ui_for_click.borrow().screen.prev();
            if let Some(previous) = previous {
                {
                    let mut borrowed = ui_for_click.borrow_mut();
                    borrowed.screen = previous;
                    borrowed.card_selected = 0;
                }
                draw(&ui_for_click);
            }
        });
        actions.pack_start(&back, false, false, 0);
    }

    let button = gtk::Button::with_label(primary_label);
    button.set_size_request(0, 36);
    button.set_valign(gtk::Align::Center);
    theme::class(&button, "su-primary");
    let ui_for_click = ui.clone();
    button.connect_clicked(move |_| primary(&ui_for_click));
    actions.pack_end(&button, false, false, 0);

    let hint_label = gtk::Label::new(Some(hint));
    hint_label.set_halign(gtk::Align::End);
    hint_label.set_valign(gtk::Align::Center);
    hint_label.set_margin_end(14);
    theme::class(&hint_label, "su-hint");
    actions.pack_end(&hint_label, false, false, 0);
    button
}

/// Rounded-rectangle path for the sheet's own surface.
fn rounded_path(cr: &gtk::cairo::Context, x: f64, y: f64, width: f64, height: f64, radius: f64) {
    use std::f64::consts::{FRAC_PI_2, PI};
    cr.new_sub_path();
    cr.arc(x + width - radius, y + radius, radius, -FRAC_PI_2, 0.0);
    cr.arc(
        x + width - radius,
        y + height - radius,
        radius,
        0.0,
        FRAC_PI_2,
    );
    cr.arc(x + radius, y + height - radius, radius, FRAC_PI_2, PI);
    cr.arc(x + radius, y + radius, radius, PI, 3.0 * FRAC_PI_2);
    cr.close_path();
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
