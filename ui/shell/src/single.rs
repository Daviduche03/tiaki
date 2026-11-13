//! Single-instance guard for shell popups.
//!
//! Shell menus are separate processes so they can escape the bar's 30px layer
//! surface. The launcher and Control Center toggle closed on a second launch;
//! choosing another menu replaces the open one.

use std::path::{Path, PathBuf};
use std::time::Duration;

pub struct Instance;

impl Instance {
    pub fn acquire(name: &str, marker: &str, replace: bool) -> Option<Self> {        let base = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
        let path = PathBuf::from(base).join(format!("{name}.lock"));

        if Self::create(&path) {
            return Some(Self);
        }

        let pid: Option<u32> = std::fs::read_to_string(path.join("pid"))
            .ok()
            .and_then(|text| text.trim().parse().ok());
        if let Some(pid) = pid {
            if Self::is_ours(pid, marker) {
                let _ = std::process::Command::new("kill")
                    .arg(pid.to_string())
                    .status();
                if !replace {
                    return None;
                }
                // Wait for the old process to exit and release its Wayland layer surface
                for _ in 0..10 {
                    std::thread::sleep(Duration::from_millis(30));
                    let alive = std::process::Command::new("kill")
                        .args(["-0", &pid.to_string()])
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false);
                    if !alive {
                        break;
                    }
                }
            }
        }

        let _ = std::fs::remove_dir_all(&path);
        if Self::create(&path) {
            return Some(Self);
        }
        None
    }

    fn is_ours(pid: u32, marker: &str) -> bool {
        let Ok(command) = std::fs::read_to_string(format!("/proc/{pid}/cmdline")) else {
            return false;
        };
        let exe = command.split('\0').next().unwrap_or_default();
        let is_shell = exe.ends_with("tiaki-shell")
            || exe.ends_with("tiaki-shell")
            || exe.ends_with("tiaki-shell (deleted)")
            || exe.ends_with("tiaki-shell (deleted)");
        is_shell && command.split('\0').any(|arg| arg == marker)
    }

    fn create(path: &Path) -> bool {
        if std::fs::create_dir(path).is_err() {
            return false;
        }
        let _ = std::fs::write(path.join("pid"), std::process::id().to_string());
        true
    }

    /// Close the popup holding `lock_name` (e.g. "tiaki-notify"), if it is
    /// one of ours. Popups dismiss each other this way: opening Control
    /// Center closes Notifications and vice versa.
    pub fn dismiss(lock_name: &str, marker: &str) {
        let base = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
        let path = PathBuf::from(base).join(format!("{lock_name}.lock"));

        if let Ok(text) = std::fs::read_to_string(path.join("pid")) {
            if let Ok(pid) = text.trim().parse::<u32>() {
                if Self::is_ours(pid, marker) {
                    let _ = std::process::Command::new("kill")
                        .arg(pid.to_string())
                        .status();
                }
            }
        }
        let _ = std::fs::remove_dir_all(&path);
    }
}
