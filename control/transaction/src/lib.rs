//! Transaction engine: check → snapshot → apply → verify → commit or revert.
//!
//! `check` is the OS equivalent of `cargo check` for state mutations: it runs
//! static validators and a dry run on a scratch copy, and mutates nothing.
//! `apply` refuses to touch state when `check` fails.
//!
//! See TIAKI.md §8 and §8.8.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};

use agentic_core_log::{ActionLog, EntryInput, Impact, LogEntry, LogError, Status, Summary};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Operation {
    Set { path: String, value: String },
    Remove { path: String },
}

impl Operation {
    pub fn path(&self) -> &str {
        match self {
            Operation::Set { path, .. } | Operation::Remove { path } => path,
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Operation::Set { path, value } => format!("set {path}={value}"),
            Operation::Remove { path } => format!("remove {path}"),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub values: BTreeMap<String, String>,
}

pub trait StateStore {
    fn get(&self, path: &str) -> Option<String>;
    fn set(&mut self, path: &str, value: String) -> io::Result<()>;
    fn remove(&mut self, path: &str) -> io::Result<()>;
    fn snapshot(&self) -> Snapshot;
    fn restore(&mut self, snapshot: &Snapshot) -> io::Result<()>;
    fn values(&self) -> &BTreeMap<String, String>;
}

/// A store that can re-read its backing file. Long-running front ends use this
/// to see edits made by another process.
pub trait Reloadable {
    fn reload(&mut self) -> io::Result<()>;
}

#[derive(Default)]
pub struct MemoryStore {
    values: BTreeMap<String, String>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl StateStore for MemoryStore {
    fn get(&self, path: &str) -> Option<String> {
        self.values.get(path).cloned()
    }

    fn set(&mut self, path: &str, value: String) -> io::Result<()> {
        self.values.insert(path.to_string(), value);
        Ok(())
    }

    fn remove(&mut self, path: &str) -> io::Result<()> {
        self.values.remove(path);
        Ok(())
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            values: self.values.clone(),
        }
    }

    fn restore(&mut self, snapshot: &Snapshot) -> io::Result<()> {
        self.values = snapshot.values.clone();
        Ok(())
    }

    fn values(&self) -> &BTreeMap<String, String> {
        &self.values
    }
}

pub struct FileStore {
    path: PathBuf,
    values: BTreeMap<String, String>,
}

impl FileStore {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let values = read_values(&path)?;
        Ok(Self { path, values })
    }

    /// Re-read the file from disk. The desktop server calls this before each
    /// request so edits made by another `agentd` process are visible.
    pub fn reload(&mut self) -> io::Result<()> {
        self.values = read_values(&self.path)?;
        Ok(())
    }

    fn save(&self) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = String::new();
        for (key, value) in &self.values {
            out.push_str(&escape(key));
            out.push('\t');
            out.push_str(&escape(value));
            out.push('\n');
        }
        fs::write(&self.path, out)
    }
}

fn read_values(path: &Path) -> io::Result<BTreeMap<String, String>> {
    let mut values = BTreeMap::new();
    if !path.exists() {
        return Ok(values);
    }
    let file = fs::File::open(path)?;
    for line in BufReader::new(file).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let mut parts = line.splitn(2, '\t');
        let key = parts.next().unwrap_or_default();
        let value = parts.next().unwrap_or_default();
        values.insert(unescape(key), unescape(value));
    }
    Ok(values)
}

impl StateStore for FileStore {
    fn get(&self, path: &str) -> Option<String> {
        self.values.get(path).cloned()
    }

    fn set(&mut self, path: &str, value: String) -> io::Result<()> {
        self.values.insert(path.to_string(), value);
        self.save()
    }

    fn remove(&mut self, path: &str) -> io::Result<()> {
        self.values.remove(path);
        self.save()
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            values: self.values.clone(),
        }
    }

    fn restore(&mut self, snapshot: &Snapshot) -> io::Result<()> {
        self.values = snapshot.values.clone();
        self.save()
    }

    fn values(&self) -> &BTreeMap<String, String> {
        &self.values
    }
}

