# ADR 0006 — Keyboard-first desktop, Omarchy as inspiration

**Status:** accepted · 2026-11-07

## Context

Omarchy is an opinionated Arch + Hyprland setup: tiling windows, Super-key
bindings, a launcher, one CLI for themes and keybinds. It demonstrates that a
keyboard-driven desktop is fast, legible, and pleasant. It is also a curated
configuration bundle on top of another distribution, which is not our stack.

## Decision

Adopt the **interaction model**, not the implementation:

- Keyboard-first defaults: Super for window and workspace actions, a command
  palette on Super+Space, focus movement on Super+Arrows, workspaces on
  Super+1..9.
- One source of truth for bindings: a keymap resource, not a compiled config.
- Named actions (`focus.left`, `workspace.3`, `theme.toggle`) as the stable
  vocabulary shared by the user interface and the agent.
- The keymap is a transactional resource under `<scope>/input/keymap`, so
  rebinding is attributable and reversible.

We do not adopt Hyprland, Arch, Waybar, or Omarchy's config format. Rendering
stays declarative and from-scratch; the backend is a host-side renderer now and
a Wayland compositor later.

## Consequences

**Positive**

- Keyboard-first is a strong default for an agent-operated OS: actions are
  discrete, named, and testable.
- Because actions are named, an agent can invoke `focus.left` directly instead
  of synthesizing a keystroke. This is safer, more attributable, and works
  without a visible input device.
- Rebinding is a normal transaction, so a bad keymap can be reverted like any
  other change.
- The same action catalog can feed the command palette, the structured UI tree,
  and future computer-use tooling.

**Negative / costs**

- We must keep the action namespace stable; renaming an action breaks bindings
  and agent prompts.
- Keyboard-first alone is not accessible to every user. Pointer and assistive
  input remain first-class; the keyboard model is a default, not a requirement.
- Some actions (session exit, terminal launch) are platform-specific and remain
  stubs until the session tier exists.

## Alternatives considered

- **Ship Omarchy/Hyprland as the desktop.** Rejected: it owns configuration and
  update semantics, conflicts with transactional generations, and imports a
  distribution we are not building on.
- **No keyboard model until the compositor exists.** Rejected: the keymap and
  action vocabulary are design decisions we can settle now, and they shape the
  UI tree and computer-use API.
- **Compiled keybindings in a config file.** Rejected: not attributable, not
  reversible, and not available to the agent as structured data.
