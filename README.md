# Tiaki

An operating system whose maintenance, customization, and repair loop is an agent — where every change is a reversible, attributable transaction.

- **Base:** Linux kernel + from-scratch userspace and control plane
- **Language:** Rust for privileged/system components; shell only inside a sandbox
- **Status:** Milestones 0–6 implemented: ledger, preflight, principals, capabilities, sandbox profiles, the plugin host, a declarative UI with a transactional keyboard keymap, an inference broker, a runtime ABI with trusted-input computer use, and invisible-reboot generations with session restore. A tree-driven GTK3 shell (menu bar, floating dock, home-screen launcher) runs on Sway/Wayland; it is still a development-host prototype, not a bootable image.

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
docker run -d --name tiaki-wayland -p 5901:5900 tiaki-wayland:dev
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

### Milestone 0 demo

`agentd` is the ledger CLI. State, snapshots, and the hash-chained log live under `$TIAKI_HOME` (default `.tiaki`).

```sh
# preflight: prove the change before touching state
cargo run -p agentd -- check user/local/theme/mode purple --intent "Make it purple"   # rejected, state untouched
cargo run -p agentd -- check user/local/desktop/clock/position top-right              # passes
cargo run -p agentd -- apply user/local/desktop/clock/position top-right --dry-run    # report only

# commit
cargo run -p agentd -- apply user/local/desktop/clock/position top-right --intent "Move the clock"
cargo run -p agentd -- log
cargo run -p agentd -- state
cargo run -p agentd -- verify
cargo run -p agentd -- revert <transaction-id>
```

### Milestone 1 demo

Every command takes a principal and an access mode. Defaults are `user:local`
and `guarded`. Users own `user/<id>/**`; cross-boundary work needs a grant.

```sh
# Ada writes her own scope
cargo run -p agentd -- --principal user:ada apply user/ada/desktop/clock/position top-right --intent "Move Ada's clock"

# Ada cannot touch Bob, in either mode
cargo run -p agentd -- --principal user:ada apply user/bob/theme/mode dark      # rejected

# An agent borrows Ada's authority, and only Ada's
cargo run -p agentd -- --principal agent:opencode:ses_1@user:ada apply user/ada/theme/mode dark

# Full access is still scoped and still sandboxed
cargo run -p agentd -- --principal user:ada --mode full apply system/services/network/state down  # rejected without a grant

# System grants a cross-boundary write that needs approval
cargo run -p agentd -- --principal system grant user:ada state.write 'system/services/network/**' --approval

# Guarded mode parks it; full mode auto-approves the same grant
cargo run -p agentd -- --principal user:ada apply system/services/network/state down    # pending-approval
cargo run -p agentd -- --principal user:ada --mode full apply system/services/network/state down

# Cross-principal revert is denied; Ada can revert her own
cargo run -p agentd -- --principal user:bob revert <ada-transaction-id>                 # rejected
cargo run -p agentd -- --principal user:ada revert <ada-transaction-id>

# Inspect grants and plan a sandbox (plan only; no process starts yet)
cargo run -p agentd -- grants
cargo run -p agentd -- --principal user:ada --mode full sandbox bash -lc "echo hi" --network

cargo run -p agentd -- log
cargo run -p agentd -- verify
```

Recovery paths (`core/`, `boot/`, `recovery/`, `system/recovery/`) are denied
in both modes, even for `system`. The log records principal, mode, and sandbox
class for every attempt. See [docs/specs/capability.md](docs/specs/capability.md)
and [docs/specs/sandbox-profiles.md](docs/specs/sandbox-profiles.md).

> The `sandbox` command only plans a spec through `SimulatedProvisioner`.
> Real Linux execution arrives with the provisioner (Rust module or the Go
> service).

Outcomes are distinct:

- **`rejected`** — a preflight gate failed. State was never touched; the attempt is still logged.
- **`pending-approval`** — allowed, but guarded mode requires trusted approval before mutation.
- **`reverted`** — preflight passed but the post-apply health check failed; the snapshot was restored.
- **`committed`** — applied and healthy.

`verify` recomputes the SHA-256 chain over every entry. See [docs/specs/validation.md](docs/specs/validation.md) for the full gate model.

### Milestone 2 demo

Plugin changes need `plugin.manage` on `system/plugins/<id>`, which by default
only `system` holds. The example clock is signed with a real content digest.

