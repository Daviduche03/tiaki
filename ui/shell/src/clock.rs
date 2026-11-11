//! Clocks. The launcher uses the typographic clock, set in the system face.

use gtk::glib;
use gtk::prelude::*;

/// The launcher clock, set in the system face.
pub fn label_clock() -> gtk::Label {
    let label = gtk::Label::new(Some(&time_text()));
    label.set_halign(gtk::Align::Center);
    crate::theme::class(&label, "home-clock");
    let label_clone = label.clone();
    glib::timeout_add_seconds_local(1, move || {
        label_clone.set_text(&time_text());
        glib::ControlFlow::Continue
    });
    label
}

fn time_text() -> String {
    match glib::DateTime::now_local() {
        Ok(now) => format!("{:02}:{:02}", now.hour(), now.minute()),
        Err(_) => "--:--".to_string(),
    }
}
