# Tiaki Agent Master Engineering & Execution Guide (`agent.md`)

This document is the primary instructions, architecture, runbook, and changelog guide for AI agents working in the **Tiaki OS** repository. It must be kept aligned alongside `CLAUDE.md`.

---

## 1. Project Mission & Core Invariants

**Tiaki** is a general-purpose Linux-based operating system designed from the ground up for autonomous AI agents.

### Core Invariants:
1. **Full Access, Guaranteed Undo:** The agent may modify system configuration, UI, and services, but every mutation must be a reversible transaction on an immutable, append-only cryptographic ledger.
2. **Every Change is Attributable:** Every action logs who, what, when, why, the exact diff, and a plain-language summary for humans.
3. **No Raw Ambient Root:** Every agent action runs inside an isolated sandbox profile (`inspect`, `user-write`, `system-write`, `privileged`) using Linux namespaces, Landlock, and Seccomp.
4. **Physical Trusted Input:** High-risk elevation dialogs require physical user input (trackpad/keyboard). Synthetic automated clicks cannot approve elevated actions.
5. **Full CLI Parity:** Every GUI setting or toggle has an exact, functioning command-line equivalent in `agentd`, `swaymsg`, or standard system tools.
6. **Zero-Lag VNC / Web Rendering:** Maintain $0\%$ idle CPU for compositing and streaming. Avoid full-window layout invalidations during hover/animations.
7. **Branch Hygiene:** Always work on feature branches (e.g. `redesign/ui-issues`). Never push directly to `master`.

---

## 2. Architecture & Deployment Topology

### Remote Rig (`dami`)
- **Server:** `31.220.93.61` (alias `dami`, user `root`, SSH key `~/.ssh/id_ed25519`)
- **Host OS:** Ubuntu 24.04 LTS (8 GB RAM, 96 GB disk, Docker v29.7.2)
- **Deployment Directory:** `/root/tiaki-deploy/tiaki`
- **Docker Container:** `tiaki-wayland` running `tiaki-wayland:dev`
  - Compositor: Headless Sway (`HEADLESS-1`, resolution `1440x900`)
  - VNC Server: `wayvnc` on port `5900`
  - Wayland Display: `wayland-1`
  - Runtime Socket: `/run/tiaki/sway-ipc.1000.123.sock`
  - Systemd Session: `tiaki-session.service`
- **WebSocket Bridge:**
  - `websockify` bridges port `6080` (WebSocket) $\rightarrow$ `127.0.0.1:5900` (RFB)
  - noVNC HTML web client at `/usr/share/novnc/`

### Local Rig (Mac)
- **Repository Path:** `/Users/eric/tiaki`
- **Rust Toolchain:** `stable-aarch64-apple-darwin` (Rust 1.99.1)
- **Active SSH Tunnels:**
  - `6080:127.0.0.1:6080` (noVNC web client)
  - `5900:127.0.0.1:5900` (raw RFB / VNC)
- **Live Desktop Access:**
  - Web VNC URL: `http://localhost:6080/vnc.html?autoconnect=true&resize=scale`

---

## 3. Operational Runbook & Common Commands

### Local Code Verification
```bash
cd /Users/eric/tiaki
cargo check --workspace
cargo test --workspace
```
*(Note: `ui/shell` requires Linux GTK3 dev libraries; build/test `ui/shell` inside the container on `dami`).*

### SSH Tunnel Check & Startup
```bash
# Check if tunnels are active
lsof -i :6080 -i :5900

# Start tunnels with keepalive if down
ssh -f -N -o ServerAliveInterval=30 -o ServerAliveCountMax=10 -L 6080:127.0.0.1:6080 -L 5900:127.0.0.1:5900 dami
```

### Remote Docker Build & Deploy (on dami)
```bash
# 1. Commit and push locally
git push origin redesign/ui-issues

# 2. Pull and rebuild on VPS
ssh dami "cd /root/tiaki-deploy/tiaki && git checkout redesign/ui-issues && git pull && docker build -f desktop/wayland/Dockerfile -t tiaki-wayland:dev ."

# 3. Restart container
ssh dami "docker rm -f tiaki-wayland && docker run -d --name tiaki-wayland --privileged -p 127.0.0.1:5900:5900 tiaki-wayland:dev"

# 4. Verify websockify is alive
ssh dami "pgrep websockify || nohup websockify --web /usr/share/novnc/ 127.0.0.1:6080 127.0.0.1:5900 >/tmp/websockify.log 2>&1 &"
```

