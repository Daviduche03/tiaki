# Compositor spike: Hyprland + Quickshell on the ARM rig

Date: 2026-11-12. Question: can Hyprland run in our ARM64 QEMU dev loop,
so we can adopt Omarchy's stack (Hyprland + Quickshell) instead of
migrating to Omarchy the distro? Verdict: **yes, with caveats**.

## Result

- Hyprland 0.57 (Debian sid, arm64) composites on this VM: KMS output
  `Virtual-1 1280x800`, foot tiled with gaps/borders/rounding, all on
  llvmpipe software GL (`kms_swrast` fallback after dri2 screen fails).
- Quickshell 0.3.2 runs and renders a QML top bar as a real layer-shell
  surface (`1280x30`, namespace `quickshell`).
- Proven visually via Hyprland's own screencopy (`grim`), not VNC.

## What did not work

- **Nested Hyprland inside Sway crashes** (`CBackend::create() failed`).
  Aquamarine tries headless (mandatory) → DRM (if-available) → Wayland
  (fallback), then needs a GBM allocator from any backend with a drmFD.
  Headless provides none; DRM fails (no seat/logind for master, no virgl
  for EGL on the node); the nested Wayland backend fails because
  pixman-Sway advertises no `zwp_linux_dmabuf_v1`, which Aquamarine
  requires. Net: Hyprland must own the DRM master, i.e. be the session
  compositor, not a nested window. Dev-loop nested testing is out.
- **wayvnc on Hyprland showed grey frames** while grim captured fine.
  Frames exist; the VNC/screencopy path needs work before remote dev
  works on a Hyprland session.
- **Performance unmeasured.** llvmpipe does all rendering on CPU.
  Static content is fine; animation FPS under load is unknown and is the
  biggest remaining risk for "Omarchy-grade smoothness" on this VM.
  (Production amd64 hardware with a real GPU would not share this risk.)

## Notes that matter for the port

- Hyprland ≥0.55 speaks **Lua config**; legacy hyprlang is degraded
  (0.57 parses `.conf` as Lua and fails on `exec-once`). Write Lua.
- Quickshell needs the `qt6-wayland` QPA package for shell-integration
  plugins; the client lib alone crashes it at startup.
- Neither package exists in trixie; both come from sid today
  (`hyprland 0.57.0+ds-1`, `quickshell 0.3.2-1`). The rig is now a
  trixie/sid hybrid (libc 2.43, libstdc++ 16, Qt 6.11). Our Rust/GTK
  shell is unaffected (no Qt linkage), and the system stayed healthy.
- Rig fixes made along the way (guest-local, re-apply in image build):
  `/etc/resolv.conf` → `nameserver 10.0.2.3` (systemd stub was dead),
  guest clock +2 days (sid signatures rejected "not yet valid").

## Recommendation

Technically viable. Before committing: (1) FPS under animation on
llvmpipe, (2) wayvnc-on-Hyprland for remote dev, (3) session design —
Hyprland replaces Sway as the session compositor, Rust daemons unchanged,
then port Omarchy's QML shell conventions (MIT) re-skinned to our tokens.
