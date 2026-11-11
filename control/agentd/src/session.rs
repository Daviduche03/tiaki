//! agentd command: Sessions, boot-once, and generation staging helpers.
use crate::generation::{
    authorize_generation, generation_list, generation_status, print_generation_usage,
    print_rollback, record_generation,
};
use crate::health::{
    stage_generation, FailingProbe, KeymapProbe, ModelProbe, SessionProbe, UiTreeProbe,
};
use crate::ui::effective_principal;
use crate::{flag_value, now_nanos, now_secs, Global, Ledger};
use agentic_capability::Principal;
use agentic_core_boot::{
    boot_generation, BootContext, BootHealth, BootImage, HealthProbe, SimulatedKexec,
};
use agentic_core_init::{SessionSnapshot, SessionStore};
use agentic_core_log::{Impact, Status, Summary};
use agentic_core_rollback::{BootDecision, GenerationTable};
use agentic_transaction::{AccessMode, Operation, SandboxClass, StateStore};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub(crate) fn session_store(home: &Path) -> SessionStore {
    SessionStore::new(home.join("session").join("snapshot.json"))
}

pub(crate) fn generations_root(home: &Path) -> PathBuf {
    home.join("generations")
}

pub(crate) fn table_path(home: &Path) -> PathBuf {
    generations_root(home).join("table.json")
}

pub(crate) fn session_command(
    engine: &mut Ledger,
    global: &Global,
    home: &Path,
    args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(command) = args.get(1).map(String::as_str) else {
        print_session_usage();
        return Ok(());
    };

    match command {
        "capture" => {
            let snapshot = capture_session(engine, global, home)?;
            println!("captured session: {}", snapshot.summary());
        }
        "show" => {
            let snapshot = session_store(home)
                .load()?
                .ok_or("no session snapshot; run `agentd session capture`")?;
            println!("{}", snapshot.render());
        }
        "restore" => {
            let snapshot = session_store(home)
                .load()?
                .ok_or("no session snapshot; run `agentd session capture`")?;
            restore_session(engine, global, &snapshot)?;
        }
        "clear" => {
            session_store(home).clear()?;
            println!("cleared the session snapshot");
        }
        other => return Err(format!("unknown session command '{other}'").into()),
    }
    Ok(())
}

pub(crate) fn capture_session(
    engine: &Ledger,
    global: &Global,
    home: &Path,
) -> Result<SessionSnapshot, Box<dyn std::error::Error>> {
    let principal = effective_principal(&global.principal);
    let store = session_store(home);
    let boot_id = GenerationTable::load(table_path(home))?.boot_id;
    let existing = store.load()?;
    let mut snapshot = existing
        .clone()
        .unwrap_or_else(|| SessionSnapshot::new(boot_id.clone(), principal.clone(), now_secs()));
    // A system-driven update captures the *user's* session, not the system's.
    if principal != "system" || existing.is_none() {
        snapshot.user = principal;
    }
    snapshot.boot_id = boot_id;
    snapshot.captured_at = now_secs();
    let scope = snapshot.user.clone();
    snapshot.ui_tree = engine.store().get(&session_scope(&scope, "ui/tree"));
    snapshot.keymap = engine.store().get(&session_scope(&scope, "input/keymap"));
    snapshot.theme = engine.store().get(&session_scope(&scope, "theme/mode"));
    snapshot.pending_transactions = engine
        .entries()
        .iter()
        .filter(|entry| entry.status == Status::PendingApproval)
        .map(|entry| entry.transaction_id.clone())
        .collect();
    store.save(&snapshot)?;
    Ok(snapshot)
}

pub(crate) fn restore_session(
    engine: &mut Ledger,
    global: &Global,
    snapshot: &SessionSnapshot,
) -> Result<(), Box<dyn std::error::Error>> {
    let principal = if snapshot.user == "system"
        || snapshot.user.starts_with("user:")
        || snapshot.user.starts_with("service:")
        || snapshot.user.starts_with("agent:")
    {
        snapshot.user.clone()
    } else {
        format!("user:{}", snapshot.user)
    };
    Principal::parse(&principal)?;

    if let Some(tree) = &snapshot.ui_tree {
        restore_value(
            engine,
            &principal,
            &session_scope(&principal, "ui/tree"),
            tree.clone(),
            "Restore the UI tree after reboot",
        )?;
    }
    if let Some(keymap) = &snapshot.keymap {
        restore_value(
            engine,
            &principal,
            &session_scope(&principal, "input/keymap"),
            keymap.clone(),
            "Restore the keymap after reboot",
        )?;
    }
    if let Some(theme) = &snapshot.theme {
        restore_value(
            engine,
            &principal,
            &session_scope(&principal, "theme/mode"),
            theme.clone(),
            "Restore the theme after reboot",
        )?;
    }

    for app in &snapshot.apps {
        println!(
            "  restored app {}: {} value(s), focus {}",
            app.app,
            app.values.len(),
            app.focused.as_deref().unwrap_or("none")
        );
    }

    let tree_ok = snapshot
        .ui_tree
        .as_deref()
        .map(|value| {
            engine
                .store()
                .get(&session_scope(&principal, "ui/tree"))
                .as_deref()
                == Some(value)
        })
        .unwrap_or(true);
    let keymap_ok = snapshot
        .keymap
        .as_deref()
        .map(|value| {
            engine
                .store()
                .get(&session_scope(&principal, "input/keymap"))
                .as_deref()
                == Some(value)
        })
        .unwrap_or(true);
    let theme_ok = snapshot
        .theme
        .as_deref()
        .map(|value| {
            engine
                .store()
                .get(&session_scope(&principal, "theme/mode"))
                .as_deref()
                == Some(value)
        })
        .unwrap_or(true);

    println!(
        "session restored: {} (tree {}, keymap {}, theme {}, {} pending)",
        snapshot.summary(),
        yes_no(tree_ok),
        yes_no(keymap_ok),
        yes_no(theme_ok),
        snapshot.pending_transactions.len()
    );

    record_session(
        engine,
        global,
        format!(
            "Restored the session for {}: {}.",
            snapshot.user,
            snapshot.summary()
        ),
    );
    Ok(())
}

