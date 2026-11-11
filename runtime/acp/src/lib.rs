//! ACP bridge: drive any Agent Client Protocol agent as an Tiaki runtime.
//!
//! The bridge is an `tiaki-abi/1` runtime on its stdio side (so `agentd
//! computer run` needs no changes) and an ACP *client* on the other: it
//! spawns the agent subprocess, runs `initialize` / `session/new` /
//! `session/prompt`, and translates the streamed answer into one `ACT` or
//! `BYE` decision per turn.
//!
//! The trust posture is deny-by-default. The bridge declares no filesystem,
//! terminal, or elicitation capabilities, and every `session/request_permission`
//! is answered with a rejection (or cancelled when nothing reject-like is
//! offered). The agent reasons from the prompt text only; it never touches the
//! machine. High-risk acts still go through the host's approval, exactly as
//! with every other runtime.

pub mod client;
pub mod decision;
pub mod mcp;

pub use client::{AcpClient, AcpError, TurnOutcome};
pub use decision::{parse_decision, Decision};

use agentic_runtime_abi::{RuntimeSpec, PROTOCOL};

/// Runtime id under which the bridge registers itself. Any ACP-speaking agent
/// — opencode, or anything else — is reachable through this one runtime.
pub const ID: &str = "acp";

/// Descriptor for `agentd runtime list`: the bridge is a process transport
/// that speaks `tiaki-abi/1` on its own stdio and ACP on the child's.
pub fn spec() -> RuntimeSpec {
    RuntimeSpec {
        id: ID.into(),
        protocol: PROTOCOL.into(),
        transport: "process://acp-bridge".into(),
        tools: vec!["computer-use".into(), "ui".into()],
    }
}
