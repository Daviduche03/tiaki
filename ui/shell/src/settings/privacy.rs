use super::widgets::*;
use crate::system;
use gtk::prelude::*;

pub(crate) fn privacy_page() -> gtk::Box {
    let page = page_shell();
    section(&page, "Security");
    let (_security_card, security_body) = card(&page);
    for (title, subtitle, value) in [
        ("Guarded mode", "Privileged actions wait for approval", "On"),
        ("Sandbox", "Every app runs in a sandbox", "bubblewrap"),
        (
            "Transaction ledger",
            "Every change is recorded and reversible",
            "On",
        ),
        (
            "Trust boundary",
            "The authority layer, not the shell",
            "agentd",
        ),
    ] {
        let (_, right) = row(&security_body, Some("privacy"), title, subtitle);
        value_text(&right, value);
    }

    section(&page, "Permissions");
    let (_permissions_card, permissions_body) = card(&page);
    for (title, subtitle, value) in [
        (
            "Inference",
            "Model calls are a first-class OS service",
            "Local runtime",
        ),
        ("Telemetry", "Nothing leaves this machine", "None"),
    ] {
        let (_, right) = row(&permissions_body, Some("spark"), title, subtitle);
        value_text(&right, value);
    }

    let review = accent_button("Review transactions");
    review.set_halign(gtk::Align::Start);
    review.set_margin_top(16);
    review.connect_clicked(|_| {
        system::spawn("foot -c /etc/tiaki/foot.ini -e /usr/local/bin/agentd log")
    });
    page.pack_start(&review, false, false, 0);
    page
}

// ----------------------------------------------------------------- update
