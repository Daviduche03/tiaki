//! Plugin manifest parsing and semantic validation.
//!
//! Grammar is the TOML subset in [`crate::toml`]. Semantic rules live here so
//! the host can reject a plugin before it ever reaches a sandbox.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use agentic_sandbox::SandboxProfile;
use agentic_transaction::Issue;

use agentic_toml::{self as toml, Document, Value};

/// The plugin API major version this host implements.
pub const HOST_API_MAJOR: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next().unwrap_or("0").parse().ok()?;
        let patch = parts.next().unwrap_or("0").parse().ok()?;
        if parts.next().is_some() {
            return None;
        }
        Some(Version {
            major,
            minor,
            patch,
        })
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluginKind {
    Ui,
    Service,
    Runtime,
    Backend,
    Tool,
}

impl PluginKind {
    pub fn as_str(self) -> &'static str {
        match self {
            PluginKind::Ui => "ui",
            PluginKind::Service => "service",
            PluginKind::Runtime => "runtime",
            PluginKind::Backend => "backend",
            PluginKind::Tool => "tool",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "ui" => Some(PluginKind::Ui),
            "service" => Some(PluginKind::Service),
            "runtime" => Some(PluginKind::Runtime),
            "backend" => Some(PluginKind::Backend),
            "tool" => Some(PluginKind::Tool),
            _ => None,
        }
    }
}

/// Swap semantics. Content and app-UI tiers hot-swap; session restarts; core
/// waits for a generation boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    Content,
    AppUi,
    Session,
    Core,
}

