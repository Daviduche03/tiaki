//! Card/row control primitives and the dropdown/grid builders.
use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

use super::data::{live_clock, set_wallpaper, wallpaper_dir};
use super::Nav;
use crate::{glyphs, system, theme};

pub(crate) fn avatar(size: i32) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.set_size_request(size, size);
    theme::class(&area, "settings-avatar");
    area.connect_draw(move |widget, cr| {
        let side = widget.allocated_width().min(widget.allocated_height()) as f64;
        if side <= 0.0 {
            return gtk::glib::Propagation::Proceed;
        }
        let radius = side / 2.0 - 1.0;
        cr.arc(side / 2.0, side / 2.0, radius, 0.0, std::f64::consts::TAU);
        cr.set_source_rgb(0.106, 0.102, 0.09);
        let _ = cr.fill_preserve();
        cr.set_source_rgba(0.902, 0.878, 0.831, 0.24);
        cr.set_line_width(1.0);
        let _ = cr.stroke();

        cr.select_font_face(
            "IBM Plex Sans",
            gtk::cairo::FontSlant::Normal,
            gtk::cairo::FontWeight::Bold,
        );
        cr.set_font_size(side * 0.40);
        if let Ok(extents) = cr.text_extents("A") {
            cr.move_to(
                side / 2.0 - extents.width() / 2.0,
                side / 2.0 + extents.height() / 2.0,
            );
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.96);
            let _ = cr.show_text("A");
        }
        gtk::glib::Propagation::Proceed
    });
    area
}

pub(crate) fn page_shell() -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    page.set_margin_top(2);
    page.set_margin_bottom(40);
    page.set_margin_start(40);
    page.set_margin_end(40);
    page
}

pub(crate) fn section(page: &gtk::Box, title: &str) {
    let label = gtk::Label::new(Some(title));
    label.set_xalign(0.0);
    label.set_margin_top(18);
    label.set_margin_bottom(8);
    theme::class(&label, "settings-section");
    page.pack_start(&label, false, false, 0);
}

pub(crate) fn new_card() -> (gtk::EventBox, gtk::Box) {
    let card = gtk::EventBox::new();
    card.set_visible_window(true);
    theme::class(&card, "settings-card");
    let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
    card.add(&body);
    (card, body)
}

pub(crate) fn card(page: &gtk::Box) -> (gtk::EventBox, gtk::Box) {
    let (card, body) = new_card();
    page.pack_start(&card, false, false, 0);
    (card, body)
}

/// A card that starts a new visual group, with the reference's gap.
pub(crate) fn card_spaced(page: &gtk::Box) -> (gtk::EventBox, gtk::Box) {
    let (card, body) = new_card();
    card.set_margin_top(8);
    page.pack_start(&card, false, false, 0);
    (card, body)
}

/// A full-width row inside a card group. Returns the row and its trailing box.
pub(crate) fn row(
    body: &gtk::Box,
    icon: Option<&str>,
    title: &str,
    subtitle: &str,
) -> (gtk::EventBox, gtk::Box) {
    row_full(body, icon, title, subtitle, None)
}

