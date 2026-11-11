//! Principals, capability grants, and policy evaluation.
//!
//! A capability is a scoped, revocable grant. An agent never has independent
//! authority: it borrows the authority of the principal it acts for.
//!
//! See TIAKI.md §9.

#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use agentic_transaction::{AccessMode, Authorization, Authorizer, Operation, SandboxClass};

// ---------------------------------------------------------------------------
// Principals
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Principal {
    System,
    User(String),
    Service(String),
    Agent {
        runtime: String,
        session: String,
        acting_for: Box<Principal>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrincipalError {
    pub input: String,
    pub reason: String,
}

impl std::fmt::Display for PrincipalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid principal '{}': {}", self.input, self.reason)
    }
}

impl std::error::Error for PrincipalError {}

impl Principal {
    /// Accepts `system`, `user:<id>`, `service:<id>`, and
    /// `agent:<runtime>:<session>@<principal>`.
    pub fn parse(value: &str) -> Result<Self, PrincipalError> {
        let fail = |reason: &str| PrincipalError {
            input: value.to_string(),
            reason: reason.to_string(),
        };

        if value == "system" {
            return Ok(Principal::System);
        }
        if let Some(id) = value.strip_prefix("user:") {
            return Ok(Principal::User(
                valid_id(id).ok_or_else(|| fail("empty user id"))?,
            ));
        }
        if let Some(id) = value.strip_prefix("service:") {
            return Ok(Principal::Service(
                valid_id(id).ok_or_else(|| fail("empty service id"))?,
            ));
        }
        if let Some(rest) = value.strip_prefix("agent:") {
            let (head, delegate) = rest
                .split_once('@')
                .ok_or_else(|| fail("agent principal must end with @<principal>"))?;
            let (runtime, session) = head.split_once(':').ok_or_else(|| {
                fail("agent principal must be agent:<runtime>:<session>@<principal>")
            })?;
            let acting_for = Principal::parse(delegate)?;
            return Ok(Principal::Agent {
                runtime: valid_id(runtime).ok_or_else(|| fail("empty runtime"))?,
                session: valid_id(session).ok_or_else(|| fail("empty session"))?,
                acting_for: Box::new(acting_for),
            });
        }
        Err(fail(
            "expected system, user:<id>, service:<id>, or agent:<runtime>:<session>@<principal>",
        ))
    }

    /// The principal whose authority this one borrows.
    pub fn effective(&self) -> &Principal {
        match self {
            Principal::Agent { acting_for, .. } => acting_for.effective(),
            other => other,
        }
    }

    pub fn is_system(&self) -> bool {
        matches!(self.effective(), Principal::System)
    }

    pub fn user_id(&self) -> Option<&str> {
        match self.effective() {
            Principal::User(id) => Some(id),
            _ => None,
        }
    }
}

impl std::fmt::Display for Principal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Principal::System => write!(f, "system"),
            Principal::User(id) => write!(f, "user:{id}"),
            Principal::Service(id) => write!(f, "service:{id}"),
            Principal::Agent {
                runtime,
                session,
                acting_for,
            } => write!(f, "agent:{runtime}:{session}@{acting_for}"),
        }
    }
}

fn valid_id(value: &str) -> Option<String> {
    if value.is_empty()
        || value.contains(':')
        || value.contains('@')
        || value.contains(char::is_whitespace)
    {
        None
    } else {
        Some(value.to_string())
    }
}

// ---------------------------------------------------------------------------
// Actions and resource patterns
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ActionClass {
    StateRead,
    StateWrite,
    StateRevert,
    ShellExec,
    ServiceManage,
    PluginManage,
    InferenceGenerate,
    InferenceManage,
    ComputerUse,
    GenerationManage,
    SystemAdmin,
}

