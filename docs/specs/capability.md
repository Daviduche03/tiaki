# Spec: Capability & principal model

Canonical definition: [TIAKI.md §9](../../TIAKI.md#9-layer-2--capability--principal-model).
Implementation: `control/capability`.

## Principals

| Form | Meaning |
|---|---|
| `system` | the OS itself; root authority |
| `user:<id>` | a human user |
| `service:<id>` | a daemon acting on its own behalf |
| `agent:<runtime>:<session>@<principal>` | an agent acting *for* a principal |

An agent never has independent authority. `Principal::effective()` unwraps an
agent to the principal it acts for, and every grant lookup uses that effective
principal. An agent's own identity is still recorded on the log entry.

Ids may not be empty or contain `:`, `@`, or whitespace.

## Actions

```
state.read
state.write
state.revert
shell.exec
service.manage
plugin.manage
system.admin
```

## Grants

A grant is `(principal, action, resource pattern)` plus constraints:

| Field | Meaning |
|---|---|
| `requires_approval` | in guarded mode the action becomes `pending-approval` |
| `max_per_minute` | rolling 60-second quota, consumed only at apply time |
| `expires_at` | unix seconds; inactive once `now >= expires_at` |
| `granted_by` | principal that issued the grant |

Resource patterns are segment globs: `*` matches exactly one segment, `**`
matches zero or more. Matching is anchored to the whole path, so
`system/services/*` matches `system/services/network` but not
`system/services/network/state`.

## Default policy

| Principal | Grants |
|---|---|
| `user:<id>` for each of `local`, `ada`, `bob` | `state.read` on `**`; `state.write`, `state.revert`, `shell.exec` on `user/<id>/**` |
| `system` | every action on `**` |
| `service:network` | `state.write` on `system/services/network/**`; `state.read` on `**` |

## Evaluation

Preflight (`Policy::authorize`) is pure: it never consumes quota and never
mutates. The engine asks it for:

1. the effective principal;
2. each operation's action and path;
3. the widest sandbox class the operations require.

Deny if any path is a recovery path (`core/`, `boot/`, `recovery/`,
`system/recovery/`), even for `system`. Otherwise deny if no active grant
matches. Otherwise widen the sandbox across all operations and report
`requires_approval` when a matching grant carries it and the mode is guarded.

After preflight passes, and immediately before mutation, `Policy::consume`
enforces `max_per_minute`. A rate-limit failure rejects the transaction
without touching state.

## Reverts

Reverting a transaction owned by another principal requires a `state.revert`
grant on the target principal's scope. Same-principal and `system` reverts are
always allowed. Revert decisions are recorded on the log like any other
transaction.

## Persistence

Grants are one tab-separated line per grant under `$TIAKI_HOME/grants`:

```
<principal> <granted_by> <action> <resource> <requires_approval> <max_per_minute> <expires_at>
```

Defaults are seeded on first run. `Policy::can_grant` limits users to granting
and revoking resources under their own `user/<id>/**`; `system` may grant
anything.

## Invariants

- Guarded and full mode are both sandboxed; full mode is a wider scope and an
  auto-approval policy, never the absence of authority checks.
- The recovery core is immutable at runtime in both modes.
- Preflight does not consume quota; the apply path does.
- Every grant is attributable through `granted_by`.
- The log records principal, mode, and sandbox class for every attempt.
