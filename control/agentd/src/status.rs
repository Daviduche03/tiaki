//! agentd command: Read-only reports: log, state, grants, impacts.
use crate::Ledger;
use agentic_core_log::{format_timestamp, Impact, Status};
use agentic_transaction::{CheckReport, Severity, StateStore};

pub(crate) fn print_report(report: &CheckReport) {
    let verdict = if report.ok {
        "preflight passed"
    } else {
        "preflight failed"
    };
    println!("{verdict}");
    println!("  {}", report.summary.one_line);
    println!("  impact: {}", describe_impact(&report.impact));

    let authorization = &report.authorization;
    if !authorization.allowed {
        println!("  authorization: denied ({})", authorization.reason);
    } else if authorization.requires_approval {
        println!(
            "  authorization: approval required, sandbox {} ({})",
            authorization.sandbox.as_str(),
            authorization.reason
        );
    } else {
        println!(
            "  authorization: allowed, sandbox {}",
            authorization.sandbox.as_str()
        );
    }

    for issue in &report.issues {
        let severity = match issue.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        let location = issue
            .path
            .as_deref()
            .map(|path| format!(" ({path})"))
            .unwrap_or_default();
        println!("  {severity}[{}]{location}: {}", issue.code, issue.message);
    }

    if let Some(health) = &report.dry_run_health {
        if health.ok {
            println!("  dry run: health check passed");
        } else {
            println!("  dry run: health check failed: {}", health.reason);
        }
    }
}

pub(crate) fn print_log(engine: &Ledger) {
    if engine.entries().is_empty() {
        println!("no entries");
        return;
    }
    for entry in engine.entries() {
        println!(
            "#{:03}  {}  {:<16}  {:<34}  {:<7}  {:<12}  {:<18}  {}",
            entry.seq,
            format_timestamp(entry.timestamp),
            status_label(entry.status),
            entry.principal,
            entry.mode,
            entry.sandbox,
            entry.transaction_id,
            entry.summary.one_line
        );
    }
}

pub(crate) fn print_state(engine: &Ledger) {
    if engine.store().values().is_empty() {
        println!("state is empty");
        return;
    }
    for (key, value) in engine.store().values() {
        println!("{key} = {value}");
    }
}

pub(crate) fn print_grants(engine: &Ledger) {
    if engine.authority().grants().is_empty() {
        println!("no grants");
        return;
    }
    for grant in engine.authority().grants() {
        println!(
            "{}  {}  {}  {}{}granted by {}",
            grant.principal,
            grant.action.as_str(),
            grant.resource.raw,
            if grant.requires_approval {
                "approval "
            } else {
                ""
            },
            grant
                .max_per_minute
                .map(|limit| format!("max {limit}/min "))
                .unwrap_or_default(),
            grant.granted_by
        );
    }
}

pub(crate) fn describe_impact(impact: &Impact) -> String {
    let mut parts = Vec::new();
    if impact.reboot_required {
        parts.push("reboot required".to_string());
    }
    if impact.services_restarted > 0 {
        parts.push(format!(
            "{} service(s) restarted",
            impact.services_restarted
        ));
    }
    if impact.ui_changed {
        parts.push("visible change".to_string());
    }
    if parts.is_empty() {
        "no visible change".to_string()
    } else {
        parts.join(", ")
    }
}

pub(crate) fn status_label(status: Status) -> &'static str {
    match status {
        Status::Committed => "committed",
        Status::Reverted => "reverted",
        Status::Failed => "failed",
        Status::Rejected => "rejected",
        Status::PendingApproval => "pending-approval",
    }
}