impl ActionClass {
    pub fn as_str(self) -> &'static str {
        match self {
            ActionClass::StateRead => "state.read",
            ActionClass::StateWrite => "state.write",
            ActionClass::StateRevert => "state.revert",
            ActionClass::ShellExec => "shell.exec",
            ActionClass::ServiceManage => "service.manage",
            ActionClass::PluginManage => "plugin.manage",
            ActionClass::InferenceGenerate => "inference.generate",
            ActionClass::InferenceManage => "inference.manage",
            ActionClass::ComputerUse => "computer.use",
            ActionClass::GenerationManage => "generation.manage",
            ActionClass::SystemAdmin => "system.admin",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "state.read" => Some(ActionClass::StateRead),
            "state.write" => Some(ActionClass::StateWrite),
            "state.revert" => Some(ActionClass::StateRevert),
            "shell.exec" => Some(ActionClass::ShellExec),
            "service.manage" => Some(ActionClass::ServiceManage),
            "plugin.manage" => Some(ActionClass::PluginManage),
            "inference.generate" => Some(ActionClass::InferenceGenerate),
            "inference.manage" => Some(ActionClass::InferenceManage),
            "computer.use" => Some(ActionClass::ComputerUse),
            "generation.manage" => Some(ActionClass::GenerationManage),
            "system.admin" => Some(ActionClass::SystemAdmin),
            _ => None,
        }
    }
}

/// Segment-aware glob: `*` matches one segment, `**` matches zero or more.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pattern {
    pub raw: String,
}

impl Pattern {
    pub fn new(raw: impl Into<String>) -> Self {
        Self { raw: raw.into() }
    }

    pub fn matches(&self, value: &str) -> bool {
        let pattern: Vec<&str> = self.raw.split('/').collect();
        let value: Vec<&str> = value.split('/').collect();
        match_segments(&pattern, &value)
    }

    pub fn is_universal(&self) -> bool {
        self.raw == "**" || self.raw == "*"
    }
}

fn match_segments(pattern: &[&str], value: &[&str]) -> bool {
    match pattern.split_first() {
        None => value.is_empty(),
        Some((&"**", rest)) => {
            if match_segments(rest, value) {
                return true;
            }
            !value.is_empty() && match_segments(pattern, &value[1..])
        }
        Some((&"*", rest)) => !value.is_empty() && match_segments(rest, &value[1..]),
        Some((segment, rest)) => {
            value.first().is_some_and(|candidate| candidate == segment)
                && match_segments(rest, &value[1..])
        }
    }
}

// ---------------------------------------------------------------------------
// Grants
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grant {
    pub principal: Principal,
    pub action: ActionClass,
    pub resource: Pattern,
    pub requires_approval: bool,
    pub max_per_minute: Option<u32>,
    pub granted_by: String,
    pub expires_at: Option<u64>,
}

impl Grant {
    pub fn new(
        principal: Principal,
        action: ActionClass,
        resource: impl Into<String>,
        granted_by: impl Into<String>,
    ) -> Self {
        Self {
            principal,
            action,
            resource: Pattern::new(resource),
            requires_approval: false,
            max_per_minute: None,
            granted_by: granted_by.into(),
            expires_at: None,
        }
    }

    pub fn requiring_approval(mut self) -> Self {
        self.requires_approval = true;
        self
    }

    pub fn limited_to(mut self, per_minute: u32) -> Self {
        self.max_per_minute = Some(per_minute);
        self
    }

    pub fn expiring_at(mut self, unix_secs: u64) -> Self {
        self.expires_at = Some(unix_secs);
        self
    }

    fn active_at(&self, now: u64) -> bool {
        self.expires_at.map_or(true, |expiry| now < expiry)
    }
}

// ---------------------------------------------------------------------------
// Policy
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum PolicyError {
    Io(io::Error),
    Parse { line: usize, reason: String },
}

impl std::fmt::Display for PolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PolicyError::Io(error) => write!(f, "io error: {error}"),
            PolicyError::Parse { line, reason } => {
                write!(f, "could not parse grant on line {line}: {reason}")
            }
        }
    }
}

impl std::error::Error for PolicyError {}

impl From<io::Error> for PolicyError {
    fn from(error: io::Error) -> Self {
        PolicyError::Io(error)
    }
}

