//! Declarative UI tree: schema, validation, live editing, and diffing.
//!
//! The UI is a tree, not a binary. The host renders it; the agent edits it; a
//! tree edit is a transaction and revert restores the previous tree.
//!
//! See TIAKI.md §11.

#![allow(dead_code)]

pub mod keymap;
pub mod ops;
pub mod validate;

use std::collections::BTreeMap;
use std::fmt;

use agentic_json::Json;

pub use keymap::{Binding, Keymap, KeymapValidator};
pub use ops::{apply, apply_ops, parse_ops, UiOp};
pub use validate::{validate, UiTreeValidator};

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: String,
    pub kind: String,
    pub props: BTreeMap<String, Json>,
    pub children: Vec<Node>,
}

impl Node {
    pub fn new(id: impl Into<String>, kind: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind: kind.into(),
            props: BTreeMap::new(),
            children: Vec::new(),
        }
    }

    pub fn with_prop(mut self, key: &str, value: Json) -> Self {
        self.props.insert(key.to_string(), value);
        self
    }

    pub fn prop(&self, key: &str) -> Option<&Json> {
        self.props.get(key)
    }

    pub fn prop_str(&self, key: &str) -> Option<&str> {
        self.props.get(key).and_then(Json::as_str)
    }

    pub fn prop_bool(&self, key: &str) -> Option<bool> {
        self.props.get(key).and_then(Json::as_bool)
    }

    pub fn prop_f64(&self, key: &str) -> Option<f64> {
        self.props.get(key).and_then(Json::as_f64)
    }

    pub fn find(&self, id: &str) -> Option<&Node> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|child| child.find(id))
    }

    pub fn find_mut(&mut self, id: &str) -> Option<&mut Node> {
        if self.id == id {
            return Some(self);
        }
        self.children
            .iter_mut()
            .find_map(|child| child.find_mut(id))
    }

    pub fn parent_of(&self, id: &str) -> Option<&Node> {
        if self.children.iter().any(|child| child.id == id) {
            return Some(self);
        }
        self.children.iter().find_map(|child| child.parent_of(id))
    }

    pub fn parent_of_mut(&mut self, id: &str) -> Option<&mut Node> {
        if self.children.iter().any(|child| child.id == id) {
            return Some(self);
        }
        self.children
            .iter_mut()
            .find_map(|child| child.parent_of_mut(id))
    }

    pub fn walk(&self, visit: &mut impl FnMut(&Node)) {
        visit(self);
        for child in &self.children {
            child.walk(visit);
        }
    }

    pub fn count(&self) -> usize {
        1 + self.children.iter().map(Node::count).sum::<usize>()
    }

    pub fn depth(&self) -> usize {
        1 + self.children.iter().map(Node::depth).max().unwrap_or(0)
    }

    pub fn from_json(value: &Json) -> Result<Self, TreeError> {
        let object = value
            .as_object()
            .ok_or_else(|| TreeError::new("a node must be a JSON object"))?;

        for key in object.keys() {
            if !matches!(key.as_str(), "id" | "kind" | "props" | "children") {
                return Err(TreeError::new(format!("unknown node field '{key}'")));
            }
        }

        let id = object
            .get("id")
            .and_then(Json::as_str)
            .ok_or_else(|| TreeError::new("a node needs a string id"))?
            .to_string();
        let kind = object
            .get("kind")
            .and_then(Json::as_str)
            .ok_or_else(|| TreeError::new(format!("node '{id}' needs a string kind")))?
            .to_string();

        let props = match object.get("props") {
            Some(value) => value
                .as_object()
                .ok_or_else(|| TreeError::new(format!("node '{id}' props must be an object")))?
                .clone(),
            None => BTreeMap::new(),
        };

        let children = match object.get("children") {
            Some(value) => value
                .as_array()
                .ok_or_else(|| TreeError::new(format!("node '{id}' children must be an array")))?
                .iter()
                .map(Node::from_json)
                .collect::<Result<Vec<_>, _>>()?,
            None => Vec::new(),
        };

        Ok(Node {
            id,
            kind,
            props,
            children,
        })
    }

    pub fn to_json(&self) -> Json {
        let mut object = BTreeMap::new();
        object.insert("id".to_string(), Json::String(self.id.clone()));
        object.insert("kind".to_string(), Json::String(self.kind.clone()));
        if !self.props.is_empty() {
            object.insert("props".to_string(), Json::Object(self.props.clone()));
        }
        if !self.children.is_empty() {
            object.insert(
                "children".to_string(),
                Json::Array(self.children.iter().map(Node::to_json).collect()),
            );
        }
        Json::Object(object)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct UiTree {
    pub root: Node,
}

impl UiTree {
    pub fn new(root: Node) -> Self {
        Self { root }
    }

    pub fn parse(text: &str) -> Result<Self, TreeError> {
        let value = agentic_json::parse(text).map_err(|error| TreeError::new(error.to_string()))?;
        Ok(Self {
            root: Node::from_json(&value)?,
        })
    }

    pub fn render(&self) -> String {
        self.root.to_json().render()
    }

    pub fn find(&self, id: &str) -> Option<&Node> {
        self.root.find(id)
    }

    pub fn node_count(&self) -> usize {
        self.root.count()
    }

    /// The tree a principal sees before making any change.
    pub fn default_for(principal: &str) -> Self {
        let user = principal
            .rsplit("user:")
            .next()
            .and_then(|tail| tail.split('@').next())
            .unwrap_or(principal);
        let owner = if user == principal { principal } else { user };

        let mut clock = Node::new("clock", "clock");
        clock
            .props
            .insert("format".into(), Json::String("24h".into()));
        clock.props.insert("show_seconds".into(), Json::Bool(false));
        clock
            .props
            .insert("position".into(), Json::String("left".into()));

        let mut spacer = Node::new("spacer", "spacer");
        spacer.props.insert("flex".into(), Json::Number(1.0));

        let mut status = Node::new("status", "status");
        status.props.insert(
            "items".into(),
            Json::Array(
                ["network", "battery", "audio"]
                    .iter()
                    .map(|item| Json::String(item.to_string()))
                    .collect(),
            ),
        );

        let mut top = Node::new("panel.top", "panel");
        top.props
            .insert("layout".into(), Json::String("row".into()));
        top.props
            .insert("title".into(), Json::String(owner.to_string()));
        top.children = vec![clock, spacer, status];

        let hint = Node::new("hint", "text").with_prop(
            "value",
            Json::String("Press Super+Space for the command palette, ? for keys.".into()),
        );

        let mut main = Node::new("panel.main", "panel");
        main.props
            .insert("layout".into(), Json::String("column".into()));
        main.children = vec![hint];

        let mut workspace = Node::new("desktop", "workspace");
        workspace.props.insert(
            "title".into(),
            Json::String(format!("Tiaki — {owner}")),
        );
        workspace.children = vec![top, main];

        Self { root: workspace }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeError {
    pub message: String,
}

impl TreeError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for TreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for TreeError {}

/// True when a state path names a UI tree, e.g. `user/ada/ui/tree`.
pub fn is_ui_tree_path(path: &str) -> bool {
    let mut segments = path.rsplit('/');
    segments.next() == Some("tree") && segments.next() == Some("ui")
}

/// True when a state path names a keymap, e.g. `user/ada/input/keymap`.
pub fn is_keymap_path(path: &str) -> bool {
    let mut segments = path.rsplit('/');
    segments.next() == Some("keymap") && segments.next() == Some("input")
}

/// Summarize what changed between two trees, in plain language.
pub fn diff(before: &UiTree, after: &UiTree) -> Vec<String> {
    let mut changes = Vec::new();

    let mut before_nodes = BTreeMap::new();
    before.root.walk(&mut |node| {
        before_nodes.insert(node.id.clone(), node.clone());
    });
    let mut after_nodes = BTreeMap::new();
    after.root.walk(&mut |node| {
        after_nodes.insert(node.id.clone(), node.clone());
    });

    for (id, node) in &after_nodes {
        match before_nodes.get(id) {
            None => changes.push(format!("added {} '{}'", node.kind, node.id)),
            Some(old) => {
                if old.kind != node.kind {
                    changes.push(format!("changed kind of '{}'", node.id));
                }
                if old.props != node.props {
                    changes.push(format!("updated '{}' props", node.id));
                }
                let old_parent = before.root.parent_of(id).map(|parent| parent.id.as_str());
                let new_parent = after.root.parent_of(id).map(|parent| parent.id.as_str());
                if old_parent != new_parent {
                    changes.push(format!(
                        "moved '{}' to '{}'",
                        node.id,
                        new_parent.unwrap_or("root")
                    ));
                }
            }
        }
    }

    for id in before_nodes.keys() {
        if !after_nodes.contains_key(id) {
            changes.push(format!("removed '{}'", id));
        }
    }

    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_tree_parses_and_round_trips() {
        let tree = UiTree::default_for("user:ada");
        let rendered = tree.render();
        let parsed = UiTree::parse(&rendered).expect("parse");
        assert_eq!(parsed, tree);
        assert_eq!(tree.node_count(), 7);
        assert!(tree.find("clock").is_some());
    }

    #[test]
    fn rejects_unknown_fields() {
        let error = UiTree::parse(r#"{"id":"x","kind":"text","surprise":true}"#).unwrap_err();
        assert!(error.message.contains("unknown node field"));
    }

    #[test]
    fn diff_reports_add_remove_and_move() {
        let before = UiTree::default_for("user:ada");
        let mut after = before.clone();
        // Move the clock into the main panel.
        let clock = {
            let root = &mut after.root;
            let top = root.find_mut("panel.top").unwrap();
            let index = top.children.iter().position(|n| n.id == "clock").unwrap();
            top.children.remove(index)
        };
        after
            .root
            .find_mut("panel.main")
            .unwrap()
            .children
            .push(clock);

        let changes = diff(&before, &after);
        assert!(changes.iter().any(|line| line.contains("moved 'clock'")));
    }
}