impl Tier {
    pub fn as_str(self) -> &'static str {
        match self {
            Tier::Content => "content",
            Tier::AppUi => "app-ui",
            Tier::Session => "session",
            Tier::Core => "core",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: Version,
    pub api: Version,
    pub kind: PluginKind,
    pub entry: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Capabilities {
    pub requires: Vec<String>,
    pub optional: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Resources {
    pub cpu_weight: Option<i64>,
    pub memory_max: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiSection {
    pub tree: String,
    pub schema: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigSection {
    pub schema: String,
    pub migrations: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateSection {
    pub channel: String,
    pub signature: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    pub info: PluginInfo,
    pub capabilities: Capabilities,
    pub resources: Resources,
    pub ui: Option<UiSection>,
    pub config: Option<ConfigSection>,
    pub update: UpdateSection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManifestError {
    pub message: String,
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ManifestError {}

impl ManifestError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Manifest {
    /// The swap tier implied by the plugin kind. UI and tool plugins are
    /// content-tier; runtimes, services, and backends need a session restart.
    pub fn tier(&self) -> Tier {
        match self.info.kind {
            PluginKind::Ui | PluginKind::Tool => Tier::Content,
            PluginKind::Service | PluginKind::Runtime | PluginKind::Backend => Tier::Session,
        }
    }

    /// The narrowest sandbox that satisfies the manifest's required
    /// capabilities. Capabilities widen, never narrow, the profile.
    pub fn required_profile(&self) -> SandboxProfile {
        let mut profile = match self.info.kind {
            PluginKind::Ui | PluginKind::Tool => SandboxProfile::UserWrite,
            PluginKind::Service | PluginKind::Runtime | PluginKind::Backend => {
                SandboxProfile::SystemWrite
            }
        };

        for capability in &self.capabilities.requires {
            let needed = capability_profile(capability);
            if needed > profile {
                profile = needed;
            }
        }
        profile
    }

    pub fn wants_network(&self) -> bool {
        self.capabilities
            .requires
            .iter()
            .chain(self.capabilities.optional.iter())
            .any(|capability| capability.starts_with("network."))
    }
}

fn capability_profile(capability: &str) -> SandboxProfile {
    if capability.starts_with("kernel.") || capability == "system.admin" {
        SandboxProfile::Privileged
    } else if capability.starts_with("system.")
        || capability.starts_with("service.")
        || capability.starts_with("plugin.")
    {
        SandboxProfile::SystemWrite
    } else {
        SandboxProfile::UserWrite
    }
}

pub fn load_manifest(path: impl AsRef<Path>) -> Result<Manifest, ManifestError> {
    let path = path.as_ref();
    let text = fs::read_to_string(path).map_err(|error| {
        ManifestError::new(format!("could not read {}: {error}", path.display()))
    })?;
    parse_manifest(&text)
}

pub fn parse_manifest(text: &str) -> Result<Manifest, ManifestError> {
    let document = toml::parse(text).map_err(|error| ManifestError::new(error.to_string()))?;

    let plugin = require_table(&document, "plugin")?;
    let info = PluginInfo {
        id: require_string(plugin, "plugin", "id")?,
        name: require_string(plugin, "plugin", "name")?,
        version: parse_version(plugin, "version")?,
        api: parse_version(plugin, "api")?,
        kind: {
            let raw = require_string(plugin, "plugin", "kind")?;
            PluginKind::parse(&raw).ok_or_else(|| {
                ManifestError::new(format!(
                    "unknown plugin kind '{raw}'; use ui, service, runtime, backend, or tool"
                ))
            })?
        },
        entry: require_string(plugin, "plugin", "entry")?,
    };

    let capabilities = match document.table("capabilities") {
        Some(table) => Capabilities {
            requires: string_array(table, "capabilities", "requires")?,
            optional: string_array(table, "capabilities", "optional")?,
        },
        None => Capabilities::default(),
    };

    let resources = match document.table("resources") {
        Some(table) => Resources {
            cpu_weight: table.get("cpu_weight").and_then(Value::as_i64),
            memory_max: table
                .get("memory_max")
                .and_then(Value::as_str)
                .map(str::to_string),
        },
        None => Resources::default(),
    };

    let ui = match document.table("ui") {
        Some(table) => Some(UiSection {
            tree: require_string(table, "ui", "tree")?,
            schema: require_string(table, "ui", "schema")?,
        }),
        None => None,
    };

    let config = match document.table("config") {
        Some(table) => Some(ConfigSection {
            schema: require_string(table, "config", "schema")?,
            migrations: table
                .get("migrations")
                .and_then(Value::as_str)
                .map(str::to_string),
        }),
        None => None,
    };

    let update_table = require_table(&document, "update")?;
    let update = UpdateSection {
        channel: require_string(update_table, "update", "channel")?,
        signature: require_string(update_table, "update", "signature")?,
    };

    Ok(Manifest {
        info,
        capabilities,
        resources,
        ui,
        config,
        update,
    })
}

fn require_table<'a>(
    document: &'a Document,
    name: &str,
) -> Result<&'a std::collections::BTreeMap<String, Value>, ManifestError> {
    document
        .table(name)
        .ok_or_else(|| ManifestError::new(format!("missing [{name}] section")))
}

fn require_string(
    table: &std::collections::BTreeMap<String, Value>,
    section: &str,
    key: &str,
) -> Result<String, ManifestError> {
    table
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| ManifestError::new(format!("missing string [{section}] {key}")))
}

fn string_array(
    table: &std::collections::BTreeMap<String, Value>,
    section: &str,
    key: &str,
) -> Result<Vec<String>, ManifestError> {
    let Some(value) = table.get(key) else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .ok_or_else(|| ManifestError::new(format!("[{section}] {key} must be an array")))?;
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| ManifestError::new(format!("[{section}] {key} must hold strings")))
        })
        .collect()
}

fn parse_version(
    table: &std::collections::BTreeMap<String, Value>,
    key: &str,
) -> Result<Version, ManifestError> {
    let raw = require_string(table, "plugin", key)?;
    Version::parse(&raw).ok_or_else(|| {
        ManifestError::new(format!("invalid {key} '{raw}'; expected MAJOR.MINOR.PATCH"))
    })
}

/// Semantic validation against the on-disk plugin root. Returns errors and
/// warnings; any error blocks install.
pub fn validate(manifest: &Manifest, root: &Path) -> Vec<Issue> {
    let mut issues = Vec::new();

    if !valid_id(&manifest.info.id) {
        issues.push(Issue::error(
            "plugin.invalid_id",
            None,
            format!(
                "'{}' is not a valid plugin id; use dotted lowercase segments",
                manifest.info.id
            ),
        ));
    }
    if manifest.info.name.trim().is_empty() {
        issues.push(Issue::error(
            "plugin.invalid_name",
            None,
            "plugin name must not be empty",
        ));
    }
    if manifest.info.api.major != HOST_API_MAJOR {
        issues.push(Issue::error(
            "plugin.incompatible_api",
            None,
            format!(
                "plugin targets api {} but this host implements {}.x",
                manifest.info.api, HOST_API_MAJOR
            ),
        ));
    }
    if manifest.update.signature.trim().is_empty() {
        issues.push(Issue::error(
            "plugin.unsigned",
            None,
            "plugin has no signature; refusing to load unsigned code",
        ));
    }
    if manifest.update.channel != "stable" && manifest.update.channel != "beta" {
        issues.push(Issue::warning(
            "plugin.unknown_channel",
            None,
            format!("unknown update channel '{}'", manifest.update.channel),
        ));
    }
    if let Some(weight) = manifest.resources.cpu_weight {
        if weight <= 0 {
            issues.push(Issue::error(
                "plugin.invalid_cpu_weight",
                None,
                "cpu_weight must be positive",
            ));
        }
    }

    check_relative_file(&mut issues, root, &manifest.info.entry, "entry");
    if let Some(ui) = &manifest.ui {
        check_relative_file(&mut issues, root, &ui.tree, "ui.tree");
        check_relative_file(&mut issues, root, &ui.schema, "ui.schema");
    }
    if let Some(config) = &manifest.config {
        check_relative_file(&mut issues, root, &config.schema, "config.schema");
        if let Some(migrations) = &config.migrations {
            check_relative_dir(&mut issues, root, migrations, "config.migrations");
        }
    }

    issues
}

fn valid_id(id: &str) -> bool {
    if id.len() < 3 || id.len() > 128 || !id.contains('.') {
        return false;
    }
    id.split('.').all(|segment| {
        !segment.is_empty()
            && segment.chars().all(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
            })
    })
}

fn check_relative_file(issues: &mut Vec<Issue>, root: &Path, relative: &str, field: &str) {
    match safe_relative(relative) {
        Some(path) => {
            if !root.join(path).is_file() {
                issues.push(Issue::error(
                    "plugin.missing_file",
                    None,
                    format!("{field} '{relative}' does not exist in the plugin"),
                ));
            }
        }
        None => issues.push(Issue::error(
            "plugin.unsafe_path",
            None,
            format!("{field} '{relative}' must be a relative path inside the plugin"),
        )),
    }
}

fn check_relative_dir(issues: &mut Vec<Issue>, root: &Path, relative: &str, field: &str) {
    match safe_relative(relative) {
        Some(path) => {
            if !root.join(path).is_dir() {
                issues.push(Issue::error(
                    "plugin.missing_dir",
                    None,
                    format!("{field} '{relative}' does not exist in the plugin"),
                ));
            }
        }
        None => issues.push(Issue::error(
            "plugin.unsafe_path",
            None,
            format!("{field} '{relative}' must be a relative path inside the plugin"),
        )),
    }
}

/// Reject absolute paths and any `..` traversal.
pub fn safe_relative(relative: &str) -> Option<PathBuf> {
    if relative.is_empty() || relative.starts_with('/') || relative.starts_with('\\') {
        return None;
    }
    let path = PathBuf::from(relative);
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return None;
    }
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"
[plugin]
id = "org.tiaki.panel.clock"
name = "Clock Panel"
version = "1.2.0"
api = "1"
kind = "ui"
entry = "libclock.so"

[capabilities]
requires = ["ui.surface.create", "time.read"]

[resources]
cpu_weight = 100
memory_max = "64M"

[ui]
tree = "ui/clock.ui.json"
schema = "ui/clock.schema.json"

[config]
schema = "schema/clock.config.schema.json"
migrations = "migrations/"

[update]
channel = "stable"
signature = "ed25519:abc"
"#;

