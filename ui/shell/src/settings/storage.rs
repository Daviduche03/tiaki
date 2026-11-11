use super::{data::*, widgets::*};

pub(crate) fn storage_page() -> gtk::Box {
    let page = page_shell();
    section(&page, "Storage");
    let (_card, body) = card(&page);
    for (key, value) in disk_rows() {
        let (_, right) = row(&body, Some("image"), &key, "");
        value_text(&right, &value);
    }

    section(&page, "Generations");
    let (_gen_card, gen_body) = card(&page);
    let (_, right) = row(
        &gen_body,
        Some("reload"),
        "Base generation",
        "The immutable system image this session booted",
    );
    value_text(&right, "base");
    let (_, right) = row(
        &gen_body,
        Some("transactions"),
        "Snapshots",
        "Every committed change has a restore point",
    );
    value_text(&right, "On");

    note(
        &page,
        "Generations are read-only. Writing to the system creates a new generation instead of mutating this one.",
    );
    page
}

// ------------------------------------------------------------------ about
