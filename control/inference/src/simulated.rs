//! Deterministic simulated backend.
//!
//! It exists so the broker's batching, preemption, isolation, and quota logic
//! can be tested offline and on hosts without a GPU. It is not a model: it
//! emits a prompt-derived token stream, one token per `step`. A real backend
//! implements the same ABI.

use std::collections::BTreeMap;

use crate::backend::{
    BackendError, BackendStats, BeginResult, CacheHandle, GenerateRequest, InferenceBackend,
    ModelHandle, SessionHandle, TokenEvent,
};
use crate::model::ModelSpec;

pub struct SimulatedBackend {
    models: BTreeMap<u64, ModelSpec>,
    next_model: u64,
    sessions: BTreeMap<u64, SimSession>,
    next_session: u64,
    caches: BTreeMap<u64, CacheState>,
    tokens_emitted: u64,
    caches_zeroed: u64,
}

struct SimSession {
    model: u64,
    cache: u64,
    tokens: Vec<String>,
    index: usize,
}

struct CacheState {
    bytes: usize,
    zeroed: bool,
}

impl Default for SimulatedBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl SimulatedBackend {
    pub fn new() -> Self {
        Self {
            models: BTreeMap::new(),
            next_model: 1,
            sessions: BTreeMap::new(),
            next_session: 1,
            caches: BTreeMap::new(),
            tokens_emitted: 0,
            caches_zeroed: 0,
        }
    }

    pub fn is_cache_zeroed(&self, cache: CacheHandle) -> Option<bool> {
        self.caches.get(&cache.0).map(|state| state.zeroed)
    }

    pub fn cached_bytes(&self, cache: CacheHandle) -> Option<usize> {
        self.caches.get(&cache.0).map(|state| state.bytes)
    }
}

impl InferenceBackend for SimulatedBackend {
    fn name(&self) -> &str {
        "simulated"
    }

    fn load(&mut self, spec: &ModelSpec) -> Result<ModelHandle, BackendError> {
        let handle = self.next_model;
        self.next_model += 1;
        self.models.insert(handle, spec.clone());
        Ok(ModelHandle(handle))
    }

    fn unload(&mut self, handle: ModelHandle) -> Result<(), BackendError> {
        self.models
            .remove(&handle.0)
            .ok_or_else(|| BackendError::new(format!("unknown model handle {}", handle.0)))?;
        self.sessions.retain(|_, session| session.model != handle.0);
        Ok(())
    }

    fn begin(
        &mut self,
        model: ModelHandle,
        request: &GenerateRequest,
        cache: CacheHandle,
    ) -> Result<BeginResult, BackendError> {
        if !self.models.contains_key(&model.0) {
            return Err(BackendError::new(format!(
                "model handle {} is not loaded",
                model.0
            )));
        }
        if let Some(state) = self.caches.get(&cache.0) {
            if state.zeroed {
                return Err(BackendError::new(
                    "refusing to begin on a zeroed cache handle",
                ));
            }
        }

        let prompt_tokens = request.prompt.split_whitespace().count();
        let tokens = response_tokens(&request.prompt, request.max_tokens);
        let session = self.next_session;
        self.next_session += 1;
        self.caches.entry(cache.0).or_insert(CacheState {
            bytes: (prompt_tokens + request.max_tokens) * 4,
            zeroed: false,
        });
        self.sessions.insert(
            session,
            SimSession {
                model: model.0,
                cache: cache.0,
                tokens,
                index: 0,
            },
        );
        Ok(BeginResult {
            session: SessionHandle(session),
            prompt_tokens,
        })
    }

    fn step(&mut self, sessions: &[SessionHandle]) -> Vec<TokenEvent> {
        let mut events = Vec::new();
        for session in sessions {
            let Some(state) = self.sessions.get_mut(&session.0) else {
                events.push(TokenEvent::Failed {
                    session: *session,
                    reason: format!("unknown session {}", session.0),
                });
                continue;
            };
            if state.index < state.tokens.len() {
                let text = state.tokens[state.index].clone();
                state.index += 1;
                self.tokens_emitted += 1;
                events.push(TokenEvent::Token {
                    session: *session,
                    text,
                });
            } else {
                events.push(TokenEvent::Done {
                    session: *session,
                    tokens: state.index,
                });
            }
        }
        events
    }

    fn cancel(&mut self, session: SessionHandle) {
        self.sessions.remove(&session.0);
    }

    fn zero_cache(&mut self, cache: CacheHandle) -> Result<(), BackendError> {
        let state = self
            .caches
            .get_mut(&cache.0)
            .ok_or_else(|| BackendError::new(format!("unknown cache handle {}", cache.0)))?;
        state.zeroed = true;
        state.bytes = 0;
        self.caches_zeroed += 1;
        Ok(())
    }

    fn stats(&self) -> BackendStats {
        BackendStats {
            loaded_models: self.models.len(),
            active_sessions: self.sessions.len(),
            tokens_emitted: self.tokens_emitted,
            caches_zeroed: self.caches_zeroed,
        }
    }
}

/// A deterministic token stream of exactly `max_tokens` tokens, drawn from the
/// prompt and then filler. Two runs with the same prompt always match.
fn response_tokens(prompt: &str, max_tokens: usize) -> Vec<String> {
    let words: Vec<&str> = prompt.split_whitespace().collect();
    let mut tokens = Vec::with_capacity(max_tokens);
    for index in 0..max_tokens {
        if index < words.len() {
            tokens.push(words[index].to_string());
        } else {
            let filler = index - words.len();
            tokens.push(format!("sim{filler}"));
        }
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::Priority;

    fn request(prompt: &str, max_tokens: usize) -> GenerateRequest {
        GenerateRequest {
            principal: "user:ada".into(),
            session: "s1".into(),
            model: "sim/tiny".into(),
            prompt: prompt.into(),
            priority: Priority::Normal,
            max_tokens,
            deadline: None,
        }
    }

    #[test]
    fn emits_a_deterministic_stream() {
        let mut backend = SimulatedBackend::new();
        let model = backend.load(&ModelSpec::example()).expect("load");
        let first = backend
            .begin(model, &request("hello world", 4), CacheHandle(1))
            .expect("begin");
        let second = backend
            .begin(model, &request("hello world", 4), CacheHandle(2))
            .expect("begin");

        let mut stream = Vec::new();
        for _ in 0..5 {
            for event in backend.step(&[first.session, second.session]) {
                if let TokenEvent::Token { text, .. } = event {
                    stream.push(text);
                }
            }
        }
        assert_eq!(
            &stream[..8],
            &["hello", "hello", "world", "world", "sim0", "sim0", "sim1", "sim1"]
        );
    }

    #[test]
    fn zeroing_is_observable() {
        let mut backend = SimulatedBackend::new();
        let model = backend.load(&ModelSpec::example()).expect("load");
        backend
            .begin(model, &request("x", 2), CacheHandle(7))
            .expect("begin");
        backend.zero_cache(CacheHandle(7)).expect("zero");
        assert_eq!(backend.is_cache_zeroed(CacheHandle(7)), Some(true));
        assert_eq!(backend.cached_bytes(CacheHandle(7)), Some(0));
        assert_eq!(backend.stats().caches_zeroed, 1);
    }
}
