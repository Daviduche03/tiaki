//! Log entry: the immutable record of one transaction.

use crate::sha256;

/// Human-facing effects of a change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Impact {
    pub reboot_required: bool,
    pub services_restarted: u32,
    pub ui_changed: bool,
}

impl Impact {
    pub fn none() -> Self {
        Self {
            reboot_required: false,
            services_restarted: 0,
            ui_changed: false,
        }
    }

    pub fn is_noop(&self) -> bool {
        !self.reboot_required && self.services_restarted == 0 && !self.ui_changed
    }
}

/// Plain-language description. Derived, never authoritative.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Summary {
    pub one_line: String,
    pub detail: String,
    pub source: String,
    pub verified: bool,
}

impl Summary {
    pub fn new(
        one_line: impl Into<String>,
        detail: impl Into<String>,
        source: impl Into<String>,
        verified: bool,
    ) -> Self {
        Self {
            one_line: one_line.into(),
            detail: detail.into(),
            source: source.into(),
            verified,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Committed,
    Reverted,
    Failed,
    Rejected,
    PendingApproval,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Committed => "committed",
            Status::Reverted => "reverted",
            Status::Failed => "failed",
            Status::Rejected => "rejected",
            Status::PendingApproval => "pending-approval",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "committed" => Some(Status::Committed),
            "reverted" => Some(Status::Reverted),
            "failed" => Some(Status::Failed),
            "rejected" => Some(Status::Rejected),
            "pending-approval" => Some(Status::PendingApproval),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LogEntry {
    pub seq: u64,
    pub prev_hash: String,
    pub hash: String,
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

impl LogEntry {
    /// Canonical, unambiguous byte string used as the hash input.
    pub fn canonical(&self) -> String {
        let mut out = String::new();
        push_field(&mut out, &self.seq.to_string());
        push_field(&mut out, &self.prev_hash);
        push_field(&mut out, &self.timestamp.to_string());
        push_field(&mut out, &self.principal);
        push_field(&mut out, &self.mode);
        push_field(&mut out, &self.sandbox);
        push_field(&mut out, &self.transaction_id);
        push_field(&mut out, &self.intent);
        push_field(&mut out, &self.summary.one_line);
        push_field(&mut out, &self.summary.detail);
        push_field(&mut out, &self.summary.source);
        push_field(&mut out, &self.summary.verified.to_string());
        push_field(&mut out, &self.impact.reboot_required.to_string());
        push_field(&mut out, &self.impact.services_restarted.to_string());
        push_field(&mut out, &self.impact.ui_changed.to_string());
        push_field(&mut out, self.status.as_str());
        push_field(&mut out, &self.operations.len().to_string());
        for operation in &self.operations {
            push_field(&mut out, operation);
        }
        out
    }

    pub fn compute_hash(&self) -> String {
        sha256::hex(&sha256::digest(self.canonical().as_bytes()))
    }

    pub fn to_line(&self) -> String {
        let operations = self.operations.join("\u{1f}");
        let fields = [
            self.seq.to_string(),
            self.prev_hash.clone(),
            self.hash.clone(),
            self.timestamp.to_string(),
            self.principal.clone(),
            self.mode.clone(),
            self.sandbox.clone(),
            self.transaction_id.clone(),
            self.intent.clone(),
            self.summary.one_line.clone(),
            self.summary.detail.clone(),
            self.summary.source.clone(),
            self.summary.verified.to_string(),
            self.impact.reboot_required.to_string(),
            self.impact.services_restarted.to_string(),
            self.impact.ui_changed.to_string(),
            self.status.as_str().to_string(),
            operations,
        ];
        fields
            .iter()
            .map(|field| escape(field))
            .collect::<Vec<_>>()
            .join("\t")
    }

    pub fn from_line(line: &str) -> Option<Self> {
        let raw: Vec<&str> = line.split('\t').collect();
        if raw.len() != 18 {
            return None;
        }
        let fields: Vec<String> = raw.iter().map(|field| unescape(field)).collect();
        let operations = if fields[17].is_empty() {
            Vec::new()
        } else {
            fields[17].split('\u{1f}').map(str::to_string).collect()
        };
        Some(LogEntry {
            seq: fields[0].parse().ok()?,
            prev_hash: fields[1].clone(),
            hash: fields[2].clone(),
            timestamp: fields[3].parse().ok()?,
            principal: fields[4].clone(),
            mode: fields[5].clone(),
            sandbox: fields[6].clone(),
            transaction_id: fields[7].clone(),
            intent: fields[8].clone(),
            summary: Summary {
                one_line: fields[9].clone(),
                detail: fields[10].clone(),
                source: fields[11].clone(),
                verified: fields[12].parse().ok()?,
            },
            impact: Impact {
                reboot_required: fields[13].parse().ok()?,
                services_restarted: fields[14].parse().ok()?,
                ui_changed: fields[15].parse().ok()?,
            },
            status: Status::parse(&fields[16])?,
            operations,
        })
    }
}

fn push_field(out: &mut String, value: &str) {
    out.push_str(&value.len().to_string());
    out.push(':');
    out.push_str(value);
}

fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{1f}' => out.push_str("\\u001f"),
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
            Some('u') => {
                let mut buf = String::new();
                for _ in 0..4 {
                    if let Some(next) = chars.next() {
                        buf.push(next);
                    }
                }
                if buf == "001f" {
                    out.push('\u{1f}');
                } else {
                    out.push('\\');
                    out.push('u');
                    out.push_str(&buf);
                }
            }
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

    fn sample() -> LogEntry {
        LogEntry {
            seq: 1,
            prev_hash: "0".repeat(64),
            hash: String::new(),
            timestamp: 1_700_000_000,
            principal: "user:ada".into(),
            mode: "guarded".into(),
            sandbox: "user-write".into(),
            transaction_id: "txn_1".into(),
            intent: "Move clock".into(),
            summary: Summary::new("Moved the clock.", "Detail.", "rule-based", true),
            impact: Impact::none(),
            status: Status::Committed,
            operations: vec!["set desktop/clock/position=top-right".into()],
        }
    }

    #[test]
    fn round_trips_through_a_line() {
        let mut entry = sample();
        entry.hash = entry.compute_hash();
        let line = entry.to_line();
        let parsed = LogEntry::from_line(&line).expect("parse");
        assert_eq!(parsed.hash, entry.hash);
        assert_eq!(parsed.summary.one_line, entry.summary.one_line);
        assert_eq!(parsed.operations, entry.operations);
    }

    #[test]
    fn escapes_tabs_and_newlines() {
        let mut entry = sample();
        entry.summary.one_line = "line one\nline\ttwo".into();
        entry.hash = entry.compute_hash();
        let parsed = LogEntry::from_line(&entry.to_line()).expect("parse");
        assert_eq!(parsed.summary.one_line, "line one\nline\ttwo");
    }
}
