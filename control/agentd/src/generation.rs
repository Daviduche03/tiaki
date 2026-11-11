//! agentd command: Boot generations: list, status, update.
use crate::session::table_path;
use crate::{now_nanos, now_secs, Global, Ledger};
use agentic_capability::ActionClass;
use agentic_core_log::{Impact, Status, Summary};
use agentic_core_rollback::{BootDecision, GenerationTable};
use agentic_transaction::SandboxClass;
use std::path::Path;

pub(crate) fn generation_list(home: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let table = GenerationTable::load(table_path(home))?;
    if table.generations.is_empty() {
        println!("no generations staged");
        return Ok(());
    }
    println!(
        "{:<10} {:<12} {:<9} {:<8} {:<16} image",
        "id", "label", "status", "attempts", "digest"
    );
    for generation in table.history() {
        let digest: String = generation.digest.chars().take(15).collect();
        println!(
            "{:<10} {:<12} {:<9} {:<8} {:<16} {}",
            generation.id,
            generation.label,
            generation.status.as_str(),
            generation.attempts,
            digest,
            generation.image
        );
    }
    Ok(())
}

pub(crate) fn generation_status(home: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let table = GenerationTable::load(table_path(home))?;
    println!("active:    {}", table.active.as_deref().unwrap_or("none"));
    println!(
        "last-good: {}",
        table.last_good.as_deref().unwrap_or("none")
    );
    println!(
        "candidate: {}",
        table.candidate.as_deref().unwrap_or("none")
    );
    println!("boot id:   {}", table.boot_id);
    println!("max boot attempts: {}", table.max_attempts);
    Ok(())
}

pub(crate) fn print_rollback(decision: &BootDecision) {
    match decision {
        BootDecision::Rollback { from, to, reason } => {
            println!("rolled back {from} -> {to}: {reason}");
        }
        BootDecision::Rescue { reason } => {
            println!("rescue required: {reason}");
        }
        BootDecision::Proceed { generation, .. } => {
            println!("proceed with {generation}");
        }
    }
}

pub(crate) fn authorize_generation(
    engine: &Ledger,
    global: &Global,
    resource: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let decision = engine.authority().authorize_action(
        &global.principal,
        global.mode,
        ActionClass::GenerationManage,
        resource,
        now_secs(),
    );
    if decision.is_clear() {
        Ok(())
    } else {
        Err(format!(
            "{} may not manage generations: {}",
            global.principal, decision.reason
        )
        .into())
    }
}

pub(crate) fn record_generation(
    engine: &mut Ledger,
    global: &Global,
    summary: String,
    status: Status,
) {
    let transaction_id = format!("generation_{:x}", now_nanos());
    let _ = engine.record(
        &global.principal,
        global.mode,
        SandboxClass::SystemWrite,
        &transaction_id,
        "Generation update",
        Summary::new(summary, String::new(), "rule-based", true),
        Impact::none(),
        status,
        vec!["generation.manage".into()],
    );
}

pub(crate) fn print_generation_usage() {
    println!("agentd generation <command>");
    println!();
    println!("  list");
    println!("  status");
    println!("  stage <dir> [--label <text>]");
    println!("  update <dir> [--fail] [--label <text>] [--downtime-ms N]");
    println!("  boot [--fail] [--downtime-ms N]");
    println!("  commit");
    println!("  rollback");
}
