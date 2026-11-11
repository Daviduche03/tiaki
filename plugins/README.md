# First-party plugins

Each plugin is a directory with a `plugin.toml` manifest and its assets.

```
plugins/<id>/
├── plugin.toml
├── ui/            # declarative UI trees (optional)
├── schema/        # config + UI schemas
└── migrations/    # config migrations (optional)
```

The manifest contract is in [docs/specs/plugin-manifest.md](../docs/specs/plugin-manifest.md).

`example-clock/` is a reference plugin showing the shape.
