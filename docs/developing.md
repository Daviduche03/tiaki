# Developing Tiaki

## Clone

```bash
git clone <this-repo> tiaki
cd tiaki
```

`LICENSE-MIT` / `LICENSE-APACHE` — dual-licensed MIT OR Apache-2.0
(matching `workspace.package.license`). The T3 Code fork is **not** vendored:
it lives at `https://github.com/Daviduche03/t3code`
(fork of `pingdotgg/t3code`, branch `agentic/extra-mcp-servers`). The image
build expects `t3code-upstream/` beside the repo — clone it there when you
need the full desktop+T3 image (`./scripts/dev` checks this for you).

## Prerequisites

* **Rust stable** — `rust-toolchain.toml` pins `stable` + `rustfmt`, `clippy`. Install with rustup.
* **Docker** — only for the desktop image (`make desktop`) and the payload (`make payload`). Control-plane work needs no Docker.
* **macOS** — needs `SDKROOT` pointing at the current Xcode SDK or `cargo` fails to link:
  ```bash
  export SDKROOT=$(xcrun --show-sdk-path)
  ```
* **Linux (for `ui/shell` only)** —
  `pkg-config libgtk-3-dev libgtk-layer-shell-dev libglib2.0-dev libcairo2-dev libpango1.0-dev libgdk-pixbuf-2.0-dev libatk1.0-dev`
  (`ui/shell` is a separate Cargo workspace; the core workspace builds anywhere).
* **opencode** — optional, for the agent entry point only (`agentd agent run/notify` shells to it).

## Bootstrap (one command)

```bash
./scripts/dev
```

Checks for `cargo`, `rustc`, `docker`; runs `cargo check --workspace` and prints what to run next. Exits non-zero with the missing tool named.

## Everyday checks

```bash
make check      # cargo check --workspace
make test       # cargo test --workspace
make fmt        # cargo fmt --all
make clippy     # cargo clippy --workspace --all-targets -- -D warnings
make ci         # check + fmt --check + clippy + test
```

## Try the control plane (no Docker)

```bash
# ledger demo — local state is $TIAKI_HOME (default .tiaki/)
cargo run -p agentd -- check user/local/theme/mode purple --intent "Make it purple"   # rejected, state untouched
cargo run -p agentd -- apply user/local/desktop/clock/position top-right --intent "Move the clock"
cargo run -p agentd -- log
cargo run -p agentd -- verify

# single entry point (needs opencode + MCP `tiaki` server; see docs/specs/agent.md)
cargo run -p agentd -- agent run --title hello -- "Reply with ONLY the word untouched."
cargo run -p agentd -- notify --title disk -- "Run df -h via terminal.exec"
```

## Build the desktop image (Docker)

```bash
docker build -f desktop/wayland/Dockerfile -t tiaki-wayland:dev .
docker run -d --name tiaki-wayland -p 5901:5900 tiaki-wayland:dev
open vnc://localhost:5901   # macOS Screen Sharing
```

Needs `t3code-upstream/` present (fork clone above) — the image does `pnpm install` there. Screenshots are in `desktop/screenshots/`.

## Payload / bare-metal install

```bash
make payload                # stages payload/ from release builds + tiaki-wm via Docker
sudo desktop/install/install.sh /dev/<disk>   # partitions, debootstraps, lays two generations
```

## OS identity

Settings → About `OS` and `fastfetch` both read `/etc/os-release` `PRETTY_NAME=`. The image and installer write Tiaki's own os-release (`NAME="Tiaki"`, `ID_LIKE=debian`) over Debian's — firstboot rewrites it on live containers too.

## Inference

The engine is **KernRT** (`~/Documents/code/rust/KernRT`, your GitHub `Daviduche03/KernRT` — hand-written kernels). Broker wiring (`agentd inference`, `control/inference/`) is the broker; KernRT is the engine.

## Commits

Conventional titles. Run `make ci` before pushing; CI runs the same four gates.
