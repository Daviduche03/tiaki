# ADR 0009 — Invisible reboot with generation rollback

**Status:** accepted · 2026-11-07

## Context

Kernel and driver updates cannot be fully live. Function-level live patching
covers a narrow class of changes; anything touching data structures, init code,
or firmware requires a reboot. The product promise therefore must be
**invisible reboot**, not "never reboot": a short kexec into the new image with
the session restored, and an automatic fallback if the new generation is
unhealthy.

## Decision

- **The unit of update is a generation.** An immutable image directory with a
  `sha256:` digest, staged under `$TIAKI_HOME/generations/<id>/image`.
- **The recovery core owns the generation table** (`active`, `last_good`,
  `candidate`, boot attempts). It is data, not code, and lives outside the
  generations it describes.
- **A generation is committed only after a healthy boot.** Health probes run
  against the restored session, UI tree, keymap, and inference registry. Any
  failure rolls back to last-good and marks the candidate failed.
- **The watchdog counts attempts.** A generation that never reports healthy is
  rolled back after `max_attempts` (default 3). With no last-good, the system
  requests rescue rather than booting a known-bad image again.
- **kexec is the fast path.** `kexec --load` + `kexec -e` skips firmware and the
  bootloader. `LinuxKexec` refuses to run without `TIAKI_ALLOW_KEXEC=1`, so a
  development command cannot accidentally reboot the host. Off Linux, a
  simulated backend records the boot.
- **Session restore is part of boot.** The UI tree, keymap, theme, focus,
  workspace, app values, and pending transactions are captured before the
  update and re-applied after. The user sees the same session with a new boot
  id.
- **The image digest is re-verified before kexec.** A tampered image is
  refused even if the table says it is a candidate.

## Consequences

**Positive**

- A bad update cannot brick the running system: last-good is preserved and
  rollback is automatic.
- Downtime is a kexec-sized pause plus service restart, not a firmware boot.
- Session restore makes the change feel like a repaint.
- Generation history is auditable and attributable.

**Negative / costs**

- kexec compatibility is hardware- and kernel-configuration-dependent; some
  devices still need a full reboot.
- The session snapshot is a best-effort handoff. Apps that cannot serialize
  their state must restart at a natural break.
- The health-probe set must be meaningful; a probe that passes on a broken
  image defeats the guarantee.
- Real image building, signing, and bootloader integration remain.
- A compromised recovery core or generation table breaks the guarantee; both
  are in the trust chain.

## Alternatives considered

- **Userspace-only atomic updates.** Rejected as the whole answer: kernel and
  driver changes still need a reboot.
- **Live patching everything.** Rejected: not generally possible for data
  structures or init code.
- **Commit before health check.** Rejected: a broken image would become active
  with no automatic fallback.
- **Manual rollback only.** Rejected: a headless or unattended system can be
  left unbootable.
