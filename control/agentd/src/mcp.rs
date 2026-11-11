//! `agentd mcp serve`: the OS capability surface as an MCP server.
//!
//! This is the replacement for the invented `tiaki-abi/1` wire protocol. An
//! agent speaks ACP to the OS and the OS hands it MCP servers; this is that
//! server. Every tool here is a real capability — files, terminal, the UI
//! tree — not a permission dialog. Safety is the sandbox plus the fact that
//! every call is recorded as a reversible transaction, not a leash.
//!
//! Protocol: MCP over stdio, newline-delimited JSON-RPC 2.0. Only JSON-RPC
//! goes to stdout; diagnostics go to stderr.
//!
//! Tools:
//!   ui.observe      read the current app tree
//!   ui.act          act on a node (toggle / invoke / set-text / select / ...)
//!   fs.list         list a directory
//!   fs.read         read a file
//!   fs.write        write a file (previous content backed up)
//!   terminal.exec   run a command and capture its output
//!
//! Access: full by default. In guarded mode the destructive tools refuse with
//! an explicit error instead of pretending to ask a human who is not there.

use std::collections::BTreeMap;
use std::io::{self, BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use agentic_core_log::{Impact, Status, Summary};
use agentic_json::Json;
use agentic_runtime_abi::{ActionKind, AppAction, AppBackend, SimulatedApp};
use agentic_transaction::{AccessMode, SandboxClass};

use crate::{now_nanos, Global, Ledger};

const PROTOCOL_VERSION: &str = "2025-06-18";
const SERVER_NAME: &str = "tiaki";
const MAX_READ: usize = 1 << 20;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

pub(crate) fn serve(
    home: &Path,
    global: &Global,
    engine: &mut Ledger,
) -> Result<(), Box<dyn std::error::Error>> {
    let workspace = std::env::var("TIAKI_WORKSPACE")
        .ok()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join("workspace"));
    std::fs::create_dir_all(&workspace)?;
    let mode = if global.mode_explicit {
        global.mode
    } else {
        // An agent is not a prisoner. Full access unless a human says guarded.
        AccessMode::Full
    };

    let mut server = Server {
        global,
        engine,
        app: SimulatedApp::checkout(),
        workspace,
        home: home.to_path_buf(),
        mode,
    };

    eprintln!(
        "[agentd mcp] serving {} as {} in {:?} mode",
        SERVER_NAME,
        global.principal,
        mode.as_str()
    );

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(_) => break,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let response = match agentic_json::parse(trimmed) {
            Ok(request) => server.handle(&request),
            Err(error) => Some(error_reply(
                Json::Null,
                -32700,
                &format!("parse error: {error}"),
            )),
        };
        if let Some(response) = response {
            writeln!(out, "{}", response.render())?;
            out.flush()?;
        }
    }
    Ok(())
}

struct Server<'a> {
    global: &'a Global,
    engine: &'a mut Ledger,
    app: SimulatedApp,
    workspace: PathBuf,
    home: PathBuf,
    mode: AccessMode,
}

