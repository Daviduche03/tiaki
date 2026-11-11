//! Queue policy: priority classes and per-principal fairness.
//!
//! The scheduler is deliberately small. The broker owns the active set and
//! preemption; this module owns "who is next" so it can be tested alone.

use std::collections::{BTreeMap, VecDeque};

use crate::backend::{CacheHandle, GenerateRequest, Priority, SessionHandle};

pub type RequestId = u64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResumeState {
    pub session: SessionHandle,
    pub cache: CacheHandle,
    pub tokens: usize,
    pub preemptions: u32,
    pub first_token_step: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueuedRequest {
    pub id: RequestId,
    pub request: GenerateRequest,
    pub resume: Option<ResumeState>,
}

pub struct Scheduler {
    next_id: RequestId,
    queues: BTreeMap<Priority, VecDeque<QueuedRequest>>,
    /// Tokens started per principal, used for fair-share selection.
    served: BTreeMap<String, u64>,
    submitted: u64,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler {
    pub fn new() -> Self {
        let mut queues = BTreeMap::new();
        queues.insert(Priority::Interactive, VecDeque::new());
        queues.insert(Priority::Normal, VecDeque::new());
        queues.insert(Priority::Batch, VecDeque::new());
        Self {
            next_id: 1,
            queues,
            served: BTreeMap::new(),
            submitted: 0,
        }
    }

    pub fn submit(&mut self, request: GenerateRequest) -> RequestId {
        let id = self.next_id;
        self.next_id += 1;
        self.submitted += 1;
        let priority = request.priority;
        self.queues
            .entry(priority)
            .or_default()
            .push_back(QueuedRequest {
                id,
                request,
                resume: None,
            });
        id
    }

    /// Pop the next request: highest priority first, then the principal that
    /// has been served least within that class (FIFO tie-break).
    pub fn pop_next(&mut self) -> Option<QueuedRequest> {
        for priority in [Priority::Interactive, Priority::Normal, Priority::Batch] {
            let Some(queue) = self.queues.get_mut(&priority) else {
                continue;
            };
            if queue.is_empty() {
                continue;
            }
            // Resumed work goes first: it already holds backend state.
            if queue
                .front()
                .map(|queued| queued.resume.is_some())
                .unwrap_or(false)
            {
                let queued = queue.pop_front()?;
                *self
                    .served
                    .entry(queued.request.principal.clone())
                    .or_insert(0) += 1;
                return Some(queued);
            }
            let mut best: Option<(usize, u64)> = None;
            for (index, queued) in queue.iter().enumerate() {
                let served = *self.served.get(&queued.request.principal).unwrap_or(&0);
                if best.is_none() || served < best.unwrap().1 {
                    best = Some((index, served));
                }
            }
            let (index, _) = best?;
            let queued = queue.remove(index)?;
            *self
                .served
                .entry(queued.request.principal.clone())
                .or_insert(0) += 1;
            return Some(queued);
        }
        None
    }

    /// Put a preempted request back at the front of its class so it resumes
    /// before new work.
    pub fn requeue_front(&mut self, queued: QueuedRequest) {
        let priority = queued.request.priority;
        self.queues.entry(priority).or_default().push_front(queued);
    }

    pub fn has_priority(&self, priority: Priority) -> bool {
        self.queues
            .get(&priority)
            .map(|queue| !queue.is_empty())
            .unwrap_or(false)
    }

    pub fn queued_for(&self, principal: &str) -> usize {
        self.queues
            .values()
            .flat_map(|queue| queue.iter())
            .filter(|queued| queued.request.principal == principal)
            .count()
    }

    pub fn queued_for_model(&self, model: &str) -> usize {
        self.queues
            .values()
            .flat_map(|queue| queue.iter())
            .filter(|queued| queued.request.model == model)
            .count()
    }

    pub fn len(&self) -> usize {
        self.queues.values().map(VecDeque::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn submitted(&self) -> u64 {
        self.submitted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(principal: &str, priority: Priority, prompt: &str) -> GenerateRequest {
        let mut request = GenerateRequest::new(principal, "sim/tiny", prompt);
        request.priority = priority;
        request
    }

    #[test]
    fn interactive_comes_first() {
        let mut scheduler = Scheduler::new();
        scheduler.submit(request("user:ada", Priority::Batch, "b"));
        scheduler.submit(request("user:bob", Priority::Interactive, "i"));
        scheduler.submit(request("user:carol", Priority::Normal, "n"));

        assert_eq!(
            scheduler.pop_next().unwrap().request.priority,
            Priority::Interactive
        );
        assert_eq!(
            scheduler.pop_next().unwrap().request.priority,
            Priority::Normal
        );
        assert_eq!(
            scheduler.pop_next().unwrap().request.priority,
            Priority::Batch
        );
    }

    #[test]
    fn fair_share_alternates_principals() {
        let mut scheduler = Scheduler::new();
        for _ in 0..3 {
            scheduler.submit(request("user:ada", Priority::Normal, "a"));
            scheduler.submit(request("user:bob", Priority::Normal, "b"));
        }

        let mut order = Vec::new();
        while let Some(queued) = scheduler.pop_next() {
            order.push(queued.request.principal);
        }
        assert_eq!(
            order,
            vec!["user:ada", "user:bob", "user:ada", "user:bob", "user:ada", "user:bob"]
        );
    }

    #[test]
    fn preempted_work_resumes_before_peers() {
        let mut scheduler = Scheduler::new();
        let first = scheduler.submit(request("user:ada", Priority::Batch, "first"));
        scheduler.submit(request("user:bob", Priority::Batch, "second"));

        let mut queued = scheduler.pop_next().unwrap();
        assert_eq!(queued.id, first);
        queued.resume = Some(ResumeState {
            session: SessionHandle(9),
            cache: CacheHandle(9),
            tokens: 3,
            preemptions: 1,
            first_token_step: Some(2),
        });
        scheduler.requeue_front(queued);

        let resumed = scheduler.pop_next().unwrap();
        assert_eq!(resumed.id, first);
        assert!(resumed.resume.is_some());
    }
}
