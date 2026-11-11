//! Per-principal quotas and token accounting.
//!
//! The capability broker says *whether* a principal may generate. The quota
//! book says *how much* and keeps one principal's batch work from starving
//! another.

use std::collections::BTreeMap;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quota {
    pub max_concurrent: usize,
    pub max_tokens_per_request: usize,
    pub tokens_per_minute: u64,
    pub window_secs: u64,
}

impl Quota {
    pub fn for_principal(principal: &str) -> Self {
        if principal == "system" || principal.starts_with("service:") {
            Self {
                max_concurrent: 8,
                max_tokens_per_request: 4096,
                tokens_per_minute: 1_000_000,
                window_secs: 60,
            }
        } else {
            Self {
                max_concurrent: 2,
                max_tokens_per_request: 1024,
                tokens_per_minute: 8192,
                window_secs: 60,
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuotaError {
    pub message: String,
}

impl QuotaError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for QuotaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for QuotaError {}

#[derive(Default)]
pub struct UsageBook {
    /// principal -> (timestamp, tokens)
    windows: BTreeMap<String, Vec<(u64, usize)>>,
}

impl UsageBook {
    pub fn new() -> Self {
        Self::default()
    }

    /// Check a request before it is queued. Does not record usage.
    pub fn try_admit(
        &self,
        principal: &str,
        now: u64,
        tokens: usize,
        quota: Quota,
    ) -> Result<(), QuotaError> {
        if tokens > quota.max_tokens_per_request {
            return Err(QuotaError::new(format!(
                "request asks for {tokens} tokens; the limit is {}",
                quota.max_tokens_per_request
            )));
        }
        let used = self.used_in_window(principal, now, quota.window_secs);
        if used.saturating_add(tokens as u64) > quota.tokens_per_minute {
            return Err(QuotaError::new(format!(
                "{principal} would exceed its token window ({used}/{} used)",
                quota.tokens_per_minute
            )));
        }
        Ok(())
    }

    /// Record tokens actually generated.
    pub fn charge(&mut self, principal: &str, now: u64, tokens: usize) {
        self.windows
            .entry(principal.to_string())
            .or_default()
            .push((now, tokens));
    }

    pub fn used_in_window(&self, principal: &str, now: u64, window_secs: u64) -> u64 {
        self.windows
            .get(principal)
            .map(|entries| {
                entries
                    .iter()
                    .filter(|(timestamp, _)| now.saturating_sub(*timestamp) < window_secs)
                    .map(|(_, tokens)| *tokens as u64)
                    .sum()
            })
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denies_over_the_token_window() {
        let quota = Quota {
            max_concurrent: 1,
            max_tokens_per_request: 64,
            tokens_per_minute: 100,
            window_secs: 60,
        };
        let mut usage = UsageBook::new();
        usage.charge("user:ada", 1_000, 80);
        assert!(usage.try_admit("user:ada", 1_010, 30, quota).is_err());
        assert!(usage.try_admit("user:ada", 1_010, 10, quota).is_ok());
        // After the window, the old usage ages out.
        assert!(usage.try_admit("user:ada", 1_100, 30, quota).is_ok());
    }

    #[test]
    fn caps_a_single_request() {
        let quota = Quota::for_principal("user:ada");
        let usage = UsageBook::new();
        assert!(usage
            .try_admit("user:ada", 0, quota.max_tokens_per_request + 1, quota)
            .is_err());
    }
}
