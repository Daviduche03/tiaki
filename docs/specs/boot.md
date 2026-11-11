# Spec: boot, generations, and install

Canonical definition: TIAKI.md (generations, A/B slots, rollback).
Implementation: `desktop/systemd/*`, `desktop/install/*`, `core/boot`,
`agentd generation`.

## Model

Two identical root generations (A and B), each carrying its own kernel,
ledger, and OS payload. `/home` is shared. New system content is staged
with `agentd generation update <dir>` and marked the boot candidate;
rebooting into it (via Limine, or `kexec -e` for a fast jump) makes it
current, and `agentd generation rollback` + reboot restores the other slot.
The running system is never mutated in place, so a bad update cannot strand
the machine.

On-disk layout (see `desktop/install/install.sh`):

| Partition | Label | Content |
|---|---|---|
| p1 | EFI | Limine + ESP |
| p2 | tiaki-boot-a | kernels for A |
| p3 | tiaki-a | root A (`/`) |
| p4 | tiaki-boot-b | kernels for B |
| p5 | tiaki-b | root B (`/`) |
| p6 | tiaki-home | `/home`, shared |

Kernels live on per-generation XBOOTLDR partitions beside the roots they
boot; systemd-boot discovers both slots with no GUID templating.
`desktop/install/install.sh` writes one entry per slot (plus the other
slot's entry, so either boot menu can reach either generation). `fstab`
mounts by label, so each generation finds its own boot partition.

## systemd

PID 1 is systemd. Three units, all enabled in the image:

- `tiaki-firstboot.service` (oneshot): seeds control-plane state on a
  fresh `$TIAKI_HOME` and delegates the cgroup subtree. Best-effort on
  delegation — read-only mounts keep the advisory fallback.
- `tiaki-ui-serve.service`: `agentd ui serve`, restart-always.
- `tiaki-session.service`: the Sway session as the `desktop` user.
  Deliberately no `PAMName`: logind would override `XDG_RUNTIME_DIR` with
  `/run/user/$UID` and split the session sockets away from `/run/tiaki`
  where every client looks.

`desktop/wayland/run-wayland.sh` remains for legacy direct launches.

## Reboot cycle (harness in `desktop/qemu/`)

`agentd generation update <staged-dir>` captures the session, stages the
target, and marks it the boot candidate; rebooting (via Limine, or `kexec -e`
as a fast path where the platform allows) makes it current, and each boot
records a generation event on the ledger. Rollback is
`agentd generation rollback` (or picking the other slot in the Limine menu),
followed by a reboot.

`desktop/qemu/harness.sh` drives the loop against a Debian ARM64 cloud
guest (HVF): NoCloud seed with an admin key, overlay qcow2 on the base
image, SSH in, `guest-phase1.sh` (PID 1, probe, gate, staging), full
reboot, `guest-phase2.sh` (boot_id changed, ledger continuous). Exit 0
only if every guest check prints PASS. `AGENTD_BIN` points at an ARM64
`agentd` build.

Kexec note: on this Mac (Apple HVF, Debian 6.12 cloud kernel) a kexec jump
panics the new kernel deterministically — PID 1 dies at +8s inside a BPF
syscall on what reads as corrupted memory, across two cmdline variants
(`--reuse-cmdline` and explicit cmdline + DTB); `kexec_file_load` is
additionally unavailable in this kernel build. Full UEFI reboot works fine,
so the harness reboots instead: the generation and ledger assertions are
identical, and only the jump mechanism differs. Re-validate the kexec fast
path on KVM/bare metal, where the CPU state handoff it depends on is real.

## Installer

`desktop/install/install.sh <disk> [--arch amd64|arm64]` partitions,
debootsraps Debian, clones A→B bit-identically, lays the staged payload
(`make payload`: binaries, units, tree, icons, wallpapers), and writes
Limine with real GUIDs. Run from the repo root after `make payload`.
`sh -n` clean; live-fire under QEMU is the remaining proof (the harness
below boots guests, but the installer itself has not yet installed one —
that run is the next commit on this track).

## Non-goals

Secure Boot key enrollment, TPM-measured boot, encrypted `/home` by
default, and installer partitioning beyond the fixed six-partition layout.
Those arrive with hardware, not before it.
