//! agentd command: Tree UI: serve, preview render, demo tree, HTTP dispatch.
use crate::status::status_label;
use crate::{now_nanos, Global, Ledger};
use agentic_capability::Principal;
use agentic_core_log::{Impact, Status, Summary};
use agentic_json::Json;
use agentic_transaction::{Operation, SandboxClass, Severity, StateStore};
use agentic_ui_tree::{apply as apply_ui_op, Keymap, UiOp, UiTree};
use agentic_widgets;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

pub(crate) fn ui(
    engine: &mut Ledger,
    global: &Global,
    args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(command) = args.get(1).map(String::as_str) else {
        print_ui_usage();
        return Ok(());
    };

    let principal = effective_principal(&global.principal);
    let tree_path = ui_scope(global, "ui/tree");
    let keymap_path = ui_scope(global, "input/keymap");
    let theme_path = ui_scope(global, "theme/mode");

    match command {
        "get" => {
            let tree = load_tree(engine, &tree_path, &principal);
            println!("{}", tree.render());
        }
        "render" => {
            let tree = load_tree(engine, &tree_path, &principal);
            let keymap = load_keymap(engine, &keymap_path);
            let theme = theme_of(engine, &theme_path);
            let page = render_page(&tree, &keymap, &theme);
            match output_path(args) {
                Some(path) => {
                    std::fs::write(path, page)?;
                    println!("wrote {path}");
                }
                None => println!("{page}"),
            }
        }
        "serve" => serve(engine, global, args)?,
        "set" => {
            let file = require_ui_file(args, "set")?;
            let text = std::fs::read_to_string(file)?;
            let tree = UiTree::parse(&text)?;
            let issues = agentic_ui_tree::validate(&tree);
            if has_errors(&issues) {
                print_issues(&issues);
                return Err("the UI tree is invalid".into());
            }
            let before = load_tree(engine, &tree_path, &principal);
            let changes = agentic_ui_tree::diff(&before, &tree);
            let intent = describe_changes("Set the UI", &changes);
            apply_change(engine, global, &tree_path, tree.render(), &intent)?;
        }
        "edit" => {
            let file = require_ui_file(args, "edit")?;
            let text = std::fs::read_to_string(file)?;
            let ops = agentic_ui_tree::parse_ops(&text)?;
            let before = load_tree(engine, &tree_path, &principal);
            let mut after = before.clone();
            agentic_ui_tree::apply_ops(&mut after, &ops)?;
            let issues = agentic_ui_tree::validate(&after);
            if has_errors(&issues) {
                print_issues(&issues);
                return Err("the edited UI tree is invalid".into());
            }
            let changes = agentic_ui_tree::diff(&before, &after);
            let intent = describe_changes("Edit the UI", &changes);
            apply_change(engine, global, &tree_path, after.render(), &intent)?;
            for change in &changes {
                println!("  {change}");
            }
        }
        "reset" => {
            let intent = "Reset the UI tree to its default";
            remove_change(engine, global, &tree_path, intent)?;
        }
        "demo" => {
            let before = load_tree(engine, &tree_path, &principal);
            let after = demo_tree(&principal);
            let issues = agentic_ui_tree::validate(&after);
            if has_errors(&issues) {
                print_issues(&issues);
                return Err("the demo tree is invalid".into());
            }
            let changes = agentic_ui_tree::diff(&before, &after);
            let intent = describe_changes("Rebuild the panel", &changes);
            apply_change(engine, global, &tree_path, after.render(), &intent)?;
            for change in &changes {
                println!("  {change}");
            }
            println!("open the UI with: agentd ui serve");
        }
        "keymap" => keymap_command(engine, global, args, &keymap_path)?,
        other => return Err(format!("unknown ui command '{other}'").into()),
    }

    Ok(())
}

pub(crate) fn keymap_command(
    engine: &mut Ledger,
    global: &Global,
    args: &[String],
    keymap_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    match args.get(2).map(String::as_str) {
        None => {
            let keymap = load_keymap(engine, keymap_path);
            println!("{}", keymap.render());
        }
        Some("default") => {
            remove_change(
                engine,
                global,
                keymap_path,
                "Reset the keymap to its default",
            )?;
        }
        Some("set") => {
            let Some(file) = args.get(3) else {
                return Err("usage: agentd ui keymap set <file>".into());
            };
            let text = std::fs::read_to_string(file)?;
            let keymap = Keymap::parse(&text)?;
            let issues = keymap.validate();
            if has_errors(&issues) {
                print_issues(&issues);
                return Err("the keymap is invalid".into());
            }
            let count = keymap.bindings.len();
            apply_change(
                engine,
                global,
                keymap_path,
                keymap.render(),
                &format!("Rebind the keyboard ({count} bindings)"),
            )?;
        }
        Some(other) => return Err(format!("unknown keymap command '{other}'").into()),
    }
    Ok(())
}