impl Reloadable for FileStore {
    fn reload(&mut self) -> io::Result<()> {
        self.values = read_values(&self.path)?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Health {
    pub ok: bool,
    pub reason: String,
}

impl Health {
    pub fn ok() -> Self {
        Self {
            ok: true,
            reason: String::new(),
        }
    }

    pub fn fail(reason: impl Into<String>) -> Self {
        Self {
            ok: false,
            reason: reason.into(),
        }
    }
}

pub trait HealthCheck {
    fn check(&self, store: &dyn StateStore) -> Health;
}

/// Minimal schema check used by the demo. Understands both global paths and
/// user-scoped paths such as `user/ada/theme/mode`.
pub struct SchemaHealth;

impl HealthCheck for SchemaHealth {
    fn check(&self, store: &dyn StateStore) -> Health {
        for (path, value) in store.values() {
            match schema_key(path) {
                "desktop/clock/position" => {
                    if !matches!(
                        value.as_str(),
                        "left" | "center" | "right" | "top-left" | "top-right"
                    ) {
                        return Health::fail(format!("{path} has unsupported value '{value}'"));
                    }
                }
                "theme/mode" => {
                    if !matches!(value.as_str(), "light" | "dark") {
                        return Health::fail(format!("{path} has unsupported value '{value}'"));
                    }
                }
                _ => {}
            }
        }
        Health::ok()
    }
}

/// Strip a `user/<id>/` scope so one schema serves every user.
pub fn schema_key(path: &str) -> &str {
    if let Some(rest) = path.strip_prefix("user/") {
        if let Some((_user, suffix)) = rest.split_once('/') {
            return suffix;
        }
    }
    path
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessMode {
    Guarded,
    Full,
}

impl AccessMode {
    pub fn as_str(self) -> &'static str {
        match self {
            AccessMode::Guarded => "guarded",
            AccessMode::Full => "full",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "guarded" => Some(AccessMode::Guarded),
            "full" => Some(AccessMode::Full),
            _ => None,
        }
    }
}

/// The narrowest sandbox a change can run in. There is deliberately no `None`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SandboxClass {
    Inspect,
    UserWrite,
    SystemWrite,
    Privileged,
}

impl SandboxClass {
    pub fn as_str(self) -> &'static str {
        match self {
            SandboxClass::Inspect => "inspect",
            SandboxClass::UserWrite => "user-write",
            SandboxClass::SystemWrite => "system-write",
            SandboxClass::Privileged => "privileged",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "inspect" => Some(SandboxClass::Inspect),
            "user-write" => Some(SandboxClass::UserWrite),
            "system-write" => Some(SandboxClass::SystemWrite),
            "privileged" => Some(SandboxClass::Privileged),
            _ => None,
        }
    }

