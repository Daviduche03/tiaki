# ADR-0001: Build on the Linux kernel

- **Status:** accepted
- **Date:** 2026-11-07
- **Related:** TIAKI.md §2.4, §4

## Context

The project needs a kernel, driver model, filesystem, networking stack, and hardware support. Writing these from scratch is a decade-scale effort that would consume the budget for the agent layer — the actual differentiator.

## Decision

Build on the Linux kernel. Write the userspace, control plane, UI, and recovery core from scratch. Do not write a kernel or driver stack.

## Consequences

### Positive

- Hardware support, drivers, and filesystems come for free.
- The team focuses on the agent control plane and UI.
- Firmware, suspend, GPU, and Wi-Fi work from day one.

### Negative

- Bound to Linux's ABI and release cadence.
- Kernel-level live update remains limited to live patching.
- Some Linux conventions leak into the design.

## Alternatives considered

- **Redox (Rust microkernel):** elegant, but lacks drivers and hardware support.
- **From-scratch kernel in Rust:** a decade-long project; would starve the product.
