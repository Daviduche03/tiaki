//! Structured app trees for computer use.
//!
//! Every native app exposes a semantic node tree: stable ids, roles, labels,
//! values, actions, and bounds. The runtime acts on nodes, not pixels. Vision
//! is a fallback, not the primary path.

use std::collections::BTreeMap;
use std::fmt;

use agentic_json::Json;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Window,
    Dialog,
    Button,
    TextField,
    Label,
    Checkbox,
    Toggle,
    List,
    ListItem,
    Menu,
    MenuItem,
    Progress,
    Image,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Window => "window",
            Role::Dialog => "dialog",
            Role::Button => "button",
            Role::TextField => "text-field",
            Role::Label => "label",
            Role::Checkbox => "checkbox",
            Role::Toggle => "toggle",
            Role::List => "list",
            Role::ListItem => "list-item",
            Role::Menu => "menu",
            Role::MenuItem => "menu-item",
            Role::Progress => "progress",
            Role::Image => "image",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "window" => Some(Role::Window),
            "dialog" => Some(Role::Dialog),
            "button" => Some(Role::Button),
            "text-field" => Some(Role::TextField),
            "label" => Some(Role::Label),
            "checkbox" => Some(Role::Checkbox),
            "toggle" => Some(Role::Toggle),
            "list" => Some(Role::List),
            "list-item" => Some(Role::ListItem),
            "menu" => Some(Role::Menu),
            "menu-item" => Some(Role::MenuItem),
            "progress" => Some(Role::Progress),
            "image" => Some(Role::Image),
            _ => None,
        }
    }

    pub fn allows_children(self) -> bool {
        matches!(self, Role::Window | Role::Dialog | Role::List | Role::Menu)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionKind {
    Invoke,
    SetText,
    Toggle,
    Select,
    Focus,
    Scroll,
}

impl ActionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ActionKind::Invoke => "invoke",
            ActionKind::SetText => "set-text",
            ActionKind::Toggle => "toggle",
            ActionKind::Select => "select",
            ActionKind::Focus => "focus",
            ActionKind::Scroll => "scroll",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "invoke" => Some(ActionKind::Invoke),
            "set-text" => Some(ActionKind::SetText),
            "toggle" => Some(ActionKind::Toggle),
            "select" => Some(ActionKind::Select),
            "focus" => Some(ActionKind::Focus),
            "scroll" => Some(ActionKind::Scroll),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Risk {
    Low,
    Medium,
    High,
}

impl Risk {
    pub fn as_str(self) -> &'static str {
        match self {
            Risk::Low => "low",
            Risk::Medium => "medium",
            Risk::High => "high",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "low" => Some(Risk::Low),
            "medium" => Some(Risk::Medium),
            "high" => Some(Risk::High),
            _ => None,
        }
    }

    /// High-risk actions always require trusted input. Medium is reserved for
    /// policy that wants a lower threshold.
    pub fn requires_approval(self) -> bool {
        self == Risk::High
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppNode {
    pub id: String,
    pub role: Role,
    pub label: String,
    pub value: Option<String>,
    pub enabled: bool,
    pub focused: bool,
    pub bounds: Option<Bounds>,
    pub actions: Vec<ActionKind>,
    pub risk: Risk,
    pub children: Vec<AppNode>,
}

impl AppNode {
    pub fn new(id: impl Into<String>, role: Role, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            role,
            label: label.into(),
            value: None,
            enabled: true,
            focused: false,
            bounds: None,
            actions: Vec::new(),
            risk: Risk::Low,
            children: Vec::new(),
        }
    }

    pub fn with_action(mut self, action: ActionKind) -> Self {
        if !self.actions.contains(&action) {
            self.actions.push(action);
        }
        self
    }

    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    pub fn high_risk(mut self) -> Self {
        self.risk = Risk::High;
        self
    }

    pub fn find(&self, id: &str) -> Option<&AppNode> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|child| child.find(id))
    }

    pub fn find_mut(&mut self, id: &str) -> Option<&mut AppNode> {
        if self.id == id {
            return Some(self);
        }
        self.children
            .iter_mut()
            .find_map(|child| child.find_mut(id))
    }

    pub fn find_label(&self, label: &str) -> Option<&AppNode> {
        if self.label.eq_ignore_ascii_case(label) {
            return Some(self);
        }
        self.children
            .iter()
            .find_map(|child| child.find_label(label))
    }

    pub fn walk(&self, visit: &mut impl FnMut(&AppNode)) {
        visit(self);
        for child in &self.children {
            child.walk(visit);
        }
    }

    pub fn count(&self) -> usize {
        1 + self.children.iter().map(AppNode::count).sum::<usize>()
    }

    pub fn depth(&self) -> usize {
        1 + self.children.iter().map(AppNode::depth).max().unwrap_or(0)
    }

    pub fn from_json(value: &Json) -> Result<Self, AppTreeError> {
        let object = value
            .as_object()
            .ok_or_else(|| AppTreeError::new("a node must be a JSON object"))?;
        for key in object.keys() {
            if !matches!(
                key.as_str(),
                "id" | "role"
                    | "label"
                    | "value"
                    | "enabled"
                    | "focused"
                    | "bounds"
                    | "actions"
                    | "risk"
                    | "children"
            ) {
                return Err(AppTreeError::new(format!("unknown node field '{key}'")));
            }
        }

        let id = object
            .get("id")
            .and_then(Json::as_str)
            .ok_or_else(|| AppTreeError::new("a node needs a string id"))?
            .to_string();
        let role_text = object
            .get("role")
            .and_then(Json::as_str)
            .ok_or_else(|| AppTreeError::new(format!("node '{id}' needs a role")))?;
        let role = Role::parse(role_text)
            .ok_or_else(|| AppTreeError::new(format!("unknown role '{role_text}'")))?;
        let label = object
            .get("label")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();

        let actions = object
            .get("actions")
            .and_then(Json::as_array)
            .map(|values| {
                values
                    .iter()
                    .map(|value| {
                        value
                            .as_str()
                            .and_then(ActionKind::parse)
                            .ok_or_else(|| AppTreeError::new("invalid action"))
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default();

        let bounds = object
            .get("bounds")
            .map(|value| {
                let object = value
                    .as_object()
                    .ok_or_else(|| AppTreeError::new("bounds must be an object"))?;
                let read = |key: &str| {
                    object
                        .get(key)
                        .and_then(Json::as_f64)
                        .map(|value| value as i64)
                        .ok_or_else(|| AppTreeError::new(format!("bounds needs '{key}'")))
                };
                Ok::<_, AppTreeError>(Bounds {
                    x: read("x")?,
                    y: read("y")?,
                    width: read("width")?,
                    height: read("height")?,
                })
            })
            .transpose()?;

        let children = object
            .get("children")
            .and_then(Json::as_array)
            .map(|values| values.iter().map(AppNode::from_json).collect())
            .transpose()?
            .unwrap_or_default();

        Ok(Self {
            id,
            role,
            label,
            value: object
                .get("value")
                .and_then(Json::as_str)
                .map(str::to_string),
            enabled: object
                .get("enabled")
                .and_then(Json::as_bool)
                .unwrap_or(true),
            focused: object
                .get("focused")
                .and_then(Json::as_bool)
                .unwrap_or(false),
            bounds,
            actions,
            risk: object
                .get("risk")
                .and_then(Json::as_str)
                .and_then(Risk::parse)
                .unwrap_or(Risk::Low),
            children,
        })
    }

    pub fn to_json(&self) -> Json {
        let mut object = BTreeMap::new();
        object.insert("id".to_string(), Json::String(self.id.clone()));
        object.insert("role".to_string(), Json::String(self.role.as_str().into()));
        object.insert("label".to_string(), Json::String(self.label.clone()));
        if let Some(value) = &self.value {
            object.insert("value".to_string(), Json::String(value.clone()));
        }
        object.insert("enabled".to_string(), Json::Bool(self.enabled));
        object.insert("focused".to_string(), Json::Bool(self.focused));
        if let Some(bounds) = &self.bounds {
            object.insert(
                "bounds".to_string(),
                Json::Object(
                    [
                        ("x", bounds.x),
                        ("y", bounds.y),
                        ("width", bounds.width),
                        ("height", bounds.height),
                    ]
                    .into_iter()
                    .map(|(key, value)| (key.to_string(), Json::Number(value as f64)))
                    .collect(),
                ),
            );
        }
        object.insert(
            "actions".to_string(),
            Json::Array(
                self.actions
                    .iter()
                    .map(|action| Json::String(action.as_str().into()))
                    .collect(),
            ),
        );
        object.insert("risk".to_string(), Json::String(self.risk.as_str().into()));
        if !self.children.is_empty() {
            object.insert(
                "children".to_string(),
                Json::Array(self.children.iter().map(AppNode::to_json).collect()),
            );
        }
        Json::Object(object)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppTree {
    pub app: String,
    pub title: String,
    pub root: AppNode,
}

impl AppTree {
    pub fn parse(text: &str) -> Result<Self, AppTreeError> {
        let value =
            agentic_json::parse(text).map_err(|error| AppTreeError::new(error.to_string()))?;
        let object = value
            .as_object()
            .ok_or_else(|| AppTreeError::new("an app tree must be a JSON object"))?;
        let app = object
            .get("app")
            .and_then(Json::as_str)
            .ok_or_else(|| AppTreeError::new("an app tree needs an 'app' id"))?
            .to_string();
        let title = object
            .get("title")
            .and_then(Json::as_str)
            .unwrap_or(&app)
            .to_string();
        let root = AppNode::from_json(
            object
                .get("root")
                .ok_or_else(|| AppTreeError::new("an app tree needs a 'root'"))?,
        )?;
        Ok(Self { app, title, root })
    }

    pub fn render(&self) -> String {
        self.to_json().render()
    }

    pub fn to_json(&self) -> Json {
        Json::Object(
            [
                ("app".to_string(), Json::String(self.app.clone())),
                ("title".to_string(), Json::String(self.title.clone())),
                ("root".to_string(), self.root.to_json()),
            ]
            .into_iter()
            .collect(),
        )
    }

    pub fn find(&self, id: &str) -> Option<&AppNode> {
        self.root.find(id)
    }

    pub fn find_label(&self, label: &str) -> Option<&AppNode> {
        self.root.find_label(label)
    }

    pub fn validate(&self) -> Vec<String> {
        let mut issues = Vec::new();
        if self.app.trim().is_empty() {
            issues.push("app id must not be empty".to_string());
        }
        if self.root.count() > 512 {
            issues.push("app tree is too large (limit 512 nodes)".to_string());
        }
        if self.root.depth() > 16 {
            issues.push("app tree is too deep (limit 16 levels)".to_string());
        }
        let mut seen = std::collections::BTreeSet::new();
        self.root.walk(&mut |node| {
            if node.id.trim().is_empty() {
                issues.push("node id must not be empty".to_string());
            }
            if !seen.insert(node.id.clone()) {
                issues.push(format!("duplicate node id '{}'", node.id));
            }
            if !node.role.allows_children() && !node.children.is_empty() {
                issues.push(format!(
                    "role '{}' cannot have children (node '{}')",
                    node.role.as_str(),
                    node.id
                ));
            }
            if let Some(bounds) = node.bounds {
                if bounds.width <= 0 || bounds.height <= 0 {
                    issues.push(format!("node '{}' has non-positive bounds", node.id));
                }
            }
        });
        issues
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppTreeError {
    pub message: String,
}

impl AppTreeError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for AppTreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AppTreeError {}

#[cfg(test)]
pub(crate) fn sample_tree() -> AppTree {
    let mut window = AppNode::new("checkout", Role::Window, "Checkout");
    window.children = vec![
        AppNode::new("heading", Role::Label, "Order summary"),
        AppNode::new("gift", Role::Checkbox, "Gift wrap")
            .with_action(ActionKind::Toggle)
            .with_value("off"),
        AppNode::new("address", Role::TextField, "Delivery address")
            .with_action(ActionKind::SetText)
            .with_value("1 Main St"),
        AppNode::new("place", Role::Button, "Place order")
            .with_action(ActionKind::Invoke)
            .with_value("idle")
            .high_risk(),
    ];
    AppTree {
        app: "checkout".into(),
        title: "Checkout".into(),
        root: window,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checkout() -> AppTree {
        sample_tree()
    }

    #[test]
    fn round_trips() {
        let tree = checkout();
        let parsed = AppTree::parse(&tree.render()).expect("parse");
        assert_eq!(parsed, tree);
    }

    #[test]
    fn finds_by_role_and_label() {
        let tree = checkout();
        assert_eq!(tree.find_label("Place order").unwrap().id, "place");
        assert_eq!(tree.find("gift").unwrap().role, Role::Checkbox);
    }

    #[test]
    fn validates_duplicates_and_bad_bounds() {
        let mut tree = checkout();
        let mut duplicate = AppNode::new("gift", Role::Label, "Dup");
        duplicate.bounds = Some(Bounds {
            x: 0,
            y: 0,
            width: 0,
            height: 10,
        });
        tree.root.children.push(duplicate);
        let issues = tree.validate();
        assert!(issues.iter().any(|issue| issue.contains("duplicate")));
        assert!(issues.iter().any(|issue| issue.contains("bounds")));
    }
}
