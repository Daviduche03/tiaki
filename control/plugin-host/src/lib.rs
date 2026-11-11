//! Plugin host: manifests, validation, signatures, isolation, lifecycle,
//! config schemas, migrations, and hot-swap tiers.
//!
//! See TIAKI.md §10. The host validates and stages plugins; execution is
//! delegated to the sandbox provisioner.

#![allow(dead_code)]

pub mod host;
pub mod manifest;
pub mod migration;
pub mod schema;
pub mod signature;

pub use agentic_json as json;
pub use agentic_json::Json;
pub use agentic_toml as toml;
pub use agentic_toml::{Document, TomlError, Value};
pub use host::{
    parse_size, HostError, InstallOptions, InstallOutcome, InstalledPlugin, PluginHost,
    PluginState, RemoveOutcome, Swap, UpdateOutcome,
};
pub use manifest::{
    load_manifest, parse_manifest, validate, Capabilities, ConfigSection, Manifest, ManifestError,
    PluginInfo, PluginKind, Resources, Tier, UiSection, UpdateSection, Version, HOST_API_MAJOR,
};
pub use migration::{load_migrations, migrate, Migration, MigrationError};
pub use schema::{parse_schema, ConfigSchema, Property, SchemaError};
pub use signature::{DigestVerifier, PresenceVerifier, SignatureStatus, SignatureVerifier};
