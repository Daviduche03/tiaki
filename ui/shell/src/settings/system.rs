use super::Nav;
use super::{data::*, widgets::*};
use crate::system;
use gtk::prelude::*;

pub(crate) fn system_page(nav: &Nav) -> gtk::Box {
    let page = page_shell();
    section(&page, "System");
    let (_card, body) = card(&page);
    let rows = [
        (
            "monitor",
            "Display",
            "Monitors, brightness, night light",
            display_summary(),
            "settings.page.display",
        ),
        (
            "audio",
            "Sound",
            "Volume, output, input",
            "PipeWire".to_string(),
            "settings.page.sound",
        ),
        (
            "power",
            "Power",
            "Battery, sleep, screen",
            power_summary(),
            "settings.page.power",
        ),
        (
            "image",
            "Storage",
            "Apps, files, generations",
            storage_summary(),
            "settings.page.storage",
        ),
        (
            "info",
            "About",
            "Device name, hardware, OS",
            "Tiaki".to_string(),
            "settings.page.about",
        ),
    ];
    for (icon, title, subtitle, value, action) in rows {
        let (row, right) = row(&body, Some(icon), title, subtitle);
        value_text(&right, &value);
        chevron_right(&right);
        link_row(&row, nav, action);
    }

    section(&page, "Related");
    let (_related, related_body) = card(&page);
    let (agents, right) = row(
        &related_body,
        Some("spark"),
        "Agents",
        "Inspect capabilities and the transaction ledger",
    );
    chevron_right(&right);
    agents.connect_button_press_event(|_, _| {
        system::spawn("foot -c /etc/tiaki/foot.ini -e /usr/local/bin/agentd state");
        gtk::glib::Propagation::Stop
    });
    page
}

// ------------------------------------------------------ bluetooth & devices
