//! agentd command: Generation health probes and directory staging.
use crate::generation::authorize_generation;
use crate::generation::record_generation;
use crate::session::{generations_root, table_path};
use crate::{now_nanos, now_secs, Global, Ledger};
use agentic_core_boot::{image_digest, BootContext, HealthProbe};
use agentic_core_init::SessionSnapshot;
use agentic_core_log::Status;
use agentic_core_rollback::GenerationTable;
use agentic_inference::{is_model_path, ModelSpec};
use agentic_transaction::Severity;
use agentic_ui_tree::{Keymap, UiTree};
use std::path::Path;

pub(crate) struct SessionProbe;

impl HealthProbe for SessionProbe {
    fn name(&self) -> &str {
        "session"
    }

    fn check(&self, context: &BootContext) -> Result<(), String> {
        match &context.session {
            Some(text) => SessionSnapshot::parse(text)
                .map(|_| ())
                .map_err(|error| error.to_string()),
            None => Ok(()),
        }
    }
}

pub(crate) struct UiTreeProbe;

impl HealthProbe for UiTreeProbe {
    fn name(&self) -> &str {
        "ui-tree"
    }

    fn check(&self, context: &BootContext) -> Result<(), String> {
        let Some(text) = &context.session else {
            return Ok(());
        };
        let snapshot = SessionSnapshot::parse(text).map_err(|error| error.to_string())?;
        let Some(tree_text) = snapshot.ui_tree else {
            return Ok(());
        };
        let tree = UiTree::parse(&tree_text).map_err(|error| error.to_string())?;
        let issues = agentic_ui_tree::validate(&tree);
        let errors: Vec<String> = issues
            .iter()
            .filter(|issue| issue.severity == Severity::Error)
            .map(|issue| issue.message.clone())
            .collect();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
}

pub(crate) struct KeymapProbe;

impl HealthProbe for KeymapProbe {
    fn name(&self) -> &str {
        "keymap"
    }

    fn check(&self, context: &BootContext) -> Result<(), String> {
        let Some(text) = &context.session else {
            return Ok(());
        };
        let snapshot = SessionSnapshot::parse(text).map_err(|error| error.to_string())?;
        let Some(keymap_text) = snapshot.keymap else {
            return Ok(());
        };
        let keymap = Keymap::parse(&keymap_text).map_err(|error| error.to_string())?;
        let issues = keymap.validate();
        let errors: Vec<String> = issues
            .iter()
            .filter(|issue| issue.severity == Severity::Error)
            .map(|issue| issue.message.clone())
            .collect();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
}

pub(crate) struct ModelProbe;

impl HealthProbe for ModelProbe {
    fn name(&self) -> &str {
        "inference-registry"
    }

    fn check(&self, context: &BootContext) -> Result<(), String> {
        for (path, value) in &context.state {
            if is_model_path(path) {
                ModelSpec::parse(value).map_err(|error| format!("{path}: {error}"))?;
            }
        }
        Ok(())
    }
}

pub(crate) struct FailingProbe;

impl HealthProbe for FailingProbe {
    fn name(&self) -> &str {
        "simulated-candidate"
    }

    fn check(&self, _context: &BootContext) -> Result<(), String> {
        Err("the candidate image failed its health check".into())
    }
}

pub(crate) fn stage_generation(
    engine: &mut Ledger,
    global: &Global,
    home: &Path,
    source: &Path,
    label: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    authorize_generation(engine, global, "system/generations/new")?;
    if !source.is_dir() {
        return Err(format!("{} is not a directory", source.display()).into());
    }

    let id = format!("gen_{:x}", now_nanos());
    let destination = generations_root(home).join(&id).join("image");
    if destination.exists() {
        std::fs::remove_dir_all(&destination)?;
    }
    copy_dir(source, &destination)?;
    let digest = image_digest(&destination)?;

    let mut table = GenerationTable::load(table_path(home))?;
    table.stage(
        &id,
        label,
        format!("{id}/image"),
        digest.clone(),
        now_secs(),
    );
    table.save(table_path(home))?;

    record_generation(
        engine,
        global,
        format!("Staged generation {id} ({label})."),
        Status::Committed,
    );
    println!("  image {digest}");
    Ok(id)
}

pub(crate) fn copy_dir(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(destination)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let target = destination.join(entry.file_name());
        if file_type.is_symlink() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("refusing symlink in image: {}", entry.path().display()),
            ));
        }
        if file_type.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}