impl Server<'_> {
    fn handle(&mut self, request: &Json) -> Option<Json> {
        let request_object = request.as_object()?;
        let id = request_object.get("id").cloned();
        let method = request_object.get("method").and_then(Json::as_str)?;
        let params = request_object.get("params").cloned().unwrap_or(Json::Null);

        // Notifications have no id and take no reply.
        let id = id?;
        let result = match method {
            "initialize" => Some(self.initialize(&params)),
            "ping" => Some(Json::Object(BTreeMap::new())),
            "tools/list" => Some(self.tools_list()),
            "tools/call" => Some(self.tools_call(&params)),
            "resources/list" => Some(object(vec![("resources", Json::Array(vec![]))])),
            "prompts/list" => Some(object(vec![("prompts", Json::Array(vec![]))])),
            other => {
                return Some(error_reply(
                    id,
                    -32601,
                    &format!("method '{other}' is not supported"),
                ))
            }
        };
        result.map(|result| ok_reply(id, result))
    }

    fn initialize(&self, params: &Json) -> Json {
        // Echo the client's version when it names one: ACP agents negotiate
        // across several MCP revisions and this server is version-agnostic.
        let version = params
            .as_object()
            .and_then(|object| object.get("protocolVersion"))
            .and_then(Json::as_str)
            .unwrap_or(PROTOCOL_VERSION)
            .to_string();
        object(vec![
            ("protocolVersion", Json::String(version)),
            (
                "capabilities",
                object(vec![("tools", object(vec![(
                    "listChanged",
                    Json::Bool(false),
                )]))]),
            ),
            (
                "serverInfo",
                object(vec![
                    ("name", Json::String(SERVER_NAME.into())),
                    ("version", Json::String(env!("CARGO_PKG_VERSION").into())),
                ]),
            ),
        ])
    }

    fn tools_list(&self) -> Json {
        object(vec![("tools", Json::Array(self.tools()))])
    }

    fn tools(&self) -> Vec<Json> {
        vec![
            tool(
                "ui.observe",
                "Read the current application UI tree. Returns named nodes with \
                 role, label, value, enabled state, allowed actions, and risk.",
                schema(vec![], vec![]),
            ),
            tool(
                "ui.act",
                "Act on a UI node by id. The node must declare the action. \
                 Example: node 'gift', action 'toggle'.",
                schema(
                    vec![
                        ("node", string_schema("The node id to act on.")),
                        (
                            "action",
                            enum_schema(
                                "The action to perform.",
                                &[
                                    "invoke", "toggle", "set-text", "select", "focus", "scroll",
                                ],
                            ),
                        ),
                        ("value", string_schema("Value for set-text/select.")),
                    ],
                    vec!["node", "action"],
                ),
            ),
            tool(
                "fs.list",
                "List a directory. Relative paths resolve under the agent workspace.",
                schema(
                    vec![("path", string_schema("Directory path. Defaults to '.'."))],
                    vec![],
                ),
            ),
            tool(
                "fs.read",
                "Read a UTF-8 file (up to 1 MiB). Relative paths resolve under the \
                 agent workspace.",
                schema(
                    vec![("path", string_schema("File path."))],
                    vec!["path"],
                ),
            ),
            tool(
                "fs.write",
                "Write a file, creating parent directories. The previous content is \
                 backed up so the write can be reverted.",
                schema(
                    vec![
                        ("path", string_schema("File path.")),
                        ("content", string_schema("Full new contents.")),
                    ],
                    vec!["path", "content"],
                ),
            ),
            tool(
                "terminal.exec",
                "Run a shell command in the workspace and return exit code, stdout, \
                 and stderr.",
                schema(
                    vec![
                        ("command", string_schema("Command line, run with sh -c.")),
                        (
                            "timeout_ms",
                            number_schema("Timeout in milliseconds. Default 120000."),
                        ),
                    ],
                    vec!["command"],
                ),
            ),
        ]
    }

    fn tools_call(&mut self, params: &Json) -> Json {
        let object = params.as_object();
        let name = object
            .and_then(|object| object.get("name"))
            .and_then(Json::as_str)
            .unwrap_or("");
        let arguments = object
            .and_then(|object| object.get("arguments"))
            .cloned()
            .unwrap_or(Json::Null);
        match name {
            "ui.observe" => self.ui_observe(),
            "ui.act" => self.ui_act(&arguments),
            "fs.list" => self.fs_list(&arguments),
            "fs.read" => self.fs_read(&arguments),
            "fs.write" => self.fs_write(&arguments),
            "terminal.exec" => self.terminal_exec(&arguments),
            other => text_error(format!("unknown tool '{other}'")),
        }
    }

    fn ui_observe(&mut self) -> Json {
        let tree = self.app.tree();
        self.record(
            "ui.observe",
            format!("Read the '{}' UI tree.", tree.app),
            SandboxClass::Inspect,
            Status::Committed,
        );
        text_ok(tree.to_json().render())
    }

    fn ui_act(&mut self, arguments: &Json) -> Json {
        let node = string_arg(arguments, "node");
        let action_name = string_arg(arguments, "action");
        let value = string_arg(arguments, "value");
        let Some(node) = node else {
            return text_error("ui.act needs 'node'".to_string());
        };
        let Some(action_name) = action_name else {
            return text_error("ui.act needs 'action'".to_string());
        };
        let Some(kind) = ActionKind::parse(&action_name) else {
            return text_error(format!("unknown action '{action_name}'"));
        };

        let tree = self.app.tree();
        let Some(target) = tree.find(&node).cloned() else {
            return text_error(format!("no node '{node}'"));
        };
        if !target.enabled {
            return text_error(format!("node '{node}' is disabled"));
        }
        if !target.actions.contains(&kind) {
            return text_error(format!(
                "node '{node}' does not support '{action_name}' (allowed: {})",
                target
                    .actions
                    .iter()
                    .map(|action| action.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        let action = AppAction {
            node: node.clone(),
            kind,
            value: value.clone(),
        };
        match self.app.act(&action) {
            Ok(message) => {
                self.record(
                    "ui.act",
                    format!("{} on '{}': {}", action_name, node, message),
                    SandboxClass::UserWrite,
                    Status::Committed,
                );
                text_ok(format!("{node}: {message}"))
            }
            Err(error) => {
                self.record(
                    "ui.act",
                    format!("{} on '{}' failed: {}", action_name, node, error),
                    SandboxClass::UserWrite,
                    Status::Failed,
                );
                text_error(error.to_string())
            }
        }
    }

    fn fs_list(&mut self, arguments: &Json) -> Json {
        let path = self.resolve(string_arg(arguments, "path").unwrap_or_else(|| ".".into()));
        let entries = match std::fs::read_dir(&path) {
            Ok(entries) => entries,
            Err(error) => return text_error(format!("cannot list {}: {error}", path.display())),
        };
        let mut lines = Vec::new();
        for entry in entries.flatten() {
            let kind = entry
                .file_type()
                .map(|kind| {
                    if kind.is_dir() {
                        "dir"
                    } else if kind.is_symlink() {
                        "link"
                    } else {
                        "file"
                    }
                })
                .unwrap_or("?");
            let size = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
            lines.push(format!(
                "{kind}\t{size}\t{}",
                entry.file_name().to_string_lossy()
            ));
        }
        lines.sort();
        self.record(
            "fs.list",
            format!("Listed {} ({} entries).", path.display(), lines.len()),
            SandboxClass::Inspect,
            Status::Committed,
        );
        text_ok(lines.join("\n"))
    }

    fn fs_read(&mut self, arguments: &Json) -> Json {
        let Some(raw) = string_arg(arguments, "path") else {
            return text_error("fs.read needs 'path'".to_string());
        };
        let path = self.resolve(raw);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => return text_error(format!("cannot read {}: {error}", path.display())),
        };
        let truncated = bytes.len() > MAX_READ;
        let slice = &bytes[..bytes.len().min(MAX_READ)];
        self.record(
            "fs.read",
            format!(
                "Read {} ({} bytes{}).",
                path.display(),
                bytes.len(),
                if truncated { ", truncated" } else { "" }
            ),
            SandboxClass::Inspect,
            Status::Committed,
        );
        let mut text = String::from_utf8_lossy(slice).to_string();
        if truncated {
            text.push_str("\n[truncated]");
        }
        text_ok(text)
    }

    fn fs_write(&mut self, arguments: &Json) -> Json {
        if let Err(error) = self.require_full("fs.write") {
            return text_error(error);
        }
        let Some(raw) = string_arg(arguments, "path") else {
            return text_error("fs.write needs 'path'".to_string());
        };
        let Some(content) = string_arg(arguments, "content") else {
            return text_error("fs.write needs 'content'".to_string());
        };
        let path = self.resolve(raw);
        if let Some(parent) = path.parent() {
            if let Err(error) = std::fs::create_dir_all(parent) {
                return text_error(format!("cannot create {}: {error}", parent.display()));
            }
        }
        let txid = format!("mcp_{:x}", now_nanos());
        let backup = if path.exists() {
            let backup_dir = self.home.join("backups").join(&txid);
            let escaped = path.to_string_lossy().replace('/', "_");
            if let Err(error) = std::fs::create_dir_all(&backup_dir) {
                return text_error(format!("cannot create backup dir: {error}"));
            }
            let backup = backup_dir.join(escaped);
            if let Err(error) = std::fs::copy(&path, &backup) {
                return text_error(format!("cannot back up {}: {error}", path.display()));
            }
            Some(backup)
        } else {
            None
        };
        if let Err(error) = std::fs::write(&path, content.as_bytes()) {
            self.record(
                "fs.write",
                format!("Write {} failed: {error}", path.display()),
                SandboxClass::UserWrite,
                Status::Failed,
            );
            return text_error(format!("cannot write {}: {error}", path.display()));
        }
        let summary = match &backup {
            Some(backup) => format!(
                "Wrote {} bytes to {} (backup {}).",
                content.len(),
                path.display(),
                backup.display()
            ),
            None => format!("Created {} ({} bytes).", path.display(), content.len()),
        };
        self.record("fs.write", summary.clone(), SandboxClass::UserWrite, Status::Committed);
        text_ok(summary)
    }

    fn terminal_exec(&mut self, arguments: &Json) -> Json {
        if let Err(error) = self.require_full("terminal.exec") {
            return text_error(error);
        }
        let Some(command) = string_arg(arguments, "command") else {
            return text_error("terminal.exec needs 'command'".to_string());
        };
        let timeout = arguments
            .as_object()
            .and_then(|object| object.get("timeout_ms"))
            .and_then(Json::as_f64)
            .map(|ms| Duration::from_millis(ms.max(1.0) as u64))
            .unwrap_or(DEFAULT_TIMEOUT);
        match run_command(&command, &self.workspace, timeout) {
            Ok(output) => {
                let first = output.lines().next().unwrap_or("").to_string();
                self.record(
                    "terminal.exec",
                    format!("Ran `{command}`: {first}"),
                    SandboxClass::UserWrite,
                    Status::Committed,
                );
                text_ok(output)
            }
            Err(error) => {
                self.record(
                    "terminal.exec",
                    format!("Run `{command}` failed: {error}"),
                    SandboxClass::UserWrite,
                    Status::Failed,
                );
                text_error(error)
            }
        }
    }

    fn resolve(&self, raw: String) -> PathBuf {
        let path = PathBuf::from(&raw);
        if path.is_absolute() {
            path
        } else {
            self.workspace.join(path)
        }
    }

    fn require_full(&self, tool: &str) -> Result<(), String> {
        if self.mode == AccessMode::Full {
            Ok(())
        } else {
            Err(format!(
                "{tool} is refused in guarded mode; start the server with --mode full to allow it"
            ))
        }
    }

    fn record(&mut self, tool: &str, summary: String, sandbox: SandboxClass, status: Status) {
        let transaction_id = format!("mcp_{:x}", now_nanos());
        let _ = self.engine.record(
            &self.global.principal,
            self.mode,
            sandbox,
            &transaction_id,
            tool,
            Summary::new(summary, String::new(), "tool-call", true),
            Impact::none(),
            status,
            vec![format!("mcp.{tool}")],
        );
    }
}

fn run_command(command: &str, cwd: &Path, timeout: Duration) -> Result<String, String> {
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("cannot start command: {error}"))?;
    let mut stdout = child.stdout.take().ok_or("no stdout pipe")?;
    let mut stderr = child.stderr.take().ok_or("no stderr pipe")?;
    let out_reader = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = stdout.read_to_end(&mut buffer);
        buffer
    });
    let err_reader = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = stderr.read_to_end(&mut buffer);
        buffer
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(error) => return Err(format!("wait failed: {error}")),
        }
    };
    let out = out_reader.join().unwrap_or_default();
    let err = err_reader.join().unwrap_or_default();

    let mut text = String::new();
    match status {
        Some(status) => text.push_str(&format!("exit {}\n", status.code().unwrap_or(-1))),
        None => text.push_str("timed out\n"),
    }
    text.push_str(&String::from_utf8_lossy(&out));
    if !err.is_empty() {
        text.push_str("--- stderr ---\n");
        text.push_str(&String::from_utf8_lossy(&err));
    }
    Ok(text)
}

