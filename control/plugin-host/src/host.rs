//! Plugin host: install, update, remove, load, and hot-swap.
//!
//! Layout under `$TIAKI_HOME/plugins`:
//!
//! ```text
//! <id>/
//!   active            # version string
//!   previous          # previous version, for rollback
//!   config.json       # { "version": "...", "values": { ... } }
//!   versions/<ver>/   # immutable installed copy of each version
//! ```
//!
//! The host never executes plugin code. It validates, verifies, stages, and
//! produces a `SandboxSpec` for the provisioner. That keeps this milestone
//! honest on a macOS host while preserving the real contract.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use agentic_sandbox::{ResourceLimits, SandboxProfile, SandboxSpec};
use agentic_transaction::Issue;

use crate::manifest::{self, Manifest, ManifestError, Tier, Version};
use crate::migration::{self, MigrationError};
use crate::schema::{self, ConfigSchema, SchemaError};
use crate::signature::{SignatureStatus, SignatureVerifier};
use agentic_json::{self as json, Json};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PluginState {
    Installed,
    Loaded,
    Running,
    Stopped,
    Broken(String),
}

impl PluginState {
    pub fn as_str(&self) -> &str {
        match self {
            PluginState::Installed => "installed",
            PluginState::Loaded => "loaded",
            PluginState::Running => "running",
            PluginState::Stopped => "stopped",
            PluginState::Broken(_) => "broken",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "installed" => Some(PluginState::Installed),
            "loaded" => Some(PluginState::Loaded),
            "running" => Some(PluginState::Running),
            "stopped" => Some(PluginState::Stopped),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct InstalledPlugin {
    pub manifest: Manifest,
    pub active: Version,
    pub previous: Option<Version>,
    pub state: PluginState,
    pub versions: Vec<Version>,
}

impl InstalledPlugin {
    pub fn id(&self) -> &str {
        &self.manifest.info.id
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallOptions {
    pub allow_unverified: bool,
}

impl Default for InstallOptions {
    fn default() -> Self {
        Self {
            allow_unverified: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct InstallOutcome {
    pub id: String,
    pub version: Version,
    pub tier: Tier,
    pub profile: SandboxProfile,
    pub signature: SignatureStatus,
    pub config: BTreeMap<String, Json>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Swap {
    /// Content-tier plugins swap live.
    HotSwapped,
    /// Session-tier plugins need a session restart.
    RestartRequired,
    /// Core-tier plugins wait for the next generation boundary.
    RebootRequired,
}

impl Swap {
    pub fn as_str(&self) -> &'static str {
        match self {
            Swap::HotSwapped => "hot-swapped",
            Swap::RestartRequired => "restart-required",
            Swap::RebootRequired => "reboot-required",
        }
    }
}

#[derive(Clone, Debug)]
pub struct UpdateOutcome {
    pub id: String,
    pub from: Version,
    pub to: Version,
    pub tier: Tier,
    pub swap: Swap,
    pub migrations: Vec<Version>,
}

#[derive(Clone, Debug)]
pub struct RemoveOutcome {
    pub id: String,
    pub version: Version,
}

#[derive(Debug)]
pub enum HostError {
    Io(io::Error),
    Manifest(ManifestError),
    Invalid(Vec<Issue>),
    Signature(String),
    NotFound(String),
    AlreadyInstalled(String),
    NotNewer {
        current: Version,
        candidate: Version,
    },
    BadState {
        id: String,
        state: PluginState,
        action: &'static str,
    },
    Migration(String),
    Schema(Vec<Issue>),
    NoConfigSchema(String),
    MissingVersion {
        id: String,
        version: Version,
    },
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostError::Io(error) => write!(f, "io error: {error}"),
            HostError::Manifest(error) => write!(f, "manifest error: {error}"),
            HostError::Invalid(issues) => {
                write!(f, "plugin failed validation: ")?;
                let messages: Vec<&str> = issues
                    .iter()
                    .filter(|issue| issue.severity == agentic_transaction::Severity::Error)
                    .map(|issue| issue.message.as_str())
                    .collect();
                write!(f, "{}", messages.join("; "))
            }
            HostError::Signature(reason) => write!(f, "signature {reason}"),
            HostError::NotFound(id) => write!(f, "plugin '{id}' is not installed"),
            HostError::AlreadyInstalled(id) => {
                write!(f, "plugin '{id}' is already installed; use update")
            }
            HostError::NotNewer { current, candidate } => write!(
                f,
                "candidate version {candidate} is not newer than installed {current}"
            ),
            HostError::BadState { id, state, action } => write!(
                f,
                "cannot {action} plugin '{id}' while it is {}",
                state.as_str()
            ),
            HostError::Migration(reason) => write!(f, "migration failed: {reason}"),
            HostError::Schema(issues) => {
                write!(f, "config failed validation: ")?;
                let messages: Vec<&str> =
                    issues.iter().map(|issue| issue.message.as_str()).collect();
                write!(f, "{}", messages.join("; "))
            }
            HostError::NoConfigSchema(id) => {
                write!(f, "plugin '{id}' declares no config schema")
            }
            HostError::MissingVersion { id, version } => {
                write!(f, "plugin '{id}' has no installed version {version}")
            }
        }
    }
}

impl std::error::Error for HostError {}

impl From<io::Error> for HostError {
    fn from(error: io::Error) -> Self {
        HostError::Io(error)
    }
}

impl From<ManifestError> for HostError {
    fn from(error: ManifestError) -> Self {
        HostError::Manifest(error)
    }
}

pub struct PluginHost {
    root: PathBuf,
    verifier: Box<dyn SignatureVerifier>,
    allow_unverified: bool,
    plugins: BTreeMap<String, InstalledPlugin>,
}

impl PluginHost {
    pub fn open(
        root: impl Into<PathBuf>,
        verifier: Box<dyn SignatureVerifier>,
    ) -> io::Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        let mut plugins = BTreeMap::new();

        for entry in fs::read_dir(&root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let dir = entry.path();
            let id = entry.file_name().to_string_lossy().to_string();
            match load_installed(&dir) {
                Ok(plugin) => {
                    plugins.insert(id, plugin);
                }
                Err(reason) => {
                    plugins.insert(
                        id.clone(),
                        InstalledPlugin {
                            manifest: placeholder_manifest(&id),
                            active: Version {
                                major: 0,
                                minor: 0,
                                patch: 0,
                            },
                            previous: None,
                            state: PluginState::Broken(reason),
                            versions: Vec::new(),
                        },
                    );
                }
            }
        }

        Ok(Self {
            root,
            verifier,
            allow_unverified: false,
            plugins,
        })
    }

    pub fn allow_unverified(mut self, allow: bool) -> Self {
        self.allow_unverified = allow;
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn get(&self, id: &str) -> Option<&InstalledPlugin> {
        self.plugins.get(id)
    }

    pub fn list(&self) -> Vec<&InstalledPlugin> {
        self.plugins.values().collect()
    }

    pub fn install(
        &mut self,
        source: &Path,
        options: &InstallOptions,
    ) -> Result<InstallOutcome, HostError> {
        let (manifest, manifest_text) = read_manifest(source)?;
        let issues = manifest::validate(&manifest, source);
        if has_errors(&issues) {
            return Err(HostError::Invalid(issues));
        }

        let signature = self.check_signature(source, &manifest_text, &manifest, options)?;

        if let Some(existing) = self.plugins.get(&manifest.info.id) {
            if !matches!(existing.state, PluginState::Broken(_)) {
                return Err(HostError::AlreadyInstalled(manifest.info.id.clone()));
            }
        }

        let plugin_dir = self.root.join(&manifest.info.id);
        let version_dir = plugin_dir
            .join("versions")
            .join(manifest.info.version.to_string());
        if version_dir.exists() {
            fs::remove_dir_all(&version_dir)?;
        }
        copy_dir(source, &version_dir)?;

        // Seed config from the schema defaults.
        let mut config = BTreeMap::new();
        if let Some(config_section) = &manifest.config {
            let schema_path = version_dir.join(&config_section.schema);
            let schema = load_schema(&schema_path)?;
            schema::apply_defaults(&schema, &mut config);
            let issues = schema::validate(&schema, &config);
            if has_errors(&issues) {
                let _ = fs::remove_dir_all(&version_dir);
                return Err(HostError::Schema(issues));
            }
        }
        write_config(&plugin_dir, manifest.info.version, &config)?;
        write_text(
            &plugin_dir.join("active"),
            &manifest.info.version.to_string(),
        )?;
        write_state(&plugin_dir, &PluginState::Installed)?;

        let installed = InstalledPlugin {
            manifest: manifest.clone(),
            active: manifest.info.version,
            previous: None,
            state: PluginState::Installed,
            versions: vec![manifest.info.version],
        };
        self.plugins.insert(manifest.info.id.clone(), installed);

        Ok(InstallOutcome {
            id: manifest.info.id.clone(),
            version: manifest.info.version,
            tier: manifest.tier(),
            profile: manifest.required_profile(),
            signature,
            config,
        })
    }

    pub fn update(
        &mut self,
        source: &Path,
        options: &InstallOptions,
    ) -> Result<UpdateOutcome, HostError> {
        let (manifest, manifest_text) = read_manifest(source)?;
        let issues = manifest::validate(&manifest, source);
        if has_errors(&issues) {
            return Err(HostError::Invalid(issues));
        }
        self.check_signature(source, &manifest_text, &manifest, options)?;

        let Some(existing) = self.plugins.get(&manifest.info.id).cloned() else {
            return Err(HostError::NotFound(manifest.info.id.clone()));
        };
        if manifest.info.version <= existing.active {
            return Err(HostError::NotNewer {
                current: existing.active,
                candidate: manifest.info.version,
            });
        }

        let plugin_dir = self.root.join(&manifest.info.id);
        let version_dir = plugin_dir
            .join("versions")
            .join(manifest.info.version.to_string());
        if version_dir.exists() {
            fs::remove_dir_all(&version_dir)?;
        }
        copy_dir(source, &version_dir)?;

        // Migrate the stored config to the new version, then validate.
        let mut config = read_config(&plugin_dir)?.1;
        let mut migrations = Vec::new();
        if let Some(config_section) = &manifest.config {
            let schema_path = version_dir.join(&config_section.schema);
            let schema = load_schema(&schema_path)?;
            if let Some(migrations_dir) = &config_section.migrations {
                let loaded = migration::load_migrations(&version_dir.join(migrations_dir))
                    .map_err(|error| HostError::Migration(error.message))?;
                // A version bump with no declared migrations carries config
                // forward unchanged. Declaring migrations promises coverage.
                if !loaded.is_empty() {
                    migrations = migration::migrate(
                        &mut config,
                        existing.active,
                        manifest.info.version,
                        &loaded,
                    )
                    .map_err(|error: MigrationError| HostError::Migration(error.message))?;
                }
            }
            schema::apply_defaults(&schema, &mut config);
            let issues = schema::validate(&schema, &config);
            if has_errors(&issues) {
                let _ = fs::remove_dir_all(&version_dir);
                return Err(HostError::Schema(issues));
            }
        }

        write_config(&plugin_dir, manifest.info.version, &config)?;
        write_text(&plugin_dir.join("previous"), &existing.active.to_string())?;
        write_text(
            &plugin_dir.join("active"),
            &manifest.info.version.to_string(),
        )?;

        let swap = match manifest.tier() {
            Tier::Content | Tier::AppUi => Swap::HotSwapped,
            Tier::Session => Swap::RestartRequired,
            Tier::Core => Swap::RebootRequired,
        };

        let mut versions = existing.versions.clone();
        if !versions.contains(&manifest.info.version) {
            versions.push(manifest.info.version);
            versions.sort();
        }
        let state = match (swap.clone(), existing.state.clone()) {
            (Swap::HotSwapped, PluginState::Running) => PluginState::Running,
            (Swap::HotSwapped, PluginState::Loaded) => PluginState::Loaded,
            (_, _) => PluginState::Stopped,
        };
        write_state(&plugin_dir, &state)?;

        self.plugins.insert(
            manifest.info.id.clone(),
            InstalledPlugin {
                manifest: manifest.clone(),
                active: manifest.info.version,
                previous: Some(existing.active),
                state,
                versions,
            },
        );

        Ok(UpdateOutcome {
            id: manifest.info.id.clone(),
            from: existing.active,
            to: manifest.info.version,
            tier: manifest.tier(),
            swap,
            migrations,
        })
    }

    pub fn remove(&mut self, id: &str) -> Result<RemoveOutcome, HostError> {
        let Some(existing) = self.plugins.get(id) else {
            return Err(HostError::NotFound(id.to_string()));
        };
        let version = existing.active;
        let plugin_dir = self.root.join(id);
        if plugin_dir.exists() {
            fs::remove_dir_all(&plugin_dir)?;
        }
        self.plugins.remove(id);
        Ok(RemoveOutcome {
            id: id.to_string(),
            version,
        })
    }

    pub fn load(&mut self, id: &str) -> Result<(), HostError> {
        let plugin_dir = self.root.join(id);
        let plugin = self
            .plugins
            .get_mut(id)
            .ok_or_else(|| HostError::NotFound(id.to_string()))?;
        match plugin.state {
            PluginState::Installed | PluginState::Stopped => {
                plugin.state = PluginState::Loaded;
                write_state(&plugin_dir, &plugin.state)?;
                Ok(())
            }
            _ => Err(HostError::BadState {
                id: id.to_string(),
                state: plugin.state.clone(),
                action: "load",
            }),
        }
    }

    pub fn start(&mut self, id: &str) -> Result<(), HostError> {
        let plugin_dir = self.root.join(id);
        let plugin = self
            .plugins
            .get_mut(id)
            .ok_or_else(|| HostError::NotFound(id.to_string()))?;
        match plugin.state {
            PluginState::Loaded | PluginState::Stopped => {
                plugin.state = PluginState::Running;
                write_state(&plugin_dir, &plugin.state)?;
                Ok(())
            }
            _ => Err(HostError::BadState {
                id: id.to_string(),
                state: plugin.state.clone(),
                action: "start",
            }),
        }
    }

    pub fn stop(&mut self, id: &str) -> Result<(), HostError> {
        let plugin_dir = self.root.join(id);
        let plugin = self
            .plugins
            .get_mut(id)
            .ok_or_else(|| HostError::NotFound(id.to_string()))?;
        match plugin.state {
            PluginState::Running => {
                plugin.state = PluginState::Stopped;
                write_state(&plugin_dir, &plugin.state)?;
                Ok(())
            }
            _ => Err(HostError::BadState {
                id: id.to_string(),
                state: plugin.state.clone(),
                action: "stop",
            }),
        }
    }

    pub fn unload(&mut self, id: &str) -> Result<(), HostError> {
        let plugin_dir = self.root.join(id);
        let plugin = self
            .plugins
            .get_mut(id)
            .ok_or_else(|| HostError::NotFound(id.to_string()))?;
        match plugin.state {
            PluginState::Loaded | PluginState::Stopped => {
                plugin.state = PluginState::Installed;
                write_state(&plugin_dir, &plugin.state)?;
                Ok(())
            }
            _ => Err(HostError::BadState {
                id: id.to_string(),
                state: plugin.state.clone(),
                action: "unload",
            }),
        }
    }

    /// Switch the active version to one already installed. Used for rollback.
    pub fn swap(&mut self, id: &str, version: Version) -> Result<Swap, HostError> {
        let plugin = self
            .plugins
            .get_mut(id)
            .ok_or_else(|| HostError::NotFound(id.to_string()))?;
        if !plugin.versions.contains(&version) {
            return Err(HostError::MissingVersion {
                id: id.to_string(),
                version,
            });
        }
        let plugin_dir = self.root.join(id);
        let previous_active = plugin.active;

        // Re-read the manifest for the target version and make sure the
        // current config still validates against it before switching.
        let target_dir = plugin_dir.join("versions").join(version.to_string());
        let manifest = manifest::load_manifest(target_dir.join("plugin.toml"))?;
        let (_, config) = read_config(&plugin_dir)?;
        if let Some(config_section) = &manifest.config {
            let schema = load_schema(&target_dir.join(&config_section.schema))?;
            let issues = schema::validate(&schema, &config);
            if has_errors(&issues) {
                return Err(HostError::Schema(issues));
            }
        }

        write_config(&plugin_dir, version, &config)?;
        write_text(&plugin_dir.join("previous"), &previous_active.to_string())?;
        write_text(&plugin_dir.join("active"), &version.to_string())?;

        plugin.manifest = manifest;
        plugin.active = version;
        plugin.previous = Some(previous_active);

        Ok(match plugin.manifest.tier() {
            Tier::Content | Tier::AppUi => Swap::HotSwapped,
            Tier::Session => Swap::RestartRequired,
            Tier::Core => Swap::RebootRequired,
        })
    }

    pub fn rollback(&mut self, id: &str) -> Result<Swap, HostError> {
        let plugin = self
            .plugins
            .get(id)
            .ok_or_else(|| HostError::NotFound(id.to_string()))?;
        let Some(previous) = plugin.previous else {
            return Err(HostError::BadState {
                id: id.to_string(),
                state: plugin.state.clone(),
                action: "roll back (no previous version)",
            });
        };
        self.swap(id, previous)
    }

    pub fn config(&self, id: &str) -> Result<BTreeMap<String, Json>, HostError> {
        let plugin = self
            .plugins
            .get(id)
            .ok_or_else(|| HostError::NotFound(id.to_string()))?;
        Ok(read_config(&self.root.join(&plugin.id()))?.1)
    }

    pub fn set_config(
        &mut self,
        id: &str,
        values: BTreeMap<String, Json>,
    ) -> Result<BTreeMap<String, Json>, HostError> {
        let plugin = self
            .plugins
            .get(id)
            .ok_or_else(|| HostError::NotFound(id.to_string()))?;
        let plugin_dir = self.root.join(plugin.id());
        let mut config = values;

        if let Some(config_section) = &plugin.manifest.config {
            let schema = load_schema(
                &plugin_dir
                    .join("versions")
                    .join(plugin.active.to_string())
                    .join(&config_section.schema),
            )?;
            schema::apply_defaults(&schema, &mut config);
            let issues = schema::validate(&schema, &config);
            if has_errors(&issues) {
                return Err(HostError::Schema(issues));
            }
        }

        write_config(&plugin_dir, plugin.active, &config)?;
        Ok(config)
    }

    /// The sandbox spec the provisioner would run for this plugin.
    pub fn spec(&self, id: &str) -> Result<SandboxSpec, HostError> {
        let plugin = self
            .plugins
            .get(id)
            .ok_or_else(|| HostError::NotFound(id.to_string()))?;
        let version_dir = self
            .root
            .join(plugin.id())
            .join("versions")
            .join(plugin.active.to_string());
        let entry = version_dir.join(&plugin.manifest.info.entry);

        let mut spec = SandboxSpec::new(
            format!("service:plugin/{}", plugin.id()),
            plugin.manifest.required_profile(),
            entry.to_string_lossy().to_string(),
        );
        spec.read_only = vec![version_dir.to_string_lossy().to_string()];
        spec.read_write = vec![self
            .root
            .join(plugin.id())
            .join("state")
            .to_string_lossy()
            .to_string()];
        spec.capabilities = plugin.manifest.capabilities.requires.clone();
        spec.limits = ResourceLimits {
            cpu_ms: None,
            memory_bytes: plugin
                .manifest
                .resources
                .memory_max
                .as_deref()
                .and_then(parse_size),
            pids: Some(64),
            network: plugin.manifest.wants_network(),
            cpu_shares: None,
        };
        spec.timeout_secs = 0;
        Ok(spec)
    }

    fn check_signature(
        &self,
        root: &Path,
        manifest_text: &str,
        manifest: &Manifest,
        options: &InstallOptions,
    ) -> Result<SignatureStatus, HostError> {
        let status = self
            .verifier
            .verify(root, manifest_text, &manifest.update.signature);
        match &status {
            SignatureStatus::Verified => Ok(status),
            SignatureStatus::Unverified(reason) => {
                if options.allow_unverified || self.allow_unverified {
                    Ok(status)
                } else {
                    Err(HostError::Signature(format!(
                        "is unverified ({reason}); pass --allow-unverified to install anyway"
                    )))
                }
            }
            SignatureStatus::Rejected(reason) => {
                Err(HostError::Signature(format!("was rejected ({reason})")))
            }
        }
    }
}

fn read_manifest(root: &Path) -> Result<(Manifest, String), HostError> {
    let path = root.join("plugin.toml");
    let text = fs::read_to_string(&path)?;
    let manifest = manifest::parse_manifest(&text)?;
    Ok((manifest, text))
}

fn load_installed(dir: &Path) -> Result<InstalledPlugin, String> {
    let active = fs::read_to_string(dir.join("active"))
        .map_err(|error| format!("missing active version: {error}"))?
        .trim()
        .to_string();
    let active = Version::parse(&active).ok_or_else(|| "invalid active version".to_string())?;
    let manifest = manifest::load_manifest(
        dir.join("versions")
            .join(active.to_string())
            .join("plugin.toml"),
    )
    .map_err(|error| error.to_string())?;

    let previous = fs::read_to_string(dir.join("previous"))
        .ok()
        .and_then(|value| Version::parse(value.trim()));

    let mut versions = Vec::new();
    if let Ok(entries) = fs::read_dir(dir.join("versions")) {
        for entry in entries.flatten() {
            if let Some(version) = entry.file_name().to_str().and_then(Version::parse) {
                versions.push(version);
            }
        }
    }
    versions.sort();

    Ok(InstalledPlugin {
        manifest,
        active,
        previous,
        state: read_state(dir),
        versions,
    })
}

fn placeholder_manifest(id: &str) -> Manifest {
    Manifest {
        info: crate::manifest::PluginInfo {
            id: id.into(),
            name: id.into(),
            version: Version {
                major: 0,
                minor: 0,
                patch: 0,
            },
            api: Version {
                major: 0,
                minor: 0,
                patch: 0,
            },
            kind: crate::manifest::PluginKind::Tool,
            entry: String::new(),
        },
        capabilities: Default::default(),
        resources: Default::default(),
        ui: None,
        config: None,
        update: crate::manifest::UpdateSection {
            channel: "stable".into(),
            signature: String::new(),
        },
    }
}

fn load_schema(path: &Path) -> Result<ConfigSchema, HostError> {
    let text = fs::read_to_string(path)?;
    schema::parse_schema(&text).map_err(|error: SchemaError| HostError::Migration(error.message))
}

fn read_config(plugin_dir: &Path) -> Result<(Version, BTreeMap<String, Json>), HostError> {
    let path = plugin_dir.join("config.json");
    if !path.exists() {
        return Ok((
            Version {
                major: 0,
                minor: 0,
                patch: 0,
            },
            BTreeMap::new(),
        ));
    }
    let text = fs::read_to_string(&path)?;
    let value = json::parse(&text).map_err(|error| HostError::Migration(error.message))?;
    let object = value
        .as_object()
        .ok_or_else(|| HostError::Migration("config.json must be an object".into()))?;
    let version = object
        .get("version")
        .and_then(Json::as_str)
        .and_then(Version::parse)
        .ok_or_else(|| HostError::Migration("config.json needs a version".into()))?;
    let values = object
        .get("values")
        .and_then(Json::as_object)
        .cloned()
        .unwrap_or_default();
    Ok((version, values))
}

fn write_config(
    plugin_dir: &Path,
    version: Version,
    values: &BTreeMap<String, Json>,
) -> Result<(), HostError> {
    let mut object = BTreeMap::new();
    object.insert("version".to_string(), Json::String(version.to_string()));
    object.insert("values".to_string(), Json::Object(values.clone()));
    let text = Json::Object(object).render();
    write_text(&plugin_dir.join("config.json"), &text)
}

fn write_text(path: &Path, text: &str) -> Result<(), HostError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)?;
    Ok(())
}

fn write_state(plugin_dir: &Path, state: &PluginState) -> Result<(), HostError> {
    write_text(&plugin_dir.join("state"), state.as_str())
}

fn read_state(plugin_dir: &Path) -> PluginState {
    fs::read_to_string(plugin_dir.join("state"))
        .ok()
        .and_then(|value| PluginState::parse(&value))
        .unwrap_or(PluginState::Installed)
}

fn copy_dir(source: &Path, destination: &Path) -> io::Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

fn has_errors(issues: &[Issue]) -> bool {
    issues
        .iter()
        .any(|issue| issue.severity == agentic_transaction::Severity::Error)
}

/// Parse sizes like `64M`, `1G`, or a plain byte count.
pub fn parse_size(text: &str) -> Option<u64> {
    let text = text.trim();
    let (number, multiplier) = match text.chars().last()? {
        'K' | 'k' => (&text[..text.len() - 1], 1024u64),
        'M' | 'm' => (&text[..text.len() - 1], 1024 * 1024),
        'G' | 'g' => (&text[..text.len() - 1], 1024 * 1024 * 1024),
        _ => (text, 1),
    };
    number
        .trim()
        .parse::<u64>()
        .ok()
        .map(|value| value * multiplier)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sizes() {
        assert_eq!(parse_size("64M"), Some(64 * 1024 * 1024));
        assert_eq!(parse_size("1G"), Some(1024 * 1024 * 1024));
        assert_eq!(parse_size("512"), Some(512));
        assert_eq!(parse_size("nope"), None);
    }
}
