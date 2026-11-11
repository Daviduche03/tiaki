# ADR 0007 — Step-driven inference ABI

**Status:** accepted · 2026-11-07

## Context

The milestone requires that an interactive request preempt a batch request at a
token boundary, that multiple principals share one forward pass, and that a
preempted request resume with its KV state intact. The obvious ABI,
`fn generate(req) -> Stream<Token>`, cannot do this: once the backend starts,
the broker has no point at which to change the batch. A separate `preempt()`
call is possible but leaves the policy split between broker and backend.

## Decision

The broker owns the batch. The backend exposes a step interface:

```rust
fn begin(&mut self, model, request, cache) -> Result<BeginResult>;
fn step(&mut self, sessions: &[SessionHandle]) -> Vec<TokenEvent>;
fn cancel(&mut self, session);
fn zero_cache(&mut self, cache) -> Result<()>;
```

- `begin` prefills and creates a session.
- `step` advances exactly the sessions the broker selects, one token each.
- Preemption is the broker declining to pass a session to `step`; the session
  and its cache stay alive.
- Completion calls `cancel` and then `zero_cache`.
- The broker allocates cache handles and guarantees they never cross principals.

The broker is synchronous and `tick`-driven so the policy is deterministic and
testable without real time or a GPU.

## Consequences

**Positive**

- Preemption, batching, and fairness are broker policy, not backend behavior.
- Tests can assert exact interleavings, token counts, and preemption order.
- The same ABI supports a deterministic simulator and a real backend.
- Cache isolation is enforced centrally; a backend cannot accidentally hand a
  cache to another principal.

**Negative / costs**

- A backend that only exposes a blocking `generate` needs a session wrapper and
  cooperative cancellation; it cannot be preempted mid-token cheaply.
- The broker carries more state (active sessions, resume state, cache table).
- Real backends must implement token-level stepping, which may be awkward for
  some serving stacks.

## Alternatives considered

- **Blocking `generate()` plus `preempt()`.** Rejected: the backend would own
  scheduling decisions and the broker could not batch across principals.
- **Async task per request with cancellation.** Rejected: no async runtime or
  external crates, and cancellation still does not give token-boundary
  fairness.
- **App-level scheduling.** Rejected: the OS is the only layer that can see all
  principals and enforce isolation and quotas.
