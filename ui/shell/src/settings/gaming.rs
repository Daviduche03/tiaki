use super::widgets::*;

pub(crate) fn gaming_page() -> gtk::Box {
    let page = page_shell();
    section(&page, "Gaming");
    let (_card, body) = card(&page);
    let (_, right) = row(
        &body,
        Some("spark"),
        "Game Mode",
        "Prioritise the foreground game",
    );
    value_text(&right, "Not available");
    let (_, right) = row(
        &body,
        Some("monitor"),
        "Captures",
        "Screen capture and the game bar",
    );
    value_text(&right, "Not available");

    note(
        &page,
        "This build ships no gaming overlay. The compositor runs native Wayland clients directly, with no injected layer between the game and the display.",
    );
    page
}

// ----------------------------------------------------------- accessibility
