//! Network: every interface the kernel lists, with its real state, address,
//! link speed and hardware address, then the resolvers in use.
use super::{data::*, widgets::*};

pub(crate) fn network_page() -> gtk::Box {
    let page = page_shell();

    section(&page, "Interfaces");
    let interfaces = network_interfaces();
    if interfaces.is_empty() {
        let (_card, body) = card(&page);
        row(
            &body,
            Some("fluent:ethernet"),
            "No network interfaces",
            "The kernel reports no network hardware.",
        );
    }
    for interface in &interfaces {
        let (_card, body) = card_spaced(&page);
        let icon = if interface.wireless {
            "fluent:wifi"
        } else {
            "fluent:ethernet"
        };
        let sys = std::path::PathBuf::from("/sys/class/net").join(interface.name.to_lowercase());
        let speed = read_trim(sys.join("speed"))
            .and_then(|value| value.parse::<i64>().ok())
            .filter(|value| *value > 0)
            .map(|value| format!(" · {value} Mb/s"))
            .unwrap_or_default();
        let (_, right) = row(
            &body,
            Some(icon),
            &interface.name,
            &format!("{}{speed}", interface.state),
        );
        value_text(&right, &interface.address);

        // Without an IPv4 address the row above already shows the MAC.
        if let Some(mac) = read_trim(sys.join("address")).filter(|mac| *mac != interface.address) {
            let (_, right) = row(&body, None, "Hardware address", "");
            value_text(&right, &mac);
        }
    }

    section(&page, "DNS");
    let (_card, body) = card(&page);
    let (_, right) = row(&body, None, "Name servers", "From /etc/resolv.conf");
    value_text(&right, &dns_servers());

    let (_, right) = row(&body, None, "Hostname", "");
    value_text(&right, &hostname());

    page
}
