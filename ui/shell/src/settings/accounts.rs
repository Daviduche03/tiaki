use super::widgets::*;
use crate::theme;
use gtk::prelude::*;

pub(crate) fn accounts_page() -> gtk::Box {
    let page = page_shell();
    section(&page, "Your account");
    let (_card, body) = card(&page);

    let profile = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    profile.set_margin_top(20);
    profile.set_margin_bottom(20);
    profile.set_margin_start(20);
    profile.set_margin_end(20);
    profile.pack_start(&avatar(64), false, false, 0);
    let who = gtk::Box::new(gtk::Orientation::Vertical, 3);
    who.set_valign(gtk::Align::Center);
    let name = gtk::Label::new(Some("Ada"));
    name.set_xalign(0.0);
    theme::class(&name, "settings-device-name");
    let sub = gtk::Label::new(Some("user:ada · guarded"));
    sub.set_xalign(0.0);
    theme::class(&sub, "settings-row-sub");
    who.pack_start(&name, false, false, 0);
    who.pack_start(&sub, false, false, 0);
    profile.pack_start(&who, true, true, 0);
    body.pack_start(&profile, false, false, 0);

    for (title, value) in [
        ("Authority", "agentd · principal-scoped"),
        ("Mode", "Guarded"),
        ("Sandbox", "bubblewrap"),
        ("Session", "Wayland · Sway 1.10"),
    ] {
        let (_, right) = row(&body, None, title, "");
        value_text(&right, value);
    }

    section(&page, "Sign-in options");
    let (_signin_card, signin_body) = card(&page);
    let (_, right) = row(
        &signin_body,
        Some("lock"),
        "Session restore",
        "The desktop comes back after a reboot",
    );
    value_text(&right, "On");

    note(
        &page,
        "The principal is the boundary: every transaction is attributed to user:ada and can be rolled back.",
    );
    page
}

// --------------------------------------------------------- time & language
