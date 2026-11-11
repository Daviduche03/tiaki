# Compositor spike — Sway → ShojiWM

Timeboxed feasibility spike, not a migration. Question: can Tiaki's existing
shell run on [ShojiWM](https://github.com/bea4dev/ShojiWM), and does that
compositor remove the two blockers Sway can never fix?

- **Blocked on Sway:** third-party GTK windows render Adwaita CSD chrome and the
  compositor draws nothing around them; no per-window shape, no window
  animation, no frame colouring.
- **Next Move #6:** "schedule compositor replacement when Sway's ceiling is
  reached."

## Verdict

Yes. The shell needs **no changes** to run under it, and every blocker question
came back positive. The one thing the spike could not measure is idle cost,
because the nested test path cannot idle by design (see below).

## What was run

ShojiWM `main` (smithay fork, Rust), built as the **Rust config runtime**
(`shojiwm_rs` + `examples/default_config`) — no TypeScript, no V8, no JS
anywhere in the process. Built in a Debian 13 arm64 container, run **nested**
(`winit` backend) inside the existing Sway session on the QEMU guest, then
Tiaki's real binaries launched into it on `wayland-2`.

![Tiaki bar inside ShojiWM](tiaki-bar-in-shojiwm.png)

*`tiaki-shell bar`, unmodified, as a layer surface inside ShojiWM. Lamp chip,
`gen a`, tray and clock all live. Mousepad in a ShojiWM-drawn frame.*

## Results

| Question | Result | Evidence |
| --- | --- | --- |
| Builds for Debian 13 / arm64 | **Yes** | 40 MB binary, rust 1.99, ~9 min cold |
| Runs nested inside Sway | **Yes** | own socket `wayland-2` |
| `wlr-layer-shell` | **Yes** | bar mapped as Top layer, 36 px, correct anchors; no `is_supported()` failure |
| `wlr-screencopy` | **Yes** | `grim` inside ShojiWM captured the full output |
| Server-side decoration | **Yes** | ShojiWM draws titlebar, buttons, border, radius around a GTK app |
| Idle cost | **Not measurable here** | nested backend repaints continuously |

### SSD is the point

![SSD frame around a GTK app](ssd-frame-around-gtk-app.png)

The frame is not hardcoded in the compositor — it is a **composition tree the
config draws** (`COMPOSITOR.window.composition`, `main.rs:958`). `WindowBorder`,
`Style::border`, `border_radius`, `box_shadow`. Tiaki would write that config
crate and draw the frame in ink and paper, with the lamp on the focused window
instead of `hex("#d7ba7d")`.

The Rust default config does **not** handle `theme.set` — only the TS config
does — and it hardcodes its colours. That is convenient: there is no M3E to
strip out. The Rust runtime is a blank frame we own.

### Idle is unmeasurable on this rig, and here is exactly why

`backend/winit.rs:1630` calls `backend.window().request_redraw()` at the end of
**every** redraw. The nested backend is a development backend that repaints
continuously. Measured CPU confirms it: 100% at rest, and *lower* (45%) once a
client exists — the loop is not frame-paced.

So the 0%-idle claim is untestable here. It also cannot be tested on this guest
generally: no GPU, software GL on an emulated ARM CPU. The only meaningful
relative number: **ShojiWM 23% idle vs Sway 30% idle**, same guest, no clients.

To test the real claim, run `--tty` on hardware with a GPU and read the FPS
overlay (`Super+Shift+F`).

## Costs found

- Build needs the smithay dev set plus `libxcb-cursor-dev` (winit's X11 path).
- The guest needed four runtime libs it did not have, copied in from the
  builder: `libxcb-cursor`, `libxcb-image`, `libxcb-render-util`, `libxcb-util`.
  All four are X11-only; a Wayland-only build would not need them, and a real
  image installs them from apt.
- The config's asset root is baked at build time (`/src/src/...`), so shaders
  resolve against the build path unless `asset_root` / `--runtime-dir` is set.
- The nested window does not present updates to the outer compositor: VNC shows
  a stale rectangle. Verification has to go through `grim` inside ShojiWM.
  Irrelevant on `--tty`.

## What this changes

The compositor and the shell toolkit are **independent decisions**. The shell
ran unmodified, so moving to ShojiWM does not force or block adopting
AntiWebUI. That decoupling is the main result.

It also means the Adwaita-chrome blocker splits in two:

1. **Frame** — solved by ShojiWM, which draws a frame we author. CSD clients
   keep their own chrome but now sit inside our border, radius and shadow.
2. **The client's own chrome** — still GTK theming. A `gtk-3.0/gtk.css` in Tiaki
   tokens, which is doable on Sway today and is worth doing regardless.

## If we proceed

1. Write a Tiaki config crate on `shojiwm_rs` — the WM is ours, in Rust,
   matching "own the thesis seams". Frame in ink/paper, lamp on focus.
2. `--tty` boot on hardware; measure the FPS overlay at rest before committing.
3. Check `wayvnc` against it (screencopy exists, so it should work) — the
   remote path depends on it.
4. Confirm `ext-workspace` or the config's IPC gives the bar workspace state.
5. Keep Sway as the generation-A fallback. This is exactly what A/B slots are
   for.

## Remote-access note (verified 2026-11-11 on the QEMU rig)

- `tiaki-wm --tty` cannot start on this rig: virtio-gpu's `/dev/dri/card0`
  refuses the open (`FailedToOpenDevice … WouldBlock`). Product `--tty`
  needs real GPU hardware; the harness can only host the nested backend.
- `wayvnc` can never send input to ShojiWM: the compositor implements no
  `zwlr_virtual_pointer_v1` anywhere (checked `shojiwm_lib` + `shojiwm_rs`),
  so it aborts with "Virtual Pointer protocol not supported by compositor"
  unless passed `--disable-input`. Screencopy works, so view-only VNC is
  possible, but remote control of a tiaki-wm session needs upstream virtual
  pointer support. This is now the remote-path blocker, not screencopy.
- Packaging consequence, applied: the desktop user is created in
  `video,render` (`install.sh`) so `--tty` can open DRM on hardware.
