//! UI tree validation against the widget catalog.
//!
//! Trees are validated before render. Errors block the transaction; the
//! previous tree stays live.

use std::collections::BTreeSet;

use agentic_json::Json;
use agentic_transaction::{Issue, Operation, StateStore, Validator};

use crate::{is_ui_tree_path, Node, UiTree};

pub const MAX_DEPTH: usize = 8;
pub const MAX_NODES: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropRule {
    Text,
    Bool,
    Number,
    Enum(&'static [&'static str]),
    EnumArray(&'static [&'static str]),
}

pub fn is_known_kind(kind: &str) -> bool {
    matches!(
        kind,
        "workspace"
            | "panel"
            | "clock"
            | "spacer"
            | "separator"
            | "status"
            | "text"
            | "button"
            | "card"
            | "section"
            | "row"
            | "value"
            | "note"
            | "hero"
            | "list"
            | "dropdown"
            | "menu"
            | "menuitem"
            | "app"
    )
}

pub fn allows_children(kind: &str) -> bool {
    matches!(
        kind,
        "workspace" | "panel" | "menu" | "section" | "card" | "row"
    )
}

pub fn prop_rule(kind: &str, prop: &str) -> Option<PropRule> {
    match (kind, prop) {
        ("workspace", "title") => Some(PropRule::Text),
        ("panel", "layout") => Some(PropRule::Enum(&["row", "column"])),
        ("panel", "title") => Some(PropRule::Text),
        ("panel", "role") => Some(PropRule::Enum(&[
            "bar", "dock", "launcher", "menubar", "status", "content", "window", "settings",
            "agents",
        ])),
        ("clock", "format") => Some(PropRule::Enum(&["12h", "24h"])),
        ("clock", "show_seconds") => Some(PropRule::Bool),
        ("clock", "show_date") => Some(PropRule::Bool),
        ("clock", "position") => Some(PropRule::Enum(&["left", "center", "right"])),
        ("spacer", "flex") => Some(PropRule::Number),
        ("separator", "orientation") => Some(PropRule::Enum(&["horizontal", "vertical"])),
        ("status", "items") => Some(PropRule::EnumArray(&[
            "network",
            "wifi",
            "battery",
            "audio",
            "inference",
            "time",
            "search",
            "control",
        ])),
        ("text", "value") => Some(PropRule::Text),
        ("text", "size") => Some(PropRule::Enum(&["small", "body", "title"])),
        ("text", "weight") => Some(PropRule::Enum(&["regular", "medium", "semibold", "bold"])),
        ("text", "color") => Some(PropRule::Text),
        ("button", "label") => Some(PropRule::Text),
        ("button", "action") => Some(PropRule::Text),
        ("button", "variant") => Some(PropRule::Enum(&[
            "default", "menubar", "logo", "status", "dock", "list", "primary", "ghost",
        ])),
        ("button", "icon") => Some(PropRule::Text),
        ("button", "detail") => Some(PropRule::Text),
        ("button", "command") => Some(PropRule::Text),
        ("button", "running") => Some(PropRule::Bool),
        ("button", "active") => Some(PropRule::Bool),
        ("card", "title") => Some(PropRule::Text),
        ("card", "body") => Some(PropRule::Text),
        ("card", "action") => Some(PropRule::Text),
        ("section", "title") => Some(PropRule::Text),
        ("row", "title") => Some(PropRule::Text),
        ("row", "subtitle") => Some(PropRule::Text),
        ("row", "icon") => Some(PropRule::Text),
        ("row", "link") => Some(PropRule::Text),
        ("row", "command") => Some(PropRule::Text),
        ("value", "source") => Some(PropRule::Enum(&[
            "time.now",
            "time.zone",
            "time.timesyncd",
            "locale.language",
            "locale.region",
            "display.summary",
            "power.summary",
            "storage.summary",
            "system.hostname",
            "system.cpu",
            "system.memory",
            "system.bits",
            "system.os",
            "system.kernel",
        ])),
        ("value", "value") => Some(PropRule::Text),
        ("note", "value") => Some(PropRule::Text),
        ("hero", "icon") => Some(PropRule::Text),
        ("hero", "title") => Some(PropRule::Text),
        ("hero", "subtitle") => Some(PropRule::Text),
        ("hero", "button") => Some(PropRule::Text),
        ("hero", "action") => Some(PropRule::Text),
        ("list", "source") => Some(PropRule::Enum(&[
            "hardware.usb",
            "hardware.input",
            "network.interfaces",
            "network.properties",
            "bluetooth.adapter",
        ])),
        ("dropdown", "source") => Some(PropRule::Enum(&["time.clock_format"])),
        ("dropdown", "options") => Some(PropRule::Text),
        ("dropdown", "action") => Some(PropRule::Text),
        ("menu", "label") => Some(PropRule::Text),
        ("menuitem", "label") => Some(PropRule::Text),
        ("menuitem", "action") => Some(PropRule::Text),
        ("menuitem", "shortcut") => Some(PropRule::Text),
        ("app", "label") => Some(PropRule::Text),
        ("app", "icon") => Some(PropRule::Text),
        ("app", "art") => Some(PropRule::Enum(&[
            "terminal",
            "files",
            "editor",
            "browser",
            "monitor",
            "agents",
            "trash",
            "transactions",
            "theme",
            "reload",
            "clock",
            "exit",
            "lock",
            "settings",
            "wallpaper",
            "launcher",
            "code",
            "downloads",
            "app",
        ])),
        ("app", "command") => Some(PropRule::Text),
        ("app", "category") => Some(PropRule::Text),
        ("app", "description") => Some(PropRule::Text),
        ("app", "color") => Some(PropRule::Enum(&[
            "blue", "purple", "green", "orange", "red", "cyan", "slate", "pink",
        ])),
        ("app", "running") => Some(PropRule::Bool),
        ("app", "pinned") => Some(PropRule::Bool),
        _ => None,
    }
}

