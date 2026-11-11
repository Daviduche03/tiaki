//! Real enforcement: a cgroup v2 provisioner.
//!
//! The supervisor decides shares; this module makes the kernel honour them.
//! Each sandbox becomes a child cgroup whose `cpu.weight` is derived from its
//! share of 1024. `memory.max` and `pids.max` come from the sandbox limits.
//!
//! Honest failure is part of the contract. If cgroup v2 is not mounted, the
//! root is not writable, or the `cpu` controller is not delegated, `probe`
//! says so and `create` returns `Unsupported` instead of pretending. Callers
//! fall back to the simulated provisioner and report which one ran.
//!
//! This module does not mount hierarchies. On a real system init delegates a
//! subtree (`/sys/fs/cgroup/tiaki`); in a container the harness does the
//! same. A provisioner that mount()s would be lying about its privileges.

use std::path::{Path, PathBuf};

use crate::{SandboxError, SandboxHandle, SandboxProvisioner, SandboxSpec, SandboxStatus};

/// Where the OS expects its delegated subtree.
pub const DEFAULT_ROOT: &str = "/sys/fs/cgroup/tiaki";
/// Set this to point the provisioner at a different delegated subtree.
pub const ROOT_ENV: &str = "TIAKI_CGROUP_ROOT";
/// The share basis. A full 1024 shares maps to cgroup's default weight (100).
pub const SHARE_BASIS: u32 = 1024;

/// What a probe found, so a CLI can explain itself instead of guessing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CgroupReport {
    pub root: PathBuf,
    pub mounted: bool,
    pub controllers: Vec<String>,
    pub ready: bool,
    pub reason: Option<String>,
}

impl CgroupReport {
    pub fn backend(&self) -> &'static str {
        if self.ready {
            "cgroup (real)"
        } else {
            "simulated (no cgroup enforcement)"
        }
    }
}

/// Convert scheduler shares to a cgroup v2 `cpu.weight` (1..=10000).
///
/// 1024 shares -> 100, the kernel's default, so a fully managed subtree sums
/// to the weight it would have had unmanaged. Never returns 0: cgroup rejects
/// it, and a zero-share sandbox should still make progress.
pub fn weight_for_shares(shares: u32) -> u32 {
    let scaled = (shares as u64 * 100) / SHARE_BASIS as u64;
    scaled.clamp(1, 10_000) as u32
}

/// Resolve the root: explicit argument, then the environment, then the default.
pub fn root_from_env() -> PathBuf {
    std::env::var(ROOT_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_ROOT))
}

pub struct CgroupProvisioner {
    root: PathBuf,
    next: std::cell::Cell<u64>,
    /// Skip the readiness check. Used by tests and by callers that already
    /// prepared the root through the same checks.
    unchecked: bool,
}

impl CgroupProvisioner {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            next: std::cell::Cell::new(1),
            unchecked: false,
        }
    }

    /// For tests and for a caller that has already run `probe` against this
    /// root successfully. Skips the re-probe on every `create`.
    pub fn unchecked(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            next: std::cell::Cell::new(1),
            unchecked: true,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Inspect a root without changing anything beyond creating the subtree
    /// directory and enabling controllers, which is what `create` needs.
    pub fn probe(root: &Path) -> CgroupReport {
        let mut report = CgroupReport {
            root: root.to_path_buf(),
            mounted: Path::new("/sys/fs/cgroup/cgroup.controllers").exists(),
            controllers: Vec::new(),
            ready: false,
            reason: None,
        };
        if !report.mounted {
            report.reason = Some("cgroup v2 is not mounted at /sys/fs/cgroup".into());
            return report;
        }
        if let Err(error) = std::fs::create_dir_all(root) {
            report.reason = Some(format!("cannot create {}: {error}", root.display()));
            return report;
        }
        match enable_controllers(root) {
            Ok(controllers) => {
                report.controllers = controllers;
                if report.controllers.iter().any(|name| name == "cpu") {
                    report.ready = true;
                } else {
                    report.reason = Some(format!(
                        "the cpu controller is not delegated to {} (have: {})",
                        root.display(),
                        if report.controllers.is_empty() {
                            "none".to_string()
                        } else {
                            report.controllers.join(", ")
                        }
                    ));
                }
            }
            Err(error) => report.reason = Some(error.to_string()),
        }
        report
    }

    fn sandbox_dir(&self, id: &str) -> PathBuf {
        self.root.join(sanitize(id))
    }

    /// Create a cgroup with a caller-chosen name, so a launcher can encode the
    /// scheduling class in the directory name and any process can read the
    /// live set back with no daemon.
    pub fn create_named(&self, spec: SandboxSpec, id: &str) -> Result<SandboxHandle, SandboxError> {
        if !self.unchecked {
            let report = Self::probe(&self.root);
            if !report.ready {
                return Err(SandboxError::Unsupported(
                    report
                        .reason
                        .unwrap_or_else(|| "cgroup v2 is not ready".into()),
                ));
            }
        }
        let dir = self.sandbox_dir(id);
        std::fs::create_dir_all(&dir)
            .map_err(|error| SandboxError::Provisioner(format!("{}: {error}", dir.display())))?;
        for (file, value) in limits_for(&spec) {
            if let Err(error) = std::fs::write(dir.join(file), value) {
                let _ = std::fs::remove_dir(&dir);
                return Err(SandboxError::Provisioner(format!(
                    "{}: {error}",
                    dir.join(file).display()
                )));
            }
        }
        Ok(SandboxHandle { id: sanitize(id) })
    }

    /// Move a running process into this sandbox's cgroup.
    pub fn attach(&self, handle: &SandboxHandle, pid: u32) -> Result<(), SandboxError> {
        let path = self.sandbox_dir(&handle.id).join("cgroup.procs");
        std::fs::write(&path, pid.to_string())
            .map_err(|error| SandboxError::Provisioner(format!("{}: {error}", path.display())))
    }
}

