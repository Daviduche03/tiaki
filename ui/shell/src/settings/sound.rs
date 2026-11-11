use super::{data::*, widgets::*};
use crate::{system, theme};
use gtk::prelude::*;

pub(crate) fn sound_page() -> gtk::Box {
    let page = page_shell();
    section(&page, "Output");
    let (_output_card, output_body) = card(&page);

    let (_, right) = row(
        &output_body,
        Some("audio"),
        "Volume",
        "Output volume over PipeWire",
    );
    match output_volume() {
        Some(level) => {
            slider(&right, level, |value| {
                system::spawn(&format!(
                    "wpctl set-volume @DEFAULT_AUDIO_SINK@ {value:.0}%"
                ));
            });
            value_text(&right, &format!("{level:.0}%"));
        }
        None => {
            let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
            scale.set_value(100.0);
            scale.set_draw_value(false);
            scale.set_size_request(300, -1);
            scale.set_sensitive(false);
            theme::class(&scale, "win-scale");
            right.pack_start(&scale, false, false, 0);
            value_text(&right, "Unavailable");
        }
    }

    let (_, right) = row(
        &output_body,
        Some("audio"),
        "Output device",
        "Where sound is played",
    );
    value_text(
        &right,
        if output_volume().is_some() {
            "Default output"
        } else {
            "No sink"
        },
    );

    let (_, right) = row(&output_body, Some("audio"), "Mute", "Silence the output");
    let mute = gtk::Switch::new();
    mute.set_sensitive(output_volume().is_some());
    mute.set_valign(gtk::Align::Center);
    theme::class(&mute, "win-switch");
    mute.connect_state_set(|_, active| {
        system::spawn(&format!(
            "wpctl set-mute @DEFAULT_AUDIO_SINK@ {}",
            if active { 1 } else { 0 }
        ));
        gtk::glib::Propagation::Proceed
    });
    right.pack_start(&mute, false, false, 0);

    if output_volume().is_none() {
        note(
            &page,
            "wpctl found no sink in this session. Volume returns when PipeWire has an output.",
        );
    }
    page
}

// ------------------------------------------------------------------ power
