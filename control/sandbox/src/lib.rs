//! Sandbox profile resolution and the pluggable provisioner ABI.
//!
//! Every agent action runs inside a profile, in both access modes. Full access
//! is a wider sandbox, not the absence of one. See TIAKI.md §9.6.

#![allow(dead_code)]

pub mod cgroup;

use std::cell::RefCell;
use std::collections::HashMap;

use agentic_transaction::{AccessMode, SandboxClass};

pub use cgroup::{
    root_from_env, weight_for_shares, CgroupProvisioner, CgroupReport, DEFAULT_ROOT, ROOT_ENV,
    SHARE_BASIS,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SandboxProfile {
    Inspect,
    UserWrite,
    SystemWrite,
    Privileged,
}

impl SandboxProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            SandboxProfile::Inspect => "inspect",
            SandboxProfile::UserWrite => "user-write",
            SandboxProfile::SystemWrite => "system-write",
            SandboxProfile::Privileged => "privileged",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "inspect" => Some(SandboxProfile::Inspect),
            "user-write" => Some(SandboxProfile::UserWrite),
            "system-write" => Some(SandboxProfile::SystemWrite),
            "privileged" => Some(SandboxProfile::Privileged),
            _ => None,
        }
    }

    pub fn is_read_only(self) -> bool {
        self == SandboxProfile::Inspect
    }
}

impl From<SandboxClass> for SandboxProfile {
    fn from(class: SandboxClass) -> Self {
        match class {
            SandboxClass::Inspect => SandboxProfile::Inspect,
            SandboxClass::UserWrite => SandboxProfile::UserWrite,
            SandboxClass::SystemWrite => SandboxProfile::SystemWrite,
            SandboxClass::Privileged => SandboxProfile::Privileged,
        }
    }
}

impl From<SandboxProfile> for SandboxClass {
    fn from(profile: SandboxProfile) -> Self {
        match profile {
            SandboxProfile::Inspect => SandboxClass::Inspect,
            SandboxProfile::UserWrite => SandboxClass::UserWrite,
            SandboxProfile::SystemWrite => SandboxClass::SystemWrite,
            SandboxProfile::Privileged => SandboxClass::Privileged,
        }
    }
}

