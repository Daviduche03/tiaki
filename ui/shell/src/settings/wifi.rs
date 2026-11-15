//! Wi-Fi: what the wireless adapter reports, or a plain statement that there
//! is none. Nothing on this page is a placeholder.
use std::process::Command;

use super::{data::*, widgets::*};
use crate::system;
use gtk::prelude::*;

pub(crate) fn wifi_page() -> gtk::Box {
    let page = page_shell();

    let interfaces = network_interfaces();
    let Some(adapter) = interfaces.iter().find(|interface| interface.wireless) else {
        section(&page, "Wi-Fi");
        let (_card, body) = card(&page);
        row(
            &body,
            Some("fluent:wifi"),
            "No Wi-Fi adapter",
            "This machine has no wireless hardware. Wired connections are under Network.",
        );
        return page;
    };

    section(&page, "Adapter");
    let (_card, body) = card(&page);
    let (_, right) = row(&body, Some("fluent:wifi"), &adapter.name, &adapter.state);
    value_text(&right, &adapter.address);

    section(&page, "Networks");
    let (_card, body) = card(&page);
    let networks = visible_networks();
    if networks.is_empty() {
        row(
            &body,
            Some("fluent:wifi"),
            "No networks found",
            if command_exists("nmcli") {
                "Nothing in range, or the adapter is switched off."
            } else {
                "NetworkManager is not installed, so networks cannot be listed."
            },
        );
    }
    for network in networks {
        let detail = format!("{} · signal {}%", network.security, network.signal);
        let (_, right) = row(&body, Some("fluent:wifi"), &network.ssid, &detail);
        if network.in_use {
            value_text(&right, "Connected");
        } else {
            let button = pill_button("Connect");
            let ssid = network.ssid.clone();
            button.connect_clicked(move |_| {
                // Open and saved networks join directly; a new secured
                // network prompts for its key in the terminal.
                system::spawn_app(
                    &format!(
                        "foot -c /etc/tiaki/foot.ini -e nmcli --ask dev wifi connect '{}'",
                        ssid.replace('\'', "")
                    ),
                    "foreground",
                );
            });
            right.pack_start(&button, false, false, 0);
        }
    }

    page
}

struct Network {
    ssid: String,
    signal: String,
    security: String,
    in_use: bool,
}

fn visible_networks() -> Vec<Network> {
    let Ok(output) = Command::new("nmcli")
        .args(["-t", "-f", "IN-USE,SSID,SIGNAL,SECURITY", "dev", "wifi", "list"])
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split(':').collect();
            if fields.len() < 4 || fields[1].is_empty() {
                return None;
            }
            Some(Network {
                in_use: fields[0].trim() == "*",
                ssid: fields[1].to_string(),
                signal: fields[2].to_string(),
                security: if fields[3].is_empty() {
                    "Open".to_string()
                } else {
                    fields[3].to_string()
                },
            })
        })
        .collect()
}