impl SandboxProvisioner for CgroupProvisioner {
    fn create(&self, spec: SandboxSpec) -> Result<SandboxHandle, SandboxError> {
        if !self.unchecked {
            let report = Self::probe(&self.root);
            if !report.ready {
                return Err(SandboxError::Unsupported(
                    report
                        .reason
                        .unwrap_or_else(|| "cgroup v2 is not ready".into()),
                ));
            }
        }
        let id = format!("sandbox_{}", self.next.get());
        self.next.set(self.next.get() + 1);
        let dir = self.sandbox_dir(&id);
        std::fs::create_dir_all(&dir)
            .map_err(|error| SandboxError::Provisioner(format!("{}: {error}", dir.display())))?;

        let limits = limits_for(&spec);
        for (file, value) in limits {
            if let Err(error) = std::fs::write(dir.join(file), value) {
                let _ = std::fs::remove_dir(&dir);
                return Err(SandboxError::Provisioner(format!(
                    "{}: {error}",
                    dir.join(file).display()
                )));
            }
        }
        Ok(SandboxHandle { id })
    }

    fn destroy(&self, handle: &SandboxHandle) -> Result<(), SandboxError> {
        let dir = self.sandbox_dir(&handle.id);
        if !dir.exists() {
            return Err(SandboxError::NotFound(handle.id.clone()));
        }
        // Kill anything left in the cgroup, then remove it. A cgroup cannot be
        // removed while it still has members.
        let _ = std::fs::write(dir.join("cgroup.kill"), "1");
        match std::fs::remove_dir(&dir) {
            Ok(()) => Ok(()),
            Err(error) => {
                // cgroupfs drops its pseudo-files with the directory; a plain
                // directory (tests, or a deliberately unprepared root) does
                // not, so fall back once.
                let _ = std::fs::remove_dir_all(&dir);
                if dir.exists() {
                    Err(SandboxError::Provisioner(format!(
                        "cannot remove {}: {error}",
                        dir.display()
                    )))
                } else {
                    Ok(())
                }
            }
        }
    }

    fn status(&self, handle: &SandboxHandle) -> Result<SandboxStatus, SandboxError> {
        let dir = self.sandbox_dir(&handle.id);
        if dir.exists() {
            Ok(SandboxStatus::Running)
        } else {
            Err(SandboxError::NotFound(handle.id.clone()))
        }
    }

    fn set_shares(&self, handle: &SandboxHandle, shares: u32) -> Result<(), SandboxError> {
        let dir = self.sandbox_dir(&handle.id);
        if !dir.exists() {
            return Err(SandboxError::NotFound(handle.id.clone()));
        }
        let weight = weight_for_shares(shares);
        std::fs::write(dir.join("cpu.weight"), weight.to_string()).map_err(|error| {
            SandboxError::Provisioner(format!("{}: {error}", dir.join("cpu.weight").display()))
        })
    }

    fn read_shares(&self, handle: &SandboxHandle) -> Result<u32, SandboxError> {
        let path = self.sandbox_dir(&handle.id).join("cpu.weight");
        let text = std::fs::read_to_string(&path)
            .map_err(|error| SandboxError::Provisioner(format!("{}: {error}", path.display())))?;
        let weight: u32 = text.trim().parse().map_err(|_| {
            SandboxError::Provisioner(format!("{} is not a number", path.display()))
        })?;
        // Back to the share basis so callers compare like with like. Round,
        // so weight_for_shares(read_shares(h)) == h for every h the kernel
        // stores exactly.
        Ok(((weight as u64 * SHARE_BASIS as u64 + 50) / 100) as u32)
    }
}

/// Files to write for a sandbox, in a stable order.
pub fn limits_for(spec: &SandboxSpec) -> Vec<(&'static str, String)> {
    let mut files = Vec::new();
    if let Some(shares) = spec.limits.cpu_shares {
        files.push(("cpu.weight", weight_for_shares(shares).to_string()));
    }
    files.push((
        "memory.max",
        spec.limits
            .memory_bytes
            .map(|bytes| bytes.to_string())
            .unwrap_or_else(|| "max".into()),
    ));
    files.push((
        "pids.max",
        spec.limits
            .pids
            .map(|pids| pids.to_string())
            .unwrap_or_else(|| "max".into()),
    ));
    files
}

