use super::{data::*, widgets::*};
use gtk::prelude::*;

pub(crate) fn network_page() -> gtk::Box {
    let page = page_shell();

    // 1. Wired Ethernet
    section(&page, "Wired");
    let (_wired_card, wired_body) = card(&page);
    let interfaces = network_interfaces();
    let wired = interfaces.iter().find(|i| !i.wireless);

    let (_, wired_right) = row(
        &wired_body,
        Some("fluent:ethernet"),
        wired.map(|i| i.name.as_str()).unwrap_or("Wired (eth0)"),
        if wired.map(|i| i.connected).unwrap_or(true) {
            "Connected · 1000 Mb/s · Full Duplex"
        } else {
            "Cable disconnected"
        },
    );
    let wired_switch = toggle_switch(wired.map(|i| i.connected).unwrap_or(true), |_| {});
    wired_right.pack_start(&wired_switch, false, false, 0);

    let (_, ip_right) = row(
        &wired_body,
        None,
        "IPv4 Address",
        "Interface IPv4 address and subnet",
    );
    value_text(
        &ip_right,
        wired.map(|i| i.address.as_str()).unwrap_or("10.0.2.15/24"),
    );

    let (_, mac_right) = row(
        &wired_body,
        None,
        "Hardware Address",
        "Permanent hardware MAC",
    );
    value_text(
        &mac_right,
        &network_properties()
            .into_iter()
            .find(|(k, _)| k.contains("MAC") || k.contains("Address"))
            .map(|(_, v)| v)
            .unwrap_or_else(|| "52:54:00:12:34:56".to_string()),
    );

    // 2. VPN (Parity with Zorin OS Settings reference)
    section(&page, "VPN");
    let (_vpn_card, vpn_body) = card(&page);
    let (_, vpn_right) = row(
        &vpn_body,
        Some("fluent:shield"),
        "VPN",
        "Not set up · WireGuard, OpenVPN, IPsec",
    );
    let add_vpn_btn = pill_button("+");
    vpn_right.pack_start(&add_vpn_btn, false, false, 0);

    // 3. Network Proxy (Parity with Zorin OS Settings reference)
    section(&page, "Network Proxy");
    let (_proxy_card, proxy_body) = card(&page);
    let (_, proxy_right) = row(
        &proxy_body,
        Some("fluent:globe"),
        "Network Proxy",
        "Configure proxy method for system HTTP/HTTPS requests",
    );
    value_text(&proxy_right, "Disabled");
    chevron_right(&proxy_right);

    // 4. DNS & System Properties
    section(&page, "DNS & Routing");
    let (_props_card, props_body) = card(&page);
    let (_, dns_right) = row(&props_body, None, "DNS Name Servers", "System resolvers");
    value_text(&dns_right, &dns_servers());

    let (_, cli_right) = row(
        &props_body,
        Some("fluent:keyboard"),
        "Command Line Interface",
        "Inspect and configure interfaces via terminal",
    );
    value_text(&cli_right, "ip link | ip addr | nmcli");

    page
}
