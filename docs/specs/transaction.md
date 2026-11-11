# Spec: Transaction

Canonical definition: [TIAKI.md §8](../../TIAKI.md#8-layer-1--transaction--state-engine). Preflight gates: [validation.md](validation.md).

## Lifecycle

```
proposed → checked → snapshotted → applied → health-checked → committed
             │                                  │
             └─ failed → rejected               └─ failed → reverted
```

- `rejected` — a preflight gate failed. State is untouched; the attempt is logged.
- `reverted` — preflight passed but the post-apply health check failed; the snapshot was restored.
- `failed` — the engine could not complete the mechanics (snapshot I/O, operation I/O, log append).

## Required fields

`id`, `principal`, `runtime`, `session`, `scope`, `intent`, `summary`, `summary_source`, `summary_verified`, `risk`, `impact`, `operations`, `snapshot`, `revert`, `created_at`, `status`.

## Invariants

1. No system mutation without a transaction.
2. A transaction is either fully committed or fully reverted.
3. The diff is authoritative; the summary is derived.
4. Every committed transaction has a working revert path.
5. Reverts are themselves transactions.
6. `check` never mutates state and never appends to the log.
7. A rejected transaction never writes state, even transiently.
8. Validators are pure: same `(state, operations)` produces the same issues.

## Open

- Generation granularity (per transaction vs. batched).
- Conflict resolution when multiple writers touch one path.
- Batching preflight across a multi-transaction plan.
