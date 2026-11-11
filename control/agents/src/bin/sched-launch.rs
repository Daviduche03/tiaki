//! `sched-launch` — start a real process inside a real cgroup.
//!
//! This is how the desktop connects to the scheduler. Every app the session
//! starts goes through here:
//!
//! ```sh
//! sched-launch --class foreground -- foot
//! sched-launch --class background -- tiaki-wallpaper init
//! sched-launch --class session    -- opencode-abi.py user:ada
//! ```
//!
//! No daemon holds the state. Each launcher creates a cgroup whose name
//! carries its class (`session-<pid>`, `fg-<pid>`, `bg-<pid>`), moves its child
//! into it, recomputes every sibling's weight from the live set, and cleans up
//! on exit. Restarting the session restarts the whole scheduler; there is
//! nothing to desynchronise.
//!
//! When the kernel cannot enforce (no delegation, read-only cgroupfs), the
//! launcher says so once on stderr and runs the command unsupervised rather
//! than pretending it was scheduled.

use std::process::{Command, ExitCode};

use agentic_agents::launch::{rebalance, Class};
use agentic_sandbox::cgroup::{root_from_env, CgroupProvisioner};
use agentic_sandbox::{plan_shell, SandboxProvisioner, SandboxSpec, ShellRequest};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse(&args) {
        Ok(Invocation {
            class,
            command,
            forward,
        }) => run(class, &command, &forward),
        Err(message) => {
            eprintln!("sched-launch: {message}");
            eprintln!(
                "usage: sched-launch [--class foreground|background|session] [--name <label>] -- <command> [args...]"
            );
            ExitCode::FAILURE
        }
    }
}

struct Invocation {
    class: Class,
    command: String,
    forward: Vec<String>,
}

fn parse(args: &[String]) -> Result<Invocation, String> {
    let mut class = Class::Foreground;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--class" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "--class needs a value".to_string())?;
                class = Class::parse(value).ok_or_else(|| format!("unknown class '{value}'"))?;
                index += 2;
            }
            "--name" => {
                // Accepted for compatibility; the cgroup name stays derived
                // from the class and pid so the live set is unambiguous.
                if args.get(index + 1).is_none() {
                    return Err("--name needs a value".to_string());
                }
                index += 2;
            }
            "--" => {
                index += 1;
                break;
            }
            other => return Err(format!("unexpected argument '{other}'")),
        }
    }
    let command = args
        .get(index)
        .ok_or_else(|| "no command given".to_string())?
        .clone();
    Ok(Invocation {
        class,
        command,
        forward: args[index + 1..].to_vec(),
    })
}

fn run(class: Class, command: &str, forward: &[String]) -> ExitCode {
    let root = root_from_env();
    let provisioner = CgroupProvisioner::new(&root);
    let report = CgroupProvisioner::probe(&root);

    if !report.ready {
        eprintln!(
            "sched-launch: running '{command}' unsupervised ({})",
            report.reason.unwrap_or_else(|| "no cgroup".into())
        );
        return exec(command, forward);
    }

    let spec = launch_spec(class, command);
    let name = format!("{}-{}", class.prefix(), std::process::id());
    let handle = match provisioner.create_named(spec, &name) {
        Ok(handle) => handle,
        Err(error) => {
            eprintln!("sched-launch: no sandbox ({error}); running unsupervised");
            return exec(command, forward);
        }
    };

    let mut child = match Command::new(command).args(forward).spawn() {
        Ok(child) => child,
        Err(error) => {
            eprintln!("sched-launch: cannot start '{command}': {error}");
            let _ = provisioner.destroy(&handle);
            return ExitCode::FAILURE;
        }
    };
    if let Err(error) = provisioner.attach(&handle, child.id()) {
        eprintln!("sched-launch: could not attach {:?}: {error}", child.id());
    }
    if let Err(error) = rebalance(&root) {
        eprintln!("sched-launch: rebalance failed: {error}");
    }

    let status = child.wait();
    let _ = provisioner.destroy(&handle);
    if let Err(error) = rebalance(&root) {
        eprintln!("sched-launch: rebalance after exit failed: {error}");
    }

    match status {
        Ok(status) => ExitCode::from(exit_code(status.code())),
        Err(error) => {
            eprintln!("sched-launch: lost '{command}': {error}");
            ExitCode::FAILURE
        }
    }
}

