//! Input trust marking and approval tokens.
//!
//! The compositor marks every event. Approval dialogs accept only physical
//! human input or a signed token from a trusted device. Synthetic events from
//! an agent are rejected, otherwise the trust model collapses.

use std::fmt;

use agentic_core_log::{digest, hex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trust {
    Physical,
    TrustedDevice,
    Synthetic,
}

impl Trust {
    pub fn as_str(self) -> &'static str {
        match self {
            Trust::Physical => "physical",
            Trust::TrustedDevice => "trusted-device",
            Trust::Synthetic => "synthetic",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "physical" => Some(Trust::Physical),
            "trusted-device" => Some(Trust::TrustedDevice),
            "synthetic" => Some(Trust::Synthetic),
            _ => None,
        }
    }

    pub fn is_trusted(self) -> bool {
        matches!(self, Trust::Physical | Trust::TrustedDevice)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputSource {
    Keyboard,
    Pointer,
    Touch,
    Synthetic,
}

impl InputSource {
    pub fn as_str(self) -> &'static str {
        match self {
            InputSource::Keyboard => "keyboard",
            InputSource::Pointer => "pointer",
            InputSource::Touch => "touch",
            InputSource::Synthetic => "synthetic",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "keyboard" => Some(InputSource::Keyboard),
            "pointer" => Some(InputSource::Pointer),
            "touch" => Some(InputSource::Touch),
            "synthetic" => Some(InputSource::Synthetic),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputEvent {
    pub kind: String,
    pub source: InputSource,
    pub trust: Trust,
    pub timestamp: u64,
    pub device: Option<String>,
    /// The approval dialog's answer: `allow` or `deny`.
    pub payload: String,
}

impl InputEvent {
    pub fn new(
        kind: impl Into<String>,
        source: InputSource,
        trust: Trust,
        payload: impl Into<String>,
        timestamp: u64,
    ) -> Self {
        Self {
            kind: kind.into(),
            source,
            trust,
            timestamp,
            device: None,
            payload: payload.into(),
        }
    }

    pub fn physical(payload: impl Into<String>, timestamp: u64) -> Self {
        Self::new(
            "approval",
            InputSource::Keyboard,
            Trust::Physical,
            payload,
            timestamp,
        )
    }

    pub fn synthetic(payload: impl Into<String>, timestamp: u64) -> Self {
        Self::new(
            "approval",
            InputSource::Synthetic,
            Trust::Synthetic,
            payload,
            timestamp,
        )
    }

    pub fn trusted_device(
        payload: impl Into<String>,
        timestamp: u64,
        device: impl Into<String>,
    ) -> Self {
        let mut event = Self::new(
            "approval",
            InputSource::Touch,
            Trust::TrustedDevice,
            payload,
            timestamp,
        );
        event.device = Some(device.into());
        event
    }

    pub fn approved(&self) -> bool {
        self.payload != "deny"
    }

    pub fn to_json(&self) -> agentic_json::Json {
        use agentic_json::Json;
        use std::collections::BTreeMap;
        let mut object = BTreeMap::new();
        object.insert("kind".into(), Json::String(self.kind.clone()));
        object.insert("source".into(), Json::String(self.source.as_str().into()));
        object.insert("trust".into(), Json::String(self.trust.as_str().into()));
        object.insert("timestamp".into(), Json::Number(self.timestamp as f64));
        object.insert("payload".into(), Json::String(self.payload.clone()));
        if let Some(device) = &self.device {
            object.insert("device".into(), Json::String(device.clone()));
        }
        Json::Object(object)
    }

    pub fn from_json(value: &agentic_json::Json) -> Result<Self, InputError> {
        use agentic_json::Json;
        let object = value
            .as_object()
            .ok_or_else(|| InputError::new("input event must be an object"))?;
        let read = |key: &str| {
            object
                .get(key)
                .and_then(Json::as_str)
                .map(str::to_string)
                .ok_or_else(|| InputError::new(format!("input event needs '{key}'")))
        };
        Ok(Self {
            kind: read("kind")?,
            source: InputSource::parse(&read("source")?)
                .ok_or_else(|| InputError::new("unknown input source"))?,
            trust: Trust::parse(&read("trust")?)
                .ok_or_else(|| InputError::new("unknown trust level"))?,
            timestamp: object
                .get("timestamp")
                .and_then(Json::as_f64)
                .map(|value| value as u64)
                .unwrap_or(0),
            device: object
                .get("device")
                .and_then(Json::as_str)
                .map(str::to_string),
            payload: read("payload")?,
        })
    }
}

/// A device's approval-signing key. On real hardware this lives in the TPM or
/// keystore; here it is derived from a seed so the demo is reproducible.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceKey(pub [u8; 32]);

impl DeviceKey {
    pub fn derive(seed: &str) -> Self {
        Self(digest(format!("tiaki-device:{seed}").as_bytes()))
    }

    pub fn sign(&self, message: &str) -> String {
        format!(
            "hmac-sha256:{}",
            hex(&hmac_sha256(&self.0, message.as_bytes()))
        )
    }
}

/// HMAC-SHA256 (RFC 2104) over the from-scratch SHA-256.
pub fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut key_block = [0u8; BLOCK];
    if key.len() > BLOCK {
        key_block[..32].copy_from_slice(&digest(key));
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }

    let mut inner = Vec::with_capacity(BLOCK + message.len());
    inner.extend(key_block.iter().map(|byte| byte ^ 0x36));
    inner.extend_from_slice(message);
    let inner_hash = digest(&inner);

    let mut outer = Vec::with_capacity(BLOCK + 32);
    outer.extend(key_block.iter().map(|byte| byte ^ 0x5c));
    outer.extend_from_slice(&inner_hash);
    digest(&outer)
}

pub fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut difference = 0u8;
    for (left, right) in a.as_bytes().iter().zip(b.as_bytes()) {
        difference |= left ^ right;
    }
    difference == 0
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputError {
    pub message: String,
}

impl InputError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for InputError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_physical_and_trusted_device_are_trusted() {
        assert!(Trust::Physical.is_trusted());
        assert!(Trust::TrustedDevice.is_trusted());
        assert!(!Trust::Synthetic.is_trusted());
    }

    #[test]
    fn hmac_matches_a_known_vector() {
        // RFC 4231 test case 2.
        let mac = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(
            hex(&mac),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn device_signatures_round_trip() {
        let key = DeviceKey::derive("laptop");
        let signature = key.sign("challenge|nonce|allow");
        assert!(constant_time_eq(
            &signature,
            &key.sign("challenge|nonce|allow")
        ));
        assert!(!constant_time_eq(
            &signature,
            &key.sign("challenge|nonce|deny")
        ));
    }
}
