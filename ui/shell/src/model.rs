//! Reading the shell out of the UI tree.
//!
//! The shell is not configured by a private file: it renders nodes owned by
//! the control plane. `/tree.json` is the source of truth; the tree baked into
//! the binary is only an offline fallback for the first paint.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use agentic_json::Json;
use agentic_ui_tree::Node;

const EMBEDDED_TREE: &str = include_str!("../../../desktop/desktop.json");
const TREE_URL: &str = "http://127.0.0.1:8787/tree.json";

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct AppItem {
    pub id: String,
    pub label: String,
    pub icon: String,
    pub art: String,
    pub command: String,
    pub category: String,
    pub description: String,
    pub color: String,
    pub running: bool,
}

#[derive(Clone, Debug)]
pub struct MenuItem {
    pub label: String,
    pub action: String,
    pub shortcut: String,
}

#[derive(Clone, Debug)]
pub enum MenuEntry {
    Item(MenuItem),
    Separator,
}

#[derive(Clone, Debug)]
pub struct MenuDef {
    pub id: String,
    pub label: String,
    pub items: Vec<MenuEntry>,
}

pub fn load_tree() -> Node {
    if let Ok(path) = std::env::var("TIAKI_TREE_FILE") {
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(tree) = agentic_ui_tree::UiTree::parse(&text) {
                return tree.root;
            }
        }
    }

    if let Some(body) = http_get(TREE_URL, Duration::from_millis(700)) {
        if let Ok(value) = agentic_json::parse(&body) {
            if let Some(tree) = value.as_object().and_then(|object| object.get("tree")) {
                if let Ok(node) = Node::from_json(tree) {
                    return node;
                }
            }
        }
    }

    agentic_ui_tree::UiTree::parse(EMBEDDED_TREE)
        .map(|tree| tree.root)
        .unwrap_or_else(|_| Node::new("desktop", "workspace"))
}

pub fn find_by_id<'a>(root: &'a Node, id: &str) -> Option<&'a Node> {
    if root.id == id {
        return Some(root);
    }
    root.children.iter().find_map(|child| find_by_id(child, id))
}

pub fn find_by_role<'a>(root: &'a Node, role: &str) -> Option<&'a Node> {
    if root.prop_str("role") == Some(role) {
        return Some(root);
    }
    root.children
        .iter()
        .find_map(|child| find_by_role(child, role))
}

pub fn apps_in(panel: &Node) -> Vec<AppItem> {
    panel
        .children
        .iter()
        .filter(|child| child.kind == "app")
        .map(app_from_node)
        .collect()
}

fn app_from_node(node: &Node) -> AppItem {
    AppItem {
        id: node.id.clone(),
        label: node.prop_str("label").unwrap_or("Application").to_string(),
        icon: node
            .prop_str("icon")
            .unwrap_or("application-x-executable-symbolic")
            .to_string(),
        art: node.prop_str("art").unwrap_or("app").to_string(),
        command: node.prop_str("command").unwrap_or("").to_string(),
        category: node
            .prop_str("category")
            .unwrap_or("Application")
            .to_string(),
        description: node.prop_str("description").unwrap_or("").to_string(),
        color: node.prop_str("color").unwrap_or("slate").to_string(),
        running: node.prop_bool("running").unwrap_or(false),
    }
}

/// Every app the launcher can open: the launcher panel first, then any pinned
/// dock apps that are not already listed.
pub fn dock_apps(root: &Node) -> Vec<AppItem> {
    find_by_role(root, "dock").map(apps_in).unwrap_or_default()
}

pub fn launcher_apps(root: &Node) -> Vec<AppItem> {
    let mut items: Vec<AppItem> = Vec::new();
    if let Some(panel) = find_by_id(root, "launcher") {
        items.extend(apps_in(panel));
    }
    if let Some(dock) = find_by_role(root, "dock") {
        for app in apps_in(dock) {
            if app.art == "trash" {
                continue;
            }
            if !items.iter().any(|item| item.label == app.label) {
                items.push(app);
            }
        }
    }
    if items.is_empty() {
        root.walk(&mut |node| {
            if node.kind == "app" {
                let app = app_from_node(node);
                if !items.iter().any(|item| item.label == app.label) {
                    items.push(app);
                }
            }
        });
    }
    items
}

pub fn menu_by_id(root: &Node, id: &str) -> Option<MenuDef> {
    root.find(id).map(menu_from_node)
}

pub fn menus_in(bar: &Node) -> Vec<MenuDef> {
    let mut menus = Vec::new();
    collect_menus(bar, &mut menus);
    menus
}

fn collect_menus(node: &Node, menus: &mut Vec<MenuDef>) {
    for child in &node.children {
        if child.kind == "menu" {
            menus.push(menu_from_node(child));
        } else {
            collect_menus(child, menus);
        }
    }
}

