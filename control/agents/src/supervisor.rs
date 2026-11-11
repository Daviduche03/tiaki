//! Supervision: admit agent sessions and split CPU shares by class.
//!
//! Priority ladder, highest first:
//!
//! 1. **Local inference.** Owned by the inference broker, not scheduled here.
//!    While a burst is active the supervisor caps the session pool at a floor;
//!    agents yield and resume automatically.
//! 2. **Agent sessions.** Starting a session automatically boosts it: shares
//!    move from ordinary apps into the session pool. No grant, no approval —
//!    the boost is policy, and the ledger records it.
//! 3. **Foreground apps.** Interactive work keeps the larger remainder.
//! 4. **Background work.** Throttled while agents run, but the pool never
//!    drops below [`BACKGROUND_FLOOR_TOTAL`].
//!
//! Shares are illustrative weights for a real cgroup/cpuset backend, recorded
//! on each session's sandbox spec (`cpu_shares`) so the provisioner can apply
//! them. Within a pool, shares split evenly; any remainder goes to the lowest
//! ids first, so the allocation is deterministic.

use std::collections::BTreeMap;

use agentic_sandbox::{
    plan_shell, SandboxHandle, SandboxProfile, SandboxProvisioner, SandboxSpec, ShellRequest,
    SimulatedProvisioner,
};

use crate::registry::RuntimeRegistry;

/// Shares in one scheduling round. Always sums to [`TOTAL_SHARES`].
pub const TOTAL_SHARES: u32 = 1024;
/// Inference burst reservation, held outside the agent/app pools.
pub const INFERENCE_RESERVE: u32 = 512;
/// Session pool while no burst is active.
pub const SESSION_POOL: u32 = 640;
/// Session pool ceiling while a burst is active: agents yield, they do not stop.
pub const SESSION_YIELD_FLOOR_TOTAL: u32 = 128;
/// Background pool floor: throttled, never starved.
pub const BACKGROUND_FLOOR_TOTAL: u32 = 128;
/// Foreground share of whatever remains after the higher pools (5/8).
pub const FOREGROUND_NUMERATOR: u32 = 5;
pub const FOREGROUND_DENOMINATOR: u32 = 8;
/// Admission cap, mirroring the inference broker's per-principal default.
pub const MAX_AGENT_SESSIONS_PER_PRINCIPAL: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AppClass {
    Foreground,
    Background,
}

