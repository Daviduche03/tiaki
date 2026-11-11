# Spec: Declarative UI & keyboard model

Canonical definition: [TIAKI.md §11](../../TIAKI.md#11-layer-4--declarative-ui).
Implementation: `ui/tree`, `ui/widgets`.

## The tree

Every surface is a node tree. The host renders it; the agent edits it. A tree
edit is a transaction, so revert restores the previous tree.

```json
{
  "id": "desktop",
  "kind": "workspace",
  "props": { "title": "Tiaki — ada" },
  "children": [
    {
      "id": "panel.top",
      "kind": "panel",
      "props": { "layout": "row", "title": "ada" },
      "children": [
        { "id": "clock", "kind": "clock", "props": { "format": "24h", "show_seconds": false } },
        { "id": "spacer", "kind": "spacer", "props": { "flex": 1 } },
        { "id": "status", "kind": "status", "props": { "items": ["network", "battery", "audio"] } }
      ]
    }
  ]
}
```

Nodes have a stable `id`, a `kind`, typed `props`, and `children`. Unknown node
fields are rejected so typos fail loudly. IDs are `[a-z0-9._-]`, at most 64
characters, unique across the tree.

## Widget catalog

| Kind | Children | Props |
|---|---|---|
| `workspace` | yes | `title` (text) |
| `panel` | yes | `layout` (`row`/`column`), `title` (text), `role` (`bar`/`dock`/`launcher`/`menubar`/`status`/`content`/`window`/`settings`/`agents`) |
| `clock` | no | `format` (`12h`/`24h`), `show_seconds` (bool), `show_date` (bool), `position` (`left`/`center`/`right`) |
| `spacer` | no | `flex` (number) |
| `separator` | no | `orientation` (`horizontal`/`vertical`) |
| `status` | no | `items` (array of `network`/`wifi`/`battery`/`audio`/`inference`/`time`/`search`/`control`) |
| `text` | no | `value` (text, required), `size` (`small`/`body`/`title`), `weight` (`regular`/`medium`/`semibold`/`bold`), `color` (text) |
| `button` | no | `label` (text, required), `action` (text, required), `variant` (`default`/`menubar`/`logo`/`status`/`dock`/`list`/`primary`/`ghost`), `icon` (text), `command` (text), `running` (bool), `active` (bool), `detail` (text) |
| `card` | no | `title` (text, required), `body` (text), `action` (text) |
| `section` | yes | `title` (text, required) — a labeled group inside a settings page; children are `row` and `note` nodes, rendered inside one card |
| `row` | yes | `title` (text, required), `subtitle` (text), `icon` (text), `link` (text: settings action to navigate to), `command` (text: shell command to spawn on tap) — a row with `link`/`command` renders a chevron and activates on tap; otherwise the single child is the trailing control |
| `value` | no | `source` (live reader: `time.*`, `locale.*`, `display.summary`, `power.summary`, `storage.summary`, `system.*`), `value` (static text; wins over `source`) |
| `note` | no | `value` (text, required) — a dim footnote rendered under the section's card |
| `hero` | no | `icon` (text, required), `title` (text, required), `subtitle` (text), `button` (text, required), `action` (text, required) — a banner card with an action button; `update.check` rewrites the subtitle with the check time |
| `list` | no | `source` (required: `hardware.usb` / `hardware.input` / `network.interfaces` / `network.properties` / `bluetooth.adapter`) — expands to rows at render time, including the empty states |
| `dropdown` | no | `source` (text, required: `time.clock_format`), `options` (text, required, `|`-separated labels), `action` (text, required) — the shell maps each option to its live value and runs the action on change |
| `menu` | yes | `label` (text, required) |
| `menuitem` | no | `label` (text, required), `action` (text, required), `shortcut` (text) |
| `app` | no | `label` (text, required), `command` (text, required), `icon` (text), `art` (`terminal`/`files`/`editor`/`browser`/`monitor`/`agents`/`trash`/`transactions`/`theme`/`reload`/`clock`/`exit`/`lock`/`settings`/`wallpaper`), `category` (text), `description` (text), `color` (`blue`/`purple`/`green`/`orange`/`red`/`cyan`/`slate`/`pink`), `running` (bool), `pinned` (bool) |

Limits: 256 nodes, depth 8.

## Shell tree

`desktop/desktop.json` is the shell. The `bar` panel (`role=bar`) holds the
brand button, app name, `menu` nodes, `status`, status buttons, and `clock`.
The `dock` panel (`role=dock`) holds `app` nodes and a `separator`. The
`launcher` panel (`role=launcher`) is the app catalog. The `settings` panel
(`role=settings`) holds `button` children that become the Settings sidebar.
The `agents` panel (`role=agents`) holds `button` children that become the
agent app's assistant roster; `detail` is the one-line description shown under
each name.
pages, plus one `panel` per data-driven page (`<action>.content`) holding
`section` / `row` / `value` / `dropdown` nodes that the shell renders
generically. `ui/shell` reads this
tree from `/tree.json`; the `tiaki-shell bar|dock|launcher|menu|control|settings|chrome`
processes render it. A tree edit can add an app, move a menu, change the clock,
restyle the dock, or reorder Settings pages as a transaction.

## Edit operations

```json
{ "ops": [
  { "op": "insert", "parent": "panel.top", "index": 1, "node": { ... } },
  { "op": "remove", "id": "status" },
  { "op": "move", "id": "clock", "parent": "panel.main", "index": 0 },
  { "op": "set_prop", "id": "clock", "key": "format", "value": "12h" },
  { "op": "remove_prop", "id": "clock", "key": "show_seconds" }
] }
```

`insert` and `move` reject duplicate IDs, unknown parents, container kinds that
cannot take children, indexes past the end (clamped), and moves into a node's
own subtree. `set_prop` validates the value against the widget catalog.
`remove` cannot remove the root.

The caller applies ops to a copy, validates the result, then stores it as one
`Operation::Set` on `<scope>/ui/tree`. A failed validation never reaches the
store.

## State paths and validation

| Resource | Path |
|---|---|
| tree | `user/<id>/ui/tree`, `system/ui/tree`, `system/services/<id>/ui/tree` |
| keymap | `user/<id>/input/keymap`, ... |
| theme | `user/<id>/theme/mode`, ... |

The daemon installs a `CompositeValidator`:

```
SchemaValidator + UiTreeValidator + KeymapValidator
```

`UiTreeValidator` parses and validates every `Set` of a `ui/tree` path during
preflight, so an invalid tree is `rejected` before snapshot or mutation.
UI-shaped values get a 64 KiB limit; ordinary settings keep 4096 bytes.

`revert` restores the previous tree through the normal snapshot path.

## Keyboard model

The keymap is itself a transactional resource (`<scope>/input/keymap`), so
rebinding is logged and reversible. A binding maps `keys` to a named `action`:

```json
{ "bindings": [
  { "keys": "super+space", "action": "palette.open", "label": "Command palette" },
  { "keys": "super+left", "action": "focus.left", "label": "Focus left" },
  { "keys": "super+t", "action": "theme.toggle", "label": "Toggle theme" }
] }
```

Default bindings follow an Omarchy-style keyboard-first model: Super for window
and workspace actions, Super+Space for the palette, Super+Left/Right for focus,
Super+1..5 for workspaces, Super+T for theme, Super+K for clock format,
Super+Shift+E to exit the session.

Two properties matter:

1. **One source of truth.** The keymap is data, not compiled into a config
   file. The agent and the user edit the same resource.
2. **Named actions are the agent's API.** `focus.left`, `workspace.3`,
   `theme.toggle` are stable identifiers. An agent invokes them directly; it
   does not synthesize keystrokes. Synthesized input remains available through
   computer use, but it is not the primary path.

Actions that mutate state (`theme.toggle`, `clock.toggle_format`) run through
`Engine::apply` and appear on the ledger. Pure navigation actions
(`focus.*`, `workspace.*`, `palette.open`) stay in the renderer. Other actions
are recorded as keyboard events.

## Renderers

`ui/tree` is renderer-independent. Three renderers exist:

- **`ui/shell`** — the real desktop: a GTK3 + `gtk-layer-shell` client with a
  tree-driven bar, floating dock, Spotlight-style launcher, menu dropdowns, and
  Control Center. It reads `/tree.json`.
- **`ui/native`** — a compact GTK3 client that consumes `/tree.json` and renders
  `ui/tree` with native widgets. It is the tree-renderer experiment.
- **`ui/widgets`** — a self-contained HTML page used as a preview. It is not a
  desktop.

`/tree.json` returns `{revision, theme, tree, keymap}` for native clients. The
HTML preview uses `/`, `/fragment`, `/state`, and `/action`. They consume the
same tree, so replacing the renderer does not change the contract.

Sway is the compositor for now; our own compositor and toolkit are the next
tier and will consume the same tree.

## Endpoints

| Endpoint | Purpose |
|---|---|
| `GET /` | HTML preview |
| `GET /fragment` | swappable preview screen |
| `GET /state` | `{revision, theme}` |
| `GET /tree.json` | `{revision, theme, tree, keymap}` for native clients |
| `GET /action?name=<action>` | dispatch and record a named action |

## Authority

Tree, keymap, and theme writes are ordinary `state.write` operations, so the
principal model applies unchanged: a user writes `user/<id>/**`, and
cross-user writes need a grant. The renderer only reads.

## CLI

```sh
agentd ui get
agentd ui render [--out <file>]
agentd ui serve [--port 8787]
agentd ui set <file>
agentd ui edit <file>
agentd ui reset
agentd ui demo
agentd ui keymap
agentd ui keymap set <file>
agentd ui keymap default
```

`ui demo` moves the clock into the main panel, adds a button, and switches the
clock to 12-hour time in one transaction. Revert restores the previous tree.
