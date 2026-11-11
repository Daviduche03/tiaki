//! End-to-end authority tests: the capability broker inside the real engine.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use agentic_capability::{ActionClass, Grant, Policy, Principal};
use agentic_core_log::{ActionLog, Status};
use agentic_explain::RuleBasedSummarizer;
use agentic_transaction::{
    AccessMode, Engine, MemoryStore, Operation, SchemaHealth, SchemaValidator, StateStore,
};

type Ledger = Engine<MemoryStore, SchemaHealth, RuleBasedSummarizer, SchemaValidator, Policy>;

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_dir() -> PathBuf {
    let index = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "tiaki-authority-{}-{}",
        std::process::id(),
        index
    ));
    let _ = fs::remove_dir_all(&dir);
    dir
}

fn engine_with(policy: Policy) -> (Ledger, PathBuf) {
    let dir = temp_dir();
    let log = ActionLog::open(dir.join("log")).expect("open log");
    let engine = Engine::with_authority(
        MemoryStore::new(),
        SchemaHealth,
        RuleBasedSummarizer,
        SchemaValidator,
        policy,
        log,
        dir.join("snapshots"),
    );
    (engine, dir)
}

fn engine() -> (Ledger, PathBuf) {
    engine_with(Policy::with_defaults())
}

fn set(path: &str, value: &str) -> Operation {
    Operation::Set {
        path: path.to_string(),
        value: value.to_string(),
    }
}

fn cleanup(dir: &Path) {
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn denied_write_is_rejected_and_state_is_untouched() {
    let (mut engine, dir) = engine();

    let error = engine
        .apply(
            "txn_cross_user",
            "user:ada",
            AccessMode::Guarded,
            "write bob's theme",
            vec![set("user/bob/theme/mode", "dark")],
        )
        .expect_err("cross-user write must be denied");

    assert_eq!(error.transaction.status, Status::Rejected);
    assert!(error.reason.contains("no capability"));
    assert_eq!(engine.store().get("user/bob/theme/mode"), None);
    assert_eq!(engine.entries().len(), 1);
    assert_eq!(engine.entries()[0].status, Status::Rejected);
    assert_eq!(engine.entries()[0].principal, "user:ada");
    assert_eq!(engine.entries()[0].sandbox, "user-write");
    cleanup(&dir);
}

#[test]
fn guarded_requires_approval_but_full_commits() {
    let mut policy = Policy::with_defaults();
    policy.add(
        Grant::new(
            Principal::User("ada".into()),
            ActionClass::StateWrite,
            "system/services/network/**",
            "system",
        )
        .requiring_approval(),
    );
    let (mut engine, dir) = engine_with(policy);

    let guarded = engine
        .apply(
            "txn_guarded",
            "user:ada",
            AccessMode::Guarded,
            "take the network down",
            vec![set("system/services/network/state", "down")],
        )
        .expect_err("guarded mode needs approval");
    assert_eq!(guarded.transaction.status, Status::PendingApproval);
    assert_eq!(engine.store().get("system/services/network/state"), None);

    let full = engine
        .apply(
            "txn_full",
            "user:ada",
            AccessMode::Full,
            "take the network down",
            vec![set("system/services/network/state", "down")],
        )
        .expect("full mode auto-approves");
    assert_eq!(full.status, Status::Committed);
    assert_eq!(
        engine
            .store()
            .get("system/services/network/state")
            .as_deref(),
        Some("down")
    );

    let last = engine.entries().last().expect("entry");
    assert_eq!(last.status, Status::Committed);
    assert_eq!(last.mode, "full");
    assert_eq!(last.sandbox, "system-write");
    cleanup(&dir);
}

#[test]
fn agent_borrows_authority_and_stays_in_scope() {
    let (mut engine, dir) = engine();

    let own = engine
        .apply(
            "txn_agent_own",
            "agent:opencode:ses_1@user:ada",
            AccessMode::Guarded,
            "switch ada to dark mode",
            vec![set("user/ada/theme/mode", "dark")],
        )
        .expect("agent may act for ada");
    assert_eq!(own.status, Status::Committed);

    let denied = engine
        .apply(
            "txn_agent_other",
            "agent:opencode:ses_1@user:ada",
            AccessMode::Guarded,
            "switch bob to dark mode",
            vec![set("user/bob/theme/mode", "dark")],
        )
        .expect_err("agent may not act for bob");
    assert_eq!(denied.transaction.status, Status::Rejected);

    assert_eq!(
        engine.entries()[0].principal,
        "agent:opencode:ses_1@user:ada"
    );
    cleanup(&dir);
}

#[test]
fn cross_principal_revert_is_denied() {
    let (mut engine, dir) = engine();

    engine
        .apply(
            "txn_ada",
            "user:ada",
            AccessMode::Guarded,
            "move ada's clock",
            vec![set("user/ada/desktop/clock/position", "top-right")],
        )
        .expect("commit");

    let denied = engine
        .revert("txn_ada", "user:bob", AccessMode::Guarded)
        .expect_err("bob cannot revert ada");
    assert!(denied.reason.contains("cannot revert"));

    assert_eq!(
        engine
            .store()
            .get("user/ada/desktop/clock/position")
            .as_deref(),
        Some("top-right")
    );
    assert_eq!(engine.entries().last().unwrap().status, Status::Rejected);

    engine
        .revert("txn_ada", "user:ada", AccessMode::Guarded)
        .expect("ada can revert her own");
    assert_eq!(engine.store().get("user/ada/desktop/clock/position"), None);
    cleanup(&dir);
}

#[test]
fn recovery_core_is_immutable_at_runtime() {
    let (mut engine, dir) = engine();

    let error = engine
        .apply(
            "txn_core",
            "system",
            AccessMode::Full,
            "patch the recovery core",
            vec![set("core/boot/kernel", "tampered")],
        )
        .expect_err("recovery core must be immutable");

    assert_eq!(error.transaction.status, Status::Rejected);
    assert!(error.reason.contains("recovery core"));
    assert_eq!(engine.store().get("core/boot/kernel"), None);
    cleanup(&dir);
}
