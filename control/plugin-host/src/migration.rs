//! Config migrations.
//!
//! A migration is a JSON file in the plugin's `migrations/` directory:
//!
//! ```json
//! { "from": "1.1.0", "to": "1.2.0", "set": { "format": "24h" }, "remove": ["legacy"] }
//! ```
//!
//! The host chains migrations from the installed config version to the new
//! plugin version, then validates the result against the new schema.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::Path;

use crate::manifest::Version;
use agentic_json::{self as json, Json};

#[derive(Clone, Debug, PartialEq)]
pub struct Migration {
    pub from: Version,
    pub to: Version,
    pub set: BTreeMap<String, Json>,
    pub remove: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrationError {
    pub message: String,
}

impl fmt::Display for MigrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for MigrationError {}

impl MigrationError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Migration {
    pub fn parse(text: &str) -> Result<Self, MigrationError> {
        let value = json::parse(text).map_err(|error| MigrationError::new(error.to_string()))?;
        let object = value
            .as_object()
            .ok_or_else(|| MigrationError::new("migration must be a JSON object"))?;

        let from = object
            .get("from")
            .and_then(Json::as_str)
            .and_then(Version::parse)
            .ok_or_else(|| MigrationError::new("migration needs a valid 'from' version"))?;
        let to = object
            .get("to")
            .and_then(Json::as_str)
            .and_then(Version::parse)
            .ok_or_else(|| MigrationError::new("migration needs a valid 'to' version"))?;
        if to <= from {
            return Err(MigrationError::new(
                "migration 'to' must be greater than 'from'",
            ));
        }

        let set = match object.get("set") {
            Some(value) => value
                .as_object()
                .ok_or_else(|| MigrationError::new("'set' must be an object"))?
                .clone(),
            None => BTreeMap::new(),
        };
        let remove = match object.get("remove") {
            Some(value) => value
                .as_array()
                .ok_or_else(|| MigrationError::new("'remove' must be an array"))?
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_string)
                        .ok_or_else(|| MigrationError::new("'remove' must hold strings"))
                })
                .collect::<Result<Vec<_>, _>>()?,
            None => Vec::new(),
        };

        Ok(Migration {
            from,
            to,
            set,
            remove,
        })
    }
}

pub fn load_migrations(dir: &Path) -> Result<Vec<Migration>, MigrationError> {
    let mut migrations = Vec::new();
    if !dir.exists() {
        return Ok(migrations);
    }
    let entries = fs::read_dir(dir).map_err(|error| {
        MigrationError::new(format!("could not read {}: {error}", dir.display()))
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| MigrationError::new(error.to_string()))?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let text = fs::read_to_string(&path).map_err(|error| {
            MigrationError::new(format!("could not read {}: {error}", path.display()))
        })?;
        let migration = Migration::parse(&text)
            .map_err(|error| MigrationError::new(format!("{}: {error}", path.display())))?;
        migrations.push(migration);
    }
    migrations.sort_by_key(|migration| migration.to);
    Ok(migrations)
}

/// Apply the migration chain from `from` to `to`. Returns the versions passed
/// through, so the caller can log them.
pub fn migrate(
    config: &mut BTreeMap<String, Json>,
    from: Version,
    to: Version,
    migrations: &[Migration],
) -> Result<Vec<Version>, MigrationError> {
    if from == to {
        return Ok(Vec::new());
    }
    if to < from {
        return Err(MigrationError::new(format!(
            "cannot migrate config backwards from {from} to {to}"
        )));
    }

    let mut applied = Vec::new();
    let mut current = from;

    while current < to {
        let next = migrations
            .iter()
            .filter(|migration| migration.from == current && migration.to <= to)
            .min_by_key(|migration| migration.to);

        let Some(migration) = next else {
            return Err(MigrationError::new(format!(
                "no migration path from {current} to {to}"
            )));
        };

        for key in &migration.remove {
            config.remove(key);
        }
        for (key, value) in &migration.set {
            config.insert(key.clone(), value.clone());
        }
        current = migration.to;
        applied.push(current);
    }

    Ok(applied)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(value: &str) -> Version {
        Version::parse(value).expect("version")
    }

    #[test]
    fn chains_migrations_in_order() {
        let migrations = vec![
            Migration::parse(r#"{"from":"1.0.0","to":"1.1.0","set":{"format":"24h"}}"#).unwrap(),
            Migration::parse(
                r#"{"from":"1.1.0","to":"1.2.0","set":{"show_seconds":true},"remove":["legacy"]}"#,
            )
            .unwrap(),
        ];
        let mut config = BTreeMap::new();
        config.insert("legacy".to_string(), Json::Bool(true));

        let applied =
            migrate(&mut config, version("1.0.0"), version("1.2.0"), &migrations).expect("migrate");

        assert_eq!(applied, vec![version("1.1.0"), version("1.2.0")]);
        assert_eq!(config.get("format"), Some(&Json::String("24h".into())));
        assert_eq!(config.get("show_seconds"), Some(&Json::Bool(true)));
        assert!(!config.contains_key("legacy"));
    }

    #[test]
    fn fails_without_a_path() {
        let error = migrate(
            &mut BTreeMap::new(),
            version("1.0.0"),
            version("2.0.0"),
            &[],
        )
        .expect_err("no path");
        assert!(error.message.contains("no migration path"));
    }
}
