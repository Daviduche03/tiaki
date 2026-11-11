//! Verified boot chain, generation loading, kexec, and health handoff.
//!
//! The real path runs `kexec` into the candidate kernel. On development hosts
//! `SimulatedKexec` records the boot instead, so the generation lifecycle and
//! health gates are testable everywhere.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use agentic_core_log::{digest, hex};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootImage {
    pub generation: String,
    pub root: PathBuf,
    pub kernel: PathBuf,
    pub initrd: Option<PathBuf>,
    pub cmdline: String,
}

impl BootImage {
    pub fn new(generation: impl Into<String>, root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let kernel = root.join("kernel");
        let initrd = root.join("initrd");
        Self {
            generation: generation.into(),
            root,
            kernel,
            initrd: if initrd.exists() { Some(initrd) } else { None },
            cmdline: "quiet".into(),
        }
    }

    pub fn validate(&self) -> Result<(), BootError> {
        if !self.root.is_dir() {
            return Err(BootError::MissingImage(format!(
                "{} is not a directory",
                self.root.display()
            )));
        }
        if !self.kernel.is_file() {
            return Err(BootError::MissingImage(format!(
                "kernel image {} is missing",
                self.kernel.display()
            )));
        }
        if let Some(initrd) = &self.initrd {
            if !initrd.is_file() {
                return Err(BootError::MissingImage(format!(
                    "initrd {} is missing",
                    initrd.display()
                )));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootTiming {
    pub started_at: u64,
    pub finished_at: u64,
    pub duration_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KexecError {
    pub message: String,
}

impl KexecError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for KexecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for KexecError {}

pub trait Kexec {
    fn name(&self) -> &str;
    fn boot(&mut self, image: &BootImage) -> Result<BootTiming, KexecError>;
}

/// Records the boot with a configurable simulated downtime.
pub struct SimulatedKexec {
    pub delay_ms: u64,
    pub fail: bool,
    pub boots: Vec<String>,
}

impl SimulatedKexec {
    pub fn new() -> Self {
        Self {
            delay_ms: 0,
            fail: false,
            boots: Vec::new(),
        }
    }

    pub fn with_delay(mut self, delay_ms: u64) -> Self {
        self.delay_ms = delay_ms;
        self
    }

    pub fn failing(mut self) -> Self {
        self.fail = true;
        self
    }
}

impl Default for SimulatedKexec {
    fn default() -> Self {
        Self::new()
    }
}

impl Kexec for SimulatedKexec {
    fn name(&self) -> &str {
        "simulated-kexec"
    }

    fn boot(&mut self, image: &BootImage) -> Result<BootTiming, KexecError> {
        image
            .validate()
            .map_err(|error| KexecError::new(error.to_string()))?;
        if self.fail {
            return Err(KexecError::new("simulated kexec failure"));
        }
        let started_at = now_millis();
        if self.delay_ms > 0 {
            std::thread::sleep(Duration::from_millis(self.delay_ms));
        }
        self.boots.push(image.generation.clone());
        let finished_at = now_millis();
        Ok(BootTiming {
            started_at,
            finished_at,
            duration_ms: finished_at.saturating_sub(started_at),
        })
    }
}

/// The real kexec path. It refuses to run unless `TIAKI_ALLOW_KEXEC=1` is
/// set, because `kexec -e` replaces the running kernel.
pub struct LinuxKexec {
    pub kexec: PathBuf,
}

impl LinuxKexec {
    pub fn new() -> Self {
        Self {
            kexec: PathBuf::from("/sbin/kexec"),
        }
    }
}

impl Default for LinuxKexec {
    fn default() -> Self {
        Self::new()
    }
}

impl Kexec for LinuxKexec {
    fn name(&self) -> &str {
        "linux-kexec"
    }

    fn boot(&mut self, image: &BootImage) -> Result<BootTiming, KexecError> {
        image
            .validate()
            .map_err(|error| KexecError::new(error.to_string()))?;

        #[cfg(target_os = "linux")]
        {
            if std::env::var("TIAKI_ALLOW_KEXEC").as_deref() != Ok("1") {
                return Err(KexecError::new(
                    "refusing to kexec; set TIAKI_ALLOW_KEXEC=1 to allow a real reboot",
                ));
            }
            let started_at = now_millis();
            let mut load = std::process::Command::new(&self.kexec);
            load.arg("--load")
                .arg(&image.kernel)
                .arg("--command-line")
                .arg(&image.cmdline);
            if let Some(initrd) = &image.initrd {
                load.arg("--initrd").arg(initrd);
            }
            let status = load
                .status()
                .map_err(|error| KexecError::new(format!("kexec --load: {error}")))?;
            if !status.success() {
                return Err(KexecError::new(format!(
                    "kexec --load exited with {status}"
                )));
            }
            let status = std::process::Command::new(&self.kexec)
                .arg("-e")
                .status()
                .map_err(|error| KexecError::new(format!("kexec -e: {error}")))?;
            if !status.success() {
                return Err(KexecError::new(format!("kexec -e exited with {status}")));
            }
            let finished_at = now_millis();
            Ok(BootTiming {
                started_at,
                finished_at,
                duration_ms: finished_at.saturating_sub(started_at),
            })
        }

        #[cfg(not(target_os = "linux"))]
        {
            Err(KexecError::new(
                "kexec is only available on Linux; use the simulated backend on this host",
            ))
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootChain {
    pub expected_digest: String,
}

impl BootChain {
    pub fn new(expected_digest: impl Into<String>) -> Self {
        Self {
            expected_digest: expected_digest.into(),
        }
    }

    /// Verify the image before boot. A mismatch means the image changed after
    /// staging and must not run.
    pub fn verify(&self, root: &Path) -> Result<String, BootError> {
        let actual = image_digest(root)?;
        if actual != self.expected_digest {
            return Err(BootError::DigestMismatch {
                expected: self.expected_digest.clone(),
                actual,
            });
        }
        Ok(actual)
    }
}

pub fn boot_generation(
    kexec: &mut dyn Kexec,
    image: &BootImage,
    expected_digest: &str,
) -> Result<BootTiming, BootError> {
    BootChain::new(expected_digest).verify(&image.root)?;
    image.validate()?;
    kexec.boot(image).map_err(BootError::Kexec)
}

/// A stable content digest for an image directory: sorted relative paths plus
/// each file's SHA-256, then a SHA-256 over that material.
pub fn image_digest(root: &Path) -> io::Result<String> {
    if !root.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} is not a directory", root.display()),
        ));
    }
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let mut material = Vec::new();
    for (relative, path) in files {
        let bytes = fs::read(&path)?;
        let relative = relative.to_string_lossy().replace('\\', "/");
        material.extend_from_slice(relative.as_bytes());
        material.push(0);
        material.extend_from_slice(hex(&digest(&bytes)).as_bytes());
        material.push(b'\n');
    }
    Ok(format!("sha256:{}", hex(&digest(&material))))
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<(PathBuf, PathBuf)>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("refusing symlink in image: {}", path.display()),
            ));
        }
        if file_type.is_dir() {
            collect_files(root, &path, out)?;
        } else if file_type.is_file() {
            let relative = path
                .strip_prefix(root)
                .map(|value| value.to_path_buf())
                .unwrap_or_else(|_| path.clone());
            out.push((relative, path));
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootContext {
    pub generation: String,
    pub label: String,
    pub boot_id: String,
    pub session: Option<String>,
    pub state: BTreeMap<String, String>,
    pub metadata: BTreeMap<String, String>,
}

pub trait HealthProbe {
    fn name(&self) -> &str;
    fn check(&self, context: &BootContext) -> Result<(), String>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HealthCheck {
    pub name: String,
    pub ok: bool,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HealthReport {
    pub ok: bool,
    pub checks: Vec<HealthCheck>,
}

pub struct BootHealth {
    probes: Vec<Box<dyn HealthProbe>>,
}

impl BootHealth {
    pub fn new(probes: Vec<Box<dyn HealthProbe>>) -> Self {
        Self { probes }
    }

    pub fn run(&self, context: &BootContext) -> HealthReport {
        let mut checks = Vec::new();
        for probe in &self.probes {
            match probe.check(context) {
                Ok(()) => checks.push(HealthCheck {
                    name: probe.name().to_string(),
                    ok: true,
                    reason: String::new(),
                }),
                Err(reason) => checks.push(HealthCheck {
                    name: probe.name().to_string(),
                    ok: false,
                    reason,
                }),
            }
        }
        HealthReport {
            ok: checks.iter().all(|check| check.ok),
            checks,
        }
    }
}

#[derive(Debug)]
pub enum BootError {
    Io(io::Error),
    MissingImage(String),
    DigestMismatch { expected: String, actual: String },
    Kexec(KexecError),
}

impl fmt::Display for BootError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BootError::Io(error) => write!(f, "io error: {error}"),
            BootError::MissingImage(reason) => write!(f, "missing image: {reason}"),
            BootError::DigestMismatch { expected, actual } => {
                write!(
                    f,
                    "image digest mismatch: expected {expected}, got {actual}"
                )
            }
            BootError::Kexec(error) => write!(f, "kexec failed: {error}"),
        }
    }
}

impl std::error::Error for BootError {}

impl From<io::Error> for BootError {
    fn from(error: io::Error) -> Self {
        BootError::Io(error)
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "tiaki-boot-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("kernel"), "vmlinuz placeholder").unwrap();
        fs::write(dir.join("initrd"), "initrd placeholder").unwrap();
        dir
    }

    #[test]
    fn digest_changes_when_the_image_changes() {
        let dir = image_dir("digest");
        let first = image_digest(&dir).expect("digest");
        fs::write(dir.join("kernel"), "tampered").unwrap();
        let second = image_digest(&dir).expect("digest");
        assert_ne!(first, second);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn boot_verifies_before_kexec() {
        let dir = image_dir("verify");
        let digest = image_digest(&dir).expect("digest");
        let image = BootImage::new("gen_1", &dir);
        let mut kexec = SimulatedKexec::new();

        boot_generation(&mut kexec, &image, &digest).expect("boot");
        assert_eq!(kexec.boots, vec!["gen_1"]);

        fs::write(dir.join("kernel"), "tampered").unwrap();
        let error = boot_generation(&mut kexec, &image, &digest).expect_err("tamper");
        assert!(matches!(error, BootError::DigestMismatch { .. }));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn health_reports_every_probe() {
        struct Pass;
        struct Fail;
        impl HealthProbe for Pass {
            fn name(&self) -> &str {
                "pass"
            }
            fn check(&self, _: &BootContext) -> Result<(), String> {
                Ok(())
            }
        }
        impl HealthProbe for Fail {
            fn name(&self) -> &str {
                "fail"
            }
            fn check(&self, _: &BootContext) -> Result<(), String> {
                Err("broken".into())
            }
        }

        let health = BootHealth::new(vec![Box::new(Pass), Box::new(Fail)]);
        let context = BootContext {
            generation: "gen_1".into(),
            label: "test".into(),
            boot_id: "boot_1".into(),
            session: None,
            state: BTreeMap::new(),
            metadata: BTreeMap::new(),
        };
        let report = health.run(&context);
        assert!(!report.ok);
        assert_eq!(report.checks.len(), 2);
        assert!(report
            .checks
            .iter()
            .any(|check| check.name == "fail" && !check.ok));
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn real_kexec_is_unavailable_off_linux() {
        let dir = image_dir("kexec");
        let image = BootImage::new("gen_1", &dir);
        let mut kexec = LinuxKexec::new();
        let error = kexec.boot(&image).expect_err("unsupported");
        assert!(error.message.contains("Linux"));
        let _ = fs::remove_dir_all(&dir);
    }
}
