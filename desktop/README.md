# Tiaki desktop

A real Sway/Wayland session with our own tree-driven shell: a macOS-like menu
bar, a floating dock, a home-screen launcher, dropdown menus, and Control
Center. The launcher has a dot-matrix clock, search pill, app grid, system /
calendar / inference / ledger widgets, page dots, and its own dock.

- **Compositor:** Sway — tiling, gaps, pixel borders, workspaces.
- **Shell:** `ui/shell` (`tiaki-shell`) — GTK3 + `gtk-layer-shell`, driven by
  `/tree.json`.
- **Bar:** brand mark, app name, File/Edit/View/Window/Agent menus, inference
  status, Wi-Fi, audio, power, search, Control Center, clock.
- **Control Center:** charcoal panel, two-column icon cards with white active
  badges, thick white brightness/volume sliders, and a power card.
- **Dock:** floating glass panel with modern WhiteSur app marks, running dots,
  separator, Trash.
 - **Agents:** the agent app — assistant roster (Pair Programmer, Researcher,
  Writer, Planner, Helper) from the `agents` panel, live default-runtime
  readout, and a button that opens the runtime in a terminal in the session
  class. Threads, the approval inbox, and scheduled tasks are next.
- **Settings:** Windows-11-dark-style window with profile header, setting search,
  icon sidebar, breadcrumb, and sectioned cards. Pages are tree-driven; every
  control is real (wallpaper picker, theme, PipeWire volume, backlight,
  Sway outputs, power, about).
- **Launcher:** home screen with dot-matrix clock, search pill, app grid,
  system/calendar/inference/ledger widgets, page dots, and dock.
- **Design:** Tokyo Night palette, Inter, JetBrains Mono, real 1440×900
  wallpapers, dark GTK, 12–22px radii.
- **Wallpaper:** four bundled photos; cycle from the launcher’s **Wallpaper**
  app or **View → Change Wallpaper**. The choice is remembered in
  `~/.config/tiaki/wallpaper` and the launcher uses it as its backdrop.
- **Icons:** modern WhiteSur app marks in `desktop/icons`; bar/status and
  Control Center glyphs are drawn by `ui/shell`.
- **Apps:** foot, mousepad, btop, Thunar, Epiphany, all launched through
  `sched-launch` so each runs in its scheduling class.
- **Scheduling:** `sched-launch --class foreground|background|session` puts a
  process in a cgroup and weights it. Agents boost automatically; inference
  bursts demote them; background work is throttled but never starved. When the
  kernel cannot enforce, the launcher says so and runs the app unsupervised.
- **Window chrome:** our own layer-surface titlebars with traffic lights
  (close / minimize to scratchpad / fullscreen), driven by Sway IPC.
- **Remote:** wayvnc on port 5901.

Screenshots are captured from the real compositor with `grim`:

- [`screenshots/macos-dark.png`](screenshots/macos-dark.png)
- [`screenshots/macos-launcher.png`](screenshots/macos-launcher.png)
- [`screenshots/macos-menu.png`](screenshots/macos-menu.png)
- [`screenshots/macos-control.png`](screenshots/macos-control.png)
- [`screenshots/macos-wallpaper.png`](screenshots/macos-wallpaper.png)
- [`screenshots/macos-search.png`](screenshots/macos-search.png)
- [`screenshots/macos-settings.png`](screenshots/macos-settings.png)

```sh
docker build -f desktop/wayland/Dockerfile -t tiaki-wayland:dev .
docker run -d --name tiaki-wayland -p 5901:5900 tiaki-wayland:dev
open vnc://localhost:5901        # macOS Screen Sharing
```

Keyboard: **Super+Space** or **Super+D** launcher, **Super+Return** terminal,
**Super+Q** close, **Super+1..5** workspaces, **Super+Left/Right** focus, **Super+F** fullscreen,
**Super+Shift+M** show minimized, **Super+Shift+L** lock,
**Super+Shift+E** exit.

