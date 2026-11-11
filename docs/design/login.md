# Tiaki — sign-in screen

No password. The screen chooses *who*, not whether. One principal per row,
`return` to enter, `g` to flip the mode.

## Layout

- Full wallpaper (`photo-03-mist-pines`), no blur, 44% ink veil
- Clock top-center in the system face, mono date under it
- Bottom-center: beacon mark, name, `return` kbd hint
- Mode as one mono line: `guarded · g for full access`
- `gen a` bottom-left, `sleep · shut down` bottom-right, text only
- No cards, panels, buttons, or red. Multi-user is the same screen with more
  rows (`up`/`down` already walks the list).

## How it runs

Sway autostarts only wallpaper, wayvnc, and `tiaki-shell greet`
(`desktop/wayland/sway.conf`). The greeter owns the screen; on confirm it:

1. Writes `$XDG_RUNTIME_DIR/tiaki-session` (`user:ada guarded`) — the session
   contract every later surface reads.
2. Signs the sign-in into the ledger (`agentd apply user/<name>/session/mode`),
   best effort. The key is user-scoped: a principal can write its own session
   state, not the system scope (`session/mode` is rejected).
3. Spawns bar, dock, chrome, and demo apps through `sched-launch`, then exits.

Keys: `return` confirm, `g` mode, `up`/`down` principal, `s` suspend,
`delete` poweroff.

## Proven

Cold boot on QEMU: sway → greeter → `return` → desktop with bar, dock,
wallpaper, and demo apps. `tiaki-session` written, ledger transaction
committed, all verified live over VNC/serial.

## Known issues

- `sched-launch` placement fails with `Permission denied` when the parent is
  outside the delegated cgroup subtree (manual launches from an admin shell).
  At boot it attaches cleanly. Children still launch unsupervised on failure;
  placement, not launching, is what breaks.
- Confirm is slow on emulated CPU (agentd apply + 5 spawns): allow 30s+ before
  judging a keypress lost.
- VNC key delivery is the test path, not the product path; on hardware the
  `Exclusive` keyboard grab owns the keys directly.
