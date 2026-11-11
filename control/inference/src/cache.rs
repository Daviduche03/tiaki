//! KV-cache ownership.
//!
//! A cache belongs to exactly one principal, session, and model. The broker
//! allocates handles and zeroes them on completion or handoff. Handles are
//! never reused, so one principal can never receive another's state.

use std::collections::BTreeMap;

use crate::backend::{BackendError, CacheHandle, InferenceBackend};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheEntry {
    pub handle: CacheHandle,
    pub principal: String,
    pub session: String,
    pub model: String,
    pub bytes: usize,
    pub zeroed: bool,
}

pub struct CacheTable {
    next: u64,
    entries: BTreeMap<CacheHandle, CacheEntry>,
    zeroed_total: u64,
}

impl Default for CacheTable {
    fn default() -> Self {
        Self::new()
    }
}

impl CacheTable {
    pub fn new() -> Self {
        Self {
            next: 1,
            entries: BTreeMap::new(),
            zeroed_total: 0,
        }
    }

    /// Allocate a fresh handle. Never reuses a previous owner's handle.
    pub fn acquire(&mut self, principal: &str, session: &str, model: &str) -> CacheHandle {
        let handle = CacheHandle(self.next);
        self.next += 1;
        self.entries.insert(
            handle,
            CacheEntry {
                handle,
                principal: principal.to_string(),
                session: session.to_string(),
                model: model.to_string(),
                bytes: 0,
                zeroed: false,
            },
        );
        handle
    }

    pub fn add_bytes(&mut self, handle: CacheHandle, bytes: usize) {
        if let Some(entry) = self.entries.get_mut(&handle) {
            entry.bytes = entry.bytes.saturating_add(bytes);
        }
    }

    /// Drop a handle without calling the backend. Used when `begin` fails
    /// before the backend ever registered the cache.
    pub fn discard(&mut self, handle: CacheHandle) {
        self.entries.remove(&handle);
    }

    pub fn owner_of(&self, handle: CacheHandle) -> Option<&str> {
        self.entries
            .get(&handle)
            .map(|entry| entry.principal.as_str())
    }

    /// Zero and drop a cache. Called when a request completes.
    pub fn release(
        &mut self,
        backend: &mut dyn InferenceBackend,
        handle: CacheHandle,
    ) -> Result<(), BackendError> {
        if !self.entries.contains_key(&handle) {
            return Ok(());
        }
        backend.zero_cache(handle)?;
        if let Some(entry) = self.entries.get_mut(&handle) {
            entry.zeroed = true;
            entry.bytes = 0;
        }
        self.entries.remove(&handle);
        self.zeroed_total += 1;
        Ok(())
    }

    /// Zero every cache owned by a principal. Used on session end or when a
    /// principal hands the device to another user.
    pub fn handoff(
        &mut self,
        backend: &mut dyn InferenceBackend,
        principal: &str,
    ) -> Result<Vec<CacheHandle>, BackendError> {
        let handles: Vec<CacheHandle> = self
            .entries
            .values()
            .filter(|entry| entry.principal == principal)
            .map(|entry| entry.handle)
            .collect();
        for handle in &handles {
            self.release(backend, *handle)?;
        }
        Ok(handles)
    }

    pub fn entries(&self) -> Vec<&CacheEntry> {
        self.entries.values().collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn zeroed_total(&self) -> u64 {
        self.zeroed_total
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{GenerateRequest, Priority};
    use crate::model::ModelSpec;
    use crate::simulated::SimulatedBackend;

    fn request(principal: &str, session: &str) -> GenerateRequest {
        GenerateRequest {
            principal: principal.into(),
            session: session.into(),
            model: "sim/tiny".into(),
            prompt: "cache test".into(),
            priority: Priority::Normal,
            max_tokens: 4,
            deadline: None,
        }
    }

    #[test]
    fn handles_are_never_shared_between_principals() {
        let mut table = CacheTable::new();
        let ada = table.acquire("user:ada", "s1", "sim/tiny");
        let bob = table.acquire("user:bob", "s1", "sim/tiny");
        assert_ne!(ada, bob);
        assert_eq!(table.owner_of(ada), Some("user:ada"));
        assert_eq!(table.owner_of(bob), Some("user:bob"));
    }

    #[test]
    fn release_zeroes_the_backend_cache() {
        let mut backend = SimulatedBackend::new();
        let model = backend.load(&ModelSpec::example()).expect("load");
        let mut table = CacheTable::new();
        let cache = table.acquire("user:ada", "s1", "sim/tiny");
        backend
            .begin(model, &request("user:ada", "s1"), cache)
            .expect("begin");

        table.release(&mut backend, cache).expect("release");
        assert_eq!(backend.is_cache_zeroed(cache), Some(true));
        assert!(table.is_empty());
        assert_eq!(table.zeroed_total(), 1);
    }

    #[test]
    fn handoff_only_touches_the_principal() {
        let mut backend = SimulatedBackend::new();
        let model = backend.load(&ModelSpec::example()).expect("load");
        let mut table = CacheTable::new();
        let ada = table.acquire("user:ada", "s1", "sim/tiny");
        let bob = table.acquire("user:bob", "s1", "sim/tiny");
        backend
            .begin(model, &request("user:ada", "s1"), ada)
            .expect("begin");
        backend
            .begin(model, &request("user:bob", "s1"), bob)
            .expect("begin");

        let zeroed = table.handoff(&mut backend, "user:ada").expect("handoff");
        assert_eq!(zeroed, vec![ada]);
        assert_eq!(backend.is_cache_zeroed(ada), Some(true));
        assert_eq!(backend.is_cache_zeroed(bob), Some(false));
        assert_eq!(table.len(), 1);
    }
}
