//! Minimal ACP client: JSON-RPC over stdio, deny-by-default permissions.
//!
//! The client speaks `initialize`, `session/new`, `session/prompt`, and
//! `session/cancel`, collects `agent_message_chunk` text, and answers every
//! `session/request_permission` with a rejection (or `cancelled` when nothing
//! reject-like is offered). It declares no filesystem, terminal, or
//! elicitation capabilities, so the agent has nothing to ask for and no
//! machine to touch — it reasons from the prompt text only.
//!
//! One request is in flight at a time. A reader thread feeds lines into a
//! channel so prompt turns can time out instead of hanging forever.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use agentic_json::Json;

const PROTOCOL_VERSION: u64 = 1;
const CLIENT_NAME: &str = "tiaki-acp-bridge";

#[derive(Debug)]
pub enum AcpError {
    Spawn(String),
    Io(String),
    Parse(String),
    Protocol(String),
    Agent(String),
    Timeout,
}

impl std::fmt::Display for AcpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AcpError::Spawn(error) => write!(f, "could not start agent: {error}"),
            AcpError::Io(error) => write!(f, "transport error: {error}"),
            AcpError::Parse(error) => write!(f, "unparsable ACP line: {error}"),
            AcpError::Protocol(error) => write!(f, "protocol error: {error}"),
            AcpError::Agent(error) => write!(f, "agent error: {error}"),
            AcpError::Timeout => write!(f, "agent did not answer in time"),
        }
    }
}

impl std::error::Error for AcpError {}

#[derive(Debug)]
pub struct TurnOutcome {
    /// Concatenated `agent_message_chunk` text.
    pub message: String,
    /// Tool calls the agent announced (all denied).
    pub tools_seen: usize,
    /// Permission requests denied.
    pub denied: usize,
    /// The turn's stop reason (`end_turn` on the happy path).
    pub stop_reason: String,
}

enum Inbound {
    Response {
        id: u64,
        result: Option<Json>,
        error: Option<String>,
    },
    Request {
        id: Json,
        method: String,
        params: Json,
    },
    Notification {
        method: String,
        params: Json,
    },
    BadLine,
    Closed,
}

pub struct AcpClient {
    child: Child,
    stdin: ChildStdin,
    next_id: u64,
    incoming: Receiver<Inbound>,
    bad_lines: u64,
}

