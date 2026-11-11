//! Append-only, hash-chained action log.

use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::entry::{Impact, LogEntry, Status, Summary};

pub const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Debug)]
pub enum LogError {
    Io(io::Error),
    Parse { line: usize },
    BrokenChain { seq: u64, reason: String },
    Tampered { seq: u64 },
}

impl std::fmt::Display for LogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogError::Io(error) => write!(f, "io error: {error}"),
            LogError::Parse { line } => write!(f, "could not parse log line {line}"),
            LogError::BrokenChain { seq, reason } => {
                write!(f, "broken hash chain at seq {seq}: {reason}")
            }
            LogError::Tampered { seq } => {
                write!(
                    f,
                    "entry {seq} does not match its hash (tampered or corrupted)"
                )
            }
        }
    }
}

impl std::error::Error for LogError {}

impl From<io::Error> for LogError {
    fn from(error: io::Error) -> Self {
        LogError::Io(error)
    }
}

/// Input for a new entry. `seq`, `prev_hash`, and `hash` are assigned by the log.
pub struct EntryInput {
    pub timestamp: u64,
    pub principal: String,
    pub mode: String,
    pub sandbox: String,
    pub transaction_id: String,
    pub intent: String,
    pub summary: Summary,
    pub impact: Impact,
    pub status: Status,
    pub operations: Vec<String>,
}

pub struct ActionLog {
    path: PathBuf,
    entries: Vec<LogEntry>,
}

impl ActionLog {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, LogError> {
        let path = path.as_ref().to_path_buf();
        let mut entries = Vec::new();

        if path.exists() {
            let file = fs::File::open(&path)?;
            for (index, line) in BufReader::new(file).lines().enumerate() {
                let line = line?;
                if line.trim().is_empty() {
                    continue;
                }
                let entry =
                    LogEntry::from_line(&line).ok_or(LogError::Parse { line: index + 1 })?;
                entries.push(entry);
            }
        }

        let log = Self { path, entries };
        log.verify()?;
        Ok(log)
    }

    pub fn entries(&self) -> &[LogEntry] {
        &self.entries
    }

    /// Re-read the log from disk. A long-running server calls this so entries
    /// appended by another `agentd` process are visible and the hash chain
    /// stays consistent.
    pub fn reload(&mut self) -> Result<(), LogError> {
        let reopened = Self::open(&self.path)?;
        self.entries = reopened.entries;
        Ok(())
    }

    pub fn last_hash(&self) -> String {
        self.entries
            .last()
            .map(|entry| entry.hash.clone())
            .unwrap_or_else(|| GENESIS_HASH.to_string())
    }

    pub fn append(&mut self, input: EntryInput) -> Result<LogEntry, LogError> {        let seq = self.entries.len() as u64 + 1;
        let prev_hash = self.last_hash();

        let mut entry = LogEntry {
            seq,
            prev_hash,
            hash: String::new(),
            timestamp: input.timestamp,
            principal: input.principal,
            mode: input.mode,
            sandbox: input.sandbox,
            transaction_id: input.transaction_id,
            intent: input.intent,
            summary: input.summary,
            impact: input.impact,
            status: input.status,
            operations: input.operations,
        };
        entry.hash = entry.compute_hash();

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(file, "{}", entry.to_line())?;
        file.flush()?;

        self.entries.push(entry.clone());
        Ok(entry)
    }

    /// Append, safe across processes. The agent runner and its MCP server are
    /// different processes writing one log; without the lock both compute the
    /// same next sequence and the chain breaks on the next read. The lock is
    /// an empty file created with `O_EXCL`, so creation itself is the atomic
    /// step; a lock older than 30 seconds is treated as a crashed holder.
    pub fn append_exclusive(&mut self, input: EntryInput) -> Result<LogEntry, LogError> {
        let lock = self.path.with_extension("lock");
        acquire_lock(&lock, Duration::from_secs(10))?;
        let result = self.reload().and_then(|()| self.append(input));
        let _ = fs::remove_file(&lock);
        result
    }

