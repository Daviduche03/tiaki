use super::{data::*, widgets::*};

pub(crate) fn accessibility_page() -> gtk::Box {
    let page = page_shell();
    section(&page, "Vision");
    let (_vision_card, vision_body) = card(&page);
    for (title, subtitle, value) in [
        (
            "Text size",
            "Make text bigger across the desktop",
            text_scale(),
        ),
        (
            "Visual effects",
            "Animations and transparency",
            "On".to_string(),
        ),
        ("Cursor size", "Size of the pointer", cursor_size()),
    ] {
        let (_, right) = row(&vision_body, Some("brightness"), title, subtitle);
        value_text(&right, &value);
    }

    section(&page, "Interaction");
    let (_interaction_card, interaction_body) = card(&page);
    let screen_reader = if command_exists("espeak-ng") {
        "Installed"
    } else {
        "Not installed"
    };
    for (title, subtitle, value) in [
        ("Sticky keys", "Press shortcuts one key at a time", "Off"),
        ("Screen reader", "Spoken interface", screen_reader),
    ] {
        let (_, right) = row(&interaction_body, Some("keyboard"), title, subtitle);
        value_text(&right, value);
    }
    page
}

// ------------------------------------------------------- privacy & security