pub struct Policy {
    grants: Vec<Grant>,
    /// (principal, action, resource) -> timestamps of recent uses.
    usage: RefCell<HashMap<String, Vec<u64>>>,
    path: Option<PathBuf>,
}

impl Default for Policy {
    fn default() -> Self {
        Self::new()
    }
}

impl Policy {
    pub fn new() -> Self {
        Self {
            grants: Vec::new(),
            usage: RefCell::new(HashMap::new()),
            path: None,
        }
    }

    /// The demo policy: each user owns `user/<id>/**`, system owns everything,
    /// and cross-boundary writes require approval in guarded mode.
    pub fn with_defaults() -> Self {
        let mut policy = Self::new();
        for user in ["local", "ada", "bob"] {
            policy.grant_user(user);
        }
        policy.grant_system();
        policy.grant_service("network");
        policy.grant_service("inference");
        policy.grant_service("update");
        policy
    }

    pub fn grant_user(&mut self, user: &str) {
        let principal = Principal::User(user.to_string());
        let owner = format!("user/{user}/**");
        self.add(Grant::new(
            principal.clone(),
            ActionClass::StateRead,
            "**",
            "system",
        ));
        self.add(Grant::new(
            principal.clone(),
            ActionClass::StateWrite,
            owner.clone(),
            "system",
        ));
        self.add(Grant::new(
            principal.clone(),
            ActionClass::StateRevert,
            owner.clone(),
            "system",
        ));
        self.add(Grant::new(
            principal.clone(),
            ActionClass::ShellExec,
            owner,
            "system",
        ));
        self.add(Grant::new(
            principal.clone(),
            ActionClass::InferenceGenerate,
            "model/**",
            "system",
        ));
        self.add(Grant::new(
            principal,
            ActionClass::ComputerUse,
            "app/**",
            "system",
        ));
    }

    pub fn grant_system(&mut self) {
        for action in [
            ActionClass::StateRead,
            ActionClass::StateWrite,
            ActionClass::StateRevert,
            ActionClass::ShellExec,
            ActionClass::ServiceManage,
            ActionClass::PluginManage,
            ActionClass::InferenceGenerate,
            ActionClass::InferenceManage,
            ActionClass::ComputerUse,
            ActionClass::GenerationManage,
            ActionClass::SystemAdmin,
        ] {
            self.add(Grant::new(Principal::System, action, "**", "system"));
        }
    }

    pub fn grant_service(&mut self, service: &str) {
        self.add(Grant::new(
            Principal::Service(service.to_string()),
            ActionClass::StateWrite,
            format!("system/services/{service}/**"),
            "system",
        ));
        self.add(Grant::new(
            Principal::Service(service.to_string()),
            ActionClass::StateRead,
            "**",
            "system",
        ));
        if service == "inference" {
            self.add(Grant::new(
                Principal::Service(service.to_string()),
                ActionClass::StateWrite,
                "system/inference/**",
                "system",
            ));
            self.add(Grant::new(
                Principal::Service(service.to_string()),
                ActionClass::InferenceManage,
                "system/inference/**",
                "system",
            ));
            self.add(Grant::new(
                Principal::Service(service.to_string()),
                ActionClass::InferenceGenerate,
                "model/**",
                "system",
            ));
        }
        if service == "update" {
            self.add(Grant::new(
                Principal::Service(service.to_string()),
                ActionClass::GenerationManage,
                "system/generations/**",
                "system",
            ));
        }
    }

    /// Add or replace a grant with the same principal, action, and resource.
    pub fn add(&mut self, grant: Grant) {
        self.grants.retain(|existing| {
            !(existing.principal == grant.principal
                && existing.action == grant.action
                && existing.resource == grant.resource)
        });
        self.grants.push(grant);
    }

    pub fn revoke(&mut self, principal: &str, action: ActionClass, resource: &str) -> bool {
        let Ok(principal) = Principal::parse(principal) else {
            return false;
        };
        let before = self.grants.len();
        self.grants.retain(|grant| {
            !(grant.principal == principal
                && grant.action == action
                && grant.resource.raw == resource)
        });
        self.grants.len() != before
    }

