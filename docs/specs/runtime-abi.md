# Spec: Runtime ABI & computer use

> **Removed.** The private `tiaki-abi/1` wire protocol is gone: no
> `RuntimeMessage`, no process envelope, no host loop, no scripted drivers,
> no bridge. Agents are driven through their own server APIs — see
> [agent.md](agent.md) for the single entry point — and the OS exposes its
> capabilities as MCP tools ([mcp.md](mcp.md)). Kept from this crate: the UI
> tree, the app backend, approval/trusted input, and the runtime descriptors.
> The history below describes what used to be here.

Canonical definition: [TIAKI.md §13–14](../../TIAKI.md#13-layer-6--agent-runtime-abi).
Implementation: `runtime/abi`, `runtime/opencode`, `runtime/codex`, `runtime/acp`.

## Separation of concerns

- **OS** = capability provider, policy enforcer, state owner.
- **Runtime** = reasoning, planning, tool selection.

Runtimes are external processes. The OS never trusts them with approval; it
validates every action and every approval event.

## Runtime discovery

`RuntimeSpec` is a TOML `[runtime]` table:

```toml
[runtime]
id = "opencode"
protocol = "tiaki-abi/1"
transport = "unix:///run/tiaki/runtime/opencode.sock"
tools = ["system", "files", "ui", "computer-use"]
```

Discovery scans `$TIAKI_HOME/runtimes/*.toml` for additional runtimes.
`opencode`, `codex`, and `acp` are built in. Validation rejects an unknown
protocol, an empty transport, duplicate tools, or a missing id.

`acp` is not an agent itself: it is the bridge (`runtime/acp`) that drives any
Agent Client Protocol agent as a runtime. See
[acp-bridge.md](acp-bridge.md).

## Typed tools

```rust
struct ToolSpec {
    name: String,
    description: String,
    input_schema: Json,
    risk: Risk,        // Low | Medium | High
    reversible: bool,
}
```

Tools return structured results. `computer.act` is high-risk and
non-reversible; `system.state.set` is medium-risk and reversible.

## Runtime protocol

JSON lines over stdio or a Unix socket. `tiaki-abi/1`:

| Direction | Messages |
|---|---|
| runtime → host | `hello`, `observe`, `act`, `approve`, `bye` |
| host → runtime | `welcome`, `tree`, `acted`, `approval`, `decision`, `error` |

`ProcessRuntime` spawns a child process and speaks the protocol over its
stdin/stdout. The in-process `ScriptedRuntime` drives the same messages for
deterministic demos and tests.

## Structured app tree

Every native app exposes `AppTree`:

```json
{
  "app": "checkout",
  "title": "Checkout",
  "root": {
    "id": "checkout",
    "role": "window",
    "label": "Checkout",
    "enabled": true,
    "focused": false,
    "risk": "low",
    "actions": [],
    "children": [
      { "id": "gift", "role": "checkbox", "label": "Gift wrap",
        "value": "off", "actions": ["toggle"], "risk": "low" },
      { "id": "place", "role": "button", "label": "Place order",
        "value": "idle", "actions": ["invoke"], "risk": "high" }
    ]
  }
}
```

Roles: `window`, `dialog`, `button`, `text-field`, `label`, `checkbox`,
`toggle`, `list`, `list-item`, `menu`, `menu-item`, `progress`, `image`.
Actions: `invoke`, `set-text`, `toggle`, `select`, `focus`, `scroll`.

Nodes have stable ids, semantic labels, current values, enabled/focused state,
optional bounds (for the vision fallback and hit testing), declared actions, and
a risk level. Limits: 512 nodes, depth 16; ids are unique; container roles are
the only ones allowed children.

## Computer-use session

```
observe → plan → act → verify
```

- `observe` returns the tree.
- The runtime picks a node and an action.
- `act` validates the node, enabled state, declared action, and value, then:
  - **low/medium risk** → applies immediately;
  - **high risk** → returns an `Approval` challenge instead of applying.
- `verify` re-reads the tree and compares the node's value.

The session applies approved actions; the runtime never gets an API to bypass
approval.

## Trusted input (non-negotiable)

`InputEvent` carries `trust`: `physical`, `trusted-device`, or `synthetic`.
Only the first two are accepted by the approval service.

- The compositor marks every event. An agent cannot mint a `physical` event.
- A trusted device signs the decision with HMAC-SHA256 over
  `challenge|nonce|payload` using a device key. The token is
  `hmac-sha256:<hex>`.
- Challenges expire and are single-use. A rejected attempt (synthetic input,
  bad token) does **not** consume the challenge, so a legitimate human can
  still decide.
- The payload is `allow` or `deny`; both are decisions.

## Approval lifecycle

1. A high-risk `act` creates a challenge with an id and nonce.
2. The runtime receives `approval { challenge, request }`.
3. The runtime may send `approve { challenge, event, token }`.
4. The host calls `ApprovalService::decide`:
   - synthetic → rejected, challenge remains open;
   - physical → accepted;
   - trusted-device → token verified;
   - expired/replayed → rejected.
5. On approval the host applies the pending action and returns
   `decision { approved: true, reason }`. On denial it drops the pending
   action.

The runtime cannot approve a challenge it did not receive (unknown id) or spend
one twice.

## Authority

| Action | Scope | Default holders |
|---|---|---|
| `computer.use` | `app/<id>` | every user, `system` |

Agents borrow the effective principal. A service has no `computer.use` by
default. Approval is separate from capability: a grant lets the runtime drive
the app; trusted input decides high-risk actions.

## Ledger

Each computer-use event is recorded with the principal, app, node, and result:

- applied low-risk action → `committed`
- rejected synthetic approval → `rejected`
- trusted approval → `committed`
- approved high-risk action → `committed` with the action result

Prompt content and input payloads are not stored.

## CLI

```sh
agentd runtime list
agentd runtime spec opencode
agentd runtime default            # which runtime `computer run` starts
agentd runtime default opencode
agentd runtime protocol
agentd computer apps
agentd computer tree checkout
agentd computer demo
agentd computer run -- tests/replay/checkout-plan.py
agentd computer run -- runtime/opencode/opencode-abi.py user:ada
agentd computer run -- acp-bridge -- opencode acp   # any ACP agent
agentd computer run               # the default runtime's driver, no arguments
```

## Choosing the runtime

`agentd` never guesses. `agentd runtime default <id>` writes one id to
`$TIAKI_HOME/runtimes/default`; `agentd computer run` with no command starts
that runtime's driver. An explicit `-- <command>` still overrides it.

Driver resolution, in order:

1. `$TIAKI_HOME/runtimes/<id>.driver` — a developer-installed command line.
   `{principal}` is replaced with the acting principal, or the principal is
   appended when there is no placeholder.
2. The bundled opencode driver next to the agentd binary.
3. The bundled `acp-bridge` next to the agentd binary, driving whatever
   `$TIAKI_HOME/runtimes/acp.agent` names (default `opencode acp`).
4. Anything else is an honest error naming the runtime with no driver.

## The shared skill

`runtime/skills/tiaki/SKILL.md` is the know-how every runtime inherits:
how to read a UI tree, the one-line `ACT`/`BYE` format, that approvals belong
to a human and are never to be attempted, and that quitting loudly beats
looping. Drivers attach it to the model (opencode via `--file`) so the
protocol lives in one versioned file instead of per-driver prompts.

`computer demo` runs the external-runtime flow against a simulated checkout
app with an in-process scripted runtime: observe, toggle gift wrap, request
`Place order` approval, fail a synthetic approval, approve with physical
input, apply, and verify.

`computer run` runs the same host against a real OS process over stdio.
`tests/replay/checkout-plan.py` replays the plan from outside the process
(the approval challenge never crosses the pipe: the runner asks the operator
and mints the physical event itself). `runtime/opencode/opencode-abi.py` lets
a real opencode reason each step — the shim only translates `ACT`/`BYE` lines
and quits rather than act on anything it cannot parse. The operator approves
at the terminal; piped stdin (`echo allow | ...`) counts the same way, as
input from outside the agent's reach.

## Non-goals

Vision/OCR, AT-SPI, and native compositor input injection are future adapters.
The structured-tree tier is the contract they fall back to. Process codex
support is the same shape as the opencode shim and is not yet written.
