use super::{data::*, widgets::*};
use crate::{glyphs, system, theme};
use gtk::prelude::*;
use std::rc::Rc;

pub(crate) fn display_page() -> gtk::Box {
    let page = page_shell();
    section(&page, "Brightness & color");
    let (_brightness_card, brightness_body) = card(&page);

    let (_, right) = row(
        &brightness_body,
        Some("brightness"),
        "Brightness",
        "Change brightness for the built-in display",
    );
    match backlight() {
        Some((dir, current, max)) => {
            let percent = current / max * 100.0;
            slider(&right, percent, move |value| {
                let raw = ((value / 100.0 * max) as u32).max(1).to_string();
                let _ = std::fs::write(dir.join("brightness"), raw);
            });
        }
        None => {
            let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
            scale.set_value(100.0);
            scale.set_draw_value(false);
            scale.set_size_request(300, -1);
            scale.set_sensitive(false);
            theme::class(&scale, "win-scale");
            right.pack_start(&scale, false, false, 0);
        }
    }
    right.pack_start(&glyphs::status_icon_tinted(glyphs::settings_icon("chevron_up"), 12, (0.56, 0.54, 0.51)), false, false, 0);

    let (_, right) = row(
        &brightness_body,
        Some(""),
        "Change brightness automatically when lighting changes",
        "",
    );
    value_text(&right, "Off");
    let automatic = gtk::Switch::new();
    automatic.set_sensitive(false);
    automatic.set_valign(gtk::Align::Center);
    theme::class(&automatic, "win-switch");
    right.pack_start(&automatic, false, false, 0);

    let mode = brightness_mode();
    let (_, right) = row_full(
        &brightness_body,
        Some(""),
        "Change brightness based on content",
        "Help save power by optimizing screen contrast and brightness for the content shown",
        Some("Learn more"),
    );
    right.pack_start(&brightness_mode_dropdown(&mode), false, false, 0);

    let (_night_card, night_body) = card_spaced(&page);
    let (_, right) = row(
        &night_body,
        Some("brightness"),
        "Night light",
        "Use warmer colors to help block blue light",
    );
    value_text(&right, "Off");
    let night = gtk::Switch::new();
    night.set_sensitive(false);
    night.set_valign(gtk::Align::Center);
    theme::class(&night, "win-switch");
    right.pack_start(&night, false, false, 0);
    chevron_right(&right);

    let (_profile_card, profile_body) = card_spaced(&page);
    let (_, right) = row(
        &profile_body,
        Some("fluent:brush"),
        "Color profile",
        "Determine how colors appear on your screen",
    );
    value_text(&right, "sRGB");
    chevron_right(&right);

    let (_hdr_card, hdr_body) = card_spaced(&page);
    let (_, right) = row_full(&hdr_body, Some("hdr"), "HDR", "", Some("More about HDR"));
    chevron_right(&right);

    section(&page, "Scale & layout");
    let (_layout_card, layout_body) = card(&page);
    let output = sway_output();

    let (_, right) = row(
        &layout_body,
        Some("monitor"),
        "Scale",
        "Change the size of text, apps, and other items",
    );
    right.pack_start(&scale_dropdown(&output), false, false, 0);

    let (_, right) = row(
        &layout_body,
        Some("monitor"),
        "Display resolution",
        "Adjust to your connected display",
    );
    right.pack_start(&resolution_dropdown(&output), false, false, 0);

    let (_, right) = row(
        &layout_body,
        Some("monitor"),
        "Display orientation",
        "Rotation of the output",
    );
    right.pack_start(&orientation_dropdown(&output), false, false, 0);

    note(
        &page,
        "The brightness slider is live on hardware with a backlight. This virtual machine has none, so the control is shown at 100%.",
    );
    page
}

pub(crate) fn brightness_mode() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/desktop".to_string());
    std::fs::read_to_string(format!("{home}/.config/tiaki/brightness-content"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Off".to_string())
}

pub(crate) fn brightness_mode_dropdown(current: &str) -> gtk::MenuButton {
    let mut options: Vec<(String, Rc<dyn Fn()>)> = Vec::new();
    for choice in ["Off", "On battery only", "Always"] {
        let label = choice.to_string();
        let value = choice.to_string();
        let action: Rc<dyn Fn()> = Rc::new(move || {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/home/desktop".to_string());
            let dir = format!("{home}/.config/tiaki");
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(format!("{dir}/brightness-content"), &value);
        });
        options.push((label, action));
    }
    dropdown(current, options)
}

pub(crate) fn scale_dropdown(output: &SwayOutput) -> gtk::MenuButton {
    let current = format!("{}%", (output.scale * 100.0).round() as u32);
    let mut options: Vec<(String, Rc<dyn Fn()>)> = Vec::new();
    for value in [1.0, 1.25, 1.5, 2.0] {
        let label = format!("{}%", (value * 100.0) as u32);
        let name = output.name.clone();
        let action: Rc<dyn Fn()> = Rc::new(move || {
            system::spawn(&format!("swaymsg output {name} scale {value}"));
        });
        options.push((label, action));
    }
    dropdown(&current, options)
}

pub(crate) fn resolution_dropdown(output: &SwayOutput) -> gtk::MenuButton {
    let current = format!("{} × {}", output.width, output.height);
    let mut options: Vec<(String, Rc<dyn Fn()>)> = Vec::new();
    for mode in output.modes.iter().take(8) {
        let label = mode.replace('x', " × ");
        let name = output.name.clone();
        let mode = mode.clone();
        let action: Rc<dyn Fn()> = Rc::new(move || {
            system::spawn(&format!("swaymsg output {name} resolution {mode}"));
        });
        options.push((label, action));
    }
    dropdown(&current, options)
}

pub(crate) fn orientation_dropdown(output: &SwayOutput) -> gtk::MenuButton {
    let name = output.name.clone();
    let landscape: Rc<dyn Fn()> = Rc::new(move || {
        system::spawn(&format!("swaymsg output {name} transform normal"));
    });
    let name = output.name.clone();
    let portrait: Rc<dyn Fn()> = Rc::new(move || {
        system::spawn(&format!("swaymsg output {name} transform 90"));
    });
    dropdown(
        &output.orientation,
        vec![
            ("Landscape".to_string(), landscape),
            ("Portrait".to_string(), portrait),
        ],
    )
}

// ------------------------------------------------------------------ sound