    #[test]
    fn parses_a_complete_manifest() {
        let manifest = parse_manifest(GOOD).expect("parse");
        assert_eq!(manifest.info.id, "org.tiaki.panel.clock");
        assert_eq!(manifest.info.version, Version::parse("1.2.0").unwrap());
        assert_eq!(manifest.info.kind, PluginKind::Ui);
        assert_eq!(manifest.capabilities.requires.len(), 2);
        assert_eq!(manifest.tier(), Tier::Content);
        assert_eq!(manifest.required_profile(), SandboxProfile::UserWrite);
    }

    #[test]
    fn widens_profile_for_system_capabilities() {
        let text = GOOD.replace(
            "requires = [\"ui.surface.create\", \"time.read\"]",
            "requires = [\"system.service.manage\"]",
        );
        let manifest = parse_manifest(&text).expect("parse");
        assert_eq!(manifest.required_profile(), SandboxProfile::SystemWrite);
    }

    #[test]
    fn rejects_bad_versions_and_kinds() {
        let bad_version = GOOD.replace("version = \"1.2.0\"", "version = \"1.2\"");
        assert!(parse_manifest(&bad_version).is_ok()); // 1.2 -> 1.2.0

        let bad_version = GOOD.replace("version = \"1.2.0\"", "version = \"x.y.z\"");
        assert!(parse_manifest(&bad_version).is_err());

        let bad_kind = GOOD.replace("kind = \"ui\"", "kind = \"daemon\"");
        assert!(parse_manifest(&bad_kind).is_err());
    }

    #[test]
    fn rejects_path_traversal() {
        assert!(safe_relative("../etc/passwd").is_none());
        assert!(safe_relative("/etc/passwd").is_none());
        assert!(safe_relative("ui/tree.json").is_some());
    }

    #[test]
    fn validation_reports_missing_assets() {
        let manifest = parse_manifest(GOOD).expect("parse");
        let root = std::env::temp_dir().join(format!("tiaki-plugin-empty-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let issues = validate(&manifest, &root);
        assert!(issues
            .iter()
            .any(|issue| issue.code == "plugin.missing_file"));
        let _ = fs::remove_dir_all(&root);
    }
}
