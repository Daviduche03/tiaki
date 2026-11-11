# ADR-0002: Full agent access with guaranteed undo

- **Status:** accepted
- **Date:** 2026-11-07
- **Related:** TIAKI.md §3, §8, §9

## Context

The product goal is an agent that can change anything: configs, services, packages, and the UI. Restricting the agent would defeat the premise. The real risk is not access; it is unrecoverable mutation.

## Decision

Grant the agent full authority within its principal's scope. Require that every mutation is a transaction with a snapshot, a diff, a principal, a rationale, and a revert path. The agent may author any transaction; it may not bypass the ledger.

The recovery core (boot, initramfs, log, rollback, capability root) remains immutable at runtime.

## Consequences

### Positive

- The agent can genuinely rebuild and repair the system.
- Every change is auditable and reversible.
- Full access becomes safe because recovery is guaranteed.

### Negative

- Transaction overhead on every mutation.
- Snapshot storage cost.
- Requires a strict rule about what may not be mutated.

## Alternatives considered

- **Restricted agent:** safe but defeats the product.
- **Full access without a ledger:** fast but one mistake from a brick.
