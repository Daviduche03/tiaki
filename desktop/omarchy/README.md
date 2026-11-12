# Omarchy shell + Hyprland defaults (vendored)

One-to-one port of the Omarchy desktop chrome so Tiaki stops hand-rolling
what commodity layers already do better.

- Source: https://github.com/omacom/omarchy (MIT, see UPSTREAM-LICENSE)
- `shell/` — Omarchy Quickshell desktop shell (bar, dock, lock, menu,
  notifications, OSD, panels, …) verbatim, Tokyo Night defaults.
- `default-hypr/` — Omarchy Hyprland defaults (Lua, Hyprland ≥0.55).
- `themes/tokyo-night/` — default theme, as shipped.
- `config/shell.json` — default shell layout.

Deliberately NOT vendored: `agents/` plugin and `omarchy-agent*`
helpers (Tiaki keeps its own agent runtime), per-app theme setters,
Arch install/provisioning.

Version skew to watch: Debian sid ships Hyprland 0.57 + Quickshell
0.3.2; Omarchy 4.0-alpha tracks newer upstreams. QML errors at startup
are the signal — iterate there, not by guessing.
