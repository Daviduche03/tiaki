//! Append-only, hash-chained action log.
//!
//! The authoritative record of every mutation. See TIAKI.md §18.2.
//!
//! - `sha256` — dependency-free SHA-256
//! - `entry` — one immutable log record
//! - `store` — the chain, its verification, and persistence

#![allow(dead_code)]

mod entry;
mod sha256;
mod store;

pub use entry::{Impact, LogEntry, Status, Summary};
pub use sha256::{digest, hex};
pub use store::{format_timestamp, ActionLog, EntryInput, LogError, GENESIS_HASH};
