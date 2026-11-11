#!/bin/sh
# Build every Tiaki binary and stage it into $1 (default /out).
#
# The image build calls this with the cargo registry and target directories
# mounted as BuildKit caches. It retries the whole compile because crates.io is
# the one network dependency left in the build, and container DNS here fails
# intermittently; cargo resumes from the target cache, so a retry is cheap.
#
# Usage: build-payload.sh [out-dir]
set -eu

OUT="${1:-/out}"
ATTEMPTS="${ATTEMPTS:-3}"

mkdir -p "$OUT"

build() {
  cargo build --release -p agentd
  cargo build --release --manifest-path ui/native/Cargo.toml
  cargo build --release --manifest-path ui/shell/Cargo.toml
  cargo build --release -p agentic-agents
  cargo build --release -p agentic-runtime-acp
}

stage() {
  cp target/release/agentd "$OUT/"
  cp ui/native/target/release/tiaki-ui-native "$OUT/"
  cp ui/shell/target/release/tiaki-shell "$OUT/"
  cp target/release/sched-launch "$OUT/"
  cp target/release/acp-run "$OUT/"
}

attempt=1
while :; do
  if build && stage; then
    break
  fi
  if [ "$attempt" -ge "$ATTEMPTS" ]; then
    echo "build failed after $attempt attempt(s)" >&2
    exit 1
  fi
  echo "build attempt $attempt failed; retrying" >&2
  attempt=$((attempt + 1))
  sleep 5
done

for bin in agentd tiaki-ui-native tiaki-shell sched-launch acp-run; do
  test -x "$OUT/$bin" || { echo "missing $OUT/$bin after build" >&2; exit 1; }
done

echo "staged: $(ls "$OUT" | tr '\n' ' ')"