impl AppClass {
    pub fn as_str(self) -> &'static str {
        match self {
            AppClass::Foreground => "foreground",
            AppClass::Background => "background",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Allocation {
    pub inference: u32,
    pub session: u32,
    pub foreground: u32,
    pub background: u32,
}

impl Allocation {
    pub fn pool(&self, class: AppClass) -> u32 {
        match class {
            AppClass::Foreground => self.foreground,
            AppClass::Background => self.background,
        }
    }
}

/// Pure policy: pools from counts plus the inference flag. Tested alone.
pub fn allocate(
    sessions: usize,
    foreground: usize,
    background: usize,
    inference_active: bool,
) -> Allocation {
    let inference = if inference_active {
        INFERENCE_RESERVE
    } else {
        0
    };
    let mut session = if sessions == 0 {
        0
    } else if inference_active {
        SESSION_YIELD_FLOOR_TOTAL
    } else {
        SESSION_POOL
    };
    let rest = TOTAL_SHARES - inference - session;
    let mut fg = 0;
    let mut bg = 0;
    if foreground > 0 && background > 0 {
        fg = rest * FOREGROUND_NUMERATOR / FOREGROUND_DENOMINATOR;
        bg = rest - fg;
        if bg < BACKGROUND_FLOOR_TOTAL {
            bg = BACKGROUND_FLOOR_TOTAL;
            fg = rest - bg;
        }
    } else if foreground > 0 {
        fg = rest;
    } else if background > 0 {
        // `rest` stays >= 384 (agent caps at 640, inference at 512, never
        // both), so the floor holds.
        bg = rest;
    } else if sessions > 0 {
        // No apps to contend with: the session pool takes the whole remainder.
        // Pools are guarantees under contention, not caps.
        session = TOTAL_SHARES - inference;
    }
    // With any work present the pools cover TOTAL; with none, all are zero.
    Allocation {
        inference,
        session,
        foreground: fg,
        background: bg,
    }
}

/// Even split of a pool; remainder goes to the lowest ids first.
pub fn split_evenly(pool: u32, count: usize) -> Vec<u32> {
    if count == 0 {
        return Vec::new();
    }
    let count = count as u32;
    let base = pool / count;
    let mut remainder = pool % count;
    (0..count)
        .map(|_| {
            let extra = if remainder > 0 {
                remainder -= 1;
                1
            } else {
                0
            };
            base + extra
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentSession {
    pub id: u64,
    pub principal: String,
    pub runtime: String,
    pub app: String,
    pub shares: u32,
    pub spec: SandboxSpec,
    /// The provisioner's handle. Shares are applied through this, not just
    /// stored, so a real backend enforces them.
    pub sandbox: SandboxHandle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppProcess {
    pub id: String,
    pub class: AppClass,
    pub shares: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentEvent {
    AppRegistered {
        id: String,
        class: AppClass,
    },
    AgentStarted {
        id: u64,
        runtime: String,
        app: String,
        shares: u32,
        boosted: bool,
    },
    AgentStopped {
        id: u64,
        runtime: String,
    },
    InferenceChanged {
        active: bool,
    },
    Shares {
        allocation: Allocation,
    },
}

impl AgentEvent {
    /// The one-line plain-language summary every change carries.
    pub fn summary(&self) -> String {
        match self {
            AgentEvent::AppRegistered { id, class } => {
                format!("Registered '{id}' as {} work.", class.as_str())
            }
            AgentEvent::AgentStarted {
                id,
                runtime,
                app,
                shares,
                boosted,
            } => {
                if *boosted {
                    format!(
                        "Agent session {id} ({runtime} driving '{app}') started and boosted to {shares} shares."
                    )
                } else {
                    format!(
                        "Agent session {id} ({runtime} driving '{app}') started with {shares} shares."
                    )
                }
            }
            AgentEvent::AgentStopped { id, runtime } => {
                format!("Agent session {id} ({runtime}) stopped; shares returned to apps.")
            }
            AgentEvent::InferenceChanged { active } => {
                if *active {
                    format!(
                        "Inference burst started; agents yield to {SESSION_YIELD_FLOOR_TOTAL} shares."
                    )
                } else {
                    "Inference burst cleared; agents resume their pool.".to_string()
                }
            }
            AgentEvent::Shares { allocation } => {
                format!(
                    "Shares now inference {} / session {} / foreground {} / background {}.",
                    allocation.inference,
                    allocation.session,
                    allocation.foreground,
                    allocation.background
                )
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum SupervisorError {
    UnknownRuntime(String),
    Denied(String),
    TooManySessions(String),
    UnknownSession(u64),
    DuplicateApp(String),
    Provision(String),
}

impl std::fmt::Display for SupervisorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SupervisorError::UnknownRuntime(id) => write!(f, "unknown runtime '{id}'"),
            SupervisorError::Denied(reason) => write!(f, "denied: {reason}"),
            SupervisorError::TooManySessions(principal) => write!(
                f,
                "{principal} already runs {MAX_AGENT_SESSIONS_PER_PRINCIPAL} agent sessions"
            ),
            SupervisorError::UnknownSession(id) => write!(f, "unknown agent session {id}"),
            SupervisorError::DuplicateApp(id) => write!(f, "app '{id}' is already registered"),
            SupervisorError::Provision(reason) => write!(f, "could not provision: {reason}"),
        }
    }
}

impl std::error::Error for SupervisorError {}

pub struct Supervisor {
    next_id: u64,
    registry: RuntimeRegistry,
    sessions: BTreeMap<u64, AgentSession>,
    apps: BTreeMap<String, AppProcess>,
    inference_active: bool,
    events: Vec<AgentEvent>,
    provisioner: Box<dyn SandboxProvisioner>,
    warnings: Vec<String>,
}

impl Supervisor {
    /// Simulated enforcement: the policy is real, the kernel is not told.
    pub fn new(registry: RuntimeRegistry) -> Self {
        Self::with_provisioner(registry, Box::new(SimulatedProvisioner::new()))
    }

    /// Real enforcement when the backend can apply weights; admission fails
    /// rather than silently running unweighted.
    pub fn with_provisioner(
        registry: RuntimeRegistry,
        provisioner: Box<dyn SandboxProvisioner>,
    ) -> Self {
        Self {
            next_id: 1,
            registry,
            sessions: BTreeMap::new(),
            apps: BTreeMap::new(),
            inference_active: false,
            events: Vec::new(),
            provisioner,
            warnings: Vec::new(),
        }
    }

    pub fn provisioner(&self) -> &dyn SandboxProvisioner {
        &*self.provisioner
    }

    /// Non-fatal problems, such as a backend that refused a re-weight. Empty
    /// means every share transition reached the backend.
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    pub fn registry(&self) -> &RuntimeRegistry {
        &self.registry
    }

    pub fn sessions(&self) -> Vec<&AgentSession> {
        self.sessions.values().collect()
    }

    pub fn apps(&self) -> Vec<&AppProcess> {
        self.apps.values().collect()
    }

    pub fn events(&self) -> &[AgentEvent] {
        &self.events
    }

    pub fn allocation(&self) -> Allocation {
        self.pools()
    }

    pub fn register_app(
        &mut self,
        id: impl Into<String>,
        class: AppClass,
    ) -> Result<(), SupervisorError> {
        let id = id.into();
        if self.apps.contains_key(&id) {
            return Err(SupervisorError::DuplicateApp(id));
        }
        self.apps.insert(
            id.clone(),
            AppProcess {
                id: id.clone(),
                class,
                shares: 0,
            },
        );
        self.events.push(AgentEvent::AppRegistered { id, class });
        self.rebalance();
        Ok(())
    }

    /// Admit an agent session. The boost is automatic: starting rebalances
    /// shares toward the session pool immediately. `authorize` is the host's
    /// `computer.use` check for `(principal, app)`; a denial rejects the
    /// session before anything is provisioned.
    pub fn start_session(
        &mut self,
        principal: &str,
        runtime: &str,
        app: &str,
        authorize: &dyn Fn(&str, &str) -> Result<(), String>,
    ) -> Result<u64, SupervisorError> {
        if self.registry.get(runtime).is_none() {
            return Err(SupervisorError::UnknownRuntime(runtime.into()));
        }
        if let Err(reason) = authorize(principal, app) {
            return Err(SupervisorError::Denied(reason));
        }
        let running = self
            .sessions
            .values()
            .filter(|session| session.principal == principal)
            .count();
        if running >= MAX_AGENT_SESSIONS_PER_PRINCIPAL {
            return Err(SupervisorError::TooManySessions(principal.into()));
        }

        let id = self.next_id;
        self.next_id += 1;
        let mut spec = plan_shell(&ShellRequest::new(
            principal,
            SandboxProfile::UserWrite,
            format!("agent:{runtime}"),
        ));
        spec.limits.cpu_shares = Some(0);
        let sandbox = self
            .provisioner
            .create(spec.clone())
            .map_err(|error| SupervisorError::Provision(error.to_string()))?;
        self.sessions.insert(
            id,
            AgentSession {
                id,
                principal: principal.into(),
                runtime: runtime.into(),
                app: app.into(),
                shares: 0,
                spec,
                sandbox,
            },
        );
        self.rebalance();
        let shares = self.sessions.get(&id).map(|s| s.shares).unwrap_or(0);
        // The first session boosts the whole session class; later sessions
        // share the pool it won.
        let boosted = self.sessions.len() == 1;
        self.events.push(AgentEvent::AgentStarted {
            id,
            runtime: runtime.into(),
            app: app.into(),
            shares,
            boosted,
        });
        self.events.push(AgentEvent::Shares {
            allocation: self.pools(),
        });
        Ok(id)
    }

    pub fn stop_session(&mut self, id: u64) -> Result<(), SupervisorError> {
        let session = self
            .sessions
            .remove(&id)
            .ok_or(SupervisorError::UnknownSession(id))?;
        if let Err(error) = self.provisioner.destroy(&session.sandbox) {
            self.warnings
                .push(format!("session {id}: could not destroy sandbox: {error}"));
        }
        self.rebalance();
        self.events.push(AgentEvent::AgentStopped {
            id,
            runtime: session.runtime,
        });
        self.events.push(AgentEvent::Shares {
            allocation: self.pools(),
        });
        Ok(())
    }

    /// The inference broker reports burst activity here. While active, the
    /// session pool is capped at the yield floor; clearing restores the boost.
    pub fn set_inference_active(&mut self, active: bool) {
        if self.inference_active == active {
            return;
        }
        self.inference_active = active;
        self.rebalance();
        self.events.push(AgentEvent::InferenceChanged { active });
        self.events.push(AgentEvent::Shares {
            allocation: self.pools(),
        });
    }

    fn pools(&self) -> Allocation {
        let foreground = self
            .apps
            .values()
            .filter(|app| app.class == AppClass::Foreground)
            .count();
        let background = self
            .apps
            .values()
            .filter(|app| app.class == AppClass::Background)
            .count();
        allocate(
            self.sessions.len(),
            foreground,
            background,
            self.inference_active,
        )
    }

    /// Recompute every share from the pools and stamp each agent session's
    /// sandbox spec, so the provisioner can apply the weights for real.
    fn rebalance(&mut self) {
        let pools = self.pools();
        let mut agent_ids: Vec<u64> = self.sessions.keys().cloned().collect();
        agent_ids.sort_unstable();
        for (id, shares) in agent_ids
            .iter()
            .zip(split_evenly(pools.session, agent_ids.len()))
        {
            if let Some(session) = self.sessions.get_mut(id) {
                session.shares = shares;
                session.spec.limits.cpu_shares = Some(shares);
            }
            // Tell the backend too. A backend that cannot enforce shares
            // reports why; that is a warning, not a silent success.
            let handle = self.sessions.get(id).map(|session| session.sandbox.clone());
            if let Some(handle) = handle {
                if let Err(error) = self.provisioner.set_shares(&handle, shares) {
                    let message = format!("session {id}: {error}");
                    if !self.warnings.contains(&message) {
                        self.warnings.push(message);
                    }
                }
            }
        }
        let mut fg_ids: Vec<String> = self
            .apps
            .values()
            .filter(|app| app.class == AppClass::Foreground)
            .map(|app| app.id.clone())
            .collect();
        fg_ids.sort();
        for (id, shares) in fg_ids
            .iter()
            .zip(split_evenly(pools.foreground, fg_ids.len()))
        {
            if let Some(app) = self.apps.get_mut(id) {
                app.shares = shares;
            }
        }
        let mut bg_ids: Vec<String> = self
            .apps
            .values()
            .filter(|app| app.class == AppClass::Background)
            .map(|app| app.id.clone())
            .collect();
        bg_ids.sort();
        for (id, shares) in bg_ids
            .iter()
            .zip(split_evenly(pools.background, bg_ids.len()))
        {
            if let Some(app) = self.apps.get_mut(id) {
                app.shares = shares;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentic_runtime_abi::PROTOCOL;

    fn spec(id: &str) -> agentic_runtime_abi::RuntimeSpec {
        agentic_runtime_abi::RuntimeSpec {
            id: id.into(),
            protocol: PROTOCOL.into(),
            transport: format!("unix:///run/tiaki/runtime/{id}.sock"),
            tools: vec!["computer-use".into()],
        }
    }

    fn supervisor() -> Supervisor {
        Supervisor::new(RuntimeRegistry::new(vec![spec("opencode"), spec("codex")]).unwrap())
    }

    fn allow(_principal: &str, _app: &str) -> Result<(), String> {
        Ok(())
    }

    fn deny(_principal: &str, _app: &str) -> Result<(), String> {
        Err("user:mallory may not drive 'docs': capability denied".into())
    }

    #[test]
    fn shares_are_pushed_to_the_backend_not_just_remembered() {
        use agentic_sandbox::{SandboxProvisioner, SimulatedProvisioner};
        use std::sync::Arc;

        // Share one provisioner with the supervisor so the test can read what
        // the backend was told.
        struct Shared(Arc<SimulatedProvisioner>);
        impl SandboxProvisioner for Shared {
            fn create(
                &self,
                spec: agentic_sandbox::SandboxSpec,
            ) -> Result<agentic_sandbox::SandboxHandle, agentic_sandbox::SandboxError> {
                self.0.create(spec)
            }
            fn destroy(
                &self,
                handle: &agentic_sandbox::SandboxHandle,
            ) -> Result<(), agentic_sandbox::SandboxError> {
                self.0.destroy(handle)
            }
            fn status(
                &self,
                handle: &agentic_sandbox::SandboxHandle,
            ) -> Result<agentic_sandbox::SandboxStatus, agentic_sandbox::SandboxError> {
                self.0.status(handle)
            }
            fn set_shares(
                &self,
                handle: &agentic_sandbox::SandboxHandle,
                shares: u32,
            ) -> Result<(), agentic_sandbox::SandboxError> {
                self.0.set_shares(handle, shares)
            }
            fn read_shares(
                &self,
                handle: &agentic_sandbox::SandboxHandle,
            ) -> Result<u32, agentic_sandbox::SandboxError> {
                self.0.read_shares(handle)
            }
        }

        let backend = Arc::new(SimulatedProvisioner::new());
        let registry = RuntimeRegistry::new(vec![spec("opencode")]).unwrap();
        let mut supervisor =
            Supervisor::with_provisioner(registry, Box::new(Shared(backend.clone())));
        // Contention is what the pools describe; with no apps the session takes
        // the whole machine, so register one.
        supervisor
            .register_app("editor", AppClass::Foreground)
            .unwrap();

        let id = supervisor
            .start_session("user:ada", "opencode", "docs", &allow)
            .unwrap();
        let session = supervisor
            .sessions()
            .into_iter()
            .find(|s| s.id == id)
            .map(|s| s.sandbox.clone())
            .unwrap();
        assert_eq!(backend.shares(&session), Some(SESSION_POOL));

        supervisor.set_inference_active(true);
        assert_eq!(backend.shares(&session), Some(SESSION_YIELD_FLOOR_TOTAL));
        assert!(
            supervisor.warnings().is_empty(),
            "backend accepted every weight"
        );
    }

    #[test]
    fn a_backend_that_cannot_enforce_shares_is_reported() {
        use agentic_sandbox::{
            SandboxError, SandboxHandle, SandboxProvisioner, SandboxSpec, SandboxStatus,
        };

        struct NoEnforcement;
        impl SandboxProvisioner for NoEnforcement {
            fn create(&self, _spec: SandboxSpec) -> Result<SandboxHandle, SandboxError> {
                Ok(SandboxHandle {
                    id: "sandbox_1".into(),
                })
            }
            fn destroy(&self, _handle: &SandboxHandle) -> Result<(), SandboxError> {
                Ok(())
            }
            fn status(&self, _handle: &SandboxHandle) -> Result<SandboxStatus, SandboxError> {
                Ok(SandboxStatus::Running)
            }
        }

        let registry = RuntimeRegistry::new(vec![spec("opencode")]).unwrap();
        let mut supervisor = Supervisor::with_provisioner(registry, Box::new(NoEnforcement));
        supervisor
            .start_session("user:ada", "opencode", "docs", &allow)
            .unwrap();
        assert!(
            !supervisor.warnings().is_empty(),
            "an unenforced share weight must surface, not be swallowed"
        );
    }

    #[test]
    fn pools_sum_to_total_when_work_is_present_and_background_is_never_starved() {
        for sessions in 0..=3 {
            for fg in 0..=2 {
                for bg in 0..=3 {
                    for inference in [false, true] {
                        let allocation = allocate(sessions, fg, bg, inference);
                        let sum = allocation.inference
                            + allocation.session
                            + allocation.foreground
                            + allocation.background;
                        // Empty classes leave their pool unallocated; whenever
                        // any work is present, the pools cover TOTAL.
                        if sessions + fg + bg > 0 {
                            assert_eq!(
                                sum, TOTAL_SHARES,
                                "sessions={sessions} fg={fg} bg={bg} inference={inference}"
                            );
                        } else {
                            assert!(sum <= TOTAL_SHARES);
                        }
                        if bg > 0 {
                            assert!(
                                allocation.background >= BACKGROUND_FLOOR_TOTAL.min(TOTAL_SHARES),
                                "background starved: {allocation:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn agent_boost_takes_from_apps_automatically() {
        let mut supervisor = supervisor();
        supervisor
            .register_app("editor", AppClass::Foreground)
            .unwrap();
        supervisor
            .register_app("file-sync", AppClass::Background)
            .unwrap();
        let before: u32 = supervisor.apps().iter().map(|app| app.shares).sum();
        assert_eq!(before, TOTAL_SHARES);

        let id = supervisor
            .start_session("user:ada", "opencode", "docs", &allow)
            .unwrap();
        let session = supervisor
            .sessions()
            .into_iter()
            .find(|s| s.id == id)
            .unwrap();
        assert_eq!(session.shares, SESSION_POOL);
        // The boost is stamped on the sandbox spec for the provisioner.
        assert_eq!(session.spec.limits.cpu_shares, Some(SESSION_POOL));
        let apps: u32 = supervisor.apps().iter().map(|app| app.shares).sum();
        assert_eq!(apps, TOTAL_SHARES - SESSION_POOL);
        assert!(supervisor
            .events()
            .iter()
            .any(|event| matches!(event, AgentEvent::AgentStarted { boosted: true, .. })));
    }

    #[test]
    fn second_runtime_runs_and_splits_the_agent_pool() {
        let mut supervisor = supervisor();
        supervisor
            .register_app("editor", AppClass::Foreground)
            .unwrap();
        let first = supervisor
            .start_session("user:ada", "opencode", "docs", &allow)
            .unwrap();
        let second = supervisor
            .start_session("user:ada", "codex", "docs", &allow)
            .unwrap();
        assert_ne!(first, second);
        let mut shares: Vec<u32> = supervisor.sessions().iter().map(|s| s.shares).collect();
        shares.sort_unstable();
        assert_eq!(shares.iter().sum::<u32>(), SESSION_POOL);
        assert!((shares[0] as i32 - shares[1] as i32).abs() <= 1);
        // The first session won the pool; the second shares it.
        let boosted: Vec<bool> = supervisor
            .events()
            .iter()
            .filter_map(|event| match event {
                AgentEvent::AgentStarted { boosted, .. } => Some(*boosted),
                _ => None,
            })
            .collect();
        assert_eq!(boosted, vec![true, false]);
    }

    #[test]
    fn inference_burst_forces_agents_to_the_floor_then_restores() {
        let mut supervisor = supervisor();
        supervisor
            .register_app("editor", AppClass::Foreground)
            .unwrap();
        supervisor
            .start_session("user:ada", "opencode", "docs", &allow)
            .unwrap();

        supervisor.set_inference_active(true);
        let agent: u32 = supervisor.sessions().iter().map(|s| s.shares).sum();
        assert_eq!(agent, SESSION_YIELD_FLOOR_TOTAL);
        assert_eq!(supervisor.allocation().inference, INFERENCE_RESERVE);

        supervisor.set_inference_active(false);
        let agent: u32 = supervisor.sessions().iter().map(|s| s.shares).sum();
        assert_eq!(agent, SESSION_POOL);
    }

    #[test]
    fn stopping_returns_every_share_to_apps() {
        let mut supervisor = supervisor();
        supervisor
            .register_app("editor", AppClass::Foreground)
            .unwrap();
        let id = supervisor
            .start_session("user:ada", "opencode", "docs", &allow)
            .unwrap();
        supervisor.stop_session(id).unwrap();
        let apps: u32 = supervisor.apps().iter().map(|app| app.shares).sum();
        assert_eq!(apps, TOTAL_SHARES);
    }

    #[test]
    fn admission_needs_a_known_runtime_a_grant_and_quota() {
        let mut supervisor = supervisor();
        assert!(matches!(
            supervisor.start_session("user:ada", "nope", "docs", &allow),
            Err(SupervisorError::UnknownRuntime(_))
        ));
        assert!(matches!(
            supervisor.start_session("user:mallory", "opencode", "docs", &deny),
            Err(SupervisorError::Denied(_))
        ));
        supervisor
            .start_session("user:ada", "opencode", "docs", &allow)
            .unwrap();
        supervisor
            .start_session("user:ada", "codex", "docs", &allow)
            .unwrap();
        assert!(matches!(
            supervisor.start_session("user:ada", "opencode", "docs", &allow),
            Err(SupervisorError::TooManySessions(_))
        ));
    }
}
