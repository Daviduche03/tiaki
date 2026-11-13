# Tiaki agent center (compositor-independent)

The Omarchy agent-center ideas, without the Omarchy chrome: one default
coding agent, per-provider subscription accounts with usage limits, and a
launcher that starts the right harness with the right flags — all plain
scripts on PATH, no compositor, no menu server, no installer framework.

## What's here

- `tiaki-agent [--inline] [--pick] [--agent <id>] [--prompt ...]`
  launches the default agent (or the named one) in foot via
  `tiaki-launch-tui`, with each harness's unattended flags. `--pick`
  offers a numbered list on stdin. Account pins (`CLAUDE_CONFIG_DIR`
  etc.) travel in the command's environment.
- `tiaki-default-agent [<id>]` prints or sets the default (`~/.config/tiaki/defaults/agent`).
- `tiaki-agent-account-{state,list,use,add,remove,rename,home,mode}`
  manages Claude/Codex/Grok subscription accounts: linked homes that
  share everything but the login, a JSON registry with an active account,
  manual/auto switching at a usage threshold. State lives in
  `~/.local/state/tiaki/agents` (fresh path; nothing migrates).
- `tiaki-agent-usage-update` + `tiaki-agent-usage-*` collectors + `lib/collectors.py`
  regenerate per-agent usage JSON under `state/tiaki/agents/usage`.
- `tiaki-agent-prompt`, `tiaki-agent-instructions` (shared `~/.agents/AGENTS.md`
  linked into each harness), `tiaki-cmd-present/missing`,
  `tiaki-launch-tui` (sched-launch + foot), `tiaki-launch-browser`
  (xdg-settings default, `--private` per browser family),
  `tiaki-launch-config-editor` (`$EDITOR` in a terminal).

## Deliberate gaps (from the Omarchy original)

- No auto-install of agent CLIs (no mise on Tiaki): setting an uninstalled
  agent as default errors with an install hint instead.
- No local-model overlay (`--local`/`--prefer-local` dropped): needs a
  local-model service Tiaki doesn't have yet.
- No click-to-switch toast action: `tiaki-notify` takes no `--exec`;
  the registry switch still happens, the toast just can't offer it.
- No OpenClaw/Hermes installers, no `agent make` starter prompts, no
  panel `@@tiaki` event consumer yet (the markers are emitted, nothing
  reads them — the future shell can).
- Launch table flags are ported verbatim and **not runtime-verified**
  per harness (no credentials on the rig); only the missing-binary path,
  the picker, get/set default, account registry ops, and usage
  collection on empty state are tested. See test log below.

## Test log

- `bash -n` clean on all 17 bash scripts; `py_compile` clean on the
  24 python collectors + registry + shared lib.
- Rig (Sway, headless-safe paths only): `tiaki-default-agent` get/set
  round-trips; `tiaki-agent --agent bogus` errors; `--pick` picks from
  piped input; `account-state list` shows primaries unsigned-in;
  `usage-update` writes empty-but-valid records for all 22 collectors.
