# Spec: Validation and Preflight

Canonical definition: [TIAKI.md §8.8](../../TIAKI.md#88-preflight-the-cargo-check-of-state).

A transaction is data, so the compiler cannot check it. The engine supplies the equivalent of `cargo check` before anything is applied.

## Gates

| Gate | Analogy | Mutates state | Writes log | Purpose |
|---|---|---|---|---|
| `check` | `cargo check` | no | no | static validation + summary verification |
| `dry-run` | sandboxed build | no | no | health checks against a scratch copy |
| `test` | `cargo test` | no | no | scenario assertions (planned) |
| `replay` | reproducible build | no | no | re-run the log, compare state (planned) |
| `apply` | `cargo build` | yes | yes | snapshot → apply → verify → commit/revert |

`check` includes a dry run. `apply --dry-run` is an alias for `check` that prints the same report and states that nothing was applied.

## Outcomes

| Status | Meaning | State |
|---|---|---|
| `rejected` | a preflight gate failed | untouched |
| `reverted` | post-apply health failed | restored from snapshot |
| `committed` | applied and healthy | new state |
| `failed` | engine mechanics failed | restored where possible |

`rejected` and `reverted` are deliberately separate: the first is a bug caught before mutation, the second is a bug caught after it. Reporting them as one number hides which gate is failing.

## Report shape

```rust
pub struct CheckReport {
    pub ok: bool,
    pub issues: Vec<Issue>,
    pub summary: Summary,
    pub impact: Impact,
    pub dry_run_health: Option<Health>,
}

pub struct Issue {
    pub severity: Severity,   // Error | Warning
    pub code: String,         // "schema.invalid_value", "path.duplicate", ...
    pub path: Option<String>,
    pub message: String,
}
```

Any `Error` makes `ok == false` and blocks `apply`. `Warning` is informational and never blocks. Current codes:

| Code | Severity | Meaning |
|---|---|---|
| `path.invalid` | error | not a valid state path |
| `path.duplicate` | error | same path written twice in one transaction |
| `value.too_large` | error | value exceeds the size limit |
| `schema.invalid_value` | error | value violates a registered schema |
| `schema.unregistered` | warning | no schema for this path; value unchecked |
| `path.missing` | warning | remove targets a path that does not exist |
| `summary.mismatch` | error | summary does not mention a touched resource |
| `health.dry_run_failed` | error | health check failed on the scratch copy |

## Validator ABI

```rust
pub trait Validator {
    fn validate(&self, store: &dyn StateStore, operations: &[Operation]) -> Vec<Issue>;
}
```

Rules:

1. **Pure.** No mutation, no network, no wall-clock time, no randomness. `check` and `replay` must be deterministic.
2. **Read-only state.** Validators see current state but may not write it.
3. **Composable.** Multiple validators run in order; their issues are concatenated.
4. **Path-scoped where possible.** A validator that owns `plugin/clock/*` should ignore operations outside that prefix.

Milestone 0 ships `SchemaValidator`, which covers path syntax, duplicates, size, the two demo schemas (`desktop/clock/position`, `theme/mode`), and warns on unregistered paths.

## Plugins

Validation is part of the plugin contract:

- A plugin declares the paths it owns in its manifest.
- It ships a schema for values a schema can express and a validator for cross-field or host-dependent rules.
- Installing or upgrading a plugin is a transaction whose preflight loads the plugin's schema and validator. A plugin that fails to load is rejected before it becomes reachable.
- Because the manifest declares writers, the engine can reject an operation that targets an unowned path at check time rather than at runtime.
- Plugin builds can run the same `check` against a scratch state to validate their own migrations before proposing them.

## Health checks and dry runs

Not every health check can run against a scratch copy. Split them:

- **State invariants** — pure functions of the state map (schema validity, cross-field consistency, reference integrity). These run in `check` and catch most bugs before mutation.
- **Live probes** — does the service answer, does the GPU enumerate, does the display come up. These only run after a real apply, so their failure produces `reverted`, not `rejected`.

A `HealthCheck` that needs the live system must declare itself as such; the engine then skips it during the dry run instead of reporting a false failure. Milestone 0 uses state invariants only.

## Determinism and replay

Given the same log and the same validators, `replay` must produce byte-identical state. Any gate that cannot satisfy this must be marked non-deterministic and excluded from replay. This is why model-backed summarizers store their output at commit time instead of regenerating it.

## Milestone 0 status

Implemented: `check`, `dry-run`, `SchemaValidator`, summary verification, `rejected` status, `Validator` trait, CLI (`agentd check`, `agentd apply --dry-run`).

Planned: scenario `test` files, log `replay`, plugin-supplied validators through the plugin ABI, and a `--json` report for agent runtimes.