fn menu_from_node(menu: &Node) -> MenuDef {
    MenuDef {
        id: menu.id.clone(),
        label: menu.prop_str("label").unwrap_or("Menu").to_string(),
        items: menu
            .children
            .iter()
            .filter_map(|child| match child.kind.as_str() {
                "menuitem" => Some(MenuEntry::Item(MenuItem {
                    label: child.prop_str("label").unwrap_or("Item").to_string(),
                    action: child.prop_str("action").unwrap_or("noop").to_string(),
                    shortcut: child.prop_str("shortcut").unwrap_or("").to_string(),
                })),
                "separator" => Some(MenuEntry::Separator),
                _ => None,
            })
            .collect(),
    }
}

pub fn status_items(bar: &Node) -> Vec<String> {
    bar.children
        .iter()
        .find(|child| child.kind == "status")
        .and_then(|status| status.prop("items"))
        .and_then(Json::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

pub fn clock_node<'a>(bar: &'a Node) -> Option<&'a Node> {
    bar.children.iter().find(|child| child.kind == "clock")
}

/// Tailor a tree menu to the focused app. Window and Assistant are global
/// and pass through; File/Edit/View are per-app so Quit never exits the
/// session and terminals don't offer Undo.
pub fn filter_menu(menu: &MenuDef, app_id: &str) -> MenuDef {
    if menu.id != "menu.file" && menu.id != "menu.edit" && menu.id != "menu.view" {
        return menu.clone();
    }
    let app = app_id.to_lowercase();
    let is_terminal = app.contains("foot") || app.contains("btop") || app.contains("agentd");
    let is_editor = app.contains("mousepad") || app.contains("editor");
    let is_files = app.contains("thunar") || app.contains("files") || app.contains("trash");
    let is_browser = app.contains("epiphany") || app.contains("browser") || app.contains("chrom") || app.contains("firefox");

    let item = |label: &str, action: &str, shortcut: &str| {
        MenuEntry::Item(MenuItem {
            label: label.to_string(),
            action: action.to_string(),
            shortcut: shortcut.to_string(),
        })
    };
    let sep = || MenuEntry::Separator;
    let close_quit = vec![
        item("Close Window", "window.close", "⌘W"),
        sep(),
        item("Quit", "window.close", "⌘Q"),
    ];

    let items = match menu.id.as_str() {
        "menu.file" => {
            let mut items = if is_terminal {
                vec![
                    item("New Window", "terminal.new", "⌘N"),
                    item("New Terminal", "terminal.new", "⌘T"),
                    sep(),
                ]
            } else if is_editor {
                vec![item("New Window", "editor.open", "⌘N"), sep()]
            } else if is_files {
                vec![item("New Window", "files.open", "⌘N"), sep()]
            } else if is_browser {
                vec![item("New Window", "browser.open", "⌘N"), sep()]
            } else {
                vec![]
            };
            items.extend(close_quit);
            items
        }
        "menu.edit" => {
            if is_terminal {
                vec![
                    item("Copy", "edit.copy", "⌘C"),
                    item("Paste", "edit.paste", "⌘V"),
                ]
            } else {
                menu.items.clone()
            }
        }
        "menu.view" => {
            if is_terminal {
                vec![
                    item("Toggle Fullscreen", "view.fullscreen", "⌃⌘F"),
                    item("Zoom", "window.zoom", ""),
                ]
            } else if is_browser {
                vec![
                    item("Reload UI", "ui.reload", "⌘R"),
                    item("Toggle Fullscreen", "view.fullscreen", "⌃⌘F"),
                ]
            } else {
                menu.items.clone()
            }
        }
        _ => menu.items.clone(),
    };
    MenuDef {
        id: menu.id.clone(),
        label: menu.label.clone(),
        items,
    }
}

pub fn http_get(url: &str, timeout: Duration) -> Option<String> {
    let rest = url.strip_prefix("http://")?;
    let (host, path) = match rest.split_once('/') {
        Some((host, path)) => (host, format!("/{path}")),
        None => (rest, "/".to_string()),
    };
    let mut stream = TcpStream::connect(host).ok()?;
    stream.set_read_timeout(Some(timeout)).ok()?;
    stream.set_write_timeout(Some(timeout)).ok()?;
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
    )
    .ok()?;
    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;
    let (_, body) = response.split_once("\r\n\r\n")?;
    Some(body.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> Node {
        agentic_ui_tree::UiTree::parse(EMBEDDED_TREE)
            .expect("embedded tree")
            .root
    }

    #[test]
    fn reads_shell_panels_from_the_tree() {
        let root = tree();
        assert!(find_by_id(&root, "bar").is_some());
        assert!(find_by_role(&root, "dock").is_some());
        assert!(find_by_role(&root, "launcher").is_some());
    }

    #[test]
    fn reads_menus_and_apps() {
        let root = tree();
        let bar = find_by_id(&root, "bar").expect("bar");
        let menus = menus_in(bar);
        assert!(menus.iter().any(|menu| menu.label == "File"));
        assert!(menu_by_id(&root, "menu.file")
            .map(|menu| menu.items.len() >= 3)
            .unwrap_or(false));

        let apps = launcher_apps(&root);
        assert!(apps.iter().any(|app| app.label == "Terminal"));
        assert!(apps.iter().any(|app| app.command.starts_with("foot")));
        assert!(apps.iter().all(|app| !app.command.is_empty()));
    }
}
