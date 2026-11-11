#!/bin/sh
# QEMU harness: boots Debian ARM64, runs the generation/kexec cycle, asserts.
#
# Usage: harness.sh [workdir]
# Needs: qemu-system-aarch64, qemu-img, mkisofs, ssh, scp, curl.
# Artifacts (not in git): <workdir>/debian-13-generic-arm64.qcow2 (base),
# overlay qcow2, seed.iso, id_ed25519 keypair.
#
# Exit 0 only if every guest check prints PASS.
set -eu

WORK="${1:-/tmp/tiaki-qemu}"
BASE="$WORK/debian-13-generic-arm64.qcow2"
IMG="$WORK/guest.qcow2"
SEED="$WORK/seed.iso"
KEY="$WORK/id_ed25519"
PORT=2227

[ -f "$BASE" ] || {
  echo "missing $BASE (Debian trixie generic-arm64 cloud image)"; exit 1; }
[ -n "${AGENTD_BIN:-}" ] || {
  echo "set AGENTD_BIN to an ARM64 agentd binary (cargo build -p agentd)"; exit 1; }
[ -f "$AGENTD_BIN" ] || { echo "no such file: $AGENTD_BIN"; exit 1; }
command -v qemu-system-aarch64 >/dev/null || { echo "need qemu"; exit 1; }
command -v mkisofs >/dev/null || { echo "need mkisofs (cdrtools)"; exit 1; }

[ -f "$KEY" ] || ssh-keygen -t ed25519 -N "" -f "$KEY" -q
PUBKEY="$(cat "$KEY.pub")"
mkdir -p "$WORK/seed"
sed "s|__SSH_PUBKEY__|$PUBKEY|" "$(dirname "$0")/seed-user-data" > "$WORK/seed/user-data"
cp "$(dirname "$0")/seed-meta-data" "$WORK/seed/meta-data"
mkisofs -output "$SEED" -volid cidata -joliet -rock "$WORK/seed/user-data" "$WORK/seed/meta-data" >/dev/null

qemu-img create -f qcow2 -b "$BASE" -F qcow2 "$IMG" 16G >/dev/null

FW="$(ls /opt/homebrew/Cellar/qemu/*/share/qemu/edk2-aarch64-code.fd 2>/dev/null | head -1)"
[ -n "$FW" ] || { echo "need edk2-aarch64-code.fd (brew reinstall qemu?)"; exit 1; }

qemu-system-aarch64 \
  -M virt -cpu host -accel hvf -m 2048 -smp 2 \
  -bios "$FW" \
  -drive "file=$IMG,format=qcow2,if=virtio" \
  -drive "file=$SEED,format=raw,if=virtio,readonly=on" \
  -nic "user,model=virtio-net-pci,hostfwd=tcp:127.0.0.1:$PORT-:22" \
  -nographic -serial mon:stdio >"$WORK/console.log" 2>&1 &
QEMU_PID=$!
cleanup() { kill "$QEMU_PID" 2>/dev/null || true; }
trap cleanup EXIT

SSH="ssh -i $KEY -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ConnectTimeout=5 -p $PORT tiaki@127.0.0.1"
echo "-- waiting for ssh (up to 4 min)"
for _ in $(seq 1 48); do
  $SSH true 2>/dev/null && break
  sleep 5
done
$SSH true || { echo "guest never came up; see $WORK/console.log"; exit 1; }

echo "-- provisioning guest"
$SSH "sudo apt-get update -qq && sudo apt-get install -y -qq kexec-tools" >/dev/null
scp -i "$KEY" -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -P "$PORT" "$AGENTD_BIN" tiaki@127.0.0.1:agentd
scp -i "$KEY" -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -P "$PORT" "$(dirname "$0")/guest-phase1.sh" "$(dirname "$0")/guest-phase2.sh" tiaki@127.0.0.1:

echo "-- phase 1"
BOOT_ID="$($SSH sh guest-phase1.sh | tee "$WORK/phase1.log" | sed -n 's/^BOOT_ID=//p' | tail -1)"
if grep -q "^FAIL" "$WORK/phase1.log" || ! grep -q "^PHASE1_DONE" "$WORK/phase1.log"; then
  echo "phase 1 failed or aborted"
  exit 1
fi

echo "-- reboot (full UEFI reboot, not kexec: kexec panics PID 1 deterministically"
echo "   under Apple HVF on this kernel - see docs/specs/boot.md. The generation"
echo "   and ledger assertions are identical either way.)"
$SSH "sudo nohup sh -c 'sleep 2; reboot' >/dev/null 2>&1 & echo rebooting" || true

echo "-- waiting for guest to return (up to 4 min)"
sleep 20
for _ in $(seq 1 48); do
  $SSH true 2>/dev/null && break
  sleep 5
done
$SSH true || { echo "guest never came back; see $WORK/console.log"; exit 1; }

echo "-- phase 2"
$SSH sh guest-phase2.sh "$BOOT_ID" | tee "$WORK/phase2.log"
if grep -q "^FAIL" "$WORK/phase2.log" || ! grep -q "^PHASE2_DONE" "$WORK/phase2.log"; then
  echo "phase 2 failed or aborted"
  exit 1
fi
echo "ALL GUEST CHECKS PASSED"