pub(crate) fn serve(
    engine: &mut Ledger,
    global: &Global,
    args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let port = args
        .iter()
        .position(|arg| arg == "--port")
        .and_then(|index| args.get(index + 1))
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8787);
    let host = args
        .iter()
        .position(|arg| arg == "--host")
        .and_then(|index| args.get(index + 1))
        .map(String::as_str)
        .unwrap_or("127.0.0.1");

    let listener = TcpListener::bind((host, port))?;
    println!("tiaki ui on http://{host}:{port}");
    println!("  Super+Space or /  command palette");
    println!("  Super+Left/Right  move focus");
    println!("  Super+T           toggle theme");
    println!("  Super+K           toggle clock format");
    println!("  Ctrl-C to stop");

    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(stream) => stream,
            Err(_) => continue,
        };
        let _ = handle_http(&mut stream, engine, global);
    }
    Ok(())
}

pub(crate) fn handle_http(
    stream: &mut TcpStream,
    engine: &mut Ledger,
    global: &Global,
) -> std::io::Result<()> {
    let mut buffer = [0u8; 8192];
    let read = stream.read(&mut buffer)?;
    if read == 0 {
        return Ok(());
    }
    let request = String::from_utf8_lossy(&buffer[..read]).to_string();
    let line = request.lines().next().unwrap_or("");
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");
    let (path, query) = target.split_once('?').unwrap_or((target, ""));

    if method != "GET" {
        return respond(
            stream,
            405,
            "text/plain; charset=utf-8",
            "method not allowed",
        );
    }

    // Pick up edits made by another agentd process (the CLI, an agent).
    let _ = engine.reload();

    let principal = effective_principal(&global.principal);
    let tree_path = ui_scope(global, "ui/tree");
    let keymap_path = ui_scope(global, "input/keymap");
    let theme_path = ui_scope(global, "theme/mode");

    match path {
        "/" => {
            let tree = load_tree(engine, &tree_path, &principal);
            let keymap = load_keymap(engine, &keymap_path);
            let theme = theme_of(engine, &theme_path);
            let page = render_page(&tree, &keymap, &theme);
            respond(stream, 200, "text/html; charset=utf-8", &page)
        }
        "/fragment" => {
            let tree = load_tree(engine, &tree_path, &principal);
            let theme = theme_of(engine, &theme_path);
            let fragment = agentic_widgets::render_screen(&tree, &theme);
            respond(stream, 200, "application/json", &fragment)
        }
        "/page.css" => {
            respond(stream, 200, "text/css; charset=utf-8", agentic_widgets::CSS)
        }
        "/page.js" => {
            respond(stream, 200, "text/javascript; charset=utf-8", agentic_widgets::SCRIPT)
        }
        "/state" => {
            let theme = theme_of(engine, &theme_path);
            let tree = load_tree(engine, &tree_path, &principal).render();
            let keymap = load_keymap(engine, &keymap_path).render();
            let revision = agentic_widgets::revision(&tree, &keymap, &theme);
            let body = format!(
                "{{\"revision\":{revision},\"theme\":\"{}\"}}",
                json_escape(&theme)
            );
            respond(stream, 200, "application/json", &body)
        }
        "/tree.json" => {
            let tree = load_tree(engine, &tree_path, &principal);
            let keymap = load_keymap(engine, &keymap_path);
            let theme = theme_of(engine, &theme_path);
            let revision = agentic_widgets::revision(&tree.render(), &keymap.render(), &theme);
            let body = Json::Object(
                [
                    ("revision".to_string(), Json::String(revision.to_string())),
                    ("theme".to_string(), Json::String(theme)),
                    ("tree".to_string(), tree.root.to_json()),
                    ("keymap".to_string(), keymap.bindings_json()),
                ]
                .into_iter()
                .collect(),
            )
            .render();
            respond(stream, 200, "application/json", &body)
        }
        "/action" => {
            let action = query_param(query, "name").unwrap_or_default();
            let body = match dispatch_action(engine, global, &tree_path, &theme_path, &action) {
                Ok(message) => format!("{{\"ok\":true,\"message\":\"{}\"}}", json_escape(&message)),
                Err(error) => format!("{{\"ok\":false,\"message\":\"{}\"}}", json_escape(&error)),
            };
            respond(stream, 200, "application/json", &body)
        }
        _ => respond(stream, 404, "text/plain; charset=utf-8", "not found"),
    }
}