    pub fn grants(&self) -> &[Grant] {
        &self.grants
    }

    /// A user may only grant resources they own; system may grant anything.
    pub fn can_grant(&self, grantor: &str, resource: &str) -> bool {
        let Ok(grantor) = Principal::parse(grantor) else {
            return false;
        };
        if grantor.is_system() {
            return true;
        }
        match grantor.effective() {
            Principal::User(id) => {
                resource == format!("user/{id}/**") || resource.starts_with(&format!("user/{id}/"))
            }
            _ => false,
        }
    }

    pub fn find_grant(
        &self,
        principal: &Principal,
        action: ActionClass,
        resource: &str,
        now: u64,
    ) -> Option<&Grant> {
        self.grants
            .iter()
            .filter(|grant| {
                &grant.principal == principal
                    && grant.action == action
                    && grant.resource.matches(resource)
                    && grant.active_at(now)
            })
            .max_by_key(|grant| grant.resource.raw.len())
    }

    /// Authorize a single action that is not a state operation, such as
    /// `shell.exec`. The transaction engine uses `authorize` instead.
    pub fn authorize_action(
        &self,
        principal: &str,
        mode: AccessMode,
        action: ActionClass,
        resource: &str,
        now: u64,
    ) -> Authorization {
        let principal = match Principal::parse(principal) {
            Ok(principal) => principal,
            Err(error) => return Authorization::deny(SandboxClass::Inspect, error.to_string()),
        };
        let effective = principal.effective().clone();

        if is_recovery_path(resource) {
            return Authorization::deny(
                SandboxClass::Privileged,
                format!("'{resource}' is part of the recovery core and is immutable at runtime"),
            );
        }

        let sandbox = required_sandbox(action, resource);
        match self.find_grant(&effective, action, resource, now) {
            Some(grant) => {
                if grant.requires_approval && mode == AccessMode::Guarded {
                    Authorization::approval(
                        sandbox,
                        format!("{} requires approval in guarded mode", principal),
                    )
                } else {
                    Authorization::allow(sandbox)
                }
            }
            None => Authorization::deny(
                sandbox,
                format!(
                    "{} has no capability for {} on {}",
                    principal,
                    action.as_str(),
                    resource
                ),
            ),
        }
    }

    fn consume_grant(&self, principal: &Principal, grant: &Grant, now: u64) -> bool {
        let Some(limit) = grant.max_per_minute else {
            return true;
        };
        let key = format!(
            "{}|{}|{}",
            principal,
            grant.action.as_str(),
            grant.resource.raw
        );
        let mut usage = self.usage.borrow_mut();
        let timestamps = usage.entry(key).or_default();
        timestamps.retain(|timestamp| now.saturating_sub(*timestamp) < 60);
        if timestamps.len() as u32 >= limit {
            return false;
        }
        timestamps.push(now);
        true
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, PolicyError> {
        let path = path.as_ref().to_path_buf();
        let mut policy = Self::new();
        if path.exists() {
            let file = fs::File::open(&path)?;
            for (index, line) in BufReader::new(file).lines().enumerate() {
                let line = line?;
                if line.trim().is_empty() || line.starts_with('#') {
                    continue;
                }
                let grant = parse_grant_line(&line).map_err(|reason| PolicyError::Parse {
                    line: index + 1,
                    reason,
                })?;
                policy.add(grant);
            }
        }
        policy.path = Some(path);
        Ok(policy)
    }

    pub fn load_or_default(path: impl AsRef<Path>) -> Result<Self, PolicyError> {
        let path = path.as_ref().to_path_buf();
        let mut policy = if path.exists() {
            Self::load(&path)?
        } else {
            let mut policy = Self::with_defaults();
            policy.path = Some(path.clone());
            policy.save()?;
            policy
        };
        policy.path = Some(path);
        Ok(policy)
    }

    pub fn save(&self) -> Result<(), PolicyError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = String::new();
        for grant in &self.grants {
            out.push_str(&format_grant_line(grant));
            out.push('\n');
        }
        let mut file = fs::File::create(path)?;
        file.write_all(out.as_bytes())?;
        file.flush()?;
        Ok(())
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
}

fn format_grant_line(grant: &Grant) -> String {
    format!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{}",
        grant.principal,
        grant.granted_by,
        grant.action.as_str(),
        grant.resource.raw,
        grant.requires_approval,
        grant
            .max_per_minute
            .map(|value| value.to_string())
            .unwrap_or_default(),
        grant
            .expires_at
            .map(|value| value.to_string())
            .unwrap_or_default(),
    )
}

