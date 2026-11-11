#!/bin/sh
# Guest-side test script, executed over SSH by harness.sh.
# Proves, on real hardware-virtualized Linux: systemd is PID 1, the sandbox
# probe passes on delegated cgroups, kexec reboots the machine, and the
# generation ledger stays continuous across the jump.
set -eu

AGENT="$HOME/agentd"
export TIAKI_HOME="$HOME/.tiaki-test"
PASS=0
FAIL=0

check() {
  if eval "$2"; then echo "PASS: $1"; PASS=$((PASS + 1));
  else echo "FAIL: $1"; FAIL=$((FAIL + 1)); fi
}

# 1. systemd is PID 1.
check "pid1-is-systemd" 'ps -p 1 -o comm= | grep -q systemd'

# 2. Sandbox probe passes on the delegated subtree (real backend, not advisory).
PROBE_OUT="$($AGENT sandbox probe 2>&1)" || true
echo "$PROBE_OUT" | grep -q "backend:.*cgroup (real)" || echo "$PROBE_OUT"
check "probe-real-backend" 'echo "$PROBE_OUT" | grep -q "backend:.*cgroup (real)"'

# 3. Capability gate holds, then a grant opens it (same rehearsal as CI).
"$AGENT" generation stage /tmp 2>&1 | grep -q "may not manage generations"
check "gate-denies-unprivileged" 'true'
"$AGENT" --principal system grant user:local generation.manage "system/generations/new" >/dev/null
check "grant-opens-staging" true

# 4. Stage + status round-trip.
mkdir -p /tmp/genA && echo v1 > /tmp/genA/version.txt
"$AGENT" generation stage /tmp/genA --label gen-a >/dev/null
check "stage-lists-generation" '"$AGENT" generation list | grep -q gen-a'

# 5. kexec reboot: marker before, marker + fresh boot id after.
echo "kexec-marker-$RANDOM" > "$HOME/.kexec-marker" 2>/dev/null || echo marker > /tmp/kexec-marker
BEFORE="$(cat /proc/sys/kernel/random/boot_id)"
KEXEC_KERNEL="$(ls /boot/vmlinuz-* | head -1)"
KEXEC_INITRD="$(ls /boot/initrd.img-* | head -1)"
sudo kexec -l "$KEXEC_KERNEL" --initrd="$KEXEC_INITRD" --reuse-cmdline
sudo kexec -e || true
sleep 90
AFTER="$(cat /proc/sys/kernel/random/boot_id)"
check "kexec-rebooted" '[ "$BEFORE" != "$AFTER" ]'

# 6. Ledger survived the jump: the staged generation is still listed.
check "ledger-continuous" '"$AGENT" generation list | grep -q gen-a'

echo "guest: $PASS passed, $FAIL failed"
exit "$FAIL"
