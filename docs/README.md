# Documentation

- [Architecture source of truth](../TIAKI.md)
- [ADRs](adr/) — decisions that shaped the design
- [Specs](specs/) — per-subsystem contracts

## ADRs

| ID | Title | Status |
|---|---|---|
| [0001](adr/0001-build-on-linux-kernel.md) | Build on the Linux kernel | accepted |
| [0002](adr/0002-full-access-guaranteed-undo.md) | Full agent access with guaranteed undo | accepted |
| [0003](adr/0003-sandboxing-orthogonal-to-mode.md) | Sandboxing is orthogonal to access mode | accepted |
| [0004](adr/0004-explainability-layer.md) | Explainability layer | accepted |
| [0005](adr/0005-base-image.md) | Base image: Debian minimal as substrate | accepted |
| [0006](adr/0006-keyboard-first-desktop.md) | Keyboard-first desktop, Omarchy as inspiration | accepted |
| [0007](adr/0007-step-driven-inference-abi.md) | Step-driven inference ABI | accepted |
| [0008](adr/0008-trusted-input-approval.md) | Trusted input gates high-risk computer use | accepted |
| [0009](adr/0009-invisible-reboot.md) | Invisible reboot with generation rollback | accepted |

## Specs

| Doc | Covers |
|---|---|
| [transaction.md](specs/transaction.md) | Transaction lifecycle and schema |
| [validation.md](specs/validation.md) | Preflight gates, outcomes, validator ABI |
| [capability.md](specs/capability.md) | Principals, grants, policy evaluation, persistence |
| [plugin-manifest.md](specs/plugin-manifest.md) | Plugin manifest, signatures, lifecycle, hot-swap, config |
| [sandbox-profiles.md](specs/sandbox-profiles.md) | Sandbox profiles and provisioner ABI |
| [ui-tree.md](specs/ui-tree.md) | UI tree schema, edit ops, keymap, renderer |
| [inference.md](specs/inference.md) | Inference broker, backend ABI, scheduling, isolation |
| [runtime-abi.md](specs/runtime-abi.md) | Runtime protocol, app trees, trusted input, computer use |
| [generations.md](specs/generations.md) | Generation table, watchdog, kexec, health probes, session restore |
