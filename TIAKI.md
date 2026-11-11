# Tiaki — Vision & Architecture

> **Name:** `Tiaki`
> **Status:** Design document, v0.1
> **Base:** Linux kernel + from-scratch userspace and control plane
> **Primary language:** Rust (privileged/system components), with shell as a sandboxed tool — never a trust boundary
> **Core claim:** An operating system whose maintenance, customization, and repair loop is an agent, and where every change is a reversible, attributable transaction.

---

## Table of contents

1. [Vision](#1-vision)
2. [Thesis](#2-thesis)
3. [Design principles](#3-design-principles)
4. [Non-goals](#4-non-goals)
5. [Glossary](#5-glossary)
6. [System overview](#6-system-overview)
7. [Layer 0 — Recovery core](#7-layer-0--recovery-core)
8. [Layer 1 — Transaction & state engine](#8-layer-1--transaction--state-engine)
9. [Layer 2 — Capability & principal model](#9-layer-2--capability--principal-model)
10. [Layer 3 — Plugin runtime](#10-layer-3--plugin-runtime)
11. [Layer 4 — Declarative UI](#11-layer-4--declarative-ui)
12. [Layer 5 — Inference broker](#12-layer-5--inference-broker)
13. [Layer 6 — Agent runtime ABI](#13-layer-6--agent-runtime-abi)
14. [Layer 7 — Computer use](#14-layer-7--computer-use)
15. [Update model](#15-update-model)
16. [Multi-user model](#16-multi-user-model)
17. [Security & threat model](#17-security--threat-model)
18. [Attribution & observability](#18-attribution--observability)
19. [Boot, recovery & failure modes](#19-boot-recovery--failure-modes)
20. [Resource management](#20-resource-management)
21. [Roadmap](#21-roadmap)
22. [Repository layout](#22-repository-layout)
23. [Open questions](#23-open-questions)
24. [Risk register](#24-risk-register)
25. [Decision log](#25-decision-log)

---

## 1. Vision

A general-purpose Linux-based operating system where:

- A **native agent** can inspect, modify, repair, and extend the system — including rebuilding parts of its own UI — through first-class, typed interfaces.
- **Every change is a transaction**: reversible, attributable, and logged. The user can always undo, and the system can always boot.
- **Customization is a conversation**: the user asks, the agent edits a declarative profile, the system converges to it.
- **The OS updates itself while running** in the sense that matters: updates are near-zero-downtime, and reboots are invisible (fast, state-preserving, automatic).
- **Local inference is a first-class OS service**, scheduled and isolated like CPU, memory, and GPU — not an app competing for leftovers.
- **Computer use is a native capability**, backed by structured UI trees rather than pixel guessing, so agents can operate apps reliably.
- **Agent runtimes are pluggable**: opencode, codex, any ACP agent through the bridge, a custom runtime, or future ones. The OS owns capabilities and policy; the runtime owns reasoning.

The product promise: *"Tell your computer what you want it to be. It becomes that, safely, and you can always go back."*

---

## 2. Thesis

### 2.1 The gap

Current operating systems treat maintenance as an exception path:

- Updates require reboots and manual recovery when they fail.
- Configuration is imperative, undocumented, and drifts.
- Customization means hand-editing files that the next update may break.
- Automation is scripts — unbounded, unauditable, and unrecoverable.
- Local models run as apps, starved of resources and unintegrated with the system.

### 2.2 The bet

Make **mutation** a first-class OS primitive:

- Every change is a transaction with a snapshot, a diff, a principal, and a revert path.
- The system has a **desired-state profile**; the agent is one writer among several; a reconciliation loop converges.
- Full agent access is safe **because** recovery is guaranteed, not because access is restricted.
- The OS schedules cognition (models, agents) the way it schedules compute.

### 2.3 Why now

- Agent tool use is good enough to drive real system operations.
- Atomic/immutable OS patterns (OSTree, Nix, A/B images) are proven.
- Local inference is viable on consumer hardware.
- No mainstream OS has made the agent the primary maintenance interface.

### 2.4 What we are not doing

We are **not** writing a kernel. We build on Linux. The differentiator is the control plane, not the scheduler or the driver model. Writing a kernel would consume the entire budget and deliver none of the product.

---

## 3. Design principles

1. **Full access, guaranteed undo.** The agent may change anything within its principal's authority. It may never destroy the ability to recover.
2. **Every mutation is a transaction.** No untracked writes to system state.
3. **Declarative over imperative.** The agent edits desired state; the system converges. Imperative commands are compiled into transactions.
4. **Attribution is permanent.** Every change records who, what, when, why, and how to revert.
5. **Capability over ambient authority.** Access is granted explicitly, scoped to a principal, and revocable.
6. **Semantic interfaces before pixels.** Structured APIs and UI trees are primary; computer use is the universal fallback.
7. **Invisible reboot beats impossible live update.** Sell continuity, not "never reboot."
8. **The recovery core is sacred and boring.** Tiny, signed, immutable at runtime.
9. **Local-first where possible.** Cloud models are allowed; core recovery and control paths must work offline.
10. **Plugins all the way up, but not through the floor.** Everything above the recovery core is a plugin.
11. **Trusted input is physical.** Approval dialogs cannot be satisfied by synthetic agent input.
12. **Rust in the trust boundary; shell outside it.** Shell is a tool in a sandbox, not a privilege.
13. **Explainable by default.** Every change carries a plain-language summary a non-technical person can read and act on.
14. **Isolation is not a mode.** Every agent action runs in a sandbox; access modes change scope and approval, never whether isolation exists.

---

## 4. Non-goals

- Writing a kernel, bootloader from zero, or driver stack from zero.
- Replacing Linux's driver ecosystem.
- Supporting every architecture on day one.
- Being a general-purpose server OS in v1.
- Perfect live kernel patching (see [Update model](#15-update-model)).
- A cloud-only OS: offline recovery is mandatory.
- A walled garden: third-party plugins and runtimes are first-class.

---

## 5. Glossary

| Term | Meaning |
|---|---|
| **Principal** | An identity that can act: a user, the system, or a service. Every action is attributed to one. |
| **Capability** | An explicit, scoped grant to perform a class of operations (e.g., `service.restart` on `nginx`). |
| **Transaction** | An atomic, reversible unit of system mutation. |
| **Generation** | A committed system state that can be booted or rolled back to. |
| **Profile** | Declarative desired state for a scope (user, machine, app). |
| **Plugin** | A loadable component providing services, UI, config, or tools, with a manifest. |
| **Recovery core** | The minimal immutable subsystem that boots, verifies, logs, and rolls back. |
| **Inference broker** | The OS service that owns model weights, scheduling, and per-principal isolation. |
| **Runtime** | An agent implementation (opencode, codex, ACP via `acp-bridge`, custom) speaking the agent ABI. |
| **UI tree** | A declarative node tree describing a surface; the unit of agent UI modification. |
| **Trusted input** | Physical human input or a signed approval token; never synthetic events. |
| **Summary** | A derived, non-authoritative plain-language description of a transaction or generation. |
| **Sandbox profile** | The enforcement scope a process runs in (`inspect`, `user-write`, `system-write`, `privileged`). |

---

## 6. System overview

```
┌──────────────────────────────────────────────────────────────────────┐
│  Agent runtimes (plugins)                                            │
│  opencode · codex · ACP (any agent) · custom · future                │
│  ── speak Agent ABI ──▶ tools, policy, state                        │
└───────────────────────────────┬──────────────────────────────────────┘
                                │
┌───────────────────────────────▼──────────────────────────────────────┐
│  Control plane (Rust)                                                │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐ ┌─────────────┐ │
│  │ Capability   │ │ Transaction  │ │ Plugin host  │ │ Inference   │ │
│  │ broker       │ │ engine       │ │              │ │ broker      │ │
│  └──────────────┘ └──────────────┘ └──────────────┘ └─────────────┘ │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐ ┌─────────────┐ │
│  │ State API    │ │ Action log   │ │ UI host      │ │ Input trust │ │
│  └──────────────┘ └──────────────┘ └──────────────┘ └─────────────┘ │
└───────────────────────────────┬──────────────────────────────────────┘
                                │
┌───────────────────────────────▼──────────────────────────────────────┐
│  Userspace services (plugins)                                        │
│  compositor · shell · apps · settings · network · storage · audio    │
└───────────────────────────────┬──────────────────────────────────────┘
                                │
┌───────────────────────────────▼──────────────────────────────────────┐
│  Recovery core (immutable, signed)                                   │
│  boot · initramfs · transaction log · rollback · capability root     │
└───────────────────────────────┬──────────────────────────────────────┘
                                │
┌───────────────────────────────▼──────────────────────────────────────┐
│  Linux kernel + drivers + firmware                                   │
└──────────────────────────────────────────────────────────────────────┘
```

---

## 7. Layer 0 — Recovery core

The recovery core is the only part of the system that cannot be mutated at runtime. It is deliberately small, auditable, and signed.

### 7.1 Responsibilities

- Verify boot integrity (measured boot, TPM where available).
- Provide a minimal environment: init, shell-free recovery, transaction log access.
- Maintain the **generation table** (bootable states).
- Enforce **boot-attempt watchdog** and automatic fallback.
- Host the **root of the capability model** (the broker's trust anchor).
- Provide out-of-band recovery: a signed rescue image and a minimal network path.

### 7.2 Why it cannot be a plugin

If the agent can rewrite the mechanism that undoes the agent, then "full access" means "one bad generation from a brick." The core is the floor. Everything above it — including the compositor, shell, and most of the control plane's own plugins — is mutable.

### 7.3 Boot flow

```
firmware → verified bootloader → signed kernel + initramfs
   → mount immutable system image (read-only)
   → mount writable overlay / state store
   → start core init
   → load generation N
   → start control plane
   → start user session plugins
   → health check
        ├─ pass → mark generation good, increment boot counter reset
        └─ fail → auto-revert to last-good generation, notify
```

### 7.4 Generation table

Each generation records:

- system image hash
- plugin set + versions
- profile snapshot hash
- kernel version
- creation timestamp, creator principal, parent generation
- health status (`candidate`, `good`, `bad`, `pinned`)
- **`summary`** — one plain-language sentence a non-technical person can read
- **`summary_source`** — which runtime/model wrote it, or `human`
- **`summary_verified`** — whether the summary is consistent with the structured diff
- **`impact`** — human-facing effects: reboot required, services restarted, visible UI change

```json
{
  "id": "gen_01J8Z9M2",
  "parent": "gen_01J8Z9K1",
  "summary": "Updated the system and moved your clock to the top-right. No restart needed.",
  "summary_source": "runtime:opencode",
  "summary_verified": true,
  "impact": { "reboot_required": false, "services_restarted": 2, "ui_changed": true },
  "health": "good"
}
```

The `summary` is **derived and non-authoritative**. The authoritative record is the hash-chained diff. A summary that disagrees with the diff is flagged, never trusted over it.

Rollback is `boot(generation_id)`, not a restore operation. It is instant and always available.

---

## 8. Layer 1 — Transaction & state engine

### 8.1 The core invariant

> Every mutation of system state is a transaction: snapshot → apply → verify → commit or revert.

There are no "just run this command" paths that skip the engine for system state. The agent may author any transaction; it may not bypass the ledger.

### 8.2 Transaction lifecycle

```
proposed → checked → snapshotted → applied → health-checked → committed
             │                                  │
             └─ failed → rejected               └─ failed → reverted
```

`checked` is the preflight gate described in [8.8](#88-preflight-the-cargo-check-of-state). It never mutates state. A transaction that fails it is recorded as `rejected` and never applied; a transaction that passes it but fails after real application is recorded as `reverted`.

### 8.3 Transaction schema

```json
{
  "id": "txn_01J8Z9K3",
  "principal": "user:ada",
  "runtime": "opencode",
  "session": "ses_abc123",
  "scope": ["user/ada/desktop", "system/services/network"],
  "intent": "Move clock to top-right and enable dark mode",
  "summary": "Moved your clock to the top-right and switched to dark mode.",
  "summary_detail": "This only changes how your desktop looks. Nothing was restarted, and you can undo it at any time.",
  "summary_source": "runtime:opencode",
  "summary_verified": true,
  "rationale": "User asked for a cleaner layout.",
  "risk": "low",
  "impact": { "reboot_required": false, "services_restarted": 0, "ui_changed": true },
  "operations": [
    { "op": "set", "path": "profile/desktop/clock.position", "value": "top-right" },
    { "op": "set", "path": "profile/theme/mode", "value": "dark" }
  ],
  "snapshot": "snap_01J8Z9K4",
  "revert": "txn_01J8Z9K3.revert",
  "created_at": "2026-11-07T15:47:00Z",
  "status": "committed"
}
```

`intent` is the agent's claim about what it is doing. `summary` is a neutral rendering derived from the diff. `summary_verified` records whether the two agree — see [8.7](#87-explainability-layer).

### 8.4 Snapshots

- **User state**: per-user overlay + content-addressed store; cheap snapshots (btrfs/zfs or overlayfs + manifest).
- **System state**: generation-level; snapshots are image references, not copies.
- **App state**: per-app data versioning with schema migrations.

Snapshots are **copy-on-write by default**. A transaction that touches one config file must not copy a 40 GB home directory.

### 8.5 Reconciliation loop

The engine continuously compares **desired state** (profiles) with **actual state** and converges:

- Drift is detected and reported, never silently "fixed" without a transaction.
- The agent is one writer; package updates and user edits are others.
- Conflicts are surfaced as merge transactions with diffs, not last-writer-wins.

### 8.6 Operation types

| Op | Meaning | Reversible |
|---|---|---|
| `set` | Set a profile path | yes |
| `patch` | Structured edit (JSON merge patch) | yes |
| `install` / `remove` | Package/plugin lifecycle | yes |
| `service` | Start/stop/restart/reconfigure | yes |
| `exec` | Sandboxed command | yes, if declared |
| `migrate` | Schema/data migration | yes, with data snapshot |

`exec` is the escape hatch: it runs in a sandbox with a capability token and must declare its side effects so a revert can be constructed. If it cannot declare them, it requires explicit approval and is flagged `non-revertible`.

### 8.7 Explainability layer

Every transaction and every generation carries a human summary. The goal: a non-technical person can open a timeline, read what happened, and undo it — without ever seeing a diff.

**Two layers, never confused:**

| Layer | Authority | Purpose |
|---|---|---|
| Structured diff + hash chain | Authoritative | Audit, replay, revert |
| Plain-language summary | Derived, attributed | Human understanding |

**Rules:**

1. **Summary never replaces the diff.** It is an annotation on the authoritative record.
2. **Generated from the diff, not from the agent's memory.** The summarizer sees the operations and the declared intent and writes the sentence. This prevents an agent from describing a benign change while doing something else.
3. **Mismatch detection.** The engine cross-checks the summary against the diff (touched resources, risk, impact). A mismatch sets `summary_verified: false` and surfaces a warning.
4. **Stored at commit time, not generated lazily.** The summary reflects what was understood when the change landed, and it is immutable.
5. **Attributed.** `summary_source` records which runtime/model wrote it, or `human`.
6. **Progressive disclosure.** One line → short paragraph → diff. The default is the one line.
7. **Action-oriented language.** "Moved your clock to the top-right. Undo." — not "set `profile/desktop/clock.position`."
8. **Localized and reading-level aware.** Summaries are translated and can be rendered at a simpler level.
9. **Deterministic where possible.** Same diff + same summarizer version → same summary, so replay is reproducible. If a model is used, store the result rather than regenerate it.

**Non-technical UX:**

- A **timeline**, not a log viewer: "Yesterday, 3 changes. All good. [View]"
- Each entry states what changed, whether it needs a reboot, whether it can be undone, and offers a single Undo button.
- Failures read as: "Tried to update the network, it didn't come back, so I put the old version back."

**Example rendering:**

```
diff:    set profile/desktop/clock.position = top-right
         set profile/theme/mode = dark
summary: "Moved your clock to the top-right and switched to dark mode."
impact:  no reboot, no services restarted, visible change
```

### 8.8 Preflight: the `cargo check` of state

Rust's `cargo check` proves code before `cargo build` runs it. The OS needs the same for transactions: prove a change is *stable* before it touches real state. Compilers cannot help here, because a transaction is data, not code. So the engine owns the gate.

**Five gates, in increasing cost:**

| Gate | Cargo analogy | Mutates state? | Answers |
|---|---|---|---|
| `check` | `cargo check` | no | Is this transaction well-formed and allowed? |
| `dry-run` | sandboxed build | no | Does it survive health checks on a scratch copy? |
| `test` | `cargo test` | no | Does it satisfy scenario assertions? |
| `replay` | reproducible build | no | Does re-running the log reproduce the same state? |
| `apply` | `cargo build` | yes | Commit it, with snapshot and revert path. |

**Outcomes are distinct and visible:**

- `rejected` — preflight failed; state was never touched. The log records the attempt and the reason.
- `reverted` — preflight passed but post-apply health failed; the snapshot was restored.
- `failed` — the engine could not complete the mechanics (I/O, log append).

The distinction matters: `rejected` is a bug caught cheaply, `reverted` is a bug caught late, and conflating them hides which gate needs work.

**What `check` runs:**

1. **Path and value validation** — syntax, duplicates, size limits, and registered schemas. Unknown paths are allowed with a `schema.unregistered` warning so plugins can extend the namespace without a central list.
2. **Summary verification** — the derived summary must mention every touched resource (see [8.7](#87-explainability-layer)); a mismatch is an error, not a warning.
3. **Dry run** — operations are replayed onto an in-memory copy of current state, then every registered `HealthCheck` runs against the copy.

**Plugins and plugin builds.** Validation is part of the plugin contract, not an afterthought:

- A plugin ships a **schema** for every path it owns and a **validator** for values a schema cannot express (cross-field rules, host capabilities).
- Installing or upgrading a plugin is itself a transaction. Its preflight includes the plugin's own schema/validator loading, so a broken plugin is rejected before it becomes reachable.
- A plugin build declares the paths it will write in its manifest; the engine can then reject an unknown writer at check time rather than at runtime.
- Validators are pure functions of `(state, operations) -> issues`. They must not mutate, call the network, or depend on wall-clock time, so `check` and `replay` stay deterministic.

**Milestone 0 scope.** Implemented: `check`, `dry-run`, `SchemaValidator`, summary verification, `rejected` status, and the `Validator` trait. Planned: scenario `test` files, log `replay`, and plugin-supplied validators loaded through the plugin ABI.

**State invariants vs. live probes.** Only checks that are pure functions of the state map can run in the dry run. Anything that needs the running system (a service answering, a GPU enumerating, a display coming up) runs after real application and produces `reverted` rather than `rejected`. Mixing the two would make preflight either slow or dishonest.

---

## 9. Layer 2 — Capability & principal model

### 9.1 Principals

- `system` — the OS itself, root authority.
- `user:<id>` — a human user's authority.
- `service:<id>` — a daemon acting on its own behalf.
- `agent:<runtime>:<session>` — an agent acting *for* a principal.

An agent never has independent authority. It borrows the authority of the principal it acts for.

### 9.2 Capabilities

A capability is a scoped, revocable grant:

```json
{
  "capability": "service.restart",
  "resource": "system/service/network",
  "constraints": { "max_per_minute": 4 },
  "granted_by": "user:ada",
  "expires": "2026-11-07T18:00:00Z"
}
```

### 9.3 Two modes

| Mode | Capability scope | Approval policy |
|---|---|---|
| **Full access** | Broadest profile within the principal's authority | Auto-approve; log everything; undo always available |
| **Guarded** | Narrower profile; cross-boundary operations require elevation | Require approval above a risk threshold |

**Important:** sandboxing is *orthogonal* to the mode. Every agent action runs inside a sandbox profile in **both** modes. The modes differ in capability scope and approval policy, not in whether isolation exists. Full access is a wider sandbox, not "no sandbox."

These are **policies**, not separate code paths. The same broker enforces both.

### 9.4 Elevation

Crossing a boundary (user → system, user A → user B) requires:

1. A capability grant from the target principal, or
2. An admin policy that permits it, and
3. A **trusted-input approval** (physical human input or signed token) for high-risk operations.

### 9.5 What "agent can change anything" means precisely

- Within its principal's authority: **anything**, including the UI, shell, compositor plugins, configs, services, packages.
- Across principals: **only with a grant**.
- The recovery core: **never at runtime**; only via a signed, offline-or-explicit update path.

### 9.6 Sandbox profiles

Every agent action is resolved through the chain **capability → sandbox profile → process**. The sandbox is the enforcement mechanism; the capability is the grant; the approval policy is the UX gate.

| Profile | Allows | Typical use |
|---|---|---|
| `inspect` | Read-only: state API, logs, UI tree read | Exploration, diagnosis |
| `user-write` | Write to the acting user's profile, home, UI tree | Customization, app config |
| `system-write` | Write to service configs, packages, system overlays | Maintenance, updates |
| `privileged` | Mediated access to kernel interfaces via the broker | Drivers, low-level repair |

**Mechanisms** (defense in depth):

- User, mount, PID, network, and cgroup namespaces.
- Landlock for filesystem scope; seccomp for syscall scope.
- cgroup v2 for CPU, memory, and IO.
- **No raw root inside any sandbox.** Privileged operations are brokered, not granted as ambient authority.
- The recovery core is unreachable from every profile.

**Pluggable provisioner.** Sandbox creation is an ABI, not a hardcoded implementation:

```rust
trait SandboxProvisioner {
    fn create(&self, spec: SandboxSpec) -> Result<SandboxHandle>;
    fn destroy(&self, handle: SandboxHandle) -> Result<()>;
    fn status(&self, handle: &SandboxHandle) -> Result<SandboxStatus>;
}

struct SandboxSpec {
    principal: PrincipalId,
    profile: SandboxProfile,     // inspect | user-write | system-write | privileged
    capabilities: Vec<Capability>,
    resources: ResourceLimits,
}
```

A provisioner can be a Rust module, or a separate service speaking the ABI over IPC — so an existing provisioning layer written in Go (or any language) can be adopted without porting it into the control plane.

**Escape handling.** A sandbox escape is a security event, not a mode transition. It is logged, quarantines the session, revokes capabilities, and notifies the user.

---

## 10. Layer 3 — Plugin runtime

### 10.1 Everything above the core is a plugin

Compositor, shell, panels, settings, apps, network manager, storage manager, agent runtimes, inference backends, even the control plane's own feature modules — all plugins.

### 10.2 Plugin manifest

```toml
[plugin]
id = "org.tiaki.panel.clock"
name = "Clock Panel"
version = "1.2.0"
api = "1"
kind = "ui"            # ui | service | runtime | backend | tool
entry = "libclock.so"

[capabilities]
requires = ["ui.surface.create", "time.read"]
optional = ["notifications.post"]

[resources]
cpu_weight = 100
memory_max = "64M"

[ui]
tree = "clock.ui.json"
schema = "clock.schema.json"

[config]
schema = "clock.config.schema.json"
migrations = "migrations/"

[update]
channel = "stable"
signature = "ed25519:..."
```

### 10.3 Hot-swap tiers

| Tier | Examples | Swap semantics |
|---|---|---|
| **Content** | widgets, panels, layouts, themes, menus | Live hot-swap, no restart |
| **App UI** | an app's interface tree | Hot-reload from declarative tree |
| **Session** | compositor, shell, desktop environment | Restart with state continuity |
| **Core** | boot, recovery, broker, log | Reboot (invisible, kexec) |

The agent's UI power lives mostly in the **content** and **app UI** tiers, which are live and reversible. Session and core swaps are rarer and follow the same transaction model.

### 10.4 Isolation

- Each plugin runs in its own process inside a **sandbox profile** (see [9.6](#96-sandbox-profiles)) with a capability token.
- Plugins get the narrowest profile that satisfies their manifest.
- Landlock + seccomp + namespaces for filesystem/network/syscall scope; cgroup v2 for resources.
- Sandbox provisioning is pluggable via the provisioner ABI in [9.6](#96-sandbox-profiles).
- Crash containment: a plugin crash never takes down the control plane or the session.

### 10.5 The recursive case

The plugin host itself must be updatable safely. Rule: **the host is versioned and swapped at generation boundaries**, with a compatibility shim for plugins targeting older API versions. Plugin API is semver'd; breaking changes require a migration path.

---

## 11. Layer 4 — Declarative UI

### 11.1 The UI is a tree, not a binary

Every surface is described by a node tree with a schema. The host renders the tree. The agent edits the tree. This is what makes "rebuild parts of the OS" instant, reversible, and safe — no binary patching, no recompilation.

```json
{
  "id": "panel.top",
  "type": "panel",
  "layout": "row",
  "children": [
    { "type": "clock", "props": { "position": "left", "format": "24h" } },
    { "type": "spacer", "props": { "flex": 1 } },
    { "type": "status", "props": { "items": ["network", "battery", "audio"] } }
  ]
}
```

### 11.2 Rules

- Trees are **validated against schema** before render.
- Every node has a stable ID so transactions can target it precisely.
- The agent may create, remove, reorder, restyle, and re-parent nodes.
- Custom widgets are plugins; the tree references them by ID.
- A tree edit is a transaction; revert restores the previous tree.

### 11.3 Why not patch binaries

Binary patching is unverifiable, unrevertible, and breaks on update. Declarative trees give diffing, validation, rollback, and multi-user isolation for free.

### 11.4 Session continuity

When the compositor or shell must restart (session tier), the system preserves:

- open windows and their state (where the app supports it)
- layout tree
- focus and workspace state
- in-flight transactions

Restart should feel like a repaint, not a logout.

---

## 12. Layer 5 — Inference broker

### 12.1 The claim

> The OS schedules cognition the way it schedules CPU, memory, and GPU.

This is the structural advantage no app-level runtime can match: the OS can reserve, guarantee, preempt, and isolate model resources.

### 12.2 Architecture

```
Agent runtimes ──▶ Inference broker ──▶ Backend plugins
                        │                 (custom engine, llama.cpp,
                        │                  vLLM, NPU runtime, cloud)
                        ├─ weight cache (shared, read-only, mlocked)
                        ├─ KV-cache manager (per-principal, zeroed on handoff)
                        ├─ scheduler (continuous batching, preemption)
                        └─ QoS (cgroups, NUMA, GPU/NPU queues, thermal)
```

### 12.3 Scheduling requirements

- **Shared weights**: one resident copy per model, mapped read-only across processes.
- **Continuous batching**: multiple agents/users share forward passes.
- **Preemption**: interactive requests preempt batch generation at token boundaries.
- **Latency first**: optimize time-to-first-token for interactive agents.
- **Fair share**: per-principal quotas; one user's batch job cannot starve another.
- **Thermal/energy aware**: battery vs plugged profiles; throttle before throttle-by-hardware.
- **NUMA/GPU/NPU pinning**: explicit placement, not best-effort.

### 12.4 Per-principal isolation

- KV cache and prompt state are scoped to a principal.
- On handoff between principals, caches are zeroed, not reused.
- Model logs are attributed and access-controlled.
- This is a security requirement, not an optimization.

### 12.5 Pluggable backends

The broker exposes a backend ABI. The custom inference engine, local runtimes, and cloud providers are all plugins. The broker owns policy; the backend owns compute.

### 12.6 Broker API

The ABI is **step-driven**, not `generate()`-to-completion: the broker must be
able to preempt at token boundaries, and a blocking call cannot be preempted.

```rust
trait InferenceBackend {
    fn load(&mut self, spec: &ModelSpec) -> Result<ModelHandle>;
    fn unload(&mut self, handle: ModelHandle) -> Result<()>;

    // Prefill the prompt into a KV cache and create a session.
    fn begin(&mut self, model: ModelHandle, req: &GenerateRequest, cache: CacheHandle)
        -> Result<BeginResult>;

    // Advance the given sessions by one token each. The broker decides the batch.
    fn step(&mut self, sessions: &[SessionHandle]) -> Vec<TokenEvent>;

    // Drop a session. Never called on preemption: preempted sessions keep their cache.
    fn cancel(&mut self, session: SessionHandle);

    // Zero and invalidate a cache. A security operation, not an optimization.
    fn zero_cache(&mut self, cache: CacheHandle) -> Result<()>;
}

struct GenerateRequest {
    principal: String,
    session: String,
    model: String,
    prompt: String,
    priority: Priority,      // Interactive | Normal | Batch
    max_tokens: usize,
    deadline: Option<u64>,
}
```

The broker owns admission, fairness, batching, preemption, cache ownership, and
quotas. The backend owns compute. A backend that only exposes a blocking
`generate` is wrapped in a session; preemption there is cooperative.

---

## 13. Layer 6 — Agent runtime ABI

### 13.1 Separation of concerns

- **OS** = capability provider + policy enforcer + state owner.
- **Runtime** = reasoning + planning + tool selection.

Runtimes are plugins. opencode, codex, and custom runtimes all speak the same ABI.

### 13.2 Tool protocol

The OS exposes typed tools:

```json
{
  "name": "service.restart",
  "description": "Restart a system service.",
  "input_schema": {
    "type": "object",
    "properties": {
      "service": { "type": "string" }
    },
    "required": ["service"]
  },
  "risk": "medium",
  "reversible": true
}
```

Calls return structured results, not scraped text:

```json
{
  "ok": true,
  "service": "network",
  "previous_state": "running",
  "new_state": "running",
  "transaction": "txn_01J8Z9K3"
}
```

### 13.3 Shell as a tool

- Bash is available as a **sandboxed tool** with a capability token.
- It runs in a namespace with declared filesystem/network scope.
- Side effects must be declared to be revertible; otherwise the transaction is marked `non-revertible` and requires approval.
- Shell output is captured and attributed.

This preserves "the agent can do anything" while keeping every action on the ledger.

### 13.4 Runtime discovery

Runtime descriptors (`RuntimeSpec`: id, transport, tools) survive as inventory;
`agentd agent default <id>` selects the entry point's agent (opencode today).
Sessions are opencode sessions, driven through `opencode run` — see
[docs/specs/agent.md](docs/specs/agent.md). The old `tiaki-abi/1` envelope is
deleted; nothing parses a wire protocol anymore.

---

## 14. Layer 7 — Computer use

### 14.1 The from-scratch advantage

Because we own the stack, **every native app can be required to expose a structured UI tree**. Computer use becomes:

1. **Structured tree first** — semantic nodes, roles, labels, actions.
2. **Vision fallback** — pixels + OCR for web and foreign apps.
3. **Input injection** at the compositor level with trust marking.

This is dramatically more reliable and cheaper than screenshot-guessing, and it is something macOS and Windows cannot retrofit.

### 14.2 Action model

```
observe → plan → act → verify
```

- **Observe**: fetch UI tree, accessibility data, app state.
- **Plan**: choose a target node and action.
- **Act**: inject input or call a structured action.
- **Verify**: re-read state, confirm the intended change.

### 14.3 Trusted input (non-negotiable)

Approval dialogs accept only:

- physical human input, or
- a signed approval token from a trusted device.

**Never** synthetic events from an agent. Otherwise an agent clicks "Allow" on its own prompt and the trust model collapses. The compositor marks every event `trusted: true|false`; the approval service rejects untrusted events.

### 14.4 Reliability tiers

| Tier | Mechanism | Use |
|---|---|---|
| 1 | Typed app API | Preferred |
| 2 | Structured UI tree | Native apps |
| 3 | Accessibility protocol (AT-SPI-like) | Legacy/native apps |
| 4 | Vision + input injection | Web, foreign apps |

The runtime should always prefer the lowest tier that works.

---

## 15. Update model

### 15.1 The honest framing

- **Userspace live update**: solved with an atomic store + restart-on-swap + socket activation.
- **Kernel live update**: function-level live patching only (no data-structure changes, no init code). Anything more is a reboot.
- **Firmware/drivers**: requires device quiesce/reset; not live.
- **Therefore**: the product promise is **invisible reboot**, not "never reboot."

### 15.2 Invisible reboot

- `kexec` into the new image: ~1–3 seconds, skips firmware/bootloader.
- Session restore: windows, layout, focus, in-flight work.
- Health check before commit; auto-revert on failure.
- The user perceives continuity, not downtime.

### 15.3 Update pipeline

```
new generation → stage → verify signature → boot candidate
   → health checks → commit or auto-revert
```

### 15.4 App updates

- Sandboxed, atomic, per-app data versioning.
- Stateful apps: session handoff, state serialization, or restart-at-natural-break.
- Rollback includes data migrations.

### 15.5 Plugin updates

- Same transaction model.
- API compatibility shims for older plugin versions.
- The plugin host updates at generation boundaries.

---

## 16. Multi-user model

### 16.1 Authority is per principal

- Each user has their own profile, plugins, UI trees, and agent sessions.
- A user's agent can rebuild that user's entire environment.
- Cross-user actions require a grant or admin policy.

### 16.2 Isolation

- Per-user namespaces for files, processes, and model state.
- Per-user cgroup quotas for compute and inference.
- Per-user plugin sets; system plugins are shared and read-only.
- Per-user action logs; system log is append-only and admin-readable.

### 16.3 Agent-to-agent boundary

- Agents acting for different principals cannot directly instruct each other.
- Any inter-agent message crosses the capability broker and is attributed.
- This prevents user A's agent from inducing user B's agent to act.

### 16.4 Shared inference

- Fair-share scheduling across users.
- Per-principal KV-cache isolation and zeroing on handoff.
- Quotas and priority classes.

---

## 17. Security & threat model

### 17.1 Assets

- User data and credentials.
- System integrity and boot chain.
- Model weights and inference state.
- The action log and generation table.
- The capability root.

### 17.2 Adversaries

- Malicious content (prompt injection from web, email, files).
- Malicious plugins.
- A compromised agent runtime.
- A malicious local user.
- Supply-chain attacks on packages/plugins/models.
- The agent itself making a catastrophic mistake.

### 17.3 Controls

| Threat | Control |
|---|---|
| Prompt injection → privileged action | Capability broker, risk thresholds, trusted-input approval, sandboxing |
| Agent self-approval | Trusted input marking; synthetic events rejected |
| Malicious plugin | Manifest capabilities, sandbox, signing, review |
| Supply chain | Signed packages/plugins/models, reproducible builds, provenance |
| Data exfiltration | Per-plugin network scope, egress policy, audit |
| Cross-user leakage | Namespaces, per-principal model state, quotas |
| Bricked system | Recovery core, generation table, boot watchdog, auto-revert |
| Catastrophic agent action | Transaction snapshots, dry-run, diff, approval, undo |
| Misleading or wrong summary | Summaries derived from the diff, mismatch detection, diff always available |
| Sandbox escape | Least-privilege profiles, no raw root, escape detection, session quarantine |
| Model outage | Offline recovery path; local fallback for core operations |

### 17.4 The trust chain

```
firmware → bootloader → kernel → recovery core → control plane → plugins → runtimes
```

Each link is verified. The recovery core is the root of runtime trust.

### 17.5 Prompt injection is the central risk

An agent with full access that reads untrusted content is one injection away from disaster **if** actions are unbounded and unrevertible. The mitigations are architectural, not prompt-based:

- Untrusted content is tagged; actions derived from it are risk-elevated.
- High-risk actions require trusted input.
- Every action is reversible by default.
- The log makes abuse visible.

---

## 18. Attribution & observability

### 18.1 Every action records

- principal, runtime, session, plugin
- transaction ID, parent generation
- intent and rationale (agent-provided)
- plain-language summary, impact, and `summary_verified` (see [8.7](#87-explainability-layer))
- operations and diffs
- timestamp, duration, result
- revert reference

### 18.2 The action log

- Append-only, hash-chained (tamper-evident).
- Queryable by principal, time, resource, transaction.
- Exportable for audit.
- The agent can read it; it cannot rewrite it.

### 18.3 State API

The agent needs **semantic introspection**, not shell parsing:

```
services.list() -> [{name, state, health, restart_count}]
packages.query(...) -> [...]
resources.snapshot() -> {cpu, memory, disk, gpu, model_queues}
logs.query(filter) -> stream
generations.list() -> [...]
```

### 18.4 Observability for humans

- A timeline of changes with diffs.
- "What did the agent do while I was away?"
- One-click undo per transaction or per session.
- Health dashboard for services, plugins, models.

---

## 19. Boot, recovery & failure modes

### 19.1 Boot watchdog

- Count boot attempts per generation.
- After N failed boots, auto-revert to last-good.
- Notify the user with the failure reason.

### 19.2 Failure modes and responses

| Failure | Response |
|---|---|
| Agent breaks a config | Transaction health check fails → auto-revert |
| Agent breaks boot | Watchdog reverts to last-good generation |
| Update fails mid-write | Atomic staging; old generation untouched |
| Power loss during transaction | Journal replay; incomplete transaction reverted |
| Disk full during update | Pre-flight space check; abort before commit |
| Model provider outage | Local fallback for core ops; queue non-urgent work |
| Agent loop / runaway | Budgets, step limits, kill switch |
| Plugin crash | Containment; restart with backoff |
| Compositor crash | Session restart with state restore |
| Malicious plugin | Sandbox, capability denial, quarantine |

### 19.3 Recovery paths

1. **Soft**: revert transaction.
2. **Medium**: reboot to last-good generation.
3. **Hard**: boot rescue image, repair, restore from snapshot.
4. **Offline**: signed recovery media.

Every path must work without network and without a model.

---

## 20. Resource management

### 20.1 Classes

- Interactive (UI, input, audio)
- Agent (reasoning loops)
- Inference (model compute)
- Background (updates, indexing)
- System (core services)

### 20.2 Mechanisms

- cgroup v2 for CPU, memory, IO.
- Per-class weights and limits.
- Preemption at safe points.
- Admission control: refuse to start work that cannot be scheduled.
- Thermal and energy awareness.
- NUMA and accelerator placement.

### 20.3 The model broker as a scheduler client

The broker participates in the same resource model as everything else, but with reserved capacity so interactive inference is never starved.

---

## 21. Roadmap

### Milestone 0 — Ledger (proves the thesis)

- Linux base image.
- Transaction engine with snapshot/rollback.
- Immutable action log.
- Preflight: `check` + `dry-run`, with `rejected` vs `reverted` outcomes.
- Plain-language `summary` + `impact` on every transaction.
- One typed tool (`config.set`).
- CLI to check, propose, apply, revert.

**Exit:** apply a change, read the one-line summary, break it, see it rejected before mutation or reverted after, inspect the log.

**Status:** walking skeleton complete. `cargo test --workspace` covers SHA-256, the hash chain, summaries, impact, preflight rejection, apply/revert, and post-apply auto-revert; the `agentd` demo runs the full loop. The Linux base image and the formal `config.set` tool wrapper remain.

### Milestone 1 — Authority

- Principal model.
- Capability broker.
- Sandbox profile resolver (`inspect` → `user-write` → `system-write` → `privileged`).
- Sandboxed shell tool.
- Guarded vs full-access policies — both sandboxed.

**Exit:** user A's agent cannot touch user B; full-access mode works, is sandboxed, and is logged.

**Status:** authority layer implemented. Principals parse `system`, `user:<id>`, `service:<id>`, and `agent:<runtime>:<session>@<principal>`; agents borrow the effective principal's authority. `Policy` holds scoped grants `(action, resource pattern)` with optional `requires_approval`, `max_per_minute`, expiry, and `granted_by`, supports `*`/`**` segment globs, persists to `$TIAKI_HOME/grants`, and is consulted by the transaction engine before every mutation. Guarded mode turns approval-required grants into `pending-approval`; full mode auto-approves them but is still capability-scoped. Recovery paths (`core/`, `boot/`, `recovery/`, `system/recovery/`) are hard-denied in both modes, even for `system`. Preflight is pure; rate-limit quota is consumed only immediately before mutation. `agentic-sandbox` resolves a sandbox profile for every action and plans specs through the `SandboxProvisioner` ABI (`SimulatedProvisioner` on hosts without a Linux provisioner). The `agentd` CLI exposes `--principal`, `--mode`, `check`, `apply --dry-run`, `revert`, `grants`, `grant`, `revoke`, `sandbox`, `log`, `state`, and `verify`. Integration tests cover cross-user denial, agent borrowing, guarded vs full, cross-principal revert, and recovery-core immutability.

**Remaining:** real Linux execution through a provisioner (Rust module or the Go service), the trusted-input approval UI for `pending-approval`, per-principal log files, and wiring the sandboxed shell tool into the transaction path.

### Milestone 2 — Plugin host

- Manifest, loading, isolation, lifecycle.
- Hot-swap content-tier plugin.
- Config schemas and migrations.

**Exit:** install, update, and remove a plugin safely.

**Status:** plugin host implemented. A strict TOML-subset parser and semantic validator handle `[plugin]`, `[capabilities]`, `[resources]`, `[ui]`, `[config]`, and `[update]`; path traversal and incompatible API majors are rejected before install. Signatures go through a `SignatureVerifier` ABI with a real `sha256:` content-digest verifier and a pluggable slot for ed25519; unverified plugins require explicit `--allow-unverified`. Installs stage immutable `versions/<version>` copies under `$TIAKI_HOME/plugins/<id>` with persisted `active`, `previous`, `state`, and `config.json`. Lifecycle is install → load → start → stop/unload, with illegal transitions refused. Content-tier plugins hot-swap live; session-tier updates report `restart-required`; rollback validates config against the target schema. Config uses a JSON-Schema subset (`type`, `enum`, `required`, `default`, `additionalProperties`) and chained JSON migrations. The host maps manifests to `SandboxProfile` (widened by capabilities) and produces the `SandboxSpec`; it never executes plugin code. `agentd plugin` exposes install/update/remove/load/unload/start/stop/swap/rollback/config/spec/sign/list, requires `plugin.manage` on `system/plugins/<id>`, and records every action on the hash chain.

**Remaining:** real plugin execution through the provisioner, ed25519 verification, the process supervisor and crash containment, and plugin-supplied UI/config validators loaded through the ABI.

### Milestone 3 — Declarative UI

- UI tree schema and renderer.
- Live tree editing by agent.
- Session continuity on restart.

**Exit:** agent rebuilds a panel live; revert restores it.

**Status:** implemented on a host-side renderer. `ui/tree` defines the node schema, a seven-widget catalog (`workspace`, `panel`, `clock`, `spacer`, `status`, `text`, `button`), stable IDs, typed props with enums, depth/size limits, and duplicate/cycle rejection. Agent edits use a JSON op language (`insert`, `remove`, `move`, `set_prop`, `remove_prop`); the daemon applies them to a copy, validates, and stores the result as one `Operation::Set` on `<scope>/ui/tree`. A `CompositeValidator` (`SchemaValidator` + `UiTreeValidator` + `KeymapValidator`) runs during preflight, so invalid trees are `rejected` before mutation; `revert` restores the previous tree through the snapshot path. `ui/widgets` renders a self-contained HTML page with a keyboard command palette, focus movement, live polling of `/state`, and `/action` dispatch. Keyboard-first defaults live in a transactional keymap resource (`<scope>/input/keymap`) with named actions (`focus.left`, `workspace.3`, `theme.toggle`, `clock.toggle_format`); mutating actions run through `Engine::apply` and are logged, and navigation actions stay client-side. The `agentd ui` CLI exposes `get`, `render`, `serve`, `set`, `edit`, `reset`, `demo`, and `keymap`. See [docs/specs/ui-tree.md](docs/specs/ui-tree.md) and [ADR 0006](docs/adr/0006-keyboard-first-desktop.md).

**Remaining:** the real Wayland compositor and widget toolkit (Sway + GTK3 is the current native backend), session continuity across compositor restarts (open windows, focus, workspaces), and the full widget set. The host-side renderer is the contract the compositor implements.

**Visual desktop:** the current desktop is a Sway session with our own tree-driven shell (`ui/shell`): a macOS-like menu bar, floating dock, home-screen launcher with dot-matrix clock and app grid, real wallpapers, custom window titlebars with traffic lights, dropdown menus, and Control Center, all rendered from `desktop/desktop.json` via `/tree.json`. Tokyo Night, Inter, generated wallpaper, tiling gaps, dark GTK, foot/mousepad/btop/Thunar/Epiphany. `ui/native` is a compact GTK3 tree renderer. Sway is an existing compositor, so blur/shadows/rounded window corners/animations and the bare-metal image remain the open items; the shell, design tokens, tree, and control plane are ours.

### Milestone 4 — Inference broker

- Shared weights, batching, preemption.
- Per-principal isolation.
- Backend ABI + first backend.

**Exit:** two agents share one model; interactive request preempts batch.

**Status:** implemented with a deterministic simulated backend. `control/inference` defines a step-driven `InferenceBackend` ABI (`load`, `begin`, `step`, `cancel`, `zero_cache`) so preemption happens at token boundaries rather than inside a blocking call. A `ModelSpec` registry validates identity, architecture, context, quantization, and a `sha256:` weight digest; model load/unload are transactions on `system/inference/models/<id>` validated by `ModelRegistryValidator`. The broker batches sessions from different principals into one `step`, preempts `Batch` and `Normal` work when an `Interactive` request arrives, and resumes it with its KV session intact. `CacheTable` allocates a fresh handle per `(principal, session, model)`, never reuses a handle across principals, and zeroes it on completion or handoff through the backend ABI. `Scheduler` enforces priority classes and per-principal fair-share selection; `UsageBook` enforces concurrent-request and token-window quotas. New actions `inference.generate` (users, `model/**`) and `inference.manage` (system/service, `system/inference/**`) integrate with the capability broker; generations are recorded on the ledger with principal, model, priority, token count, and preemptions. The `agentd inference` CLI exposes `models`, `load`, `unload`, `ask`, `bench`, `stats`, and `cache`; `bench` is the exit demo: one model, two principals batched, an interactive request preempting a batch, the batch resuming, and three caches zeroed. See [docs/specs/inference.md](docs/specs/inference.md).

**Remaining:** a real backend adapter (subprocess JSON-lines or HTTP), actual weight mmap/mlock and digest verification of bytes, GPU/NPU placement, thermal profiles, and cross-principal cache zeroing on real hardware (the ABI and tests are in place).

### Milestone 5 — Runtime ABI + computer use

- Runtime protocol.
- Structured UI tree for native apps.
- Trusted input marking.

**Exit:** an external runtime drives an app via structured tree; approval cannot be spoofed.

**Status:** replaced. The private `tiaki-abi/1` wire protocol, its host loop, and the ACP bridge are deleted. `agentd agent run|notify` drives real opencode through `opencode run` against our own `opencode serve` (single entry point, [docs/specs/agent.md](docs/specs/agent.md)); the OS exposes its capabilities as MCP tools (`agentd mcp serve`) registered once as the global `tiaki` server. Every call is a ledger transaction, full access by default, reversible and attributable. Proven live: a notification drove opencode to run `df -h`, write a workspace file, and read it back. Kept from `runtime/abi`: the UI tree, the app backend, approval/trusted input, and the runtime descriptors. History: `runtime/abi` defined `tiaki-abi/1` as JSON lines over stdio or a Unix socket (`hello`, `observe`, `act`, `approve`, `bye` / `welcome`, `tree`, `acted`, `approval`, `decision`, `error`), `RuntimeSpec` discovery, and `ToolSpec` with risk and reversibility. `ProcessRuntime` speaks the protocol to a child process; the in-process `ScriptedRuntime` drives the same messages deterministically. Apps expose an `AppTree` of semantic nodes with stable ids, roles, labels, values, enabled state, bounds, declared actions, and risk (512-node/depth-16 limits). `ComputerSession` implements observe → act → verify: it validates the node, disabled state, declared action, and value; low/medium-risk actions apply immediately, high-risk actions open an approval challenge instead. `ApprovalService` accepts only `physical` or HMAC-signed `trusted-device` input, rejects `synthetic` events, enforces expiry and single use, and never consumes a challenge on a rejected attempt. `ComputerHost` is the only component that applies pending approved actions; a runtime cannot approve a challenge it did not receive or spend one twice. `runtime/opencode` and `runtime/codex` provide descriptors and typed tool sets. `runtime/acp` is a bridge that makes any Agent Client Protocol agent a runtime: it speaks `tiaki-abi/1` to the host and ACP to the child, denies every ACP permission request, declares no client capabilities, and lands its acts on the same ledger; proven live against opencode 2.2.5 with `agentd runtime default acp` and a bare `agentd computer run`. See [docs/specs/acp-bridge.md](docs/specs/acp-bridge.md). The OS now exposes its capabilities as MCP tools (`agentd mcp serve`: `ui.observe`, `ui.act`, `fs.*`, `terminal.exec`) and hands them to an ACP agent in `session/new`; every call is a ledger transaction, full access by default, reversible and attributable. Proven live: real opencode observed the tree, toggled a node, and wrote a file entirely through MCP. See [docs/specs/mcp.md](docs/specs/mcp.md). `computer.use` is scoped to `app/<id>` for every user and `system`; agents borrow the effective principal. The `agentd` CLI exposes `runtime list|spec|protocol` and `computer apps|tree|demo|protocol`; `computer demo` observes the checkout app, toggles gift wrap, requests approval for “Place order”, fails a synthetic approval, succeeds with physical input, applies the order, and verifies it. Every step is recorded on the ledger, including the rejected approval. See [docs/specs/runtime-abi.md](docs/specs/runtime-abi.md) and [ADR 0008](docs/adr/0008-trusted-input-approval.md).

**Remaining:** real opencode/codex process adapters, vision/OCR and AT-SPI fallbacks, compositor-level input injection and trust marking, and trusted-device key provisioning.

### Milestone 6 — Invisible reboot

- kexec fast reboot.
- Session restore.
- Generation health checks.

**Exit:** update + reboot feels like a repaint.

**Status:** implemented with a simulated kexec backend. `core/rollback` owns the generation table (`active`, `last_good`, `candidate`, boot id, per-generation attempts), stages immutable images with a `sha256:` directory digest, and refuses to commit a candidate that has not passed a health check. The watchdog rolls back to last-good after `max_attempts` failed boots and requests rescue when no last-good exists. `core/boot` re-hashes the image before boot, rejects tampered images and symlinks, and exposes a `Kexec` ABI with `SimulatedKexec` (configurable downtime) and `LinuxKexec` (`kexec --load`/`-e`, gated behind `TIAKI_ALLOW_KEXEC=1`). `BootHealth` runs probes for the session snapshot, UI tree, keymap, and inference registry; any failure auto-reverts. `core/init` defines a session snapshot (UI tree, keymap, theme, focus, workspace, app values, pending transactions) that is captured before the update and re-applied as ordinary transactions after boot, with a value-by-value verification. The `agentd` CLI exposes `session capture|show|restore|clear` and `generation list|status|stage|update|boot|commit|rollback`; `generation update <dir>` runs capture → stage → candidate → boot → health → commit/rollback → restore, and `--fail` injects a failing probe to demonstrate auto-revert. `generation.manage` is scoped to `system/generations/**` and held by `system` and `service:update`. Every stage, boot, commit, and rollback is recorded on the ledger. See [docs/specs/generations.md](docs/specs/generations.md) and [ADR 0009](docs/adr/0009-invisible-reboot.md).

**Remaining:** the real image build and signing pipeline, secure/verified boot and bootloader integration, hardware validation of kexec across the target matrix, and true app-process handoff beyond the snapshot.

---

## 22. Repository layout

```
tiaki/
├── TIAKI.md              # this document
├── core/                      # recovery core (Rust, minimal)
│   ├── boot/
│   ├── init/
│   ├── json/
│   ├── log/
│   ├── toml/
│   └── rollback/
├── control/                   # control plane (Rust)│   ├── agentd/                # daemon
│   ├── capability/            # broker
│   ├── transaction/           # engine
│   ├── plugin-host/
│   ├── state-api/
│   ├── explain/               # summaries + mismatch detection
│   ├── sandbox/               # profile resolver + provisioner ABI
│   └── inference/             # broker
├── ui/                        # compositor + UI host + toolkit
│   ├── compositor/
│   ├── tree/
│   └── widgets/
├── runtime/                   # agent ABI + adapters
│   ├── abi/
│   ├── opencode/
│   └── codex/
├── plugins/                   # first-party plugins
├── models/                    # model specs + registry examples
├── image/                     # image build, signing, updates
├── docs/                      # specs, ADRs
└── tests/                     # integration, chaos, replay
```

---

## 23. Open questions

1. **Kernel boundary**: how much of the kernel interface do we expose to the agent directly (sysfs, procfs) vs. mediated by typed tools?
2. **Compositor strategy**: build on smithay, or a custom compositor? How much Wayland compatibility is required for v1?
3. **Package format**: adopt an existing format (Flatpak-like) or design one? How do plugins and apps share a model?
4. **Profile language**: JSON merge patch, a typed DSL, or Nix-like expressions? How do users share profiles?
5. **Generation granularity**: per-transaction or batched? How often do we snapshot?
6. **Recovery core language**: pure Rust, or Rust + a tiny verified bootloader?
7. **Model state isolation**: how do we guarantee KV-cache zeroing across principals without killing performance?
8. **Agent-to-agent protocol**: do we need one, or is brokered messaging enough?
9. **Update channels**: per-plugin, per-generation, or both?
10. **Testing a self-modifying OS**: what is the minimum viable replay/chaos harness before v1?
11. **Distribution**: how do users install this, and how do they migrate from existing Linux?
12. **Licensing**: Rust components (MIT/Apache) vs. any GPL glue; what is the policy?
13. **Summary trust**: run a separate trusted summarizer, or accept the acting runtime's summary with cross-checking? How strict is mismatch detection?
14. **Full-access sandbox width**: how broad can the `system-write` profile get before it is effectively root, and where is the line?
15. **Provisioner protocol**: what is the IPC contract and latency budget for an external sandbox provisioner?

---

## 24. Risk register

| Risk | Severity | Mitigation |
|---|---|---|
| Recovery core is a single point of failure | Critical | Build first; verified boot; signed rescue; watchdog |
| Prompt injection leads to destructive action | Critical | Capabilities, risk thresholds, trusted input, reversibility |
| Scope creep into kernel/drivers | High | Explicit non-goal; build on Linux |
| Hardware support gaps | High | Target a narrow hardware matrix first |
| Model dependency / outage | High | Offline recovery; local fallback |
| Cross-user agent leakage | High | Namespaces, per-principal model state, broker |
| Plugin ecosystem abuse | High | Signing, capabilities, review, sandbox |
| Testing complexity of self-modifying system | High | Replay log, golden images, chaos tests |
| Performance overhead of transactions | Medium | CoW snapshots; batching; async commit |
| Plugin API churn | Medium | Semver + compatibility shims |
| Team size vs. ambition | High | Milestone gating; walking skeleton first |

---

## 25. Decision log

| Date | Decision | Rationale |
|---|---|---|
| 2026-11-07 | Build on Linux kernel, not from scratch | Kernel is not the differentiator; drivers are a decade-long wall |
| 2026-11-07 | From-scratch userspace + control plane | Own the agent layer and the UI |
| 2026-11-07 | Full agent access, guaranteed undo | Access is not the risk; unrecoverable mutation is |
| 2026-11-07 | Every mutation is a transaction | Enables undo, attribution, and safe full access |
| 2026-11-07 | Recovery core immutable at runtime | Otherwise full access is one bad generation from a brick |
| 2026-11-07 | Declarative UI trees, not binary patching | Diffable, reversible, update-safe |
| 2026-11-07 | Multi-user via principal-scoped authority | Resolves "agent can change anything" with multi-tenancy |
| 2026-11-07 | Invisible reboot, not "never reboot" | Live kernel update is not generally possible |
| 2026-11-07 | Inference broker as first-class OS service | Structural advantage over app-level runtimes |
| 2026-11-07 | Pluggable agent runtimes via ABI | OS owns capabilities; runtime owns reasoning |
| 2026-11-07 | Computer use: structured tree first, vision fallback | From-scratch stack can require native UI trees |
| 2026-11-07 | Trusted input for approvals | Prevents agent self-approval |
| 2026-11-07 | Plain-language summary on every transaction and generation | Non-technical users must be able to understand and undo changes |
| 2026-11-07 | Summaries are derived and non-authoritative | The diff is the record; a model must never rewrite history |
| 2026-11-07 | Sandboxing is orthogonal to access mode | Full access is a wider sandbox, not no sandbox |
| 2026-11-07 | Sandbox provisioner is a pluggable ABI | Adopt an existing provisioning layer without porting it to Rust |
| 2026-11-07 | Preflight before mutation, with `rejected` distinct from `reverted` | Catch bugs before state changes; keep late failures visible |
| 2026-11-07 | Debian minimal as the base substrate, from-scratch control plane on top | glibc for GPU/prebuilt runtimes, reproducible rootfs, no competing update authority |
| 2026-11-07 | Keyboard-first desktop with named actions, inspired by Omarchy | Fast, legible, and gives the agent a structured action catalog instead of synthesized keystrokes |
| 2026-11-07 | Step-driven inference ABI with broker-owned batching and caches | Preemption, fairness, and zero-on-handoff must be broker policy, not backend behavior |
| 2026-11-07 | Trusted input gates high-risk computer use | An agent that can synthesize approval can approve itself, collapsing the human-in-the-loop guarantee |
| 2026-11-07 | Invisible reboot with generation rollback | Kernel/driver changes cannot be fully live; a kexec-sized pause plus automatic fallback is the honest promise |

---

*This document is the source of truth for the project's direction. Change it with a transaction, like everything else.*