pub(crate) fn row_full(
    body: &gtk::Box,
    icon: Option<&str>,
    title: &str,
    subtitle: &str,
    link: Option<&str>,
) -> (gtk::EventBox, gtk::Box) {
    if !body.children().is_empty() {
        let divider = gtk::Box::new(gtk::Orientation::Vertical, 0);
        divider.set_size_request(-1, 1);
        theme::class(&divider, "settings-divider");
        body.pack_start(&divider, false, false, 0);
    }

    let row = gtk::EventBox::new();
    row.set_visible_window(true);
    theme::class(&row, "settings-row");
    row.set_size_request(-1, if subtitle.is_empty() { 60 } else { 72 });

    let line = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    line.set_margin_start(20);
    line.set_margin_end(14);
    line.set_valign(gtk::Align::Center);

    match icon {
        Some("") => {
            let pad = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            pad.set_size_request(38, -1);
            line.pack_start(&pad, false, false, 0);
        }
        Some(name) => {
            line.pack_start(
                &glyphs::status_icon_tinted(
                    glyphs::settings_icon(name),
                    22,
                    (0.902, 0.878, 0.831),
                ),
                false,
                false,
                0,
            );
        }
        None => {
            let pad = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            pad.set_size_request(38, -1);
            line.pack_start(&pad, false, false, 0);
        }
    }

    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_valign(gtk::Align::Center);
    text.set_hexpand(true);
    let title_label = gtk::Label::new(Some(title));
    title_label.set_xalign(0.0);
    theme::class(&title_label, "settings-row-title");
    text.pack_start(&title_label, false, false, 0);
    if !subtitle.is_empty() {
        let sub_label = gtk::Label::new(Some(subtitle));
        sub_label.set_xalign(0.0);
        sub_label.set_line_wrap(true);
        sub_label.set_max_width_chars(62);
        theme::class(&sub_label, "settings-row-sub");
        text.pack_start(&sub_label, false, false, 0);
    }
    if let Some(link) = link {
        let link_label = gtk::Label::new(Some(link));
        link_label.set_xalign(0.0);
        theme::class(&link_label, "settings-link");
        text.pack_start(&link_label, false, false, 0);
    }
    line.pack_start(&text, true, true, 0);

    let right = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    right.set_valign(gtk::Align::Center);
    line.pack_start(&right, false, false, 0);

    row.add(&line);
    body.pack_start(&row, false, false, 0);
    (row, right)
}

/// A row whose leading widget is an app mark rather than a glyph.
pub(crate) fn row_with(
    body: &gtk::Box,
    leading: &impl IsA<gtk::Widget>,
    title: &str,
    subtitle: &str,
) -> (gtk::EventBox, gtk::Box) {
    if !body.children().is_empty() {
        let divider = gtk::Box::new(gtk::Orientation::Vertical, 0);
        divider.set_size_request(-1, 1);
        theme::class(&divider, "settings-divider");
        body.pack_start(&divider, false, false, 0);
    }
    let row = gtk::EventBox::new();
    row.set_visible_window(true);
    theme::class(&row, "settings-row");
    row.set_size_request(-1, 62);
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    line.set_margin_start(18);
    line.set_margin_end(14);
    line.set_valign(gtk::Align::Center);
    line.pack_start(leading, false, false, 0);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_valign(gtk::Align::Center);
    text.set_hexpand(true);
    let title_label = gtk::Label::new(Some(title));
    title_label.set_xalign(0.0);
    theme::class(&title_label, "settings-row-title");
    text.pack_start(&title_label, false, false, 0);
    if !subtitle.is_empty() {
        let sub_label = gtk::Label::new(Some(subtitle));
        sub_label.set_xalign(0.0);
        theme::class(&sub_label, "settings-row-sub");
        text.pack_start(&sub_label, false, false, 0);
    }
    line.pack_start(&text, true, true, 0);
    let right = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    right.set_valign(gtk::Align::Center);
    line.pack_start(&right, false, false, 0);
    row.add(&line);
    body.pack_start(&row, false, false, 0);
    (row, right)
}

pub(crate) fn link_row(row: &gtk::EventBox, nav: &Nav, action: &str) {
    row.style_context().add_class("settings-row-link");
    let nav = nav.clone();
    let action = action.to_string();
    row.connect_button_press_event(move |_, _| {
        nav.go(&action);
        gtk::glib::Propagation::Stop
    });
}

pub(crate) fn chevron_right(right: &gtk::Box) {
    let holder = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    holder.set_margin_end(4);
    holder.pack_start(&glyphs::status_icon_tinted(glyphs::settings_icon("chevron_right"), 12, (0.56, 0.54, 0.51)), false, false, 0);
    right.pack_start(&holder, false, false, 0);
}

