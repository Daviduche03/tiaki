//! Generation table, boot watchdog, and automatic fallback.
//!
//! A generation is an immutable OS image. The table tracks the running one,
//! the last known-good one, and an optional candidate. The watchdog counts
//! boot attempts; a generation that never reports healthy is rolled back.
//!
//! This is recovery-core data: it lives outside the generations it describes.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::Path;

use agentic_json::Json;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenerationStatus {
    Staged,
    Candidate,
    Active,
    LastGood,
    Failed,
    Retired,
}

impl GenerationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            GenerationStatus::Staged => "staged",
            GenerationStatus::Candidate => "candidate",
            GenerationStatus::Active => "active",
            GenerationStatus::LastGood => "last-good",
            GenerationStatus::Failed => "failed",
            GenerationStatus::Retired => "retired",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "staged" => Some(GenerationStatus::Staged),
            "candidate" => Some(GenerationStatus::Candidate),
            "active" => Some(GenerationStatus::Active),
            "last-good" => Some(GenerationStatus::LastGood),
            "failed" => Some(GenerationStatus::Failed),
            "retired" => Some(GenerationStatus::Retired),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Generation {
    pub id: String,
    pub label: String,
    pub created_at: u64,
    /// Directory under the generations root.
    pub image: String,
    pub digest: String,
    pub status: GenerationStatus,
    pub attempts: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BootDecision {
    Proceed {
        generation: String,
        attempt: u32,
    },
    Rollback {
        from: String,
        to: String,
        reason: String,
    },
    Rescue {
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenerationTable {
    pub active: Option<String>,
    pub last_good: Option<String>,
    pub candidate: Option<String>,
    pub boot_id: String,
    pub max_attempts: u32,
    pub generations: BTreeMap<String, Generation>,
}

impl Default for GenerationTable {
    fn default() -> Self {
        Self::new()
    }
}

impl GenerationTable {
    pub fn new() -> Self {
        Self {
            active: None,
            last_good: None,
            candidate: None,
            boot_id: "boot_0".into(),
            max_attempts: 3,
            generations: BTreeMap::new(),
        }
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, GenerationError> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(Self::new());
        }
        let text = fs::read_to_string(path)?;
        Self::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Self, GenerationError> {
        let value =
            agentic_json::parse(text).map_err(|error| GenerationError::new(error.to_string()))?;
        let object = value
            .as_object()
            .ok_or_else(|| GenerationError::new("generation table must be a JSON object"))?;

        let optional = |key: &str| object.get(key).and_then(Json::as_str).map(str::to_string);

        let mut generations = BTreeMap::new();
        if let Some(raw) = object.get("generations") {
            let raw = raw
                .as_object()
                .ok_or_else(|| GenerationError::new("'generations' must be an object"))?;
            for (id, value) in raw {
                let entry = value
                    .as_object()
                    .ok_or_else(|| GenerationError::new("a generation must be an object"))?;
                let read = |key: &str| {
                    entry
                        .get(key)
                        .and_then(Json::as_str)
                        .map(str::to_string)
                        .ok_or_else(|| {
                            GenerationError::new(format!("generation '{id}' needs '{key}'"))
                        })
                };
                let status = read("status")?;
                generations.insert(
                    id.clone(),
                    Generation {
                        id: id.clone(),
                        label: read("label")?,
                        created_at: entry
                            .get("created_at")
                            .and_then(Json::as_f64)
                            .map(|value| value as u64)
                            .unwrap_or(0),
                        image: read("image")?,
                        digest: read("digest")?,
                        status: GenerationStatus::parse(&status).ok_or_else(|| {
                            GenerationError::new(format!("unknown status '{status}'"))
                        })?,
                        attempts: entry
                            .get("attempts")
                            .and_then(Json::as_f64)
                            .map(|value| value as u32)
                            .unwrap_or(0),
                    },
                );
            }
        }

        Ok(Self {
            active: optional("active"),
            last_good: optional("last_good"),
            candidate: optional("candidate"),
            boot_id: optional("boot_id").unwrap_or_else(|| "boot_0".into()),
            max_attempts: object
                .get("max_attempts")
                .and_then(Json::as_f64)
                .map(|value| value as u32)
                .unwrap_or(3),
            generations,
        })
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), GenerationError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, self.render())?;
        Ok(())
    }

    pub fn render(&self) -> String {
        self.to_json().render()
    }

    pub fn to_json(&self) -> Json {
        let generations = self
            .generations
            .iter()
            .map(|(id, generation)| {
                let mut entry = BTreeMap::new();
                entry.insert("id".to_string(), Json::String(generation.id.clone()));
                entry.insert("label".to_string(), Json::String(generation.label.clone()));
                entry.insert(
                    "created_at".to_string(),
                    Json::Number(generation.created_at as f64),
                );
                entry.insert("image".to_string(), Json::String(generation.image.clone()));
                entry.insert(
                    "digest".to_string(),
                    Json::String(generation.digest.clone()),
                );
                entry.insert(
                    "status".to_string(),
                    Json::String(generation.status.as_str().into()),
                );
                entry.insert(
                    "attempts".to_string(),
                    Json::Number(generation.attempts as f64),
                );
                (id.clone(), Json::Object(entry))
            })
            .collect();

        let mut object = BTreeMap::new();
        object.insert("version".to_string(), Json::Number(1.0));
        object.insert(
            "active".to_string(),
            self.active.clone().map(Json::String).unwrap_or(Json::Null),
        );
        object.insert(
            "last_good".to_string(),
            self.last_good
                .clone()
                .map(Json::String)
                .unwrap_or(Json::Null),
        );
        object.insert(
            "candidate".to_string(),
            self.candidate
                .clone()
                .map(Json::String)
                .unwrap_or(Json::Null),
        );
        object.insert("boot_id".to_string(), Json::String(self.boot_id.clone()));
        object.insert(
            "max_attempts".to_string(),
            Json::Number(self.max_attempts as f64),
        );
        object.insert("generations".to_string(), Json::Object(generations));
        Json::Object(object)
    }

    pub fn stage(
        &mut self,
        id: impl Into<String>,
        label: impl Into<String>,
        image: impl Into<String>,
        digest: impl Into<String>,
        now: u64,
    ) -> &Generation {
        let id = id.into();
        self.generations.insert(
            id.clone(),
            Generation {
                id: id.clone(),
                label: label.into(),
                created_at: now,
                image: image.into(),
                digest: digest.into(),
                status: GenerationStatus::Staged,
                attempts: 0,
            },
        );
        self.generations.get(&id).expect("just inserted")
    }

    pub fn set_candidate(&mut self, id: &str) -> Result<(), GenerationError> {
        let generation = self
            .generations
            .get_mut(id)
            .ok_or_else(|| GenerationError::new(format!("unknown generation '{id}'")))?;
        generation.status = GenerationStatus::Candidate;
        generation.attempts = 0;
        self.candidate = Some(id.to_string());
        Ok(())
    }

    pub fn generation(&self, id: &str) -> Option<&Generation> {
        self.generations.get(id)
    }

    pub fn active_generation(&self) -> Option<&Generation> {
        self.active
            .as_deref()
            .and_then(|id| self.generations.get(id))
    }

    pub fn candidate_generation(&self) -> Option<&Generation> {
        self.candidate
            .as_deref()
            .and_then(|id| self.generations.get(id))
    }

    /// The generation that should boot next.
    pub fn boot_target(&self) -> Option<&Generation> {
        self.candidate
            .as_deref()
            .and_then(|id| self.generations.get(id))
            .or_else(|| self.active_generation())
    }

    /// Count a boot attempt and decide whether the watchdog must intervene.
    pub fn record_boot(&mut self, now: u64) -> BootDecision {
        self.boot_id = format!("boot_{now:x}");
        let Some(target) = self.boot_target().map(|generation| generation.id.clone()) else {
            return BootDecision::Rescue {
                reason: "no generation to boot".into(),
            };
        };

        let (attempt, status) = {
            let generation = self.generations.get_mut(&target).expect("target exists");
            generation.attempts += 1;
            (generation.attempts, generation.status)
        };

        if attempt > self.max_attempts {
            if let Some(last_good) = self.last_good.clone() {
                if let Some(generation) = self.generations.get_mut(&target) {
                    generation.status = GenerationStatus::Failed;
                }
                self.candidate = None;
                self.active = Some(last_good.clone());
                if let Some(generation) = self.generations.get_mut(&last_good) {
                    generation.status = GenerationStatus::Active;
                    generation.attempts = 0;
                }
                return BootDecision::Rollback {
                    from: target,
                    to: last_good,
                    reason: format!("{attempt} failed boot attempts"),
                };
            }
            return BootDecision::Rescue {
                reason: format!(
                    "{target} failed {attempt} boots and no last-good generation exists"
                ),
            };
        }

        if status == GenerationStatus::Candidate {
            // Keep the candidate status until commit or rollback.
        }

        BootDecision::Proceed {
            generation: target,
            attempt,
        }
    }

    /// A successful health check clears the watchdog counter.
    pub fn mark_healthy(&mut self) -> Result<(), GenerationError> {
        let Some(target) = self.boot_target().map(|generation| generation.id.clone()) else {
            return Err(GenerationError::new("no generation to mark healthy"));
        };
        if let Some(generation) = self.generations.get_mut(&target) {
            generation.attempts = 0;
            if generation.status == GenerationStatus::Candidate {
                generation.status = GenerationStatus::Candidate;
            }
        }
        Ok(())
    }

    /// Promote the candidate to active and keep the previous active as
    /// last-good.
    pub fn commit(&mut self) -> Result<String, GenerationError> {
        let Some(candidate) = self.candidate.clone() else {
            return Err(GenerationError::new("no candidate to commit"));
        };
        let candidate_attempts = self
            .generations
            .get(&candidate)
            .map(|generation| generation.attempts)
            .unwrap_or(0);
        if candidate_attempts != 0 {
            return Err(GenerationError::new(
                "candidate has not passed a health check",
            ));
        }

        let previous = self.active.clone();
        if let Some(previous) = &previous {
            if let Some(generation) = self.generations.get_mut(previous) {
                generation.status = GenerationStatus::LastGood;
                generation.attempts = 0;
            }
            self.last_good = Some(previous.clone());
        } else if self.last_good.is_none() {
            // The first committed generation is also the first known-good
            // fallback.
            self.last_good = Some(candidate.clone());
        }
        if let Some(generation) = self.generations.get_mut(&candidate) {
            generation.status = GenerationStatus::Active;
            generation.attempts = 0;
        }
        self.active = Some(candidate.clone());
        self.candidate = None;
        Ok(candidate)
    }

    /// Fail the candidate and go back to last-good.
    pub fn rollback(&mut self, reason: impl Into<String>) -> BootDecision {
        let reason = reason.into();
        let from = self.candidate.clone().or_else(|| self.active.clone());
        if let Some(from) = &from {
            if let Some(generation) = self.generations.get_mut(from) {
                generation.status = GenerationStatus::Failed;
            }
        }
        self.candidate = None;

        if let Some(last_good) = self.last_good.clone() {
            self.active = Some(last_good.clone());
            if let Some(generation) = self.generations.get_mut(&last_good) {
                generation.status = GenerationStatus::Active;
                generation.attempts = 0;
            }
            return BootDecision::Rollback {
                from: from.unwrap_or_default(),
                to: last_good,
                reason,
            };
        }

        self.active = None;
        BootDecision::Rescue { reason }
    }

    pub fn retire(&mut self, id: &str) -> Result<(), GenerationError> {
        let generation = self
            .generations
            .get_mut(id)
            .ok_or_else(|| GenerationError::new(format!("unknown generation '{id}'")))?;
        generation.status = GenerationStatus::Retired;
        Ok(())
    }

    pub fn history(&self) -> Vec<&Generation> {
        let mut generations: Vec<&Generation> = self.generations.values().collect();
        generations.sort_by_key(|generation| generation.created_at);
        generations
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenerationError {
    pub message: String,
}

impl GenerationError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for GenerationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for GenerationError {}

impl From<std::io::Error> for GenerationError {
    fn from(error: std::io::Error) -> Self {
        Self::new(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table_with_base() -> GenerationTable {
        let mut table = GenerationTable::new();
        table.stage("gen_base", "base", "gen_base", "sha256:aa", 1_000);
        table.set_candidate("gen_base").unwrap();
        // First boot: no last-good yet.
        assert!(matches!(
            table.record_boot(1_001),
            BootDecision::Proceed { .. }
        ));
        table.mark_healthy().unwrap();
        table.commit().unwrap();
        table
    }

    #[test]
    fn commit_promotes_and_keeps_last_good() {
        let mut table = table_with_base();
        assert_eq!(table.active.as_deref(), Some("gen_base"));

        table.stage("gen_next", "next", "gen_next", "sha256:bb", 2_000);
        table.set_candidate("gen_next").unwrap();
        assert!(matches!(
            table.record_boot(2_001),
            BootDecision::Proceed { .. }
        ));
        table.mark_healthy().unwrap();
        let active = table.commit().unwrap();
        assert_eq!(active, "gen_next");
        assert_eq!(table.active.as_deref(), Some("gen_next"));
        assert_eq!(table.last_good.as_deref(), Some("gen_base"));
        assert_eq!(
            table.generation("gen_base").unwrap().status,
            GenerationStatus::LastGood
        );
    }

    #[test]
    fn watchdog_rolls_back_after_repeated_failures() {
        let mut table = table_with_base();
        table.stage("gen_bad", "bad", "gen_bad", "sha256:cc", 2_000);
        table.set_candidate("gen_bad").unwrap();

        for attempt in 0..table.max_attempts {
            assert!(matches!(
                table.record_boot(2_001 + attempt as u64),
                BootDecision::Proceed { .. }
            ));
        }
        // The next attempt trips the watchdog.
        match table.record_boot(3_000) {
            BootDecision::Rollback { from, to, .. } => {
                assert_eq!(from, "gen_bad");
                assert_eq!(to, "gen_base");
            }
            other => panic!("unexpected: {other:?}"),
        }
        assert_eq!(table.active.as_deref(), Some("gen_base"));
        assert_eq!(
            table.generation("gen_bad").unwrap().status,
            GenerationStatus::Failed
        );
    }

    #[test]
    fn rollback_without_last_good_requests_rescue() {
        let mut table = GenerationTable::new();
        table.stage("gen_only", "only", "gen_only", "sha256:dd", 1_000);
        table.set_candidate("gen_only").unwrap();
        assert!(matches!(
            table.record_boot(1_001),
            BootDecision::Proceed { .. }
        ));
        match table.rollback("health check failed") {
            BootDecision::Rescue { .. } => {}
            other => panic!("unexpected: {other:?}"),
        }
        assert_eq!(table.active, None);
        assert_eq!(
            table.generation("gen_only").unwrap().status,
            GenerationStatus::Failed
        );
    }

    #[test]
    fn table_round_trips() {
        let table = table_with_base();
        let parsed = GenerationTable::parse(&table.render()).expect("parse");
        assert_eq!(parsed, table);
    }
}