pub fn validate(tree: &UiTree) -> Vec<Issue> {
    let mut issues = Vec::new();
    let mut seen = BTreeSet::new();

    if tree.root.count() > MAX_NODES {
        issues.push(Issue::error(
            "tree.too_large",
            None,
            format!(
                "tree has {} nodes; the limit is {MAX_NODES}",
                tree.root.count()
            ),
        ));
    }
    if tree.root.depth() > MAX_DEPTH {
        issues.push(Issue::error(
            "tree.too_deep",
            None,
            format!(
                "tree is {} levels deep; the limit is {MAX_DEPTH}",
                tree.root.depth()
            ),
        ));
    }

    validate_node(&tree.root, None, 1, &mut seen, &mut issues);
    issues
}

fn validate_node(
    node: &Node,
    parent: Option<&str>,
    depth: usize,
    seen: &mut BTreeSet<String>,
    issues: &mut Vec<Issue>,
) {
    let path = Some(node.id.clone());

    if !valid_id(&node.id) {
        issues.push(Issue::error(
            "tree.invalid_id",
            path.clone(),
            format!(
                "'{}' is not a valid node id; use lowercase letters, digits, '.', '_', or '-'",
                node.id
            ),
        ));
    }
    if !seen.insert(node.id.clone()) {
        issues.push(Issue::error(
            "tree.duplicate_id",
            path.clone(),
            format!("node id '{}' is used more than once", node.id),
        ));
    }

    if !is_known_kind(&node.kind) {
        issues.push(Issue::error(
            "tree.unknown_kind",
            path.clone(),
            format!(
                "unknown widget kind '{}'; expected workspace, panel, clock, spacer, separator, status, text, button, card, section, row, value, note, hero, list, dropdown, menu, menuitem, or app",
                node.kind
            ),
        ));
    } else {
        if !node.children.is_empty() && !allows_children(&node.kind) {
            issues.push(Issue::error(
                "tree.unexpected_children",
                path.clone(),
                format!("a '{}' widget cannot have children", node.kind),
            ));
        }
        for prop in node.props.keys() {
            if prop_rule(&node.kind, prop).is_none() {
                issues.push(Issue::error(
                    "tree.unknown_prop",
                    path.clone(),
                    format!("'{}' is not a property of a '{}' widget", prop, node.kind),
                ));
            }
        }
        validate_required(node, path.clone(), issues);
    }

    for (key, value) in &node.props {
        if let Some(rule) = prop_rule(&node.kind, key) {
            if let Err(reason) = check_value(rule, value) {
                issues.push(Issue::error(
                    "tree.invalid_prop",
                    path.clone(),
                    format!("'{}.{}': {reason}", node.id, key),
                ));
            }
        }
    }

    if depth > MAX_DEPTH {
        issues.push(Issue::error(
            "tree.too_deep",
            path.clone(),
            format!(
                "'{}{}' exceeds the depth limit",
                parent.map(|p| format!("{p}/")).unwrap_or_default(),
                node.id
            ),
        ));
    }

    for child in &node.children {
        validate_node(child, Some(&node.id), depth + 1, seen, issues);
    }
}

