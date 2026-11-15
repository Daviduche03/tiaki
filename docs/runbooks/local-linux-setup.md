# Running Tiaki OS on Local Linux (Zorin OS, Ubuntu, Debian)

This guide provides step-by-step instructions for running **Tiaki OS** natively on a local Linux computer (such as a dual-boot PC or dedicated workstation running **Zorin OS**, **Ubuntu 22.04/24.04 LTS**, **Debian 12/13**, or other modern distributions).

---

## Why Run Locally?

- **On macOS:** Tiaki's graphical shell (`tiaki-shell`) depends on Linux Wayland protocols (`wlr-layer-shell-unstable-v1`), Linux cgroups v2 (`sched-launch`), and Linux DRM/KMS. On macOS, Tiaki runs inside a Docker container streamed over Web VNC (noVNC) or raw RFB.
- **On Linux (Zorin OS / Ubuntu / Debian):** You can run Tiaki directly on the local graphics hardware without any VNC streaming latency, with 100% native Wayland performance, zero-lag input, hardware cursor acceleration, and physical multi-touch trackpad gestures.

---

## Method 1: Nested Session in a Window (Fastest for Dev & Daily Use)

In this mode, Tiaki runs inside an isolated, floating Wayland window directly on top of your existing Zorin OS / GNOME desktop. You do **not** need to log out of your current user session.

### Step 1: Install System Dependencies

On Zorin OS / Ubuntu / Debian:

```bash
sudo apt update
sudo apt install -y \
    build-essential pkg-config libgtk-3-dev libgtk-layer-shell-dev \
    libglib2.0-dev libcairo2-dev libpango1.0-dev libgdk-pixbuf-2.0-dev libatk1.0-dev \
    sway swaybg foot btop fonts-jetbrains-mono fonts-font-awesome adwaita-icon-theme \
    grim curl unzip xz-utils libgtk-3-0 libgtk-layer-shell0 ca-certificates wtype \
    wl-clipboard brightnessctl pamixer fastfetch librsvg2-common python3 git
```