Window arrangements: tiling is automatic. **Super+B/V** split horizontal/vertical,
**Super+S/W** stacking/tabbed, **Super+E** toggle split, **Super+Shift+Space** float,
**Super+Shift+Arrows** move, **Super+R** resize mode, **Super+drag** moves and
right-drags resizes floating windows, and tiled windows can be dragged to swap.
The same layouts live in the **Window** menu, so the agent can arrange windows
as a tree action. The desktop dock hides while the launcher is open.

## The tree is the desktop

`desktop/desktop.json` is the shell:

```json
{ "id": "bar", "kind": "panel", "props": { "role": "bar" }, "children": [ ... ] }
{ "id": "dock", "kind": "panel", "props": { "role": "dock" }, "children": [ ... ] }
{ "id": "launcher", "kind": "panel", "props": { "role": "launcher" }, "children": [ ... ] }
```

The bar, dock, launcher, menus, status items, and app catalog are nodes.
`agentd` validates and stores the tree; `tiaki-shell` reads `/tree.json`.
Adding an app, changing a menu, moving the clock, or restyling the dock is a
tree edit and a transaction, not a rebuild. `app.art` selects the app mark
(WhiteSur SVG, with a drawn fallback). The bar and dock read the tree at
start; menus and the launcher read it on every open (live re-render is next).

## Ours vs. borrowed

| Layer | This pass | Direction |
|---|---|---|
| Design tokens, bar, dock, launcher, menus, Control Center, window chrome | **ours** | extend the language |
| UI tree + control plane | **ours** | unchanged |
| Init + services (systemd units) | **ours** | `desktop/systemd/`: firstboot, ui-serve, session; PID 1 is systemd |
| Generations, installer, boot config | **ours** | `desktop/install/` (A/B slots, Limine); QEMU harness in `desktop/qemu/` |
| Compositor (Sway) | borrowed | SwayFX/Wayfire for blur/shadows/rounded corners, then our own |
| Kernel, drivers, base userland | borrowed (Debian) | unchanged, except our kernel config if one is ever needed |
| Applications (foot, mousepad, btop, Thunar, Epiphany) | borrowed | our own apps later |
| Toolkit (GTK3) | borrowed | deeply themed now, our own later |

## System: init, generations, install, test

PID 1 is systemd (`ENTRYPOINT ["/sbin/init"]`). `tiaki-firstboot`
seeds state and delegates the cgroup subtree, `tiaki-ui-serve` answers
the tree API with restart-always, and `tiaki-session` runs Sway as the
desktop user. Verified in a privileged container: all three units active,
`sandbox probe` on the real backend, `sandbox bench` at 8.72:1 vs 8.86:1
expected, desktop rendering over VNC.

Two identical root generations (A/B) plus shared `/home`; each carries its
own kernel and ledger. `desktop/install/install.sh` partitions, debootsraps
Debian, clones A→B, lays the staged payload (`make payload`), and writes
Limine with real partition GUIDs. `desktop/qemu/harness.sh` boots a Debian
ARM64 guest under HVF and runs the kexec + ledger-continuity cycle; exit 0
only if every guest check prints PASS. See `docs/specs/boot.md`.

Sway cannot draw window-body shadows, blur, rounded window corners, or
workspace animations. Our titlebars provide real window headers and traffic
lights; the window bodies remain flat until the compositor layer changes.
That is the remaining visual gap.

## Reference imagery (not shipped)

`desktop/reference/` holds freely licensed macOS screenshots used only as
layout references. See [reference/README.md](reference/README.md).

## Native tree renderer

`ui/native` is the earlier GTK3 renderer of `ui/tree`. It remains a compact
proof that the same tree can drive native widgets; the visual desktop is
`ui/shell`.

## Web preview (not the desktop)

`desktop/Dockerfile` is the earlier HTML renderer. It is useful for quick
inspection in a browser, but it is a web shell, not a desktop.

```sh
docker build -f desktop/Dockerfile -t tiaki-desktop:dev .
docker run -d --name tiaki-desktop -p 8788:8787 tiaki-desktop:dev
open http://localhost:8788
```
