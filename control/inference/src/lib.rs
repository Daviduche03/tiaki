//! Inference broker: model registry, step-driven backend ABI, continuous
//! batching, preemption, per-principal KV isolation, and quotas.
//!
//! See TIAKI.md §12 and `docs/specs/inference.md`.

#![allow(dead_code)]

pub mod backend;
pub mod broker;
pub mod cache;
pub mod model;
pub mod quota;
pub mod scheduler;
pub mod simulated;
pub mod validator;

pub use backend::{
    BackendError, BackendStats, BeginResult, CacheHandle, GenerateRequest, InferenceBackend,
    ModelHandle, Priority, SessionHandle, TokenEvent,
};
pub use broker::{
    Broker, BrokerConfig, BrokerError, BrokerEvent, BrokerStats, OutcomeStatus, RequestOutcome,
};
pub use cache::{CacheEntry, CacheTable};
pub use model::{is_model_path, model_path, ModelError, ModelSpec};
pub use quota::{Quota, QuotaError, UsageBook};
pub use scheduler::{QueuedRequest, RequestId, ResumeState, Scheduler};
pub use simulated::SimulatedBackend;
pub use validator::ModelRegistryValidator;
