//! The backend ABI.
//!
//! The broker owns policy and scheduling; a backend owns compute. The ABI is
//! step-driven so the broker can batch across principals and preempt at token
//! boundaries. A blocking `generate()` cannot be preempted.

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModelHandle(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SessionHandle(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CacheHandle(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Batch,
    Normal,
    Interactive,
}

impl Priority {
    pub fn as_str(self) -> &'static str {
        match self {
            Priority::Batch => "batch",
            Priority::Normal => "normal",
            Priority::Interactive => "interactive",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "batch" => Some(Priority::Batch),
            "normal" => Some(Priority::Normal),
            "interactive" => Some(Priority::Interactive),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenerateRequest {
    /// The principal whose quota and cache scope this request uses.
    pub principal: String,
    /// A session identifier within the principal, for cache reuse.
    pub session: String,
    pub model: String,
    pub prompt: String,
    pub priority: Priority,
    pub max_tokens: usize,
    pub deadline: Option<u64>,
}

impl GenerateRequest {
    pub fn new(
        principal: impl Into<String>,
        model: impl Into<String>,
        prompt: impl Into<String>,
    ) -> Self {
        Self {
            principal: principal.into(),
            session: "default".into(),
            model: model.into(),
            prompt: prompt.into(),
            priority: Priority::Normal,
            max_tokens: 128,
            deadline: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BeginResult {
    pub session: SessionHandle,
    /// Prompt tokens consumed by prefill; used for accounting.
    pub prompt_tokens: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokenEvent {
    Token {
        session: SessionHandle,
        text: String,
    },
    Done {
        session: SessionHandle,
        tokens: usize,
    },
    Failed {
        session: SessionHandle,
        reason: String,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BackendStats {
    pub loaded_models: usize,
    pub active_sessions: usize,
    pub tokens_emitted: u64,
    pub caches_zeroed: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendError {
    pub message: String,
}

impl BackendError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for BackendError {}

pub trait InferenceBackend {
    fn name(&self) -> &str;

    fn load(&mut self, spec: &crate::model::ModelSpec) -> Result<ModelHandle, BackendError>;
    fn unload(&mut self, handle: ModelHandle) -> Result<(), BackendError>;

    /// Prefill the prompt into a KV cache and create a session.
    fn begin(
        &mut self,
        model: ModelHandle,
        request: &GenerateRequest,
        cache: CacheHandle,
    ) -> Result<BeginResult, BackendError>;

    /// Advance the given sessions by one token each. The broker decides the
    /// batch; the backend does not schedule.
    fn step(&mut self, sessions: &[SessionHandle]) -> Vec<TokenEvent>;

    /// Drop a session. Called on completion or failure, never on preemption:
    /// preempted sessions stay alive with their cache intact.
    fn cancel(&mut self, session: SessionHandle);

    /// Zero and invalidate a cache. This is a security operation, not an
    /// optimization: a cache must never cross principals.
    fn zero_cache(&mut self, cache: CacheHandle) -> Result<(), BackendError>;

    fn stats(&self) -> BackendStats;
}