fn string_arg(arguments: &Json, key: &str) -> Option<String> {
    arguments
        .as_object()
        .and_then(|object| object.get(key))
        .and_then(Json::as_str)
        .map(str::to_string)
}

fn ok_reply(id: Json, result: Json) -> Json {
    object(vec![
        ("jsonrpc", Json::String("2.0".into())),
        ("id", id),
        ("result", result),
    ])
}

fn error_reply(id: Json, code: i64, message: &str) -> Json {
    object(vec![
        ("jsonrpc", Json::String("2.0".into())),
        ("id", id),
        (
            "error",
            object(vec![
                ("code", Json::Number(code as f64)),
                ("message", Json::String(message.into())),
            ]),
        ),
    ])
}

fn text_ok(text: impl Into<String>) -> Json {
    object(vec![
        (
            "content",
            Json::Array(vec![object(vec![
                ("type", Json::String("text".into())),
                ("text", Json::String(text.into())),
            ])]),
        ),
        ("isError", Json::Bool(false)),
    ])
}

fn text_error(text: impl Into<String>) -> Json {
    object(vec![
        (
            "content",
            Json::Array(vec![object(vec![
                ("type", Json::String("text".into())),
                ("text", Json::String(text.into())),
            ])]),
        ),
        ("isError", Json::Bool(true)),
    ])
}

