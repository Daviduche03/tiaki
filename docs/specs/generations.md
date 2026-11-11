# Spec: Generations, boot, and session restore

Canonical definition: [TIAKI.md §15 and §19](../../TIAKI.md#15-update-model).
Implementation: `core/rollback`, `core/boot`, `core/init`.

## Generations

A generation is an immutable OS image. The generation table is recovery-core
data, stored at `$TIAKI_HOME/generations/table.json`:

| Field | Meaning |
|---|---|
| `active` | the running generation |
| `last_good` | the last generation that passed health |
| `candidate` | the generation staged for the next boot |
| `boot_id` | the current boot identifier |
| `max_attempts` | watchdog threshold (default 3) |
| `generations` | id → `{label, created_at, image, digest, status, attempts}` |

Statuses: `staged`, `candidate`, `active`, `last-good`, `failed`, `retired`.

## Lifecycle

```
stage → candidate → boot → health → commit
                        └── fail → rollback to last-good (or rescue)
```

- **Stage** copies the image under `generations/<id>/image` and records its
  `sha256:` directory digest.
- **Candidate** marks it as the next boot target.
- **Boot** increments the attempt counter and returns a `BootDecision`.
- **Commit** promotes the candidate to active and the previous active to
  last-good. It refuses a candidate that has not passed a health check.
- **Rollback** marks the candidate failed and makes last-good active. With no
  last-good, it returns `Rescue`.

## Watchdog

`record_boot` counts attempts. If attempts exceed `max_attempts`, the table
rolls back to last-good (or requests rescue). A successful health check clears
the counter. A boot that crashes before reporting healthy therefore triggers a
rollback on the next attempt.

## Boot chain

```rust
pub trait Kexec {
    fn name(&self) -> &str;
    fn boot(&mut self, image: &BootImage) -> Result<BootTiming, KexecError>;
}
```

`boot_generation` re-hashes the image directory and compares it with the
digest recorded at staging. A mismatch is refused before kexec.

- `SimulatedKexec` records the boot with a configurable simulated downtime.
- `LinuxKexec` runs `kexec --load` and `kexec -e`. It refuses to run unless
  `TIAKI_ALLOW_KEXEC=1`, because `kexec -e` replaces the running kernel. Off
  Linux it returns an explicit unsupported error.

Images with symlinks are rejected. The digest covers sorted relative paths and
each file's SHA-256.

## Health probes

```rust
pub trait HealthProbe {
    fn name(&self) -> &str;
    fn check(&self, context: &BootContext) -> Result<(), String>;
}
```

`BootHealth` runs every probe and reports each result; any failure fails the
boot. The daemon wires:

| Probe | Checks |
|---|---|
| `session` | the snapshot parses |
| `ui-tree` | the tree parses and validates |
| `keymap` | the keymap parses and validates |
| `inference-registry` | every registered model spec parses |
| `simulated-candidate` | injected by `--fail` to demo auto-revert |

`BootContext` carries the generation, label, boot id, rendered session
snapshot, a state-store snapshot, and free-form metadata.

## Session snapshot

Stored at `$TIAKI_HOME/session/snapshot.json`:

```json
{
  "version": 1,
  "boot_id": "boot_...",
  "captured_at": 1760000000,
  "user": "user:ada",
  "ui_tree": "{...}",
  "keymap": "{...}",
  "theme": "dark",
  "focus": "place",
  "workspace": 1,
  "apps": [
    { "app": "checkout", "values": { "gift": "on", "place": "placed" }, "focused": "place" }
  ],
  "pending_transactions": []
}
```

`capture` refreshes the state-store fields while preserving apps, focus, and
workspace. A system-driven update captures the *user's* session, not the
system's, when a snapshot already exists.

`restore` re-applies the UI tree, keymap, and theme as ordinary transactions on
the snapshot's principal, prints the app state, then verifies that each stored
value matches. Pending approvals are preserved in the ledger. The result is
"the same session, new boot id": a repaint, not a logout.

## Update pipeline

```
capture session → stage image → mark candidate → boot + verify digest
   → health probes → commit or rollback → restore session
```

`agentd generation update <dir>` runs the whole pipeline. `--fail` injects a
failing probe so auto-revert is visible.

## Authority

| Action | Scope | Default holders |
|---|---|---|
| `generation.manage` | `system/generations/**` | `system`, `service:update` |

A user cannot update or roll back the OS. Every stage, boot, commit, and
rollback is recorded on the ledger; boots are `committed` or `reverted`.

## CLI

```sh
agentd session capture | show | restore | clear
agentd generation list | status | history
agentd generation stage <dir> [--label <text>]
agentd generation update <dir> [--fail] [--downtime-ms N]
agentd generation boot [--fail] [--downtime-ms N]
agentd generation commit | rollback
```

## Non-goals

The image build pipeline, bootloader/firmware, secure boot, and real kexec
execution on development hosts are out of scope. `LinuxKexec` is the seam; the
default is `SimulatedKexec`. App process handoff is represented by the session
snapshot, not by a live process migration.
