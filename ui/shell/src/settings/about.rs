use super::{data::*, widgets::*};

pub(crate) fn about_page() -> gtk::Box {
    let page = page_shell();
    section(&page, "Device specifications");
    let (_card, body) = card(&page);
    for (key, value) in [
        ("Device name", hostname()),
        ("Processor", cpu_model()),
        ("Installed RAM", memory_total()),
        ("System type", format!("{}-bit", usize::BITS)),
        ("OS", os_pretty()),
        ("Kernel", kernel()),
    ] {
        let (_, right) = row(&body, None, key, "");
        value_text(&right, &value);
    }

    section(&page, "Tiaki");
    let (_agentic_card, agentic_body) = card(&page);
    for (key, value) in [
        ("Principal", "user:ada".to_string()),
        ("Mode", "Guarded".to_string()),
        ("Ledger", "Every change recorded".to_string()),
        ("Shell", "tiaki-shell".to_string()),
        ("Compositor", "Sway 1.10 · Wayland".to_string()),
    ] {
        let (_, right) = row(&agentic_body, None, key, "");
        value_text(&right, &value);
    }
    page
}

// ---------------------------------------------------------- wallpaper data