    /// Combine two requirements by taking the wider profile.
    pub fn widen(self, other: SandboxClass) -> SandboxClass {
        self.max(other)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Authorization {
    pub allowed: bool,
    pub requires_approval: bool,
    pub reason: String,
    pub sandbox: SandboxClass,
}

impl Authorization {
    pub fn allow(sandbox: SandboxClass) -> Self {
        Self {
            allowed: true,
            requires_approval: false,
            reason: String::new(),
            sandbox,
        }
    }

    pub fn deny(sandbox: SandboxClass, reason: impl Into<String>) -> Self {
        Self {
            allowed: false,
            requires_approval: false,
            reason: reason.into(),
            sandbox,
        }
    }

    pub fn approval(sandbox: SandboxClass, reason: impl Into<String>) -> Self {
        Self {
            allowed: true,
            requires_approval: true,
            reason: reason.into(),
            sandbox,
        }
    }

    pub fn is_clear(&self) -> bool {
        self.allowed && !self.requires_approval
    }
}

/// Authority is a separate layer: the engine asks, the capability broker answers.
pub trait Authorizer {
    fn authorize(
        &self,
        principal: &str,
        mode: AccessMode,
        operations: &[Operation],
        now: u64,
    ) -> Authorization;

    fn authorize_revert(
        &self,
        principal: &str,
        mode: AccessMode,
        target_principal: &str,
        now: u64,
    ) -> Authorization;

    /// Called only after a preflight passes, immediately before mutation.
    /// Preflight must not consume quota.
    fn consume(
        &self,
        _principal: &str,
        _mode: AccessMode,
        _operations: &[Operation],
        _now: u64,
    ) -> Result<(), String> {
        Ok(())
    }
}

/// Test-only escape hatch. The daemon always installs a real policy.
pub struct AllowAll;

impl Authorizer for AllowAll {
    fn authorize(
        &self,
        _principal: &str,
        _mode: AccessMode,
        _operations: &[Operation],
        _now: u64,
    ) -> Authorization {
        Authorization::allow(SandboxClass::SystemWrite)
    }

    fn authorize_revert(
        &self,
        _principal: &str,
        _mode: AccessMode,
        _target_principal: &str,
        _now: u64,
    ) -> Authorization {
        Authorization::allow(SandboxClass::SystemWrite)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issue {
    pub severity: Severity,
    pub code: String,
    pub path: Option<String>,
    pub message: String,
}

impl Issue {
    pub fn error(
        code: impl Into<String>,
        path: Option<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: Severity::Error,
            code: code.into(),
            path,
            message: message.into(),
        }
    }

    pub fn warning(
        code: impl Into<String>,
        path: Option<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: Severity::Warning,
            code: code.into(),
            path,
            message: message.into(),
        }
    }
}

/// Result of `Engine::check`. A report with any `Error` blocks `apply`.
#[derive(Clone, Debug)]
pub struct CheckReport {
    pub ok: bool,
    pub issues: Vec<Issue>,
    pub summary: Summary,
    pub impact: Impact,
    pub dry_run_health: Option<Health>,
    pub authorization: Authorization,
}

impl CheckReport {
    pub fn errors(&self) -> impl Iterator<Item = &Issue> {
        self.issues
            .iter()
            .filter(|issue| issue.severity == Severity::Error)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &Issue> {
        self.issues
            .iter()
            .filter(|issue| issue.severity == Severity::Warning)
    }
}

/// Static validation. Implementations must not mutate the store.
pub trait Validator {
    fn validate(&self, store: &dyn StateStore, operations: &[Operation]) -> Vec<Issue>;
}

/// Runs several validators in order. Used by the daemon to combine the core
/// schema checks with UI-tree and keymap validators.
pub struct CompositeValidator {
    validators: Vec<Box<dyn Validator>>,
}

impl CompositeValidator {
    pub fn new(validators: Vec<Box<dyn Validator>>) -> Self {
        Self { validators }
    }
}

impl Default for CompositeValidator {
    fn default() -> Self {
        Self {
            validators: Vec::new(),
        }
    }
}

impl Validator for CompositeValidator {
    fn validate(&self, store: &dyn StateStore, operations: &[Operation]) -> Vec<Issue> {
        self.validators
            .iter()
            .flat_map(|validator| validator.validate(store, operations))
            .collect()
    }
}

/// Milestone 0 validator: path syntax, duplicates, and the two known schemas.
/// Milestone 2 replaces this with a schema registry populated by plugins.
pub struct SchemaValidator;

impl Validator for SchemaValidator {
    fn validate(&self, store: &dyn StateStore, operations: &[Operation]) -> Vec<Issue> {
        let mut issues = Vec::new();
        let mut seen = BTreeSet::new();

        for operation in operations {
            let path = operation.path();

            if !valid_path(path) {
                issues.push(Issue::error(
                    "path.invalid",
                    Some(path.to_string()),
                    format!("'{path}' is not a valid state path"),
                ));
                continue;
            }

            if !seen.insert(path.to_string()) {
                issues.push(Issue::error(
                    "path.duplicate",
                    Some(path.to_string()),
                    format!("'{path}' is written more than once in one transaction"),
                ));
            }

            match operation {
                Operation::Set { path, value } => {
                    let limit = value_limit(path);
                    if value.len() > limit {
                        issues.push(Issue::error(
                            "value.too_large",
                            Some(path.clone()),
                            format!("value exceeds {limit} bytes"),
                        ));
                    }
                    match schema_key(path) {
                        "desktop/clock/position" => {
                            if !matches!(
                                value.as_str(),
                                "left" | "center" | "right" | "top-left" | "top-right"
                            ) {
                                issues.push(Issue::error(
                                    "schema.invalid_value",
                                    Some(path.clone()),
                                    format!("'{value}' is not a valid clock position"),
                                ));
                            }
                        }
                        "theme/mode" => {
                            if !matches!(value.as_str(), "light" | "dark") {
                                issues.push(Issue::error(
                                    "schema.invalid_value",
                                    Some(path.clone()),
                                    format!("'{value}' is not a valid theme mode"),
                                ));
                            }
                        }
                        _ => {
                            issues.push(Issue::warning(
                                "schema.unregistered",
                                Some(path.clone()),
                                format!("no schema registered for '{path}'; value was not checked"),
                            ));
                        }
                    }
                }
                Operation::Remove { path } => {
                    if store.get(path).is_none() {
                        issues.push(Issue::warning(
                            "path.missing",
                            Some(path.clone()),
                            format!("'{path}' does not exist"),
                        ));
                    }
                }
            }
        }

        issues
    }
}

/// JSON-shaped resources such as UI trees and keymaps need more room than a
/// single setting. Everything else keeps the small bound.
fn value_limit(path: &str) -> usize {
    let key = schema_key(path);
    if key == "ui/tree" || key == "input/keymap" {
        64 * 1024
    } else {
        4096
    }
}

fn valid_path(path: &str) -> bool {
    if path.is_empty() || path.len() > 256 {
        return false;
    }
    if path.starts_with('/') || path.ends_with('/') {
        return false;
    }
    path.split('/')
        .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

pub trait Summarizer {
    fn summarize(&self, intent: &str, operations: &[Operation]) -> Summary;
    fn impact(&self, operations: &[Operation]) -> Impact;
    fn verify(&self, summary: &Summary, operations: &[Operation]) -> bool;
}

#[derive(Clone, Debug)]
pub struct Transaction {
    pub id: String,
    pub principal: String,
    pub intent: String,
    pub operations: Vec<Operation>,
    pub summary: Summary,
    pub impact: Impact,
    pub status: Status,
    pub snapshot_id: String,
}

#[derive(Debug)]
pub struct ApplyError {
    pub transaction: Transaction,
    pub reason: String,
}

impl std::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.transaction.id, self.reason)
    }
}

impl std::error::Error for ApplyError {}

pub struct Engine<S: StateStore, H: HealthCheck, M: Summarizer, V: Validator, A = AllowAll> {
    store: S,
    health: H,
    summarizer: M,
    validator: V,
    authority: A,
    log: ActionLog,
    snapshot_dir: PathBuf,
}

impl<S: StateStore, H: HealthCheck, M: Summarizer, V: Validator> Engine<S, H, M, V, AllowAll> {
    /// Construct an engine with authorization disabled. Tests only; the daemon
    /// must use `with_authority`.
    pub fn new(
        store: S,
        health: H,
        summarizer: M,
        validator: V,
        log: ActionLog,
        snapshot_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            store,
            health,
            summarizer,
            validator,
            authority: AllowAll,
            log,
            snapshot_dir: snapshot_dir.into(),
        }
    }
}

