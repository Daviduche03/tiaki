//! Tiaki control-plane daemon (Milestones 1–2 CLI).
//!
//! Commands:
//!   agentd check <path> <value> [--intent <text>]
//!   agentd apply <path> <value> [--intent <text>] [--dry-run]
//!   agentd revert <transaction-id>
//!   agentd log | state | verify
//!   agentd grants
//!   agentd grant <principal> <action> <resource> [--approval] [--max-per-minute N]
//!   agentd revoke <principal> <action> <resource>
//!   agentd sandbox <command> [args...] [--network]
//!   agentd plugin list
//!   agentd plugin install <dir> [--allow-unverified]
//!   agentd plugin update <dir> [--allow-unverified]
//!   agentd plugin remove <id>
//!   agentd plugin load|unload|start|stop <id>
//!   agentd plugin swap <id> <version>
//!   agentd plugin rollback <id>
//!   agentd plugin config <id> [key=value ...]
//!   agentd plugin spec <id>
//!
//! Global flags:
//!   --principal <id>   default user:local
//!   --mode <mode>      guarded | full (default guarded)
//!
//! State, log, snapshots, grants, and plugins live under `$TIAKI_HOME`
//! (default `.tiaki`).
use agentic_capability::{Policy, Principal};
use agentic_core_log::ActionLog;
use agentic_explain::RuleBasedSummarizer;
use agentic_inference::ModelRegistryValidator;
use agentic_transaction::{
    AccessMode, CompositeValidator, Engine, FileStore, SchemaHealth, SchemaValidator,
};
use agentic_ui_tree::{KeymapValidator, UiTreeValidator};
use std::env;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) type Ledger =
    Engine<FileStore, SchemaHealth, RuleBasedSummarizer, CompositeValidator, Policy>;

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Global {
    pub(crate) principal: String,
    pub(crate) mode: AccessMode,
    /// Whether `--mode` was given. Commands that default to full access (the
    /// MCP server) only honour guarded when the human asked for it.
    pub(crate) mode_explicit: bool,
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let raw: Vec<String> = env::args().skip(1).collect();
    let (global, args) = parse_global(&raw)?;

    let home = env::var("TIAKI_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(".tiaki"));
    std::fs::create_dir_all(&home)?;

    // Repair runs before the ledger opens: a corrupt chain fails `open`.
    if args.first().map(String::as_str) == Some("log") && args.iter().any(|arg| arg == "--repair") {
        let count = ActionLog::repair_file(home.join("log"))?;
        println!("repaired log: {count} entries resequenced");
        return Ok(());
    }

    let mut engine = open_ledger(&home)?;

    match args.first().map(String::as_str) {
        Some("check") => check(&engine, &global, &args)?,
        Some("apply") => apply(&mut engine, &global, &args)?,
        Some("revert") => revert(&mut engine, &global, &args)?,
        Some("log") => print_log(&engine),
        Some("state") => print_state(&engine),
        Some("verify") => {
            engine.log().verify()?;
            println!(
                "log verified: {} entries, head {}",
                engine.entries().len(),
                &engine.log().last_hash()[..12]
            );
        }
        Some("grants") => print_grants(&engine),
        Some("grant") => grant(&mut engine, &global, &args)?,
        Some("revoke") => revoke(&mut engine, &global, &args)?,
        Some("sandbox") => sandbox(&engine, &global, &args)?,
        Some("plugin") => plugin(&mut engine, &global, &home, &args)?,
        Some("ui") => ui(&mut engine, &global, &args)?,
        Some("inference") => inference(&mut engine, &global, &args)?,
        Some("agent") => agent_command(&mut engine, &global, &home, &args)?,
        Some("session") => session_command(&mut engine, &global, &home, &args)?,
        Some("generation") => generation_command(&mut engine, &global, &home, &args)?,
        Some("mcp") => mcp_command(&mut engine, &global, &home, &args)?,
        _ => print_usage(),
    }

    Ok(())
}

/// The one place a ledger is opened. Every command that mutates state gets the
/// same store, log, validators, and policy; the MCP server does too, so a tool
/// call and a CLI change land in the same transaction log.
pub(crate) fn open_ledger(home: &std::path::Path) -> Result<Ledger, Box<dyn std::error::Error>> {
    let policy = Policy::load_or_default(home.join("grants"))?;
    let log = ActionLog::open(home.join("log"))?;
    let store = FileStore::open(home.join("state"))?;
    let snapshot_dir = home.join("snapshots");
    std::fs::create_dir_all(&snapshot_dir)?;
    Ok(Engine::with_authority(
        store,
        SchemaHealth,
        RuleBasedSummarizer,
        CompositeValidator::new(vec![
            Box::new(SchemaValidator),
            Box::new(UiTreeValidator),
            Box::new(KeymapValidator),
            Box::new(ModelRegistryValidator),
        ]),
        policy,
        log,
        snapshot_dir,
    ))
}

/// `agentd mcp serve`: run the OS capability surface as an MCP server on stdio.
fn mcp_command(
    engine: &mut Ledger,
    global: &Global,
    home: &std::path::Path,
    args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    match args.get(1).map(String::as_str) {
        Some("serve") => mcp::serve(home, global, engine),
        _ => {
            println!("usage: agentd mcp serve   # MCP capability server on stdio");
            Ok(())
        }
    }
}

fn parse_global(args: &[String]) -> Result<(Global, Vec<String>), Box<dyn std::error::Error>> {
    let mut global = Global {
        // The acting principal travels out of band when the OS starts a child
        // (the ACP bridge, the MCP server). `--principal` still wins.
        principal: env::var("TIAKI_PRINCIPAL")
            .ok()
            .filter(|principal| !principal.is_empty())
            .unwrap_or_else(|| "user:local".to_string()),
        mode: AccessMode::Guarded,
        mode_explicit: false,
    };
    let mut rest = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--principal" => {
                global.principal = args
                    .get(index + 1)
                    .ok_or("--principal needs a value")?
                    .clone();
                index += 2;
            }
            "--mode" => {
                let value = args.get(index + 1).ok_or("--mode needs a value")?;
                global.mode = AccessMode::parse(value)
                    .ok_or_else(|| format!("unknown mode '{value}'; use guarded or full"))?;
                global.mode_explicit = true;
                index += 2;
            }
            other => {
                rest.push(other.to_string());
                index += 1;
            }
        }
    }

    // Validate the principal early so every command gets a clear error.
    Principal::parse(&global.principal)?;

    Ok((global, rest))
}

