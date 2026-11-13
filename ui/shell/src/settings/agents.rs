use super::widgets::*;
use crate::system;
use gtk::prelude::*;
use std::rc::Rc;

pub(crate) fn agents_page() -> gtk::Box {
    let page = page_shell();

    // 1. Autonomy & Governance
    section(&page, "Autonomy & Governance");
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

    let (_, approval_right) = row(
        &gov_body,
        Some("fluent:shield"),
        "Cross-Boundary Approval Gate",
        "Prompt user before any tool mutates files outside /var/lib/tiaki/workspace",
    );
    let approval_switch = toggle_switch(true, |_| {});
    approval_right.pack_start(&approval_switch, false, false, 0);

    let (_, rate_right) = row(
        &gov_body,
        Some("fluent:clock"),
        "Action Rate Limit",
        "Maximum autonomous tool executions permitted per minute (anti-runaway guard)",
    );
    value_text(&rate_right, "30 actions / min");

    // 2. Inference & Model Registry
    section(&page, "Inference & Model Registry");
    let (_model_card, model_body) = card(&page);

    let (_, model_right) = row(
        &model_body,
        Some("fluent:spark"),
        "Active Model Provider",
        "Neural weights and backend used for reasoning and tool generation",
    );
    let model_drop = dropdown(
        "llama-3.2-3b-instruct (Local Fast)",
        vec![
            (
                "llama-3.2-3b-instruct (Local Fast)".to_string(),
                Rc::new(|| {}),
            ),
            (
                "qwen-2.5-coder-7b (Code Specialist)".to_string(),
                Rc::new(|| {}),
            ),
            (
                "claude-3-5-sonnet (Cloud Remote)".to_string(),
                Rc::new(|| {}),
            ),
            (
                "local-simulated (Embedded Dev)".to_string(),
                Rc::new(|| {}),
            ),
        ],
    );
    model_right.pack_start(&model_drop, false, false, 0);

    let (_, ctx_right) = row(
        &model_body,
        None,
        "Context Window Limit",
        "Maximum attention context before rolling KV cache compression",
    );
    let ctx_drop = dropdown(
        "32,768 tokens",
        vec![
            ("8,192 tokens".to_string(), Rc::new(|| {})),
            ("32,768 tokens".to_string(), Rc::new(|| {})),
            ("131,072 tokens".to_string(), Rc::new(|| {})),
        ],
    );
    ctx_right.pack_start(&ctx_drop, false, false, 0);

    let (_, kv_right) = row(
        &model_body,
        None,
        "KV Cache Preemption",
        "Evict stale key-value attention tensors under system RAM pressure",
    );
    let kv_switch = toggle_switch(true, |_| {});
    kv_right.pack_start(&kv_switch, false, false, 0);

    let (_, cli_model_right) = row(
        &model_body,
        Some("fluent:keyboard"),
        "CLI Model Query",
        "Direct command-line interface for manual model queries and benchmarks",
    );
    value_text(&cli_model_right, "agentd inference ask --model <name>");

    // 3. Sandbox & Capability Boundaries
    section(&page, "Sandbox & Capabilities");
    let (_sandbox_card, sandbox_body) = card(&page);

    let (_, ws_right) = row(
        &sandbox_body,
        Some("fluent:drive"),
        "Workspace Boundary",
        "Strict filesystem boundary for all agentic read and write operations",
    );
    value_text(&ws_right, "/var/lib/tiaki/workspace");

    let (_, net_right) = row(
        &sandbox_body,
        Some("fluent:globe"),
        "Outbound Network Access",
        "Permit sandboxed agents to resolve DNS and issue HTTP/HTTPS requests",
    );
    let net_switch = toggle_switch(true, |_| {});
    net_right.pack_start(&net_switch, false, false, 0);

    let (_, runtime_right) = row(
        &sandbox_body,
        Some("fluent:gear"),
        "Isolation Engine",
        "Sandboxing primitives enforced at system call level",
    );
    value_text(&runtime_right, "bubblewrap + seccomp (ABI Level 2)");

    let (_, grants_right) = row(
        &sandbox_body,
        Some("fluent:shield"),
        "Granted Capabilities",
        "Active permissions granted to default user:local principal",
    );
    value_text(&grants_right, "fs:workspace, net:http, proc:spawn");

    // 4. Cryptographic Ledger & Audit
    section(&page, "Cryptographic Ledger & Audit");
    let (_ledger_card, ledger_body) = card(&page);

    let (_, tx_right) = row(
        &ledger_body,
        Some("fluent:clock"),
        "Immutable State Ledger",
        "Append-only SHA-256 state journal recording all system actions",
    );
    value_text(&tx_right, "142 transactions committed");

    let (_, verify_right) = row(
        &ledger_body,
        Some("fluent:shield"),
        "Chain Integrity",
        "Cryptographic proof validating entire commit history against root hash",
    );
    let verify_btn = pill_button("Verify Now");
    verify_btn.connect_clicked(|_| {
        system::spawn("agentd verify");
    });
    verify_right.pack_start(&verify_btn, false, false, 0);
    value_text(&verify_right, "Valid (0 errors)");

    let (_, rollback_right) = row(
        &ledger_body,
        Some("fluent:update"),
        "System Rollback Generations",
        "Instant zero-loss recovery snapshots generated before major actions",
    );
    let roll_btn = pill_button("View Generations");
    roll_btn.connect_clicked(|_| {
        system::spawn("agentd generation list");
    });
    rollback_right.pack_start(&roll_btn, false, false, 0);

    let (_, cli_audit_right) = row(
        &ledger_body,
        Some("fluent:keyboard"),
        "CLI Audit Commands",
        "Command line commands for manual ledger verification and history",
    );
    value_text(&cli_audit_right, "agentd verify | agentd log | agentd revert");

    page
}
