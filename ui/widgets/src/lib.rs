//! HTML renderer for the declarative UI tree.
//!
//! This is the host-side renderer used before the Wayland compositor exists.
//! The same tree drives the real session-tier renderer later; only the backend
//! changes.

#![allow(dead_code)]

use agentic_ui_tree::{Keymap, Node, UiTree};

/// Render a full page. `revision` lets the page poll for agent changes.
pub fn render_page(tree: &UiTree, keymap: &Keymap, theme: &str, revision: u64) -> String {
    let screen = render_screen(tree, theme);
    PAGE.replace("__CSS__", CSS)
        .replace("__SCREEN__", &screen)
        .replace("__KEYMAP__", &keymap.bindings_json().render())
        .replace("__REVISION__", &revision.to_string())
        .replace("__THEME__", theme)
}

/// Render the swappable screen fragment.
pub fn render_screen(tree: &UiTree, theme: &str) -> String {
    format!(
        "<div id=\"screen\" data-theme=\"{}\">{}</div>",
        escape(theme),
        render_node(&tree.root)
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

fn render_node(node: &Node) -> String {
    match node.kind.as_str() {
        "workspace" => {
            let title = node.prop_str("title").unwrap_or("Tiaki");
            format!(
                "<main class=\"workspace\" data-node=\"{}\"><div class=\"workspace-title\">{}</div><div class=\"workspace-body\">{}</div></main>",
                escape(&node.id),
                escape(title),
                render_children(node)
            )
        }
        "panel" => {
            let layout = node.prop_str("layout").unwrap_or("row");
            let title = node
                .prop_str("title")
                .map(|title| format!("<div class=\"panel-title\">{}</div>", escape(title)))
                .unwrap_or_default();
            format!(
                "<section class=\"panel panel-{}\" data-node=\"{}\">{}{}</section>",
                if layout == "column" { "column" } else { "row" },
                escape(&node.id),
                title,
                format!(
                    "<div class=\"panel-body panel-{}\">{}</div>",
                    if layout == "column" { "column" } else { "row" },
                    render_children(node)
                )
            )
        }
        "clock" => {
            let format = node.prop_str("format").unwrap_or("24h");
            let seconds = node.prop_bool("show_seconds").unwrap_or(false);
            format!(
                "<div class=\"clock focusable\" tabindex=\"0\" data-node=\"{}\" data-format=\"{}\" data-seconds=\"{}\"><span class=\"clock-time\">--:--</span></div>",
                escape(&node.id),
                escape(format),
                seconds
            )
        }
        "spacer" => {
            let flex = node.prop_f64("flex").unwrap_or(1.0);
            format!(
                "<div class=\"spacer\" data-node=\"{}\" style=\"flex:{};\"></div>",
                escape(&node.id),
                flex
            )
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
                            format!(
                                "<span class=\"chip chip-{0}\" title=\"{0}\">{0}</span>",
                                escape(item)
                            )
                        })
                        .collect::<String>()
                })
                .unwrap_or_default();
            format!(
                "<div class=\"status\" data-node=\"{}\">{}</div>",
                escape(&node.id),
                items
            )
        }
        "text" => {
            let value = node.prop_str("value").unwrap_or("");
            let size = node.prop_str("size").unwrap_or("body");
            format!(
                "<div class=\"text text-{}\" data-node=\"{}\">{}</div>",
                escape(size),
                escape(&node.id),
                escape(value)
            )
        }
        "button" => {
            let label = node.prop_str("label").unwrap_or("Button");
            let action = node.prop_str("action").unwrap_or("noop");
            format!(
                "<button class=\"button focusable\" tabindex=\"0\" data-node=\"{}\" data-action=\"{}\">{}</button>",
                escape(&node.id),
                escape(action),
                escape(label)
            )
        }
        "card" => {
            let title = node.prop_str("title").unwrap_or("Card");
            let body = node.prop_str("body").unwrap_or("");
            let action = node.prop_str("action").unwrap_or("noop");
            format!(
                "<button class=\"card focusable\" tabindex=\"0\" data-node=\"{}\" data-action=\"{}\"><span class=\"card-title\">{}</span><span class=\"card-body\">{}</span></button>",
                escape(&node.id),
                escape(action),
                escape(title),
                escape(body)
            )
        }
        other => format!(
            "<div class=\"unknown\" data-node=\"{}\">unknown widget: {}</div>",
            escape(&node.id),
            escape(other)
        ),
    }
}

