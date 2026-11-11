# Spec: Inference broker

Canonical definition: [TIAKI.md §12](../../TIAKI.md#12-layer-5--inference-broker).
Implementation: `control/inference`.

## The split

> The broker owns policy; the backend owns compute.

The broker handles admission, model registry, batching, preemption, fairness,
cache ownership, quotas, and ledger events. A backend implements compute and
nothing else. On hosts without a GPU, `SimulatedBackend` lets the broker's
policy be tested deterministically.

## Backend ABI

```rust
trait InferenceBackend {
    fn name(&self) -> &str;
    fn load(&mut self, spec: &ModelSpec) -> Result<ModelHandle, BackendError>;
    fn unload(&mut self, handle: ModelHandle) -> Result<(), BackendError>;

    fn begin(&mut self, model: ModelHandle, request: &GenerateRequest, cache: CacheHandle)
        -> Result<BeginResult, BackendError>;
    fn step(&mut self, sessions: &[SessionHandle]) -> Vec<TokenEvent>;
    fn cancel(&mut self, session: SessionHandle);
    fn zero_cache(&mut self, cache: CacheHandle) -> Result<(), BackendError>;

    fn stats(&self) -> BackendStats;
}
```

The ABI is **step-driven**, not a blocking `generate()`:

- `begin` prefills the prompt into a KV cache and returns a session.
- `step` advances every session passed by the broker by one token.
- The broker decides the batch, so batching and preemption are policy.
- `cancel` is called on completion or failure, never on preemption: a
  preempted session stays alive with its cache.
- `zero_cache` is a security operation. The backend must actually clear the
  state; the broker calls it on completion and handoff.

`TokenEvent` is `Token { session, text }`, `Done { session, tokens }`, or
`Failed { session, reason }`.

## Requests

```rust
struct GenerateRequest {
    principal: String,   // effective principal; agents borrow their user's scope
    session: String,
    model: String,
    prompt: String,
    priority: Priority,  // Batch | Normal | Interactive
    max_tokens: usize,
    deadline: Option<u64>,
}
```

## Model registry

`ModelSpec` fields: `id`, `version`, `architecture`, `context_length`,
`quantization`, `weights_digest` (`sha256:<64 hex>`), `tokenizer`, `license`,
`backend`, `loaded`.

State path: `system/inference/models/<id>`. Model ids may contain `/`, so
`sim/tiny` lives at `system/inference/models/sim/tiny`. `ModelRegistryValidator`
runs in preflight: it rejects malformed specs and an id that does not match the
path suffix. Load and unload are ordinary transactions; the spec stays in the
registry with `loaded` toggled.

Weights are content-addressed and immutable. The Linux implementation maps them
read-only and locks them; the simulated backend needs no bytes.

## Scheduling

- Three priority classes: `Interactive` > `Normal` > `Batch`.
- **Continuous batching**: one `step` carries sessions from multiple principals.
- **Preemption**: when an `Interactive` request arrives and the active set is
  full, the broker removes the lowest-priority active session at a token
  boundary, requeues it at the front of its class, and keeps its session and
  cache. No `cancel`, no cache release.
- **Fair share**: within a class, the scheduler picks the principal with the
  fewest started requests, FIFO on ties. Resumed work goes first.
- A preempted request resumes with its token count and first-token step intact.

## Cache isolation

`CacheTable` owns the handles:

- `acquire(principal, session, model)` always allocates a fresh handle. Handles
  are never reused, so one principal cannot receive another's state.
- `release(backend, handle)` calls `zero_cache`, records the zeroing, and drops
  the entry. Called on completion or failure.
- `handoff(backend, principal)` zeroes every cache owned by a principal, for
  session end or device handoff.
- `discard(handle)` is used only when `begin` fails before the backend
  registered the cache.

Per-principal isolation is a security requirement, not an optimization.

## Quotas

`Quota::for_principal` seeds defaults:

| Principal | Concurrent | Tokens/request | Tokens/min |
|---|---|---|---|
| user | 2 | 1024 | 8192 |
| system, service | 8 | 4096 | 1,000,000 |

`UsageBook` tracks a sliding token window. Admission occurs at `submit`;
actual tokens are charged on completion. Exceeding the concurrent cap or the
token window returns `BrokerError::Quota` and the request never starts.

## Authority

| Action | Scope | Default holders |
|---|---|---|
| `inference.generate` | `model/**` | every user, `system`, `service:inference` |
| `inference.manage` | `system/inference/**` | `system`, `service:inference` |
| `state.write` | `system/inference/**` | `system`, `service:inference` |

`service:inference` needs the registry write grant in addition to
`inference.manage`, because model load/unload is a state transaction.

Agents borrow the effective principal, so they consume that user's quota and
cache scope. A user cannot load, unload, or register models.

## Ledger

- Model load/unload: `Operation::Set` on `system/inference/models/<id>`, with a
  summary like "Updated model sim/tiny."; revert restores the previous spec.
- Each generation: one event with principal, model, priority, token count,
  preemption count, and status. Prompt content is not stored.
- Bench runs: one summary event.

## CLI

```sh
agentd inference models
agentd inference load models/sim-tiny.json
agentd inference unload sim/tiny
agentd inference ask sim/tiny "summarize the logs" --priority interactive --max-tokens 32
agentd inference bench
agentd inference stats
agentd inference cache
```

`bench` is the exit demo: one model, Ada's batch and Bob's normal request
batched together, Carol's interactive request preempting Ada's batch, Ada
resuming, and every cache zeroed at completion.

## Backend roadmap

1. **SimulatedBackend** (now): deterministic token streams, controllable
   batching, preemption, and isolation tests.
2. **ProcessBackend**: JSON-lines over a subprocess, running under a sandbox
   spec; the adapter for llama.cpp or a local server.
3. **HTTPBackend**: an OpenAI-compatible local endpoint, with cooperative
   cancellation.
4. **Native GPU/NPU backends**: placement, thermal profiles, real shared
   weights.

## Non-goals

Training, fine-tuning, distributed inference, model quality, and a general
public API. Those are backend concerns and later milestones.