Install the Rust toolchain (if not already present):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
rustup default stable
```

### Step 2: Install Required Fonts & Cursors

Tiaki uses **Aspekta**, **IBM Plex Sans**, and the authentic macOS high-DPI cursor suite:

```bash
# Refresh system font cache with Tiaki bundled fonts
sudo mkdir -p /usr/share/fonts/truetype/tiaki
sudo cp -r desktop/fonts/* /usr/share/fonts/truetype/tiaki/
sudo fc-cache -f

# Install macOS cursor suite
sudo mkdir -p /usr/share/icons
sudo cp -r desktop/icons/cursors/macOS /usr/share/icons/
sudo cp -r desktop/icons/cursors/macOS-White /usr/share/icons/
```

### Step 3: Build the Tiaki Binaries

In the root of the `tiaki` repository:

```bash
# 1. Build the transaction engine and UI tree server
cargo build --release -p agentd

# 2. Build the GTK3 layer-shell (Dock, Top Menu Bar, Settings, Launcher, Overview)
cargo build --release --manifest-path ui/shell/Cargo.toml

# 3. Build the process scheduler and ACP runtime
cargo build --release -p agentic-agents
cargo build --release -p agentic-runtime-acp
```

### Step 4: Seed the UI Tree and Launch the Nested Session

Run the launcher script or execute directly:

```bash
# 1. Create runtime directory
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp/tiaki-runtime}"
mkdir -p "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"

# 2. Seed default desktop tree and start agentd UI server in the background
./target/release/agentd --principal user:local ui set desktop/desktop.json
./target/release/agentd --principal user:local ui serve --host 127.0.0.1 --port 8787 &

# 3. Launch Sway using the Tiaki configuration (opens in a window on your desktop)
sway -c desktop/wayland/sway.conf
```

A 1440x900 window will open containing the complete Tiaki desktop:
- Authentic frosted glass top menu bar with brand Beacon, menus, and status menlets.
- Magnifying frosted glass macOS-style dock with full 19-application suite.
- Press `Super+Space` to launch the Applications Modal.
- Press `Super+Return` to open Foot terminal.
- Launch the Settings app or Mission Control Overview (`Super+Tab`).

---

## Method 2: Native Bare-Metal Desktop Session (Full OS Experience)

Turn your Zorin OS machine into a dedicated Tiaki OS station by adding Tiaki to your system display manager (GDM / LightDM / SDDM).

### Step 1: Install Binaries to `/usr/local/bin`

After building (from Step 3 above):

```bash
sudo cp target/release/agentd /usr/local/bin/
sudo cp ui/shell/target/release/tiaki-shell /usr/local/bin/
sudo cp target/release/sched-launch /usr/local/bin/
sudo cp target/release/acp-run /usr/local/bin/
sudo cp desktop/wayland/tiaki-session /usr/local/bin/
sudo cp desktop/wayland/tiaki-wallpaper.sh /usr/local/bin/tiaki-wallpaper
sudo chmod 0755 /usr/local/bin/agentd /usr/local/bin/tiaki-shell \
    /usr/local/bin/sched-launch /usr/local/bin/acp-run \
    /usr/local/bin/tiaki-session /usr/local/bin/tiaki-wallpaper
```

### Step 2: Install Desktop Assets & Configurations

```bash
sudo mkdir -p /etc/tiaki /usr/share/tiaki /var/lib/tiaki
sudo cp desktop/wayland/sway.conf /etc/tiaki/
sudo cp desktop/wayland/wayvnc.conf /etc/tiaki/
sudo cp desktop/wayland/foot.ini /etc/tiaki/
sudo cp desktop/desktop.json /usr/share/tiaki/
sudo cp -r desktop/icons /usr/share/tiaki/
sudo cp -r desktop/wallpapers /usr/share/tiaki/
```

### Step 3: Register the Wayland Session in Display Manager

Create `/usr/share/wayland-sessions/tiaki.desktop`:

```ini
[Desktop Entry]
Name=Tiaki
Comment=Tiaki Autonomous Agent Operating System
Exec=/usr/local/bin/tiaki-session
Type=Application
DesktopNames=Tiaki
```

### Step 4: Login

1. Log out of your Zorin OS desktop session.
2. At the login screen (GDM), click on your username.
3. Click the gear icon (`⚙`) in the bottom-right corner of the screen.
4. Select **Tiaki**.
5. Enter your password and log in.
6. The display manager will launch `tiaki-session`, which initializes DRM/KMS hardware rendering, sets the high-definition wallpaper, and starts the full Tiaki environment.

---

## Method 3: Local Docker Container (Isolated Sandbox)

If you do not want to install any GTK dev packages on your Linux host:

```bash
# 1. Build the dev container
docker build -f desktop/wayland/Dockerfile -t tiaki-wayland:dev .

# 2. Run container in privileged mode (required for systemd and cgroups v2)
docker run -d --name tiaki-wayland --privileged \
    -p 127.0.0.1:5900:5900 \
    -p 127.0.0.1:6080:6080 \
    tiaki-wayland:dev

# 3. View live in your web browser:
xdg-open "http://localhost:6080/vnc.html?autoconnect=true&resize=scale"
```

---

## Physical Trackpad Gestures & Shortcuts Reference

| Action | Trackpad Gesture | Keyboard Shortcut | CLI Equivalent |
|---|---|---|---|
| **Applications Modal** | Pinch inward (3 or 4 fingers) | `Super + Space` | `tiaki-shell launcher` |
| **Mission Control** | Swipe up (3 or 4 fingers) | `Super + Tab` | `tiaki-shell overview` |
| **App Exposé / Switcher**| Swipe down (3 or 4 fingers) | `Alt + Tab` | `tiaki-shell switcher focused` |
| **Show Desktop** | Pinch outward (3 or 4 fingers) | `Super + D` | `swaymsg workspace number 1` |
| **Switch Workspaces** | Swipe left / right (3 or 4 fingers) | `Super + 1..5` | `swaymsg workspace <n>` |
| **Snap Window Left** | — | `Ctrl + Alt + Left` | `swaymsg move position 0 28, resize set 720 872` |
| **Snap Window Right** | — | `Ctrl + Alt + Right` | `swaymsg move position 720 28, resize set 720 872` |
| **Maximize Window** | — | `Ctrl + Alt + Return` | `swaymsg move position 0 28, resize set 1440 872` |
| **Lock Screen** | — | `Ctrl + Super + Q` or `Super + L` | `swaylock` |
| **Terminal** | — | `Super + Return` | `foot` |
| **Settings** | — | Click Dock Settings | `tiaki-shell settings` |
| **AI Copilot** | — | Click Top Bar `✦` | `tiaki-shell palette` |

---

## Troubleshooting on Zorin OS / Ubuntu

1. **`libgtk-layer-shell-dev` not found:**
   - Make sure `universe` repository is enabled: `sudo add-apt-repository universe && sudo apt update`.
2. **Missing Wayland display socket:**
   - Ensure `XDG_RUNTIME_DIR` is set and owned by your user (`chmod 700 $XDG_RUNTIME_DIR`).
3. **Cgroup subtree warning (`sched-launch: advisory`):**
   - Tiaki creates cgroup slice `/sys/fs/cgroup/tiaki`. In nested mode, cgroups are advisory and apps will still run normally without root.
4. **Font rendering looks incorrect:**
   - Run `sudo fc-cache -f -v` to ensure `Aspekta` and `IBM Plex Sans` are loaded.