pub(crate) fn dispatch_action(
    engine: &mut Ledger,
    global: &Global,
    tree_path: &str,
    theme_path: &str,
    action: &str,
) -> Result<String, String> {
    match action {
        "theme.toggle" => {
            let current = engine
                .store()
                .get(theme_path)
                .unwrap_or_else(|| "dark".to_string());
            let next = if current == "dark" { "light" } else { "dark" };
            apply_change(
                engine,
                global,
                theme_path,
                next.to_string(),
                "Toggle the theme from the keyboard",
            )
            .map_err(|error| error.to_string())?;
            Ok(format!("theme is now {next}"))
        }
        "clock.toggle_format" => {
            let principal = effective_principal(&global.principal);
            let mut tree = load_tree(engine, tree_path, &principal);
            let current = tree
                .find("clock")
                .and_then(|clock| clock.prop_str("format"))
                .unwrap_or("24h")
                .to_string();
            let next = if current == "24h" { "12h" } else { "24h" };
            apply_ui_op(
                &mut tree,
                &UiOp::SetProp {
                    id: "clock".into(),
                    key: "format".into(),
                    value: Json::String(next.to_string()),
                },
            )
            .map_err(|error| error.to_string())?;
            apply_change(
                engine,
                global,
                tree_path,
                tree.render(),
                "Toggle the clock format from the keyboard",
            )
            .map_err(|error| error.to_string())?;
            Ok(format!("clock format is now {next}"))
        }
        other => {
            let transaction_id = format!("action_{:x}", now_nanos());
            let _ = engine.record(
                &global.principal,
                global.mode,
                SandboxClass::Inspect,
                &transaction_id,
                &format!("Keyboard action {other}"),
                Summary::new(
                    format!("Dispatched {other} from the keyboard."),
                    String::new(),
                    "rule-based",
                    true,
                ),
                Impact::none(),
                Status::Committed,
                vec![format!("action {other}")],
            );
            Ok(format!("dispatched {other}"))
        }
    }
}

pub(crate) fn apply_change(
    engine: &mut Ledger,
    global: &Global,
    path: &str,
    value: String,
    intent: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let id = format!("txn_{:x}", now_nanos());
    match engine.apply(
        &id,
        &global.principal,
        global.mode,
        intent,
        vec![Operation::Set {
            path: path.to_string(),
            value,
        }],
    ) {
        Ok(transaction) => {
            println!("committed {}", transaction.id);
            println!("  {}", transaction.summary.one_line);
        }
        Err(error) => {
            println!(
                "{} {}",
                status_label(error.transaction.status),
                error.transaction.id
            );
            println!("  reason: {}", error.reason);
            std::process::exit(2);
        }
    }
    Ok(())
}

pub(crate) fn remove_change(
    engine: &mut Ledger,
    global: &Global,
    path: &str,
    intent: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let id = format!("txn_{:x}", now_nanos());
    match engine.apply(
        &id,
        &global.principal,
        global.mode,
        intent,
        vec![Operation::Remove {
            path: path.to_string(),
        }],
    ) {
        Ok(transaction) => {
            println!("committed {}", transaction.id);
            println!("  {}", transaction.summary.one_line);
        }
        Err(error) => {
            println!(
                "{} {}",
                status_label(error.transaction.status),
                error.transaction.id
            );
            println!("  reason: {}", error.reason);
            std::process::exit(2);
        }
    }
    Ok(())
}

pub(crate) fn load_tree(engine: &Ledger, path: &str, principal: &str) -> UiTree {
    engine
        .store()
        .get(path)
        .and_then(|text| UiTree::parse(&text).ok())
        .unwrap_or_else(|| UiTree::default_for(principal))
}

