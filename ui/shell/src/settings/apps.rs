use super::widgets::*;
use crate::{icons, model, system};
use agentic_ui_tree::Node;
use gtk::prelude::*;

pub(crate) fn apps_page(root: &Node) -> gtk::Box {
    let page = page_shell();
    section(&page, "Installed apps");
    let (_card, body) = card(&page);
    for app in model::launcher_apps(root).into_iter().take(14) {
        let (_, right) = row_with(
            &body,
            &icons::app_icon(&app.art, 26),
            &app.label,
            &app.category,
        );
        let open = pill_button("Open");
        let command = app.command.clone();
        open.connect_clicked(move |_| system::open_command(&command));
        right.pack_start(&open, false, false, 0);
    }
    note(
        &page,
        "Apps are tree nodes. Adding, removing, or reordering one is a transaction on the ledger.",
    );
    page
}

// --------------------------------------------------------------- accounts
