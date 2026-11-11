//! Live scheduling for real processes: the class a launch belongs to, and
//! the rebalance that pushes shares into the kernel.
//!
//! The launcher binary and the inference broker both call [`rebalance`], so a
//! burst takes effect on running processes instead of waiting for the next
//! launch.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use agentic_sandbox::cgroup::CgroupProvisioner;
use agentic_sandbox::{SandboxHandle, SandboxProfile, SandboxProvisioner};

use crate::supervisor::{allocate, split_evenly};

#[allow(dead_code)]
pub const PREFIX_SESSION: &str = "session";
pub const PREFIX_FOREGROUND: &str = "fg";
pub const PREFIX_BACKGROUND: &str = "bg";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Session,
    Foreground,
    Background,
}

impl Class {
    pub fn prefix(self) -> &'static str {
        match self {
            Class::Session => PREFIX_SESSION,
            Class::Foreground => PREFIX_FOREGROUND,
            Class::Background => PREFIX_BACKGROUND,
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "session" => Some(Class::Session),
            "foreground" | "fg" => Some(Class::Foreground),
            "background" | "bg" => Some(Class::Background),
            _ => None,
        }
    }

    pub fn from_dir(name: &str) -> Option<Self> {
        // `fg-1234` -> Foreground, `session-1234` -> Session.
        let prefix = name.split('-').next()?;
        match prefix {
            PREFIX_SESSION => Some(Class::Session),
            PREFIX_FOREGROUND => Some(Class::Foreground),
            PREFIX_BACKGROUND => Some(Class::Background),
            _ => None,
        }
    }

    pub fn profile(self) -> SandboxProfile {
        SandboxProfile::UserWrite
    }
}

/// Where the inference broker signals a burst.
pub fn burst_flag() -> PathBuf {
    std::env::var("TIAKI_INFERENCE_FLAG")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/run/tiaki/inference-active"))
}

pub fn rebalance(root: &Path) -> Result<Vec<(String, u32)>, String> {
    let burst = burst_flag().exists();
    let mut buckets: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) => return Err(format!("cannot read {}: {error}", root.display())),
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if let Some(class) = Class::from_dir(&name) {
            buckets.entry(class.prefix()).or_default().push(name);
        }
    }
    for names in buckets.values_mut() {
        names.sort();
    }

    let sessions = buckets.get(PREFIX_SESSION).map(Vec::len).unwrap_or(0);
    let foreground = buckets.get(PREFIX_FOREGROUND).map(Vec::len).unwrap_or(0);
    let background = buckets.get(PREFIX_BACKGROUND).map(Vec::len).unwrap_or(0);
    let allocation = allocate(sessions, foreground, background, burst);

    let provisioner = CgroupProvisioner::new(root);
    let mut applied = Vec::new();
    for prefix in [PREFIX_SESSION, PREFIX_FOREGROUND, PREFIX_BACKGROUND] {
        let Some(names) = buckets.get(prefix) else {
            continue;
        };
        let pool = if prefix == PREFIX_SESSION {
            allocation.session
        } else if prefix == PREFIX_FOREGROUND {
            allocation.foreground
        } else {
            allocation.background
        };
        for (name, shares) in names.iter().zip(split_evenly(pool, names.len())) {
            provisioner
                .set_shares(&SandboxHandle { id: name.clone() }, shares)
                .map_err(|error| format!("{name}: {error}"))?;
            applied.push((name.clone(), shares));
        }
    }
    Ok(applied)
}