pub(crate) fn load_keymap(engine: &Ledger, path: &str) -> Keymap {
    engine
        .store()
        .get(path)
        .and_then(|text| Keymap::parse(&text).ok())
        .unwrap_or_else(Keymap::default_for)
}

pub(crate) fn theme_of(engine: &Ledger, path: &str) -> String {
    engine
        .store()
        .get(path)
        .unwrap_or_else(|| "dark".to_string())
}

pub(crate) fn render_page(tree: &UiTree, keymap: &Keymap, theme: &str) -> String {
    let revision = agentic_widgets::revision(&tree.render(), &keymap.render(), theme);
    agentic_widgets::render_page(tree, keymap, theme, revision)
}

pub(crate) fn demo_tree(principal: &str) -> UiTree {
    let mut tree = UiTree::default_for(principal);

    let clock = {
        let top = tree.root.find_mut("panel.top").expect("panel.top");
        let index = top
            .children
            .iter()
            .position(|node| node.id == "clock")
            .expect("clock");
        top.children.remove(index)
    };
    tree.root
        .find_mut("panel.main")
        .expect("panel.main")
        .children
        .insert(0, clock);

    let mut button = agentic_ui_tree::Node::new("cta", "button");
    button
        .props
        .insert("label".into(), Json::String("Open settings".into()));
    button
        .props
        .insert("action".into(), Json::String("settings.open".into()));
    tree.root
        .find_mut("panel.main")
        .expect("panel.main")
        .children
        .push(button);

    if let Some(clock) = tree.root.find_mut("clock") {
        clock
            .props
            .insert("format".into(), Json::String("12h".into()));
    }

    tree
}

pub(crate) fn describe_changes(prefix: &str, changes: &[String]) -> String {
    if changes.is_empty() {
        format!("{prefix} (no visible change)")
    } else {
        format!("{prefix}: {}", changes.join("; "))
    }
}

pub(crate) fn effective_principal(principal: &str) -> String {
    Principal::parse(principal)
        .map(|principal| principal.effective().to_string())
        .unwrap_or_else(|_| principal.to_string())
}

/// Scope a resource path for the acting principal.

pub(crate) fn ui_scope(global: &Global, suffix: &str) -> String {
    let effective = Principal::parse(&global.principal)
        .map(|principal| principal.effective().clone())
        .unwrap_or(Principal::User("local".into()));
    match effective {
        Principal::System => format!("system/{suffix}"),
        Principal::User(id) => format!("user/{id}/{suffix}"),
        Principal::Service(id) => format!("system/services/{id}/{suffix}"),
        Principal::Agent { .. } => format!("system/{suffix}"),
    }
}

pub(crate) fn output_path(args: &[String]) -> Option<&str> {
    args.iter()
        .position(|arg| arg == "--out")
        .and_then(|index| args.get(index + 1))
        .map(String::as_str)
}

pub(crate) fn require_ui_file<'a>(
    args: &'a [String],
    command: &str,
) -> Result<&'a str, Box<dyn std::error::Error>> {
    args.get(2)
        .map(String::as_str)
        .ok_or_else(|| format!("usage: agentd ui {command} <file>").into())
}

pub(crate) fn print_issues(issues: &[agentic_transaction::Issue]) {
    for issue in issues {
        let severity = match issue.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        let location = issue
            .path
            .as_deref()
            .map(|path| format!(" ({path})"))
            .unwrap_or_default();
        println!("  {severity}[{}]{location}: {}", issue.code, issue.message);
    }
}

pub(crate) fn has_errors(issues: &[agentic_transaction::Issue]) -> bool {
    issues.iter().any(|issue| issue.severity == Severity::Error)
}

pub(crate) fn query_param(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        if name == key {
            Some(url_decode(value))
        } else {
            None
        }
    })
}

pub(crate) fn url_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte);
                    index += 3;
                } else {
                    out.push(bytes[index]);
                    index += 1;
                }
            }
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

pub(crate) fn json_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

pub(crate) fn respond(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &str,
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "OK",
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes())?;
    stream.flush()
}

pub(crate) fn print_ui_usage() {
    println!("agentd ui <command>");
    println!();
    println!("  get");
    println!("  render [--out <file>]");
    println!("  serve [--port 8787] [--host 127.0.0.1]");
    println!("  set <file>");
    println!("  edit <file>");
    println!("  reset");
    println!("  demo");
    println!("  keymap [set <file> | default]");
}
