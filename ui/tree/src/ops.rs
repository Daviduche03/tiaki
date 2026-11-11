//! Agent-editable UI operations.
//!
//! An edit file is `{"ops":[...]}`. Each op is applied in order to a copy of
//! the current tree; the caller stores the result as a transaction.

use std::collections::BTreeSet;

use agentic_json::Json;

use crate::validate::{allows_children, check_prop_value};
use crate::{Node, TreeError, UiTree};

#[derive(Clone, Debug, PartialEq)]
pub enum UiOp {
    Insert {
        parent: String,
        index: usize,
        node: Node,
    },
    Remove {
        id: String,
    },
    Move {
        id: String,
        parent: String,
        index: usize,
    },
    SetProp {
        id: String,
        key: String,
        value: Json,
    },
    RemoveProp {
        id: String,
        key: String,
    },
}

pub fn parse_ops(text: &str) -> Result<Vec<UiOp>, TreeError> {
    let document = agentic_json::parse(text).map_err(|error| TreeError::new(error.to_string()))?;
    let object = document
        .as_object()
        .ok_or_else(|| TreeError::new("an edit file must be a JSON object"))?;
    let ops = object
        .get("ops")
        .and_then(Json::as_array)
        .ok_or_else(|| TreeError::new("an edit file needs an 'ops' array"))?;

    ops.iter()
        .map(|value| {
            let op = value
                .as_object()
                .ok_or_else(|| TreeError::new("each op must be an object"))?;
            match op.get("op").and_then(Json::as_str) {
                Some("insert") => Ok(UiOp::Insert {
                    parent: require_str(op, "parent")?,
                    index: optional_index(op, "index")?.unwrap_or(usize::MAX),
                    node: Node::from_json(
                        op.get("node")
                            .ok_or_else(|| TreeError::new("insert needs a 'node'"))?,
                    )?,
                }),
                Some("remove") => Ok(UiOp::Remove {
                    id: require_str(op, "id")?,
                }),
                Some("move") => Ok(UiOp::Move {
                    id: require_str(op, "id")?,
                    parent: require_str(op, "parent")?,
                    index: optional_index(op, "index")?.unwrap_or(usize::MAX),
                }),
                Some("set_prop") => Ok(UiOp::SetProp {
                    id: require_str(op, "id")?,
                    key: require_str(op, "key")?,
                    value: op
                        .get("value")
                        .cloned()
                        .ok_or_else(|| TreeError::new("set_prop needs a 'value'"))?,
                }),
                Some("remove_prop") => Ok(UiOp::RemoveProp {
                    id: require_str(op, "id")?,
                    key: require_str(op, "key")?,
                }),
                Some(other) => Err(TreeError::new(format!(
                    "unknown op '{other}'; expected insert, remove, move, set_prop, or remove_prop"
                ))),
                None => Err(TreeError::new("each op needs an 'op' field")),
            }
        })
        .collect()
}

fn require_str(
    object: &std::collections::BTreeMap<String, Json>,
    key: &str,
) -> Result<String, TreeError> {
    object
        .get(key)
        .and_then(Json::as_str)
        .map(str::to_string)
        .ok_or_else(|| TreeError::new(format!("op needs a string '{key}'")))
}

fn optional_index(
    object: &std::collections::BTreeMap<String, Json>,
    key: &str,
) -> Result<Option<usize>, TreeError> {
    let Some(value) = object.get(key) else {
        return Ok(None);
    };
    let number = value
        .as_f64()
        .ok_or_else(|| TreeError::new(format!("'{key}' must be a number")))?;
    if number < 0.0 || number.fract() != 0.0 {
        return Err(TreeError::new(format!(
            "'{key}' must be a non-negative integer"
        )));
    }
    Ok(Some(number as usize))
}

pub fn apply_ops(tree: &mut UiTree, ops: &[UiOp]) -> Result<(), TreeError> {
    for op in ops {
        apply(tree, op)?;
    }
    Ok(())
}

pub fn apply(tree: &mut UiTree, op: &UiOp) -> Result<(), TreeError> {
    match op {
        UiOp::Insert {
            parent,
            index,
            node,
        } => insert(tree, parent, *index, node.clone()),
        UiOp::Remove { id } => remove(tree, id),
        UiOp::Move { id, parent, index } => move_node(tree, id, parent, *index),
        UiOp::SetProp { id, key, value } => set_prop(tree, id, key, value.clone()),
        UiOp::RemoveProp { id, key } => remove_prop(tree, id, key),
    }
}

fn insert(tree: &mut UiTree, parent: &str, index: usize, node: Node) -> Result<(), TreeError> {
    // Every id in the new subtree must be unused.
    let mut existing = BTreeSet::new();
    tree.root.walk(&mut |node| {
        existing.insert(node.id.clone());
    });
    let mut added = BTreeSet::new();
    node.walk(&mut |child| {
        added.insert(child.id.clone());
    });
    if let Some(duplicate) = added.intersection(&existing).next() {
        return Err(TreeError::new(format!(
            "node id '{duplicate}' already exists"
        )));
    }

    let target = tree
        .root
        .find_mut(parent)
        .ok_or_else(|| TreeError::new(format!("no node '{parent}'")))?;
    if !allows_children(&target.kind) {
        return Err(TreeError::new(format!(
            "a '{}' widget cannot have children",
            target.kind
        )));
    }
    let index = index.min(target.children.len());
    target.children.insert(index, node);
    Ok(())
}

