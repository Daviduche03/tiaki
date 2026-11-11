//! End-to-end plugin lifecycle: install, verify, update, migrate, roll back,
//! remove, and config validation.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use agentic_plugin_host::signature::compute_signature;
use agentic_plugin_host::{
    DigestVerifier, HostError, InstallOptions, Json, PluginHost, PluginState, Swap, Version,
};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_dir(name: &str) -> PathBuf {
    let index = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "tiaki-plugin-{}-{}-{}",
        name,
        std::process::id(),
        index
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

const CONFIG_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "position": { "enum": ["left", "center", "right"] },
        "format": { "enum": ["12h", "24h"] },
        "show_seconds": { "type": "boolean", "default": false }
    },
    "additionalProperties": false
}"#;

struct Fixture {
    version: &'static str,
    kind: &'static str,
    api: &'static str,
    migrations: Vec<(&'static str, &'static str, &'static str)>,
}

impl Default for Fixture {
    fn default() -> Self {
        Self {
            version: "1.0.0",
            kind: "ui",
            api: "1",
            migrations: Vec::new(),
        }
    }
}

/// Write a plugin directory and sign it with a real content digest.
fn write_plugin(dir: &Path, fixture: &Fixture) {
    fs::create_dir_all(dir.join("ui")).unwrap();
    fs::create_dir_all(dir.join("schema")).unwrap();
    fs::create_dir_all(dir.join("migrations")).unwrap();

    fs::write(dir.join("libclock.so"), "fake shared object").unwrap();
    fs::write(
        dir.join("ui/clock.ui.json"),
        "{\"id\":\"clock\",\"type\":\"clock\"}",
    )
    .unwrap();
    fs::write(dir.join("ui/clock.schema.json"), "{}").unwrap();
    fs::write(dir.join("schema/clock.config.schema.json"), CONFIG_SCHEMA).unwrap();

    for (index, (from, to, set)) in fixture.migrations.iter().enumerate() {
        fs::write(
            dir.join(format!("migrations/{index:04}.json")),
            format!(r#"{{"from":"{from}","to":"{to}","set":{set}}}"#),
        )
        .unwrap();
    }

    let manifest = manifest_text(fixture, "sha256:placeholder");
    let signature = compute_signature(dir, &manifest).expect("signature");
    fs::write(dir.join("plugin.toml"), manifest_text(fixture, &signature)).unwrap();
}

fn manifest_text(fixture: &Fixture, signature: &str) -> String {
    format!(
        r#"[plugin]
id = "org.tiaki.panel.clock"
name = "Clock Panel"
version = "{}"
api = "{}"
kind = "{}"
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
signature = "{signature}"
"#,
        fixture.version, fixture.api, fixture.kind
    )
}

fn open_host(root: &Path) -> PluginHost {
    PluginHost::open(root, Box::new(DigestVerifier)).expect("open host")
}

fn install(host: &mut PluginHost, source: &Path) -> agentic_plugin_host::InstallOutcome {
    host.install(source, &InstallOptions::default())
        .expect("install")
}

fn version(value: &str) -> Version {
    Version::parse(value).unwrap()
}

#[test]
fn installs_verifies_and_builds_a_spec() {
    let work = temp_dir("install");
    let source = work.join("source");
    fs::create_dir_all(&source).unwrap();
    write_plugin(&source, &Fixture::default());

    let mut host = open_host(&work.join("root"));
    let outcome = install(&mut host, &source);

    assert!(outcome.signature.is_verified());
    assert_eq!(outcome.version, version("1.0.0"));
    assert_eq!(outcome.tier, agentic_plugin_host::Tier::Content);
    assert_eq!(
        outcome.config.get("show_seconds"),
        Some(&Json::Bool(false)),
        "schema defaults are seeded"
    );

    let installed = host.get("org.tiaki.panel.clock").expect("installed");
    assert_eq!(installed.state, PluginState::Installed);
    assert_eq!(installed.active, version("1.0.0"));

    let spec = host.spec("org.tiaki.panel.clock").expect("spec");
    assert!(spec.command.ends_with("libclock.so"));
    assert_eq!(spec.limits.memory_bytes, Some(64 * 1024 * 1024));
    assert!(!spec.limits.network);

    let _ = fs::remove_dir_all(&work);
}

#[test]
fn update_hot_swaps_and_migrates_config() {
    let work = temp_dir("update");
    let v1 = work.join("v1");
    let v2 = work.join("v2");
    fs::create_dir_all(&v1).unwrap();
    fs::create_dir_all(&v2).unwrap();
    write_plugin(&v1, &Fixture::default());
    write_plugin(
        &v2,
        &Fixture {
            version: "1.1.0",
            migrations: vec![("1.0.0", "1.1.0", r#"{"format":"24h"}"#)],
            ..Fixture::default()
        },
    );

    let mut host = open_host(&work.join("root"));
    install(&mut host, &v1);

    let mut values = BTreeMap::new();
    values.insert("position".to_string(), Json::String("left".into()));
    host.set_config("org.tiaki.panel.clock", values)
        .expect("config");

    let outcome = host
        .update(&v2, &InstallOptions::default())
        .expect("update");
    assert_eq!(outcome.from, version("1.0.0"));
    assert_eq!(outcome.to, version("1.1.0"));
    assert_eq!(outcome.swap, Swap::HotSwapped);
    assert_eq!(outcome.migrations, vec![version("1.1.0")]);

    let config = host.config("org.tiaki.panel.clock").expect("config");
    assert_eq!(config.get("position"), Some(&Json::String("left".into())));
    assert_eq!(config.get("format"), Some(&Json::String("24h".into())));

    let installed = host.get("org.tiaki.panel.clock").unwrap();
    assert_eq!(installed.active, version("1.1.0"));
    assert_eq!(installed.previous, Some(version("1.0.0")));

    let swap = host.rollback("org.tiaki.panel.clock").expect("rollback");
    assert_eq!(swap, Swap::HotSwapped);
    assert_eq!(
        host.get("org.tiaki.panel.clock").unwrap().active,
        version("1.0.0")
    );

    let _ = fs::remove_dir_all(&work);
}

#[test]
fn session_tier_asks_for_a_restart() {
    let work = temp_dir("session");
    let v1 = work.join("v1");
    let v2 = work.join("v2");
    fs::create_dir_all(&v1).unwrap();
    fs::create_dir_all(&v2).unwrap();
    write_plugin(
        &v1,
        &Fixture {
            kind: "service",
            ..Fixture::default()
        },
    );
    write_plugin(
        &v2,
        &Fixture {
            version: "1.1.0",
            kind: "service",
            ..Fixture::default()
        },
    );

    let mut host = open_host(&work.join("root"));
    install(&mut host, &v1);
    let outcome = host
        .update(&v2, &InstallOptions::default())
        .expect("update");
    assert_eq!(outcome.swap, Swap::RestartRequired);

    let _ = fs::remove_dir_all(&work);
}

#[test]
fn remove_unloads_and_deletes() {
    let work = temp_dir("remove");
    let source = work.join("source");
    fs::create_dir_all(&source).unwrap();
    write_plugin(&source, &Fixture::default());

    let mut host = open_host(&work.join("root"));
    install(&mut host, &source);
    host.load("org.tiaki.panel.clock").expect("load");
    host.start("org.tiaki.panel.clock").expect("start");

    let outcome = host.remove("org.tiaki.panel.clock").expect("remove");
    assert_eq!(outcome.version, version("1.0.0"));
    assert!(host.get("org.tiaki.panel.clock").is_none());
    assert!(!work.join("root/org.tiaki.panel.clock").exists());

    let _ = fs::remove_dir_all(&work);
}

#[test]
fn rejects_unverified_and_tampered_plugins() {
    let work = temp_dir("signature");
    let source = work.join("source");
    fs::create_dir_all(&source).unwrap();
    write_plugin(&source, &Fixture::default());

    // Replace the digest with an ed25519 signature this host cannot verify.
    let manifest = fs::read_to_string(source.join("plugin.toml")).unwrap();
    let manifest = manifest
        .lines()
        .map(|line| {
            if line.starts_with("signature") {
                "signature = \"ed25519:abc\""
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(source.join("plugin.toml"), manifest).unwrap();

    let mut host = open_host(&work.join("root"));
    let error = host
        .install(&source, &InstallOptions::default())
        .expect_err("unverified must be rejected");
    assert!(matches!(error, HostError::Signature(_)));

    host.install(
        &source,
        &InstallOptions {
            allow_unverified: true,
        },
    )
    .expect("explicit override installs");

    // Now tamper with an asset after signing.
    let work2 = temp_dir("tamper");
    let source2 = work2.join("source");
    fs::create_dir_all(&source2).unwrap();
    write_plugin(&source2, &Fixture::default());
    fs::write(source2.join("libclock.so"), "tampered").unwrap();

    let mut host2 = open_host(&work2.join("root"));
    let error = host2
        .install(&source2, &InstallOptions::default())
        .expect_err("tamper must be rejected");
    assert!(matches!(error, HostError::Signature(_)));

    let _ = fs::remove_dir_all(&work);
    let _ = fs::remove_dir_all(&work2);
}

#[test]
fn config_is_validated_against_the_schema() {
    let work = temp_dir("config");
    let source = work.join("source");
    fs::create_dir_all(&source).unwrap();
    write_plugin(&source, &Fixture::default());

    let mut host = open_host(&work.join("root"));
    install(&mut host, &source);

    let mut bad = BTreeMap::new();
    bad.insert("position".to_string(), Json::String("diagonal".into()));
    let error = host
        .set_config("org.tiaki.panel.clock", bad)
        .expect_err("bad enum");
    assert!(matches!(error, HostError::Schema(_)));

    let _ = fs::remove_dir_all(&work);
}

#[test]
fn incompatible_api_is_rejected() {
    let work = temp_dir("api");
    let source = work.join("source");
    fs::create_dir_all(&source).unwrap();
    write_plugin(
        &source,
        &Fixture {
            api: "2",
            ..Fixture::default()
        },
    );

    let mut host = open_host(&work.join("root"));
    let error = host
        .install(&source, &InstallOptions::default())
        .expect_err("api mismatch");
    match error {
        HostError::Invalid(issues) => assert!(issues
            .iter()
            .any(|issue| issue.code == "plugin.incompatible_api")),
        other => panic!("unexpected error: {other}"),
    }

    let _ = fs::remove_dir_all(&work);
}
