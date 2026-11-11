//! HTML renderer for the declarative UI tree.
//!
//! This is the host-side renderer used before the Wayland compositor exists.
//! The same tree drives the real session-tier renderer later; only the backend
//! changes.
//!
//! Rust emits no HTML. The renderer walks the tree and returns JSON; the
//! page (assets/page.html + page.js) builds the DOM from it. Styles live in
//! assets/page.css; no CSS, HTML, or JavaScript literal appears in this file.

#![allow(dead_code)]

use agentic_json::Json;
use agentic_ui_tree::{Keymap, Node, UiTree};

/// Render a full page. `revision` lets the page poll for agent changes.
///
/// What reaches the browser is a change order, not markup: the boot JSON
/// carries the tree, keymap, revision and theme, and page.js mounts it.
pub fn render_page(tree: &UiTree, keymap: &Keymap, theme: &str, revision: u64) -> String {
    let boot = Json::Object(
        [
            ("tree".into(), tree.root.to_json()),
            ("keymap".into(), keymap.bindings_json()),
            ("revision".into(), Json::Number(revision as f64)),
            ("theme".into(), Json::String(theme.into())),
        ]
        .into_iter()
        .collect(),
    );
    PAGE.replace("__BOOT_JSON__", &escape_script_break(boot.render()))
}

/// Render the swappable screen fragment as JSON, not markup.
pub fn render_screen(tree: &UiTree, theme: &str) -> String {
    screen_json(&tree.root, theme).render()
}

/// Wrap the tree root in the page's theme envelope.
fn screen_json(root: &Node, theme: &str) -> Json {
    Json::Object(
        [
            ("theme".into(), Json::String(theme.into())),
            ("root".into(), node_json(root)),
        ]
        .into_iter()
        .collect(),
    )
}

