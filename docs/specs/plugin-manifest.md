# Spec: Plugin manifest & host

Canonical definition: [TIAKI.md §10](../../TIAKI.md#10-layer-3--plugin-runtime).
Implementation: `control/plugin-host`.

## Required sections

- `[plugin]` — id, name, version, api, kind, entry
- `[capabilities]` — required and optional grants
- `[resources]` — cpu weight, memory cap
- `[update]` — channel, signature

## Optional sections

- `[ui]` — tree and schema paths
- `[config]` — config schema and migrations

## Kinds and swap tiers

| Kind | Tier | Swap semantics |
|---|---|---|
| `ui`, `tool` | content | live hot-swap |
| `service`, `runtime`, `backend` | session | restart with state continuity |
| core modules (host itself) | core | generation boundary / kexec |

The tier is derived from the kind. Content-tier plugins hot-swap with no
restart; session-tier plugins report `restart-required`.

## Grammar

Manifests are a strict TOML subset: tables, strings, integers, booleans, and
flat arrays of strings or integers. Dotted keys, inline tables, multiline
strings, floats, and datetimes are rejected. This keeps the manifest surface
small enough to audit.

## Semantic rules

1. `id` is dotted lowercase segments (`[a-z0-9-]`), at least two segments.
2. `version` and `api` parse as `MAJOR.MINOR.PATCH`; `MAJOR.MINOR` is accepted
   and fills `PATCH = 0`.
3. `api.major` must equal the host's `HOST_API_MAJOR`.
4. `entry`, `ui.tree`, `ui.schema`, `config.schema`, and `config.migrations`
   must be relative paths that exist inside the plugin. `..` and absolute
   paths are rejected.
5. `update.signature` must be present.
6. `cpu_weight` must be positive.
7. Any validation error blocks install; warnings do not.

## Signatures

The host takes a `SignatureVerifier`:

```rust
trait SignatureVerifier {
    fn verify(&self, root: &Path, manifest_text: &str, signature: &str) -> SignatureStatus;
}
```

`DigestVerifier` verifies `sha256:<hex>` over the manifest (with the signature
line removed) plus every asset, addressed by relative path and hashed
individually. `PresenceVerifier` accepts any non-empty signature as
`Unverified`; it is only used with `--allow-unverified`.

Production uses `ed25519:`; the verifier ABI is the seam where it lands.
Unverified plugins are refused unless the caller explicitly allows them.
Tampering with any asset after signing changes the digest and is rejected.

`agentd plugin sign <dir>` prints the digest for a plugin directory.

## Install layout

```text
$TIAKI_HOME/plugins/<id>/
  active              # version string
  previous            # previous version, for rollback
  state               # installed | loaded | running | stopped
  config.json         # { "version": "...", "values": { ... } }
  versions/<version>/ # immutable copy of each installed version
```

Installs copy the source into `versions/<version>`. Removing a plugin deletes
the whole `<id>` directory after it is unloaded. Updates keep the old version
for `rollback`.

## Lifecycle

```text
install → installed → load → loaded → start → running
                         ↑                    │
                         └── unload ← stopped ← stop
```

State is persisted, so it survives host restarts. Illegal transitions return
`BadState`.

## Config and migrations

`[config] schema` is the JSON-Schema subset in `control/plugin-host/src/schema.rs`:
object properties, `type`, `enum`, `required`, `default`, and
`additionalProperties`. Unknown keys are rejected when `additionalProperties`
is false. Defaults are seeded on install.

A migration is a JSON file in `migrations/`:

```json
{ "from": "1.1.0", "to": "1.2.0", "set": { "format": "24h" }, "remove": ["legacy"] }
```

On update the host chains migrations from the installed version to the new
one, applies `remove` then `set`, seeds defaults, and validates the result
against the new schema. If migrations are declared but no path exists, the
update is refused. A version bump with no declared migrations carries config
forward unchanged. A failed validation aborts the update before it becomes
active.

Rollback validates the current config against the target version's schema
before switching, so a config that only makes sense on the newer version is
refused rather than silently broken.

## Isolation

The host maps the manifest to a sandbox profile:

| Signal | Profile |
|---|---|
| default `ui` / `tool` | `user-write` |
| default `service` / `runtime` / `backend` | `system-write` |
| `system.*`, `service.*`, `plugin.*` capability | `system-write` |
| `kernel.*`, `system.admin` capability | `privileged` |

Capabilities widen the profile; they never narrow it. `network.*` turns on
network access in the spec. `PluginHost::spec` produces the `SandboxSpec` the
provisioner executes; the host never runs plugin code itself.

## Authority

Plugin lifecycle actions require `plugin.manage` on
`system/plugins/<id>`. By default only `system` holds that grant, so a user or
agent must be granted it explicitly. Every lifecycle action is appended to the
same hash-chained log with principal, mode, sandbox, summary, and status.

## CLI

```sh
agentd plugin list
agentd plugin install <dir> [--allow-unverified]
agentd plugin update <dir> [--allow-unverified]
agentd plugin remove <id>
agentd plugin load|unload|start|stop <id>
agentd plugin swap <id> <version>
agentd plugin rollback <id>
agentd plugin config <id> [key=value ...]
agentd plugin spec <id>
agentd plugin sign <dir>
```

## Example

See [`plugins/example-clock`](../../plugins/example-clock/plugin.toml). Its
signature is a real content digest, so it installs without an override.