/// Access mode changes approval policy and capability scope, never whether a
/// sandbox exists.
pub fn resolve_profile(_mode: AccessMode, class: SandboxClass) -> SandboxProfile {
    SandboxProfile::from(class)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceLimits {
    pub cpu_ms: Option<u64>,
    pub memory_bytes: Option<u64>,
    pub pids: Option<u32>,
    pub network: bool,
    /// Scheduler weight (cgroup cpu.shares style). `None` means the
    /// provisioner's default; the agent supervisor stamps the rebalanced
    /// share on every agent session so the backend can apply it for real.
    pub cpu_shares: Option<u32>,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            cpu_ms: None,
            memory_bytes: Some(2 * 1024 * 1024 * 1024),
            pids: Some(256),
            network: false,
            cpu_shares: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandboxSpec {
    pub principal: String,
    pub profile: SandboxProfile,
    pub command: String,
    pub args: Vec<String>,
    pub read_only: Vec<String>,
    pub read_write: Vec<String>,
    pub capabilities: Vec<String>,
    pub limits: ResourceLimits,
    pub timeout_secs: u64,
}

impl SandboxSpec {
    pub fn new(
        principal: impl Into<String>,
        profile: SandboxProfile,
        command: impl Into<String>,
    ) -> Self {
        Self {
            principal: principal.into(),
            profile,
            command: command.into(),
            args: Vec::new(),
            read_only: Vec::new(),
            read_write: Vec::new(),
            capabilities: Vec::new(),
            limits: ResourceLimits::default(),
            timeout_secs: 30,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SandboxHandle {
    pub id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SandboxStatus {
    Created,
    Running,
    Exited { code: i32 },
    Failed(String),
}

#[derive(Debug)]
pub enum SandboxError {
    Unsupported(String),
    Provisioner(String),
    NotFound(String),
}

impl std::fmt::Display for SandboxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SandboxError::Unsupported(reason) => write!(f, "unsupported: {reason}"),
            SandboxError::Provisioner(reason) => write!(f, "provisioner error: {reason}"),
            SandboxError::NotFound(id) => write!(f, "unknown sandbox {id}"),
        }
    }
}

impl std::error::Error for SandboxError {}

/// The ABI a Rust module or an external service can implement. The Go
/// provisioner slots in here over IPC.
pub trait SandboxProvisioner {
    fn create(&self, spec: SandboxSpec) -> Result<SandboxHandle, SandboxError>;
    fn destroy(&self, handle: &SandboxHandle) -> Result<(), SandboxError>;
    fn status(&self, handle: &SandboxHandle) -> Result<SandboxStatus, SandboxError>;

    /// Re-weight a running sandbox. Default: this backend cannot enforce
    /// shares, and says so rather than silently accepting the number.
    fn set_shares(&self, _handle: &SandboxHandle, _shares: u32) -> Result<(), SandboxError> {
        Err(SandboxError::Unsupported(
            "this provisioner cannot enforce CPU shares".into(),
        ))
    }

    /// Read the enforced share weight back, in the same 1024 basis.
    fn read_shares(&self, _handle: &SandboxHandle) -> Result<u32, SandboxError> {
        Err(SandboxError::Unsupported(
            "this provisioner cannot read CPU shares".into(),
        ))
    }
}

/// Records what would have run. Used on hosts without a real provisioner and
/// in tests, so the control plane can be exercised everywhere.
pub struct SimulatedProvisioner {
    next: RefCell<u64>,
    specs: RefCell<HashMap<String, SandboxSpec>>,
    statuses: RefCell<HashMap<String, SandboxStatus>>,
    shares: RefCell<HashMap<String, u32>>,
}

impl Default for SimulatedProvisioner {
    fn default() -> Self {
        Self::new()
    }
}

impl SimulatedProvisioner {
    pub fn new() -> Self {
        Self {
            next: RefCell::new(1),
            specs: RefCell::new(HashMap::new()),
            statuses: RefCell::new(HashMap::new()),
            shares: RefCell::new(HashMap::new()),
        }
    }

    pub fn recorded(&self) -> Vec<SandboxSpec> {
        let specs = self.specs.borrow();
        let mut values: Vec<SandboxSpec> = specs.values().cloned().collect();
        values.sort_by(|a, b| a.command.cmp(&b.command));
        values
    }

    /// The weights this backend was asked to apply. The simulated backend
    /// records them so tests can assert the supervisor actually calls
    /// `set_shares` instead of only changing its own bookkeeping.
    pub fn shares(&self, handle: &SandboxHandle) -> Option<u32> {
        self.shares.borrow().get(&handle.id).copied()
    }
}

impl SandboxProvisioner for SimulatedProvisioner {
    fn create(&self, spec: SandboxSpec) -> Result<SandboxHandle, SandboxError> {
        let mut next = self.next.borrow_mut();
        let id = format!("sandbox_{}", *next);
        *next += 1;
        self.statuses
            .borrow_mut()
            .insert(id.clone(), SandboxStatus::Running);
        self.specs.borrow_mut().insert(id.clone(), spec);
        Ok(SandboxHandle { id })
    }

    fn destroy(&self, handle: &SandboxHandle) -> Result<(), SandboxError> {
        self.specs
            .borrow_mut()
            .remove(&handle.id)
            .ok_or_else(|| SandboxError::NotFound(handle.id.clone()))?;
        self.statuses.borrow_mut().remove(&handle.id);
        Ok(())
    }

    fn status(&self, handle: &SandboxHandle) -> Result<SandboxStatus, SandboxError> {
        self.statuses
            .borrow()
            .get(&handle.id)
            .cloned()
            .ok_or_else(|| SandboxError::NotFound(handle.id.clone()))
    }

    fn set_shares(&self, handle: &SandboxHandle, shares: u32) -> Result<(), SandboxError> {
        if !self.specs.borrow().contains_key(&handle.id) {
            return Err(SandboxError::NotFound(handle.id.clone()));
        }
        self.shares.borrow_mut().insert(handle.id.clone(), shares);
        Ok(())
    }

    fn read_shares(&self, handle: &SandboxHandle) -> Result<u32, SandboxError> {
        self.shares
            .borrow()
            .get(&handle.id)
            .copied()
            .ok_or_else(|| SandboxError::NotFound(handle.id.clone()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellRequest {
    pub principal: String,
    pub profile: SandboxProfile,
    pub command: String,
    pub args: Vec<String>,
    pub network: bool,
    pub timeout_secs: u64,
}

impl ShellRequest {
    pub fn new(
        principal: impl Into<String>,
        profile: SandboxProfile,
        command: impl Into<String>,
    ) -> Self {
        Self {
            principal: principal.into(),
            profile,
            command: command.into(),
            args: Vec::new(),
            network: false,
            timeout_secs: 30,
        }
    }
}

pub struct ShellTool<P: SandboxProvisioner> {
    provisioner: P,
}

impl<P: SandboxProvisioner> ShellTool<P> {
    pub fn new(provisioner: P) -> Self {
        Self { provisioner }
    }

    pub fn plan(&self, request: &ShellRequest) -> SandboxSpec {
        plan_shell(request)
    }

    pub fn run(&self, request: &ShellRequest) -> Result<SandboxHandle, SandboxError> {
        self.provisioner.create(plan_shell(request))
    }

    pub fn provisioner(&self) -> &P {
        &self.provisioner
    }
}

/// Turn a shell request into the profile's sandbox spec. Pure, so planners and
/// supervisors share exactly one definition of what a profile grants.
pub fn plan_shell(request: &ShellRequest) -> SandboxSpec {
    let mut spec = SandboxSpec::new(
        request.principal.clone(),
        request.profile,
        request.command.clone(),
    );
    spec.args = request.args.clone();
    spec.timeout_secs = request.timeout_secs;
    spec.limits.network = request.network;

    match request.profile {
        SandboxProfile::Inspect => {
            spec.read_only = vec!["/".into(), "/run/tiaki/state".into()];
        }
        SandboxProfile::UserWrite => {
            spec.read_only = vec!["/usr".into(), "/etc".into(), "/run/tiaki/state".into()];
            if let Some(home) = user_home(&request.principal) {
                spec.read_write.push(home);
            }
            spec.read_write.push("/run/tiaki/user".into());
        }
        SandboxProfile::SystemWrite => {
            spec.read_only = vec!["/usr".into()];
            spec.read_write = vec!["/etc".into(), "/var".into(), "/run/tiaki/system".into()];
        }
        SandboxProfile::Privileged => {
            spec.read_only = vec!["/".into()];
            spec.read_write = vec!["/run/tiaki/broker".into()];
            spec.capabilities = vec!["broker.mediated".into()];
            // Privileged operations are brokered, never raw root.
            spec.limits.network = false;
        }
    }

    spec
}

fn user_home(principal: &str) -> Option<String> {
    if !principal.contains("user:") {
        return None;
    }
    let tail = principal.rsplit("user:").next()?;
    let id = tail
        .split(['@', '/'])
        .next()?;
    if id.is_empty() {
        None
    } else {
        Some(format!("/home/{id}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_class_maps_to_a_profile() {
        for class in [
            SandboxClass::Inspect,
            SandboxClass::UserWrite,
            SandboxClass::SystemWrite,
            SandboxClass::Privileged,
        ] {
            let profile = resolve_profile(AccessMode::Full, class);
            assert_eq!(profile.as_str(), class.as_str());
        }
    }

    #[test]
    fn full_mode_is_still_sandboxed() {
        let profile = resolve_profile(AccessMode::Full, SandboxClass::SystemWrite);
        assert_eq!(profile, SandboxProfile::SystemWrite);
        assert!(!profile.is_read_only());

        let tool = ShellTool::new(SimulatedProvisioner::new());
        let request = ShellRequest::new("user:ada", profile, "systemctl");
        let spec = tool.plan(&request);
        assert_eq!(spec.profile, SandboxProfile::SystemWrite);
        assert!(!spec.read_write.is_empty());
        assert!(spec.limits.memory_bytes.is_some());
    }

    #[test]
    fn user_write_profile_scopes_the_home_directory() {
        let tool = ShellTool::new(SimulatedProvisioner::new());
        let request = ShellRequest::new(
            "agent:opencode:ses_1@user:ada",
            SandboxProfile::UserWrite,
            "bash",
        );
        let spec = tool.plan(&request);
        assert!(spec.read_write.contains(&"/home/ada".to_string()));
        assert!(spec.read_only.contains(&"/usr".to_string()));
    }

    #[test]
    fn simulated_provisioner_records_and_destroys() {
        let tool = ShellTool::new(SimulatedProvisioner::new());
        let request = ShellRequest::new("user:ada", SandboxProfile::Inspect, "ls");
        let handle = tool.run(&request).expect("create");
        assert_eq!(
            tool.provisioner().status(&handle).unwrap(),
            SandboxStatus::Running
        );
        assert_eq!(tool.provisioner().recorded().len(), 1);
        tool.provisioner().destroy(&handle).expect("destroy");
        assert!(tool.provisioner().status(&handle).is_err());
    }

    #[test]
    fn privileged_is_brokered_not_raw_root() {
        let tool = ShellTool::new(SimulatedProvisioner::new());
        let request = ShellRequest::new("system", SandboxProfile::Privileged, "modprobe");
        let spec = tool.plan(&request);
        assert_eq!(spec.capabilities, vec!["broker.mediated".to_string()]);
        assert!(!spec.limits.network);
    }
}
