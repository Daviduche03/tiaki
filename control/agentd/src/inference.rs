//! agentd command: Inference backends, records, and benchmarks.
use crate::ui::{apply_change, effective_principal};
use crate::{now_nanos, now_secs, Global, Ledger};
use agentic_capability::ActionClass;
use agentic_core_log::{Impact, Status, Summary};
use agentic_inference::{
    is_model_path, model_path, Broker, BrokerConfig, BrokerEvent, GenerateRequest, ModelSpec,
    Priority, Quota, SimulatedBackend,
};
use agentic_transaction::{SandboxClass, StateStore};
use std::collections::BTreeMap;

pub(crate) fn inference(
    engine: &mut Ledger,
    global: &Global,
    args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(command) = args.get(1).map(String::as_str) else {
        print_inference_usage();
        return Ok(());
    };

    match command {
        "models" => {
            let mut found = false;
            for (path, value) in engine.store().values() {
                if !is_model_path(path) {
                    continue;
                }
                let Ok(spec) = ModelSpec::parse(value) else {
                    continue;
                };
                if !found {
                    println!(
                        "{:<18} {:<8} {:<11} {:<7} {:<7} digest",
                        "id", "version", "architecture", "context", "loaded"
                    );
                    found = true;
                }
                let digest = spec.weights_digest.trim_start_matches("sha256:");
                println!(
                    "{:<18} {:<8} {:<11} {:<7} {:<7} sha256:{}…",
                    spec.id,
                    spec.version,
                    spec.architecture,
                    spec.context_length,
                    spec.loaded,
                    &digest[..12]
                );
            }
            if !found {
                println!(
                    "no models registered; load one with: agentd inference load models/sim-tiny.json"
                );
            }
        }
        "load" => {
            let Some(file) = args.get(2) else {
                return Err("usage: agentd inference load <file>".into());
            };
            let text = std::fs::read_to_string(file)?;
            let mut spec = ModelSpec::parse(&text)?;
            spec.loaded = true;
            authorize_inference(
                engine,
                global,
                ActionClass::InferenceManage,
                &format!("system/inference/models/{}", spec.id),
            )?;
            apply_change(
                engine,
                global,
                &model_path(&spec.id),
                spec.render(),
                &format!("Load model {}", spec.id),
            )?;
            println!(
                "loaded {} ({}, {} context, {} backend)",
                spec.id, spec.architecture, spec.context_length, spec.backend
            );
        }
        "unload" => {
            let Some(id) = args.get(2) else {
                return Err("usage: agentd inference unload <id>".into());
            };
            let path = model_path(id);
            let text = engine
                .store()
                .get(&path)
                .ok_or_else(|| format!("model '{id}' is not registered"))?;
            let mut spec = ModelSpec::parse(&text)?;
            authorize_inference(
                engine,
                global,
                ActionClass::InferenceManage,
                &format!("system/inference/models/{id}"),
            )?;
            spec.loaded = false;
            apply_change(
                engine,
                global,
                &path,
                spec.render(),
                &format!("Unload model {id}"),
            )?;
            println!("unloaded {id}");
        }
        "ask" => {
            let Some(model) = args.get(2) else {
                return Err(
                    "usage: agentd inference ask <model> <prompt> [--priority ...] [--max-tokens N]"
                        .into(),
                );
            };
            let Some(prompt) = args.get(3) else {
                return Err("agentd inference ask needs a prompt".into());
            };
            let mut priority = Priority::Normal;
            let mut max_tokens = 32usize;
            let mut index = 4;
            while index < args.len() {
                match args[index].as_str() {
                    "--priority" => {
                        let value = args.get(index + 1).ok_or("--priority needs a value")?;
                        priority = Priority::parse(value)
                            .ok_or_else(|| format!("unknown priority '{value}'"))?;
                        index += 2;
                    }
                    "--max-tokens" => {
                        let value = args.get(index + 1).ok_or("--max-tokens needs a value")?;
                        max_tokens = value
                            .parse()
                            .map_err(|_| format!("invalid token count '{value}'"))?;
                        index += 2;
                    }
                    other => return Err(format!("unknown argument '{other}'").into()),
                }
            }

            authorize_inference(
                engine,
                global,
                ActionClass::InferenceGenerate,
                &format!("model/{model}"),
            )?;

            let path = model_path(model);
            let text = engine
                .store()
                .get(&path)
                .ok_or_else(|| format!("model '{model}' is not registered"))?;
            let spec = ModelSpec::parse(&text)?;
            if !spec.loaded {
                return Err(format!("model '{model}' is registered but not loaded").into());
            }

            let principal = effective_principal(&global.principal);
            let request = GenerateRequest {
                principal: principal.clone(),
                session: "cli".into(),
                model: model.to_string(),
                prompt: prompt.to_string(),
                priority,
                max_tokens,
                deadline: None,
            };
            let mut broker =
                Broker::new(Box::new(SimulatedBackend::new()), BrokerConfig::default());
            broker.load(&spec)?;
            let (output, outcome) = broker
                .generate_one(request, now_secs())
                .map_err(|error| error.to_string())?;

            println!("{output}");
            println!(
                "  {} tokens, {} preemption(s), {}",
                outcome.tokens,
                outcome.preemptions,
                outcome.status.as_str()
            );
            record_inference(
                engine,
                global,
                format!(
                    "{principal} generated {} tokens with {model} ({}).",
                    outcome.tokens,
                    priority.as_str()
                ),
                Status::Committed,
                SandboxClass::Inspect,
            );
        }
        "bench" => inference_bench(engine, global)?,
        "burst" => {
            // The broker's only hook into process scheduling: while this flag
            // exists, every launcher pushes agents down to their floor.
            let flag = std::env::var("TIAKI_INFERENCE_FLAG")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| std::path::PathBuf::from("/run/tiaki/inference-active"));
            match args.get(2).map(String::as_str) {
                Some("on") => {
                    if let Some(dir) = flag.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    std::fs::write(&flag, "1")?;
                    println!("inference burst started: {}", flag.display());
                    apply_burst_to_running();
                    println!("  agents yield to their floor until `agentd inference burst off`.");
                }
                Some("off") => match std::fs::remove_file(&flag) {
                    Ok(()) => {
                        println!("inference burst cleared: agents resume.");
                        apply_burst_to_running();
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        println!("no burst was active.")
                    }
                    Err(error) => return Err(error.into()),
                },
                _ => return Err("usage: agentd inference burst on|off".into()),
            }
        }
        "stats" => {
            let loaded = engine
                .store()
                .values()
                .iter()
                .filter(|(path, _)| is_model_path(path))
                .filter_map(|(_, value)| ModelSpec::parse(value).ok())
                .filter(|spec| spec.loaded)
                .count();
            println!("inference broker (simulated backend)");
            println!("  loaded models: {loaded}");
            for principal in ["user:ada", "system", "service:inference"] {
                let quota = Quota::for_principal(principal);
                println!(
                    "  {}: {} concurrent, {} tokens/request, {}/min",
                    principal,
                    quota.max_concurrent,
                    quota.max_tokens_per_request,
                    quota.tokens_per_minute
                );
            }
            println!("  run `agentd inference bench` for a live scheduling trace");
        }
        "cache" => {
            println!("cache state is per-broker and zeroed on completion");
            println!("run `agentd inference bench` to watch isolation and zeroing");
        }
        other => return Err(format!("unknown inference command '{other}'").into()),
    }

    Ok(())
}

