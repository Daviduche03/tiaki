//! agentd command: the single entry point for everything bound for an agent.
//!
//! The default agent is opencode, and agentd drives its real CLI —
//! `opencode run` against our own `opencode serve` — never a private wire
//! protocol. A notification starts a session exactly like a message does;
//! both go through [`run_agent`].
//!
//! Full access by default: unless `--mode guarded` was given explicitly, the
//! agent runs with `--auto` (approve whatever is not explicitly denied) and
//! every act lands on the ledger. Guarded mode runs without `--auto`, so
//! opencode's own policy asks; with no human on the pipe the run fails
//! loudly instead of acting.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use agentic_agents::RuntimeRegistry;
use agentic_core_log::{Impact, Status, Summary};
use agentic_json::Json;
use agentic_transaction::{AccessMode, SandboxClass};

use crate::ui::effective_principal;
use crate::{flag_value, now_nanos, Global, Ledger};

const DEFAULT_PORT: u16 = 4096;
const SERVER_START_TIMEOUT: Duration = Duration::from_secs(25);
const RUN_TIMEOUT: Duration = Duration::from_secs(600);
const MCP_SERVER_NAME: &str = "tiaki";

pub(crate) fn agent_command(
    engine: &mut Ledger,
    global: &Global,
    home: &Path,
    args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(command) = args.get(1).map(String::as_str) else {
        print_agent_usage();
        return Ok(());
    };
    match command {
        "run" => {
            let (title, session, message) = parse_message_args(args)?;
            run_agent(engine, global, home, "agent run", &title, session, &message)?;
        }
        "notify" => {
            let (title, session, message) = parse_message_args(args)?;
            run_agent(
                engine,
                global,
                home,
                "notification",
                &title,
                session,
                &message,
            )?;
        }
        "list" => {
            let principal = effective_principal(&global.principal);
            let server = ensure_server(home, &principal)?;
            let output = opencode_session(&server, home, &principal, &["list"])?;
            println!("{output}");
        }
        "show" => {
            let Some(id) = args.get(2) else {
                return Err("usage: agentd agent show <session-id>".into());
            };
            let principal = effective_principal(&global.principal);
            let server = ensure_server(home, &principal)?;
            let output = opencode_session(&server, home, &principal, &["export", id])?;
            println!("{output}");
        }
        "default" => match args.get(2).map(String::as_str) {
            None => match RuntimeRegistry::read_default(&home.join("runtimes")) {
                Some(id) => println!("default agent: {id}"),
                None => println!("no default agent set."),
            },
            Some(id) => {
                RuntimeRegistry::write_default(&home.join("runtimes"), id)
                    .map_err(|error| error.to_string())?;
                println!("default agent: {id}");
            }
        },
        other => return Err(format!("unknown agent command '{other}'").into()),
    }
    Ok(())
}

fn print_agent_usage() {
    println!("usage:");
    println!("  agentd agent run [--title T] [--session ID] -- <message...>");
    println!("  agentd agent notify [--title T] -- <text...>");
    println!("  agentd agent list");
    println!("  agentd agent show <session-id>");
    println!("  agentd agent default [id]");
}

/// The one path every agent-bound thing takes: a notification and a message
/// differ only in the ledger intent that records them.
fn run_agent(
    engine: &mut Ledger,
    global: &Global,
    home: &Path,
    intent: &str,
    title: &str,
    resume: Option<String>,
    message: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let principal = effective_principal(&global.principal);
    let full = !global.mode_explicit || global.mode == AccessMode::Full;
    let server = ensure_server(home, &principal)?;
    ensure_mcp(home, &server, &principal)?;

    record(
        engine,
        global,
        full,
        intent,
        format!("{principal} started {intent} '{title}'."),
        Status::Committed,
    );
    let outcome = opencode_run(&server, home, &principal, full, title, resume, message)?;
    eprintln!(
        "[agent] session {} finished ({}, {} input + {} output tokens)",
        outcome.session, outcome.reason, outcome.tokens_in, outcome.tokens_out
    );
    record(
        engine,
        global,
        full,
        intent,
        format!(
            "{principal} {intent} '{title}' done: {}",
            first_line(&outcome.answer)
        ),
        Status::Committed,
    );
    println!("{}", outcome.answer);
    Ok(())
}

struct RunOutcome {
    session: String,
    answer: String,
    reason: String,
    tokens_in: u64,
    tokens_out: u64,
}

