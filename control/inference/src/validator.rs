//! Preflight validation for model registry writes.

use agentic_transaction::{Issue, Operation, StateStore, Validator};

use crate::model::{is_model_path, ModelSpec};

pub struct ModelRegistryValidator;

impl Validator for ModelRegistryValidator {
    fn validate(&self, _store: &dyn StateStore, operations: &[Operation]) -> Vec<Issue> {
        let mut issues = Vec::new();
        for operation in operations {
            let Operation::Set { path, value } = operation else {
                continue;
            };
            if !is_model_path(path) {
                continue;
            }
            match ModelSpec::parse(value) {
                Ok(spec) => {
                    let expected = path
                        .strip_prefix("system/inference/models/")
                        .unwrap_or_default();
                    if spec.id != expected {
                        issues.push(Issue::error(
                            "model.id_mismatch",
                            Some(path.clone()),
                            format!("path says '{expected}' but the spec id is '{}'", spec.id),
                        ));
                    }
                }
                Err(error) => issues.push(Issue::error(
                    "model.invalid",
                    Some(path.clone()),
                    error.to_string(),
                )),
            }
        }
        issues
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentic_transaction::MemoryStore;

    #[test]
    fn validates_specs_and_paths() {
        let validator = ModelRegistryValidator;
        let store = MemoryStore::new();

        let good = Operation::Set {
            path: "system/inference/models/sim/tiny".into(),
            value: ModelSpec::example().render(),
        };
        assert!(validator.validate(&store, &[good]).is_empty());

        let bad_path = Operation::Set {
            path: "system/inference/models/other".into(),
            value: ModelSpec::example().render(),
        };
        let issues = validator.validate(&store, &[bad_path]);
        assert!(issues.iter().any(|issue| issue.code == "model.id_mismatch"));
    }
}
