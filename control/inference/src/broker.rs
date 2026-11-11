//! The inference broker: admission, batching, preemption, isolation, quotas.
//!
//! The broker is synchronous and step-driven so the policy is deterministic
//! and testable. `step()` advances every active session by one token; `run()`
//! loops until the queues drain.

use std::collections::BTreeMap;
use std::fmt;

use crate::backend::{
    BackendError, BackendStats, CacheHandle, GenerateRequest, InferenceBackend, ModelHandle,
    Priority, SessionHandle, TokenEvent,
};
use crate::cache::{CacheEntry, CacheTable};
use crate::model::ModelSpec;
use crate::quota::{Quota, QuotaError, UsageBook};
use crate::scheduler::{QueuedRequest, RequestId, ResumeState, Scheduler};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BrokerConfig {
    pub max_active: usize,
    pub preempt_for_interactive: bool,
}

impl Default for BrokerConfig {
    fn default() -> Self {
        Self {
            max_active: 4,
            preempt_for_interactive: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutcomeStatus {
    Completed,
    Failed,
    Denied,
}

impl OutcomeStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            OutcomeStatus::Completed => "completed",
            OutcomeStatus::Failed => "failed",
            OutcomeStatus::Denied => "denied",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestOutcome {
    pub id: RequestId,
    pub principal: String,
    pub model: String,
    pub priority: Priority,
    pub tokens: usize,
    pub preemptions: u32,
    pub first_token_step: Option<u64>,
    pub status: OutcomeStatus,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrokerEvent {
    Token {
        request: RequestId,
        text: String,
    },
    Completed {
        request: RequestId,
        tokens: usize,
        preemptions: u32,
    },
    Preempted {
        request: RequestId,
        principal: String,
        tokens: usize,
    },
    Failed {
        request: RequestId,
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrokerStats {
    pub loaded_models: usize,
    pub queued: usize,
    pub active: usize,
    pub completed: usize,
    pub preemptions: u32,
    pub tokens: u64,
    pub caches: usize,
    pub backend: BackendStats,
}

#[derive(Debug)]
pub enum BrokerError {
    ModelNotLoaded(String),
    ModelInUse(String),
    Backend(BackendError),
    Quota(QuotaError),
    UnknownRequest(RequestId),
    Runaway,
}

impl fmt::Display for BrokerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BrokerError::ModelNotLoaded(model) => write!(f, "model '{model}' is not loaded"),
            BrokerError::ModelInUse(model) => write!(f, "model '{model}' is in use"),
            BrokerError::Backend(error) => write!(f, "backend error: {error}"),
            BrokerError::Quota(error) => write!(f, "quota: {error}"),
            BrokerError::UnknownRequest(id) => write!(f, "unknown request {id}"),
            BrokerError::Runaway => write!(f, "scheduler did not converge"),
        }
    }
}

impl std::error::Error for BrokerError {}

impl From<BackendError> for BrokerError {
    fn from(error: BackendError) -> Self {
        BrokerError::Backend(error)
    }
}

impl From<QuotaError> for BrokerError {
    fn from(error: QuotaError) -> Self {
        BrokerError::Quota(error)
    }
}

struct ActiveSession {
    id: RequestId,
    request: GenerateRequest,
    session: SessionHandle,
    cache: CacheHandle,
    tokens: usize,
    preemptions: u32,
    first_token_step: Option<u64>,
}

pub struct Broker {
    backend: Box<dyn InferenceBackend>,
    config: BrokerConfig,
    scheduler: Scheduler,
    caches: CacheTable,
    loaded: BTreeMap<String, ModelHandle>,
    active: Vec<ActiveSession>,
    usage: UsageBook,
    outcomes: BTreeMap<RequestId, RequestOutcome>,
    order: Vec<RequestId>,
    steps: u64,
}

impl Broker {
    pub fn new(backend: Box<dyn InferenceBackend>, config: BrokerConfig) -> Self {
        Self {
            backend,
            config,
            scheduler: Scheduler::new(),
            caches: CacheTable::new(),
            loaded: BTreeMap::new(),
            active: Vec::new(),
            usage: UsageBook::new(),
            outcomes: BTreeMap::new(),
            order: Vec::new(),
            steps: 0,
        }
    }

    pub fn backend_name(&self) -> &str {
        self.backend.name()
    }

    pub fn load(&mut self, spec: &ModelSpec) -> Result<ModelHandle, BrokerError> {
        if let Some(handle) = self.loaded.get(&spec.id) {
            return Ok(*handle);
        }
        let handle = self.backend.load(spec)?;
        self.loaded.insert(spec.id.clone(), handle);
        Ok(handle)
    }

    pub fn unload(&mut self, model: &str) -> Result<(), BrokerError> {
        let handle = *self
            .loaded
            .get(model)
            .ok_or_else(|| BrokerError::ModelNotLoaded(model.to_string()))?;
        let in_use = self
            .active
            .iter()
            .any(|active| active.request.model == model)
            || self.scheduler.queued_for_model(model) > 0;
        if in_use {
            return Err(BrokerError::ModelInUse(model.to_string()));
        }
        self.backend.unload(handle)?;
        self.loaded.remove(model);
        Ok(())
    }

    pub fn submit(&mut self, request: GenerateRequest, now: u64) -> Result<RequestId, BrokerError> {
        if !self.loaded.contains_key(&request.model) {
            return Err(BrokerError::ModelNotLoaded(request.model.clone()));
        }
        let quota = Quota::for_principal(&request.principal);
        let outstanding =
            self.active_for(&request.principal) + self.scheduler.queued_for(&request.principal);
        if outstanding >= quota.max_concurrent {
            return Err(BrokerError::Quota(QuotaError::new(format!(
                "{} already has {outstanding} in-flight requests (max {})",
                request.principal, quota.max_concurrent
            ))));
        }
        self.usage
            .try_admit(&request.principal, now, request.max_tokens, quota)?;
        Ok(self.scheduler.submit(request))
    }

    /// Advance one token for every active session.
    pub fn step(&mut self, now: u64) -> Result<Vec<BrokerEvent>, BrokerError> {
        self.steps += 1;
        let step = self.steps;
        let mut events = Vec::new();

        // Interactive work preempts batch/normal work at a token boundary.
        while self.config.preempt_for_interactive
            && self.scheduler.has_priority(Priority::Interactive)
            && self.active.len() >= self.config.max_active
        {
            let Some(index) = self.preemption_victim() else {
                break;
            };
            events.push(self.preempt(index));
        }

        while self.active.len() < self.config.max_active {
            let Some(queued) = self.scheduler.pop_next() else {
                break;
            };
            self.activate(queued)?;
        }

        if self.active.is_empty() {
            return Ok(events);
        }

        let sessions: Vec<SessionHandle> =
            self.active.iter().map(|active| active.session).collect();
        let token_events = self.backend.step(&sessions);

        for event in token_events {
            match event {
                TokenEvent::Token { session, text } => {
                    if let Some(active) = self.active.iter_mut().find(|a| a.session == session) {
                        active.tokens += 1;
                        if active.first_token_step.is_none() {
                            active.first_token_step = Some(step);
                        }
                        events.push(BrokerEvent::Token {
                            request: active.id,
                            text,
                        });
                    }
                }
                TokenEvent::Done { session, tokens } => {
                    if let Some(index) = self.active.iter().position(|a| a.session == session) {
                        let active = self.active.remove(index);
                        self.backend.cancel(active.session);
                        self.caches.release(&mut *self.backend, active.cache)?;
                        self.usage.charge(&active.request.principal, now, tokens);
                        let outcome = RequestOutcome {
                            id: active.id,
                            principal: active.request.principal.clone(),
                            model: active.request.model.clone(),
                            priority: active.request.priority,
                            tokens,
                            preemptions: active.preemptions,
                            first_token_step: active.first_token_step,
                            status: OutcomeStatus::Completed,
                            reason: String::new(),
                        };
                        events.push(BrokerEvent::Completed {
                            request: active.id,
                            tokens,
                            preemptions: active.preemptions,
                        });
                        self.record(outcome);
                    }
                }
                TokenEvent::Failed { session, reason } => {
                    if let Some(index) = self.active.iter().position(|a| a.session == session) {
                        let active = self.active.remove(index);
                        self.backend.cancel(active.session);
                        self.caches.release(&mut *self.backend, active.cache)?;
                        let outcome = RequestOutcome {
                            id: active.id,
                            principal: active.request.principal.clone(),
                            model: active.request.model.clone(),
                            priority: active.request.priority,
                            tokens: active.tokens,
                            preemptions: active.preemptions,
                            first_token_step: active.first_token_step,
                            status: OutcomeStatus::Failed,
                            reason: reason.clone(),
                        };
                        events.push(BrokerEvent::Failed {
                            request: active.id,
                            reason,
                        });
                        self.record(outcome);
                    }
                }
            }
        }

        Ok(events)
    }

    /// Run until every queued and active request finishes.
    pub fn run(&mut self, now: u64) -> Result<Vec<BrokerEvent>, BrokerError> {
        let mut events = Vec::new();
        let mut guard = 0u64;
        while !self.scheduler.is_empty() || !self.active.is_empty() {
            let step_now = now.saturating_add(self.steps);
            events.extend(self.step(step_now)?);
            guard += 1;
            if guard > 1_000_000 {
                return Err(BrokerError::Runaway);
            }
        }
        Ok(events)
    }

    /// Convenience for one request: submit, run, and return its text.
    pub fn generate_one(
        &mut self,
        request: GenerateRequest,
        now: u64,
    ) -> Result<(String, RequestOutcome), BrokerError> {
        let id = self.submit(request, now)?;
        let events = self.run(now)?;
        let text = events
            .iter()
            .filter_map(|event| match event {
                BrokerEvent::Token { request, text } if *request == id => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ");
        let outcome = self
            .outcome(id)
            .cloned()
            .ok_or(BrokerError::UnknownRequest(id))?;
        Ok((text, outcome))
    }

    pub fn outcome(&self, id: RequestId) -> Option<&RequestOutcome> {
        self.outcomes.get(&id)
    }

    pub fn outcomes(&self) -> Vec<&RequestOutcome> {
        self.order
            .iter()
            .filter_map(|id| self.outcomes.get(id))
            .collect()
    }

    pub fn cache_entries(&self) -> Vec<&CacheEntry> {
        self.caches.entries()
    }

    pub fn stats(&self) -> BrokerStats {
        BrokerStats {
            loaded_models: self.loaded.len(),
            queued: self.scheduler.len(),
            active: self.active.len(),
            completed: self
                .outcomes
                .values()
                .filter(|outcome| outcome.status == OutcomeStatus::Completed)
                .count(),
            preemptions: self
                .outcomes
                .values()
                .map(|outcome| outcome.preemptions)
                .sum(),
            tokens: self.backend.stats().tokens_emitted,
            caches: self.caches.len(),
            backend: self.backend.stats(),
        }
    }

    fn active_for(&self, principal: &str) -> usize {
        self.active
            .iter()
            .filter(|active| active.request.principal == principal)
            .count()
    }

    fn activate(&mut self, queued: QueuedRequest) -> Result<(), BrokerError> {
        let QueuedRequest {
            id,
            request,
            resume,
        } = queued;
        if let Some(resume) = resume {
            self.active.push(ActiveSession {
                id,
                request,
                session: resume.session,
                cache: resume.cache,
                tokens: resume.tokens,
                preemptions: resume.preemptions,
                first_token_step: resume.first_token_step,
            });
            return Ok(());
        }

        let handle = *self
            .loaded
            .get(&request.model)
            .ok_or_else(|| BrokerError::ModelNotLoaded(request.model.clone()))?;
        let cache = self
            .caches
            .acquire(&request.principal, &request.session, &request.model);
        let begin = match self.backend.begin(handle, &request, cache) {
            Ok(begin) => begin,
            Err(error) => {
                self.caches.discard(cache);
                return Err(BrokerError::Backend(error));
            }
        };
        self.caches.add_bytes(cache, begin.prompt_tokens * 4);
        self.active.push(ActiveSession {
            id,
            request,
            session: begin.session,
            cache,
            tokens: 0,
            preemptions: 0,
            first_token_step: None,
        });
        Ok(())
    }

    fn preemption_victim(&self) -> Option<usize> {
        self.active
            .iter()
            .enumerate()
            .filter(|(_, active)| active.request.priority != Priority::Interactive)
            .min_by_key(|(_, active)| active.request.priority)
            .map(|(index, _)| index)
    }

    fn preempt(&mut self, index: usize) -> BrokerEvent {
        let active = self.active.remove(index);
        let event = BrokerEvent::Preempted {
            request: active.id,
            principal: active.request.principal.clone(),
            tokens: active.tokens,
        };
        self.scheduler.requeue_front(QueuedRequest {
            id: active.id,
            request: active.request,
            resume: Some(ResumeState {
                session: active.session,
                cache: active.cache,
                tokens: active.tokens,
                preemptions: active.preemptions + 1,
                first_token_step: active.first_token_step,
            }),
        });
        event
    }

    fn record(&mut self, outcome: RequestOutcome) {
        if !self.outcomes.contains_key(&outcome.id) {
            self.order.push(outcome.id);
        }
        self.outcomes.insert(outcome.id, outcome);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulated::SimulatedBackend;

    fn broker(max_active: usize) -> Broker {
        let mut broker = Broker::new(
            Box::new(SimulatedBackend::new()),
            BrokerConfig {
                max_active,
                preempt_for_interactive: true,
            },
        );
        broker.load(&ModelSpec::example()).expect("load");
        broker
    }

    fn request(principal: &str, priority: Priority, max_tokens: usize) -> GenerateRequest {
        let mut request = GenerateRequest::new(principal, "sim/tiny", "hello world from the test");
        request.priority = priority;
        request.max_tokens = max_tokens;
        request
    }

    #[test]
    fn two_principals_share_one_batch() {
        let mut broker = broker(2);
        let ada = broker
            .submit(request("user:ada", Priority::Normal, 8), 0)
            .expect("submit");
        let bob = broker
            .submit(request("user:bob", Priority::Normal, 8), 0)
            .expect("submit");

        let events = broker.step(0).expect("step");
        let tokens: Vec<RequestId> = events
            .iter()
            .filter_map(|event| match event {
                BrokerEvent::Token { request, .. } => Some(*request),
                _ => None,
            })
            .collect();
        assert!(tokens.contains(&ada));
        assert!(tokens.contains(&bob));

        broker.run(0).expect("run");
        assert_eq!(
            broker.outcome(ada).unwrap().status,
            OutcomeStatus::Completed
        );
        assert_eq!(
            broker.outcome(bob).unwrap().status,
            OutcomeStatus::Completed
        );
    }

    #[test]
    fn interactive_preempts_batch_and_batch_resumes() {
        let mut broker = broker(1);
        let batch = broker
            .submit(request("user:ada", Priority::Batch, 40), 0)
            .expect("batch");

        // Let the batch make progress.
        broker.step(0).expect("step");
        broker.step(0).expect("step");

        let interactive = broker
            .submit(request("user:carol", Priority::Interactive, 6), 0)
            .expect("interactive");

        let events = broker.step(0).expect("step");
        assert!(events.iter().any(|event| matches!(
            event,
            BrokerEvent::Preempted { request, .. } if *request == batch
        )));

        broker.run(0).expect("run");
        let batch_outcome = broker.outcome(batch).expect("batch outcome");
        let interactive_outcome = broker.outcome(interactive).expect("interactive outcome");
        assert_eq!(interactive_outcome.status, OutcomeStatus::Completed);
        assert_eq!(batch_outcome.status, OutcomeStatus::Completed);
        assert!(batch_outcome.preemptions >= 1);
        assert_eq!(batch_outcome.tokens, 40);
    }

    #[test]
    fn caches_are_zeroed_and_never_shared() {
        let mut broker = broker(2);
        broker
            .submit(request("user:ada", Priority::Normal, 6), 0)
            .expect("ada");
        broker
            .submit(request("user:bob", Priority::Normal, 6), 0)
            .expect("bob");
        broker.run(0).expect("run");

        assert!(broker.cache_entries().is_empty());
        let stats = broker.stats();
        assert_eq!(stats.caches, 0);
        assert!(stats.backend.caches_zeroed >= 2);
    }

    #[test]
    fn quota_denies_excess_concurrency() {
        let mut broker = broker(4);
        // The default per-principal limit is two in-flight requests.
        broker
            .submit(request("user:ada", Priority::Batch, 4), 0)
            .expect("first");
        broker
            .submit(request("user:ada", Priority::Batch, 4), 0)
            .expect("second");
        let third = broker.submit(request("user:ada", Priority::Batch, 4), 0);
        assert!(matches!(third, Err(BrokerError::Quota(_))));
    }

    #[test]
    fn unloaded_models_are_rejected() {
        let mut broker = Broker::new(Box::new(SimulatedBackend::new()), BrokerConfig::default());
        let error = broker
            .submit(request("user:ada", Priority::Normal, 4), 0)
            .expect_err("not loaded");
        assert!(matches!(error, BrokerError::ModelNotLoaded(_)));
    }
}