fn parse_grant_line(line: &str) -> Result<Grant, String> {
    let fields: Vec<&str> = line.split('\t').collect();
    if fields.len() != 7 {
        return Err(format!("expected 7 fields, found {}", fields.len()));
    }
    let principal = Principal::parse(fields[0]).map_err(|error| error.to_string())?;
    let action =
        ActionClass::parse(fields[2]).ok_or_else(|| format!("unknown action '{}'", fields[2]))?;
    let requires_approval = fields[4]
        .parse()
        .map_err(|_| format!("invalid approval flag '{}'", fields[4]))?;
    let max_per_minute = if fields[5].is_empty() {
        None
    } else {
        Some(
            fields[5]
                .parse()
                .map_err(|_| format!("invalid rate limit '{}'", fields[5]))?,
        )
    };
    let expires_at = if fields[6].is_empty() {
        None
    } else {
        Some(
            fields[6]
                .parse()
                .map_err(|_| format!("invalid expiry '{}'", fields[6]))?,
        )
    };
    Ok(Grant {
        principal,
        action,
        resource: Pattern::new(fields[3]),
        requires_approval,
        max_per_minute,
        granted_by: fields[1].to_string(),
        expires_at,
    })
}

// ---------------------------------------------------------------------------
// Authorization
// ---------------------------------------------------------------------------

fn is_recovery_path(path: &str) -> bool {
    path.starts_with("core/")
        || path.starts_with("boot/")
        || path.starts_with("recovery/")
        || path.starts_with("system/recovery/")
}

fn required_sandbox(action: ActionClass, path: &str) -> SandboxClass {
    match action {
        ActionClass::StateRead => SandboxClass::Inspect,
        ActionClass::ShellExec => SandboxClass::UserWrite,
        ActionClass::SystemAdmin => SandboxClass::Privileged,
        ActionClass::ServiceManage | ActionClass::PluginManage => SandboxClass::SystemWrite,
        ActionClass::InferenceGenerate => SandboxClass::Inspect,
        ActionClass::InferenceManage => SandboxClass::SystemWrite,
        ActionClass::ComputerUse => SandboxClass::UserWrite,
        ActionClass::GenerationManage => SandboxClass::SystemWrite,
        ActionClass::StateWrite | ActionClass::StateRevert => {
            if path.starts_with("user/") {
                SandboxClass::UserWrite
            } else {
                SandboxClass::SystemWrite
            }
        }
    }
}

fn action_for_operation(_operation: &Operation) -> ActionClass {
    ActionClass::StateWrite
}

impl Authorizer for Policy {
    fn authorize(
        &self,
        principal: &str,
        mode: AccessMode,
        operations: &[Operation],
        now: u64,
    ) -> Authorization {
        let principal = match Principal::parse(principal) {
            Ok(principal) => principal,
            Err(error) => return Authorization::deny(SandboxClass::Inspect, error.to_string()),
        };
        let effective = principal.effective().clone();

        if operations.is_empty() {
            return Authorization::allow(SandboxClass::Inspect);
        }

        let mut sandbox = SandboxClass::Inspect;
        let mut requires_approval = false;

        for operation in operations {
            let path = operation.path();
            if is_recovery_path(path) {
                return Authorization::deny(
                    SandboxClass::Privileged,
                    format!("'{path}' is part of the recovery core and is immutable at runtime"),
                );
            }

            let action = action_for_operation(operation);
            sandbox = sandbox.widen(required_sandbox(action, path));

            match self.find_grant(&effective, action, path, now) {
                Some(grant) => {
                    if grant.requires_approval && mode == AccessMode::Guarded {
                        requires_approval = true;
                    }
                }
                None => {
                    return Authorization::deny(
                        sandbox,
                        format!(
                            "{} has no capability for {} on {}",
                            principal,
                            action.as_str(),
                            path
                        ),
                    );
                }
            }
        }

        if requires_approval {
            Authorization::approval(
                sandbox,
                format!(
                    "{} requires approval in guarded mode for this change",
                    principal
                ),
            )
        } else {
            Authorization::allow(sandbox)
        }
    }