impl AcpClient {
    pub fn spawn(command: &str, args: &[String]) -> Result<Self, AcpError> {
        let mut child = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| AcpError::Spawn(error.to_string()))?;
        let stdin = child.stdin.take().ok_or_else(|| {
            AcpError::Spawn("agent stdin unavailable".to_string())
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            AcpError::Spawn("agent stdout unavailable".to_string())
        })?;
        let (sender, incoming) = mpsc::channel();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                let inbound = match line {
                    Err(_) => Inbound::Closed,
                    Ok(text) => classify(&text),
                };
                let closed = matches!(inbound, Inbound::Closed);
                if sender.send(inbound).is_err() || closed {
                    break;
                }
            }
            let _ = sender.send(Inbound::Closed);
        });
        Ok(Self {
            child,
            stdin,
            next_id: 1,
            incoming,
            bad_lines: 0,
        })
    }

    pub fn bad_lines(&self) -> u64 {
        self.bad_lines
    }

    /// `initialize` and agree on protocol v1. Anything else is a clean error,
    /// not a guess.
    pub fn initialize(&mut self) -> Result<(), AcpError> {
        let id = self.send("initialize", object(vec![
            ("protocolVersion", Json::Number(PROTOCOL_VERSION as f64)),
            ("clientCapabilities", object(vec![])),
            ("clientInfo", object(vec![
                ("name", Json::String(CLIENT_NAME.into())),
                ("version", Json::String(env!("CARGO_PKG_VERSION").into())),
            ])),
        ]))?;
        match self.wait_for(id, Duration::from_secs(120))? {
            Response::Ok(result) => {
                let version = result
                    .as_ref()
                    .and_then(|value| value.as_object())
                    .and_then(|object| object.get("protocolVersion"))
                    .and_then(Json::as_f64)
                    .unwrap_or(0.0);
                if version as u64 != PROTOCOL_VERSION {
                    return Err(AcpError::Protocol(format!(
                        "agent speaks protocol version {version}, this bridge speaks {PROTOCOL_VERSION}"
                    )));
                }
                Ok(())
            }
            Response::Err(message) => Err(AcpError::Agent(format!("initialize: {message}"))),
        }
    }

    pub fn new_session(&mut self, cwd: &str, mcp_servers: Json) -> Result<String, AcpError> {
        let id = self.send("session/new", object(vec![
            ("cwd", Json::String(cwd.into())),
            ("mcpServers", mcp_servers),
        ]))?;
        match self.wait_for(id, Duration::from_secs(120))? {
            Response::Ok(result) => result
                .as_ref()
                .and_then(|value| value.as_object())
                .and_then(|object| object.get("sessionId"))
                .and_then(Json::as_str)
                .map(str::to_string)
                .ok_or_else(|| AcpError::Protocol("session/new has no sessionId".into())),
            Response::Err(message) => Err(AcpError::Agent(format!("session/new: {message}"))),
        }
    }

    /// One prompt turn: collect the answer, deny every permission request,
    /// give up loudly on timeout (after asking the agent to cancel first).
    pub fn prompt_turn(
        &mut self,
        session_id: &str,
        text: &str,
        timeout: Duration,
    ) -> Result<TurnOutcome, AcpError> {
        let id = self.send("session/prompt", object(vec![
            ("sessionId", Json::String(session_id.into())),
            ("prompt", Json::Array(vec![object(vec![
                ("type", Json::String("text".into())),
                ("text", Json::String(text.into())),
            ])])),
        ]))?;
        let deadline = Instant::now() + timeout;
        let mut message = String::new();
        let mut tools_seen = 0;
        let mut denied = 0;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            // The turn's own read times out first so `session/cancel` still
            // gets its answer below instead of hanging with it.
            let inbound = self.next(remaining)?;
            match inbound {
                Inbound::Notification { method, params } => {
                    if method == "session/update" {
                        self.absorb_update(&params, &mut message, &mut tools_seen);
                    }
                }
                Inbound::Request { id, method, params } => {
                    if method == "session/request_permission" {
                        denied += 1;
                        let response = deny(&params);
                        self.reply(id, response);
                    } else {
                        self.reply(id, error_reply(-32601, "method not supported"));
                    }
                }
                Inbound::Response { id: rid, result, error } => {
                    if rid != id {
                        continue;
                    }
                    if let Some(message) = error {
                        return Err(AcpError::Agent(format!("session/prompt: {message}")));
                    }
                    let stop = result
                        .as_ref()
                        .and_then(|value| value.as_object())
                        .and_then(|object| object.get("stopReason"))
                        .and_then(Json::as_str)
                        .unwrap_or("")
                        .to_string();
                    return Ok(TurnOutcome {
                        message,
                        tools_seen,
                        denied,
                        stop_reason: stop,
                    });
                }
                Inbound::BadLine => {
                    self.bad_lines += 1;
                }
                Inbound::Closed => {
                    return Err(AcpError::Io("agent closed stdout".into()));
                }
            }
            if Instant::now() >= deadline {
                let _ = self.notify("session/cancel", object(vec![
                    ("sessionId", Json::String(session_id.into())),
                ]));
                return Err(AcpError::Timeout);
            }
        }
    }

    fn absorb_update(&self, params: &Json, message: &mut String, tools_seen: &mut usize) {
        let update = params
            .as_object()
            .and_then(|object| object.get("update"))
            .and_then(Json::as_object);
        let Some(update) = update else { return };
        let kind = update
            .get("sessionUpdate")
            .and_then(Json::as_str)
            .unwrap_or("");
        match kind {
            "agent_message_chunk" => {
                if let Some(text) = update
                    .get("content")
                    .and_then(Json::as_object)
                    .and_then(|content| content.get("text"))
                    .and_then(Json::as_str)
                {
                    message.push_str(text);
                }
            }
            "agent_thought_chunk" => {
                if let Some(text) = update
                    .get("content")
                    .and_then(Json::as_object)
                    .and_then(|content| content.get("text"))
                    .and_then(Json::as_str)
                {
                    eprintln!("[acp-bridge:thought] {text}");
                }
            }
            "tool_call" => {
                *tools_seen += 1;
                if let Some(title) = update.get("title").and_then(Json::as_str) {
                    eprintln!("[acp-bridge:tool] {title}");
                }
            }
            _ => {}
        }
    }

    fn send(&mut self, method: &str, params: Json) -> Result<u64, AcpError> {
        let id = self.next_id;
        self.next_id += 1;
        let mut fields = BTreeMap::new();
        fields.insert("jsonrpc".to_string(), Json::String("2.0".into()));
        fields.insert("id".to_string(), Json::Number(id as f64));
        fields.insert("method".to_string(), Json::String(method.into()));
        fields.insert("params".to_string(), params);
        let line = Json::Object(fields).render();
        writeln!(self.stdin, "{line}")
            .and_then(|()| self.stdin.flush())
            .map_err(|error| AcpError::Io(error.to_string()))?;
        Ok(id)
    }

    fn notify(&mut self, method: &str, params: Json) -> Result<(), AcpError> {
        let mut fields = BTreeMap::new();
        fields.insert("jsonrpc".to_string(), Json::String("2.0".into()));
        fields.insert("method".to_string(), Json::String(method.into()));
        fields.insert("params".to_string(), params);
        let line = Json::Object(fields).render();
        writeln!(self.stdin, "{line}")
            .and_then(|()| self.stdin.flush())
            .map_err(|error| AcpError::Io(error.to_string()))
    }

    fn reply(&mut self, id: Json, payload: Json) {
        let mut fields = BTreeMap::new();
        fields.insert("jsonrpc".to_string(), Json::String("2.0".into()));
        fields.insert("id".to_string(), id);
        for (key, value) in payload.as_object().cloned().unwrap_or_default() {
            fields.insert(key, value);
        }
        let line = Json::Object(fields).render();
        let _ = writeln!(self.stdin, "{line}").and_then(|()| self.stdin.flush());
    }

    fn next(&mut self, timeout: Duration) -> Result<Inbound, AcpError> {
        match self.incoming.recv_timeout(timeout) {
            Ok(inbound) => Ok(inbound),
            Err(_) => Err(AcpError::Timeout),
        }
    }

    fn wait_for(&mut self, id: u64, timeout: Duration) -> Result<Response, AcpError> {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                let _ = self.notify("session/cancel", object(vec![]));
                return Err(AcpError::Timeout);
            }
            match self.next(remaining)? {
                Inbound::Response { id: rid, result, error } if rid == id => {
                    if let Some(message) = error {
                        return Ok(Response::Err(message));
                    }
                    return Ok(Response::Ok(result));
                }
                Inbound::Request { id, method, params } => {
                    if method == "session/request_permission" {
                        self.reply(id, deny(&params));
                    } else {
                        self.reply(id, error_reply(-32601, "method not supported"));
                    }
                }
                Inbound::BadLine => {
                    self.bad_lines += 1;
                }
                Inbound::Closed => {
                    return Err(AcpError::Io("agent closed stdout".into()));
                }
                _ => {}
            }
        }
    }
}

