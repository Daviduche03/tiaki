//! Plain-language summaries, impact rendering, and diff/summary mismatch detection.
//!
//! Milestone 0 uses a deterministic, rule-based summarizer. A model-backed
//! summarizer plugs into the same `Summarizer` trait later. See TIAKI.md §8.7.

#![allow(dead_code)]

use agentic_core_log::{Impact, Summary};
use agentic_transaction::{Operation, Summarizer};

pub struct RuleBasedSummarizer;

impl Summarizer for RuleBasedSummarizer {
    fn summarize(&self, intent: &str, operations: &[Operation]) -> Summary {
        let one_line = match operations {
            [] => "No changes.".to_string(),
            [Operation::Set { path, .. }] if is_ui_tree(path) => "Rebuilt the UI tree.".to_string(),
            [Operation::Set { path, .. }] if is_keymap(path) => "Updated the keymap.".to_string(),
            [Operation::Set { path, value }] if is_inference_model(path) => {
                let id = model_id(path);
                match agentic_inference::ModelSpec::parse(value) {
                    Ok(spec) if spec.loaded => format!("Loaded model {id}."),
                    Ok(_) => format!("Unloaded model {id}."),
                    Err(_) => format!("Updated model {id}."),
                }
            }
            [Operation::Set { path, value }] => format!("Set {} to {}.", human(path), value),
            [Operation::Remove { path }] if is_ui_tree(path) => {
                "Reset the UI tree to its default.".to_string()
            }
            [Operation::Remove { path }] if is_keymap(path) => {
                "Reset the keymap to its default.".to_string()
            }
            [Operation::Remove { path }] if is_inference_model(path) => {
                format!("Removed model {}.", model_id(path))
            }
            [Operation::Remove { path }] => format!("Removed {}.", human(path)),
            _ => format!("Changed {} settings.", operations.len()),
        };

        let detail = if intent.trim().is_empty() {
            one_line.clone()
        } else {
            format!("{one_line} Requested: {intent}")
        };

        Summary::new(one_line, detail, "rule-based", true)
    }

    fn impact(&self, operations: &[Operation]) -> Impact {
        let mut impact = Impact::none();
        for operation in operations {
            let path = unscoped(operation.path());
            if path.starts_with("desktop/") || path.starts_with("theme/") || path.starts_with("ui/")
            {
                impact.ui_changed = true;
            }
            if path.starts_with("service/")
                || path.starts_with("services/")
                || path.starts_with("system/services/")
            {
                impact.services_restarted += 1;
            }
            if path.starts_with("kernel/")
                || path.starts_with("system/image")
                || path.starts_with("boot/")
                || path.starts_with("core/")
                || path.starts_with("recovery/")
                || path.starts_with("system/recovery/")
            {
                impact.reboot_required = true;
            }
        }
        impact
    }

    fn verify(&self, summary: &Summary, operations: &[Operation]) -> bool {
        operations.iter().all(|operation| {
            let path = operation.path();
            let leaf = path.rsplit('/').next().unwrap_or(path);
            summary.one_line.contains(leaf) || summary.detail.contains(leaf)
        })
    }
}

fn human(path: &str) -> String {
    path.replace('/', " ")
}

/// Strip a `user/<id>/` scope so impact rules match per-user paths too.
fn unscoped(path: &str) -> &str {
    if let Some(rest) = path.strip_prefix("user/") {
        if let Some((_user, suffix)) = rest.split_once('/') {
            return suffix;
        }
    }
    path
}

fn is_ui_tree(path: &str) -> bool {
    let mut segments = path.rsplit('/');
    segments.next() == Some("tree") && segments.next() == Some("ui")
}

fn is_keymap(path: &str) -> bool {
    let mut segments = path.rsplit('/');
    segments.next() == Some("keymap") && segments.next() == Some("input")
}

fn is_inference_model(path: &str) -> bool {
    path.starts_with("system/inference/models/")
}

fn model_id(path: &str) -> &str {
    path.strip_prefix("system/inference/models/")
        .unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_change_reads_naturally() {
        let summary = RuleBasedSummarizer.summarize(
            "Make the clock sit top-right",
            &[Operation::Set {
                path: "desktop/clock/position".into(),
                value: "top-right".into(),
            }],
        );
        assert_eq!(summary.one_line, "Set desktop clock position to top-right.");
        assert!(summary.detail.contains("Requested:"));
        assert!(summary.verified);
    }

    #[test]
    fn impact_flags_ui_and_services() {
        let impact = RuleBasedSummarizer.impact(&[
            Operation::Set {
                path: "theme/mode".into(),
                value: "dark".into(),
            },
            Operation::Set {
                path: "service/network/restart".into(),
                value: "true".into(),
            },
            Operation::Set {
                path: "kernel/args".into(),
                value: "quiet".into(),
            },
        ]);
        assert!(impact.ui_changed);
        assert_eq!(impact.services_restarted, 1);
        assert!(impact.reboot_required);
    }

    #[test]
    fn impact_understands_user_scoped_paths() {
        let impact = RuleBasedSummarizer.impact(&[
            Operation::Set {
                path: "user/ada/theme/mode".into(),
                value: "dark".into(),
            },
            Operation::Set {
                path: "user/ada/desktop/clock/position".into(),
                value: "top-right".into(),
            },
            Operation::Set {
                path: "system/services/network/state".into(),
                value: "down".into(),
            },
        ]);
        assert!(impact.ui_changed);
        assert_eq!(impact.services_restarted, 1);
        assert!(!impact.reboot_required);
    }

    #[test]
    fn verify_rejects_unrelated_summary() {
        let summary = Summary::new("Changed the wallpaper.", "", "rule-based", true);
        let operations = [Operation::Set {
            path: "desktop/clock/position".into(),
            value: "top-right".into(),
        }];
        assert!(!RuleBasedSummarizer.verify(&summary, &operations));
    }

    #[test]
    fn ui_resources_get_plain_summaries() {
        let tree = RuleBasedSummarizer.summarize(
            "Rebuild the panel",
            &[Operation::Set {
                path: "user/ada/ui/tree".into(),
                value: "{}".into(),
            }],
        );
        assert_eq!(tree.one_line, "Rebuilt the UI tree.");
        assert!(tree.verified);

        let keymap = RuleBasedSummarizer.summarize(
            "Rebind",
            &[Operation::Set {
                path: "user/ada/input/keymap".into(),
                value: "{}".into(),
            }],
        );
        assert_eq!(keymap.one_line, "Updated the keymap.");
        assert!(keymap.verified);
    }

    #[test]
    fn inference_models_get_plain_summaries() {
        let summary = RuleBasedSummarizer.summarize(
            "Load the simulator",
            &[Operation::Set {
                path: "system/inference/models/sim/tiny".into(),
                value: "{}".into(),
            }],
        );
        assert_eq!(summary.one_line, "Updated model sim/tiny.");
        assert!(summary.verified);

        let mut spec = agentic_inference::ModelSpec::example();
        spec.loaded = true;
        let loaded = RuleBasedSummarizer.summarize(
            "Load the simulator",
            &[Operation::Set {
                path: "system/inference/models/sim/tiny".into(),
                value: spec.render(),
            }],
        );
        assert_eq!(loaded.one_line, "Loaded model sim/tiny.");
        assert!(loaded.verified);
    }
}