pub(crate) fn value_text(right: &gtk::Box, text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    theme::class(&label, "settings-value");
    right.pack_start(&label, false, false, 0);
    label
}

pub(crate) fn toggle_switch(active: bool, on_change: impl Fn(bool) + 'static) -> gtk::Switch {
    let switch = gtk::Switch::new();
    switch.set_active(active);
    switch.set_valign(gtk::Align::Center);
    theme::class(&switch, "win-switch");
    switch.connect_state_set(move |_, state| {
        on_change(state);
        gtk::glib::Propagation::Proceed
    });
    switch
}

pub(crate) fn slider(right: &gtk::Box, value: f64, on_change: impl Fn(f64) + 'static) {
    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    scale.set_value(value);
    scale.set_draw_value(false);
    scale.set_size_request(300, -1);
    theme::class(&scale, "win-scale");
    scale.connect_value_changed(move |scale| on_change(scale.value()));
    right.pack_start(&scale, false, false, 0);
}

pub(crate) fn dropdown(current: &str, options: Vec<(String, Rc<dyn Fn()>)>) -> gtk::MenuButton {
    let button = gtk::MenuButton::new();
    button.set_relief(gtk::ReliefStyle::None);
    theme::class(&button, "settings-dropdown");

    let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    line.set_margin_start(12);
    line.set_margin_end(10);
    let label = gtk::Label::new(Some(current));
    label.set_xalign(0.0);
    label.set_hexpand(true);
    theme::class(&label, "settings-dropdown-label");
    line.pack_start(&label, true, true, 0);
    line.pack_start(&glyphs::status_icon_tinted(glyphs::settings_icon("chevron_down"), 9, (0.56, 0.54, 0.51)), false, false, 0);
    button.add(&line);

    let popover = gtk::Popover::new(None::<&gtk::Widget>);
    popover.set_relative_to(Some(&button));
    popover.set_position(gtk::PositionType::Bottom);
    popover.set_modal(false);
    theme::class(&popover, "settings-popover");
    let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
    for (text, action) in options {
        let item = gtk::Button::with_label(&text);
        item.set_relief(gtk::ReliefStyle::None);
        theme::class(&item, "settings-popover-item");
        let popover_clone = popover.clone();
        let label_clone = label.clone();
        let text_clone = text.clone();
        item.connect_clicked(move |_| {
            action();
            label_clone.set_text(&text_clone);
            popover_clone.popdown();
        });
        list.pack_start(&item, false, false, 0);
    }
    popover.add(&list);
    // Showing the popover itself would map it at build time; show only its
    // contents, and let the menu button map it when it opens.
    list.show_all();
    button.set_popover(Some(&popover));
    button
}

pub(crate) fn pill_button(text: &str) -> gtk::Button {
    let button = gtk::Button::with_label(text);
    button.set_relief(gtk::ReliefStyle::None);
    theme::class(&button, "settings-pill");
    button
}

pub(crate) fn accent_button(text: &str) -> gtk::Button {
    let button = gtk::Button::with_label(text);
    button.set_relief(gtk::ReliefStyle::None);
    theme::class(&button, "settings-accent");
    button
}

pub(crate) fn nav_button(text: &str, nav: &Nav, action: &str, accent: bool) -> gtk::Button {
    let button = if accent {
        accent_button(text)
    } else {
        pill_button(text)
    };
    let nav = nav.clone();
    let action = action.to_string();
    button.connect_clicked(move |_| nav.go(&action));
    button
}

pub(crate) fn note(page: &gtk::Box, text: &str) {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.set_line_wrap(true);
    label.set_max_width_chars(88);
    label.set_margin_top(12);
    theme::class(&label, "settings-note");
    page.pack_start(&label, false, false, 0);
}