pub(crate) fn inference_bench(
    engine: &mut Ledger,
    global: &Global,
) -> Result<(), Box<dyn std::error::Error>> {
    let spec = ModelSpec::example();
    let now = now_secs();
    let mut broker = Broker::new(
        Box::new(SimulatedBackend::new()),
        BrokerConfig {
            max_active: 2,
            preempt_for_interactive: true,
        },
    );
    broker.load(&spec)?;

    let ada = broker.submit(
        GenerateRequest {
            principal: "user:ada".into(),
            session: "bench-ada".into(),
            model: spec.id.clone(),
            prompt: "summarize the system logs".into(),
            priority: Priority::Batch,
            max_tokens: 48,
            deadline: None,
        },
        now,
    )?;
    let bob = broker.submit(
        GenerateRequest {
            principal: "user:bob".into(),
            session: "bench-bob".into(),
            model: spec.id.clone(),
            prompt: "check the network".into(),
            priority: Priority::Normal,
            max_tokens: 32,
            deadline: None,
        },
        now,
    )?;

    let mut labels: BTreeMap<u64, String> = BTreeMap::new();
    labels.insert(ada, "ada:batch".into());
    labels.insert(bob, "bob:normal".into());

    let mut events = Vec::new();
    for _ in 0..2 {
        events.extend(broker.step(now)?);
    }

    println!("inference bench (one model, simulated backend, max_active=2)");
    println!("  shared weights: {}", spec.id);
    println!("  active caches after two steps:");
    for entry in broker.cache_entries() {
        println!(
            "    {} / {} / {} ({} bytes)",
            entry.principal, entry.session, entry.model, entry.bytes
        );
    }

    let carol = broker.submit(
        GenerateRequest {
            principal: "user:carol".into(),
            session: "bench-carol".into(),
            model: spec.id.clone(),
            prompt: "what is broken?".into(),
            priority: Priority::Interactive,
            max_tokens: 12,
            deadline: None,
        },
        now,
    )?;
    labels.insert(carol, "carol:interactive".into());

    events.extend(broker.run(now)?);

    let mut texts: BTreeMap<u64, String> = BTreeMap::new();
    let mut preempted = Vec::new();
    let mut completed = Vec::new();
    for event in &events {
        match event {
            BrokerEvent::Token { request, text } => {
                let entry = texts.entry(*request).or_default();
                entry.push_str(text);
                entry.push(' ');
            }
            BrokerEvent::Preempted {
                request,
                principal,
                tokens,
            } => preempted.push((*request, principal.clone(), *tokens)),
            BrokerEvent::Completed {
                request,
                tokens,
                preemptions,
            } => completed.push((*request, *tokens, *preemptions)),
            BrokerEvent::Failed { request, reason } => {
                println!("  {} failed: {reason}", label_of(&labels, *request));
            }
        }
    }

    for (request, principal, tokens) in &preempted {
        println!(
            "  preempted {} ({principal}) after {tokens} tokens",
            label_of(&labels, *request)
        );
    }
    for (request, tokens, preemptions) in &completed {
        println!(
            "  completed {}: {tokens} tokens, {preemptions} preemption(s)",
            label_of(&labels, *request)
        );
    }
    for (request, text) in &texts {
        let preview: String = text.chars().take(56).collect();
        println!("  {} → {preview}…", label_of(&labels, *request));
    }

    let stats = broker.stats();
    println!(
        "  tokens: {}  preemptions: {}  caches after: {}  zeroed: {}",
        stats.tokens, stats.preemptions, stats.caches, stats.backend.caches_zeroed
    );
    println!(
        "  {} requests, {} completed, fair share across 3 principals",
        broker.outcomes().len(),
        stats.completed
    );

    if stats.preemptions == 0 {
        println!("  note: no preemption occurred; the batch finished too early");
    }

    record_inference(
        engine,
        global,
        format!(
            "Ran inference bench: {} requests, {} preemption(s), {} tokens.",
            broker.outcomes().len(),
            stats.preemptions,
            stats.tokens
        ),
        Status::Committed,
        SandboxClass::Inspect,
    );

    Ok(())
}

