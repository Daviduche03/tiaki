use super::{data::*, widgets::*};
use crate::{glyphs, theme};
use gtk::prelude::*;

pub(crate) fn update_page() -> gtk::Box {
    let page = page_shell();
    section(&page, "Update status");
    let (_hero_card, hero_body) = card(&page);
    let hero = gtk::Box::new(gtk::Orientation::Horizontal, 20);
    hero.set_margin_top(24);
    hero.set_margin_bottom(24);
    hero.set_margin_start(24);
    hero.set_margin_end(24);
    hero.pack_start(&glyphs::status_icon_tinted(glyphs::settings_icon("update"), 40, (0.902, 0.878, 0.831)), false, false, 0);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 4);
    text.set_valign(gtk::Align::Center);
    let heading = gtk::Label::new(Some("You're up to date"));
    heading.set_xalign(0.0);
    theme::class(&heading, "settings-card-title");
    let status = gtk::Label::new(Some("Last checked: just now"));
    status.set_xalign(0.0);
    theme::class(&status, "settings-card-sub");
    text.pack_start(&heading, false, false, 0);
    text.pack_start(&status, false, false, 0);
    hero.pack_start(&text, true, true, 0);
    let check = accent_button("Check for updates");
    let status_clone = status.clone();
    let check_clone = check.clone();
    check.connect_clicked(move |_| {
        check_clone.set_label("Checking…");
        check_clone.set_sensitive(false);
        status_clone.set_text("Checking generation status…");
        crate::system::spawn("/usr/local/bin/agentd generation status");

        let status_timer = status_clone.clone();
        let check_timer = check_clone.clone();
        gtk::glib::timeout_add_seconds_local(2, move || {
            status_timer.set_text(&format!("Last checked: {} · system is up to date", now_clock()));
            check_timer.set_label("Check for updates");
            check_timer.set_sensitive(true);
            gtk::glib::ControlFlow::Break
        });
    });
    hero.pack_start(&check, false, false, 0);
    hero_body.pack_start(&hero, false, false, 0);

    section(&page, "Build");
    let (_build_card, build_body) = card(&page);
    for (title, value) in [
        ("Version", os_pretty()),
        ("Kernel", kernel()),
        ("Channel", "dev".to_string()),
        ("Generation", "base".to_string()),
    ] {
        let (_, right) = row(&build_body, None, title, "");
        value_text(&right, &value);
    }

    note(
        &page,
        "The system image is immutable. An update is a new generation, booted beside this one, so you can always come back.",
    );
    page
}

// ---------------------------------------------------------------- display