pub(crate) fn restore_value(
    engine: &mut Ledger,
    principal: &str,
    path: &str,
    value: String,
    intent: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let id = format!("txn_{:x}", now_nanos());
    match engine.apply(
        &id,
        principal,
        AccessMode::Guarded,
        intent,
        vec![Operation::Set {
            path: path.to_string(),
            value,
        }],
    ) {
        Ok(transaction) => {
            println!("  {}", transaction.summary.one_line);
            Ok(())
        }
        Err(error) => Err(format!("{}: {}", error.reason, path).into()),
    }
}

pub(crate) fn record_session(engine: &mut Ledger, global: &Global, summary: String) {
    let transaction_id = format!("session_{:x}", now_nanos());
    let _ = engine.record(
        &global.principal,
        global.mode,
        SandboxClass::Inspect,
        &transaction_id,
        "Session restore",
        Summary::new(summary, String::new(), "rule-based", true),
        Impact::none(),
        Status::Committed,
        vec!["session.restore".into()],
    );
}

pub(crate) fn session_scope(user: &str, suffix: &str) -> String {
    match Principal::parse(user) {
        Ok(principal) => match principal.effective() {
            Principal::User(id) => format!("user/{id}/{suffix}"),
            Principal::Service(id) => format!("system/services/{id}/{suffix}"),
            _ => format!("system/{suffix}"),
        },
        Err(_) => format!("system/{suffix}"),
    }
}

pub(crate) fn yes_no(value: bool) -> &'static str {
    if value {
        "ok"
    } else {
        "MISMATCH"
    }
}

pub(crate) fn generation_command(
    engine: &mut Ledger,
    global: &Global,
    home: &Path,
    args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(command) = args.get(1).map(String::as_str) else {
        print_generation_usage();
        return Ok(());
    };

    match command {
        "list" | "history" => generation_list(home)?,
        "status" => generation_status(home)?,
        "stage" => {
            let Some(dir) = args.get(2) else {
                return Err("usage: agentd generation stage <dir> [--label <text>]".into());
            };
            let label = flag_value(args, "--label").unwrap_or("staged");
            let id = stage_generation(engine, global, home, Path::new(dir), label)?;
            println!("staged {id}");
        }
        "update" => generation_update(engine, global, home, args)?,
        "boot" => {
            let fail = args.iter().any(|arg| arg == "--fail");
            let downtime = flag_value(args, "--downtime-ms")
                .and_then(|value| value.parse().ok())
                .unwrap_or(180);
            capture_session(engine, global, home)?;
            boot_once(engine, global, home, fail, downtime)?;
        }
        "commit" => {
            authorize_generation(engine, global, "system/generations/commit")?;
            let mut table = GenerationTable::load(table_path(home))?;
            let id = table.commit()?;
            table.save(table_path(home))?;
            println!("committed {id}");
            if let Some(snapshot) = session_store(home).load()? {
                restore_session(engine, global, &snapshot)?;
            }
            record_generation(
                engine,
                global,
                format!("Committed generation {id}."),
                Status::Committed,
            );
        }
        "rollback" => {
            authorize_generation(engine, global, "system/generations/rollback")?;
            let mut table = GenerationTable::load(table_path(home))?;
            let decision = table.rollback("manual rollback");
            table.save(table_path(home))?;
            print_rollback(&decision);
            if let Some(snapshot) = session_store(home).load()? {
                restore_session(engine, global, &snapshot)?;
            }
            record_generation(
                engine,
                global,
                format!("Manually rolled back: {decision:?}."),
                Status::Reverted,
            );
        }
        other => return Err(format!("unknown generation command '{other}'").into()),
    }

    Ok(())
}

