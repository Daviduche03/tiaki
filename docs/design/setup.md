# Tiaki — first-run setup

Five screens, one question each. Runs inside the greet flow when
`~/.config/tiaki/setup-done` is missing; finishing writes the flag and hands
over to the sign-in screen.

## Screens

1. **Welcome** — one promise, one button. `Begin`.
2. **Principal** — name field (default Ada). Becomes `user:<name>`, owner.
3. **Default mode** — Guarded (recommended) or Full access. Cards, `1`/`2` or
   click, `up`/`down` also move.
4. **Inference** — On this machine, Remote endpoint (with URL field), or Not
   now.
5. **Review & finish** — the four changes as ledger rows. `Finish setup`
   writes them, then a Done screen reports `N of 4 transactions recorded`
   and returns to sign-in on `return`.

Keys everywhere: `return` primary action, `esc` back.

## What finishing writes

One ledger transaction per answer (best effort each), via
`agentd --principal user:ada apply`:

| Key | Value (defaults) |
| --- | --- |
| `user/<name>/profile/name` | display name |
| `user/<name>/session/mode` | `guarded` / `full` |
| `user/<name>/inference/backend` | `local` / `remote[:url]` / `later` |
| `user/<name>/setup/generation` | `a` |

Keys are user-scoped: a principal can write its own state, not the system
scope. Then `~/.config/tiaki/setup-done` is written so setup never runs again.

## Proven

- All five screens render live on the guest with correct state (contents rail
  marks done/here per screen).
- All four transactions commit with these exact keys (verified individually
  against the control plane).
- Greet→setup chain on cold boot (no flag → setup Welcome) and setup→greet
  handoff (flag present → login straight away).

## Known limits

- The `Finish setup` button path itself is not clicked through on the guest:
  this rig cannot deliver input (pointer or keyboard) to layer surfaces
  reliably — see below. The writes it performs are verified; the wiring is
  the same `connect_clicked → primary() → finish()` pattern as the Continue
  buttons, which do advance screens.
- Test-rig input notes (headless QEMU + wayvnc, no input devices):
  `seat0` reports zero devices; wayvnc's virtual keyboard is added/removed per
  VNC connection (observed in Sway's input events), so VNC keys land only
  intermittently; `swaymsg cursor press/release` moves the cursor but does not
  reliably activate GTK buttons. On hardware with a real keyboard, the
  `Exclusive` grab owns input deterministically. Re-verify input on hardware.
