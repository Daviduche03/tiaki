//! Agent runtime support: typed tools, structured app trees, app backends,
//! and trusted input.
//!
//! The OS owns capabilities and policy; the agent owns reasoning and tool
//! selection. Computer use prefers a structured tree over pixels, and approval
//! can never be spoofed by a synthetic event. Agents are driven through their
//! own server APIs; there is no private wire protocol here.

pub mod approval;
pub mod input;
pub mod protocol;
pub mod session;
pub mod tree;

pub use approval::{ApprovalDecision, ApprovalError, ApprovalRequest, ApprovalService, Challenge};
pub use input::{constant_time_eq, hmac_sha256, DeviceKey, InputEvent, InputSource, Trust};
pub use protocol::{ProtocolError, RuntimeSpec, ToolSpec, PROTOCOL};
pub use session::{AppAction, AppBackend, AppBackendError, SimulatedApp};
pub use tree::{ActionKind, AppNode, AppTree, AppTreeError, Bounds, Risk, Role};
