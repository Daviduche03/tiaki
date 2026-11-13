//! The agent app, slice one: the roster window.
//!
//! The daily home for assistants. The roster comes from the `agents` panel in
//! the tree (label, icon, one-line detail per assistant), so adding an
//! assistant is a transaction, not a rebuild. The detail pane shows the live
//! default runtime read from `$TIAKI_HOME/runtimes/default`, and the button
//! below it really opens that runtime in a terminal, scheduled in the session
//! class. Threads, the approval inbox, and scheduled tasks are later slices;
//! this one proves the app exists, is reachable, and only offers real doors.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

use agentic_ui_tree::Node;

use crate::{glyphs, model, single, system, theme};

pub const STYLE_SHEETS: &[&str] = &[include_str!("../css/agents.css")];

#[derive(Clone)]
struct Assistant {
    label: String,
    action: String,
    icon: String,
    detail: String,
}

fn assistants_from(root: &Node) -> Vec<Assistant> {
    if let Some(panel) = model::find_by_id(root, "agents") {
        let assistants: Vec<Assistant> = panel
            .children
            .iter()
            .filter(|child| child.kind == "button")
            .map(|child| Assistant {
                label: child.prop_str("label").unwrap_or("Assistant").to_string(),
                action: child.prop_str("action").unwrap_or("noop").to_string(),
                icon: child.prop_str("icon").unwrap_or("agents").to_string(),
                detail: child
                    .prop_str("detail")
                    .unwrap_or("General help.")
                    .to_string(),
            })
            .collect();
        if !assistants.is_empty() {
            return assistants;
        }
    }
    vec![Assistant {
        label: "Helper".to_string(),
        action: "agents.thread.helper".to_string(),
        icon: "agents".to_string(),
        detail: "General tasks, file chores, first stop for anything.".to_string(),
    }]
}

/// The default coding agent, from the agent center first
/// (`tiaki-default-agent`, which reads ~/.config/tiaki/defaults/agent) and
/// the control plane's older runtime pointer second. Read live so the app
/// never disagrees with whichever store the user actually set.
fn default_runtime() -> Option<String> {
    let center = std::env::var("HOME")
        .map(|home| format!("{home}/.config/tiaki/defaults/agent"))
        .unwrap_or_default();
    let from_center = std::fs::read_to_string(center).ok();
    let home = std::env::var("TIAKI_HOME").unwrap_or_else(|_| "/var/lib/tiaki".to_string());
    let from_control = std::fs::read_to_string(format!("{home}/runtimes/default")).ok();
    from_center
        .or(from_control)
        .map(|text| text.trim().to_string())
        .filter(|id| !id.is_empty())
}

/// How many harnesses the agent center has usage records for, and the
/// freshest one. The collectors write one JSON record per harness under
/// ~/.local/state/tiaki/agents/usage; the panel only reads them.
fn usage_summary() -> Option<(usize, String)> {
    let state = std::env::var("XDG_STATE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(|home| std::path::PathBuf::from(home).join(".local/state")))
        .ok()?;
    let dir = state.join("tiaki/agents/usage");
    let entries = std::fs::read_dir(dir).ok()?;
    let mut count = 0usize;
    let mut newest: Option<(std::time::SystemTime, String)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        count += 1;
        let id = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("")
            .to_string();
        let modified = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        if newest.as_ref().map(|(when, _)| modified > *when).unwrap_or(true) {
            newest = Some((modified, id));
        }
    }
    if count == 0 {
        return None;
    }
    let label = match newest {
        Some((_, id)) => format!("{count} harnesses · last {id}"),
        None => format!("{count} harnesses"),
    };
    Some((count, label))
}

