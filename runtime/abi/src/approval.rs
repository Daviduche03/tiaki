//! Approval challenges that only trusted input can answer.

use std::collections::BTreeMap;
use std::fmt;

use agentic_core_log::{digest, hex};
use agentic_json::Json;

use crate::input::{constant_time_eq, DeviceKey, InputEvent, Trust};
use crate::tree::Risk;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApprovalRequest {
    /// Assigned by the approval service.
    pub id: String,
    pub principal: String,
    pub runtime: String,
    pub app: String,
    pub node: String,
    pub label: String,
    pub action: String,
    pub risk: Risk,
    pub reason: String,
}

impl ApprovalRequest {
    /// Eight fields, eight parameters; callers use struct-update syntax when
    /// they only set a few.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        principal: impl Into<String>,
        runtime: impl Into<String>,
        app: impl Into<String>,
        node: impl Into<String>,
        label: impl Into<String>,
        action: impl Into<String>,
        risk: Risk,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            id: String::new(),
            principal: principal.into(),
            runtime: runtime.into(),
            app: app.into(),
            node: node.into(),
            label: label.into(),
            action: action.into(),
            risk,
            reason: reason.into(),
        }
    }

    pub fn to_json(&self) -> Json {
        Json::Object(
            [
                ("id".to_string(), Json::String(self.id.clone())),
                (
                    "principal".to_string(),
                    Json::String(self.principal.clone()),
                ),
                ("runtime".to_string(), Json::String(self.runtime.clone())),
                ("app".to_string(), Json::String(self.app.clone())),
                ("node".to_string(), Json::String(self.node.clone())),
                ("label".to_string(), Json::String(self.label.clone())),
                ("action".to_string(), Json::String(self.action.clone())),
                ("risk".to_string(), Json::String(self.risk.as_str().into())),
                ("reason".to_string(), Json::String(self.reason.clone())),
            ]
            .into_iter()
            .collect(),
        )
    }

    pub fn from_json(value: &Json) -> Result<Self, ApprovalError> {
        let object = value.as_object().ok_or(ApprovalError::Malformed)?;
        let read = |key: &str| {
            object
                .get(key)
                .and_then(Json::as_str)
                .map(str::to_string)
                .ok_or(ApprovalError::Malformed)
        };
        Ok(Self {
            id: read("id")?,
            principal: read("principal")?,
            runtime: read("runtime")?,
            app: read("app")?,
            node: read("node")?,
            label: read("label")?,
            action: read("action")?,
            risk: Risk::parse(&read("risk")?).ok_or(ApprovalError::Malformed)?,
            reason: read("reason")?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Challenge {
    pub id: String,
    pub nonce: String,
    pub request: ApprovalRequest,
    pub expires_at: u64,
    pub used: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApprovalDecision {
    pub challenge: String,
    pub approved: bool,
    pub request: ApprovalRequest,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApprovalError {
    UnknownChallenge,
    Expired,
    AlreadyUsed,
    UntrustedInput,
    MissingToken,
    InvalidToken,
    Malformed,
}

impl fmt::Display for ApprovalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApprovalError::UnknownChallenge => write!(f, "unknown approval challenge"),
            ApprovalError::Expired => write!(f, "approval challenge expired"),
            ApprovalError::AlreadyUsed => write!(f, "approval challenge already used"),
            ApprovalError::UntrustedInput => {
                write!(f, "approval requires physical or trusted-device input")
            }
            ApprovalError::MissingToken => {
                write!(f, "trusted-device approval needs a signed token")
            }
            ApprovalError::InvalidToken => write!(f, "approval token did not verify"),
            ApprovalError::Malformed => write!(f, "malformed approval request"),
        }
    }
}

impl std::error::Error for ApprovalError {}

pub struct ApprovalService {
    next: u64,
    challenges: BTreeMap<String, Challenge>,
    device_key: DeviceKey,
}

impl ApprovalService {
    pub fn new(device_key: DeviceKey) -> Self {
        Self {
            next: 1,
            challenges: BTreeMap::new(),
            device_key,
        }
    }

    /// Create a challenge for a request. The challenge id and nonce are
    /// deterministic from the sequence and request, which is enough for
    /// replay protection and keeps the demo reproducible.
    pub fn request(&mut self, mut request: ApprovalRequest, now: u64, ttl_secs: u64) -> Challenge {
        let id = format!("appr_{}", self.next);
        self.next += 1;
        request.id = id.clone();
        let nonce = hex(&digest(
            format!(
                "{id}:{}:{}:{}:{now}",
                request.app, request.node, request.action
            )
            .as_bytes(),
        ));
        let challenge = Challenge {
            id: id.clone(),
            nonce,
            request,
            expires_at: now.saturating_add(ttl_secs),
            used: false,
        };
        self.challenges.insert(id, challenge.clone());
        challenge
    }

    pub fn challenge(&self, id: &str) -> Option<&Challenge> {
        self.challenges.get(id)
    }

    /// Answer a challenge. A rejected attempt (synthetic input, bad token)
    /// does not consume the challenge, so a legitimate human can still decide.
    pub fn decide(
        &mut self,
        challenge_id: &str,
        event: &InputEvent,
        token: Option<&str>,
        now: u64,
    ) -> Result<ApprovalDecision, ApprovalError> {
        let challenge = self
            .challenges
            .get_mut(challenge_id)
            .ok_or(ApprovalError::UnknownChallenge)?;
        if challenge.used {
            return Err(ApprovalError::AlreadyUsed);
        }
        if now >= challenge.expires_at {
            return Err(ApprovalError::Expired);
        }

        match event.trust {
            Trust::Synthetic => return Err(ApprovalError::UntrustedInput),
            Trust::Physical => {}
            Trust::TrustedDevice => {
                let token = token.ok_or(ApprovalError::MissingToken)?;
                let message = format!("{}|{}|{}", challenge.id, challenge.nonce, event.payload);
                let expected = self.device_key.sign(&message);
                if !constant_time_eq(token, &expected) {
                    return Err(ApprovalError::InvalidToken);
                }
            }
        }

        challenge.used = true;
        let approved = event.approved();
        Ok(ApprovalDecision {
            challenge: challenge.id.clone(),
            approved,
            request: challenge.request.clone(),
            reason: if approved {
                "approved by trusted input".to_string()
            } else {
                "denied by trusted input".to_string()
            },
        })
    }

    /// Convenience for a trusted device: sign the decision for a challenge.
    pub fn sign_for_device(&self, challenge_id: &str, payload: &str) -> Option<String> {
        let challenge = self.challenges.get(challenge_id)?;
        Some(
            self.device_key
                .sign(&format!("{}|{}|{}", challenge.id, challenge.nonce, payload)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service() -> ApprovalService {
        ApprovalService::new(DeviceKey::derive("test-device"))
    }

    fn request() -> ApprovalRequest {
        ApprovalRequest::new(
            "user:ada",
            "opencode",
            "checkout",
            "place",
            "Place order",
            "invoke",
            Risk::High,
            "high-risk action",
        )
    }

    #[test]
    fn synthetic_input_cannot_approve() {
        let mut service = service();
        let challenge = service.request(request(), 1_000, 60);
        let event = InputEvent::synthetic("allow", 1_001);
        let error = service
            .decide(&challenge.id, &event, None, 1_001)
            .expect_err("synthetic must fail");
        assert_eq!(error, ApprovalError::UntrustedInput);
        assert!(!service.challenge(&challenge.id).unwrap().used);
    }

    #[test]
    fn physical_input_approves_and_consumes() {
        let mut service = service();
        let challenge = service.request(request(), 1_000, 60);
        let decision = service
            .decide(
                &challenge.id,
                &InputEvent::physical("allow", 1_001),
                None,
                1_001,
            )
            .expect("physical");
        assert!(decision.approved);
        let replay = service
            .decide(
                &challenge.id,
                &InputEvent::physical("allow", 1_002),
                None,
                1_002,
            )
            .expect_err("replay");
        assert_eq!(replay, ApprovalError::AlreadyUsed);
    }

    #[test]
    fn physical_deny_is_a_decision() {
        let mut service = service();
        let challenge = service.request(request(), 1_000, 60);
        let decision = service
            .decide(
                &challenge.id,
                &InputEvent::physical("deny", 1_001),
                None,
                1_001,
            )
            .expect("deny");
        assert!(!decision.approved);
    }

    #[test]
    fn trusted_device_tokens_must_verify() {
        let mut service = service();
        let challenge = service.request(request(), 1_000, 60);
        let event = InputEvent::trusted_device("allow", 1_001, "phone");
        let good = service
            .sign_for_device(&challenge.id, "allow")
            .expect("sign");

        assert_eq!(
            service.decide(&challenge.id, &event, Some("hmac-sha256:00"), 1_001),
            Err(ApprovalError::InvalidToken)
        );
        let decision = service
            .decide(&challenge.id, &event, Some(&good), 1_001)
            .expect("verified");
        assert!(decision.approved);
    }

    #[test]
    fn expired_challenges_are_rejected() {
        let mut service = service();
        let challenge = service.request(request(), 1_000, 10);
        let error = service
            .decide(
                &challenge.id,
                &InputEvent::physical("allow", 1_020),
                None,
                1_020,
            )
            .expect_err("expired");
        assert_eq!(error, ApprovalError::Expired);
    }
}