pub(crate) struct Invocation {
    pub(crate) path: String,
    pub(crate) value: String,
    pub(crate) intent: String,
    pub(crate) dry_run: bool,
}

pub(crate) fn parse_invocation(args: &[String]) -> Result<Invocation, Box<dyn std::error::Error>> {
    if args.len() < 3 {
        return Err(
            "usage: agentd <check|apply> <path> <value> [--intent <text>] [--dry-run]".into(),
        );
    }

    let mut intent = String::new();
    let mut dry_run = false;
    let mut index = 3;
    while index < args.len() {
        match args[index].as_str() {
            "--intent" => {
                intent = args.get(index + 1).cloned().unwrap_or_default();
                index += 2;
            }
            "--dry-run" => {
                dry_run = true;
                index += 1;
            }
            other => return Err(format!("unknown argument '{other}'").into()),
        }
    }

    Ok(Invocation {
        path: args[1].clone(),
        value: args[2].clone(),
        intent,
        dry_run,
    })
}

mod agent;
mod auth;
mod generation;
mod health;
mod inference;
mod mcp;
mod plugin;
mod sandbox;
mod session;
mod status;
mod txn;
mod ui;

use crate::agent::agent_command;
use crate::auth::{grant, revoke};
use crate::inference::inference;
use crate::plugin::plugin;
use crate::sandbox::sandbox;
use crate::session::{generation_command, session_command};
use crate::status::{print_grants, print_log, print_state};
use crate::txn::{apply, check, revert};
use crate::ui::ui;

pub(crate) fn flag_value<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1))
        .map(String::as_str)
}

pub(crate) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

pub(crate) fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

pub(crate) fn print_usage() {
    println!("agentd — Tiaki control plane");
    println!();
    println!("  agentd check <path> <value> [--intent <text>]");
    println!("  agentd apply <path> <value> [--intent <text>] [--dry-run]");
    println!("  agentd revert <transaction-id>");
    println!("  agentd log | state | verify");
    println!("  agentd grants");
    println!("  agentd grant <principal> <action> <resource> [--approval] [--max-per-minute N]");
    println!("  agentd revoke <principal> <action> <resource>");
    println!("  agentd sandbox <command> [args...] [--network]   (plan, probe, bench)");
    println!("  agentd plugin <command> ...   (list, install, update, remove, ...)");
    println!(
        "  agentd ui <command> ...       (get, render, serve, set, edit, reset, demo, keymap)"
    );
    println!(
        "  agentd inference <command> ... (models, load, unload, ask, bench, stats, cache, burst)"
    );
    println!("  agentd agent <command> ... (run, notify, list, show, default)");
    println!("  agentd session <command> ...   (capture, show, restore, clear)");
    println!("  agentd generation <command> ... (list, stage, update, boot, commit, rollback)");
    println!();
    println!("Global flags:");
    println!("  --principal <id>   default user:local");
    println!("  --mode guarded|full   default guarded");
    println!();
    println!("State, log, snapshots, grants, and plugins live in $TIAKI_HOME (default .tiaki).");
}