fn remove(tree: &mut UiTree, id: &str) -> Result<(), TreeError> {
    if tree.root.id == id {
        return Err(TreeError::new("cannot remove the root node"));
    }
    match detach(&mut tree.root, id) {
        Some(_) => Ok(()),
        None => Err(TreeError::new(format!("no node '{id}'"))),
    }
}

fn move_node(tree: &mut UiTree, id: &str, parent: &str, index: usize) -> Result<(), TreeError> {
    if tree.root.id == id {
        return Err(TreeError::new("cannot move the root node"));
    }

    // Check for cycles before detaching, so a failed move leaves the tree intact.
    let moved = tree
        .root
        .find(id)
        .ok_or_else(|| TreeError::new(format!("no node '{id}'")))?;
    if moved.find(parent).is_some() {
        return Err(TreeError::new(format!(
            "cannot move '{id}' into its own subtree"
        )));
    }
    let target_allows = tree
        .root
        .find(parent)
        .map(|node| allows_children(&node.kind))
        .unwrap_or(false);
    if !target_allows {
        return Err(TreeError::new(format!(
            "'{parent}' is not a container that accepts children"
        )));
    }

    let (node, old_parent, old_index) =
        detach(&mut tree.root, id).ok_or_else(|| TreeError::new(format!("no node '{id}'")))?;

    let target = tree
        .root
        .find_mut(parent)
        .ok_or_else(|| TreeError::new(format!("no node '{parent}'")))?;
    let mut index = index.min(target.children.len() + usize::from(old_parent == parent));
    if old_parent == parent && old_index < index {
        index -= 1;
    }
    let index = index.min(target.children.len());
    target.children.insert(index, node);
    Ok(())
}

fn set_prop(tree: &mut UiTree, id: &str, key: &str, value: Json) -> Result<(), TreeError> {
    let node = tree
        .root
        .find_mut(id)
        .ok_or_else(|| TreeError::new(format!("no node '{id}'")))?;
    check_prop_value(&node.kind, key, &value).map_err(TreeError::new)?;
    node.props.insert(key.to_string(), value);
    Ok(())
}

fn remove_prop(tree: &mut UiTree, id: &str, key: &str) -> Result<(), TreeError> {
    let node = tree
        .root
        .find_mut(id)
        .ok_or_else(|| TreeError::new(format!("no node '{id}'")))?;
    if node.props.remove(key).is_none() {
        return Err(TreeError::new(format!("'{id}' has no property '{key}'")));
    }
    Ok(())
}

fn detach(root: &mut Node, id: &str) -> Option<(Node, String, usize)> {
    if let Some(index) = root.children.iter().position(|child| child.id == id) {
        let node = root.children.remove(index);
        return Some((node, root.id.clone(), index));
    }
    for child in &mut root.children {
        if let Some(found) = detach(child, id) {
            return Some(found);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_a_node_between_panels() {
        let mut tree = UiTree::default_for("user:ada");
        apply(
            &mut tree,
            &UiOp::Move {
                id: "clock".into(),
                parent: "panel.main".into(),
                index: 0,
            },
        )
        .expect("move");
        assert!(tree
            .root
            .find("panel.main")
            .unwrap()
            .children
            .iter()
            .any(|node| node.id == "clock"));
    }

    #[test]
    fn rejects_a_cycle() {
        let mut tree = UiTree::default_for("user:ada");
        let error = apply(
            &mut tree,
            &UiOp::Move {
                id: "panel.top".into(),
                parent: "panel.top".into(),
                index: 0,
            },
        )
        .unwrap_err();
        assert!(error.message.contains("subtree"));
    }

    #[test]
    fn parses_an_edit_file_and_applies_it() {
        let ops = parse_ops(
            r#"{"ops":[
                {"op":"set_prop","id":"clock","key":"format","value":"12h"},
                {"op":"insert","parent":"panel.main","index":0,
                 "node":{"id":"cta","kind":"button","props":{"label":"Open settings","action":"settings.open"}}}
            ]}"#,
        )
        .expect("parse");

        let mut tree = UiTree::default_for("user:ada");
        apply_ops(&mut tree, &ops).expect("apply");

        assert_eq!(
            tree.root.find("clock").unwrap().prop_str("format"),
            Some("12h")
        );
        assert_eq!(
            tree.root.find("cta").unwrap().prop_str("action"),
            Some("settings.open")
        );
    }

    #[test]
    fn rejects_invalid_prop_values() {
        let mut tree = UiTree::default_for("user:ada");
        let error = apply(
            &mut tree,
            &UiOp::SetProp {
                id: "clock".into(),
                key: "format".into(),
                value: Json::String("36h".into()),
            },
        )
        .unwrap_err();
        assert!(error.message.contains("not one of"));
    }
}
