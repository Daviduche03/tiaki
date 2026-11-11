//! App backends for computer use: typed actions against a declarative tree.
//!
//! The tree is the payload of the `ui.observe` / `ui.act` MCP tools; the old
//! observe → act → verify session referee is gone, and opencode itself is
//! driven through its own server API.

use std::collections::BTreeMap;
use std::fmt;

use crate::tree::{ActionKind, AppNode, AppTree, Role};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppAction {
    pub node: String,
    pub kind: ActionKind,
    pub value: Option<String>,
}

impl AppAction {
    pub fn new(node: impl Into<String>, kind: ActionKind) -> Self {
        Self {
            node: node.into(),
            kind,
            value: None,
        }
    }

    pub fn set_text(node: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            node: node.into(),
            kind: ActionKind::SetText,
            value: Some(value.into()),
        }
    }

    pub fn describe(&self) -> String {
        match &self.value {
            Some(value) => format!("{} '{}' = {value}", self.kind.as_str(), self.node),
            None => format!("{} '{}'", self.kind.as_str(), self.node),
        }
    }
}

pub trait AppBackend {
    fn app_id(&self) -> &str;
    fn tree(&self) -> AppTree;
    fn act(&mut self, action: &AppAction) -> Result<String, AppBackendError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppBackendError {
    UnknownNode(String),
    Failed(String),
}

impl fmt::Display for AppBackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppBackendError::UnknownNode(node) => write!(f, "app does not know node '{node}'"),
            AppBackendError::Failed(reason) => write!(f, "app action failed: {reason}"),
        }
    }
}

impl std::error::Error for AppBackendError {}

/// A deterministic checkout app used by the demo and tests.
pub struct SimulatedApp {
    app: String,
    title: String,
    values: BTreeMap<String, String>,
}

impl SimulatedApp {
    pub fn checkout() -> Self {
        let mut values = BTreeMap::new();
        values.insert("gift".into(), "off".into());
        values.insert("address".into(), "1 Main St".into());
        values.insert("place".into(), "idle".into());
        values.insert("cancel".into(), "idle".into());
        Self {
            app: "checkout".into(),
            title: "Checkout".into(),
            values,
        }
    }

    pub fn value(&self, node: &str) -> Option<&str> {
        self.values.get(node).map(String::as_str)
    }

    pub fn values(&self) -> &BTreeMap<String, String> {
        &self.values
    }

    pub fn from_values(values: BTreeMap<String, String>) -> Self {
        Self {
            app: "checkout".into(),
            title: "Checkout".into(),
            values,
        }
    }
}

impl Default for SimulatedApp {
    fn default() -> Self {
        Self::checkout()
    }
}

impl AppBackend for SimulatedApp {
    fn app_id(&self) -> &str {
        &self.app
    }

    fn tree(&self) -> AppTree {
        let value = |node: &str| self.values.get(node).cloned();
        let mut window = AppNode::new("checkout", Role::Window, &self.title);
        window.children = vec![
            AppNode::new("heading", Role::Label, "Order summary"),
            AppNode::new("gift", Role::Checkbox, "Gift wrap")
                .with_action(ActionKind::Toggle)
                .with_value(value("gift").unwrap_or_else(|| "off".into())),
            AppNode::new("address", Role::TextField, "Delivery address")
                .with_action(ActionKind::SetText)
                .with_value(value("address").unwrap_or_default()),
            AppNode::new("place", Role::Button, "Place order")
                .with_action(ActionKind::Invoke)
                .with_value(value("place").unwrap_or_else(|| "idle".into()))
                .high_risk(),
            AppNode::new("cancel", Role::Button, "Cancel")
                .with_action(ActionKind::Invoke)
                .with_value(value("cancel").unwrap_or_else(|| "idle".into())),
        ];
        AppTree {
            app: self.app.clone(),
            title: self.title.clone(),
            root: window,
        }
    }

    fn act(&mut self, action: &AppAction) -> Result<String, AppBackendError> {
        match (action.node.as_str(), action.kind) {
            ("gift", ActionKind::Toggle) => {
                let next = if self.values.get("gift").map(String::as_str) == Some("on") {
                    "off"
                } else {
                    "on"
                };
                self.values.insert("gift".into(), next.into());
                Ok(format!("Gift wrap is {next}"))
            }
            ("address", ActionKind::SetText) => {
                let value = action
                    .value
                    .clone()
                    .ok_or_else(|| AppBackendError::Failed("missing address".into()))?;
                self.values.insert("address".into(), value.clone());
                Ok(format!("Delivery address is {value}"))
            }
            ("place", ActionKind::Invoke) => {
                self.values.insert("place".into(), "placed".into());
                Ok("Order placed".into())
            }
            ("cancel", ActionKind::Invoke) => {
                self.values.insert("cancel".into(), "cancelled".into());
                Ok("Order cancelled".into())
            }
            _ => Err(AppBackendError::UnknownNode(action.node.clone())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggling_gift_updates_the_tree() {
        let mut app = SimulatedApp::checkout();
        let message = app
            .act(&AppAction::new("gift", ActionKind::Toggle))
            .expect("toggle");
        assert_eq!(message, "Gift wrap is on");
        assert_eq!(app.value("gift"), Some("on"));
        assert_eq!(
            app.tree().find("gift").and_then(|node| node.value.clone()),
            Some("on".to_string())
        );
    }

    #[test]
    fn unknown_nodes_are_rejected() {
        let mut app = SimulatedApp::checkout();
        assert!(matches!(
            app.act(&AppAction::new("nope", ActionKind::Toggle)),
            Err(AppBackendError::UnknownNode(_))
        ));
    }
}