fn render_children(node: &Node) -> String {
    node.children
        .iter()
        .map(render_node)
        .collect::<Vec<_>>()
        .join("")
}

fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            character => out.push(character),
        }
    }
    out
}

const CSS: &str = r#"
:root { color-scheme: dark; }
* { box-sizing: border-box; }
body {
  margin: 0;
  min-height: 100vh;
  font-family: ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, sans-serif;
  background:
    radial-gradient(1100px 700px at 15% -10%, #1b2740 0%, transparent 55%),
    radial-gradient(900px 620px at 100% 0%, #2a1b33 0%, transparent 50%),
    radial-gradient(1000px 700px at 50% 115%, #0f1a29 0%, transparent 62%),
    #08090b;
  color: #e6e8eb;
  -webkit-font-smoothing: antialiased;
}
body[data-theme="light"] {
  background:
    radial-gradient(1100px 700px at 15% -10%, #e8edf6 0%, transparent 55%),
    #f4f5f7;
  color: #171a1f;
  color-scheme: light;
}
#screen { min-height: 100vh; display: flex; flex-direction: column; }
.workspace { flex: 1; display: flex; flex-direction: column; padding: 24px; gap: 16px; }
.workspace-title {
  font-size: 12px; letter-spacing: .09em; text-transform: uppercase;
  color: #7f8794; font-weight: 600;
}
.workspace-body { flex: 1; display: flex; flex-direction: column; gap: 16px; }
.panel {
  border: 1px solid rgba(60, 72, 92, .45); background: rgba(18, 22, 28, .72);
  backdrop-filter: blur(16px);
  border-radius: 16px; padding: 12px 14px;
  box-shadow: 0 10px 30px rgba(0, 0, 0, .25);
}
body[data-theme="light"] .panel {
  background: rgba(255, 255, 255, .78); border-color: #e2e5ea;
  box-shadow: 0 10px 30px rgba(20, 30, 50, .07);
}
.panel-title { font-size: 11px; color: #7f8794; margin-bottom: 8px; text-transform: uppercase; letter-spacing: .07em; }
.panel-body { display: flex; gap: 12px; align-items: center; }
.panel-body.panel-column { flex-direction: column; align-items: stretch; }
.panel-body.panel-row { flex-direction: row; }
.clock { font-variant-numeric: tabular-nums; font-size: 16px; font-weight: 650; letter-spacing: .01em; }
.spacer { min-width: 0; }
.status { display: flex; gap: 6px; flex-wrap: wrap; }
.chip {
  font-size: 11px; padding: 3px 9px; border-radius: 999px;
  background: #1a1f27; border: 1px solid #2a303b; color: #aab2bf;
}
body[data-theme="light"] .chip { background: #f1f3f6; border-color: #e2e5ea; color: #4a5260; }
.text { color: #aab2bf; font-size: 14px; line-height: 1.5; }
.text-title {
  color: #eef1f5; font-size: 22px; font-weight: 650; line-height: 1.28;
  letter-spacing: -.02em; max-width: 70ch;
}
.text-small { color: #8f98a7; font-size: 12.5px; }
body[data-theme="light"] .text { color: #4a5260; }
body[data-theme="light"] .text-title { color: #141821; }
body[data-theme="light"] .text-small { color: #6a7280; }
.button {
  font: inherit; font-size: 13px; font-weight: 550; color: #e6e8eb;
  background: #1c2230; border: 1px solid #2c3444; border-radius: 9px;
  padding: 8px 14px; cursor: pointer; transition: background .12s ease, border-color .12s ease;
}
.button:hover { background: #232b3b; border-color: #3a455a; }
body[data-theme="light"] .button {
  background: #eef1f6; border-color: #dfe3ea; color: #1a1e26;
}
body[data-theme="light"] .button:hover { background: #e4e9f1; border-color: #cfd6e2; }
.card {
  flex: 1 1 0; align-self: stretch; min-width: 0;
  display: flex; flex-direction: column; gap: 7px;
  padding: 16px; text-align: left; font: inherit; color: inherit;
  background: linear-gradient(180deg, rgba(36,46,64,.92) 0%, rgba(20,25,34,.92) 100%);
  border: 1px solid rgba(70,86,112,.5); border-radius: 14px;
  cursor: pointer; box-shadow: 0 6px 18px rgba(0,0,0,.22);
  transition: transform .12s ease, border-color .12s ease, background .12s ease;
}
.card:hover { transform: translateY(-1px); border-color: rgba(110,168,254,.55); }
.card-title { font-size: 15px; font-weight: 650; letter-spacing: -.01em; }
.card-body { font-size: 12.5px; line-height: 1.45; color: #9aa4b4; }
body[data-theme="light"] .card {
  background: linear-gradient(180deg, #ffffff 0%, #f3f5f9 100%);
  border-color: #dfe3ea; color: #171a1f;
  box-shadow: 0 6px 18px rgba(20,30,50,.06);
}
body[data-theme="light"] .card-body { color: #5a6270; }
.focusable:focus-visible, .button:focus-visible {
  outline: 2px solid #6ea8fe; outline-offset: 2px;
}
.unknown { color: #ff6b6b; font-size: 12px; }
#palette {
  position: fixed; inset: 0; display: none; align-items: flex-start; justify-content: center;
  padding-top: 12vh; background: rgba(0,0,0,.5); backdrop-filter: blur(2px); z-index: 20;
}
#palette.open { display: flex; }
.palette-box {
  width: min(580px, 92vw); background: #12161c; border: 1px solid #2a303b;
  border-radius: 16px; overflow: hidden; box-shadow: 0 30px 90px rgba(0,0,0,.6);
}
body[data-theme="light"] .palette-box { background: #fff; border-color: #e2e5ea; }
.palette-head { display: flex; align-items: center; gap: 10px; padding: 12px 14px; border-bottom: 1px solid #232833; }
.palette-head input {
  flex: 1; background: transparent; border: none; outline: none;
  color: inherit; font: inherit; font-size: 15px;
}
.palette-head input::placeholder { color: #6f7784; }
.palette-list { max-height: 46vh; overflow: auto; padding: 6px; }
.palette-item {
  width: 100%; display: flex; align-items: center; justify-content: space-between; gap: 12px;
  background: transparent; border: none; color: inherit; font: inherit; text-align: left;
  padding: 9px 10px; border-radius: 9px; cursor: pointer;
}
.palette-item.selected, .palette-item:hover { background: #1b2230; }
body[data-theme="light"] .palette-item.selected, body[data-theme="light"] .palette-item:hover { background: #f0f3f8; }
.palette-item .label { font-size: 14px; }
kbd {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 11px;
  color: #9aa3b2; background: #1a1f27; border: 1px solid #2a303b;
  border-radius: 6px; padding: 2px 7px; white-space: nowrap;
}
body[data-theme="light"] kbd { background: #f1f3f6; border-color: #e2e5ea; color: #4a5260; }
#toast {
  position: fixed; left: 50%; bottom: 26px; transform: translateX(-50%) translateY(8px);
  opacity: 0; pointer-events: none; background: #1c2230; border: 1px solid #2c3444;
  color: #e6e8eb; font-size: 13px; padding: 8px 14px; border-radius: 10px;
  transition: opacity .16s ease, transform .16s ease; z-index: 30;
}
#toast.show { opacity: 1; transform: translateX(-50%) translateY(0); }
"#;

const PAGE: &str = r#"<!doctype html>
<html lang="en" data-theme="__THEME__">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Tiaki</title>
<style>__CSS__</style>
</head>
<body data-theme="__THEME__">
__SCREEN__
<div id="palette" aria-hidden="true">
  <div class="palette-box" role="dialog" aria-modal="true">
    <div class="palette-head">
      <input id="palette-input" type="text" placeholder="Type a command…" autocomplete="off" spellcheck="false">
      <kbd id="palette-hint">esc</kbd>
    </div>
    <div class="palette-list" id="palette-list"></div>
  </div>
</div>
<div id="toast"></div>
<script>
const KEYMAP = __KEYMAP__;
let currentRevision = __REVISION__;

const palette = document.getElementById('palette');
const paletteInput = document.getElementById('palette-input');
const paletteList = document.getElementById('palette-list');
let visible = KEYMAP;
let selected = 0;

function normalize(event) {
  const parts = [];
  if (event.metaKey) parts.push('super');
  if (event.ctrlKey) parts.push('ctrl');
  if (event.altKey) parts.push('alt');
  if (event.shiftKey) parts.push('shift');
  let key = event.key.toLowerCase();
  if (key === ' ') key = 'space';
  if (key === '/') key = 'slash';
  if (key === 'enter') key = 'return';
  if (key === 'escape') key = 'escape';
  parts.push(key);
  return parts.join('+');
}

function prettyKeys(keys) {
  return keys.split('+').map((part) => {
    if (part === 'super') return '⌘';
    if (part === 'shift') return '⇧';
    if (part === 'alt') return '⌥';
    if (part === 'ctrl') return '⌃';
    return part.length === 1 ? part.toUpperCase() : part[0].toUpperCase() + part.slice(1);
  }).join(' ');
}

function renderPalette() {
  paletteList.innerHTML = '';
  visible.forEach((binding, index) => {
    const button = document.createElement('button');
    button.className = 'palette-item' + (index === selected ? ' selected' : '');
    button.innerHTML = '<span class="label"></span><kbd></kbd>';
    button.querySelector('.label').textContent = binding.label;
    button.querySelector('kbd').textContent = prettyKeys(binding.keys);
    button.addEventListener('click', () => run(binding.action));
    paletteList.appendChild(button);
  });
  if (visible.length === 0) {
    paletteList.innerHTML = '<div class="palette-item"><span class="label">No matching command</span></div>';
  }
}

function openPalette(query = '') {
  palette.classList.add('open');
  palette.setAttribute('aria-hidden', 'false');
  selected = 0;
  visible = KEYMAP;
  paletteInput.value = query;
  filterPalette(query);
  paletteInput.focus();
}

function closePalette() {
  palette.classList.remove('open');
  palette.setAttribute('aria-hidden', 'true');
  paletteInput.value = '';
}

function filterPalette(query) {
  const needle = query.trim().toLowerCase();
  visible = needle.length === 0
    ? KEYMAP
    : KEYMAP.filter((binding) =>
        binding.label.toLowerCase().includes(needle) ||
        binding.action.toLowerCase().includes(needle) ||
        binding.keys.toLowerCase().includes(needle));
  selected = 0;
  renderPalette();
}

function moveSelection(delta) {
  if (visible.length === 0) return;
  selected = (selected + delta + visible.length) % visible.length;
  renderPalette();
  const node = paletteList.querySelectorAll('.palette-item')[selected];
  if (node) node.scrollIntoView({ block: 'nearest' });
}

function run(action) {
  closePalette();
  dispatch(action);
}

function toast(text) {
  const element = document.getElementById('toast');
  element.textContent = text;
  element.classList.add('show');
  clearTimeout(element._timer);
  element._timer = setTimeout(() => element.classList.remove('show'), 1400);
}

function focusMove(direction) {
  const focusable = Array.from(document.querySelectorAll('.focusable'));
  if (focusable.length === 0) return;
  const current = focusable.indexOf(document.activeElement);
  let next;
  if (current === -1) {
    next = 0;
  } else if (direction === 'left' || direction === 'up') {
    next = (current - 1 + focusable.length) % focusable.length;
  } else {
    next = (current + 1) % focusable.length;
  }
  focusable[next].focus();
}

async function dispatch(action) {
  if (action === 'palette.open') return openPalette();
  if (action === 'help.show') return openPalette();
  if (action === 'ui.reload') return location.reload();
  if (action.startsWith('focus.')) return focusMove(action.split('.')[1]);
  if (action.startsWith('workspace.')) {
    toast('Workspace ' + action.split('.')[1]);
    return;
  }
  try {
    await fetch('/action?name=' + encodeURIComponent(action));
    toast(action.replace('.', ' '));
    await refresh(true);
  } catch (error) {
    toast('Action failed: ' + action);
  }
}

function updateClock(element) {
  const format = element.dataset.format || '24h';
  const seconds = element.dataset.seconds === 'true';
  const now = new Date();
  let hours = now.getHours();
  let suffix = '';
  if (format === '12h') {
    suffix = hours >= 12 ? ' PM' : ' AM';
    hours = hours % 12 || 12;
  }
  const pad = (value) => String(value).padStart(2, '0');
  const time = pad(hours) + ':' + pad(now.getMinutes()) + (seconds ? ':' + pad(now.getSeconds()) : '') + suffix;
  const target = element.querySelector('.clock-time');
  if (target) target.textContent = time;
}

async function refresh(force) {
  try {
    const stateResponse = await fetch('/state');
    const state = await stateResponse.json();
    if (!force && state.revision === currentRevision) return;
    currentRevision = state.revision;
    document.body.dataset.theme = state.theme;
    const fragment = await fetch('/fragment');
    const html = await fragment.text();
    const screen = document.getElementById('screen');
    if (screen) screen.outerHTML = html;
  } catch (error) {
    /* the host may be restarting */
  }
}

paletteInput.addEventListener('input', () => filterPalette(paletteInput.value));

document.addEventListener('keydown', (event) => {
  if (palette.classList.contains('open')) {
    if (event.key === 'Escape') { closePalette(); event.preventDefault(); return; }
    if (event.key === 'ArrowDown') { moveSelection(1); event.preventDefault(); return; }
    if (event.key === 'ArrowUp') { moveSelection(-1); event.preventDefault(); return; }
    if (event.key === 'Enter') {
      if (visible[selected]) run(visible[selected].action);
      event.preventDefault();
      return;
    }
    return;
  }

  const target = event.target;
  const typing = target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable);
  if (!typing && event.key === '/') { openPalette(); event.preventDefault(); return; }
  if (!typing && event.key === '?') { openPalette(); event.preventDefault(); return; }

  const combo = normalize(event);
  const binding = KEYMAP.find((entry) => entry.keys === combo);
  if (binding) {
    event.preventDefault();
    dispatch(binding.action);
  }
});

document.addEventListener('click', (event) => {
  const target = event.target.closest('[data-action]');
  if (target && (target.classList.contains('button') || target.classList.contains('card'))) {
    dispatch(target.dataset.action);
  }
});

setInterval(() => {
  document.querySelectorAll('.clock').forEach(updateClock);
}, 1000);
document.querySelectorAll('.clock').forEach(updateClock);
setInterval(() => refresh(false), 1500);

if (new URLSearchParams(location.search).has('palette')) {
  openPalette();
}
</script>
</body>
</html>
"#;

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
        assert!(page.contains("data-theme=\"dark\""));
    }

    #[test]
    fn escapes_html() {
        let mut tree = UiTree::default_for("user:ada");
        tree.root.find_mut("hint").unwrap().props.insert(
            "value".into(),
            agentic_json::Json::String("<script>x</script>".into()),
        );
        let html = render_screen(&tree, "dark");
        assert!(!html.contains("<script>x</script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn revision_changes_with_content() {
        let a = revision("{\"a\":1}", "{}", "dark");
        let b = revision("{\"a\":2}", "{}", "dark");
        assert_ne!(a, b);
    }
}