pub(crate) fn device_row(
    body: &gtk::Box,
    icon: &str,
    title: &str,
    subtitle: &str,
) -> (gtk::EventBox, gtk::Box) {
    let row = gtk::EventBox::new();
    row.set_visible_window(true);
    theme::class(&row, "settings-row");
    row.set_size_request(-1, 76);
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    line.set_margin_start(20);
    line.set_margin_end(16);
    line.set_valign(gtk::Align::Center);
    line.pack_start(
        &glyphs::status_icon_tinted(
            glyphs::settings_icon(icon),
            30,
            (0.902, 0.878, 0.831),
        ),
        false,
        false,
        0,
    );
    let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
    text.set_valign(gtk::Align::Center);
    text.set_hexpand(true);
    let title_label = gtk::Label::new(Some(title));
    title_label.set_xalign(0.0);
    theme::class(&title_label, "settings-device-name");
    text.pack_start(&title_label, false, false, 0);
    if !subtitle.is_empty() {
        let sub_label = gtk::Label::new(Some(subtitle));
        sub_label.set_xalign(0.0);
        theme::class(&sub_label, "settings-row-sub");
        text.pack_start(&sub_label, false, false, 0);
    }
    line.pack_start(&text, true, true, 0);
    let right = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    right.set_valign(gtk::Align::Center);
    line.pack_start(&right, false, false, 0);
    row.add(&line);
    body.pack_start(&row, false, false, 0);
    (row, right)
}

// ------------------------------------------------------------------- home

pub(crate) fn clock_dropdown(current: &str) -> gtk::MenuButton {
    let h12: Rc<dyn Fn()> = Rc::new(|| {
        if live_clock() != "12h" {
            system::run_action("clock.toggle_format");
        }
    });
    let h24: Rc<dyn Fn()> = Rc::new(|| {
        if live_clock() != "24h" {
            system::run_action("clock.toggle_format");
        }
    });
    dropdown(
        if current == "24h" {
            "24-hour"
        } else {
            "12-hour"
        },
        vec![("12-hour".to_string(), h12), ("24-hour".to_string(), h24)],
    )
}

pub(crate) fn font_dropdown(current: &str) -> gtk::MenuButton {
    let mut options = Vec::new();
    for (name, desc) in system_fonts() {
        let n = name.to_string();
        let cb: Rc<dyn Fn()> = Rc::new(move || {
            set_font(&n);
        });
        options.push((format!("{name} — {desc}"), cb));
    }
    dropdown(current, options)
}

// ------------------------------------------------------------------- apps

pub(crate) fn wallpaper_grid(
    names: &[String],
    columns: i32,
    width: i32,
    height: i32,
    current: &str,
) -> gtk::Grid {
    let grid = gtk::Grid::new();
    grid.set_row_spacing(8);
    grid.set_column_spacing(8);
    grid.set_halign(gtk::Align::Start);
    let group: Rc<RefCell<Vec<gtk::Button>>> = Rc::new(RefCell::new(Vec::new()));

    for (index, name) in names.iter().enumerate() {
        let button = gtk::Button::new();
        button.set_relief(gtk::ReliefStyle::None);
        theme::class(&button, "wall-thumb");
        if name == current {
            button.style_context().add_class("active");
        }
        let path = wallpaper_dir().join(name);
        let image = match gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(&path, width, height, false) {
            Ok(pixbuf) => gtk::Image::from_pixbuf(Some(&pixbuf)),
            Err(_) => gtk::Image::new(),
        };
        image.set_size_request(width, height);
        button.add(&image);

        let name = name.clone();
        let group_clone = group.clone();
        let button_clone = button.clone();
        button.connect_clicked(move |_| {
            set_wallpaper(&name);
            for candidate in group_clone.borrow().iter() {
                candidate.style_context().remove_class("active");
            }
            button_clone.style_context().add_class("active");
        });
        group.borrow_mut().push(button.clone());
        grid.attach(
            &button,
            index as i32 % columns,
            index as i32 / columns,
            1,
            1,
        );
    }
    grid
}

// --------------------------------------------------------- live tree data