impl Drop for AcpClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

enum Response {
    Ok(Option<Json>),
    Err(String),
}

fn classify(text: &str) -> Inbound {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Inbound::BadLine;
    }
    let parsed = match agentic_json::parse(trimmed) {
        Ok(value) => value,
        Err(_) => return Inbound::BadLine,
    };
    let Some(object) = parsed.as_object() else {
        return Inbound::BadLine;
    };
    let id = object.get("id").cloned();
    let method = object.get("method").and_then(Json::as_str).map(str::to_string);
    match (id, method) {
        (Some(id), Some(method)) => Inbound::Request {
            id,
            method,
            params: object.get("params").cloned().unwrap_or(Json::Null),
        },
        (Some(Json::Number(id)), None) => {
            if let Some(error) = object.get("error") {
                let message = error
                    .as_object()
                    .and_then(|object| object.get("message"))
                    .and_then(Json::as_str)
                    .unwrap_or("unknown error")
                    .to_string();
                Inbound::Response {
                    id: id as u64,
                    result: None,
                    error: Some(message),
                }
            } else {
                Inbound::Response {
                    id: id as u64,
                    result: object.get("result").cloned(),
                    error: None,
                }
            }
        }
        (Some(_), None) => Inbound::BadLine,
        (None, Some(method)) => Inbound::Notification {
            method,
            params: object.get("params").cloned().unwrap_or(Json::Null),
        },
        (None, None) => Inbound::BadLine,
    }
}