fn validate_required(node: &Node, path: Option<String>, issues: &mut Vec<Issue>) {
    let required: &[&str] = match node.kind.as_str() {
        "text" => &["value"],
        "button" => &["label", "action"],
        "card" => &["title"],
        "section" => &["title"],
        "row" => &["title"],
        "value" => &[],
        "note" => &["value"],
        "hero" => &["icon", "title", "button", "action"],
        "list" => &["source"],
        "dropdown" => &["source", "options", "action"],
        "menu" => &["label"],
        "menuitem" => &["label", "action"],
        "app" => &["label", "command"],
        _ => &[],
    };
    for key in required {
        if !node.props.contains_key(*key) {
            issues.push(Issue::error(
                "tree.missing_prop",
                path.clone(),
                format!("a '{}' widget requires '{}'", node.kind, key),
            ));
        }
    }
}

/// Check one property value against the widget catalog.
pub fn check_prop_value(kind: &str, key: &str, value: &Json) -> Result<(), String> {
    match prop_rule(kind, key) {
        Some(rule) => check_value(rule, value),
        None => Err(format!("'{key}' is not a property of a '{kind}' widget")),
    }
}

fn check_value(rule: PropRule, value: &Json) -> Result<(), String> {
    match rule {
        PropRule::Text => match value {
            Json::String(_) => Ok(()),
            other => Err(format!("expected a string, found {}", other.type_name())),
        },
        PropRule::Bool => match value {
            Json::Bool(_) => Ok(()),
            other => Err(format!("expected a boolean, found {}", other.type_name())),
        },
        PropRule::Number => match value {
            Json::Number(_) => Ok(()),
            other => Err(format!("expected a number, found {}", other.type_name())),
        },
        PropRule::Enum(allowed) => match value.as_str() {
            Some(text) if allowed.contains(&text) => Ok(()),
            Some(text) => Err(format!(
                "'{text}' is not one of {}",
                allowed
                    .iter()
                    .map(|value| format!("\"{value}\""))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            None => Err(format!("expected a string, found {}", value.type_name())),
        },
        PropRule::EnumArray(allowed) => {
            let values = value
                .as_array()
                .ok_or_else(|| format!("expected an array, found {}", value.type_name()))?;
            for item in values {
                match item.as_str() {
                    Some(text) if allowed.contains(&text) => {}
                    Some(text) => {
                        return Err(format!(
                            "'{text}' is not one of {}",
                            allowed
                                .iter()
                                .map(|value| format!("\"{value}\""))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ));
                    }
                    None => return Err("array items must be strings".to_string()),
                }
            }
            Ok(())
        }
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '.' | '_' | '-')
        })
}

/// Validates any operation that writes a UI tree.
pub struct UiTreeValidator;