pub(crate) fn generation_update(
    engine: &mut Ledger,
    global: &Global,
    home: &Path,
    args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = args.get(2) else {
        return Err("usage: agentd generation update <dir> [--fail]".into());
    };
    let label = flag_value(args, "--label").unwrap_or("update");
    let fail = args.iter().any(|arg| arg == "--fail");
    let downtime = flag_value(args, "--downtime-ms")
        .and_then(|value| value.parse().ok())
        .unwrap_or(180);

    let snapshot = capture_session(engine, global, home)?;
    println!("session captured: {}", snapshot.summary());

    let id = stage_generation(engine, global, home, Path::new(dir), label)?;
    let mut table = GenerationTable::load(table_path(home))?;
    table.set_candidate(&id)?;
    table.save(table_path(home))?;

    println!("candidate {id} staged; rebooting");
    boot_once(engine, global, home, fail, downtime)?;
    Ok(())
}

pub(crate) fn boot_once(
    engine: &mut Ledger,
    global: &Global,
    home: &Path,
    fail: bool,
    downtime_ms: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut table = GenerationTable::load(table_path(home))?;
    let target = table
        .boot_target()
        .cloned()
        .ok_or("no generation to boot; stage one first")?;

    let decision = table.record_boot(now_secs());
    match decision {
        BootDecision::Proceed {
            generation,
            attempt,
        } => {
            let image_root = generations_root(home).join(&target.image);
            let image = BootImage::new(&target.id, &image_root);
            let mut kexec = SimulatedKexec::new().with_delay(downtime_ms);
            let timing = match boot_generation(&mut kexec, &image, &target.digest) {
                Ok(timing) => timing,
                Err(error) => {
                    let reason = format!("boot failed: {error}");
                    let decision = table.rollback(reason.clone());
                    table.save(table_path(home))?;
                    if let Some(snapshot) = session_store(home).load()? {
                        restore_session(engine, global, &snapshot)?;
                    }
                    print_rollback(&decision);
                    record_generation(
                        engine,
                        global,
                        format!("Rolled back after {reason}."),
                        Status::Reverted,
                    );
                    return Ok(());
                }
            };

            let snapshot = session_store(home).load()?;
            let context = BootContext {
                generation: generation.clone(),
                label: target.label.clone(),
                boot_id: table.boot_id.clone(),
                session: snapshot.as_ref().map(SessionSnapshot::render),
                state: engine.store().values().clone(),
                metadata: BTreeMap::new(),
            };
            let health = boot_health(fail).run(&context);

            println!(
                "  booted {generation} in {}ms (attempt {attempt})",
                timing.duration_ms
            );
            for check in &health.checks {
                if check.ok {
                    println!("  health {}: ok", check.name);
                } else {
                    println!("  health {}: FAILED — {}", check.name, check.reason);
                }
            }

            if health.ok {
                table.mark_healthy()?;
                let committed = if table.candidate.is_some() {
                    Some(table.commit()?)
                } else {
                    None
                };
                table.save(table_path(home))?;
                if let Some(snapshot) = &snapshot {
                    restore_session(engine, global, snapshot)?;
                }
                if let Some(id) = committed {
                    println!(
                        "committed {id}; last-good is {}",
                        table.last_good.clone().unwrap_or_else(|| "none".into())
                    );
                }
                println!(
                    "reboot complete: {}ms downtime, session restored — it reads as a repaint",
                    timing.duration_ms
                );
                record_generation(
                    engine,
                    global,
                    format!(
                        "Booted generation {generation} in {}ms and passed health.",
                        timing.duration_ms
                    ),
                    Status::Committed,
                );
            } else {
                let reason = health
                    .checks
                    .iter()
                    .filter(|check| !check.ok)
                    .map(|check| format!("{}: {}", check.name, check.reason))
                    .collect::<Vec<_>>()
                    .join("; ");
                let decision = table.rollback(format!("health check failed: {reason}"));
                table.save(table_path(home))?;
                if let Some(snapshot) = &snapshot {
                    restore_session(engine, global, snapshot)?;
                }
                print_rollback(&decision);
                record_generation(
                    engine,
                    global,
                    format!("Auto-reverted after a failed boot: {reason}."),
                    Status::Reverted,
                );
            }
        }
        BootDecision::Rollback { from, to, reason } => {
            table.save(table_path(home))?;
            println!("watchdog rolled back {from} -> {to}: {reason}");
            record_generation(
                engine,
                global,
                format!("Watchdog rolled back {from} to {to}: {reason}."),
                Status::Reverted,
            );
        }
        BootDecision::Rescue { reason } => {
            table.save(table_path(home))?;
            println!("rescue required: {reason}");
            record_generation(
                engine,
                global,
                format!("Boot rescue required: {reason}."),
                Status::Reverted,
            );
        }
    }
    Ok(())
}

pub(crate) fn boot_health(fail: bool) -> BootHealth {
    let mut probes: Vec<Box<dyn HealthProbe>> = vec![
        Box::new(SessionProbe),
        Box::new(UiTreeProbe),
        Box::new(KeymapProbe),
        Box::new(ModelProbe),
    ];
    if fail {
        probes.push(Box::new(FailingProbe));
    }
    BootHealth::new(probes)
}

pub(crate) fn print_session_usage() {
    println!("agentd session <command>");
    println!();
    println!("  capture");
    println!("  show");
    println!("  restore");
    println!("  clear");
}
