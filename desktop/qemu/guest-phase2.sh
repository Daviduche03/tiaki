#!/bin/sh
# Phase 2 (post-reboot): the machine rebooted iff BOOT_ID (argument) differs
# from the running one, and the ledger still lists the staged generation.
set -eu
AGENT="$HOME/agentd"
export TIAKI_HOME="$HOME/.tiaki-test"
BEFORE="$1"

AFTER="$(cat /proc/sys/kernel/random/boot_id)"
if [ "$BEFORE" != "$AFTER" ]; then
  echo "PASS rebooted ($BEFORE -> $AFTER)"
else
  echo "FAIL rebooted (boot_id unchanged)"
fi
"$AGENT" generation list 2>/dev/null | grep -q gen-a \
  && echo "PASS ledger-continuous" || echo "FAIL ledger-continuous"
echo "PHASE2_DONE"
