use super::{data::*, widgets::*};
use agentic_ui_tree::Node;
use gtk::prelude::*;

pub(crate) fn time_page(root: &Node) -> gtk::Box {
    let page = page_shell();
    section(&page, "Date & time");
    let (_card, body) = card(&page);
    for (title, value) in [("Current time", now_string()), ("Time zone", timezone())] {
        let (_, right) = row(&body, None, title, "");
        value_text(&right, &value);
    }
    let (_, right) = row(
        &body,
        Some("clock"),
        "Clock format",
        "12-hour or 24-hour time in the menu bar",
    );
    right.pack_start(&clock_dropdown(&clock_format(root)), false, false, 0);
    let (_, right) = row(
        &body,
        Some("update"),
        "Set time automatically",
        "Keep the clock right with systemd-timesyncd",
    );
    value_text(&right, if timesyncd_active() { "On" } else { "Off" });

    section(&page, "Language & region");
    let (_language_card, language_body) = card(&page);
    for (title, value) in [("Display language", language()), ("Region", region())] {
        let (_, right) = row(&language_body, None, title, "");
        value_text(&right, &value);
    }
    page
}

// ----------------------------------------------------------------- gaming
