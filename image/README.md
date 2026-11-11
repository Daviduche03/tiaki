# Image

Build, sign, and update the OS image.

Planned contents:

- `build/` — reproducible image build (rootfs + kernel + recovery core)
- `sign/` — signing keys and provenance
- `update/` — A/B staging, health checks, kexec fast reboot
- `rescue/` — signed rescue image

Design: [TIAKI.md §15](../TIAKI.md#15-update-model) and [§19](../TIAKI.md#19-boot-recovery--failure-modes).