/// Deny a permission request: prefer an explicit reject option, otherwise
/// answer `cancelled`. Never an allow — the bridge has nothing it may grant.
fn deny(params: &Json) -> Json {
    let empty: &[Json] = &[];
    let options = params
        .as_object()
        .and_then(|object| object.get("options"))
        .and_then(Json::as_array)
        .unwrap_or(empty);
    let mut fallback: Option<&str> = None;
    for option in options {
        let object = option.as_object();
        let id = object
            .and_then(|object| object.get("optionId"))
            .and_then(Json::as_str);
        let kind = object
            .and_then(|object| object.get("kind"))
            .and_then(Json::as_str)
            .unwrap_or("");
        let Some(id) = id else { continue };
        if kind.contains("reject") {
            if kind.contains("once") {
                return selected(id);
            }
            if fallback.is_none() {
                fallback = Some(id);
            }
        }
    }
    if let Some(id) = fallback {
        return selected(id);
    }
    object(vec![(
        "outcome",
        object(vec![("outcome", Json::String("cancelled".into()))]),
    )])
}

fn selected(option_id: &str) -> Json {
    object(vec![(
        "outcome",
        object(vec![
            ("outcome", Json::String("selected".into())),
            ("optionId", Json::String(option_id.into())),
        ]),
    )])
}

fn error_reply(code: i64, message: &str) -> Json {
    object(vec![(
        "error",
        object(vec![
            ("code", Json::Number(code as f64)),
            ("message", Json::String(message.into())),
        ]),
    )])
}

fn object(fields: Vec<(&str, Json)>) -> Json {
    Json::Object(
        fields
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect::<BTreeMap<_, _>>(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_responses_notifications_and_requests() {
        let response =
            classify(r#"{"jsonrpc":"2.0","id":3,"result":{"sessionId":"s"}}"#);
        assert!(matches!(response, Inbound::Response { id: 3, .. }));

        let error =
            classify(r#"{"jsonrpc":"2.0","id":4,"error":{"code":-32600,"message":"bad"}}"#);
        assert!(matches!(
            error,
            Inbound::Response { id: 4, error: Some(_), .. }
        ));

        let notification = classify(
            r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s"}}"#,
        );
        assert!(matches!(notification, Inbound::Notification { .. }));

        let request = classify(
            r#"{"jsonrpc":"2.0","id":5,"method":"session/request_permission","params":{}}"#,
        );
        assert!(matches!(request, Inbound::Request { .. }));

        assert!(matches!(classify(""), Inbound::BadLine));
        assert!(matches!(classify("not json"), Inbound::BadLine));
    }

    #[test]
    fn deny_prefers_explicit_reject_once() {
        let params = agentic_json::parse(
            r#"{"options":[
                {"optionId":"allow-once","name":"Allow once","kind":"allow_once"},
                {"optionId":"reject-once","name":"Reject","kind":"reject_once"}
            ]}"#,
        )
        .unwrap();
        let reply = deny(&params);
        let outcome = reply
            .as_object()
            .unwrap()
            .get("outcome")
            .unwrap()
            .as_object()
            .unwrap();
        assert_eq!(
            outcome.get("optionId").and_then(Json::as_str),
            Some("reject-once")
        );
    }

    #[test]
    fn deny_without_reject_option_cancels() {
        let params =
            agentic_json::parse(r#"{"options":[{"optionId":"a","name":"A","kind":"allow_once"}]}"#)
                .unwrap();
        let reply = deny(&params);
        let outcome = reply
            .as_object()
            .unwrap()
            .get("outcome")
            .unwrap()
            .as_object()
            .unwrap();
        assert_eq!(
            outcome.get("outcome").and_then(Json::as_str),
            Some("cancelled")
        );
        assert!(outcome.get("optionId").is_none());
    }

    #[test]
    fn deny_without_options_cancels() {
        let reply = deny(&Json::Object(BTreeMap::new()));
        let outcome = reply
            .as_object()
            .unwrap()
            .get("outcome")
            .unwrap()
            .as_object()
            .unwrap();
        assert_eq!(
            outcome.get("outcome").and_then(Json::as_str),
            Some("cancelled")
        );
    }
}
