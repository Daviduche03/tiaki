# tiaki-shell

The desktop shell: a **tree-driven** top bar, floating dock, home-screen
launcher, menu bar dropdowns, and Control Center.

It is a GTK3 + `gtk-layer-shell` client in its own Cargo workspace so the core
workspace stays dependency-free. It reads `/tree.json` from `agentd`, which
means the agent can restyle, rearrange, add, or remove shell elements as a
transaction. The tree baked into the binary is only an offline first-paint
fallback.

```sh
tiaki-shell bar       # top menu bar (30px, exclusive zone)
tiaki-shell dock      # floating dock at the bottom
tiaki-shell launcher  # Spotlight-style launcher (Super+Space)
tiaki-shell menu <id> <x>
tiaki-shell control <x>
```

## Design

- **Tokens:** Tokyo Night palette, Inter, 8px spacing, 12–22px radii.
- **Bar:** brand mark, active app name, `File / Edit / View / Window / Agent`
  menus, inference status, Wi-Fi, audio, power, search, Control Center, clock.
- **Dock:** pinned apps from the `dock` panel, running dots, separator, Trash.
- **Windows:** apps open as floating windows and cascade, like macOS. App
  windows are free-resized (drag any edge; `default_floating_border pixel 4`
  provides the grab area), and double-clicking a titlebar zooms to fill the
  workspace and restores on the second double-click. Green is full screen,
  yellow minimises to the scratchpad. New windows are placed once by the
  overlay (`tiaki-wm-placed` remembers ids), so restarts do not move them.
- **Grouping:** ⌘-Tab (Super+Tab) opens the window switcher; Super+` lists the
  focused app's windows; the dock focuses a running app's single window or
  lists its windows when there are several; Window → All Windows / Zoom.
- **Window chrome:** a 30px layer-surface titlebar per window with traffic
  lights: close, minimize to scratchpad, and fullscreen. Because layer
  surfaces always paint above normal windows, only the focused window (and
  unobstructed windows) carry a titlebar — a bar never paints over the window
  above it. One chrome process runs at a time (a relaunch replaces the old
  instance).
- **Launcher:** a home screen with a dot-matrix clock, search pill, fixed app grid, system/calendar/inference/ledger widgets, page dots, and its own dock. It uses the current wallpaper as its backdrop and can cycle wallpapers (`wallpaper.next`). The desktop dock hides while it is open.
- **Settings:** a Windows-11-dark-style window (profile header, setting search, icon sidebar, breadcrumb, sectioned cards). Sidebar pages come from the `settings` panel in the tree. Pages: Wallpaper (thumbnail picker + slideshow), Appearance (color mode + clock format), Sound (PipeWire volume), Display (brightness + real Sway output data), Power (supply readout), About (system info).
- **Launching apps:** dock and launcher entries start through
  `/usr/local/bin/sched-launch --class <class> --`, so a real app lands in a
  cgroup and gets a share of the machine. If the launcher is absent the shell
  starts the command plainly.
  Code layout mirrors the styles: `ui/shell/src/settings/` holds `mod.rs` (window + navigation), `widgets.rs` (card/row/dropdown/slider primitives), `data.rs` (live machine readers), and one file per page (`home.rs`, `system.rs`, `display.rs`, …). New Win11 pages slot in as one Rust file plus rules in `css/settings.css`.
  The Time, System, About, Gaming, Bluetooth, Network, and Update pages are data instead of code: their content lives in the tree (`settings.page.<name>.content` panels of `section` / `row` / `value` / `dropdown` / `note` / `hero` / `list` nodes, see `docs/specs/ui-tree.md`), rendered generically by `settings/render.rs` with live sources resolved by a registry. No other page is data-driven yet; `render_content` falls back to hand-written UI when the tree ships no content panel.
- **Menus/Control Center:** separate overlay surfaces so they are not clipped
  by the 30px bar layer surface. The Control Center is a charcoal panel with
  white-badge connectivity cards, thick white sliders, and a power card.
- **Styles:** no CSS lives in Rust. `ui/shell/css/` holds one sheet per
  module (`base`, `bar`, `menu`, `dock`, `launcher`, `chrome`, `settings`,
  `control`); each module declares its own `STYLE_SHEETS` list, embedded at
  compile time via `include_str!` so the offline build stays offline, and
  `theme::install` concatenates them in cascade order. Restyle by
  transacting on CSS files, not source.

Everything is rendered from `desktop/desktop.json` (or the live tree):

| Tree node | Renders as |
|---|---|
| `panel[role=bar]` | top bar |
| `menu` / `menuitem` | menu bar dropdowns |
| `panel[role=dock]` + `app` | floating dock |
| `panel[role=launcher]` + `app` | home-screen app grid and catalog |
| `status` | inference/Wi-Fi/audio/battery indicators |
| `clock` | menu bar clock |
| `button[variant=status]` | search and Control Center buttons |
| `app.art` | app mark (`terminal`, `files`, ...; WhiteSur SVG with drawn fallback) |

The bar and dock read the tree at start; menus and the launcher read it on
every open. Live re-render on a tree revision is the next step. App marks live
in `desktop/icons` (WhiteSur, GPL-3.0); bar/status and Control Center glyphs
are drawn by `ui/shell`.

## Build

The core workspace does not include this crate. Build it separately:

```sh
cargo build --release --manifest-path ui/shell/Cargo.toml
cargo test --manifest-path ui/shell/Cargo.toml
```

Debian build dependencies: `libgtk-3-dev libgtk-layer-shell-dev`.
Runtime: `libgtk-3-0 libgtk-layer-shell0 librsvg2-common`.

## Boundaries

Sway is still the compositor, so window shadows, blur, rounded window corners,
and workspace animations are not available. Those belong to the compositor
layer (SwayFX/Wayfire, then our own). The shell, design tokens, tree, and
control plane are ours.
