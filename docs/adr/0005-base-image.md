# ADR 0005 — Base image: Debian minimal as substrate, from-scratch on top

- **Status:** accepted
- **Date:** 2026-11-07
- **Supersedes:** none

## Context

Milestone 1 needs a real Linux substrate: principals, capability checks, and sandboxes have to be exercised against an actual kernel, filesystem, and process tree.

The design already says we do not write a kernel, bootloader, or driver stack ([ADR 0001](0001-build-on-linux-kernel.md)). The same logic applies to commodity userspace: rebuilding glibc, coreutils, and a package ecosystem is years of maintenance that does not advance the thesis. "From-scratch userspace" means the parts that define the product — init, control plane, state engine, UI, update and generation model — not a reimplementation of POSIX plumbing.

Constraints that shape the choice:

- **glibc, not musl.** Local inference means CUDA/ROCm, vendor GPU drivers, and prebuilt agent runtimes. musl breaks or complicates all three.
- **Reproducible rootfs.** A generation must be buildable from pinned inputs, not "whatever the mirror had that day."
- **Broad hardware.** v1 targets laptops and desktops, including GPUs and firmware.
- **Our update model, not the distro's.** Generations and rollback belong to the recovery core. The base must not impose its own atomic-update authority.
- **Minimal enough to strip.** The shipping image should be small; the bootstrap image only needs to be workable.

## Decision

Use **Debian 13 "trixie" minimal** as the bootstrap substrate, with a from-scratch control plane on top.

| Concern | Choice |
|---|---|
| Root filesystem | Debian minimal via `mmdebstrap --variant=minbase`, glibc |
| Architectures | `amd64` first, `arm64` second |
| Reproducibility | Pin to `snapshot.debian.org`; record the exact snapshot in the image manifest |
| Kernel | Debian's 6.12 LTS line initially; later pin and sign our own build of the same LTS line |
| Init | systemd as bootstrap init until `core/init` can supervise services |
| Image layout | Read-only system image + writable overlay/state; generations owned by the recovery core |
| Package manager | Build-time tool (`apt`/`dpkg` in the image builder), never a runtime authority |
| Development host | Linux VM (Lima/UTM/QEMU) on macOS; control plane stays cross-platform Rust |

The distro is the **chassis**, not the product. We replace or own init, the control plane, the UI, the update model, and the generation table. We keep libc, the kernel, firmware, and the low-level toolchain.

## Consequences

**Good**

- GPU stacks, firmware, and prebuilt agent runtimes work without a compatibility layer.
- `mmdebstrap` plus snapshot pinning gives reproducible, inspectable images.
- The 6.12 LTS kernel and Debian firmware cover most laptops and desktops.
- No snap, no vendor account, no telemetry, no competing update authority.
- The team can reach Milestones 1–4 without becoming a distro maintainer.

**Costs**

- Debian's package model is not generation-native. Our engine must treat `dpkg` as a build-time input and own generations itself.
- Some packages assume systemd; replacing PID 1 will require auditing that dependency over time.
- Non-free firmware and GPU drivers must be enabled explicitly.
- The bootstrap image is larger than a purpose-built one; shrinking is deferred, not solved.

**Later**

Once the control plane is stable, build the shipping and rescue images with Buildroot or a custom Rust image assembler, keeping the same glibc ABI and kernel line. Buildroot is the right tool for a small, signed, single-purpose rescue image *now*; it is the wrong tool for the daily-driver image *before* the agentic layer works.

## Alternatives rejected

| Option | Why not |
|---|---|
| **Alpine** | musl breaks CUDA/ROCm, vendor GPU drivers, and many prebuilt runtimes; `gcompat` is a band-aid |
| **NixOS** | Closest existing system conceptually, but it is a *competing* declarative model. Its store and language become a second source of truth, and "the agent can change anything" fights purity: imperative changes vanish on rebuild. Steal its generation/rollback UX, not its substrate. |
| **Fedora CoreOS / Silverblue** | `rpm-ostree` owns generations and package layering; container-first; short lifecycle and high churn. We would be building around another system's update model. |
| **Arch** | Rolling release is acceptable for a developer workstation, not for an OS sold to non-technical users. No reproducibility story. |
| **Ubuntu** | Viable fallback if laptop hardware support becomes a blocker (HWE kernels, NVIDIA packaging). Rejected as default because of snap, cloud-init, and Canonical-specific surface to strip. |
| **Buildroot / Yocto now** | Maximum control, but you become a distro maintainer: toolchain, CVEs, GPU drivers, firmware. Years of work before the thesis is tested. |
| **Linux From Scratch** | Educational, not maintainable; no package management or security update path. |

## Notes

- On macOS we cannot boot this image directly. Milestone 1 runs the control plane natively (pure Rust, cross-platform) and exercises sandboxing inside a Debian VM.
- If the Go sandbox provisioner assumes a particular distro or kernel feature, that becomes an input to this decision; the provisioner ABI is designed to absorb it.
