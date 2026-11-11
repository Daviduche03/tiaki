use super::Nav;
use super::{data::*, widgets::*};
use crate::{icons, theme};
use agentic_ui_tree::Node;
use gtk::prelude::*;

pub(crate) fn home_page(root: &Node, nav: &Nav) -> gtk::Box {
    let page = page_shell();

    let top = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    let (device_card, device_body) = new_card();
    let (device_row, device_right) = device_row(
        &device_body,
        "device",
        "Tiaki",
        "Virtual machine · x86_64",
    );
    device_row.set_valign(gtk::Align::Center);
    device_right.pack_start(
        &nav_button("Details", nav, "settings.page.about", false),
        false,
        false,
        0,
    );
    top.pack_start(&device_card, true, true, 0);

    let tiles = gtk::Box::new(gtk::Orientation::Vertical, 12);
    tiles.set_size_request(286, -1);
    let (network_card, network_body) = new_card();
    let (network_row, _) = row(
        &network_body,
        Some("fluent:ethernet"),
        "Ethernet",
        &network_summary(),
    );
    link_row(&network_row, nav, "settings.page.network");
    tiles.pack_start(&network_card, true, true, 0);
    let (update_card, update_body) = new_card();
    let (update_row, _) = row(
        &update_body,
        Some("fluent:update"),
        "System update",
        "Last checked just now",
    );
    link_row(&update_row, nav, "settings.page.update");
    tiles.pack_start(&update_card, true, true, 0);
    top.pack_start(&tiles, false, false, 0);
    page.pack_start(&top, false, false, 0);

    let columns = gtk::Box::new(gtk::Orientation::Horizontal, 20);
    columns.set_margin_top(8);
    let main = gtk::Box::new(gtk::Orientation::Vertical, 0);
    main.set_hexpand(true);
    let side = gtk::Box::new(gtk::Orientation::Vertical, 0);
    side.set_size_request(286, -1);
    columns.pack_start(&main, true, true, 0);
    columns.pack_start(&side, false, false, 0);
    page.pack_start(&columns, true, true, 0);

    section(&main, "Your account");
    let (_account_card, account_body) = card(&main);
    let pad = gtk::Box::new(gtk::Orientation::Vertical, 8);
    pad.set_margin_top(20);
    pad.set_margin_bottom(20);
    pad.set_margin_start(20);
    pad.set_margin_end(20);
    let heading = gtk::Label::new(Some("It's all here with your account"));
    heading.set_xalign(0.0);
    theme::class(&heading, "settings-card-title");
    let sub = gtk::Label::new(Some("One principal, one authority, one ledger."));
    sub.set_xalign(0.0);
    theme::class(&sub, "settings-card-sub");
    let marks = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    for art in ["agents", "files", "terminal", "browser", "settings"] {
        marks.pack_start(&icons::app_icon(art, 30), false, false, 0);
    }
    let manage = nav_button("Manage account", nav, "settings.page.accounts", true);
    manage.set_halign(gtk::Align::Start);
    manage.set_margin_top(4);
    pad.pack_start(&heading, false, false, 0);
    pad.pack_start(&sub, false, false, 0);
    pad.pack_start(&marks, false, false, 0);
    pad.pack_start(&manage, false, false, 0);
    account_body.pack_start(&pad, false, false, 0);

    section(&main, "Personalize your device");
    let (_wall_card, wall_body) = card(&main);
    let current = current_wallpaper();
    let grid = wallpaper_grid(
        &wallpaper_names().into_iter().take(8).collect::<Vec<_>>(),
        4,
        104,
        66,
        &current,
    );
    let wall_pad = gtk::Box::new(gtk::Orientation::Vertical, 0);
    wall_pad.set_margin_top(14);
    wall_pad.set_margin_bottom(14);
    wall_pad.set_margin_start(14);
    wall_pad.set_margin_end(14);
    wall_pad.pack_start(&grid, false, false, 0);
    wall_body.pack_start(&wall_pad, false, false, 0);

    section(&main, "Color mode");
    let (_mode_card, mode_body) = card(&main);
    let (_, mode_right) = row(
        &mode_body,
        Some("moon"),
        "Choose your mode",
        "Change the colors of the desktop and apps",
    );
    mode_right.pack_start(&theme_dropdown(), false, false, 0);

    section(&side, "Recommended settings");
    let (_recommended_card, recommended_body) = card(&side);
    let recommended = [
        (
            "monitor",
            "Display",
            "Brightness, scale, resolution",
            "settings.page.display",
        ),
        ("audio", "Sound", "Volume and output", "settings.page.sound"),
        ("power", "Power", "Battery and sleep", "settings.page.power"),
        (
            "fluent:apps",
            "Default apps",
            "Terminal, browser, files",
            "settings.page.apps",
        ),
    ];
    for (icon, title, subtitle, action) in recommended {
        let (row, right) = row(&recommended_body, Some(icon), title, subtitle);
        chevron_right(&right);
        link_row(&row, nav, action);
    }

    let _ = root;
    page
}
