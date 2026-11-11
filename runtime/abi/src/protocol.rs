//! Runtime discovery and typed tools.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use agentic_json::Json;
use agentic_toml::{self as toml, Value};

use crate::tree::Risk;

pub const PROTOCOL: &str = "tiaki-abi/1";

#[derive(Clone, Debug, PartialEq)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: Json,
    pub risk: Risk,
    pub reversible: bool,
}

impl ToolSpec {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: Json,
        risk: Risk,
        reversible: bool,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
            risk,
            reversible,
        }
    }

    pub fn to_json(&self) -> Json {
        object(vec![
            ("name", Json::String(self.name.clone())),
            ("description", Json::String(self.description.clone())),
            ("input_schema", self.input_schema.clone()),
            ("risk", Json::String(self.risk.as_str().into())),
            ("reversible", Json::Bool(self.reversible)),
        ])
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeSpec {
    pub id: String,
    pub protocol: String,
    pub transport: String,
    pub tools: Vec<String>,
}

impl RuntimeSpec {
    pub fn parse(text: &str) -> Result<Self, ProtocolError> {
        let document = toml::parse(text).map_err(|error| ProtocolError::new(error.to_string()))?;
        let table = document
            .table("runtime")
            .ok_or_else(|| ProtocolError::new("runtime discovery needs a [runtime] section"))?;

        let id = table
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| ProtocolError::new("runtime needs an 'id'"))?
            .to_string();
        let protocol = table
            .get("protocol")
            .and_then(Value::as_str)
            .ok_or_else(|| ProtocolError::new("runtime needs a 'protocol'"))?
            .to_string();
        let transport = table
            .get("transport")
            .and_then(Value::as_str)
            .ok_or_else(|| ProtocolError::new("runtime needs a 'transport'"))?
            .to_string();
        let tools = table
            .get("tools")
            .and_then(Value::as_array)
            .ok_or_else(|| ProtocolError::new("runtime needs a 'tools' array"))?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_string)
                    .ok_or_else(|| ProtocolError::new("tools must be strings"))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let spec = Self {
            id,
            protocol,
            transport,
            tools,
        };
        spec.validate()?;
        Ok(spec)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, ProtocolError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|error| {
            ProtocolError::new(format!("could not read {}: {error}", path.display()))
        })?;
        Self::parse(&text)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.id.trim().is_empty() {
            return Err(ProtocolError::new("runtime id must not be empty"));
        }
        if self.protocol != PROTOCOL {
            return Err(ProtocolError::new(format!(
                "runtime '{}' speaks '{}', but this host speaks '{PROTOCOL}'",
                self.id, self.protocol
            )));
        }
        if self.transport.trim().is_empty() {
            return Err(ProtocolError::new("runtime transport must not be empty"));
        }
        if self.tools.is_empty() {
            return Err(ProtocolError::new("runtime must declare at least one tool"));
        }
        let mut seen = std::collections::BTreeSet::new();
        for tool in &self.tools {
            if !seen.insert(tool) {
                return Err(ProtocolError::new(format!(
                    "tool '{tool}' is declared twice"
                )));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtocolError {
    pub message: String,
}

impl ProtocolError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ProtocolError {}

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
    fn parses_runtime_discovery() {
        let text = r#"
[runtime]
id = "opencode"
protocol = "tiaki-abi/1"
transport = "unix:///run/tiaki/runtime/opencode.sock"
tools = ["system", "files", "ui", "computer-use"]
"#;
        let spec = RuntimeSpec::parse(text).expect("parse");
        assert_eq!(spec.id, "opencode");
        assert_eq!(spec.tools.len(), 4);
    }

    #[test]
    fn rejects_wrong_protocol() {
        let text = r#"
[runtime]
id = "old"
protocol = "tiaki-abi/0"
transport = "stdio"
tools = ["ui"]
"#;
        assert!(RuntimeSpec::parse(text).is_err());
    }
}