```sh
# A user cannot manage plugins
cargo run -p agentd -- --principal user:ada plugin install plugins/example-clock   # denied

# System installs the signed example
cargo run -p agentd -- --principal system plugin install plugins/example-clock
cargo run -p agentd -- plugin list

# Config is validated against the plugin's schema
cargo run -p agentd -- --principal system plugin config org.tiaki.panel.clock position=left show_seconds=true
cargo run -p agentd -- --principal system plugin config org.tiaki.panel.clock position=diagonal   # rejected
cargo run -p agentd -- plugin config org.tiaki.panel.clock

# Lifecycle persists across invocations
cargo run -p agentd -- --principal system plugin load org.tiaki.panel.clock
cargo run -p agentd -- --principal system plugin start org.tiaki.panel.clock
cargo run -p agentd -- plugin list

# The spec the provisioner would run
cargo run -p agentd -- plugin spec org.tiaki.panel.clock

# Re-sign after editing any asset
cargo run -p agentd -- plugin sign plugins/example-clock
```

A content-tier update hot-swaps live and migrates config:

```sh
# create a 1.2.1 copy with a 1.2.0 -> 1.2.1 migration, then re-sign it
cargo run -p agentd -- --principal system plugin update <dir>
cargo run -p agentd -- --principal system plugin rollback org.tiaki.panel.clock
```

Every lifecycle action is logged with principal, mode, sandbox, and a one-line
summary, and `verify` covers those entries too. See
[docs/specs/plugin-manifest.md](docs/specs/plugin-manifest.md).

### Milestone 3 demo

The UI is a declarative tree. You can edit it, render it, and drive it from the
keyboard; every edit is a transaction and revert restores the previous panel.

```sh
# See the tree for a user
cargo run -p agentd -- --principal user:ada ui get

# Rebuild the panel in one transaction: move the clock, add a button, switch to 12h
cargo run -p agentd -- --principal user:ada ui demo

# Render a self-contained page, or serve it with live reload
cargo run -p agentd -- --principal user:ada ui render --out /tmp/tiaki-ui.html
cargo run -p agentd -- --principal user:ada ui serve --port 8787

# Edit with ops, then undo
cat > /tmp/edit.json <<'EOF'
{"ops":[
  {"op":"set_prop","id":"clock","key":"show_seconds","value":true},
  {"op":"insert","parent":"panel.top","index":2,
   "node":{"id":"ready","kind":"text","props":{"value":"system ready"}}}
]}
EOF
cargo run -p agentd -- --principal user:ada ui edit /tmp/edit.json
cargo run -p agentd -- --principal user:ada revert <transaction-id>

# The keymap is a transactional resource, not compiled config
cargo run -p agentd -- --principal user:ada ui keymap
cargo run -p agentd -- --principal user:ada ui keymap default
```