impl<S: StateStore, H: HealthCheck, M: Summarizer, V: Validator, A: Authorizer>
    Engine<S, H, M, V, A>
{
    pub fn with_authority(
        store: S,
        health: H,
        summarizer: M,
        validator: V,
        authority: A,
        log: ActionLog,
        snapshot_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            store,
            health,
            summarizer,
            validator,
            authority,
            log,
            snapshot_dir: snapshot_dir.into(),
        }
    }

    pub fn store(&self) -> &S {
        &self.store
    }

    pub fn store_mut(&mut self) -> &mut S {
        &mut self.store
    }

    /// Re-read state and log from disk. Used by the desktop server so edits
    /// made by another `agentd` process are visible and the hash chain stays
    /// consistent.
    pub fn reload(&mut self) -> Result<(), String>
    where
        S: Reloadable,
    {
        self.store.reload().map_err(|error| error.to_string())?;
        self.log.reload().map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn log(&self) -> &ActionLog {
        &self.log
    }

    pub fn entries(&self) -> &[LogEntry] {
        self.log.entries()
    }

    pub fn authority(&self) -> &A {
        &self.authority
    }

    pub fn authority_mut(&mut self) -> &mut A {
        &mut self.authority
    }

    /// Record an attributable event that is not a state mutation, such as a
    /// plugin lifecycle action. It still goes through the same hash chain.
    pub fn record(
        &mut self,
        principal: &str,
        mode: AccessMode,
        sandbox: SandboxClass,
        transaction_id: &str,
        intent: &str,
        summary: Summary,
        impact: Impact,
        status: Status,
        operations: Vec<String>,
    ) -> Result<LogEntry, LogError> {
        // Exclusive: the agent runner and its MCP server are different
        // processes sharing one log, and plain append corrupts the chain.
        self.log.append_exclusive(EntryInput {
            timestamp: now_secs(),
            principal: principal.to_string(),
            mode: mode.as_str().to_string(),
            sandbox: sandbox.as_str().to_string(),
            transaction_id: transaction_id.to_string(),
            intent: intent.to_string(),
            summary,
            impact,
            status,
            operations,
        })
    }

    /// Preflight: authority, static validation, and a dry run on a scratch copy.
    /// Never mutates state and never writes to the log.
    pub fn check(
        &self,
        principal: &str,
        mode: AccessMode,
        intent: &str,
        operations: &[Operation],
    ) -> CheckReport {
        let mut summary = self.summarizer.summarize(intent, operations);
        summary.verified = self.summarizer.verify(&summary, operations);
        let impact = self.summarizer.impact(operations);

        let authorization = self
            .authority
            .authorize(principal, mode, operations, now_secs());
        let mut issues = Vec::new();

        if !authorization.allowed {
            issues.push(Issue::error(
                "authorization.denied",
                None,
                authorization.reason.clone(),
            ));
        } else if authorization.requires_approval {
            issues.push(Issue::error(
                "authorization.approval_required",
                None,
                authorization.reason.clone(),
            ));
        }

        issues.extend(self.validator.validate(&self.store, operations));

        if !summary.verified {
            issues.push(Issue::error(
                "summary.mismatch",
                None,
                "the summary does not match the operations",
            ));
        }

        let dry_run_health = self.dry_run(operations);
        if let Some(health) = &dry_run_health {
            if !health.ok {
                issues.push(Issue::error(
                    "health.dry_run_failed",
                    None,
                    health.reason.clone(),
                ));
            }
        }

        let ok = !issues.iter().any(|issue| issue.severity == Severity::Error);

        CheckReport {
            ok,
            issues,
            summary,
            impact,
            dry_run_health,
            authorization,
        }
    }

    pub fn apply(
        &mut self,
        id: &str,
        principal: &str,
        mode: AccessMode,
        intent: &str,
        operations: Vec<Operation>,
    ) -> Result<Transaction, ApplyError> {
        let report = self.check(principal, mode, intent, &operations);

        if !report.ok {
            let reason = report
                .errors()
                .map(|issue| issue.message.clone())
                .collect::<Vec<_>>()
                .join("; ");
            let status = if report.authorization.allowed && report.authorization.requires_approval {
                Status::PendingApproval
            } else {
                Status::Rejected
            };
            let transaction = pending(
                id,
                principal,
                intent,
                &operations,
                &report.summary,
                &report.impact,
                &format!("snap_{id}"),
                status,
            );
            let _ = self.log.append(EntryInput {
                timestamp: now_secs(),
                principal: principal.to_string(),
                mode: mode.as_str().to_string(),
                sandbox: report.authorization.sandbox.as_str().to_string(),
                transaction_id: id.to_string(),
                intent: intent.to_string(),
                summary: report.summary.clone(),
                impact: report.impact.clone(),
                status,
                operations: operations.iter().map(Operation::describe).collect(),
            });
            return Err(ApplyError {
                transaction,
                reason,
            });
        }

        if let Err(reason) = self
            .authority
            .consume(principal, mode, &operations, now_secs())
        {
            let transaction = pending(
                id,
                principal,
                intent,
                &operations,
                &report.summary,
                &report.impact,
                &format!("snap_{id}"),
                Status::Rejected,
            );
            let _ = self.log.append(EntryInput {
                timestamp: now_secs(),
                principal: principal.to_string(),
                mode: mode.as_str().to_string(),
                sandbox: report.authorization.sandbox.as_str().to_string(),
                transaction_id: id.to_string(),
                intent: intent.to_string(),
                summary: report.summary.clone(),
                impact: report.impact.clone(),
                status: Status::Rejected,
                operations: operations.iter().map(Operation::describe).collect(),
            });
            return Err(ApplyError {
                transaction,
                reason,
            });
        }

        let summary = report.summary;
        let impact = report.impact;
        let sandbox = report.authorization.sandbox.as_str().to_string();
        let snapshot_id = format!("snap_{id}");
        let snapshot = self.store.snapshot();

        if let Err(error) = self.save_snapshot(&snapshot_id, &snapshot) {
            return Err(ApplyError {
                transaction: pending(
                    id,
                    principal,
                    intent,
                    &operations,
                    &summary,
                    &impact,
                    &snapshot_id,
                    Status::Failed,
                ),
                reason: format!("could not write snapshot: {error}"),
            });
        }

        for operation in &operations {
            let result = match operation {
                Operation::Set { path, value } => self.store.set(path, value.clone()),
                Operation::Remove { path } => self.store.remove(path),
            };
            if let Err(error) = result {
                let _ = self.store.restore(&snapshot);
                return Err(ApplyError {
                    transaction: pending(
                        id,
                        principal,
                        intent,
                        &operations,
                        &summary,
                        &impact,
                        &snapshot_id,
                        Status::Failed,
                    ),
                    reason: format!("operation failed: {error}"),
                });
            }
        }

        let health = self.health.check(&self.store);
        let status = if health.ok {
            Status::Committed
        } else {
            Status::Reverted
        };
        if !health.ok {
            let _ = self.store.restore(&snapshot);
        }

        let operation_descriptions: Vec<String> =
            operations.iter().map(Operation::describe).collect();
        let transaction = pending(
            id,
            principal,
            intent,
            &operations,
            &summary,
            &impact,
            &snapshot_id,
            status,
        );

        if let Err(error) = self.log.append(EntryInput {
            timestamp: now_secs(),
            principal: principal.to_string(),
            mode: mode.as_str().to_string(),
            sandbox: sandbox.clone(),
            transaction_id: id.to_string(),
            intent: intent.to_string(),
            summary: summary.clone(),
            impact: impact.clone(),
            status,
            operations: operation_descriptions,
        }) {
            if health.ok {
                let _ = self.store.restore(&snapshot);
            }
            return Err(ApplyError {
                transaction,
                reason: format!("log append failed: {error}"),
            });
        }

        if health.ok {
            Ok(transaction)
        } else {
            Err(ApplyError {
                transaction,
                reason: health.reason,
            })
        }
    }

    pub fn revert(
        &mut self,
        txn_id: &str,
        principal: &str,
        mode: AccessMode,
    ) -> Result<Transaction, ApplyError> {
        let target_snapshot_id = format!("snap_{txn_id}");
        let revert_id = format!("txn_revert_{txn_id}");
        let intent = format!("Undo {txn_id}");
        let summary = Summary::new(
            format!("Undid {txn_id}."),
            format!("Restored the state from before {txn_id}."),
            "rule-based",
            true,
        );
        let impact = Impact::none();
        let current_snapshot_id = format!("snap_{revert_id}");

        let target_entry = self
            .log
            .entries()
            .iter()
            .rev()
            .find(|entry| entry.transaction_id == txn_id)
            .cloned();

        let Some(target_entry) = target_entry else {
            return Err(ApplyError {
                transaction: pending(
                    &revert_id,
                    principal,
                    &intent,
                    &[],
                    &summary,
                    &impact,
                    &current_snapshot_id,
                    Status::Failed,
                ),
                reason: format!("unknown transaction {txn_id}"),
            });
        };

        if target_entry.status != Status::Committed {
            return Err(ApplyError {
                transaction: pending(
                    &revert_id,
                    principal,
                    &intent,
                    &[],
                    &summary,
                    &impact,
                    &current_snapshot_id,
                    Status::Failed,
                ),
                reason: format!(
                    "only committed transactions can be reverted; {txn_id} is {}",
                    target_entry.status.as_str()
                ),
            });
        }

        let authorization =
            self.authority
                .authorize_revert(principal, mode, &target_entry.principal, now_secs());

        if !authorization.is_clear() {
            let status = if authorization.allowed {
                Status::PendingApproval
            } else {
                Status::Rejected
            };
            let transaction = pending(
                &revert_id,
                principal,
                &intent,
                &[],
                &summary,
                &impact,
                &current_snapshot_id,
                status,
            );
            let _ = self.log.append(EntryInput {
                timestamp: now_secs(),
                principal: principal.to_string(),
                mode: mode.as_str().to_string(),
                sandbox: authorization.sandbox.as_str().to_string(),
                transaction_id: revert_id,
                intent,
                summary,
                impact,
                status,
                operations: vec![format!("revert {txn_id}")],
            });
            return Err(ApplyError {
                transaction,
                reason: authorization.reason,
            });
        }

        let sandbox = authorization.sandbox.as_str().to_string();

        let target = match self.load_snapshot(&target_snapshot_id) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                return Err(ApplyError {
                    transaction: pending(
                        &revert_id,
                        principal,
                        &intent,
                        &[],
                        &summary,
                        &impact,
                        &current_snapshot_id,
                        Status::Failed,
                    ),
                    reason: format!("could not load snapshot {target_snapshot_id}: {error}"),
                });
            }
        };

        let current = self.store.snapshot();
        if let Err(error) = self.save_snapshot(&current_snapshot_id, &current) {
            return Err(ApplyError {
                transaction: pending(
                    &revert_id,
                    principal,
                    &intent,
                    &[],
                    &summary,
                    &impact,
                    &current_snapshot_id,
                    Status::Failed,
                ),
                reason: format!("could not write snapshot: {error}"),
            });
        }
        if let Err(error) = self.store.restore(&target) {
            return Err(ApplyError {
                transaction: pending(
                    &revert_id,
                    principal,
                    &intent,
                    &[],
                    &summary,
                    &impact,
                    &current_snapshot_id,
                    Status::Failed,
                ),
                reason: format!("could not restore snapshot: {error}"),
            });
        }

        let transaction = pending(
            &revert_id,
            principal,
            &intent,
            &[],
            &summary,
            &impact,
            &current_snapshot_id,
            Status::Committed,
        );

        if let Err(error) = self.log.append(EntryInput {
            timestamp: now_secs(),
            principal: principal.to_string(),
            mode: mode.as_str().to_string(),
            sandbox,
            transaction_id: revert_id,
            intent,
            summary,
            impact,
            status: Status::Committed,
            operations: vec![format!("restore {target_snapshot_id}")],
        }) {
            return Err(ApplyError {
                transaction,
                reason: format!("log append failed: {error}"),
            });
        }

        Ok(transaction)
    }

    fn dry_run(&self, operations: &[Operation]) -> Option<Health> {
        let mut scratch = MemoryStore::new();
        scratch.restore(&self.store.snapshot()).ok()?;
        for operation in operations {
            let result = match operation {
                Operation::Set { path, value } => scratch.set(path, value.clone()),
                Operation::Remove { path } => scratch.remove(path),
            };
            result.ok()?;
        }
        Some(self.health.check(&scratch))
    }

    fn save_snapshot(&self, id: &str, snapshot: &Snapshot) -> io::Result<()> {
        fs::create_dir_all(&self.snapshot_dir)?;
        let mut out = String::new();
        for (key, value) in &snapshot.values {
            out.push_str(&escape(key));
            out.push('\t');
            out.push_str(&escape(value));
            out.push('\n');
        }
        fs::write(self.snapshot_dir.join(id), out)
    }

    fn load_snapshot(&self, id: &str) -> io::Result<Snapshot> {
        let file = fs::File::open(self.snapshot_dir.join(id))?;
        let mut values = BTreeMap::new();
        for line in BufReader::new(file).lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let mut parts = line.splitn(2, '\t');
            let key = parts.next().unwrap_or_default();
            let value = parts.next().unwrap_or_default();
            values.insert(unescape(key), unescape(value));
        }
        Ok(Snapshot { values })
    }
}