fn parse_message_args(
    args: &[String],
) -> Result<(String, Option<String>, String), Box<dyn std::error::Error>> {
    let title = flag_value(args, "--title")
        .unwrap_or("untitled")
        .to_string();
    let session = flag_value(args, "--session").map(str::to_string);
    let mut words: Vec<&str> = Vec::new();
    let mut index = 2;
    while index < args.len() {
        match args[index].as_str() {
            "--title" | "--session" => index += 2,
            "--" => {
                index += 1;
                break;
            }
            word => {
                words.push(word);
                index += 1;
            }
        }
    }
    words.extend(args[index..].iter().map(String::as_str));
    if words.iter().all(|word| word.is_empty()) {
        return Err("no message: agentd agent run -- <message...>".into());
    }
    Ok((title, session, words.join(" ")))
}

fn workspace(home: &Path) -> PathBuf {
    std::env::var("TIAKI_WORKSPACE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home.join("workspace"))
}

fn port() -> u16 {
    std::env::var("TIAKI_OPENCODE_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_PORT)
}

fn server_url() -> String {
    format!("http://127.0.0.1:{}", port())
}

fn server_up() -> bool {
    std::net::TcpStream::connect_timeout(
        &format!("127.0.0.1:{}", port()).parse().expect("loopback"),
        Duration::from_millis(500),
    )
    .is_ok()
}

/// Our opencode server, started on demand. The password lives in the home
/// directory so later invocations reuse the same server instead of spawning
/// (and failing to authenticate against) a second one. The server inherits
/// our environment explicitly: opencode's `{env:}` substitution exports empty
/// strings for missing variables, which would silently lose the principal.
fn ensure_server(home: &Path, principal: &str) -> Result<String, Box<dyn std::error::Error>> {
    if server_up() {
        return Ok(server_url());
    }
    let pass_file = home.join("opencode.pass");
    let password = match std::fs::read_to_string(&pass_file) {
        Ok(password) if !password.trim().is_empty() => password.trim().to_string(),
        _ => {
            let password: String = (0..24)
                .map(|_| {
                    const HEX: &[u8] = b"0123456789abcdef";
                    HEX[(now_nanos() % 16) as usize] as char
                })
                .collect();
            std::fs::create_dir_all(home)?;
            std::fs::write(&pass_file, &password)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                let _ =
                    std::fs::set_permissions(&pass_file, std::fs::Permissions::from_mode(0o600));
            }
            password
        }
    };
    let workdir = workspace(home);
    std::fs::create_dir_all(&workdir)?;
    Command::new("opencode")
        .arg("serve")
        .arg("--port")
        .arg(port().to_string())
        .arg("--hostname")
        .arg("127.0.0.1")
        .current_dir(&workdir)
        .env("OPENCODE_SERVER_PASSWORD", &password)
        .env("TIAKI_HOME", home)
        .env("TIAKI_PRINCIPAL", principal)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("cannot start opencode serve: {error}"))?;
    let deadline = Instant::now() + SERVER_START_TIMEOUT;
    while Instant::now() < deadline {
        if server_up() {
            return Ok(server_url());
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    Err("opencode serve did not come up in time".into())
}

fn server_password(home: &Path) -> Option<String> {
    std::fs::read_to_string(home.join("opencode.pass"))
        .ok()
        .map(|password| password.trim().to_string())
        .filter(|password| !password.is_empty())
}

/// The user-global opencode config. Firstboot seeds it with a concrete
/// tiaki entry; `ensure_mcp` must not replace that with {env:}
/// references, which resolve empty under servers T3 spawns itself.
fn global_config_path() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .filter(|home| !home.is_empty())
        .map(|home| {
            PathBuf::from(home)
                .join(".config")
                .join("opencode")
                .join("opencode.json")
        })
}

fn global_config_has_server(name: &str) -> bool {
    let Some(path) = global_config_path() else {
        return false;
    };
    config_file_has_server(&path, name)
}

fn config_file_has_server(path: &Path, name: &str) -> bool {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(_) => return false,
    };
    let config = match agentic_json::parse(&text) {
        Ok(config) => config,
        Err(_) => return false,
    };
    config
        .as_object()
        .and_then(|object| object.get("mcp"))
        .and_then(|mcp| mcp.as_object())
        .and_then(|mcp| mcp.get("servers"))
        .and_then(|servers| servers.as_object())
        .is_some_and(|servers| servers.contains_key(name))
}

