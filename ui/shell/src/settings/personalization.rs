use super::Nav;
use super::{data::*, widgets::*};
use crate::theme;
use agentic_ui_tree::Node;
use gtk::prelude::*;

pub(crate) fn personalization_page(root: &Node, nav: &Nav) -> gtk::Box {
    let page = page_shell();

    section(&page, "Background");
    let (_background_card, background_body) = card(&page);
    let current = current_wallpaper();
    let holder = gtk::Box::new(gtk::Orientation::Vertical, 12);
    holder.set_margin_top(16);
    holder.set_margin_bottom(16);
    holder.set_margin_start(16);
    holder.set_margin_end(16);

    let preview_path = wallpaper_dir().join(&current);
    if let Ok(pixbuf) = gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(&preview_path, 760, 260, false)
    {
        let image = gtk::Image::from_pixbuf(Some(&pixbuf));
        image.set_halign(gtk::Align::Center);
        theme::class(&image, "wall-preview");
        holder.pack_start(&image, false, false, 0);
    }

    let grid = wallpaper_grid(&wallpaper_names(), 4, 150, 94, &current);
    holder.pack_start(&grid, false, false, 0);

    let caption = gtk::Label::new(Some(
        "Click a picture to apply it now. The choice is remembered.",
    ));
    caption.set_xalign(0.0);
    theme::class(&caption, "settings-note");
    holder.pack_start(&caption, false, false, 0);
    background_body.pack_start(&holder, false, false, 0);

    section(&page, "Typography");
    let (_typo_card, typo_body) = card(&page);
    let cur_font = current_font();
    let (_, typo_right) = row(
        &typo_body,
        Some("settings"),
        "System Typeface",
        "Select primary UI font (Aspekta, SF Pro, Plus Jakarta Sans, IBM Plex)",
    );
    typo_right.pack_start(&font_dropdown(&cur_font), false, false, 0);

    section(&page, "Time");
    let (_time_card, time_body) = card(&page);
    let format = clock_format(root);
    let (_, clock_right) = row(
        &time_body,
        Some("clock"),
        "Menu bar clock",
        "Switch between 12-hour and 24-hour time",
    );
    clock_right.pack_start(&clock_dropdown(&format), false, false, 0);

    section(&page, "Lock screen");
    let (_lock_card, lock_body) = card(&page);
    let (_, right) = row(
        &lock_body,
        Some("lock"),
        "Lock screen",
        "The wallpaper and the session lock",
    );
    value_text(&right, "Session lock");

    let _ = nav;
    page
}