    fn authorize_revert(
        &self,
        principal: &str,
        mode: AccessMode,
        target_principal: &str,
        now: u64,
    ) -> Authorization {
        let principal = match Principal::parse(principal) {
            Ok(principal) => principal,
            Err(error) => return Authorization::deny(SandboxClass::Inspect, error.to_string()),
        };
        let effective = principal.effective().clone();
        let target = match Principal::parse(target_principal) {
            Ok(target) => target,
            Err(error) => return Authorization::deny(SandboxClass::Inspect, error.to_string()),
        };

        let sandbox = match target.effective() {
            Principal::User(_) => SandboxClass::UserWrite,
            _ => SandboxClass::SystemWrite,
        };

        if effective == *target.effective() || effective.is_system() {
            return Authorization::allow(sandbox);
        }

        let resource = match target.effective() {
            Principal::User(id) => format!("user/{id}/x"),
            Principal::Service(id) => format!("system/services/{id}/x"),
            _ => "system/x".to_string(),
        };

        match self.find_grant(&effective, ActionClass::StateRevert, &resource, now) {
            Some(grant) => {
                if grant.requires_approval && mode == AccessMode::Guarded {
                    Authorization::approval(
                        sandbox,
                        format!("reverting {target} requires approval in guarded mode"),
                    )
                } else {
                    Authorization::allow(sandbox)
                }
            }
            None => Authorization::deny(
                sandbox,
                format!("{principal} cannot revert a transaction owned by {target}"),
            ),
        }
    }

