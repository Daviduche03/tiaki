use super::{data::*, widgets::*};
use gtk::prelude::*;

pub(crate) fn wifi_page() -> gtk::Box {
    let page = page_shell();

    // 1. Master Wi-Fi Switch
    section(&page, "Wi-Fi");
    let (_master_card, master_body) = card(&page);
    let (_, master_right) = row(
        &master_body,
        Some("fluent:wifi"),
        "Wi-Fi",
        "Enable or disable wireless network connectivity",
    );
    let master_switch = toggle_switch(true, |_active| {});
    master_right.pack_start(&master_switch, false, false, 0);

    // 2. Connected Network
    section(&page, "Connected Network");
    let (_connected_card, connected_body) = card(&page);
    let interfaces = network_interfaces();
    let wifi_iface = interfaces.iter().find(|i| i.wireless);

    let (ssid_name, ip_addr) = if let Some(iface) = wifi_iface {
        (iface.name.as_str(), iface.address.as_str())
    } else {
        ("Tiaki-Mesh-5G", "192.168.1.142")
    };

    let (_, conn_right) = row(
        &connected_body,
        Some("fluent:wifi"),
        ssid_name,
        "Connected · WPA3-Personal · 5 GHz (866 Mb/s) · Signal: 96% (-42 dBm)",
    );
    let disconnect_btn = pill_button("Disconnect");
    conn_right.pack_start(&disconnect_btn, false, false, 0);

    let (_, ip_right) = row(
        &connected_body,
        None,
        "IPv4 Address",
        "Assigned by wireless gateway DHCP",
    );
    value_text(&ip_right, ip_addr);

    let (_, mac_right) = row(
        &connected_body,
        None,
        "Hardware MAC Address",
        "Physical wireless adapter address",
    );
    value_text(&mac_right, "e4:5f:01:8b:32:19");

    // 3. Visible Networks
    section(&page, "Visible Networks");
    let (_networks_card, networks_body) = card(&page);

    let available = [
        ("Nordic-Office-Fast", "WPA2/WPA3 · 5 GHz · Signal: 88%", "Connect"),
        ("Studio-Guest", "Open (Captive Portal) · 2.4 GHz · Signal: 74%", "Connect"),
        ("Lab-Secure-6E", "WPA3-Enterprise 802.1X · 6 GHz · Signal: 62%", "Connect"),
        ("IoT-SmartHome-Net", "WPA2-Personal · 2.4 GHz · Signal: 45%", "Connect"),
    ];

    for (ssid, detail, btn_text) in available {
        let (_, right) = row(
            &networks_body,
            Some("fluent:wifi"),
            ssid,
            detail,
        );
        let btn = pill_button(btn_text);
        right.pack_start(&btn, false, false, 0);
    }

    // 4. Hotspot & Preferences
    section(&page, "Hotspot & Preferences");
    let (_hotspot_card, hotspot_body) = card(&page);

    let (_, hs_right) = row(
        &hotspot_body,
        Some("fluent:network"),
        "Wi-Fi Hotspot",
        "Share machine internet with phones, tablets and other computers",
    );
    let hs_switch = toggle_switch(false, |_| {});
    hs_right.pack_start(&hs_switch, false, false, 0);

    let (_, saved_right) = row(
        &hotspot_body,
        Some("fluent:gear"),
        "Saved Networks",
        "Manage known network credentials and auto-reconnect",
    );
    chevron_right(&saved_right);

    let (_, cli_right) = row(
        &hotspot_body,
        Some("fluent:keyboard"),
        "Command Line Management",
        "CLI equivalents for headless or terminal control",
    );
    value_text(&cli_right, "nmcli dev wifi [list|connect]");

    page
}
