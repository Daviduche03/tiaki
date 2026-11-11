# Tiaki

An operating system whose maintenance, customization, and repair loop is an agent — where every change is a reversible, attributable transaction.

- **Base:** Linux kernel + from-scratch userspace and control plane
- **Language:** Rust for privileged/system components; shell only inside a sandbox
- **Status:** Shipped: ledger + preflight + principals + capabilities + sandbox, plugin host, transactional UI tree, inference broker, single agent entry point (`agentd agent run|notify` → opencode), MCP capability server (`agentd mcp serve`), Sway desktop (`tiaki-shell`). A bootable image pipeline is not complete yet — the desktop runs as a dev container, not yet a bootable OS.

The full design lives in [TIAKI.md](TIAKI.md). This repository is the documented project structure from §22 of that document.

## Desktop

The desktop is a real **Sway/Wayland session with our own tree-driven shell**
(`ui/shell`): a macOS-like menu bar, floating dock, Spotlight-style launcher,
dropdown menus, and Control Center. The bar, dock, launcher, menus, status
items, and app catalog are all nodes in `desktop/desktop.json`, served by
`agentd` at `/tree.json`. Adding an app or restyling the desktop is a
transaction, not a rebuild.

- [`desktop/screenshots/macos-dark.png`](desktop/screenshots/macos-dark.png)
- [`desktop/screenshots/macos-launcher.png`](desktop/screenshots/macos-launcher.png)
- [`desktop/screenshots/macos-menu.png`](desktop/screenshots/macos-menu.png)
- [`desktop/screenshots/macos-control.png`](desktop/screenshots/macos-control.png)
- [`desktop/screenshots/macos-wallpaper.png`](desktop/screenshots/macos-wallpaper.png)
- [`desktop/screenshots/macos-search.png`](desktop/screenshots/macos-search.png)
- [`desktop/screenshots/macos-settings.png`](desktop/screenshots/macos-settings.png)

```sh
docker build -f desktop/wayland/Dockerfile -t tiaki-wayland:dev .
docker run -d --privileged --name tiaki-wayland -p 5901:5900 tiaki-wayland:dev
open vnc://localhost:5901        # macOS Screen Sharing
```

Super+Space opens the launcher; Super+1..5 switch workspaces; Super+F
fullscreen; Super+Shift+M restores minimized windows; Super+Shift+L locks. Sway is the compositor, so blur, shadows, rounded window corners, and
workspace animations remain the compositor-layer gap (SwayFX/Wayfire, then our
own). The earlier HTML renderer in `desktop/Dockerfile` is a preview, not the
desktop; `ui/native` is the compact GTK3 tree-renderer experiment.

See [`desktop/README.md`](desktop/README.md) and
[`ui/shell/README.md`](ui/shell/README.md).

## Layout

```
tiaki/
├── TIAKI.md              # design source of truth
├── Cargo.toml                 # workspace
├── core/                      # recovery core (minimal, signed, immutable at runtime)
│   ├── boot/                  # verified boot chain, generation loading
│   ├── init/                  # minimal PID 1
│   ├── json/                  # dependency-free JSON for state, UI, manifests
│   ├── log/                   # append-only, hash-chained action log
│   ├── toml/                  # strict TOML subset for manifests and discovery
│   └── rollback/              # generation table, boot watchdog, fallback
├── control/                   # control plane
│   ├── agentd/                # daemon entry point
│   ├── capability/            # principals, grants, elevation, policy
│   ├── transaction/           # validate → snapshot → apply → verify → commit
│   ├── plugin-host/           # manifest, isolation, lifecycle, hot-swap
│   ├── state-api/             # semantic system introspection
│   ├── explain/               # plain-language summaries + mismatch detection
│   ├── sandbox/               # profile resolver + provisioner ABI
│   └── inference/             # model broker
├── ui/                        # compositor + UI host + toolkit
│   ├── compositor/
│   ├── tree/                  # declarative UI tree, ops, validation
│   ├── shell/                 # tree-driven GTK3 bar, dock, launcher
│   ├── native/                # compact GTK3 tree renderer
│   └── widgets/               # HTML preview
├── runtime/                   # agent ABI + adapters
│   ├── abi/
│   ├── opencode/
│   └── codex/
├── plugins/                   # first-party plugins
├── models/                    # model specs (weights are content-addressed)
├── image/                     # image build, signing, updates
├── docs/                      # ADRs and specs
└── tests/                     # integration, chaos, replay
```

## Quickstart

Prerequisites + full developer guide: `docs/developing.md` (clone → check → try the
ledger → Docker desktop → bare-metal payload).

```sh
git clone <this-repo> tiaki && cd tiaki

./scripts/dev                 # checks prerequisites, then cargo check
make test                     # 178 tests
make ci                       # check + fmt + clippy + test (what CI runs)
```

Dual-licensed MIT OR Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE`).

## Commands

```sh
# Ledger: preflight → commit → verify (all recorded)
cargo run -p agentd -- check user/local/theme/mode purple --intent "Make it purple"   # rejected
cargo run -p agentd -- apply user/local/desktop/clock/position top-right --intent "Move the clock"
cargo run -p agentd -- log && cargo run -p agentd -- verify && cargo run -p agentd -- revert <id>

# UI tree: get/set/edit are transactions; the shell re-renders from /tree.json
cargo run -p agentd -- --principal user:ada ui get
cargo run -p agentd -- --principal user:ada ui edit /tmp/edit.json  # {"ops":[...]}
cargo run -p agentd -- --principal user:ada ui serve --port 8787

# Agent: the single entry point — message and notification start a session the same way
cargo run -p agentd -- agent run --title hello -- "Reply with ONLY the word untouched."
cargo run -p agentd -- notify --title disk -- "Run df -h via terminal.exec"

# Inference priority + scheduling (inference > sessions > foreground > background)
cargo run -p agentd -- inference burst on
cargo run -p agentd -- sandbox probe
```

Specs: [agent entry point](docs/specs/agent.md) · [MCP tools](docs/specs/mcp.md) ·
[transactions](docs/specs/transaction.md) · [validation](docs/specs/validation.md) ·
[generations](docs/specs/generations.md) · [TIAKI §21 roadmap](TIAKI.md#21-roadmap).

## Documentation



- [Architecture](TIAKI.md)
- [ADRs](docs/adr/)
- [Specs](docs/specs/)