fn tool(name: &str, description: &str, input_schema: Json) -> Json {
    object(vec![
        ("name", Json::String(name.into())),
        ("description", Json::String(description.into())),
        ("inputSchema", input_schema),
    ])
}

fn schema(properties: Vec<(&str, Json)>, required: Vec<&str>) -> Json {
    object(vec![
        ("type", Json::String("object".into())),
        (
            "properties",
            Json::Object(
                properties
                    .into_iter()
                    .map(|(key, value)| (key.to_string(), value))
                    .collect(),
            ),
        ),
        (
            "required",
            Json::Array(required.into_iter().map(|key| Json::String(key.into())).collect()),
        ),
    ])
}

fn string_schema(description: &str) -> Json {
    object(vec![
        ("type", Json::String("string".into())),
        ("description", Json::String(description.into())),
    ])
}

fn number_schema(description: &str) -> Json {
    object(vec![
        ("type", Json::String("number".into())),
        ("description", Json::String(description.into())),
    ])
}

fn enum_schema(description: &str, values: &[&str]) -> Json {
    object(vec![
        ("type", Json::String("string".into())),
        ("description", Json::String(description.into())),
        (
            "enum",
            Json::Array(values.iter().map(|value| Json::String((*value).into())).collect()),
        ),
    ])
}

fn object(fields: Vec<(&str, Json)>) -> Json {
    Json::Object(
        fields
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect::<BTreeMap<_, _>>(),
    )
}