/// The OS capability surface, registered once as a global MCP server so every
/// agent session gets it. Values pass through from our own environment at
/// spawn time, so the principal is per-invocation, not baked into config.
fn ensure_mcp(
    home: &Path,
    server: &str,
    principal: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let agentd = std::env::current_exe()
        .map(|exe| exe.display().to_string())
        .unwrap_or_else(|_| "agentd".to_string());
    // The global config file wins over the live listing: firstboot seeds a
    // concrete tiaki entry for T3-spawned servers, and `mcp add` would
    // replace it with {env:} references that go empty under T3. Never
    // clobber an entry that is already there.
    if global_config_has_server(MCP_SERVER_NAME) {
        return Ok(());
    }
    let list = run_opencode(server, home, principal, &["mcp", "list"], None)?;
    if list.contains(MCP_SERVER_NAME) {
        return Ok(());
    }
    let status = Command::new("opencode")
        .args(["mcp", "add", "--global", MCP_SERVER_NAME])
        .args(["--env", "TIAKI_HOME={env:TIAKI_HOME}"])
        .args(["--env", "TIAKI_PRINCIPAL={env:TIAKI_PRINCIPAL}"])
        .args(["--env", "TIAKI_WORKSPACE={env:TIAKI_WORKSPACE}"])
        .arg("--")
        .arg(&agentd)
        .args(["mcp", "serve"])
        .env(
            "OPENCODE_SERVER_PASSWORD",
            server_password(home).unwrap_or_default(),
        )
        .env(
            "OPENCODE_PASSWORD",
            server_password(home).unwrap_or_default(),
        )
        .env("TIAKI_HOME", home)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| format!("cannot register MCP server: {error}"))?;
    if !status.success() {
        return Err("opencode mcp add failed".into());
    }
    Ok(())
}

fn opencode_session(
    server: &str,
    home: &Path,
    principal: &str,
    args: &[&str],
) -> Result<String, Box<dyn std::error::Error>> {
    // Project-scoped commands (list, export) resolve the project from the
    // caller's directory, so run them from the workspace: the same project
    // the server was started in.
    let mut command = vec!["session".to_string()];
    command.extend(args.iter().map(|arg| arg.to_string()));
    run_opencode_in(server, home, principal, &command, None, &workspace(home))
}

fn opencode_run(
    server: &str,
    home: &Path,
    principal: &str,
    full: bool,
    title: &str,
    resume: Option<String>,
    message: &str,
) -> Result<RunOutcome, Box<dyn std::error::Error>> {
    let mut argv: Vec<String> = vec![
        "run".into(),
        "--server".into(),
        server.into(),
        "--format".into(),
        "json".into(),
        "--title".into(),
        title.into(),
    ];
    if let Some(session) = resume {
        argv.push("--session".into());
        argv.push(session);
    }
    if full {
        argv.push("--auto".into());
    }
    argv.push("--".into());
    argv.push(message.into());
    let output = run_opencode(server, home, principal, &argv, Some(RUN_TIMEOUT))?;
    parse_run_output(&output)
}

fn run_opencode(
    server: &str,
    home: &Path,
    principal: &str,
    argv: &[impl AsRef<str>],
    timeout: Option<Duration>,
) -> Result<String, Box<dyn std::error::Error>> {
    let workdir = workspace(home);
    if workdir.is_dir() {
        run_opencode_in(server, home, principal, argv, timeout, &workdir)
    } else {
        run_opencode_in(server, home, principal, argv, timeout, home)
    }
}

fn run_opencode_in(
    server: &str,
    home: &Path,
    principal: &str,
    argv: &[impl AsRef<str>],
    timeout: Option<Duration>,
    cwd: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    let argv: Vec<String> = argv.iter().map(|arg| arg.as_ref().to_string()).collect();
    let mut child = Command::new("opencode")
        .args(["--server", server])
        .arg(&argv[0])
        .args(&argv[1..])
        .current_dir(cwd)
        .env(
            "OPENCODE_SERVER_PASSWORD",
            server_password(home).unwrap_or_default(),
        )
        .env(
            "OPENCODE_PASSWORD",
            server_password(home).unwrap_or_default(),
        )
        .env("TIAKI_HOME", home)
        .env("TIAKI_PRINCIPAL", principal)
        .env("TIAKI_WORKSPACE", workspace(home).display().to_string())
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| format!("cannot start opencode: {error}"))?;
    let stdout = child.stdout.take().ok_or("no stdout pipe")?;
    // Drain on its own thread: a long run exceeds the pipe buffer and would
    // block the child while we wait for its exit.
    let collected = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let sink = std::sync::Arc::clone(&collected);
    let drainer = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            match line {
                Ok(line) => {
                    sink.lock().expect("log lock").push_str(&line);
                    sink.lock().expect("log lock").push('\n');
                }
                Err(_) => break,
            }
        }
    });
    let deadline = timeout.map(|timeout| Instant::now() + timeout);
    loop {
        if let Some(deadline) = deadline {
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                let _ = drainer.join();
                return Err("opencode run timed out".into());
            }
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let _ = drainer.join();
                let output = collected.lock().expect("log lock").clone();
                if !status.success() {
                    return Err(format!("opencode exited with {status}").into());
                }
                return Ok(output);
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(200)),
            Err(error) => return Err(format!("wait failed: {error}").into()),
        }
    }
}