fn exec(command: &str, forward: &[String]) -> ExitCode {
    match Command::new(command).args(forward).status() {
        Ok(status) => ExitCode::from(exit_code(status.code())),
        Err(error) => {
            eprintln!("sched-launch: cannot start '{command}': {error}");
            ExitCode::FAILURE
        }
    }
}

fn exit_code(code: Option<i32>) -> u8 {
    code.unwrap_or(1).clamp(0, 255) as u8
}

fn launch_spec(class: Class, command: &str) -> SandboxSpec {
    let mut spec = plan_shell(&ShellRequest::new("user:local", class.profile(), command));
    // A running agent session gets room for a working set; background helpers do not.
    match class {
        Class::Session => {
            spec.limits.memory_bytes = Some(4 * 1024 * 1024 * 1024);
            spec.limits.pids = Some(512);
        }
        Class::Foreground => {
            spec.limits.memory_bytes = Some(3 * 1024 * 1024 * 1024);
            spec.limits.pids = Some(256);
        }
        Class::Background => {
            spec.limits.memory_bytes = Some(768 * 1024 * 1024);
            spec.limits.pids = Some(96);
        }
    }
    spec
}

/// Recompute every live sandbox's weight from the current membership.
///
/// This is the whole scheduler at runtime: count the members, run the same
/// `allocate` the tests use, split each pool evenly, write the weights.

#[cfg(test)]
mod tests {
    use super::*;
    use agentic_agents::{allocate, split_evenly};
    use agentic_sandbox::SandboxProfile;

    #[test]
    fn classes_round_trip_through_directory_names() {
        for class in [Class::Session, Class::Foreground, Class::Background] {
            let name = format!("{}-1234", class.prefix());
            assert_eq!(Class::from_dir(&name), Some(class), "{name}");
        }
        assert_eq!(Class::from_dir("sandbox_1"), None);
        assert_eq!(Class::from_dir("tiaki"), None);
        assert_eq!(Class::from_dir("keep"), None);
    }

    #[test]
    fn parsing_accepts_classes_and_requires_a_command() {
        let invocation = parse(&[
            "--class".into(),
            "background".into(),
            "--".into(),
            "sleep".into(),
            "5".into(),
        ])
        .expect("parse");
        assert_eq!(invocation.class, Class::Background);
        assert_eq!(invocation.command, "sleep");
        assert_eq!(invocation.forward, vec!["5".to_string()]);

        assert!(parse(&["--class".into(), "nope".into(), "--".into(), "x".into()]).is_err());
        assert!(parse(&["--class".into()]).is_err());
        assert!(parse(&[]).is_err());
    }

    #[test]
    fn allocation_matches_the_supervisor_policy() {
        // One agent, one foreground app, one background helper: the agent
        // pool wins, background keeps its floor.
        let allocation = allocate(1, 1, 1, false);
        assert_eq!(allocation.session, 640);
        assert_eq!(allocation.inference, 0);
        let fg = split_evenly(allocation.foreground, 1);
        let bg = split_evenly(allocation.background, 1);
        assert_eq!(fg, vec![240]);
        assert_eq!(bg, vec![144]);
        assert!(bg.iter().all(|shares| *shares >= 128));
    }

    #[test]
    fn a_lone_agent_gets_the_whole_machine() {
        let allocation = allocate(1, 0, 0, false);
        assert_eq!(allocation.session, 1024);
    }

    #[test]
    fn a_burst_demotes_agents_without_stopping_them() {
        let allocation = allocate(2, 1, 1, true);
        assert_eq!(allocation.inference, 512);
        assert_eq!(allocation.session, 128);
        assert_eq!(split_evenly(allocation.session, 2), vec![64, 64]);
    }

    #[test]
    fn spec_limits_follow_the_class() {
        let agent = launch_spec(Class::Session, "opencode");
        let background = launch_spec(Class::Background, "tiaki-wallpaper");
        assert!(agent.limits.memory_bytes > background.limits.memory_bytes);
        assert_eq!(agent.profile, SandboxProfile::UserWrite);
    }
}
