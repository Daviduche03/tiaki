# ADR 0008 — Trusted input gates high-risk computer use

**Status:** accepted · 2026-11-07

## Context

Computer use lets an agent click, type, and invoke app actions. The design
requires approval for high-risk actions (place an order, delete data, change
security settings). If an agent can synthesize the approval event, it can
approve itself and the trust model collapses: the human is no longer in the
loop, and the ledger records consent that never happened.

## Decision

Approvals are gated on **input provenance**, not on the agent's claim:

- The compositor marks every input event `physical`, `trusted-device`, or
  `synthetic`.
- The approval service accepts only `physical` and `trusted-device` events.
- Trusted-device approvals carry an HMAC-SHA256 signature over
  `challenge|nonce|payload` from a device key.
- Challenges expire and are single-use. A rejected attempt does not consume the
  challenge.
- High-risk actions are not applied until the host receives a trusted decision.
  The runtime never has an API to apply them directly.
- Only the host applies pending actions; the runtime only sends protocol
  messages.

The runtime protocol therefore carries `approve { challenge, event, token }`,
and the host is the only component that calls `ApprovalService::decide`.

## Consequences

**Positive**

- An agent cannot self-approve, even with full computer-use access.
- Approvals are attributable and recorded; the log shows denials as well as
  grants.
- The trust rule is architectural, not prompt-based.
- The same mechanism works for a local dialog and a phone/tablet approval.

**Negative / costs**

- The compositor must mark events correctly; a compromised compositor breaks
  the guarantee. The compositor belongs to the recovery-core trust chain.
- A trusted-device flow needs key provisioning and a signing app.
- Headless or keyboard-less setups need an explicit trusted-device path.
- Challenge state must be protected from replay and expiry bugs.

## Alternatives considered

- **Capability-only approval.** Rejected: a capability says the agent may act,
  not that a human consented to this action.
- **Prompt-based "ask the user" with the agent reading the answer.** Rejected:
  the agent can fabricate or misinterpret the answer.
- **Approval by the runtime.** Rejected: the runtime is the party being
  approved; it cannot be the approver.
- **No high-risk actions.** Rejected: it would make computer use useless for
  real work.
