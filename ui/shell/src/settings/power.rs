use super::{data::*, widgets::*};

pub(crate) fn power_page() -> gtk::Box {
    let page = page_shell();
    section(&page, "Power");
    let (_card, body) = card(&page);
    let mut rows = power_rows();
    if rows.is_empty() {
        rows.push((
            "Source".to_string(),
            "AC · no battery data in this session".to_string(),
        ));
    }
    for (key, value) in rows {
        let (_, right) = row(&body, Some("power"), &key, "");
        value_text(&right, &value);
    }
    let (_, right) = row(&body, Some("moon"), "Suspend", "Suspend the session to RAM");
    value_text(
        &right,
        if command_exists("systemctl") {
            "Available"
        } else {
            "Unavailable"
        },
    );

    note(
        &page,
        "Power data comes from /sys/class/power_supply. This machine reports AC only.",
    );
    page
}

// ---------------------------------------------------------------- storage
