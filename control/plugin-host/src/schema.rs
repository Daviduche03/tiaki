//! The JSON-Schema subset used by plugin config: object properties, `type`,
//! `enum`, `required`, `default`, and `additionalProperties`.

use std::collections::BTreeMap;
use std::fmt;

use agentic_transaction::Issue;

use agentic_json::{self as json, Json};

#[derive(Clone, Debug, PartialEq)]
pub struct Property {
    pub kind: Option<String>,
    pub enum_values: Option<Vec<Json>>,
    pub default: Option<Json>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConfigSchema {
    pub properties: BTreeMap<String, Property>,
    pub required: Vec<String>,
    pub additional: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaError {
    pub message: String,
}

impl fmt::Display for SchemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SchemaError {}

impl SchemaError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

pub fn parse_schema(text: &str) -> Result<ConfigSchema, SchemaError> {
    let value = json::parse(text).map_err(|error| SchemaError::new(error.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| SchemaError::new("config schema must be a JSON object"))?;

    if let Some(kind) = object.get("type").and_then(Json::as_str) {
        if kind != "object" {
            return Err(SchemaError::new("config schema type must be 'object'"));
        }
    }

    let additional = object
        .get("additionalProperties")
        .and_then(Json::as_bool)
        .unwrap_or(true);

    let required = object
        .get("required")
        .and_then(Json::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    let mut properties = BTreeMap::new();
    if let Some(raw) = object.get("properties") {
        let raw = raw
            .as_object()
            .ok_or_else(|| SchemaError::new("'properties' must be an object"))?;
        for (name, value) in raw {
            let definition = value.as_object().ok_or_else(|| {
                SchemaError::new(format!("property '{name}' must be a schema object"))
            })?;
            let enum_values = match definition.get("enum") {
                Some(value) => Some(
                    value
                        .as_array()
                        .ok_or_else(|| {
                            SchemaError::new(format!("property '{name}' enum must be an array"))
                        })?
                        .to_vec(),
                ),
                None => None,
            };
            properties.insert(
                name.clone(),
                Property {
                    kind: definition
                        .get("type")
                        .and_then(Json::as_str)
                        .map(str::to_string),
                    enum_values,
                    default: definition.get("default").cloned(),
                },
            );
        }
    }

    Ok(ConfigSchema {
        properties,
        required,
        additional,
    })
}

/// Validate a config object. Does not apply defaults.
pub fn validate(schema: &ConfigSchema, config: &BTreeMap<String, Json>) -> Vec<Issue> {
    let mut issues = Vec::new();

    for key in config.keys() {
        if !schema.properties.contains_key(key) && !schema.additional {
            issues.push(Issue::error(
                "config.unknown_key",
                Some(key.clone()),
                format!("'{key}' is not a known config key"),
            ));
        }
    }

    for key in &schema.required {
        if !config.contains_key(key) {
            issues.push(Issue::error(
                "config.missing",
                Some(key.clone()),
                format!("'{key}' is required"),
            ));
        }
    }

    for (key, value) in config {
        let Some(property) = schema.properties.get(key) else {
            continue;
        };
        if let Some(kind) = &property.kind {
            if !type_matches(kind, value) {
                issues.push(Issue::error(
                    "config.invalid_type",
                    Some(key.clone()),
                    format!("'{key}' must be a {kind}, found {}", value.type_name()),
                ));
                continue;
            }
        }
        if let Some(allowed) = &property.enum_values {
            if !allowed.iter().any(|candidate| candidate == value) {
                issues.push(Issue::error(
                    "config.not_in_enum",
                    Some(key.clone()),
                    format!(
                        "'{key}' must be one of {}",
                        allowed
                            .iter()
                            .map(Json::render)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                ));
            }
        }
    }

    issues
}

/// Fill in defaults for missing keys. The caller validates afterwards.
pub fn apply_defaults(schema: &ConfigSchema, config: &mut BTreeMap<String, Json>) {
    for (key, property) in &schema.properties {
        if !config.contains_key(key) {
            if let Some(default) = &property.default {
                config.insert(key.clone(), default.clone());
            }
        }
    }
}

fn type_matches(kind: &str, value: &Json) -> bool {
    match kind {
        "string" => matches!(value, Json::String(_)),
        "boolean" => matches!(value, Json::Bool(_)),
        "number" => matches!(value, Json::Number(_)),
        "integer" => matches!(value, Json::Number(number) if number.fract() == 0.0),
        "array" => matches!(value, Json::Array(_)),
        "object" => matches!(value, Json::Object(_)),
        "null" => matches!(value, Json::Null),
        // Unknown types are permissive rather than silently wrong.
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema() -> ConfigSchema {
        parse_schema(
            r#"{
                "type": "object",
                "properties": {
                    "position": { "enum": ["left", "center", "right"] },
                    "format": { "enum": ["12h", "24h"] },
                    "show_seconds": { "type": "boolean", "default": false }
                },
                "additionalProperties": false
            }"#,
        )
        .expect("schema")
    }

    fn config(pairs: &[(&str, Json)]) -> BTreeMap<String, Json> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.clone()))
            .collect()
    }

    #[test]
    fn accepts_valid_config() {
        let issues = validate(
            &schema(),
            &config(&[("position", Json::String("left".into()))]),
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn rejects_unknown_keys_and_bad_enums() {
        let issues = validate(
            &schema(),
            &config(&[
                ("position", Json::String("diagonal".into())),
                ("surprise", Json::Bool(true)),
            ]),
        );
        assert!(issues
            .iter()
            .any(|issue| issue.code == "config.not_in_enum"));
        assert!(issues
            .iter()
            .any(|issue| issue.code == "config.unknown_key"));
    }

    #[test]
    fn applies_defaults() {
        let mut config = config(&[("position", Json::String("left".into()))]);
        apply_defaults(&schema(), &mut config);
        assert_eq!(config.get("show_seconds"), Some(&Json::Bool(false)));
        assert!(validate(&schema(), &config).is_empty());
    }
}
