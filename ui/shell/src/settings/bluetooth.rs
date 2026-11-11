use super::{data::*, widgets::*};

pub(crate) fn bluetooth_page() -> gtk::Box {
    let page = page_shell();

    section(&page, "Devices");
    let (_devices_card, devices_body) = card(&page);
    let devices = usb_devices();
    if devices.is_empty() {
        row(
            &devices_body,
            Some("fluent:usb"),
            "No devices",
            "Nothing is connected right now.",
        );
    } else {
        for name in devices.iter().take(6) {
            let (_, right) = row(&devices_body, Some("fluent:usb"), name, "Connected via USB");
            value_text(&right, "Connected");
        }
    }

    section(&page, "Bluetooth");
    let (_bluetooth_card, bluetooth_body) = card(&page);
    let present = bluetooth_present();
    let (_, right) = row(
        &bluetooth_body,
        Some("fluent:bluetooth"),
        "Bluetooth",
        if present {
            "This machine has a Bluetooth adapter"
        } else {
            "No Bluetooth adapter detected"
        },
    );
    value_text(&right, if present { "On" } else { "Unavailable" });

    section(&page, "Mouse, keyboard & pen");
    let (_input_card, input_body) = card(&page);
    let inputs = input_devices();
    if inputs.is_empty() {
        row(
            &input_body,
            Some("fluent:keyboard"),
            "No input devices",
            "No keyboards or pointing devices were found.",
        );
    } else {
        for name in inputs.iter().take(8) {
            let (_, right) = row(&input_body, Some("fluent:keyboard"), name, "");
            value_text(&right, "Ready");
        }
    }

    page
}

// ----------------------------------------------------- network & internet