/// Enable the controllers this provisioner needs on `dir`, then report what
/// children will actually have. Missing controllers are not an error here;
/// the caller decides whether it can proceed.
fn enable_controllers(dir: &Path) -> Result<Vec<String>, SandboxError> {
    let available = std::fs::read_to_string(dir.join("cgroup.controllers")).map_err(|error| {
        SandboxError::Provisioner(format!(
            "{} is not a cgroup directory: {error}",
            dir.display()
        ))
    })?;
    if available.contains("cpu") {
        let wanted: String = ["cpu", "cpuset", "memory", "pids"]
            .iter()
            .filter(|name| available.split_whitespace().any(|have| have == **name))
            .map(|name| format!("+{name} "))
            .collect();
        let _ = std::fs::write(dir.join("cgroup.subtree_control"), wanted.trim());
    }
    let effective = std::fs::read_to_string(dir.join("cgroup.controllers")).unwrap_or_default();
    Ok(effective.split_whitespace().map(str::to_string).collect())
}

/// cgroup directory names accept only a narrow alphabet.
fn sanitize(id: &str) -> String {
    id.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ResourceLimits, SandboxProfile};

    fn spec(shares: u32) -> SandboxSpec {
        let mut spec = SandboxSpec::new("user:ada", SandboxProfile::UserWrite, "sleep");
        spec.limits = ResourceLimits {
            cpu_shares: Some(shares),
            ..ResourceLimits::default()
        };
        spec
    }

    #[test]
    fn shares_map_onto_the_kernel_weight_range() {
        assert_eq!(weight_for_shares(1024), 100);
        assert_eq!(weight_for_shares(640), 62);
        assert_eq!(weight_for_shares(512), 50);
        assert_eq!(weight_for_shares(72), 7);
        assert_eq!(weight_for_shares(0), 1, "never 0: cgroup rejects it");
        assert_eq!(weight_for_shares(u32::MAX), 10_000, "clamped");
        // More shares must never mean less weight.
        let mut previous = 0;
        for shares in [1, 7, 32, 144, 240, 320, 640, 1024] {
            let weight = weight_for_shares(shares);
            assert!(weight >= previous);
            previous = weight;
        }
    }

    #[test]
    fn limits_include_the_declared_knobs() {
        let mut spec = spec(640);
        spec.limits.memory_bytes = Some(512 * 1024 * 1024);
        spec.limits.pids = Some(64);
        let files = limits_for(&spec);
        assert_eq!(
            files,
            vec![
                ("cpu.weight", "62".to_string()),
                ("memory.max", "536870912".to_string()),
                ("pids.max", "64".to_string()),
            ]
        );
    }

    #[test]
    fn missing_limits_become_max_not_zero() {
        let mut spec = spec(240);
        spec.limits.cpu_shares = None;
        spec.limits.memory_bytes = None;
        spec.limits.pids = None;
        let files = limits_for(&spec);
        assert!(!files.iter().any(|(file, _)| *file == "cpu.weight"));
        assert!(files.contains(&("memory.max", "max".to_string())));
        assert!(files.contains(&("pids.max", "max".to_string())));
    }

    #[test]
    fn probe_is_honest_when_cgroup_v2_is_absent() {
        let report = CgroupProvisioner::probe(Path::new("/nonexistent-tiaki-cgroup"));
        assert!(!report.ready);
        assert!(report.reason.is_some());
        assert_eq!(report.backend(), "simulated (no cgroup enforcement)");
    }

    #[test]
    fn a_plain_directory_is_not_a_cgroup() {
        let root = std::env::temp_dir().join(format!("tiaki-notcgroup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let report = CgroupProvisioner::probe(&root);
        assert!(!report.ready);
        // On Linux this fails the cgroup.controllers read; elsewhere the
        // mount check already answered.
        assert!(report.reason.is_some());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn unchecked_create_lays_out_the_files_and_reads_them_back() {
        let root = std::env::temp_dir().join(format!("tiaki-layout-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let provisioner = CgroupProvisioner::unchecked(&root);

        let handle = provisioner.create(spec(640)).expect("create");
        assert_eq!(provisioner.status(&handle).unwrap(), SandboxStatus::Running);
        let dir = root.join(&handle.id);
        assert_eq!(
            std::fs::read_to_string(dir.join("cpu.weight"))
                .unwrap()
                .trim(),
            "62"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("pids.max"))
                .unwrap()
                .trim(),
            "256"
        );

        provisioner.set_shares(&handle, 72).expect("shares");
        assert_eq!(
            std::fs::read_to_string(dir.join("cpu.weight"))
                .unwrap()
                .trim(),
            "7"
        );
        assert_eq!(provisioner.read_shares(&handle).unwrap(), 72);

        provisioner.destroy(&handle).expect("destroy");
        assert!(provisioner.status(&handle).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn ids_are_sanitized_for_cgroup_names() {
        assert_eq!(sanitize("sandbox_1"), "sandbox_1");
        assert_eq!(sanitize("a/b c"), "a_b_c");
    }
}
