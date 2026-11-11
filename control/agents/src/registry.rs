//! Runtime discovery: which agent implementations the OS can reach.
//!
//! `opencode` and `codex` are built in; anything else arrives as TOML under
//! `$TIAKI_HOME/runtimes/`. Every spec must speak `tiaki-abi/1`, declare a
//! transport and at least one tool, and use an id the registry does not
//! already hold. Bad files are reported as warnings, never fatal: one broken
//! runtime must not take down supervision.

use std::path::{Path, PathBuf};

use agentic_runtime_abi::RuntimeSpec;

pub struct RuntimeRegistry {
    specs: Vec<RuntimeSpec>,
}

impl RuntimeRegistry {
    /// Built-ins plus anything the caller already validated. Duplicate ids
    /// keep their first registration.
    pub fn new(specs: Vec<RuntimeSpec>) -> Result<Self, String> {
        let mut registry = Self { specs: Vec::new() };
        for spec in specs {
            spec.validate()
                .map_err(|error| format!("runtime '{}': {error}", spec.id))?;
            if !registry.specs.iter().any(|known| known.id == spec.id) {
                registry.specs.push(spec);
            }
        }
        Ok(registry)
    }

    /// Scan a directory for `*.toml` runtime descriptors. Returns one warning
    /// per skipped file; valid new runtimes are appended.
    pub fn discover(&mut self, dir: &Path) -> Vec<String> {
        let mut warnings = Vec::new();
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(_) => return warnings,
        };
        let mut paths: Vec<_> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("toml"))
            .collect();
        paths.sort();
        for path in paths {
            match RuntimeSpec::load(&path) {
                Ok(spec) => {
                    if self.specs.iter().any(|known| known.id == spec.id) {
                        warnings.push(format!(
                            "{}: duplicate runtime id '{}', keeping the first",
                            path.display(),
                            spec.id
                        ));
                    } else {
                        self.specs.push(spec);
                    }
                }
                Err(error) => warnings.push(format!("{}: {error}", path.display())),
            }
        }
        warnings
    }

    pub fn get(&self, id: &str) -> Option<&RuntimeSpec> {
        self.specs.iter().find(|spec| spec.id == id)
    }

    /// The developer-configured default: which runtime `agentd` starts when
    /// it is not told otherwise. A plain `default` file next to the
    /// descriptors, holding one id. Absent means no default, not a guess.
    pub fn default_path(dir: &Path) -> PathBuf {
        dir.join("default")
    }

    pub fn read_default(dir: &Path) -> Option<String> {
        std::fs::read_to_string(Self::default_path(dir))
            .ok()
            .map(|text| text.trim().to_string())
            .filter(|id| !id.is_empty())
    }

    pub fn write_default(dir: &Path, id: &str) -> Result<(), String> {
        std::fs::create_dir_all(dir)
            .and_then(|()| std::fs::write(Self::default_path(dir), format!("{id}\n")))
            .map_err(|error| format!("cannot write default: {error}"))
    }

    pub fn ids(&self) -> Vec<&str> {
        self.specs.iter().map(|spec| spec.id.as_str()).collect()
    }

    pub fn len(&self) -> usize {
        self.specs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.specs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn builtin(id: &str) -> RuntimeSpec {
        RuntimeSpec {
            id: id.into(),
            protocol: agentic_runtime_abi::PROTOCOL.into(),
            transport: format!("unix:///run/tiaki/runtime/{id}.sock"),
            tools: vec!["computer-use".into()],
        }
    }

    #[test]
    fn builtins_register_and_duplicates_keep_the_first() {
        let registry = RuntimeRegistry::new(vec![builtin("opencode"), builtin("codex")]).unwrap();
        assert_eq!(registry.ids(), vec!["opencode", "codex"]);
    }

    #[test]
    fn wrong_protocol_is_rejected() {
        let mut bad = builtin("old");
        bad.protocol = "tiaki-abi/0".into();
        assert!(RuntimeRegistry::new(vec![bad]).is_err());
    }

    #[test]
    fn discovery_loads_toml_and_warns_on_bad_files() {
        let dir = std::env::temp_dir().join(format!("tiaki-runtimes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("custom.toml"),
            "[runtime]\nid = \"custom\"\nprotocol = \"tiaki-abi/1\"\ntransport = \"stdio\"\ntools = [\"ui\"]\n",
        )
        .unwrap();
        std::fs::write(dir.join("notes.txt"), "not a runtime").unwrap();
        std::fs::write(dir.join("broken.toml"), "[runtime]\nid = \"broken\"\n").unwrap();

        let mut registry = RuntimeRegistry::new(vec![builtin("opencode")]).unwrap();
        let warnings = registry.discover(&dir);
        assert_eq!(registry.ids(), vec!["opencode", "custom"]);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("broken.toml"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_directory_discovers_nothing() {
        let mut registry = RuntimeRegistry::new(vec![builtin("opencode")]).unwrap();
        let warnings = registry.discover(Path::new("/nonexistent-tiaki-runtimes"));
        assert!(warnings.is_empty());
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn default_is_explicit_never_guessed() {
        let dir = std::env::temp_dir().join(format!("tiaki-default-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(RuntimeRegistry::read_default(&dir), None);
        RuntimeRegistry::write_default(&dir, "opencode").unwrap();
        assert_eq!(
            RuntimeRegistry::read_default(&dir).as_deref(),
            Some("opencode")
        );
        // Whitespace and empties do not count.
        std::fs::write(RuntimeRegistry::default_path(&dir), "  \n").unwrap();
        assert_eq!(RuntimeRegistry::read_default(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