fn pending(
    id: &str,
    principal: &str,
    intent: &str,
    operations: &[Operation],
    summary: &Summary,
    impact: &Impact,
    snapshot_id: &str,
    status: Status,
) -> Transaction {
    Transaction {
        id: id.to_string(),
        principal: principal.to_string(),
        intent: intent.to_string(),
        operations: operations.to_vec(),
        summary: summary.clone(),
        impact: impact.clone(),
        status,
        snapshot_id: snapshot_id.to_string(),
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            other => out.push(other),
        }
    }
    out
}

fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TestSummarizer;

    impl Summarizer for TestSummarizer {
        fn summarize(&self, intent: &str, operations: &[Operation]) -> Summary {
            Summary::new(
                format!("{intent} ({} ops)", operations.len()),
                String::new(),
                "test",
                true,
            )
        }

        fn impact(&self, _operations: &[Operation]) -> Impact {
            Impact::none()
        }

        fn verify(&self, _summary: &Summary, _operations: &[Operation]) -> bool {
            true
        }
    }

    struct FlakyHealth {
        calls: AtomicUsize,
    }

    impl FlakyHealth {
        fn new() -> Self {
            Self {
                calls: AtomicUsize::new(0),
            }
        }
    }

    impl HealthCheck for FlakyHealth {
        fn check(&self, _store: &dyn StateStore) -> Health {
            if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                Health::ok()
            } else {
                Health::fail("service did not come up")
            }
        }
    }

    fn temp_dir() -> PathBuf {
        let index = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("tiaki-txn-{}-{}", std::process::id(), index));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn test_engine() -> Engine<MemoryStore, SchemaHealth, TestSummarizer, SchemaValidator> {
        let dir = temp_dir();
        let log = ActionLog::open(dir.join("log")).expect("open log");
        Engine::new(
            MemoryStore::new(),
            SchemaHealth,
            TestSummarizer,
            SchemaValidator,
            log,
            dir.join("snapshots"),
        )
    }

    #[test]
    fn applies_then_reverts() {
        let mut engine = test_engine();
        let transaction = engine
            .apply(
                "txn_1",
                "user:ada",
                AccessMode::Guarded,
                "set clock",
                vec![Operation::Set {
                    path: "desktop/clock/position".into(),
                    value: "top-right".into(),
                }],
            )
            .expect("commit");

        assert_eq!(transaction.status, Status::Committed);
        assert_eq!(
            engine.store().get("desktop/clock/position").as_deref(),
            Some("top-right")
        );
        assert_eq!(engine.entries().len(), 1);

        let revert = engine
            .revert("txn_1", "user:ada", AccessMode::Guarded)
            .expect("revert");
        assert_eq!(revert.status, Status::Committed);
        assert_eq!(engine.store().get("desktop/clock/position"), None);
        assert_eq!(engine.entries().len(), 2);
    }

    #[test]
    fn check_blocks_invalid_value_without_mutating() {
        let engine = test_engine();
        let report = engine.check(
            "user:ada",
            AccessMode::Guarded,
            "set theme",
            &[Operation::Set {
                path: "theme/mode".into(),
                value: "purple".into(),
            }],
        );

        assert!(!report.ok);
        assert!(report
            .errors()
            .any(|issue| issue.code == "schema.invalid_value"));
        assert_eq!(engine.store().get("theme/mode"), None);
        assert_eq!(engine.entries().len(), 0);
    }

    #[test]
    fn apply_rejects_before_snapshot() {
        let mut engine = test_engine();
        let error = engine
            .apply(
                "txn_bad",
                "user:ada",
                AccessMode::Guarded,
                "set theme",
                vec![Operation::Set {
                    path: "theme/mode".into(),
                    value: "purple".into(),
                }],
            )
            .expect_err("should reject");

        assert_eq!(error.transaction.status, Status::Rejected);
        assert_eq!(engine.store().get("theme/mode"), None);
        assert_eq!(engine.entries().len(), 1);
        assert_eq!(engine.entries()[0].status, Status::Rejected);
    }

    #[test]
    fn unknown_path_passes_with_warning() {
        let engine = test_engine();
        let report = engine.check(
            "user:ada",
            AccessMode::Guarded,
            "set plugin value",
            &[Operation::Set {
                path: "plugin/clock/accent".into(),
                value: "amber".into(),
            }],
        );

        assert!(report.ok);
        assert!(report
            .warnings()
            .any(|issue| issue.code == "schema.unregistered"));
    }

    #[test]
    fn duplicate_path_is_rejected() {
        let engine = test_engine();
        let report = engine.check(
            "user:ada",
            AccessMode::Guarded,
            "set twice",
            &[
                Operation::Set {
                    path: "theme/mode".into(),
                    value: "dark".into(),
                },
                Operation::Set {
                    path: "theme/mode".into(),
                    value: "light".into(),
                },
            ],
        );

        assert!(!report.ok);
        assert!(report.errors().any(|issue| issue.code == "path.duplicate"));
    }

    #[test]
    fn invalid_path_is_rejected() {
        let engine = test_engine();
        let report = engine.check(
            "user:ada",
            AccessMode::Guarded,
            "escape",
            &[Operation::Set {
                path: "../etc/shadow".into(),
                value: "x".into(),
            }],
        );

        assert!(!report.ok);
        assert!(report.errors().any(|issue| issue.code == "path.invalid"));
    }

    struct MismatchSummarizer;

    impl Summarizer for MismatchSummarizer {
        fn summarize(&self, _intent: &str, _operations: &[Operation]) -> Summary {
            Summary::new("Changed the wallpaper.", "", "test", true)
        }

        fn impact(&self, _operations: &[Operation]) -> Impact {
            Impact::none()
        }

        fn verify(&self, _summary: &Summary, _operations: &[Operation]) -> bool {
            false
        }
    }

    #[test]
    fn summary_mismatch_is_rejected() {
        let dir = temp_dir();
        let log = ActionLog::open(dir.join("log")).expect("open log");
        let engine = Engine::new(
            MemoryStore::new(),
            SchemaHealth,
            MismatchSummarizer,
            SchemaValidator,
            log,
            dir.join("snapshots"),
        );

        let report = engine.check(
            "user:ada",
            AccessMode::Guarded,
            "set clock",
            &[Operation::Set {
                path: "desktop/clock/position".into(),
                value: "top-right".into(),
            }],
        );

        assert!(!report.ok);
        assert!(report
            .errors()
            .any(|issue| issue.code == "summary.mismatch"));
    }

    #[test]
    fn runtime_failure_after_preflight_reverts() {
        let dir = temp_dir();
        let log = ActionLog::open(dir.join("log")).expect("open log");
        let mut engine = Engine::new(
            MemoryStore::new(),
            FlakyHealth::new(),
            TestSummarizer,
            SchemaValidator,
            log,
            dir.join("snapshots"),
        );

        let error = engine
            .apply(
                "txn_flaky",
                "user:ada",
                AccessMode::Guarded,
                "set unknown service state",
                vec![Operation::Set {
                    path: "service/network/state".into(),
                    value: "down".into(),
                }],
            )
            .expect_err("should revert");

        assert_eq!(error.transaction.status, Status::Reverted);
        assert_eq!(engine.store().get("service/network/state"), None);
        assert_eq!(engine.entries().last().unwrap().status, Status::Reverted);
    }
}
