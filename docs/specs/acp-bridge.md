# Spec: ACP bridge

Implementation: `runtime/acp` (`agentic-runtime-acp`, binary `acp-bridge` (now `acp-run`)).

The bridge makes any [Agent Client Protocol](https://agentclientprotocol.com)
agent — opencode, or anything else that speaks ACP — a first-class Tiaki
runtime. It is the shortest path from "an agent exists" to "the OS can drive
it", with no per-agent adapter and no rewrite.

## Two faces

```
agentd computer run
        │  tiaki-abi/1 (newline JSON, stdio)
        ▼
   acp-bridge
        │  ACP v1 (JSON-RPC, newline delimited, stdio)
        ▼
   opencode acp   (any ACP agent)
```

- **Host side**: the bridge is an ordinary `tiaki-abi/1` runtime process. It
  sends `hello`, `observe`, `act`, and `bye`; the host needs no changes.
- **Agent side**: the bridge is an ACP *client*. It runs `initialize`,
  `session/new`, and one `session/prompt` per decision, then parses the
  streamed answer.

## Selection

The runtime id is `acp`. It is built in, so it appears in
`agentd runtime list`:

```
acp  tiaki-abi/1  process://acp-bridge  computer-use, ui
```

`agentd runtime default acp` makes it the default; a bare
`agentd computer run` then starts it. Which ACP agent it drives is developer
config, not code: `$TIAKI_HOME/runtimes/acp.agent` holds the command words
(default `opencode acp`). Switching to another ACP agent is one file edit.

An explicit command still overrides everything:

```
agentd computer run -- /usr/local/bin/acp-bridge -- opencode acp
```

## MCP capabilities

The bridge offers the agent the OS capability server in `session/new`:

```json
"mcpServers": [{ "name": "tiaki", "command": ".../agentd",
                 "args": ["mcp", "serve"], "env": [...] }]
```

That is where the agent's real tools come from — files, terminal, the UI tree.
See [mcp.md](mcp.md). The ABI decision loop described below is the legacy
path; `acp-run` is the direct ACP path that replaces it.

## Trust posture

Deny by default, twice over:

- The bridge declares **no** ACP client capabilities — no filesystem, no
  terminal, no elicitation. The agent has no handle on the machine.
- Every `session/request_permission` is answered with a rejection (or
  `cancelled` when the agent offers nothing reject-like). The bridge never
  grants a permission, because it has nothing it may grant.
- The agent reasons from the prompt text only: the current UI tree and the
  shared skill. It cannot read files, run commands, or touch the display
  itself.
- High-risk acts still go through the host's human approval, exactly as with
  every other runtime. The ACP layer cannot bypass the OS gate; it is below it.

This is why the bridge can safely front an agent that is fully capable: the
capability is not in the agent, it is in the host, and the host says no.

## Decision contract

The model answers each turn with exactly one line:

```
ACT <node> <action>      # e.g. ACT gift toggle, ACT place invoke
BYE
```

The parser takes the first usable decision line and ignores surrounding prose.
An unusable answer is retried once with a stricter prompt; a second unusable
answer ends the session without acting. The round budget is 8 turns, after
which the bridge sends `bye` and exits non-zero rather than guessing.

## Turn loop

1. Bridge sends `observe checkout`; host returns the tree.
2. Bridge builds a prompt: goal, tree JSON, the previous action's outcome, and
   the shared skill (`runtime/skills/tiaki/SKILL.md`, or
   `$TIAKI_SKILL`, or `/usr/share/tiaki/skills/tiaki/SKILL.md`).
3. One `session/prompt` turn; `agent_message_chunk` text is collected,
   `tool_call` and permission requests are counted and denied.
4. Parse the decision; send `act`; read `acted` / `decision` / `error` back.
5. Repeat until `bye` or the budget is spent.

Timeouts: 120 s for `initialize` and `session/new`, 300 s per prompt turn. On
timeout the bridge sends `session/cancel` and gives up loudly.

## Transport details

- Newline-delimited JSON-RPC 2.0, UTF-8, no embedded newlines.
- One request in flight at a time; a reader thread feeds a channel so a turn
  can time out instead of hanging.
- Unparsable lines are counted and skipped, never fatal.
- `stdin` is piped; `stdout` is piped; the agent's `stderr` is inherited so its
  logs stay visible to the operator.

## Principal

The acting principal travels out of band as `TIAKI_PRINCIPAL`, set by the
host when it spawns the runtime. It is never taken from argv, because global
flags are stripped before subcommands see them. The bridge refuses to start
without a principal.

## Evidence

Live in the desktop container against real opencode 2.2.5 (model
`meta/muse-spark-1.3-contributor`), bare `agentd computer run` with
`runtime default acp`:

```
  runtime → observe checkout
  host    → tree checkout (6 nodes)
  runtime → act checkout/gift toggle
  host    → acted ok=true: Gift wrap is on
  runtime → act checkout/place invoke
  host    → approval appr_1: 'Place order' (risk high)
  host    → decision appr_1 approved=true: Order placed
  runtime → bye
```

The ledger recorded all three acts (`gift`, approval `Place order`, `place`),
proving the ACP path lands in the same transaction log as every other runtime.

## Non-goals

- Not an ACP *server*. The OS is the client; agents are the servers.
- Not a permission broker. ACP permission requests are denied here; real
  approval lives in the host and the interception broker.
- Not a capability upgrade. The bridge adds reach, never authority.