Open the served page and use the keyboard: **Super+Space** or **/** for the
command palette, **Super+Left/Right** to move focus, **Super+T** to toggle the
theme, **Super+K** to toggle the clock format. Mutating actions run through the
transaction engine and appear in `agentd log`. Named actions (`focus.left`,
`theme.toggle`) are the same vocabulary an agent uses — it invokes them
directly instead of synthesizing keystrokes.

Preflight validates the tree (`ui/tree`) and keymap (`input/keymap`) before any
mutation, so a bad edit is `rejected` and the live panel is untouched. See
[docs/specs/ui-tree.md](docs/specs/ui-tree.md) and
[ADR 0006](docs/adr/0006-keyboard-first-desktop.md).

### Milestone 4 demo

The broker schedules cognition like any other resource. The first backend is a
deterministic simulator, so batching, preemption, isolation, and quotas are
testable offline.

```sh
# Registry + load (only system/service:inference may manage models)
cargo run -p agentd -- inference models
cargo run -p agentd -- --principal user:ada inference load models/sim-tiny.json   # denied
cargo run -p agentd -- --principal system inference load models/sim-tiny.json
cargo run -p agentd -- inference models

# One request as a user; agents borrow the same quota and cache scope
cargo run -p agentd -- --principal user:ada inference ask sim/tiny "summarize the system logs" --max-tokens 12
cargo run -p agentd -- --principal user:bob inference ask sim/tiny "what is broken" --priority interactive --max-tokens 8

# The exit demo: two principals batched, interactive preempts batch, cache isolation
cargo run -p agentd -- inference bench

cargo run -p agentd -- inference stats
cargo run -p agentd -- log
```

`bench` prints the active caches for Ada and Bob, preempts Ada's batch for
Carol's interactive request, resumes it, and reports 92 tokens, 1 preemption,
and 3 zeroed caches. The broker never gives one principal another's KV state:
handles are allocated per `(principal, session, model)` and zeroed on
completion or handoff.

Model load/unload are transactions (`system/inference/models/<id>`), so revert
restores the registry. Generations are recorded with principal, model,
priority, token count, and preemptions — never the prompt text. See
[docs/specs/inference.md](docs/specs/inference.md) and
[ADR 0007](docs/adr/0007-step-driven-inference-abi.md).

### Milestone 5 demo

An external runtime drives an app through a structured tree. High-risk actions
require trusted input; a synthetic approval is rejected and recorded.

```sh
# Runtime discovery and typed tools
cargo run -p agentd -- runtime list
cargo run -p agentd -- runtime spec opencode

# Inspect the semantic app tree
cargo run -p agentd -- --principal user:ada computer tree checkout

# The exit demo: observe, toggle, request approval, spoof it, approve for real, verify
cargo run -p agentd -- --principal user:ada computer demo

cargo run -p agentd -- log
```

The demo prints the protocol exchange:

```
runtime → act checkout/place invoke
host    → approval required for 'Place order' (high, appr_1)
runtime → approve appr_1 with synthetic trust
host    → decision appr_1 approved=false: approval requires physical or trusted-device input
runtime → approve appr_1 with physical trust
host    → decision appr_1 approved=true: Order placed
```

The ledger records the low-risk toggle, the **rejected synthetic approval**, the
trusted approval, and the placed order. The runtime never applies a high-risk
action itself: the host does, after `ApprovalService` accepts a `physical` or
HMAC-signed `trusted-device` event. `computer.use` is scoped to `app/<id>`;
agents borrow the user's authority. See
[docs/specs/runtime-abi.md](docs/specs/runtime-abi.md) and
[ADR 0008](docs/adr/0008-trusted-input-approval.md).

### Milestone 6 demo

An update stages an immutable generation, boots it with kexec (simulated here),
runs health probes, commits or rolls back, and restores the session. A failed
candidate auto-reverts.

```sh
# Set up a live session: a rebuilt panel, a keymap, a theme, and app state
cargo run -p agentd -- --principal system inference load models/sim-tiny.json
cargo run -p agentd -- --principal user:ada ui demo
cargo run -p agentd -- --principal user:ada ui keymap > /tmp/keymap.json
cargo run -p agentd -- --principal user:ada ui keymap set /tmp/keymap.json
cargo run -p agentd -- --principal user:ada apply user/ada/theme/mode dark --intent "Set dark theme"
cargo run -p agentd -- --principal user:ada computer demo
cargo run -p agentd -- --principal user:ada session capture

# Boot the base generation: stage → candidate → boot → health → commit → restore
cargo run -p agentd -- --principal system generation update image/examples/base --label base

# A candidate that fails its health check is rolled back automatically
cargo run -p agentd -- --principal system generation update image/examples/candidate --label candidate --fail

cargo run -p agentd -- generation status
cargo run -p agentd -- generation list
cargo run -p agentd -- log
```

The boot output:

```
booted gen_... in 184ms (attempt 1)
health session: ok
health ui-tree: ok
health keymap: ok
health inference-registry: ok
...
session restored: 1 app(s), workspace 1, focus place (tree ok, keymap ok, theme ok, 0 pending)
committed gen_...; last-good is gen_...
reboot complete: 184ms downtime, session restored — it reads as a repaint
```

The `--fail` run prints the failing probe and `rolled back gen_candidate ->
gen_base`. The generation table keeps last-good, the failed candidate is marked
`failed`, and the session is restored either way. `generation.manage` is
system-only (`service:update` also holds it), and every stage/boot/commit/
rollback is on the ledger. See
[docs/specs/generations.md](docs/specs/generations.md) and
[ADR 0009](docs/adr/0009-invisible-reboot.md).

> macOS note: if linking fails with `unknown architecture arm64e.x1-macos`, point `SDKROOT` at the Xcode SDK, e.g.
> `SDKROOT=$(ls -d /Applications/Xcode.app/Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX*.sdk | tail -1) cargo test --workspace`.

## Milestones

See §21 of [TIAKI.md](TIAKI.md). Milestone 0 is the ledger: transaction engine, snapshots, rollback, immutable log, and a plain-language summary on every change. Milestone 1 is authority: principals, scoped capability grants, guarded vs full mode, and sandbox profiles. Milestone 2 is the plugin host: manifests, signatures, lifecycle, hot-swap, config schemas, and migrations. Milestone 3 is the declarative UI: tree schema, live agent editing, a host-side renderer, and a transactional keyboard-first keymap. Milestone 4 is the inference broker: model registry, step-driven backend ABI, continuous batching, preemption, per-principal KV isolation, and quotas. Milestone 5 is the runtime ABI and computer use: protocol, structured app trees, trusted input, and approval-gated actions. Milestone 6 is invisible reboot: generation table, watchdog, kexec, health-gated commits, and session restore.

## Documentation

- [Architecture](TIAKI.md)
- [ADRs](docs/adr/)
- [Specs](docs/specs/)
