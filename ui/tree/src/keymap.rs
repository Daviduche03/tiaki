//! Keyboard-first keymap: named actions the desktop and the agent can invoke.
//!
//! The keymap is a transactional state resource (`<scope>/input/keymap`), so
//! rebinding is logged and reversible. Named actions double as a structured
//! action catalog: an agent calls `focus.left`, it does not synthesize a key.

use std::collections::BTreeSet;

use agentic_json::Json;
use agentic_transaction::{Issue, Operation, StateStore, Validator};

use crate::{is_keymap_path, TreeError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub keys: String,
    pub action: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keymap {
    pub bindings: Vec<Binding>,
}

impl Keymap {
    /// Keyboard-first defaults inspired by Omarchy: Super for window and
    /// workspace actions, Super+Space for the command palette.
    pub fn default_for() -> Self {
        let pairs = [
            ("super+return", "terminal.new", "New terminal"),
            ("super+q", "window.close", "Close window"),
            ("super+left", "focus.left", "Focus left"),
            ("super+right", "focus.right", "Focus right"),
            ("super+up", "focus.up", "Focus up"),
            ("super+down", "focus.down", "Focus down"),
            ("super+1", "workspace.1", "Workspace 1"),
            ("super+2", "workspace.2", "Workspace 2"),
            ("super+3", "workspace.3", "Workspace 3"),
            ("super+4", "workspace.4", "Workspace 4"),
            ("super+5", "workspace.5", "Workspace 5"),
            ("super+space", "palette.open", "Command palette"),
            ("super+slash", "help.show", "Show keybindings"),
            ("super+r", "ui.reload", "Reload UI"),
            ("super+t", "theme.toggle", "Toggle theme"),
            ("super+k", "clock.toggle_format", "Toggle clock format"),
            ("super+shift+e", "session.exit", "Exit session"),
        ];
        Self {
            bindings: pairs
                .iter()
                .map(|(keys, action, label)| Binding {
                    keys: keys.to_string(),
                    action: action.to_string(),
                    label: label.to_string(),
                })
                .collect(),
        }
    }

    pub fn parse(text: &str) -> Result<Self, TreeError> {
        let value = agentic_json::parse(text).map_err(|error| TreeError::new(error.to_string()))?;
        let object = value
            .as_object()
            .ok_or_else(|| TreeError::new("a keymap must be a JSON object"))?;
        let bindings = object
            .get("bindings")
            .and_then(Json::as_array)
            .ok_or_else(|| TreeError::new("a keymap needs a 'bindings' array"))?
            .iter()
            .map(|value| {
                let entry = value
                    .as_object()
                    .ok_or_else(|| TreeError::new("each binding must be an object"))?;
                Ok(Binding {
                    keys: entry
                        .get("keys")
                        .and_then(Json::as_str)
                        .ok_or_else(|| TreeError::new("a binding needs 'keys'"))?
                        .to_string(),
                    action: entry
                        .get("action")
                        .and_then(Json::as_str)
                        .ok_or_else(|| TreeError::new("a binding needs 'action'"))?
                        .to_string(),
                    label: entry
                        .get("label")
                        .and_then(Json::as_str)
                        .ok_or_else(|| TreeError::new("a binding needs 'label'"))?
                        .to_string(),
                })
            })
            .collect::<Result<Vec<_>, TreeError>>()?;
        Ok(Self { bindings })
    }

    pub fn render(&self) -> String {
        self.to_json().render()
    }

    /// The bindings array the renderer injects as `KEYMAP`.
    pub fn bindings_json(&self) -> Json {
        Json::Array(
            self.bindings
                .iter()
                .map(|binding| {
                    Json::Object(
                        [
                            ("keys".to_string(), Json::String(binding.keys.clone())),
                            ("action".to_string(), Json::String(binding.action.clone())),
                            ("label".to_string(), Json::String(binding.label.clone())),
                        ]
                        .into_iter()
                        .collect(),
                    )
                })
                .collect(),
        )
    }

    pub fn to_json(&self) -> Json {
        Json::Object(
            [("bindings".to_string(), self.bindings_json())]
                .into_iter()
                .collect(),
        )
    }

    pub fn find(&self, keys: &str) -> Option<&Binding> {
        self.bindings
            .iter()
            .find(|binding| binding.keys.eq_ignore_ascii_case(keys))
    }

    pub fn validate(&self) -> Vec<Issue> {
        let mut issues = Vec::new();
        let mut seen = BTreeSet::new();

        if self.bindings.is_empty() {
            issues.push(Issue::error(
                "keymap.empty",
                None,
                "a keymap needs at least one binding",
            ));
        }

        for binding in &self.bindings {
            if binding.keys.trim().is_empty() || binding.keys.contains(char::is_whitespace) {
                issues.push(Issue::error(
                    "keymap.invalid_keys",
                    None,
                    format!("'{}' is not a valid key combination", binding.keys),
                ));
            } else if !seen.insert(binding.keys.to_ascii_lowercase()) {
                issues.push(Issue::error(
                    "keymap.duplicate_keys",
                    None,
                    format!("'{}' is bound more than once", binding.keys),
                ));
            }
            if !valid_action(&binding.action) {
                issues.push(Issue::error(
                    "keymap.invalid_action",
                    None,
                    format!(
                        "'{}' is not a valid action name; use dotted lowercase names",
                        binding.action
                    ),
                ));
            }
            if binding.label.trim().is_empty() {
                issues.push(Issue::error(
                    "keymap.missing_label",
                    None,
                    format!("binding '{}' needs a label", binding.keys),
                ));
            }
        }

        issues
    }
}

fn valid_action(action: &str) -> bool {
    if action.is_empty() || action.contains(char::is_whitespace) || !action.contains('.') {
        return false;
    }
    action.split('.').all(|segment| {
        !segment.is_empty()
            && segment.chars().all(|character| {
                character.is_ascii_lowercase()
                    || character.is_ascii_digit()
                    || character == '_'
                    || character == '-'
            })
    })
}

/// Validates any operation that writes a keymap.
pub struct KeymapValidator;

impl Validator for KeymapValidator {
    fn validate(&self, _store: &dyn StateStore, operations: &[Operation]) -> Vec<Issue> {
        let mut issues = Vec::new();
        for operation in operations {
            let Operation::Set { path, value } = operation else {
                continue;
            };
            if !is_keymap_path(path) {
                continue;
            }
            match Keymap::parse(value) {
                Ok(keymap) => {
                    for mut issue in keymap.validate() {
                        if issue.path.is_none() {
                            issue.path = Some(path.clone());
                        }
                        issues.push(issue);
                    }
                }
                Err(error) => issues.push(Issue::error(
                    "keymap.parse",
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

    #[test]
    fn default_keymap_is_valid() {
        assert!(Keymap::default_for().validate().is_empty());
    }

    #[test]
    fn round_trips() {
        let keymap = Keymap::default_for();
        let parsed = Keymap::parse(&keymap.render()).expect("parse");
        assert_eq!(parsed, keymap);
    }

    #[test]
    fn rejects_duplicates_and_bad_actions() {
        let keymap = Keymap {
            bindings: vec![
                Binding {
                    keys: "super+space".into(),
                    action: "palette.open".into(),
                    label: "Palette".into(),
                },
                Binding {
                    keys: "super+space".into(),
                    action: "Not An Action".into(),
                    label: "".into(),
                },
            ],
        };
        let issues = keymap.validate();
        assert!(issues
            .iter()
            .any(|issue| issue.code == "keymap.duplicate_keys"));
        assert!(issues
            .iter()
            .any(|issue| issue.code == "keymap.invalid_action"));
    }

    #[test]
    fn omarchy_style_defaults_exist() {
        let keymap = Keymap::default_for();
        assert_eq!(
            keymap.find("super+space").map(|b| b.action.as_str()),
            Some("palette.open")
        );
        assert_eq!(
            keymap.find("super+left").map(|b| b.action.as_str()),
            Some("focus.left")
        );
    }
}
