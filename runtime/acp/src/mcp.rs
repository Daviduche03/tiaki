//! The OS capability server, expressed as an ACP `mcpServers` entry.
//!
//! ACP hands agents their tools by naming MCP servers in `session/new`. This
//! module builds that entry for `agentd mcp serve`, so an ACP agent gets the
//! OS's real capabilities — files, terminal, the UI tree — with no bespoke
//! protocol in between.
//!
//! Resolution order for the server binary:
//!   1. `TIAKI_MCP_SERVER` (explicit override)
//!   2. `agentd` next to the running executable
//!   3. `agentd` on `PATH`
//!
//! Arguments come from `TIAKI_MCP_ARGS` (default `mcp serve`). The principal,
//! home, and workspace are forwarded as the server's environment.

use std::collections::BTreeMap;
use std::path::Path;

use agentic_json::Json;

pub const SERVER_NAME: &str = "tiaki";
pub const DEFAULT_ARGS: &str = "mcp serve";

/// Build the `mcpServers` array for `session/new`.
pub fn servers() -> Json {
    let command = resolve_command();
    let args = std::env::var("TIAKI_MCP_ARGS")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_ARGS.to_string());
    let mut env = Vec::new();
    for key in ["TIAKI_HOME", "TIAKI_PRINCIPAL", "TIAKI_WORKSPACE"] {
        if let Ok(value) = std::env::var(key) {
            if !value.is_empty() {
                env.push((key.to_string(), value));
            }
        }
    }
    build(command, &args, env)
}

/// Pure builder: given a resolved command, the arg line, and the environment
/// to forward, produce the ACP `mcpServers` array.
pub fn build(command: Option<String>, args: &str, env: Vec<(String, String)>) -> Json {
    let Some(command) = command else {
        // Honest: no server found means the agent gets no OS capabilities.
        return Json::Array(Vec::new());
    };
    let args = Json::Array(
        args.split_whitespace()
            .map(|arg| Json::String(arg.to_string()))
            .collect(),
    );
    let env = Json::Array(
        env.into_iter()
            .map(|(name, value)| {
                object(vec![
                    ("name", Json::String(name)),
                    ("value", Json::String(value)),
                ])
            })
            .collect(),
    );
    Json::Array(vec![object(vec![
        ("name", Json::String(SERVER_NAME.to_string())),
        ("command", Json::String(command)),
        ("args", args),
        ("env", env),
    ])])
}

fn resolve_command() -> Option<String> {
    if let Ok(explicit) = std::env::var("TIAKI_MCP_SERVER") {
        if !explicit.trim().is_empty() {
            return Some(explicit);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("agentd");
            if candidate.exists() {
                return Some(candidate.display().to_string());
            }
        }
    }
    find_on_path("agentd")
}

fn find_on_path(name: &str) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = Path::new(&dir).join(name);
        if candidate.is_file() {
            return Some(candidate.display().to_string());
        }
    }
    None
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
    fn builds_a_stdio_entry_with_env() {
        let servers = build(
            Some("/opt/agentd".into()),
            "mcp serve",
            vec![("TIAKI_HOME".into(), "/tmp/home".into())],
        );
        let entry = &servers.as_array().unwrap()[0];
        let entry = entry.as_object().unwrap();
        assert_eq!(entry.get("name").and_then(Json::as_str), Some(SERVER_NAME));
        assert_eq!(
            entry.get("command").and_then(Json::as_str),
            Some("/opt/agentd")
        );
        let args: Vec<_> = entry
            .get("args")
            .and_then(Json::as_array)
            .unwrap()
            .iter()
            .filter_map(Json::as_str)
            .collect();
        assert_eq!(args, vec!["mcp", "serve"]);
        let env = entry.get("env").and_then(Json::as_array).unwrap();
        assert_eq!(env.len(), 1);
    }

    #[test]
    fn no_command_means_no_capabilities() {
        assert_eq!(
            build(None, "mcp serve", Vec::new()),
            Json::Array(Vec::new())
        );
    }
}
