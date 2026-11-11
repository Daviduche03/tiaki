//! Model registry entries.
//!
//! A model is data: identity, architecture, context, quantization, a verified
//! weight digest, and a backend hint. Weights themselves are content-addressed
//! and immutable; the registry tracks which specs are loaded.

use std::collections::BTreeMap;
use std::fmt;

use agentic_json::Json;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelSpec {
    pub id: String,
    pub version: String,
    pub architecture: String,
    pub context_length: usize,
    pub quantization: String,
    pub weights_digest: String,
    pub tokenizer: String,
    pub license: String,
    pub backend: String,
    pub loaded: bool,
}

impl ModelSpec {
    pub fn example() -> Self {
        Self {
            id: "sim/tiny".into(),
            version: "1.0.0".into(),
            architecture: "simulated".into(),
            context_length: 8192,
            quantization: "none".into(),
            weights_digest: format!("sha256:{}", "0".repeat(64)),
            tokenizer: "word".into(),
            license: "Apache-2.0".into(),
            backend: "simulated".into(),
            loaded: false,
        }
    }

    pub fn parse(text: &str) -> Result<Self, ModelError> {
        let value =
            agentic_json::parse(text).map_err(|error| ModelError::new(error.to_string()))?;
        let object = value
            .as_object()
            .ok_or_else(|| ModelError::new("a model spec must be a JSON object"))?;

        for key in object.keys() {
            if !matches!(
                key.as_str(),
                "id" | "version"
                    | "architecture"
                    | "context_length"
                    | "quantization"
                    | "weights_digest"
                    | "tokenizer"
                    | "license"
                    | "backend"
                    | "loaded"
            ) {
                return Err(ModelError::new(format!("unknown model field '{key}'")));
            }
        }

        let require = |key: &str| -> Result<String, ModelError> {
            object
                .get(key)
                .and_then(Json::as_str)
                .map(str::to_string)
                .ok_or_else(|| ModelError::new(format!("model needs a string '{key}'")))
        };

        let context_length = object
            .get("context_length")
            .and_then(Json::as_f64)
            .filter(|value| *value > 0.0 && value.fract() == 0.0)
            .map(|value| value as usize)
            .ok_or_else(|| ModelError::new("model needs a positive integer 'context_length'"))?;

        let spec = Self {
            id: require("id")?,
            version: require("version")?,
            architecture: require("architecture")?,
            context_length,
            quantization: require("quantization")?,
            weights_digest: require("weights_digest")?,
            tokenizer: require("tokenizer")?,
            license: require("license")?,
            backend: require("backend")?,
            loaded: object
                .get("loaded")
                .and_then(Json::as_bool)
                .unwrap_or(false),
        };
        spec.validate()?;
        Ok(spec)
    }

    pub fn render(&self) -> String {
        self.to_json().render()
    }

    pub fn to_json(&self) -> Json {
        let mut object = BTreeMap::new();
        object.insert("id".to_string(), Json::String(self.id.clone()));
        object.insert("version".to_string(), Json::String(self.version.clone()));
        object.insert(
            "architecture".to_string(),
            Json::String(self.architecture.clone()),
        );
        object.insert(
            "context_length".to_string(),
            Json::Number(self.context_length as f64),
        );
        object.insert(
            "quantization".to_string(),
            Json::String(self.quantization.clone()),
        );
        object.insert(
            "weights_digest".to_string(),
            Json::String(self.weights_digest.clone()),
        );
        object.insert(
            "tokenizer".to_string(),
            Json::String(self.tokenizer.clone()),
        );
        object.insert("license".to_string(), Json::String(self.license.clone()));
        object.insert("backend".to_string(), Json::String(self.backend.clone()));
        object.insert("loaded".to_string(), Json::Bool(self.loaded));
        Json::Object(object)
    }

    pub fn validate(&self) -> Result<(), ModelError> {
        if self.id.trim().is_empty() || self.id.contains(char::is_whitespace) {
            return Err(ModelError::new("model id must be non-empty with no spaces"));
        }
        if self.version.trim().is_empty() {
            return Err(ModelError::new("model version must not be empty"));
        }
        if self.architecture.trim().is_empty() {
            return Err(ModelError::new("model architecture must not be empty"));
        }
        if self.context_length == 0 {
            return Err(ModelError::new("context_length must be positive"));
        }
        let Some(digest) = self.weights_digest.strip_prefix("sha256:") else {
            return Err(ModelError::new("weights_digest must be sha256:<hex>"));
        };
        if digest.len() != 64
            || !digest
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        {
            return Err(ModelError::new(
                "weights_digest must carry a 64-character hex digest",
            ));
        }
        if self.backend.trim().is_empty() {
            return Err(ModelError::new("model backend must not be empty"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelError {
    pub message: String,
}

impl ModelError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ModelError {}

/// State path for a model spec. Model ids may contain `/`.
pub fn model_path(id: &str) -> String {
    format!("system/inference/models/{id}")
}

pub fn is_model_path(path: &str) -> bool {
    path.starts_with("system/inference/models/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_round_trips() {
        let spec = ModelSpec::example();
        let parsed = ModelSpec::parse(&spec.render()).expect("parse");
        assert_eq!(parsed, spec);
    }

    #[test]
    fn rejects_bad_digest_and_unknown_fields() {
        let mut spec = ModelSpec::example();
        spec.weights_digest = "md5:abc".into();
        assert!(spec.validate().is_err());

        let text = r#"{"id":"sim/tiny","version":"1","architecture":"sim","context_length":8,"quantization":"none","weights_digest":"sha256:0000000000000000000000000000000000000000000000000000000000000000","tokenizer":"word","license":"MIT","backend":"simulated","surprise":true}"#;
        assert!(ModelSpec::parse(text).is_err());
    }

    #[test]
    fn paths_are_namespaced() {
        assert_eq!(model_path("sim/tiny"), "system/inference/models/sim/tiny");
        assert!(is_model_path("system/inference/models/sim/tiny"));
        assert!(!is_model_path("system/inference/backends/sim"));
    }
}
