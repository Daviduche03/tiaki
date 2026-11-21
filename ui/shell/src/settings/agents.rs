use super::widgets::*;
use crate::system;
use gtk::prelude::*;
use std::rc::Rc;

pub(crate) fn agents_page() -> gtk::Box {
    let page = page_shell();

    // 1. Autonomy & Governance
    section(&page, "Autonomy");
    let (_gov_card, gov_body) = card(&page);

    let (_, mode_right) = row(
        &gov_body,
        Some("fluent:agents"),
        "Execution Mode",
        "Guarded requires confirmation before file writes; Full executes autonomously",
    );
    let mode_drop = dropdown(
        "Guarded",
        vec![
            (
                "Guarded (Recommended)".to_string(),
                Rc::new(|| {
                    system::spawn("agentd --mode guarded check state.mode guarded");
                }),
            ),
            (
                "Full Autonomy".to_string(),
                Rc::new(|| {
                    system::spawn("agentd --mode full check state.mode full");
                }),
            ),
        ],
    );
    mode_right.pack_start(&mode_drop, false, false, 0);

    // 2. Ledger
    section(&page, "Ledger");
    let (_ledger_card, ledger_body) = card(&page);

    let (_, log_right) = row(
        &ledger_body,
        Some("fluent:clock"),
        "Transactions",
        "Every state change agents and you have made, in order",
    );
    let log_btn = pill_button("Open log");
    log_btn.connect_clicked(|_| system::run_action("transactions.open"));
    log_right.pack_start(&log_btn, false, false, 0);

    let (_, verify_right) = row(
        &ledger_body,
        Some("fluent:shield"),
        "Integrity",
        "Check the whole history against its root hash",
    );
    let verify_btn = pill_button("Verify");
    verify_btn.connect_clicked(|_| in_terminal("agentd verify"));
    verify_right.pack_start(&verify_btn, false, false, 0);

    let (_, rollback_right) = row(
        &ledger_body,
        Some("fluent:update"),
        "Generations",
        "Snapshots you can roll the system back to",
    );
    let roll_btn = pill_button("View");
    roll_btn.connect_clicked(|_| in_terminal("agentd generation list"));
    rollback_right.pack_start(&roll_btn, false, false, 0);

    page
}

/// Run an agentd query where its output can be read: a terminal that stays
/// open until a key is pressed.
fn in_terminal(command: &str) {
    system::spawn_app(
        &format!("foot -c /etc/tiaki/foot.ini -e sh -c '/usr/local/bin/{command}; read _'"),
        "foreground",
    );
}
