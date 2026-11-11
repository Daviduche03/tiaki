//! Multi-agent supervision: runtime registry, admission, and CPU-share scheduling.
//!
//! The control plane supports several agent runtimes (opencode, codex, custom)
//! speaking `tiaki-abi/1`. When an agent session runs, it is automatically
//! boosted above ordinary apps: shares move from small processes to the agent
//! pool. Local inference always outranks agents: an inference burst forces
//! agents down to a floor until it clears. Background work is throttled but
//! never starved.
//!
//! Everything here is deterministic and offline: the allocator is a pure
//! function of counts plus the inference flag, so policy is testable without
//! spawning anything.

pub mod launch;
pub mod registry;
pub mod supervisor;

pub use registry::RuntimeRegistry;
pub use supervisor::{
    allocate, split_evenly, AgentEvent, AgentSession, Allocation, AppClass, AppProcess, Supervisor,
    SupervisorError, BACKGROUND_FLOOR_TOTAL, MAX_AGENT_SESSIONS_PER_PRINCIPAL, TOTAL_SHARES,
};
