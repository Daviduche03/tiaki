# Spec: MCP capability server

Implementation: `control/agentd/src/mcp.rs` (`agentd mcp serve`),
`runtime/acp/src/mcp.rs` (ACP `mcpServers` entry), `runtime/acp/src/bin/acp-run.rs`.

This is the replacement for the invented `tiaki-abi/1` wire protocol. The OS
does not define a private way for agents to act. It speaks **ACP** to the
agent and exposes its capabilities as **MCP tools**. Both are standards; both
are inspectable; neither is a narrative we made up.

## The shape

```
agent (opencode / codex / anything ACP)
   │  ACP (JSON-RPC over stdio)
   ▼
acp-run / acp-bridge          ← ACP client
   │  session/new { mcpServers: [tiaki] }
   ▼
agentd mcp serve              ← MCP server on stdio
   │
   ├── ui.observe / ui.act
   ├── fs.list / fs.read / fs.write
   ├── terminal.exec
   └── one ledger transaction per call
```

The agent's tools *are* the OS's capabilities. There is no second protocol
underneath.

## Access model

**Full access by default.** An agent is not a prisoner. The safety boundary is
the sandbox plus reversibility plus attribution, not a permission prompt:

- every call is a ledger transaction with a one-line summary;
- `fs.write` backs up the previous contents before writing;
- the principal is recorded on every entry;
- `terminal.exec` runs in the agent workspace.

Guarded mode is opt-in (`--mode guarded`) and only refuses the destructive
tools (`fs.write`, `terminal.exec`) with an explicit error. It does not pretend
to ask a human who is not present.

## Tools

| tool | input | effect |
|---|---|---|
| `ui.observe` | — | returns the app tree as JSON |
| `ui.act` | `node`, `action`, `value?` | validates the node and applies the action |
| `fs.list` | `path?` | lists a directory (relative paths under the workspace) |
| `fs.read` | `path` | reads up to 1 MiB of UTF-8 |
| `fs.write` | `path`, `content` | writes, creating parents, backing up the old file |
| `terminal.exec` | `command`, `timeout_ms?` | runs `sh -c`, captures exit/stdout/stderr |

`ui.act` validates exactly like the old session did — node exists, enabled,
declares the action, value present when required — so a model cannot invent a
verb a widget does not support.

## Protocol

MCP over stdio, newline-delimited JSON-RPC 2.0. Only JSON-RPC goes to stdout;
diagnostics go to stderr.

- `initialize` → echoes the client's `protocolVersion`, advertises `tools`.
- `notifications/initialized` → no reply.
- `tools/list` → the six tools above with JSON Schema inputs.
- `tools/call` → `{content:[{type:"text",text}], isError}`.
- `ping`, `resources/list`, `prompts/list` → minimal valid replies.
- unknown method → `-32601`.

## How the agent gets it

`runtime/acp/src/mcp.rs` builds the ACP `mcpServers` entry:

```json
{
  "name": "tiaki",
  "command": "/usr/local/bin/agentd",
  "args": ["mcp", "serve"],
  "env": [
    { "name": "TIAKI_HOME", "value": "/home/desktop/.tiaki" },
    { "name": "TIAKI_PRINCIPAL", "value": "user:ada" },
    { "name": "TIAKI_WORKSPACE", "value": "..." }
  ]
}
```

Resolution: `TIAKI_MCP_SERVER`, then `agentd` beside the executable, then
`agentd` on `PATH`. No server found means no capabilities — said plainly,
never faked.

The principal travels out of band as `TIAKI_PRINCIPAL`; `agentd` reads it
when `--principal` is absent. That is how a tool call is attributed to the
human the agent acts for.

## Evidence

Live in the desktop container, real opencode 2.2.5, no `tiaki-abi/1` in the
path (`acp-run -- opencode acp`):
```
[acp-run] offering 1 MCP server(s) to the agent
[acp-run] ACP session ses_e41650bb4ffeVQs7xc0Sq6jxbu
[acp-run] stop=end_turn tool_calls=4 denied=0
```

Ledger:

```
#024  user:ada  full  inspect     mcp_...  Read the 'checkout' UI tree.
#025  user:ada  full  user-write  mcp_...  toggle on 'gift': Gift wrap is on
#026  user:ada  full  user-write  mcp_...  Created .../notes/acp-proof.txt (15 bytes).
```

And the file the agent wrote:

```
gift is now on
```

## What this replaces

| old `tiaki-abi/1` | now |
|---|---|
| `hello` / `welcome` | ACP `initialize` + `session/new` |
| `observe` / `tree` | MCP `ui.observe` |
| `act` / `acted` | MCP `ui.act` |
| `approval` / `decision` | gone by default; guarded refuses instead |
| `bye` | session close |

`RuntimeMessage`, `ProcessRuntime`'s envelope, `host.rs`, and the
`computer run` message loop are now legacy. They are deleted once the ACP
path covers the demos.

## T3 Code fork

Our T3 Code fork (`t3code-upstream`, branch `agentic/extra-mcp-servers`,
v0.1.7 base) appends `T3_ACP_EXTRA_MCP_SERVERS` (a JSON array of ACP
`mcpServers` entries) to every `session/new|load|resume|fork` payload, after
the built-in `t3-code` server. Name collisions lose to `t3-code`; malformed
entries are dropped. To give a T3 thread the OS tools:

```sh
export T3_ACP_EXTRA_MCP_SERVERS='[{"name":"tiaki","command":"/usr/local/bin/agentd","args":["mcp","serve"],"env":[{"name":"TIAKI_HOME","value":"/home/desktop/.tiaki"},{"name":"TIAKI_PRINCIPAL","value":"user:ada"}]}]'
```

Single choke point: `sessionMcpServers()` in
`packages/provider-acp/src/server/AcpSessionRuntime.ts`, via
`extraMcpServers.ts`. Native OpenCode sessions bypass the generic ACP path,
so the same variable is honored a second time:
`packages/provider-opencode/src/server/extraMcpServers.ts` translates each
entry to OpenCode's native `client.mcp.add` shape (`local` → command array +
environment, `http` → url + headers, `acp` skipped), and both the v1 and v2
adapters add them per session, each failure-isolated with a warning.
No settings-schema or contract changes; unset means stock behavior.