    /// Rewrite a corrupted log in on-disk order with a fresh sequence and
    /// hash chain. Content is preserved; only bookkeeping is recomputed. The
    /// original is kept beside it as `log.corrupt-<nanos>`.
    pub fn repair_file(path: impl AsRef<Path>) -> Result<usize, LogError> {
        let path = path.as_ref();
        let file = fs::File::open(path)?;
        let mut entries = Vec::new();
        for (index, line) in BufReader::new(file).lines().enumerate() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            entries.push(
                LogEntry::from_line(&line).ok_or(LogError::Parse { line: index + 1 })?,
            );
        }
        let mut prev = GENESIS_HASH.to_string();
        for (index, entry) in entries.iter_mut().enumerate() {
            entry.seq = index as u64 + 1;
            entry.prev_hash = std::mem::replace(&mut prev, String::new());
            entry.hash = entry.compute_hash();
            prev = entry.hash.clone();
        }
        let backup = path.with_extension(format!(
            "corrupt-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        fs::copy(path, &backup)?;
        let mut out = String::new();
        for entry in &entries {
            out.push_str(&entry.to_line());
            out.push('\n');
        }
        fs::write(path, out)?;
        Ok(entries.len())
    }

    pub fn verify(&self) -> Result<(), LogError> {
        let mut prev = GENESIS_HASH.to_string();
        for (index, entry) in self.entries.iter().enumerate() {
            let expected_seq = index as u64 + 1;
            if entry.seq != expected_seq {
                return Err(LogError::BrokenChain {
                    seq: entry.seq,
                    reason: format!("expected seq {expected_seq}"),
                });
            }
            if entry.prev_hash != prev {
                return Err(LogError::BrokenChain {
                    seq: entry.seq,
                    reason: "prev_hash does not match the previous entry".into(),
                });
            }
            if entry.compute_hash() != entry.hash {
                return Err(LogError::Tampered { seq: entry.seq });
            }
            prev = entry.hash.clone();
        }
        Ok(())
    }
}

/// Format Unix seconds as `YYYY-MM-DD HH:MM:SSZ` (UTC).
fn acquire_lock(lock: &Path, timeout: Duration) -> Result<(), LogError> {
    let deadline = Instant::now() + timeout;
    loop {
        match OpenOptions::new().write(true).create_new(true).open(lock) {
            Ok(mut file) => {
                let _ = writeln!(file, "{}", std::process::id());
                return Ok(());
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                if let Ok(meta) = fs::metadata(lock) {
                    if let Ok(mtime) = meta.modified() {
                        if mtime.elapsed().unwrap_or_default() > Duration::from_secs(30) {
                            let _ = fs::remove_file(lock);
                            continue;
                        }
                    }
                }
                if Instant::now() >= deadline {
                    return Err(LogError::Io(io::Error::new(
                        io::ErrorKind::TimedOut,
                        format!("log lock busy: {}", lock.display()),
                    )));
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(error) => return Err(LogError::Io(error)),
        }
    }
}

pub fn format_timestamp(seconds: u64) -> String {    let days = (seconds / 86_400) as i64;
    let remainder = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = remainder / 3_600;
    let minute = (remainder % 3_600) / 60;
    let second = remainder % 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}Z")
}

/// Howard Hinnant's `civil_from_days`.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(id: &str) -> EntryInput {
        EntryInput {
            timestamp: 1_700_000_000,
            principal: "user:ada".into(),
            mode: "guarded".into(),
            sandbox: "user-write".into(),
            transaction_id: id.into(),
            intent: "test".into(),
            summary: Summary::new("Did a thing.", "", "rule-based", true),
            impact: Impact::none(),
            status: Status::Committed,
            operations: vec!["set a=b".into()],
        }
    }

    #[test]
    fn timestamp_formats() {
        assert_eq!(format_timestamp(0), "1970-01-01 00:00:00Z");
        assert_eq!(format_timestamp(1_700_000_000), "2023-11-14 22:13:20Z");
    }

    #[test]
    fn chain_verifies_and_detects_tampering() {
        let dir = std::env::temp_dir().join(format!("tiaki-log-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("log");

        let mut log = ActionLog::open(&path).expect("open");
        log.append(input("txn_1")).expect("append");
        log.append(input("txn_2")).expect("append");
        log.verify().expect("valid chain");
        assert_eq!(log.entries().len(), 2);

        let mut reopened = ActionLog::open(&path).expect("reopen");
        reopened.verify().expect("valid after reopen");

        let mut tampered = log.entries()[0].clone();
        tampered.summary.one_line = "Something else.".into();
        assert_ne!(tampered.compute_hash(), log.entries()[0].hash);

        let _ = fs::remove_dir_all(&dir);
    }
}