### Screenshot Capture & Inspection
```bash
ssh dami "docker exec -u desktop tiaki-wayland sh -c 'XDG_RUNTIME_DIR=/run/tiaki WAYLAND_DISPLAY=wayland-1 grim /tmp/tiaki-live.png' && docker cp tiaki-wayland:/tmp/tiaki-live.png /root/tiaki-live.png"
scp dami:/root/tiaki-live.png /tmp/tiaki-live.png
```

### Local Linux (Zorin OS / Ubuntu / Debian) Execution Runbook
When running Tiaki directly on a local Linux computer (e.g. Zorin OS, Ubuntu, Debian), you can run without Docker or VNC streaming:

#### Method A: Nested in a Window (Fastest / Recommended for Dev)
Runs Tiaki inside a standalone Wayland window on top of your current GNOME/Wayland/X11 session:
```bash
# 1. Quick launch using the helper script
tiaki-nested
# or run manually:
sway -c /etc/tiaki/sway.conf
```

#### Method B: Native Bare-Metal Desktop Session (Full OS Experience)
Register Tiaki in GDM/LightDM display manager:
1. Select 'Tiaki' at the display manager login gear menu and log in.
See [docs/runbooks/local-linux-setup.md](file:///home/eric/tiaki/docs/runbooks/local-linux-setup.md) for full details.

---

## 4. Design Tokens & Anti-Slop UI Principles

- **Base Ink:** `#151413` (background), `#1b1917` (cards/modals), `#131314` (sidebar rail)
- **Paper Text:** `#e6e0d4` (high contrast), `rgba(230, 224, 212, 0.62)` (secondary), `rgba(230, 224, 212, 0.40)` (muted metadata)
- **Lamp Accent:** `#d08a2c` (warm gold for active rings, selection chips, focus states)
- **Hairlines:** `1px solid rgba(230, 224, 212, 0.10)` to `0.14`
- **Typography:**
  - Display / Body: **IBM Plex Sans**
  - Code / Metadata / Clocks / Shortcuts: **IBM Plex Mono**
  - *Inter font is strictly prohibited.*
- **Window Management:** Floating by default; tiling opt-in via keybindings.
- **Micro-Interactions:**
  - Animation durations strictly between $150\text{ms}$ and $300\text{ms}$.
  - Spring physics / natural deceleration cubic bezier `(0.16, 1, 0.3, 1)`.
  - Staggered cascaded micro-delays (20-30ms) for grouped elements.
  - No purple/blue gradient slop or heavy blur box shadows.

---

## 5. Subsystem Architecture

### Layer 0: Recovery Core (Immutable)
- Minimal, signed root. Generation table, boot watchdog, rollback mechanism.
- Completely read-only at runtime; unreachable by agent sandboxes.

### Layer 1: Transaction & State Engine (`agentd`)
- Atomic mutations: `check` (preflight dry-run) $\rightarrow$ `apply` (CoW snapshot) $\rightarrow$ `health-check` $\rightarrow$ `commit`.
- Automatic rollback on health-check failure.
- SHA-256 hash-chained append-only ledger (`agentd log`, `agentd verify`).
- Human plain-language summaries generated from diffs.

### Layer 2: Capability & Principal Model
- Principals: `system`, `user:<id>`, `service:<id>`, `agent:<runtime>:<session>@<principal>`.
- Capabilities: Scoped grants (`action`, `resource pattern`).
- Modes: **Guarded** (requires trusted approval for elevation) vs. **Full** (auto-approves within scoped grants).
- Sandbox profiles: `inspect`, `user-write`, `system-write`, `privileged`.

### Layer 3: Plugin Host
- Manifests (`plugin.toml`) defining capabilities, config schemas, and UI trees.
- Live content hot-swapping without desktop restarts.

### Layer 4: Declarative UI Tree (`ui/tree`, `desktop/desktop.json`)
- UI surfaces described as semantic JSON trees with stable node IDs.
- Agents inspect and modify structured UI trees rather than guessing screen pixels.

### Layer 5: Inference Broker
- Schedules cognition like CPU/RAM.
- Shared resident weights in RAM/VRAM.
- Step-driven token scheduling and priority preemption (interactive requests preempt background batch jobs).
- Per-principal KV-cache isolation with zeroing on handoff.

---

## 6. Recent Work & Current State

### macOS Desktop Parity & Physical Micro-Interactions
- **Zero-Lag Continuous Dock:** Cosine bell-curve magnification ($R=110\text{px}$, $40\text{px}$ to $50\text{px}$), fixed button geometry, Cairo-only icon scaling, sinusoidal spring bounce on launch.
- **WhiteSur SVG Vector Icons:** High-fidelity squircle icons for Finder, Safari, Terminal, TextEdit, Activity Monitor, Settings, Trash, and AI Agents.
- **Window Traffic Lights:** macOS circular beads (`#ff5f57` red, `#febc2e` yellow, `#28c840` green) on client window headers.
- **Default Wallpaper:** Photorealistic `photo-03-mist-pines.jpg`.

### Multi-Touch Trackpad Gestures (`sway.conf`)
- `bindgesture pinch:3:inward` & `pinch:4:inward`: Launch Applications Modal (`tiaki-shell launcher`).
- `bindgesture pinch:3:outward` & `pinch:4:outward`: Show Desktop (`swaymsg workspace number 1`).
- `bindgesture swipe:3:up` & `swipe:4:up`: Mission Control Overview (`tiaki-shell overview`).
- `bindgesture swipe:3:down` & `swipe:4:down`: App Exposé Switcher (`tiaki-shell switcher focused`).
- `bindgesture swipe:3:left/right`: Switch workspaces prev/next.
- Native touchpad config: `dwt enabled`, `tap enabled`, `natural_scroll enabled`, `click_method clickfinger`.

### Applications Modal Overlay Redesign
- Redesigned into a floating modal ($1040 \times 680\text{px}$) with $18\text{px}$ rounded corners matching user reference.
- Horizontal category tabs: *All, Developer Tools, Productivity & Finance, Social, Utilities, Creativity, Information & Reading*.
- 7-column squircle application card grid with live search field, arrow navigation, and Escape/scrim dismiss.

### Full Modern Settings App (Zorin OS Parity)
- Dedicated settings pages with rounded-pill sidebar navigation:
  - **Wi-Fi**: Master toggle, connected network card, scanned networks list, hotspot toggle, and `nmcli` references.
  - **Network**: Wired Ethernet ($1000\text{ Mb/s}$ Full Duplex), VPN setup card with `+` button, Network Proxy, and DNS.
  - **Bluetooth & Devices**: Master toggle, paired devices with battery percentages, device scan list, and USB peripherals.
  - **Agents & AI**: Autonomy mode toggle, approval gates, model registry, KV cache preemption, workspace path, network access, and live ledger verification.
  - **Keyboard & Shortcuts**: Keybinding table with CLI equivalents, layout selector, repeat delay/speed sliders.
  - **Displays, Sound, Power, Personalization, Storage, Apps, About**.

### Complete UI & Physical Interaction Overhaul
- **Authentic macOS Frosted Glass Dock (Reference Parity):**
  - Rebuilt `.dock` in `ui/shell/css/dock.css` with multilayer specular refraction gradient (`rgba(255, 255, 255, 0.34)` to `rgba(22, 18, 16, 0.70)`), `18px` border-radius, `inset 0 1px 1px rgba(255, 255, 255, 0.65)` top rim light, and deep `0 20px 48px` drop shadow.
  - Pre-populated the dock with the full 19-item macOS application suite in `desktop/desktop.json`: Finder (`files.svg`), Launchpad (`launcher.svg`), Mail (`mail.svg`), Maps (`maps.svg`), Terminal (`terminal.svg`), Safari (`browser.svg`), Code Editor (`code.svg`), Notes (`editor.svg`), Music (`music.svg`), Calendar (`calendar.svg`), Calculator (`calc.svg`), Docker (`docker.svg`), GitHub (`github.svg`), Discord (`discord.svg`), Activity Monitor (`monitor.svg`), Settings (`settings.svg`), AI Agents (`agents.svg`), glass separator (`dock-sep`), Downloads stack (`downloads.svg`), and Trash (`trash.svg`).
  - Added crisp 1px glass divider (`dock-sep`, `min-height: 32px`, `rgba(255, 255, 255, 0.30)`) with drop shadow separating app launcher section from folder stack/trash.
  - Tightened spacing: `40x52px` buttons, `40x40px` drawing areas, $R=90\text{px}$ continuous cosine magnification (base $36\text{px}$, peak $48\text{px}$), luminous pearl running dots (`•`), and floating label pill.
  - Live remote screenshot verification captured at `tiaki-perfect-dock.png`: all 19 items perfectly centered without clipping.
- **Frosted Glass Top Menu Bar:**
  - Replaced solid opaque Cairo fill (`cr.set_source_rgb`) with `gtk::cairo::LinearGradient` and alpha compositing (`0.72` to `0.78` alpha) over the mountain sunset sky, with a 1px bottom rim stroke (`rgba(255, 255, 255, 0.16)`).
- **Typography & Font Suite:**
  - Installed **Aspekta** (modern European neo-grotesque), **SF Pro Display** (Apple System), and **Plus Jakarta Sans** in `desktop/fonts/`.
  - Configured **Aspekta** as the primary default system font across all CSS and GTK settings.
  - Added live Typography selector in Settings under Personalization with instant switching.
- **macOS High-DPI Cursor Theme & Window Interaction:**
  - Installed authentic `macOS` cursor theme (black pointer with crisp white border, size 28) in `desktop/icons/cursors/macOS`.
  - Configured `seat * xcursor_theme macOS 28` in `sway.conf` and `gtk-cursor-theme-name=macOS` in `gtk-settings.ini`.
  - Expanded floating window grab area to 8px (`default_floating_border pixel 8`) for effortless live mouse resizing from any edge or corner.
  - Added macOS Sequoia / Rectangle window snapping hotkeys (`Ctrl+Alt+Left`, `Ctrl+Alt+Right`, `Ctrl+Alt+Up`, `Ctrl+Alt+Return`, `Ctrl+Alt+Down`, `Cmd+Ctrl+*`, quadrant shortcuts).
- **Vibrant Wallpapers:**
  - Added high-definition dynamic wallpapers: `photo-06-sequoia-sunset.jpg` (golden hour mountain sunset, set as vibrant default), `photo-07-sonoma-coast.jpg`, `drawn-06-aurora-silk.jpg` (Zorin style), and `photo-08-golden-dunes.jpg`.
- **Authentic Frosted Glass Top Menu Bar (Tiaki Brand Identity):**
  - **Left Cluster:**
    - Tiaki Beacon brand mark button (`icons::logo_mark(18)`), opening system launcher/menu. Zero Apple logos or branding.
    - Active Application Title in bold (`bar-app-title`, font-weight 700, 13.5px, e.g. "Tiaki", "Terminal", "Editor", "Safari", or "Finder" when desktop is active).
    - Persistent Menus: `File`, `Edit`, `View`, `Window`, `Assistant` with subtle hover highlights, removed auto-hide logic so menus remain permanently accessible.
    - Clean typography with subtle drop shadow (`text-shadow: 0 1px 2px rgba(0, 0, 0, 0.45)`) for high legibility over both dark and bright sky wallpapers.
  - **Right Cluster (Menlets / Status Extras):**
    - Active privacy/agent status chip: orange capsule pill with white microphone icon (`( 🎙 1 approval )`).
    - AI Copilot sparkle glyph button (`✦`, `palette.open`).
    - Bluetooth status button (`tiaki-shell settings bluetooth`).
    - Wi-Fi status button (3-arc signal, `tiaki-shell settings wifi`).
    - Battery percentage label + horizontal battery capsule glyph with positive terminal bump and yellow fill (`61% [== ]`).
    - Spotlight Search icon button (`🔍`, `palette.open`).
    - Control Center double-slider toggle button (`⚲`, opens Control Center popup).
    - Clock formatted as `Fri Nov 13  15:01` (3-letter weekday, month, day, and time), opening Notification Center on click.
  - **True Frosted Glass RGBA Transparency:**
    - Removed opaque EventBox subwindow; rendered directly on RGBA layer-shell window with `Operator::Source` transparent clear + translucent glass gradient and 1px specular bottom highlight (`rgba(255, 255, 255, 0.20)`). Wallpaper colors and mountain sky flow seamlessly through the bar.
  - Verified live on remote rig `dami` with 0% idle CPU and pixel-perfect alignment (`tiaki-glass-topbar.png`, `tiaki-glass-crop.png`).
- **Mission Control Overview Grid & Unified Token Architecture (`ef2cdb6`, `7ab8338`):**
  - **Overview Grid (`overview.rs`)**: Full Mission Control window grid rendering live window captures with shared honest scaling, occluded icon fallback cards, near-opaque ink backdrop, and active window selection ring (`#d08a2c`).
  - **Unified Token Foundation (`tokens.css`)**: Single source of truth for color, type, spacing, and radius. Dock magnification and overlay entrances driven by damped spring physics (`motion.rs`). Live running indicators and real hardware introspection across Settings and the status bar.
  - Live remote screenshot verification captured at [final_step_16.png](file:///Users/eric/.gemini/antigravity/brain/792b9d32-0497-43e5-be96-7e56ab4c1e8f/browser_evidence/63dfb91c-a640-4f35-a5dd-44a1980950b5/final_step_16.png).

---

## 7. Mandatory Protocol for Future Agent Turns

Whenever completing work in this repository:
1. **Always update both `CLAUDE.md` and `agent.md`** with implementation details, decisions, file links, and verification evidence.
2. Verify that workspace checks pass (`cargo check --workspace`).
3. Commit cleanly with descriptive conventional commit messages.
4. Keep the desktop idle CPU at $0\%$ and verify VNC latency remains smooth.
