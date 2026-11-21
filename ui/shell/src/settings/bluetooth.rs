//! Bluetooth and attached devices, read from the system. No sample devices.
use std::process::Command;

use super::{data::*, widgets::*};

pub(crate) fn bluetooth_page() -> gtk::Box {
    let page = page_shell();

    section(&page, "Bluetooth");
    let (_card, body) = card(&page);
    if !bluetooth_present() {
        row(
            &body,
            Some("fluent:bluetooth"),
            "No Bluetooth adapter",
            "This machine has no Bluetooth radio.",
        );
    } else {
        let devices = paired_devices();
        if devices.is_empty() {
            row(
                &body,
                Some("fluent:bluetooth"),
                "No paired devices",
                "Pair a device with bluetoothctl; it will appear here.",
            );
        }
        for (name, address) in devices {
            let (_, right) = row(&body, Some("fluent:bluetooth"), &name, "Paired");
            value_text(&right, &address);
        }
    }

    let usb = usb_devices();
    if !usb.is_empty() {
        section(&page, "USB");
        let (_card, body) = card(&page);
        for device in usb.iter().take(8) {
            row(&body, Some("fluent:usb"), device, "Connected");
        }
    }

    let inputs = input_devices();
    if !inputs.is_empty() {
        section(&page, "Input devices");
        let (_card, body) = card(&page);
        for device in inputs.iter().take(8) {
            row(
                &body,
                Some("fluent:keyboard"),
                device,
                "Registered with the kernel",
            );
        }
    }

    page
}

fn paired_devices() -> Vec<(String, String)> {
    let Ok(output) = Command::new("bluetoothctl")
        .args(["devices", "Paired"])
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("Device ")?;
            let (address, name) = rest.split_once(' ')?;
            Some((name.to_string(), address.to_string()))
        })
        .collect()
}
