#!/bin/sh
# First-boot (and every-boot) host setup for Tiaki.
#
# Seeds control-plane state on a fresh $TIAKI_HOME, then delegates a
# cgroup subtree so scheduled launches are enforced by the kernel instead
# of running unsupervised. Delegation is best-effort: read-only cgroup
# mounts (plain containers) keep the advisory fallback.
set -eu

export TIAKI_HOME="${TIAKI_HOME:-/var/lib/tiaki}"
mkdir -p "$TIAKI_HOME"

# Debian is the base; the OS is Tiaki. Rewrite os-release on every boot so an
# image built before the rebrand (or one whose base package reinstalled it)
# still reports Tiaki to Settings > About, fastfetch, and apps.
if ! grep -q '^ID=tiaki$' /usr/lib/os-release 2>/dev/null; then
  mkdir -p /usr/lib
  cat > /usr/lib/os-release <<'OSRELEASE'
PRETTY_NAME="Tiaki"
NAME="Tiaki"
ID=tiaki
ID_LIKE=debian
VERSION="0.1 (dev)"
VERSION_ID="0.1"
HOME_URL="https://github.com/Daviduche03/tiaki"
BUG_REPORT_URL="https://github.com/Daviduche03/tiaki/issues"
OSRELEASE
  ln -sf ../usr/lib/os-release /etc/os-release
fi

# Per-home workspaces. The system home serves the daemons; the desktop
# principal gets its own home so agent file acts land in one attributable
# place ($TIAKI_HOME/workspace) instead of wherever a child was spawned.
mkdir -p "$TIAKI_HOME/workspace"
if id desktop >/dev/null 2>&1; then
  su -s /bin/sh desktop -c 'mkdir -p /home/desktop/.tiaki/workspace' 2>/dev/null || true
fi

if [ ! -e "$TIAKI_HOME/state" ]; then
  /usr/local/bin/agentd --principal user:ada ui set /usr/share/tiaki/desktop.json \
    || echo "tiaki-firstboot: could not install the shell tree" >&2
  /usr/local/bin/agentd --principal user:ada apply user/ada/theme/mode dark \
    --intent "Set the desktop theme" || true
fi

if mkdir -p /sys/fs/cgroup/tiaki 2>/dev/null; then
  # One write per controller: a bundled write fails wholesale when any
  # controller is unavailable (e.g. cpuset without parent enablement), and
  # `sandbox probe` is the real gate on the result anyway.
  for controller in cpu memory pids cpuset; do
    echo "+$controller" > /sys/fs/cgroup/tiaki/cgroup.subtree_control 2>/dev/null || true
  done
  chown desktop:desktop /sys/fs/cgroup/tiaki 2>/dev/null || true
  echo "tiaki-firstboot: cgroup subtree delegated at /sys/fs/cgroup/tiaki"
else
  echo "tiaki-firstboot: cgroup delegation unavailable; scheduling is advisory" >&2
fi

# Clock + DNS without coupling the session to firstboot. The rig proved
# both failure modes: a 2-day clock skew breaks TLS, and missing DNS
# breaks every fetch. Best-effort: never fail the boot for this.
if command -v timedatectl >/dev/null 2>&1; then
  timedatectl set-ntp true 2>/dev/null || true
fi
mkdir -p /etc/systemd/resolved.conf.d
if [ ! -e /etc/systemd/resolved.conf.d/tiaki-fallback.conf ]; then
  cat > /etc/systemd/resolved.conf.d/tiaki-fallback.conf <<'RESOLVED' 2>/dev/null || true
[Resolve]
FallbackDNS=8.8.8.8 1.1.1.1
RESOLVED
fi

# Seed the desktop principal's agent homes. Idempotent merges: hand-chosen
# values are never overwritten, missing pieces are filled in.
if id desktop >/dev/null 2>&1; then
  su -s /bin/sh desktop -c '/usr/bin/python3 -' <<'PYEOF' 2>/dev/null || true
import json, os

# opencode global config: model plus the tiaki MCP server with concrete
# paths. Concrete, not {env:} references: servers T3 spawns do not inherit
# our environment, and empty expansion silently breaks the spawn.
oc_path = "/home/desktop/.config/opencode/opencode.json"
os.makedirs(os.path.dirname(oc_path), exist_ok=True)
try:
    oc = json.load(open(oc_path))
except (OSError, ValueError):
    oc = {}
oc.setdefault("model", "meta/muse-spark-1.3-contributor")
servers = oc.setdefault("mcp", {}).setdefault("servers", {})
servers["tiaki"] = {
    "type": "local",
    "command": ["/usr/local/bin/agentd", "mcp", "serve"],
    "environment": {
        "TIAKI_HOME": "/home/desktop/.tiaki",
        "TIAKI_PRINCIPAL": "user:ada",
        "TIAKI_WORKSPACE": "/home/desktop/.tiaki/workspace",
    },
}
json.dump(oc, open(oc_path, "w"), indent=2)

# T3 Code settings: enable the opencode provider (T3 ships it disabled).
# Created when missing; T3 fills in the rest on first launch.
for settings_path in ("/home/desktop/.t3/dev/settings.json",):
    os.makedirs(os.path.dirname(settings_path), exist_ok=True)
    try:
        settings = json.load(open(settings_path))
    except (OSError, ValueError):
        settings = {}
    instances = settings.setdefault("providerInstances", {})
    entry = instances.setdefault("opencode", {})
    entry["driver"] = "opencode"
    entry.setdefault("displayName", "OpenCode")
    entry["enabled"] = True
    entry.setdefault("config", {})["binaryPath"] = "/usr/local/bin/opencode"
    json.dump(settings, open(settings_path, "w"), indent=2)
PYEOF
  echo "tiaki-firstboot: desktop agent homes seeded"
fi
