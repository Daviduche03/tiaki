#!/bin/sh
# Phase 1 (pre-reboot): policy, probe, staging. Prints BOOT_ID=<id> last.
# Any unexpected failure prints FAIL and stops the harness from proceeding
# blind (an aborted script with no FAIL line looks like success otherwise).
set -eu
AGENT="$HOME/agentd"
export TIAKI_HOME="$HOME/.tiaki-test"

[ "$(ps -p 1 -o comm=)" = systemd ] && echo "PASS pid1-is-systemd" || echo "FAIL pid1-is-systemd"

# Real machines delegate a subtree at boot (firstboot does this); without
# it the probe must report advisory, and the test would prove nothing.
sudo mkdir -p /sys/fs/cgroup/tiaki
# Controllers are enabled one per write: a single bundled write fails
# wholesale if any controller is unavailable (cpuset needs parent enablement
# the cloud kernel may lack), and the probe below is the real gate anyway.
for controller in cpu memory pids cpuset; do
  echo "+$controller" | sudo tee /sys/fs/cgroup/tiaki/cgroup.subtree_control >/dev/null 2>&1 || true
done
sudo chown -R "$(id -u):$(id -g)" /sys/fs/cgroup/tiaki
"$AGENT" sandbox probe 2>&1 | grep -q "backend:.*cgroup (real)" \
  && echo "PASS probe-real-backend" || echo "FAIL probe-real-backend"

if "$AGENT" generation stage /tmp 2>&1 | grep -q "may not manage generations"; then
  echo "PASS gate-denies-unprivileged"
else
  echo "FAIL gate-denies-unprivileged"
fi
"$AGENT" --principal system grant user:local generation.manage "system/generations/new" >/dev/null 2>&1 || true

mkdir -p /tmp/genA && echo v1 > /tmp/genA/version.txt
"$AGENT" generation stage /tmp/genA --label gen-a >/dev/null
"$AGENT" generation list 2>/dev/null | grep -q gen-a \
  && echo "PASS stage-lists-generation" || echo "FAIL stage-lists-generation"

echo "BOOT_ID=$(cat /proc/sys/kernel/random/boot_id)"
echo "PHASE1_DONE"
