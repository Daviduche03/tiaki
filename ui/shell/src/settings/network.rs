use super::{data::*, widgets::*};

pub(crate) fn network_page() -> gtk::Box {
    let page = page_shell();

    section(&page, "Network");
    let (_network_card, network_body) = card(&page);
    let interfaces = network_interfaces();
    if interfaces.is_empty() {
        row(
            &network_body,
            Some("fluent:network"),
            "No network interfaces",
            "No link was found in /sys/class/net.",
        );
    } else {
        for interface in interfaces.iter().take(4) {
            let (_, right) = row(
                &network_body,
                Some(if interface.wireless {
                    "fluent:wifi"
                } else {
                    "fluent:ethernet"
                }),
                &interface.name,
                &format!("{} · {}", interface.state, interface.address),
            );
            value_text(
                &right,
                if interface.connected {
                    "Connected"
                } else {
                    "Disconnected"
                },
            );
        }
    }

    section(&page, "Properties");
    let (_properties_card, properties_body) = card(&page);
    for (key, value) in network_properties() {
        let (_, right) = row(&properties_body, None, &key, "");
        value_text(&right, &value);
    }

    page
}

// ------------------------------------------------------- personalization
