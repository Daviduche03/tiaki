# Spec: Sandbox profiles

Canonical definition: [TIAKI.md §9.6](../../TIAKI.md#96-sandbox-profiles).
Implementation: `control/sandbox`.

## Profiles

| Profile | Scope |
|---|---|
| `inspect` | read-only: state API, logs, UI tree |
| `user-write` | acting user's profile, home, UI tree |
| `system-write` | service configs, packages, system overlays |
| `privileged` | mediated kernel access via the broker |

Profiles are ordered `inspect < user-write < system-write < privileged`. A
transaction that touches several resources runs at the widest profile it
needs. The capability broker maps an action and path to a class:

| Action | Path | Class |
|---|---|---|
| `state.read` | any | `inspect` |
| `shell.exec` | any | `user-write` |
| `state.write`, `state.revert` | `user/**` | `user-write` |
| `state.write`, `state.revert` | anything else | `system-write` |
| `service.manage`, `plugin.manage` | any | `system-write` |
| `system.admin` | any | `privileged` |

## Resolution

```
capability → sandbox class → profile → provisioner spec → process
```

Access mode changes approval policy and capability scope. It never changes
whether a sandbox exists: full access is a wider sandbox, not the absence of
one.

## Mechanisms

- namespaces (user, mount, PID, network, cgroup)
- Landlock (filesystem), seccomp (syscalls)
- cgroup v2 (CPU, memory, IO)
- no raw root in any profile
- recovery core unreachable from all profiles

## Provisioner ABI

```rust
trait SandboxProvisioner {
    fn create(&self, spec: SandboxSpec) -> Result<SandboxHandle, SandboxError>;
    fn destroy(&self, handle: &SandboxHandle) -> Result<(), SandboxError>;
    fn status(&self, handle: &SandboxHandle) -> Result<SandboxStatus, SandboxError>;

    // Optional; default to Unsupported so a backend cannot silently accept a
    // weight it cannot enforce.
    fn set_shares(&self, handle: &SandboxHandle, shares: u32) -> Result<(), SandboxError>;
    fn read_shares(&self, handle: &SandboxHandle) -> Result<u32, SandboxError>;
}
```

`SandboxSpec` carries the principal, profile, command and args, read-only and
read-write mounts, mediated capabilities, resource limits (CPU shares, memory,
PIDs, network), and a timeout. The ABI is implementable in-process (Rust) or as
an external service over IPC; the existing Go provisioning layer plugs in as
the external implementation.

Two implementations ship:

- **`CgroupProvisioner`** (`control/sandbox/src/cgroup.rs`) — real cgroup v2.
  `cpu_shares` becomes `cpu.weight`, `memory_bytes` becomes `memory.max`,
  `pids` becomes `pids.max`. It consumes a delegated subtree
  (`/sys/fs/cgroup/tiaki`, or `TIAKI_CGROUP_ROOT`) and never mounts one.
  `probe` reports not-mounted, not-writable, and undelegated-controller cases
  with a reason; `create` fails rather than running unweighted.
- **`SimulatedProvisioner`** — records the spec and the weights it was asked
  to apply, reports `Running`, and starts no process. It lets the control
  plane and CLI be exercised everywhere, and records shares so tests can
  assert the supervisor really calls `set_shares`.

`agentd sandbox probe` reports which backend is available; `agentd sandbox
bench` measures the split two weights actually produce (see
[agents.md](agents.md#measured)).

## Shell tool

`ShellTool` turns a `ShellRequest` (principal, profile, command, args,
network flag, timeout) into a `SandboxSpec` with profile-scoped mounts:

- `inspect`: `/` and the state API read-only.
- `user-write`: `/usr`, `/etc`, state read-only; the acting user's home and
  `/run/tiaki/user` writable.
- `system-write`: `/usr` read-only; `/etc`, `/var`, `/run/tiaki/system`
  writable.
- `privileged`: `/` read-only; only `/run/tiaki/broker` writable, with the
  `broker.mediated` capability and network off. Privileged work is brokered,
  never raw root.

## Escape handling

An escape is a security event: log, quarantine session, revoke capabilities, notify the user. Never a silent mode transition.
