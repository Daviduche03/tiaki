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

/// The default agent `agentd agent default` named, if any. Read live so the app
/// never disagrees with the control plane.
fn default_runtime() -> Option<String> {
    let home = std::env::var("TIAKI_HOME").unwrap_or_else(|_| "/var/lib/tiaki".to_string());
    std::fs::read_to_string(format!("{home}/runtimes/default"))
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|id| !id.is_empty())
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
    let skill_row = detail_row("Skill", "tiaki");
    content.pack_start(&skill_row.0, false, false, 0);

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
            let open_runtime = runtime.clone();
            open.connect_clicked(move |_| {
                system::spawn_app(
                    &format!("foot -c /etc/tiaki/foot.ini -e {open_runtime}"),
                    "session",
                );
            });
            note.set_text(
                "Opens the default runtime in a terminal, scheduled in the session class. \
                 Threads, approvals, and scheduled tasks land in later slices.",
            );
        }
        None => {
            runtime_row.1.set_text("none set");
            open.set_sensitive(false);
            note.set_text("No default agent. Set one with: agentd agent default <id>");
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
