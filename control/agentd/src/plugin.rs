//! agentd command: Plugin lifecycle: install, load, swap, config, spec.
use crate::{now_nanos, now_secs, Global, Ledger};
use agentic_capability::ActionClass;
use agentic_core_log::{Impact, Status, Summary};
use agentic_json::Json;
use agentic_plugin_host::{DigestVerifier, InstallOptions, InstalledPlugin, PluginHost, Version};
use agentic_transaction::SandboxClass;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub(crate) fn plugin(
    engine: &mut Ledger,
    global: &Global,
    home: &Path,
    args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(command) = args.get(1).map(String::as_str) else {
        print_plugin_usage();
        return Ok(());
    };

    let mut host = PluginHost::open(home.join("plugins"), Box::new(DigestVerifier))?;

    match command {
        "list" => {
            if host.list().is_empty() {
                println!("no plugins installed");
            } else {
                for installed in host.list() {
                    print_plugin(installed);
                }
            }
        }
        "install" | "update" => {
            let Some(source) = args.get(2) else {
                return Err(
                    format!("usage: agentd plugin {command} <dir> [--allow-unverified]").into(),
                );
            };
            let source = PathBuf::from(source);
            let allow_unverified = args.iter().any(|arg| arg == "--allow-unverified");
            let options = InstallOptions { allow_unverified };

            let manifest = agentic_plugin_host::load_manifest(source.join("plugin.toml"))?;
            let id = manifest.info.id.clone();
            authorize_plugin(
                engine,
                global,
                ActionClass::PluginManage,
                &format!("system/plugins/{id}"),
            )?;

            let result = if command == "install" {
                host.install(&source, &options).map(|outcome| {
                    println!(
                        "installed {} {} ({}, {}, signature {})",
                        outcome.id,
                        outcome.version,
                        outcome.tier.as_str(),
                        outcome.profile.as_str(),
                        outcome.signature
                    );
                    if !outcome.config.is_empty() {
                        println!("  config: {}", render_config(&outcome.config));
                    }
                    println!(
                        "  sandbox: {} (planned; the provisioner executes it)",
                        outcome.profile.as_str()
                    );
                })
            } else {
                host.update(&source, &options).map(|outcome| {
                    println!(
                        "updated {} {} -> {} ({}, {})",
                        outcome.id,
                        outcome.from,
                        outcome.to,
                        outcome.tier.as_str(),
                        outcome.swap.as_str()
                    );
                    if !outcome.migrations.is_empty() {
                        println!(
                            "  migrated config: {}",
                            outcome
                                .migrations
                                .iter()
                                .map(|version| version.to_string())
                                .collect::<Vec<_>>()
                                .join(" -> ")
                        );
                    }
                })
            };

            match result {
                Ok(()) => {
                    record_plugin(
                        engine,
                        global,
                        command,
                        &id,
                        format!(
                            "{} plugin {id} {}.",
                            capitalize(command),
                            manifest.info.version
                        ),
                        Status::Committed,
                        manifest.required_profile().into(),
                    );
                }
                Err(error) => {
                    record_plugin(
                        engine,
                        global,
                        command,
                        &id,
                        format!("Could not {command} plugin {id}: {error}."),
                        Status::Failed,
                        manifest.required_profile().into(),
                    );
                    return Err(error.into());
                }
            }
        }
        "remove" => {
            let id = require_plugin_id(args, command)?;
            authorize_plugin(
                engine,
                global,
                ActionClass::PluginManage,
                &format!("system/plugins/{id}"),
            )?;
            let outcome = host.remove(id)?;
            println!("removed {} {}", outcome.id, outcome.version);
            record_plugin(
                engine,
                global,
                "remove",
                id,
                format!("Removed plugin {} {}.", outcome.id, outcome.version),
                Status::Committed,
                SandboxClass::SystemWrite,
            );
        }
        "load" | "unload" | "start" | "stop" => {
            let id = require_plugin_id(args, command)?;
            authorize_plugin(
                engine,
                global,
                ActionClass::PluginManage,
                &format!("system/plugins/{id}"),
            )?;
            let result = match command {
                "load" => host.load(id),
                "unload" => host.unload(id),
                "start" => host.start(id),
                "stop" => host.stop(id),
                _ => unreachable!(),
            };
            result?;
            let state = host
                .get(id)
                .map(|plugin| plugin.state.as_str().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            println!("{command} {id} ({state})");
            record_plugin(
                engine,
                global,
                command,
                id,
                format!("{} plugin {id}.", capitalize(command)),
                Status::Committed,
                SandboxClass::SystemWrite,
            );
        }
        "swap" => {
            let id = require_plugin_id(args, command)?;
            let Some(raw_version) = args.get(3) else {
                return Err("usage: agentd plugin swap <id> <version>".into());
            };
            let version = Version::parse(raw_version)
                .ok_or_else(|| format!("invalid version '{raw_version}'"))?;
            authorize_plugin(
                engine,
                global,
                ActionClass::PluginManage,
                &format!("system/plugins/{id}"),
            )?;
            let swap = host.swap(id, version)?;
            println!("swapped {id} to {version} ({})", swap.as_str());
            record_plugin(
                engine,
                global,
                "swap",
                id,
                format!("Switched plugin {id} to version {version}."),
                Status::Committed,
                SandboxClass::SystemWrite,
            );
        }
        "rollback" => {
            let id = require_plugin_id(args, command)?;
            authorize_plugin(
                engine,
                global,
                ActionClass::PluginManage,
                &format!("system/plugins/{id}"),
            )?;
            let swap = host.rollback(id)?;
            let version = host
                .get(id)
                .map(|plugin| plugin.active.to_string())
                .unwrap_or_default();
            println!("rolled back {id} to {version} ({})", swap.as_str());
            record_plugin(
                engine,
                global,
                "rollback",
                id,
                format!("Rolled plugin {id} back to version {version}."),
                Status::Committed,
                SandboxClass::SystemWrite,
            );
        }
        "config" => {
            let id = require_plugin_id(args, command)?;
            if args.len() <= 3 {
                let config = host.config(id)?;
                if config.is_empty() {
                    println!("no config");
                } else {
                    println!("{}", render_config(&config));
                }
            } else {
                authorize_plugin(
                    engine,
                    global,
                    ActionClass::PluginManage,
                    &format!("system/plugins/{id}"),
                )?;
                let mut values = BTreeMap::new();
                for pair in &args[3..] {
                    let (key, raw) = pair
                        .split_once('=')
                        .ok_or_else(|| format!("expected key=value, found '{pair}'"))?;
                    values.insert(key.to_string(), parse_config_value(raw));
                }
                let config = host.set_config(id, values)?;
                println!("config updated: {}", render_config(&config));
                record_plugin(
                    engine,
                    global,
                    "config",
                    id,
                    format!("Updated config for plugin {id}."),
                    Status::Committed,
                    SandboxClass::SystemWrite,
                );
            }
        }
        "spec" => {
            let id = require_plugin_id(args, command)?;
            let spec = host.spec(id)?;
            println!("sandbox spec for {id}");
            println!("  principal: {}", spec.principal);
            println!("  profile:   {}", spec.profile.as_str());
            println!("  command:   {}", spec.command);
            println!("  read-only: {}", spec.read_only.join(", "));
            println!("  writable:  {}", spec.read_write.join(", "));
            println!("  caps:      {}", spec.capabilities.join(", "));
            println!(
                "  limits:    memory={:?} pids={:?} network={}",
                spec.limits.memory_bytes, spec.limits.pids, spec.limits.network
            );
        }
        "sign" => {
            let Some(dir) = args.get(2) else {
                return Err("usage: agentd plugin sign <dir>".into());
            };
            let dir = PathBuf::from(dir);
            let manifest_text = std::fs::read_to_string(dir.join("plugin.toml"))?;
            let signature =
                agentic_plugin_host::signature::compute_signature(&dir, &manifest_text)?;
            println!("{signature}");
        }
        other => return Err(format!("unknown plugin command '{other}'").into()),
    }

    Ok(())
}

pub(crate) fn require_plugin_id<'a>(
    args: &'a [String],
    command: &str,
) -> Result<&'a str, Box<dyn std::error::Error>> {
    args.get(2)
        .map(String::as_str)
        .ok_or_else(|| format!("usage: agentd plugin {command} <id>").into())
}