/// A cheap content hash so the page knows when to re-fetch.
pub fn revision(tree_json: &str, keymap_json: &str, theme: &str) -> u64 {
    let mut hash = 1469598103934665603u64;
    for byte in tree_json
        .bytes()
        .chain(std::iter::once(0))
        .chain(keymap_json.bytes())
        .chain(std::iter::once(0))
        .chain(theme.bytes())
    {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    hash
}

/// JSON describing one node for the page to mount. Text is data, not markup —
/// page.js sets it with textContent, so user content can never become an
/// element.
fn node_json(node: &Node) -> Json {
    fn children_of(node: &Node) -> Vec<Json> {
        node.children.iter().map(node_json).collect()
    }
    fn text(value: &str) -> Json {
        Json::Object(
            [("text".into(), Json::String(value.into()))]
                .into_iter()
                .collect(),
        )
    }
    fn object(fields: Vec<(&str, Json)>) -> Json {
        Json::Object(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }

    match node.kind.as_str() {
        "workspace" => object(vec![
            ("tag", Json::String("main".into())),
            ("cls", Json::String("workspace".into())),
            ("id", Json::String(node.id.clone())),
            (
                "children",
                Json::Array({
                    let title = node.prop_str("title").unwrap_or("Tiaki");
                    let mut kids = vec![object(vec![
                        ("tag", Json::String("div".into())),
                        ("cls", Json::String("workspace-title".into())),
                        ("children", Json::Array(vec![text(title)])),
                    ])];
                    kids.extend(children_of(node));
                    kids
                }),
            ),
        ]),
        "panel" => {
            let layout = node.prop_str("layout").unwrap_or("row");
            let dir = if layout == "column" { "column" } else { "row" };
            let mut kids = Vec::new();
            if let Some(title) = node.prop_str("title") {
                kids.push(object(vec![
                    ("tag", Json::String("div".into())),
                    ("cls", Json::String("panel-title".into())),
                    ("children", Json::Array(vec![text(title)])),
                ]));
            }
            let body = object(vec![
                ("tag", Json::String("div".into())),
                ("cls", Json::String(format!("panel-body panel-{dir}"))),
                ("children", Json::Array(children_of(node))),
            ]);
            kids.push(body);
            object(vec![
                ("tag", Json::String("section".into())),
                ("cls", Json::String(format!("panel panel-{dir}"))),
                ("id", Json::String(node.id.clone())),
                ("children", Json::Array(kids)),
            ])
        }
        "clock" => {
            let format = node.prop_str("format").unwrap_or("24h");
            let seconds = node.prop_bool("show_seconds").unwrap_or(false);
            object(vec![
                ("tag", Json::String("div".into())),
                ("cls", Json::String("clock focusable".into())),
                ("id", Json::String(node.id.clone())),
                ("tabindex", Json::Number(0.0)),
                (
                    "ds",
                    Json::Object(
                        [
                            ("format".into(), Json::String(format.into())),
                            ("seconds".into(), Json::String(seconds.to_string())),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                ),
                (
                    "children",
                    Json::Array(vec![object(vec![
                        ("tag", Json::String("span".into())),
                        ("cls", Json::String("clock-time".into())),
                        ("children", Json::Array(vec![text("--:--")])),
                    ])]),
                ),
            ])
        }
        "spacer" => {
            let flex = node.prop_f64("flex").unwrap_or(1.0);
            object(vec![
                ("tag", Json::String("div".into())),
                ("cls", Json::String("spacer".into())),
                ("id", Json::String(node.id.clone())),
                ("style", Json::String(format!("flex:{flex};"))),
            ])
        }
        "status" => {
            let items = node
                .props
                .get("items")
                .and_then(|value| value.as_array())
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|value| value.as_str())
                        .map(|item| {
                            object(vec![
                                ("tag", Json::String("span".into())),
                                ("cls", Json::String(format!("chip chip-{item}"))),
                                ("title", Json::String(item.into())),
                                ("children", Json::Array(vec![text(item)])),
                            ])
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            object(vec![
                ("tag", Json::String("div".into())),
                ("cls", Json::String("status".into())),
                ("id", Json::String(node.id.clone())),
                ("children", Json::Array(items)),
            ])
        }
        "text" => {
            let value = node.prop_str("value").unwrap_or("");
            let size = node.prop_str("size").unwrap_or("body");
            object(vec![
                ("tag", Json::String("div".into())),
                ("cls", Json::String(format!("text text-{size}"))),
                ("id", Json::String(node.id.clone())),
                ("children", Json::Array(vec![text(value)])),
            ])
        }
        "button" => {
            let label = node.prop_str("label").unwrap_or("Button");
            let action = node.prop_str("action").unwrap_or("noop");
            object(vec![
                ("tag", Json::String("button".into())),
                ("cls", Json::String("button focusable".into())),
                ("id", Json::String(node.id.clone())),
                ("tabindex", Json::Number(0.0)),
                (
                    "ds",
                    Json::Object(
                        [("action".into(), Json::String(action.into()))]
                            .into_iter()
                            .collect(),
                    ),
                ),
                ("children", Json::Array(vec![text(label)])),
            ])
        }
        "card" => {
            let title = node.prop_str("title").unwrap_or("Card");
            let body = node.prop_str("body").unwrap_or("");
            let action = node.prop_str("action").unwrap_or("noop");
            object(vec![
                ("tag", Json::String("button".into())),
                ("cls", Json::String("card focusable".into())),
                ("id", Json::String(node.id.clone())),
                ("tabindex", Json::Number(0.0)),
                (
                    "ds",
                    Json::Object(
                        [("action".into(), Json::String(action.into()))]
                            .into_iter()
                            .collect(),
                    ),
                ),
                (
                    "children",
                    Json::Array(vec![
                        object(vec![
                            ("tag", Json::String("span".into())),
                            ("cls", Json::String("card-title".into())),
                            ("children", Json::Array(vec![text(title)])),
                        ]),
                        object(vec![
                            ("tag", Json::String("span".into())),
                            ("cls", Json::String("card-body".into())),
                            ("children", Json::Array(vec![text(body)])),
                        ]),
                    ]),
                ),
            ])
        }
        other => object(vec![
            ("tag", Json::String("div".into())),
            ("cls", Json::String("unknown".into())),
            ("id", Json::String(node.id.clone())),
            (
                "children",
                Json::Array(vec![text(&format!("unknown widget: {other}"))]),
            ),
        ]),
    }
}

/// JSON text destined for an inline `<script>` block cannot contain the
/// closing sequence; escaping the slash makes it inert to the HTML parser
/// while staying valid JSON.
fn escape_script_break(value: String) -> String {
    value.replace("</", "<\\/")
}

pub const CSS: &str = include_str!("../assets/page.css");

pub const SCRIPT: &str = include_str!("../assets/page.js");

const PAGE: &str = include_str!("../assets/page.html");

#[cfg(test)]
mod tests {
    use super::*;
    use agentic_ui_tree::Keymap;

    #[test]
    fn renders_the_default_tree() {
        let tree = UiTree::default_for("user:ada");
        let keymap = Keymap::default_for();
        let page = render_page(&tree, &keymap, "dark", 42);
        assert!(page.contains("panel.top"));
        assert!(page.contains("Command palette"));
        assert!(page.contains("42"));
        assert!(page.contains("dark"));
    }

    #[test]
    fn escapes_html() {
        let mut tree = UiTree::default_for("user:ada");
        tree.root.find_mut("hint").unwrap().props.insert(
            "value".into(),
            agentic_json::Json::String("<script>x</script>".into()),
        );
        // The fragment is JSON data (textContent-built), never markup: the
        // payload can only ever become text, not an element.
        let json = render_screen(&tree, "dark");
        assert!(json.contains("<script>x"));
        // And when it is embedded in the page's <script type=application/json>
        // block, the closing sequence is neutralised.
        let page = render_page(&tree, &Keymap::default_for(), "dark", 0);
        assert!(!page.contains("</script>x"));
    }

    #[test]
    fn revision_changes_with_content() {
        let a = revision("{\"a\":1}", "{}", "dark");
        let b = revision("{\"a\":2}", "{}", "dark");
        assert_ne!(a, b);
    }
}
