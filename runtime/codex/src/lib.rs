//! codex runtime adapter.
//!
//! A descriptor and tool set for an external runtime that speaks
//! `tiaki-abi/1`.

use agentic_json::Json;
use agentic_runtime_abi::{Risk, RuntimeSpec, ToolSpec, PROTOCOL};

pub const ID: &str = "codex";

pub fn spec() -> RuntimeSpec {
    RuntimeSpec {
        id: ID.into(),
        protocol: PROTOCOL.into(),
        transport: "unix:///run/tiaki/runtime/codex.sock".into(),
        tools: vec!["system".into(), "files".into(), "computer-use".into()],
    }
}

pub fn tools() -> Vec<ToolSpec> {
    vec![
        ToolSpec::new(
            "system.state.get",
            "Read a state path.",
            schema(
                r#"{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}"#,
            ),
            Risk::Low,
            true,
        ),
        ToolSpec::new(
            "system.state.set",
            "Write a state path as a transaction.",
            schema(
                r#"{"type":"object","properties":{"path":{"type":"string"},"value":{"type":"string"}},"required":["path","value"]}"#,
            ),
            Risk::Medium,
            true,
        ),
        ToolSpec::new(
            "computer.observe",
            "Read a structured app tree.",
            schema(
                r#"{"type":"object","properties":{"app":{"type":"string"}},"required":["app"]}"#,
            ),
            Risk::Low,
            true,
        ),
        ToolSpec::new(
            "computer.act",
            "Invoke a structured action on an app node.",
            schema(
                r#"{"type":"object","properties":{"app":{"type":"string"},"node":{"type":"string"},"action":{"type":"string"}},"required":["app","node","action"]}"#,
            ),
            Risk::High,
            false,
        ),
    ]
}

fn schema(text: &str) -> Json {
    agentic_json::parse(text).expect("static tool schema")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_is_valid() {
        let spec = spec();
        assert_eq!(spec.id, ID);
        spec.validate().expect("valid");
        assert_eq!(tools().len(), 4);
    }
}