pub(crate) fn authorize_plugin(
    engine: &Ledger,
    global: &Global,
    action: ActionClass,
    resource: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let decision = engine.authority().authorize_action(
        &global.principal,
        global.mode,
        action,
        resource,
        now_secs(),
    );
    if decision.is_clear() {
        Ok(())
    } else {
        Err(format!(
            "{} may not manage plugins: {}",
            global.principal, decision.reason
        )
        .into())
    }
}

pub(crate) fn record_plugin(
    engine: &mut Ledger,
    global: &Global,
    action: &str,
    id: &str,
    summary: String,
    status: Status,
    sandbox: SandboxClass,
) {
    let transaction_id = format!("plugin_{action}_{id}_{:x}", now_nanos());
    let _ = engine.record(
        &global.principal,
        global.mode,
        sandbox,
        &transaction_id,
        &format!("{action} plugin {id}"),
        Summary::new(summary, String::new(), "rule-based", true),
        Impact::none(),
        status,
        vec![format!("plugin.{action} {id}")],
    );
}

pub(crate) fn print_plugin(installed: &InstalledPlugin) {
    println!(
        "{:<34} {:<9} {:<8} {:<9} {:<10} {}",
        installed.id(),
        installed.active,
        installed.manifest.info.kind.as_str(),
        installed.manifest.tier().as_str(),
        installed.state.as_str(),
        installed.manifest.required_profile().as_str()
    );
}

pub(crate) fn render_config(config: &BTreeMap<String, Json>) -> String {
    config
        .iter()
        .map(|(key, value)| format!("{key}={}", value.render()))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn parse_config_value(raw: &str) -> Json {
    agentic_plugin_host::json::parse(raw).unwrap_or_else(|_| Json::String(raw.to_string()))
}

pub(crate) fn capitalize(value: &str) -> String {
    let mut characters = value.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
        None => String::new(),
    }
}

pub(crate) fn print_plugin_usage() {
    println!("agentd plugin <command>");
    println!();
    println!("  list");
    println!("  install <dir> [--allow-unverified]");
    println!("  update <dir> [--allow-unverified]");
    println!("  remove <id>");
    println!("  load|unload|start|stop <id>");
    println!("  swap <id> <version>");
    println!("  rollback <id>");
    println!("  config <id> [key=value ...]");
    println!("  spec <id>");
    println!("  sign <dir>");
}
