# Spec: the agent entry point

Implementation: `control/agentd/src/agent.rs`.

Everything bound for an agent goes through one path, and that path drives the
default agent — opencode — through its real CLI. There is no private wire
protocol. A notification starts a session exactly like a message does.

## The path

```
agentd agent run -- <message>        agentd agent notify -- <text>
        │                                      │
        └──────────────┬───────────────────────┘
                       ▼
              ensure opencode serve
              ensure tiaki MCP
              opencode run --server … --format json
                       │
                       ▼
              NDJSON events → answer + ledger
```

- `ensure_server`: probe `127.0.0.1:$TIAKI_OPENCODE_PORT` (default 4096);
  otherwise spawn `opencode serve` in the workspace with a stored password
  (`$TIAKI_HOME/opencode.pass`, mode 600) and wait for it. One server per
  home directory, reused across invocations.
- `ensure_mcp`: register `tiaki` once as a global opencode MCP server
  (`agentd mcp serve` over stdio). Values pass through from our own
  environment at spawn time (`{env:TIAKI_HOME}` etc.), so the principal is
  per-invocation, never baked into config.
- `opencode run --server … --format json [--session ID] [--title T] [--auto]`:
  one NDJSON event per line on stdout. `run` waits and prints the final
  answer; `notify` takes the identical path with ledger intent
  `notification`.
- Full access is the default (unless `--mode guarded` was explicit): `--auto`
  approves whatever is not explicitly denied. Guarded runs without `--auto`,
  so opencode's own policy asks — and with no human on the pipe the run fails
  loudly instead of acting.

## What the runner reads

Each event carries the session id. Text accumulates from `part.text`; the
`step_finish` part carries `reason`, `cost`, and token counts. Unknown event
types go to stderr, never silently dropped. A missing session id at the end
is an error, not an empty success.

## What lands on the ledger

- `agent run` / `notification` started: who started what, under which title.
- Every MCP tool call the agent makes (written by `agentd mcp serve` itself,
  a separate process sharing the log — see below).
- `agent run` / `notification` done: first line of the answer.

## The log is multi-writer

The runner and its MCP server are different processes appending to one
hash-chained log. `ActionLog::append_exclusive` takes an `O_EXCL` lock file,
reloads, appends, and releases; a lock older than 30 seconds is treated as a
crashed holder. `agentd log --repair` resequences a corrupted file in on-disk
order (keeping a `log.corrupt-*` backup) after verifying everything else
first.

## CLI

```
agentd agent run [--title T] [--session ID] -- <message...>
agentd agent notify [--title T] -- <text...>
agentd agent list
agentd agent show <session-id>
agentd agent default [id]
```

`list` and `show` are thin over `opencode session list|export`, run from the
workspace so project scoping matches the server.

## Image

Everything the live proof needed by hand is now baked or seeded:

- `desktop/wayland/Dockerfile`: pinned Node 26 + pnpm 12 (arch-aware),
  pinned opencode 2.2.5 from its release tarball, `COPY t3code-upstream`
  plus `pnpm install`, `EXPOSE 5733`, `tiaki-t3.service` enabled.
- `desktop/systemd/tiaki-t3.{sh,service}`: starts the fork as `desktop`
  with `T3_ACP_EXTRA_MCP_SERVERS` hardcoded to the concrete desktop paths
  (`{env:}` references resolve empty under servers T3 spawns itself).
- `tiaki-firstboot.sh`: per-home workspaces (`/var/lib/tiaki/workspace`,
  `/home/desktop/.tiaki/workspace`); idempotent seeds for the desktop
  opencode config (model + hardcoded `tiaki` entry) and the T3 opencode
  provider (`enabled`, `binaryPath`).
- Password lifecycle: `agentd` generates `$TIAKI_HOME/opencode.pass`
  (mode 600) on first use and reuses it; `ensure_mcp` reads the global
  config file first and never replaces an existing `tiaki` entry,
  so firstboot's concrete seed survives agent runs.

## What died for this

- `tiaki-abi/1`: `RuntimeMessage` and its render/parse, `ProcessRuntime`,
  `ComputerHost`/`ScriptedRuntime`, the `computer` message loop, the python
  `opencode-abi.py` driver, and the `acp-bridge` translator.
- `agentd runtime`, `agentd computer`, `agentd agents` commands.
- Kept: the tree (payload of `ui.observe`/`ui.act`), the app backend and
  `SimulatedApp` (which `agentd mcp serve` drives), approval/trusted input,
  `RuntimeSpec`/`ToolSpec` descriptors, the registry and scheduler.
- `acp-run` stays as a dev prober: pure ACP, no ABI in its path.

## Evidence

Live in the container, real opencode 2.5, `agent notify`:

```
#033  user:ada  full  user-write  agent_…  user:ada started notification 'disk-warning'.
#034  user:ada  full  user-write  mcp_…      Ran `df -h`: exit 0
#035  user:ada  full  user-write  mcp_…      Created …/workspace/notes/disk-note.txt (33 bytes).
#036  user:ada  full  inspect     mcp_…      Read …/workspace/notes/disk-note.txt (33 bytes).
#037  user:ada  full  user-write  agent_…  user:ada notification 'disk-warning' done: …
```

The agent ran a shell command, wrote the file to the absolute workspace path,
read it back to verify, and every step is attributed to `user:ada`.
