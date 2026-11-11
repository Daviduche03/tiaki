//! `acp-run`: run one ACP agent with the OS's MCP capabilities.
//!
//! This is the direct path the ABI used to sit under. It initializes an ACP
//! session, hands the agent the `tiaki` MCP server, sends one prompt, and
//! prints what happened. No `tiaki-abi/1` envelope.
//!
//! Usage:
//!   acp-run [--principal <id>] [--prompt <text>] -- <agent command> [args...]
//!
//! Example:
//!   acp-run -- opencode acp

use std::process::ExitCode;
use std::time::Duration;

use agentic_runtime_acp::{mcp, AcpClient};

const TURN_TIMEOUT: Duration = Duration::from_secs(600);

const DEFAULT_PROMPT: &str = "\
You have the `tiaki` MCP tools. Do these in order:
1. Call `ui.observe` to read the current application tree.
2. Call `ui.act` with node `gift` and action `toggle`.
3. Call `fs.write` to write `notes/acp-proof.txt` with one line saying what the
   `gift` node's value is now.
4. Reply with a one-line summary of what you did.";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("acp-run: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let mut principal = std::env::var("TIAKI_PRINCIPAL")
        .ok()
        .filter(|p| !p.is_empty());
    let mut prompt = DEFAULT_PROMPT.to_string();

    let mut index = 0;
    while index < raw.len() {
        match raw[index].as_str() {
            "--principal" => {
                principal = Some(
                    raw.get(index + 1)
                        .ok_or("--principal needs a value")?
                        .clone(),
                );
                index += 2;
            }
            "--prompt" => {
                prompt = raw.get(index + 1).ok_or("--prompt needs a value")?.clone();
                index += 2;
            }
            "--" => {
                index += 1;
                break;
            }
            _ => break,
        }
    }

    let Some((command, rest)) = raw[index..].split_first() else {
        return Err(
            "usage: acp-run [--principal <id>] [--prompt <text>] -- <agent command> [args...]"
                .into(),
        );
    };
    let rest: Vec<String> = rest.to_vec();

    // The MCP server needs to know who it acts for; the agent itself does not.
    if let Some(principal) = &principal {
        std::env::set_var("TIAKI_PRINCIPAL", principal);
    }

    let mut agent = AcpClient::spawn(command, &rest).map_err(|error| error.to_string())?;
    agent
        .initialize()
        .map_err(|error| format!("initialize: {error}"))?;

    let cwd = std::env::current_dir()
        .map_err(|error| format!("no working directory: {error}"))?
        .display()
        .to_string();
    let servers = mcp::servers();
    let tools = servers.as_array().map(|servers| servers.len()).unwrap_or(0);
    eprintln!("[acp-run] offering {tools} MCP server(s) to the agent");
    let session = agent
        .new_session(&cwd, servers)
        .map_err(|error| format!("session/new: {error}"))?;
    eprintln!("[acp-run] ACP session {session}");

    let turn = agent
        .prompt_turn(&session, &prompt, TURN_TIMEOUT)
        .map_err(|error| format!("prompt turn: {error}"))?;

    println!("{}", turn.message.trim());
    eprintln!(
        "[acp-run] stop={} tool_calls={} denied={}",
        turn.stop_reason, turn.tools_seen, turn.denied
    );
    Ok(())
}
