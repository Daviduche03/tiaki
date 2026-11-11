# ADR-0003: Sandboxing is orthogonal to access mode

- **Status:** accepted
- **Date:** 2026-11-07
- **Related:** TIAKI.md §9.3, §9.6

## Context

An earlier proposal sandboxed only guarded mode. That makes full-access mode the least isolated mode — exactly backwards, since it is the mode that can do the most damage.

## Decision

Every agent action runs in a sandbox profile in both modes. Modes differ in capability scope and approval policy, not in whether isolation exists. Full access is a wider sandbox, not no sandbox.

The chain is: capability → sandbox profile → process.

Profiles: `inspect`, `user-write`, `system-write`, `privileged`. No profile gets raw root; privileged operations are brokered.

Sandbox creation is a pluggable `SandboxProvisioner` ABI, implementable in Rust or as a separate service over IPC.

## Consequences

### Positive

- Defense in depth in every mode.
- Existing provisioning layers can be adopted without porting.
- Escapes are detectable events, not silent mode transitions.

### Negative

- Full access needs a carefully scoped `system-write` profile.
- More moving parts than a single "trusted agent" path.

## Alternatives considered

- **Sandbox only in guarded mode:** rejected; inverts the risk model.
- **No sandbox, broker-only mediation:** weaker containment.
