//! Native Tiaki desktop client.
//!
//! This is a real GTK3/Wayland application: real surfaces, real input, no
//! browser. It consumes the same `ui/tree` as the control plane and renders
//! it with native widgets. The compositor is Sway.

use std::cell::RefCell;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::rc::Rc;
use std::time::Duration;

use agentic_json::Json;
use agentic_ui_tree::{Binding, Keymap, Node, UiTree};
use gtk::prelude::*;

const DEFAULT_URL: &str = "http://127.0.0.1:8787";

const KEY_ESCAPE: u32 = 0xff1b;
const KEY_SPACE: u32 = 0x20;
const KEY_SLASH: u32 = 0x2f;
const KEY_QUESTION: u32 = 0x3f;
const KEY_T: u32 = 0x74;
const KEY_K: u32 = 0x6b;
const KEY_LEFT: u32 = 0xff51;
const KEY_RIGHT: u32 = 0xff53;

const CSS: &str = r#"
window.dark {
  background-image: linear-gradient(150deg, #0e1624 0%, #08090b 55%, #0b1018 100%);
}
window.light {
  background-image: linear-gradient(150deg, #eef2f8 0%, #f7f8fa 100%);
}
.panel {
  background-color: rgba(18, 22, 28, 0.86);
  border: 1px solid rgba(62, 75, 96, 0.55);
  border-radius: 16px;
  padding: 12px;
}
window.light .panel {
  background-color: rgba(255, 255, 255, 0.92);
  border-color: #e1e5ec;
}
.workspace-title { color: #7f8794; font-size: 12px; font-weight: 700; }
.panel-title { color: #7f8794; font-size: 11px; font-weight: 700; }
.brand { color: #e6e8eb; font-size: 14px; font-weight: 700; }
window.light .brand { color: #171a1f; }
.text { color: #aab2bf; font-size: 14px; }
.text-title { color: #eef1f5; font-size: 22px; font-weight: 600; }
.text-small { color: #8f98a7; font-size: 12px; }
window.light .text { color: #4a5260; }
window.light .text-title { color: #141821; }
window.light .text-small { color: #6a7280; }
.clock { color: #e6e8eb; font-size: 16px; font-weight: 600; }
window.light .clock { color: #171a1f; }
.chip {
  background-color: #1a1f27;
  border: 1px solid #2a303b;
  border-radius: 999px;
  padding: 2px 9px;
}
.chip label { color: #aab2bf; font-size: 11px; }
window.light .chip { background-color: #f1f3f6; border-color: #e2e5ea; }
window.light .chip label { color: #4a5260; }
.card {
  background-image: none;
  background-color: #1b2230;
  border: 1px solid #2c3444;
  border-radius: 14px;
  padding: 16px;
  color: #e6e8eb;
}
.card:hover { background-color: #232b3b; }
window.light .card { background-color: #ffffff; border-color: #dfe3ea; color: #171a1f; }
.card-title { font-size: 15px; font-weight: 700; }
.card-body { color: #9aa4b4; font-size: 12px; }
window.light .card-body { color: #5a6270; }
.button {
  background-image: none;
  background-color: #1c2230;
  border: 1px solid #2c3444;
  border-radius: 9px;
  padding: 8px 14px;
  color: #e6e8eb;
  font-size: 13px;
}
.button:hover { background-color: #232b3b; }
window.light .button {
  background-color: #eef1f6;
  border-color: #dfe3ea;
  color: #1a1e26;
}
.palette {
  background-color: #12161c;
  border: 1px solid #2a303b;
  border-radius: 16px;
  padding: 10px;
}
window.light .palette { background-color: #ffffff; border-color: #e2e5ea; }
.palette { color: #e6e8eb; }
.palette scrolledwindow,
.palette viewport,
.palette list {
  background-color: transparent;
}
.palette list row {
  background-color: transparent;
  color: #e6e8eb;
}
.palette list row:hover,
.palette list row:selected {
  background-color: #1b2230;
}
.palette list row label { color: #e6e8eb; }
window.light .palette { color: #171a1f; }
window.light .palette scrolledwindow,
window.light .palette viewport,
window.light .palette list {
  background-color: transparent;
}
window.light .palette list row { color: #171a1f; }
window.light .palette list row:hover,
window.light .palette list row:selected { background-color: #f0f3f8; }
window.light .palette list row label { color: #171a1f; }
window.light .kbd { color: #6a7280; }
.palette-entry {
  background-image: none;
  background-color: #0f1319;
  border: 1px solid #2a303b;
  border-radius: 10px;
  padding: 8px 10px;
  color: #e6e8eb;
  font-size: 15px;
}
window.light .palette-entry { background-color: #f6f7f9; border-color: #e2e5ea; color: #171a1f; }
.palette-row { padding: 4px 6px; border-radius: 8px; }
.palette-row:selected { background-color: #1b2230; }
window.light .palette-row:selected { background-color: #f0f3f8; }
.kbd { color: #9aa3b2; font-size: 11px; }
"#;

struct State {
    revision: u64,
    theme: String,
    tree: UiTree,
    keymap: Vec<Binding>,
}

struct App {
    window: gtk::Window,
    root: gtk::Box,
    palette: gtk::EventBox,
    palette_entry: gtk::Entry,
    palette_list: gtk::ListBox,
    server: String,
    revision: u64,
    keymap: Vec<Binding>,
    focusables: Vec<gtk::Widget>,
    focus_index: usize,
    clocks: Vec<(gtk::Label, String, bool)>,
}

fn main() {
    if gtk::init().is_err() {
        eprintln!("could not initialize GTK");
        return;
    }
    install_css();

    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("Tiaki");
    window.set_decorated(false);
    window.set_default_size(1440, 900);
    window.fullscreen();
    window.connect_destroy(|_| gtk::main_quit());
    window.style_context().add_class("dark");

    let overlay = gtk::Overlay::new();
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.set_margin_top(24);
    root.set_margin_bottom(24);
    root.set_margin_start(24);
    root.set_margin_end(24);
    overlay.add(&root);

    let palette = gtk::EventBox::new();
    palette.style_context().add_class("palette");
    palette.set_halign(gtk::Align::Center);
    palette.set_valign(gtk::Align::Start);
    palette.set_margin_top(96);
    palette.set_size_request(560, -1);
    palette.set_no_show_all(true);
    let palette_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let palette_entry = gtk::Entry::new();
    palette_entry.set_placeholder_text(Some("Type a command…"));
    palette_entry.style_context().add_class("palette-entry");
    palette_box.add(&palette_entry);
    let palette_list = gtk::ListBox::new();
    palette_list.set_selection_mode(gtk::SelectionMode::Single);
    let scroller = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_size_request(-1, 300);
    scroller.add(&palette_list);
    palette_box.add(&scroller);
    palette.add(&palette_box);
    overlay.add_overlay(&palette);
    window.add(&overlay);

    let server = std::env::var("TIAKI_DESKTOP_URL").unwrap_or_else(|_| DEFAULT_URL.into());
    let app = Rc::new(RefCell::new(App {
        window: window.clone(),
        root: root.clone(),
        palette: palette.clone(),
        palette_entry: palette_entry.clone(),
        palette_list: palette_list.clone(),
        server,
        revision: 0,
        keymap: Keymap::default_for().bindings,
        focusables: Vec::new(),
        focus_index: 0,
        clocks: Vec::new(),
    }));

    show_connecting(&root);

    // Palette filtering and Enter.
    {
        let app_changed = app.clone();
        let list_changed = palette_list.clone();
        palette_entry.connect_changed(move |entry| {
            let keymap = app_changed.borrow().keymap.clone();
            rebuild_palette(&list_changed, &keymap, &entry.text().to_lowercase());
        });
    }
    {
        let app_enter = app.clone();
        let list_enter = palette_list.clone();
        let palette_enter = palette.clone();
        palette_entry.connect_activate(move |_| {
            if let Some(row) = first_row(&list_enter) {
                palette_enter.hide();
                dispatch(&app_enter, &row.widget_name());
            }
        });
    }
    {
        let app_row = app.clone();
        let palette_row = palette.clone();
        palette_list.connect_row_activated(move |_, row| {
            palette_row.hide();
            dispatch(&app_row, &row.widget_name());
        });
    }
    {
        let palette_esc = palette.clone();
        palette_entry.connect_key_press_event(move |_, event| {
            if event.keyval() == KEY_ESCAPE.into() {
                palette_esc.hide();
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
    }

    let app_keys = app.clone();
    window.connect_key_press_event(move |_, event| handle_key(&app_keys, event));

    let app_poll = app.clone();
    gtk::glib::timeout_add_seconds_local(2, move || {
        poll(&app_poll);
        gtk::glib::ControlFlow::Continue
    });
    let app_clock = app.clone();
    gtk::glib::timeout_add_seconds_local(1, move || {
        update_clocks(&app_clock);
        gtk::glib::ControlFlow::Continue
    });

    window.show_all();
    poll(&app);
    gtk::main();
}

fn install_css() {
    let provider = gtk::CssProvider::new();
    provider
        .load_from_data(CSS.as_bytes())
        .expect("valid desktop CSS");
    if let Some(screen) = gtk::gdk::Screen::default() {
        gtk::StyleContext::add_provider_for_screen(
            &screen,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

fn show_connecting(root: &gtk::Box) {
    let label = gtk::Label::new(Some("Connecting to Tiaki…"));
    label.set_xalign(0.0);
    label.style_context().add_class("text-title");
    root.pack_start(&label, false, false, 0);
    root.show_all();
}

fn handle_key(app: &Rc<RefCell<App>>, event: &gtk::gdk::EventKey) -> gtk::glib::Propagation {
    let key = event.keyval();
    let state = event.state();
    let sup = state.contains(gtk::gdk::ModifierType::MOD4_MASK);
    let palette_visible = app.borrow().palette.is_visible();

    if key == KEY_ESCAPE.into() && palette_visible {
        app.borrow().palette.hide();
        return gtk::glib::Propagation::Stop;
    }
    if palette_visible {
        return gtk::glib::Propagation::Proceed;
    }
    if sup && key == KEY_SPACE.into() {
        open_palette(app);
        return gtk::glib::Propagation::Stop;
    }
    if key == KEY_SLASH.into()
        || key == KEY_QUESTION.into()
    {
        open_palette(app);
        return gtk::glib::Propagation::Stop;
    }
    if sup && key == KEY_T.into() {
        dispatch(app, "theme.toggle");
        return gtk::glib::Propagation::Stop;
    }
    if sup && key == KEY_K.into() {
        dispatch(app, "clock.toggle_format");
        return gtk::glib::Propagation::Stop;
    }
    if sup && key == KEY_LEFT.into() {
        dispatch(app, "focus.left");
        return gtk::glib::Propagation::Stop;
    }
    if sup && key == KEY_RIGHT.into() {
        dispatch(app, "focus.right");
        return gtk::glib::Propagation::Stop;
    }
    gtk::glib::Propagation::Proceed
}

fn open_palette(app: &Rc<RefCell<App>>) {
    let (palette, entry, list) = {
        let app = app.borrow();
        (
            app.palette.clone(),
            app.palette_entry.clone(),
            app.palette_list.clone(),
        )
    };
    let keymap = app.borrow().keymap.clone();
    entry.set_text("");
    rebuild_palette(&list, &keymap, "");
    palette.set_no_show_all(false);
    palette.show_all();
    entry.grab_focus();
}

fn rebuild_palette(list: &gtk::ListBox, keymap: &[Binding], needle: &str) {
    for child in list.children() {
        list.remove(&child);
    }
    for binding in keymap {
        let haystack = format!(
            "{} {} {}",
            binding.label.to_lowercase(),
            binding.action.to_lowercase(),
            binding.keys.to_lowercase()
        );
        if !needle.is_empty() && !haystack.contains(needle) {
            continue;
        }
        let row = gtk::ListBoxRow::new();
        row.set_widget_name(&binding.action);
        row.style_context().add_class("palette-row");
        let line = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        line.set_margin_top(4);
        line.set_margin_bottom(4);
        let label = gtk::Label::new(Some(&binding.label));
        label.set_xalign(0.0);
        label.set_hexpand(true);
        line.add(&label);
        let keys = gtk::Label::new(Some(&pretty_keys(&binding.keys)));
        keys.style_context().add_class("kbd");
        line.add(&keys);
        row.add(&line);
        list.add(&row);
    }
    list.show_all();
}

fn first_row(list: &gtk::ListBox) -> Option<gtk::ListBoxRow> {
    list.children().into_iter().find_map(|child| {
        child
            .downcast::<gtk::ListBoxRow>()
            .ok()
            .filter(|row| row.is_visible())
    })
}

fn dispatch(app: &Rc<RefCell<App>>, action: &str) {
    match action {
        "palette.open" => {
            open_palette(app);
            return;
        }
        "ui.reload" => {
            poll(app);
            return;
        }
        "focus.left" | "focus.up" => {
            focus_move(app, -1);
            return;
        }
        "focus.right" | "focus.down" => {
            focus_move(app, 1);
            return;
        }
        _ => {}
    }
    let server = app.borrow().server.clone();
    if let Some(body) = http_get(&format!("{server}/action?name={action}")) {
        if let Some(state) = parse_state(&body) {
            apply_state(app, state);
            return;
        }
    }
    poll(app);
}

fn focus_move(app: &Rc<RefCell<App>>, delta: isize) {
    let (focusables, index) = {
        let app = app.borrow();
        (app.focusables.clone(), app.focus_index)
    };
    if focusables.is_empty() {
        return;
    }
    let next = ((index as isize + delta).rem_euclid(focusables.len() as isize)) as usize;
    focusables[next].grab_focus();
    app.borrow_mut().focus_index = next;
}

fn poll(app: &Rc<RefCell<App>>) {
    let server = app.borrow().server.clone();
    if let Some(body) = http_get(&format!("{server}/tree.json")) {
        if let Some(state) = parse_state(&body) {
            if state.revision != app.borrow().revision {
                apply_state(app, state);
            }
        }
    }
    if std::path::Path::new("/tmp/tiaki-palette").exists() {
        let _ = std::fs::remove_file("/tmp/tiaki-palette");
        open_palette(app);
    }
}

fn apply_state(app: &Rc<RefCell<App>>, state: State) {
    let (root, window) = {
        let app = app.borrow();
        (app.root.clone(), app.window.clone())
    };
    for child in root.children() {
        root.remove(&child);
    }
    {
        let mut app = app.borrow_mut();
        app.revision = state.revision;
        app.keymap = state.keymap.clone();
        app.focusables.clear();
        app.focus_index = 0;
        app.clocks.clear();
    }
    let context = window.style_context();
    context.remove_class("dark");
    context.remove_class("light");
    context.add_class(&state.theme);

    let widget = build_node(app, &state.tree.root);
    root.pack_start(&widget, true, true, 0);
    root.show_all();
    update_clocks(app);
}

fn build_node(app: &Rc<RefCell<App>>, node: &Node) -> gtk::Widget {
    match node.kind.as_str() {
        "workspace" => {
            let outer = gtk::Box::new(gtk::Orientation::Vertical, 16);
            outer.set_hexpand(true);
            outer.set_vexpand(true);
            if let Some(title) = node.prop_str("title") {
                let label = gtk::Label::new(Some(&title.to_uppercase()));
                label.set_xalign(0.0);
                label.style_context().add_class("workspace-title");
                outer.pack_start(&label, false, false, 0);
            }
            for child in &node.children {
                let widget = build_node(app, child);
                let expand = child.kind == "spacer";
                outer.pack_start(&widget, expand, expand, 0);
            }
            outer.upcast()
        }
        "panel" => {
            let panel = gtk::EventBox::new();
            panel.style_context().add_class("panel");
            panel.set_hexpand(true);
            let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
            if let Some(title) = node.prop_str("title") {
                let label = gtk::Label::new(Some(&title.to_uppercase()));
                label.set_xalign(0.0);
                label.style_context().add_class("panel-title");
                content.pack_start(&label, false, false, 0);
            }
            let layout = node.prop_str("layout").unwrap_or("row");
            let body = gtk::Box::new(
                if layout == "column" {
                    gtk::Orientation::Vertical
                } else {
                    gtk::Orientation::Horizontal
                },
                12,
            );
            body.set_hexpand(true);
            for child in &node.children {
                let widget = build_node(app, child);
                let expand = child.kind == "spacer" || child.kind == "card";
                body.pack_start(&widget, expand, expand, 0);
            }
            content.pack_start(&body, true, true, 0);
            panel.add(&content);
            panel.upcast()
        }
        "text" => {
            let value = node.prop_str("value").unwrap_or("");
            let size = node.prop_str("size").unwrap_or("body");
            let label = gtk::Label::new(Some(value));
            label.set_xalign(0.0);
            label.set_line_wrap(true);
            label.set_max_width_chars(90);
            label.style_context().add_class(match size {
                "title" => "text-title",
                "small" => "text-small",
                _ => "text",
            });
            label.upcast()
        }
        "clock" => {
            let format = node.prop_str("format").unwrap_or("24h").to_string();
            let seconds = node.prop_bool("show_seconds").unwrap_or(false);
            let label = gtk::Label::new(Some("--:--"));
            label.style_context().add_class("clock");
            app.borrow_mut()
                .clocks
                .push((label.clone(), format, seconds));
            label.upcast()
        }
        "status" => {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            if let Some(items) = node.props.get("items").and_then(Json::as_array) {
                for item in items {
                    if let Some(text) = item.as_str() {
                        let chip = gtk::EventBox::new();
                        chip.style_context().add_class("chip");
                        let label = gtk::Label::new(Some(text));
                        chip.add(&label);
                        row.pack_start(&chip, false, false, 0);
                    }
                }
            }
            row.upcast()
        }
        "spacer" => {
            let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            spacer.upcast()
        }
        "card" => {
            let action = node.prop_str("action").unwrap_or("noop").to_string();
            let button = gtk::Button::new();
            button.set_relief(gtk::ReliefStyle::None);
            button.style_context().add_class("card");
            button.set_hexpand(true);
            let content = gtk::Box::new(gtk::Orientation::Vertical, 4);
            let title = gtk::Label::new(Some(node.prop_str("title").unwrap_or("Card")));
            title.set_xalign(0.0);
            title.style_context().add_class("card-title");
            content.add(&title);
            if let Some(body) = node.prop_str("body") {
                let label = gtk::Label::new(Some(body));
                label.set_xalign(0.0);
                label.set_line_wrap(true);
                label.style_context().add_class("card-body");
                content.add(&label);
            }
            button.add(&content);
            let app_click = app.clone();
            let action_click = action.clone();
            button.connect_clicked(move |_| dispatch(&app_click, &action_click));
            app.borrow_mut()
                .focusables
                .push(button.clone().upcast());
            button.upcast()
        }
        "button" => {
            let action = node.prop_str("action").unwrap_or("noop").to_string();
            let button = gtk::Button::with_label(node.prop_str("label").unwrap_or("Button"));
            button.set_relief(gtk::ReliefStyle::None);
            button.style_context().add_class("button");
            let app_click = app.clone();
            let action_click = action.clone();
            button.connect_clicked(move |_| dispatch(&app_click, &action_click));
            app.borrow_mut()
                .focusables
                .push(button.clone().upcast());
            button.upcast()
        }
        "menu" => {
            let label = gtk::Label::new(Some(node.prop_str("label").unwrap_or("Menu")));
            label.set_xalign(0.0);
            label.style_context().add_class("text-small");
            label.upcast()
        }
        "menuitem" => {
            let action = node.prop_str("action").unwrap_or("noop").to_string();
            let button = gtk::Button::with_label(node.prop_str("label").unwrap_or("Item"));
            button.set_relief(gtk::ReliefStyle::None);
            button.style_context().add_class("button");
            let app_click = app.clone();
            let action_click = action.clone();
            button.connect_clicked(move |_| dispatch(&app_click, &action_click));
            button.upcast()
        }
        "app" => {
            let label = gtk::Label::new(Some(node.prop_str("label").unwrap_or("App")));
            label.set_xalign(0.0);
            label.style_context().add_class("text");
            label.upcast()
        }
        "separator" => {
            let orientation = if node.prop_str("orientation") == Some("horizontal") {
                gtk::Orientation::Horizontal
            } else {
                gtk::Orientation::Vertical
            };
            gtk::Separator::new(orientation).upcast()
        }
        other => {
            let label = gtk::Label::new(Some(&format!("unknown widget: {other}")));
            label.upcast()
        }
    }
}

fn update_clocks(app: &Rc<RefCell<App>>) {
    let clocks = app.borrow().clocks.clone();
    if clocks.is_empty() {
        return;
    }
    let (hours, minutes, seconds) = gtk::glib::DateTime::now_local()
        .map(|now| (now.hour(), now.minute(), now.second()))
        .unwrap_or((0, 0, 0));
    for (label, format, show_seconds) in clocks {
        let text = if format == "12h" {
            let suffix = if hours >= 12 { " PM" } else { " AM" };
            let hour = if hours % 12 == 0 { 12 } else { hours % 12 };
            if show_seconds {
                format!("{hour:02}:{minutes:02}:{seconds:02}{suffix}")
            } else {
                format!("{hour:02}:{minutes:02}{suffix}")
            }
        } else if show_seconds {
            format!("{hours:02}:{minutes:02}:{seconds:02}")
        } else {
            format!("{hours:02}:{minutes:02}")
        };
        label.set_text(&text);
    }
}

fn parse_state(body: &str) -> Option<State> {
    let value = agentic_json::parse(body).ok()?;
    let object = value.as_object()?;
    let revision = object.get("revision")?.as_str()?.parse().ok()?;
    let theme = object.get("theme")?.as_str()?.to_string();
    let tree = UiTree::parse(&object.get("tree")?.render()).ok()?;
    let keymap = object
        .get("keymap")
        .and_then(Json::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| {
                    let entry = value.as_object()?;
                    Some(Binding {
                        keys: entry.get("keys")?.as_str()?.to_string(),
                        action: entry.get("action")?.as_str()?.to_string(),
                        label: entry.get("label")?.as_str()?.to_string(),
                    })
                })
                .collect::<Vec<_>>()
        })
        .filter(|bindings| !bindings.is_empty())
        .unwrap_or_else(|| Keymap::default_for().bindings);
    Some(State {
        revision,
        theme,
        tree,
        keymap,
    })
}

fn http_get(url: &str) -> Option<String> {
    let rest = url.strip_prefix("http://")?;
    let (host, path) = match rest.split_once('/') {
        Some((host, path)) => (host, format!("/{path}")),
        None => (rest, "/".to_string()),
    };
    let mut stream = TcpStream::connect(host).ok()?;
    stream
        .set_read_timeout(Some(Duration::from_millis(800)))
        .ok()?;
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
    )
    .ok()?;
    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;
    let (_, body) = response.split_once("\r\n\r\n")?;
    Some(body.to_string())
}

fn pretty_keys(keys: &str) -> String {
    keys.split('+')
        .map(|part| match part {
            "super" => "⌘".to_string(),
            "shift" => "⇧".to_string(),
            "alt" => "⌥".to_string(),
            "ctrl" => "⌃".to_string(),
            other => {
                let mut chars = other.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