impl Validator for UiTreeValidator {
    fn validate(&self, _store: &dyn StateStore, operations: &[Operation]) -> Vec<Issue> {
        let mut issues = Vec::new();
        for operation in operations {
            let Operation::Set { path, value } = operation else {
                continue;
            };
            if !is_ui_tree_path(path) {
                continue;
            }
            match UiTree::parse(value) {
                Ok(tree) => {
                    for mut issue in validate(&tree) {
                        if issue.path.is_none() {
                            issue.path = Some(path.clone());
                        }
                        issues.push(issue);
                    }
                }
                Err(error) => issues.push(Issue::error(
                    "tree.parse",
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
    fn default_tree_is_valid() {
        assert!(validate(&UiTree::default_for("user:ada")).is_empty());
    }

    #[test]
    fn rejects_unknown_kind_and_prop() {
        let mut tree = UiTree::default_for("user:ada");
        tree.root.children[0].kind = "hologram".into();
        tree.root
            .find_mut("panel.main")
            .unwrap()
            .props
            .insert("glow".into(), Json::Bool(true));
        let issues = validate(&tree);
        assert!(issues.iter().any(|issue| issue.code == "tree.unknown_kind"));
        assert!(issues.iter().any(|issue| issue.code == "tree.unknown_prop"));
    }

    #[test]
    fn rejects_children_on_leaf_widgets() {
        let mut tree = UiTree::default_for("user:ada");
        tree.root
            .find_mut("clock")
            .unwrap()
            .children
            .push(Node::new("nested", "text"));
        let issues = validate(&tree);
        assert!(issues
            .iter()
            .any(|issue| issue.code == "tree.unexpected_children"));
    }

    #[test]
    fn accepts_shell_tree() {
        let tree = UiTree::parse(include_str!("../../../desktop/desktop.json")).expect("parse");
        let issues = validate(&tree);
        assert!(
            issues.is_empty(),
            "shell tree should validate: {:?}",
            issues
                .iter()
                .map(|issue| format!("{}: {}", issue.code, issue.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn accepts_cards() {
        let mut tree = UiTree::default_for("user:ada");
        tree.root.find_mut("panel.main").unwrap().children.push(
            Node::new("card", "card")
                .with_prop("title", Json::String("Rebuild".into()))
                .with_prop("body", Json::String("The UI is a tree.".into()))
                .with_prop("action", Json::String("ui.reload".into())),
        );
        assert!(validate(&tree).is_empty());
    }

    #[test]
    fn accepts_settings_content() {
        let mut tree = UiTree::default_for("user:ada");
        tree.root.find_mut("panel.main").unwrap().children.push(
            Node::new("settings.page.time.content", "panel")
                .with_prop("title", Json::String("Time".into())),
        );
        let content = tree.root.find_mut("settings.page.time.content").unwrap();
        let mut section = Node::new("settings.page.time.section.datetime", "section");
        section
            .props
            .insert("title".into(), Json::String("Date & time".into()));
        let mut row = Node::new("settings.page.time.row.zone", "row");
        row.props
            .insert("title".into(), Json::String("Time zone".into()));
        let mut value = Node::new("settings.page.time.row.zone.value", "value");
        value
            .props
            .insert("source".into(), Json::String("time.zone".into()));
        row.children.push(value);
        let mut format = Node::new("settings.page.time.row.format", "row");
        format
            .props
            .insert("title".into(), Json::String("Clock format".into()));
        format
            .props
            .insert("icon".into(), Json::String("clock".into()));
        let mut clock = Node::new("settings.page.time.row.format.control", "dropdown");
        clock
            .props
            .insert("source".into(), Json::String("time.clock_format".into()));
        clock
            .props
            .insert("options".into(), Json::String("12-hour|24-hour".into()));
        clock
            .props
            .insert("action".into(), Json::String("clock.toggle_format".into()));
        format.children.push(clock);
        section.children.push(row);
        section.children.push(format);
        content.children.push(section);
        assert!(validate(&tree).is_empty());
    }

    #[test]
    fn rejects_bad_enum_and_duplicate_ids() {
        let mut tree = UiTree::default_for("user:ada");
        tree.root
            .find_mut("clock")
            .unwrap()
            .props
            .insert("format".into(), Json::String("36h".into()));
        tree.root
            .find_mut("panel.main")
            .unwrap()
            .children
            .push(Node::new("clock", "clock"));
        let issues = validate(&tree);
        assert!(issues.iter().any(|issue| issue.code == "tree.invalid_prop"));
        assert!(issues.iter().any(|issue| issue.code == "tree.duplicate_id"));
    }
}