pub(crate) fn label_of(labels: &BTreeMap<u64, String>, request: u64) -> String {
    labels
        .get(&request)
        .cloned()
        .unwrap_or_else(|| format!("request {request}"))
}

pub(crate) fn authorize_inference(
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
            "{} may not use inference: {}",
            global.principal, decision.reason
        )
        .into())
    }
}

pub(crate) fn record_inference(
    engine: &mut Ledger,
    global: &Global,
    summary: String,
    status: Status,
    sandbox: SandboxClass,
) {
    let transaction_id = format!("inference_{:x}", now_nanos());
    let _ = engine.record(
        &global.principal,
        global.mode,
        sandbox,
        &transaction_id,
        "Inference request",
        Summary::new(summary, String::new(), "rule-based", true),
        Impact::none(),
        status,
        vec!["inference.generate".into()],
    );
}

pub(crate) fn print_inference_usage() {
    println!("agentd inference <command>");
    println!();
    println!("  models");
    println!("  load <file>");
    println!("  unload <id>");
    println!("  ask <model> <prompt> [--priority batch|normal|interactive] [--max-tokens N]");
    println!("  bench");
    println!("  stats");
    println!("  cache");
}

/// Push the new burst state into every running sandbox. A burst that only
/// takes effect on the next launch would be a lie about a running system.
fn apply_burst_to_running() {
    let root = agentic_sandbox::root_from_env();
    match agentic_agents::launch::rebalance(&root) {
        Ok(applied) if applied.is_empty() => {
            println!("  no supervised processes are running.");
        }
        Ok(applied) => {
            println!("  re-weighted {} running process(es).", applied.len());
        }
        Err(error) => {
            println!("  could not re-weight running processes: {error}");
        }
    }
}
