# Example: Clock Panel

A reference plugin showing the full manifest shape:

- `plugin.toml` — manifest
- `ui/clock.ui.json` — declarative UI tree
- `schema/clock.schema.json` — UI tree schema
- `schema/clock.config.schema.json` — config schema
- `migrations/` — config migrations
- `libclock.so` — placeholder for the compiled module

The manifest's `[update] signature` is a real content digest, so this plugin
installs without `--allow-unverified`:

```sh
cargo run -p agentd -- plugin sign plugins/example-clock
cargo run -p agentd -- plugin install plugins/example-clock
```

Re-run `plugin sign` after changing any asset and copy the new value into
`plugin.toml`. Production uses `ed25519:` signatures; the digest verifier is
the development path until the ed25519 verifier lands.
