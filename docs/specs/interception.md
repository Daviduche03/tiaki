# Spec: Event interception

Canonical definition: [TIAKI.md §13–14](../../TIAKI.md#13-layer-6--agent-runtime-abi)
(`tiaki-abi/1` protocol, computer-use sessions) and [§14.3](../../TIAKI.md#143-trusted-input-non-negotiable)
(trusted input).
Implementation: `runtime/abi` (verdict messages), `control/agents`
(supervisor scheduling of interceptor sessions), future `control/notify`
(broker daemon).

## Principle

Every system event is agent-readable (with a grant) and agent-actionable
(with approval). The human is the escalation path, not the router.

Today a notification is a dead end: it shouts at the human and dies. Under
this spec the agent gets first refusal on every ping — it reads the event,
updates its context, and either handles it silently, drafts something for
approval, or wakes the human because it actually needs them. The same
contract then extends to every event stream the OS delivers dumbly today:
crashes, messages, calendar, network, power, files, updates.

## The contract

```
subscribe (grant) → deliver → verdict → ledger
```

1. **Subscribe.** An agent session registers interest in an event source
   (`app/<id>`, `system/crash`, `system/power`, ...). Subscription requires
   a `notify.observe` grant on that scope. No grant, no events — the broker
   drops the subscription instead of delivering.
2. **Deliver.** The broker hands the event to at most one interceptor session
   (first matching subscription wins; ties break toward the oldest). Delivery
   starts a supervised agent session, scheduled like any other agent work.
3. **Verdict.** The agent answers with exactly one of:
   - `suppress` — handled, say nothing (build finished, nothing to report);
   - `note` — file a one-line context update, no user-visible output;
   - `draft` — prepare content (a reply, a summary, a briefing) for the human
     to send or dismiss;
   - `act` — perform a structured action, which re-enters the normal gate:
     low/medium risk applies, high risk raises a trusted-input approval;
   - `escalate` — hand to the human now, with the agent's one-line reason
     attached. Anything unhandled after the timeout escalates automatically.
4. **Ledger.** Every delivery, verdict, and resulting action is recorded with
   principal, runtime, source, and summary. Event payloads and message bodies
   are never stored — only what happened, never what it said.

## Loop guard

An agent acting generates events (approves an order → "order confirmed").
Agent-caused events carry their cause chain and **never re-trigger** an
interceptor. As a backstop, any chain deeper than 3 is escalated to the human
with the full trail instead of being processed. An infinite spending machine
must be impossible by construction, not by politeness.

## Capability model

Reading is its own permission, separate from acting:

| Action | Scope | Default holders |
|---|---|---|
| `notify.observe` | `app/<id>`, `system/<source>` | nobody (grant only) |
| `notify.act` | `app/<id>` | via `computer.use` (unchanged) |

Rationale: `computer.use` lets an agent drive what it can already see. Reading
someone's messages, OTP codes, and bank alerts is a different power and gets
its own gate, default-deny, per-app, per-principal. Agents borrow their
user's scope, so `user:ada`'s agent can only ever observe what Ada granted —
never Bob's streams.

Every read is ledgered (`observed <source>`). A principal can list exactly
which agents watch which streams, and revoke any of them.

## Protocol

Additive messages on `tiaki-abi/1` (no version bump; old runtimes ignore what
they never receive):

| Direction | Messages |
|---|---|
| host → runtime | `event { id, source, cause, summary }` |
| runtime → host | `verdict { id, kind, payload }` |

`cause` is the chain this event answers (`[]` for exogenous events like an
incoming message; `[evt_41]` for "order confirmed" answering the approval).
`summary` is a redacted one-liner — enough to triage on, never credentials,
codes, or full bodies. The runtime that wants detail already holds the
`notify.observe` grant and reads through the same gate as everything else.

Unknown verdict kinds and verdicts for unknown ids are errors, not
escalations: a confused interceptor must not be able to page the human by
accident.

## Notification broker (first binding)

The broker is a daemon, not a library: it must outlive senders, queue bursts,
and be reachable from any sandbox. Intake is twofold:

- **D-Bus `org.freedesktop.Notifications`** — so existing Linux apps work
  unmodified. Their bubbles become events like any other.
- **Native `agentd` API** — first-class sources with structured payloads.

Routing per event: interceptors first (one session, verdict within the
timeout), then the user surface (banner, center, history) unless suppressed.
Do-not-disturb, per-app quiet hours, and the agent-watching indicator all
live here as tree nodes. The broker never approves anything; an `act` verdict
on a high-risk action raises the same trusted-input challenge as a direct
computer-use act, citing the originating event.

## Crash interceptor (second binding)

The proof that interception helps instead of merely filtering. On crash, the
broker delivers the core dump reference, logs, and the faulting update (if
any) to a supervisor session — not a "report to Apple" dialog to the human.
The agent diagnoses and either files a repair transaction (config revert,
generation rollback proposal) or reports in plain language what broke and
what it did. Telemetry into a void becomes self-healing with a receipt.

## CLI

```sh
agentd notify send <app> <title> <body>   # inject an event (demo / tests)
agentd notify list                        # recent deliveries and verdicts
agentd notify watch app/inbox             # subscribe this principal's agent
agentd notify unwatch app/inbox
agentd runtime list                       # runtimes eligible as interceptors
```

Subscriptions are grants under the hood: `watch` is
`grant <principal> notify.observe app/<id>` with a human-readable name.

## Demos

- `agentd notify demo`: a message arrives, the scripted runtime files a
  `note`, then a high-risk `act` that the operator approves; the ledger shows
  delivery → verdict → approval → applied, and the final banner is the
  approved result, not the raw event.
- `agentd computer demo` stays unchanged: no subscription, no interception,
  the host behaves exactly as today.

## Non-goals

Cloud relay of events, content-based filtering by third parties, breaking
D-Bus compatibility, and intercepting streams the OS cannot see (E2E-encrypted
payloads stay opaque; the agent gets metadata and the user's own client, not
plaintext). Multi-principal contention over one stream keeps per-principal
grants; there is no shared inbox.
