use super::{data::*, widgets::*};
use gtk::prelude::*;

pub(crate) fn bluetooth_page() -> gtk::Box {
    let page = page_shell();

    // 1. Bluetooth Master Switch
    section(&page, "Bluetooth");
    let (_bt_card, bt_body) = card(&page);
    let present = bluetooth_present();
    let (_, bt_right) = row(
        &bt_body,
        Some("fluent:bluetooth"),
        "Bluetooth",
        if present {
            "Visible as 'Tiaki-Workstation' to nearby devices"
        } else {
            "Bluetooth radio adapter enabled"
        },
    );
    let bt_switch = toggle_switch(true, |_| {});
    bt_right.pack_start(&bt_switch, false, false, 0);

    // 2. Paired & Connected Peripherals
    section(&page, "Paired Devices");
    let (_paired_card, paired_body) = card(&page);

    let paired = [
        ("Magic Keyboard", "Connected · 88% battery · Bluetooth LE", "Disconnect"),
        ("MX Master 3S", "Connected · 74% battery · Wireless receiver", "Disconnect"),
        ("WH-1000XM5 Headphones", "Paired · High Definition Audio (LDAC)", "Connect"),
    ];

    for (name, detail, btn_text) in paired {
        let (_, right) = row(
            &paired_body,
            Some("fluent:bluetooth"),
            name,
            detail,
        );
        let btn = pill_button(btn_text);
        right.pack_start(&btn, false, false, 0);
    }

    // 3. Available Devices
    section(&page, "Available Devices");
    let (_avail_card, avail_body) = card(&page);
    let avail = [
        ("AirPods Pro (2nd gen)", "Nearby · Ready to pair"),
        ("DualSense Wireless Controller", "Nearby · Ready to pair"),
    ];
    for (name, detail) in avail {
        let (_, right) = row(
            &avail_body,
            Some("fluent:bluetooth"),
            name,
            detail,
        );
        let btn = pill_button("Pair");
        right.pack_start(&btn, false, false, 0);
    }

    // 4. USB & Connected Input Hardware
    section(&page, "USB & Input Hardware");
    let (_hw_card, hw_body) = card(&page);
    let usb = usb_devices();
    if usb.is_empty() {
        row(
            &hw_body,
            Some("fluent:usb"),
            "USB Host Controller",
            "Root hub active (USB 3.1 xHCI)",
        );
    } else {
        for dev in usb.iter().take(5) {
            let (_, right) = row(
                &hw_body,
                Some("fluent:usb"),
                dev,
                "Connected via USB bus",
            );
            value_text(&right, "Connected");
        }
    }

    let inputs = input_devices();
    for dev in inputs.iter().take(4) {
        let (_, right) = row(
            &hw_body,
            Some("fluent:keyboard"),
            dev,
            "Input device registered with evdev",
        );
        value_text(&right, "Ready");
    }

    let (_, cli_right) = row(
        &hw_body,
        Some("fluent:keyboard"),
        "Command Line Bluetooth",
        "Terminal control via standard Linux Bluetooth utility",
    );
    value_text(&cli_right, "bluetoothctl [scan|devices|connect]");

    page
}