/// The NDJSON stream `opencode run --format json` prints: one event per line,
/// the session id on every event, text in `part.text`, the bill in the final
/// `step_finish`. Unknown event types go to stderr, never silently dropped.
fn parse_run_output(output: &str) -> Result<RunOutcome, Box<dyn std::error::Error>> {
    let mut session = String::new();
    let mut answer = String::new();
    let mut reason = "unknown".to_string();
    let mut tokens_in = 0;
    let mut tokens_out = 0;
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let event = agentic_json::parse(trimmed)
            .map_err(|error| format!("unparsable opencode event: {error}"))?;
        let object = event.as_object().ok_or("opencode event is not an object")?;
        if let Some(id) = object.get("sessionID").and_then(Json::as_str) {
            if session.is_empty() {
                session = id.to_string();
            }
        }
        match object.get("type").and_then(Json::as_str).unwrap_or("") {
            "text" => {
                if let Some(text) = object
                    .get("part")
                    .and_then(Json::as_object)
                    .and_then(|part| part.get("text"))
                    .and_then(Json::as_str)
                {
                    // One event per message on this stream; a newline keeps
                    // them readable instead of jammed end to end.
                    if !answer.is_empty() {
                        answer.push('\n');
                    }
                    answer.push_str(text);
                }
            }
            "step_finish" => {
                if let Some(part) = object.get("part").and_then(Json::as_object) {
                    if let Some(value) = part.get("reason").and_then(Json::as_str) {
                        reason = value.to_string();
                    }
                    if let Some(tokens) = part.get("tokens").and_then(Json::as_object) {
                        tokens_in =
                            tokens.get("input").and_then(Json::as_f64).unwrap_or(0.0) as u64;
                        tokens_out =
                            tokens.get("output").and_then(Json::as_f64).unwrap_or(0.0) as u64;
                    }
                }
            }
            "error" => {
                let message = object
                    .get("error")
                    .and_then(Json::as_object)
                    .and_then(|error| error.get("message"))
                    .and_then(Json::as_str)
                    .unwrap_or("unknown error");
                return Err(format!("opencode error: {message}").into());
            }
            other => {
                eprintln!("[agent] ignored event '{other}'");
            }
        }
    }
    if session.is_empty() {
        return Err("opencode produced no session".into());
    }
    Ok(RunOutcome {
        session,
        answer: answer.trim().to_string(),
        reason,
        tokens_in,
        tokens_out,
    })
}

fn first_line(text: &str) -> String {
    const LIMIT: usize = 120;
    let line = text.lines().next().unwrap_or("").trim();
    if line.len() > LIMIT {
        format!("{}…", &line[..LIMIT])
    } else {
        line.to_string()
    }
}

fn record(
    engine: &mut Ledger,
    global: &Global,
    full: bool,
    intent: &str,
    summary: String,
    status: Status,
) {
    let transaction_id = format!("agent_{:x}", now_nanos());
    let _ = engine.record(
        &effective_principal(&global.principal),
        if full {
            AccessMode::Full
        } else {
            AccessMode::Guarded
        },
        SandboxClass::UserWrite,
        &transaction_id,
        intent,
        Summary::new(summary, String::new(), intent, true),
        Impact::none(),
        status,
        vec!["agent.run".into()],
    );
}

#[cfg(test)]
mod tests {
    use super::{config_file_has_server, now_nanos};

    fn write_config(dir: &std::path::Path, text: &str) -> std::path::PathBuf {
        let path = dir.join(format!("opencode-{}.json", now_nanos()));
        std::fs::write(&path, text).expect("write config");
        path
    }

    #[test]
    fn seeded_entry_counts_as_present() {
        let dir = std::env::temp_dir();
        let path = write_config(
            &dir,
            r#"{"mcp":{"servers":{"tiaki":{"type":"local","command":["agentd","mcp","serve"]}}}}"#,
        );
        assert!(config_file_has_server(&path, "tiaki"));
        assert!(!config_file_has_server(&path, "other"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_or_broken_config_counts_as_absent() {
        let dir = std::env::temp_dir();
        let missing = dir.join("opencode-definitely-missing.json");
        let _ = std::fs::remove_file(&missing);
        assert!(!config_file_has_server(&missing, "tiaki"));
        let bad = write_config(&dir, "not json");
        assert!(!config_file_has_server(&bad, "tiaki"));
        let _ = std::fs::remove_file(&bad);
        let empty = write_config(&dir, "{}");
        assert!(!config_file_has_server(&empty, "tiaki"));
        let _ = std::fs::remove_file(&empty);
    }
}