    fn consume(
        &self,
        principal: &str,
        _mode: AccessMode,
        operations: &[Operation],
        now: u64,
    ) -> Result<(), String> {
        let principal = Principal::parse(principal).map_err(|error| error.to_string())?;
        let effective = principal.effective().clone();

        for operation in operations {
            let action = action_for_operation(operation);
            let path = operation.path();
            if let Some(grant) = self.find_grant(&effective, action, path, now) {
                if !self.consume_grant(&effective, grant, now) {
                    return Err(format!(
                        "rate limit exceeded for {} on {}",
                        action.as_str(),
                        path
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(path: &str) -> Operation {
        Operation::Set {
            path: path.to_string(),
            value: "x".to_string(),
        }
    }

    #[test]
    fn parses_principals() {
        assert_eq!(Principal::parse("system").unwrap(), Principal::System);
        assert_eq!(
            Principal::parse("user:ada").unwrap(),
            Principal::User("ada".into())
        );
        assert_eq!(
            Principal::parse("service:network").unwrap(),
            Principal::Service("network".into())
        );
        let agent = Principal::parse("agent:opencode:ses_1@user:ada").unwrap();
        assert_eq!(agent.effective(), &Principal::User("ada".into()));
        assert!(Principal::parse("root").is_err());
    }

    #[test]
    fn pattern_matching() {
        assert!(Pattern::new("user/ada/**").matches("user/ada/theme/mode"));
        assert!(Pattern::new("user/*/theme/mode").matches("user/ada/theme/mode"));
        assert!(!Pattern::new("user/*/theme/mode").matches("user/ada/desktop/x"));
        assert!(Pattern::new("**").matches("anything/at/all"));
        assert!(Pattern::new("system/services/*").matches("system/services/network"));
        assert!(!Pattern::new("system/services/*").matches("system/services/network/x"));
    }

    #[test]
    fn user_cannot_write_another_users_scope() {
        let policy = Policy::with_defaults();
        let decision = policy.authorize(
            "user:ada",
            AccessMode::Guarded,
            &[set("user/bob/theme/mode")],
            1_700_000_000,
        );
        assert!(!decision.allowed);
        assert!(decision.reason.contains("no capability"));
    }

    #[test]
    fn agent_borrows_the_users_authority() {
        let policy = Policy::with_defaults();
        let own = policy.authorize(
            "agent:opencode:ses_1@user:ada",
            AccessMode::Guarded,
            &[set("user/ada/theme/mode")],
            1_700_000_000,
        );
        assert!(own.allowed);

        let other = policy.authorize(
            "agent:opencode:ses_1@user:ada",
            AccessMode::Guarded,
            &[set("user/bob/theme/mode")],
            1_700_000_000,
        );
        assert!(!other.allowed);
    }

    #[test]
    fn guarded_mode_requires_approval_for_cross_boundary_grants() {
        let mut policy = Policy::with_defaults();
        policy.add(
            Grant::new(
                Principal::User("ada".into()),
                ActionClass::StateWrite,
                "system/services/network/**",
                "user:ada",
            )
            .requiring_approval(),
        );

        let guarded = policy.authorize(
            "user:ada",
            AccessMode::Guarded,
            &[set("system/services/network/state")],
            1_700_000_000,
        );
        assert!(guarded.allowed);
        assert!(guarded.requires_approval);
        assert_eq!(guarded.sandbox, SandboxClass::SystemWrite);

        let full = policy.authorize(
            "user:ada",
            AccessMode::Full,
            &[set("system/services/network/state")],
            1_700_000_000,
        );
        assert!(full.allowed);
        assert!(!full.requires_approval);
    }

    #[test]
    fn full_mode_is_still_scoped() {
        let policy = Policy::with_defaults();
        let decision = policy.authorize(
            "user:ada",
            AccessMode::Full,
            &[set("system/services/network/state")],
            1_700_000_000,
        );
        assert!(!decision.allowed);
    }

    #[test]
    fn recovery_core_is_never_writable() {
        let policy = Policy::with_defaults();
        let decision = policy.authorize(
            "system",
            AccessMode::Full,
            &[set("core/boot/kernel")],
            1_700_000_000,
        );
        assert!(!decision.allowed);
        assert!(decision.reason.contains("recovery core"));
    }

    #[test]
    fn rate_limits_are_enforced() {
        let mut policy = Policy::new();
        policy.add(
            Grant::new(
                Principal::User("ada".into()),
                ActionClass::StateWrite,
                "user/ada/**",
                "system",
            )
            .limited_to(1),
        );

        let first = policy.authorize(
            "user:ada",
            AccessMode::Guarded,
            &[set("user/ada/theme/mode")],
            1_700_000_000,
        );
        assert!(first.allowed);
        policy
            .consume(
                "user:ada",
                AccessMode::Guarded,
                &[set("user/ada/theme/mode")],
                1_700_000_000,
            )
            .expect("first use");

        // Preflight is pure: it still says yes, but committing is refused.
        let second = policy.authorize(
            "user:ada",
            AccessMode::Guarded,
            &[set("user/ada/theme/mode")],
            1_700_000_010,
        );
        assert!(second.allowed);
        let refused = policy.consume(
            "user:ada",
            AccessMode::Guarded,
            &[set("user/ada/theme/mode")],
            1_700_000_010,
        );
        assert!(refused.is_err());
        assert!(refused.unwrap_err().contains("rate limit"));

        let later = policy.consume(
            "user:ada",
            AccessMode::Guarded,
            &[set("user/ada/theme/mode")],
            1_700_000_070,
        );
        assert!(later.is_ok());
    }

    #[test]
    fn inference_is_scoped() {
        let policy = Policy::with_defaults();
        let now = 1_700_000_000;

        let generate = policy.authorize_action(
            "user:ada",
            AccessMode::Guarded,
            ActionClass::InferenceGenerate,
            "model/sim/tiny",
            now,
        );
        assert!(generate.allowed);

        let manage = policy.authorize_action(
            "user:ada",
            AccessMode::Guarded,
            ActionClass::InferenceManage,
            "system/inference/models/sim/tiny",
            now,
        );
        assert!(!manage.allowed);

        let admin = policy.authorize_action(
            "system",
            AccessMode::Guarded,
            ActionClass::InferenceManage,
            "system/inference/models/sim/tiny",
            now,
        );
        assert!(admin.allowed);

        let service = policy.authorize_action(
            "service:inference",
            AccessMode::Guarded,
            ActionClass::InferenceManage,
            "system/inference/models/sim/tiny",
            now,
        );
        assert!(service.allowed);

        // Managing models means writing the registry path, not just holding
        // the action class.
        let write = policy.authorize(
            "service:inference",
            AccessMode::Guarded,
            &[set("system/inference/models/sim/tiny")],
            now,
        );
        assert!(write.allowed);
    }

    #[test]
    fn computer_use_is_scoped_to_apps() {
        let policy = Policy::with_defaults();
        let now = 1_700_000_000;

        assert!(
            policy
                .authorize_action(
                    "user:ada",
                    AccessMode::Guarded,
                    ActionClass::ComputerUse,
                    "app/checkout",
                    now,
                )
                .allowed
        );
        assert!(
            policy
                .authorize_action(
                    "system",
                    AccessMode::Guarded,
                    ActionClass::ComputerUse,
                    "app/checkout",
                    now,
                )
                .allowed
        );
        assert!(
            !policy
                .authorize_action(
                    "service:network",
                    AccessMode::Guarded,
                    ActionClass::ComputerUse,
                    "app/checkout",
                    now,
                )
                .allowed
        );
    }

    #[test]
    fn only_system_and_the_update_service_manage_generations() {
        let policy = Policy::with_defaults();
        let now = 1_700_000_000;

        assert!(
            policy
                .authorize_action(
                    "system",
                    AccessMode::Guarded,
                    ActionClass::GenerationManage,
                    "system/generations/gen_1",
                    now,
                )
                .allowed
        );
        assert!(
            policy
                .authorize_action(
                    "service:update",
                    AccessMode::Guarded,
                    ActionClass::GenerationManage,
                    "system/generations/gen_1",
                    now,
                )
                .allowed
        );
        assert!(
            !policy
                .authorize_action(
                    "user:ada",
                    AccessMode::Guarded,
                    ActionClass::GenerationManage,
                    "system/generations/gen_1",
                    now,
                )
                .allowed
        );
    }

    #[test]
    fn cross_principal_revert_requires_a_grant() {
        let policy = Policy::with_defaults();
        let denied =
            policy.authorize_revert("user:ada", AccessMode::Guarded, "user:bob", 1_700_000_000);
        assert!(!denied.allowed);

        let own =
            policy.authorize_revert("user:ada", AccessMode::Guarded, "user:ada", 1_700_000_000);
        assert!(own.allowed);

        let admin =
            policy.authorize_revert("system", AccessMode::Guarded, "user:bob", 1_700_000_000);
        assert!(admin.allowed);
    }

    #[test]
    fn grants_round_trip_through_a_file() {
        let dir = std::env::temp_dir().join(format!("tiaki-grants-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("grants");

        let mut policy = Policy::with_defaults();
        policy.add(
            Grant::new(
                Principal::User("ada".into()),
                ActionClass::StateWrite,
                "system/services/network/**",
                "user:ada",
            )
            .requiring_approval()
            .limited_to(4),
        );
        policy.path = Some(path.clone());
        policy.save().expect("save");

        let loaded = Policy::load(&path).expect("load");
        assert!(loaded.grants().iter().any(|grant| {
            grant.principal == Principal::User("ada".into())
                && grant.action == ActionClass::StateWrite
                && grant.resource.raw == "system/services/network/**"
                && grant.requires_approval
                && grant.max_per_minute == Some(4)
        }));

        let _ = fs::remove_dir_all(&dir);
    }
}