pub fn run(root: &Node) {
    let Some(_instance) = single::Instance::acquire("tiaki-agents", "agents", false) else {
        return;
    };

    let assistants = assistants_from(root);
    let first = assistants.first().cloned().unwrap_or(Assistant {
        label: "Helper".to_string(),
        action: "agents.thread.helper".to_string(),
        icon: "agents".to_string(),
        detail: "General tasks.".to_string(),
    });

    let window = theme::base_window("agents-window");
    window.set_title("Agents");
    window.set_resizable(true);
    window.set_default_size(1000, 680);
    window.resize(1000, 680);

    let panel = gtk::EventBox::new();
    panel.set_visible_window(true);
    theme::class(&panel, "agents-panel");

    let shell = gtk::Box::new(gtk::Orientation::Vertical, 0);
    panel.add(&shell);

    // Same 34px reservation as Settings: the chrome overlay sits on top.
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_size_request(-1, 34);
    shell.pack_start(&spacer, false, false, 0);

    let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    body.set_vexpand(true);

    // ------------------------------------------------------------ sidebar
    let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 0);
    sidebar.set_size_request(300, -1);
    theme::class(&sidebar, "agents-sidebar");

    let heading = gtk::Label::new(Some("Assistants"));
    heading.set_xalign(0.0);
    heading.set_margin_top(18);
    heading.set_margin_start(20);
    heading.set_margin_end(20);
    heading.set_margin_bottom(12);
    theme::class(&heading, "agents-heading");
    sidebar.pack_start(&heading, false, false, 0);

    let search_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    search_box.set_margin_start(20);
    search_box.set_margin_end(20);
    search_box.set_margin_bottom(12);
    theme::class(&search_box, "agents-search");
    let search = gtk::Entry::new();
    search.set_has_frame(false);
    search.set_placeholder_text(Some("Find an assistant"));
    theme::class(&search, "agents-search-entry");
    search_box.pack_start(&search, true, true, 0);
    search_box.pack_end(&glyphs::status_icon_tinted(glyphs::settings_icon("search"), 15, (0.56, 0.54, 0.51)), false, false, 0);
    sidebar.pack_start(&search_box, false, false, 0);

    let nav_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    nav_box.set_margin_start(12);
    nav_box.set_margin_end(12);
    sidebar.pack_start(&nav_box, true, true, 0);

    // ------------------------------------------------------------ detail
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.set_hexpand(true);
    // Minimum width so the floating window opens at full size even before any
    // page content forces it wider (sidebar 300 + this 560 + margins).
    content.set_size_request(560, -1);
    content.set_margin_top(18);
    content.set_margin_start(32);
    content.set_margin_end(32);

    let name = gtk::Label::new(None);
    name.set_xalign(0.0);
    theme::class(&name, "agents-name");
    content.pack_start(&name, false, false, 0);

    let detail = gtk::Label::new(None);
    detail.set_xalign(0.0);
    detail.set_line_wrap(true);
    detail.set_margin_top(6);
    detail.set_margin_bottom(24);
    theme::class(&detail, "agents-detail");
    content.pack_start(&detail, false, false, 0);

    let runtime_row = detail_row("Runtime", "—");
    content.pack_start(&runtime_row.0, false, false, 0);
    let usage_row = detail_row("Usage", "—");
    content.pack_start(&usage_row.0, false, false, 0);
    let skill_row = detail_row("Skill", "tiaki");
    content.pack_start(&skill_row.0, false, false, 0);

    match usage_summary() {
        Some((_, label)) => usage_row.1.set_text(&label),
        None => usage_row.1.set_text("no records yet — run tiaki-agent-usage-update"),
    }

    let open = gtk::Button::with_label("Open terminal session");
    open.set_margin_top(24);
    open.set_size_request(220, 40);
    open.set_halign(gtk::Align::Start);
    theme::class(&open, "agents-open");
    content.pack_start(&open, false, false, 0);

    let note = gtk::Label::new(None);
    note.set_xalign(0.0);
    note.set_line_wrap(true);
    note.set_margin_top(16);
    theme::class(&note, "agents-note");
    content.pack_start(&note, false, false, 0);

    body.pack_start(&sidebar, false, false, 0);
    body.pack_start(&content, true, true, 0);
    shell.pack_start(&body, true, true, 0);
    window.add(&panel);

    // ---------------------------------------------------------- behavior
    let buttons: Rc<RefCell<Vec<(String, gtk::Button)>>> = Rc::new(RefCell::new(Vec::new()));
    let name_for_select = name.clone();
    let detail_for_select = detail.clone();
    let select = Rc::new(move |assistant: &Assistant| {
        name_for_select.set_text(&assistant.label);
        detail_for_select.set_text(&assistant.detail);
    });

    for assistant in &assistants {
        let button = gtk::Button::new();
        button.set_relief(gtk::ReliefStyle::None);
        button.set_size_request(-1, 52);
        theme::class(&button, "agents-nav");

        let line = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        line.set_margin_start(14);
        line.set_margin_end(12);
        line.pack_start(&glyphs::status_icon_tinted(glyphs::settings_icon(&assistant.icon), 20, (0.902, 0.878, 0.831)), false, false, 0);
        let text = gtk::Box::new(gtk::Orientation::Vertical, 1);
        text.set_hexpand(true);
        let label = gtk::Label::new(Some(&assistant.label));
        label.set_xalign(0.0);
        theme::class(&label, "agents-nav-label");
        text.pack_start(&label, false, false, 0);
        let sub = gtk::Label::new(Some(&assistant.detail));
        sub.set_xalign(0.0);
        sub.set_ellipsize(gtk::pango::EllipsizeMode::End);
        sub.set_max_width_chars(30);
        theme::class(&sub, "agents-nav-sub");
        text.pack_start(&sub, false, false, 0);
        line.pack_start(&text, true, true, 0);
        button.add(&line);

        let select_clone = select.clone();
        let own = assistant.clone();
        let buttons_clone = buttons.clone();
        button.connect_clicked(move |clicked| {
            for (_, other) in buttons_clone.borrow().iter() {
                other.style_context().remove_class("agents-nav-selected");
            }
            clicked.style_context().add_class("agents-nav-selected");
            select_clone(&own);
        });
        nav_box.pack_start(&button, false, false, 0);
        buttons
            .borrow_mut()
            .push((assistant.action.clone(), button));
    }

    let buttons_for_search = buttons.clone();
    let assistants_for_search = assistants.clone();
    search.connect_changed(move |entry| {
        let query = entry.text().to_lowercase();
        for (action, button) in buttons_for_search.borrow().iter() {
            let matches = assistants_for_search
                .iter()
                .find(|assistant| assistant.action == *action)
                .map(|assistant| assistant.label.to_lowercase().contains(&query))
                .unwrap_or(false);
            button.set_visible(query.is_empty() || matches);
        }
    });

    match default_runtime() {
        Some(runtime) => {
            runtime_row.1.set_text(&runtime);
            open.connect_clicked(move |_| {
                // tiaki-agent knows each harness's unattended flags and the
                // account pin, so it is the one launch path for the center.
                system::spawn_app("tiaki-agent", "foreground");
            });
            note.set_text(
                "Opens the default agent in a terminal, scheduled in the foreground class. \
                 Change it with: tiaki-default-agent <id>",
            );
        }
        None => {
            runtime_row.1.set_text("none set");
            open.set_sensitive(false);
            note.set_text("No default agent. Set one with: tiaki-default-agent <id>");
        }
    }

    // Select the first assistant for real, through the same path a click takes.
    if let Some((_, first_button)) = buttons.borrow().first() {
        first_button
            .style_context()
            .add_class("agents-nav-selected");
    }
    select(&first);

    window.connect_destroy(|_| gtk::main_quit());
    window.show_all();
    window.present();
    gtk::main();
}

/// A two-column facts row: dim label left, value right.
fn detail_row(label: &str, value: &str) -> (gtk::Box, gtk::Label) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    row.set_margin_top(4);
    row.set_margin_bottom(4);
    let key = gtk::Label::new(Some(label));
    key.set_xalign(0.0);
    key.set_size_request(120, -1);
    theme::class(&key, "agents-row-key");
    row.pack_start(&key, false, false, 0);
    let val = gtk::Label::new(Some(value));
    val.set_xalign(0.0);
    val.set_hexpand(true);
    theme::class(&val, "agents-row-value");
    row.pack_start(&val, true, true, 0);
    (row, val)
}
