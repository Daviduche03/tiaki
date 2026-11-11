# Spec: Multi-agent supervision

Canonical definition: [TIAKI.md §12–13](../../TIAKI.md#12-layer-5--inference-broker).
Implementation: `control/agents`, `agentd agents demo`.

## Priority ladder

Highest first:

1. **Local inference.** Owned by the inference broker, not scheduled here.
   While a burst is active the session pool is capped at a 128-share floor;
   agents yield automatically and resume when the burst clears.
2. **Agent sessions.** Starting a session automatically boosts it: shares move
   from ordinary apps into a 640-share session pool. No grant, no approval —
   the boost is policy, and the ledger records it.
3. **Foreground apps.** Interactive work keeps five eighths of the remainder.
4. **Background work.** Throttled while agents run, never below 128 shares
   in total.

Pools are minimum guarantees under contention, not caps: with no apps to
contend with, the session pool takes the whole remainder. Within a pool, shares
split evenly (remainder to the lowest ids first), so the allocation is
deterministic. Every pool configuration sums to 1024 shares.

## Registry

`RuntimeRegistry` holds the runtimes the OS can reach. `opencode` and `codex`
are built in; anything else arrives as TOML under `$TIAKI_HOME/runtimes/`.
Every spec must speak `tiaki-abi/1`, declare a transport and at least one
tool, and use an unclaimed id. Bad files produce warnings, never failures.

## Supervisor

`Supervisor` owns sessions (`{id, principal, runtime, app, sandbox spec}`)
and app processes. Admission requires three things:

- a known runtime id,
- the host's `computer.use` check on `app/<id>` for the session principal
  (agents borrow their user's authority; the supervisor never trusts the
  runtime's word),
- at most two concurrent agent sessions per principal, mirroring the
  inference broker's default.

Each session runs in a `user-write` sandbox whose spec carries the rebalanced
`cpu_shares`, so a real provisioner can apply the weights (cgroup shares,
cpuset) instead of simulating them. Every transition — start, stop, yield,
resume — emits an event with a one-line summary, and `agentd` records the
semantic ones on the ledger. Prompt content and tool payloads are never
stored.

## Knowledge-work apps

The demo drives a `docs` surface through the same `computer.use` gate as the
checkout app. The control plane does not distinguish code work from knowledge
work: both are structured app trees over `tiaki-abi/1`, so any runtime can
drive documents, notes, research, and approvals once the app exposes them.
The T3code-fork UI with knowledge-work features is a separate desktop
workstream that consumes this protocol; it is not part of the sim.

## CLI

```sh
agentd --principal system grant user:ada computer.use 'app/docs'
agentd --principal user:ada agents demo
agentd sandbox probe     # is the kernel able to enforce this?
agentd sandbox bench     # measure the split two weights actually produce
```

`agents demo` registers one foreground and two background apps, starts an
opencode session (automatic boost), starts a codex session (the session pool
splits — the second runtime only has to prove it can run alongside the
first), raises and clears an inference burst (agents yield to 128, then
resume), stops both sessions (shares restored), and records each transition.

## Live launches

`sched-launch` connects the policy to the desktop. Every app the session
starts goes through it:

```sh
sched-launch --class foreground -- foot
sched-launch --class background -- tiaki-wallpaper init
sched-launch --class session     -- opencode-abi.py user:ada
```

No daemon holds scheduler state. Each launcher creates a cgroup whose name
carries its class (`session-<pid>`, `fg-<pid>`, `bg-<pid>`), moves its child into
it, recomputes every sibling from the live directory names, writes the weights,
and removes the cgroup when the child exits. Restarting the session restarts
the scheduler; nothing can drift out of sync.

`Sway` starts the bar, dock, chrome, terminal, editor, wallpaper, and VNC
through it (see `desktop/wayland/sway.conf`), and `ui/shell` launches dock and
launcher apps through it too. If the launcher is absent (host dev builds) the
shell starts apps plainly.

When the kernel cannot enforce — no delegation, read-only cgroupfs, missing
controller — the launcher prints one line and runs the command unsupervised.
It never claims a weight it did not set.

### Bursts reach running processes

`agentd inference burst on|off` writes the broker's flag and immediately
re-weights every live sandbox, so a running agent yields without being
restarted. Verified on live processes:

```
1) one app + one helper:      fg 62, bg 37
2) an agent starts:           agent 62, fg 23, bg 14   (apps re-weighted in place)
3) inference burst:           agent 12, fg 23, bg 14   (agent yields)
4) burst cleared:             agent 62, fg 23, bg 14   (agent resumes)
```

Measured CPU for agent vs background on one pinned core: **1.67:1 observed,
1.68:1 expected** from weights 62:37.

## Kernel mapping

Shares are not bookkeeping. `control/sandbox/src/cgroup.rs` implements
`SandboxProvisioner` against cgroup v2:

| Field | cgroup file |
|---|---|
| `cpu_shares` (0–1024) | `cpu.weight` (1–10000; 1024 shares = 100, the kernel default) |
| `memory_bytes` | `memory.max` |
| `pids` | `pids.max` |

The supervisor provisions a cgroup per session, writes the share weight, and
re-writes it on every rebalance (boost, split, yield, resume). `destroy` kills
anything left and removes the cgroup. `read_shares` reads the weight back, so
a caller can check the kernel rather than the planner.

`probe` is explicit about failure: cgroup v2 not mounted, root not writable,
or the `cpu` controller not delegated each produce a reason. Admission then
fails rather than running an unweighted session. `agentd agents demo` picks
the real backend when the probe is ready and the simulated one otherwise,
printing which one ran.

This module never mounts a hierarchy. On a real system init delegates
`/sys/fs/cgroup/tiaki` (`TIAKI_CGROUP_ROOT` overrides it); in a container
the harness does the same. A provisioner that mounted hierarchies would be
claiming privileges it may not have.

## Measured

`agentd sandbox bench` is the evidence command. It pins a busy loop in each of
two cgroups to one CPU (the cpuset controller when delegated; otherwise it
saturates the machine) and reads `cpu.stat`.

Observed in a privileged Linux container:

```
agent      weight 62 -> 2251862 us
background weight  7 ->  258322 us
observed ratio: 8.72:1   expected: 8.86:1
```

Materially different weights produce a materially different split. That is
the difference between this and the earlier share table.

## Non-goals

GPU placement, thermal policy, IO and memory pressure tuning beyond the
declared ceilings, and multi-principal contention (quotas stay per-principal
caps, as in the broker). Mounting hierarchies stays with init: the provisioner
consumes a delegated subtree, it does not claim privileges to create one.
