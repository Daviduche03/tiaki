//! Minimal PID 1 state: session capture and restore across a reboot.
//!
//! The session snapshot is deliberately plain data: the UI tree, keymap, theme,
//! focus, workspace, app values, and pending transactions. Restoring it makes a
//! generation swap feel like a repaint rather than a logout.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use agentic_json::Json;

pub const SESSION_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppSessionState {
    pub app: String,
    pub values: BTreeMap<String, String>,
    pub focused: Option<String>,
}

impl AppSessionState {
    pub fn new(app: impl Into<String>) -> Self {
        Self {
            app: app.into(),
            values: BTreeMap::new(),
            focused: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionSnapshot {
    pub version: u32,
    pub boot_id: String,
    pub captured_at: u64,
    pub user: String,
    pub ui_tree: Option<String>,
    pub keymap: Option<String>,
    pub theme: Option<String>,
    pub focus: Option<String>,
    pub workspace: u32,
    pub apps: Vec<AppSessionState>,
    pub pending_transactions: Vec<String>,
}

impl SessionSnapshot {
    pub fn new(boot_id: impl Into<String>, user: impl Into<String>, captured_at: u64) -> Self {
        Self {
            version: SESSION_VERSION,
            boot_id: boot_id.into(),
            captured_at,
            user: user.into(),
            ui_tree: None,
            keymap: None,
            theme: None,
            focus: None,
            workspace: 1,
            apps: Vec::new(),
            pending_transactions: Vec::new(),
        }
    }

    pub fn render(&self) -> String {
        self.to_json().render()
    }

    pub fn to_json(&self) -> Json {
        let mut object = BTreeMap::new();
        object.insert("version".to_string(), Json::Number(self.version as f64));
        object.insert("boot_id".to_string(), Json::String(self.boot_id.clone()));
        object.insert(
            "captured_at".to_string(),
            Json::Number(self.captured_at as f64),
        );
        object.insert("user".to_string(), Json::String(self.user.clone()));
        if let Some(tree) = &self.ui_tree {
            object.insert("ui_tree".to_string(), Json::String(tree.clone()));
        }
        if let Some(keymap) = &self.keymap {
            object.insert("keymap".to_string(), Json::String(keymap.clone()));
        }
        if let Some(theme) = &self.theme {
            object.insert("theme".to_string(), Json::String(theme.clone()));
        }
        if let Some(focus) = &self.focus {
            object.insert("focus".to_string(), Json::String(focus.clone()));
        }
        object.insert("workspace".to_string(), Json::Number(self.workspace as f64));
        object.insert(
            "apps".to_string(),
            Json::Array(
                self.apps
                    .iter()
                    .map(|app| {
                        Json::Object(
                            [
                                ("app".to_string(), Json::String(app.app.clone())),
                                (
                                    "values".to_string(),
                                    Json::Object(
                                        app.values
                                            .iter()
                                            .map(|(key, value)| {
                                                (key.clone(), Json::String(value.clone()))
                                            })
                                            .collect(),
                                    ),
                                ),
                            ]
                            .into_iter()
                            .chain(
                                app.focused
                                    .clone()
                                    .map(|focus| ("focused".to_string(), Json::String(focus))),
                            )
                            .collect(),
                        )
                    })
                    .collect(),
            ),
        );
        object.insert(
            "pending_transactions".to_string(),
            Json::Array(
                self.pending_transactions
                    .iter()
                    .map(|transaction| Json::String(transaction.clone()))
                    .collect(),
            ),
        );
        Json::Object(object)
    }

    pub fn parse(text: &str) -> Result<Self, SessionError> {
        let value =
            agentic_json::parse(text).map_err(|error| SessionError::new(error.to_string()))?;
        let object = value
            .as_object()
            .ok_or_else(|| SessionError::new("session snapshot must be a JSON object"))?;

        let version = object
            .get("version")
            .and_then(Json::as_f64)
            .map(|value| value as u32)
            .unwrap_or(SESSION_VERSION);
        if version != SESSION_VERSION {
            return Err(SessionError::new(format!(
                "unsupported session version {version}"
            )));
        }

        let read_opt = |key: &str| object.get(key).and_then(Json::as_str).map(str::to_string);

        let apps = object
            .get("apps")
            .and_then(Json::as_array)
            .map(|values| {
                values
                    .iter()
                    .map(|value| {
                        let entry = value
                            .as_object()
                            .ok_or_else(|| SessionError::new("an app entry must be an object"))?;
                        let app = entry
                            .get("app")
                            .and_then(Json::as_str)
                            .ok_or_else(|| SessionError::new("app entry needs 'app'"))?
                            .to_string();
                        let values = entry
                            .get("values")
                            .and_then(Json::as_object)
                            .map(|values| {
                                values
                                    .iter()
                                    .filter_map(|(key, value)| {
                                        value.as_str().map(|value| (key.clone(), value.to_string()))
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        Ok(AppSessionState {
                            app,
                            values,
                            focused: entry
                                .get("focused")
                                .and_then(Json::as_str)
                                .map(str::to_string),
                        })
                    })
                    .collect::<Result<Vec<_>, SessionError>>()
            })
            .transpose()?
            .unwrap_or_default();

        let pending_transactions = object
            .get("pending_transactions")
            .and_then(Json::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(|value| value.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();

        let user = object
            .get("user")
            .and_then(Json::as_str)
            .ok_or_else(|| SessionError::new("session snapshot needs 'user'"))?
            .to_string();
        if user.trim().is_empty() {
            return Err(SessionError::new("session user must not be empty"));
        }

        Ok(Self {
            version,
            boot_id: read_opt("boot_id").unwrap_or_else(|| "boot_unknown".into()),
            captured_at: object
                .get("captured_at")
                .and_then(Json::as_f64)
                .map(|value| value as u64)
                .unwrap_or(0),
            user,
            ui_tree: read_opt("ui_tree"),
            keymap: read_opt("keymap"),
            theme: read_opt("theme"),
            focus: read_opt("focus"),
            workspace: object
                .get("workspace")
                .and_then(Json::as_f64)
                .map(|value| value as u32)
                .unwrap_or(1),
            apps,
            pending_transactions,
        })
    }

    pub fn app(&self, app: &str) -> Option<&AppSessionState> {
        self.apps.iter().find(|state| state.app == app)
    }

    pub fn summary(&self) -> String {
        format!(
            "{} app(s), workspace {}, focus {}",
            self.apps.len(),
            self.workspace,
            self.focus.as_deref().unwrap_or("none")
        )
    }
}

pub struct SessionStore {
    path: PathBuf,
}

impl SessionStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    pub fn save(&self, snapshot: &SessionSnapshot) -> Result<(), SessionError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.path, snapshot.render())?;
        Ok(())
    }

    pub fn load(&self) -> Result<Option<SessionSnapshot>, SessionError> {
        if !self.path.exists() {
            return Ok(None);
        }
        let text = fs::read_to_string(&self.path)?;
        Ok(Some(SessionSnapshot::parse(&text)?))
    }

    pub fn clear(&self) -> Result<(), SessionError> {
        if self.path.exists() {
            fs::remove_file(&self.path)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionError {
    pub message: String,
}

impl SessionError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SessionError {}

impl From<std::io::Error> for SessionError {
    fn from(error: std::io::Error) -> Self {
        Self::new(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_with_apps_and_pending_work() {
        let mut snapshot = SessionSnapshot::new("boot_1", "user:ada", 1_000);
        snapshot.ui_tree = Some("{\"id\":\"desktop\"}".into());
        snapshot.keymap = Some("{\"bindings\":[]}".into());
        snapshot.theme = Some("dark".into());
        snapshot.focus = Some("gift".into());
        snapshot.workspace = 2;
        let mut app = AppSessionState::new("checkout");
        app.values.insert("gift".into(), "on".into());
        app.focused = Some("gift".into());
        snapshot.apps.push(app);
        snapshot.pending_transactions.push("txn_1".into());

        let parsed = SessionSnapshot::parse(&snapshot.render()).expect("parse");
        assert_eq!(parsed, snapshot);
        assert_eq!(parsed.app("checkout").unwrap().values["gift"], "on");
    }

    #[test]
    fn store_returns_none_when_missing() {
        let path = std::env::temp_dir().join(format!(
            "tiaki-session-{}-{:?}.json",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_file(&path);
        let store = SessionStore::new(&path);
        assert!(store.load().expect("load").is_none());
    }

    #[test]
    fn store_round_trips() {
        let dir = std::env::temp_dir().join(format!(
            "tiaki-session-store-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        let store = SessionStore::new(dir.join("snapshot.json"));
        let snapshot = SessionSnapshot::new("boot_1", "user:ada", 1_000);
        store.save(&snapshot).expect("save");
        assert_eq!(store.load().expect("load").unwrap(), snapshot);
        store.clear().expect("clear");
        assert!(!store.exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
