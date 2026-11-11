//! agentd command: sandbox planning, probe, and share enforcement.
//!
//! `sandbox <cmd>` still plans a spec; `sandbox probe` reports whether the
//! kernel can enforce it, and `sandbox bench` measures the split two weights
//! actually produce. Nothing here pretends: an unavailable backend says why.

use std::process::{Command, Stdio};
use std::time::Duration;

use crate::{now_secs, Global, Ledger};
use agentic_capability::{ActionClass, Principal};
use agentic_sandbox::{
    plan_shell, root_from_env, weight_for_shares, CgroupProvisioner, SandboxProfile,
    SandboxProvisioner, SandboxSpec, ShellRequest,
};

pub(crate) fn sandbox(
    engine: &Ledger,
    global: &Global,
    args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() < 2 {
        return Err("usage: agentd sandbox <command|probe|bench> [args...] [--network]".into());
    }

    match args[1].as_str() {
        "probe" => return probe(),
        "bench" => return bench(),
        _ => {}
    }

    let command = args[1].clone();
    let network = args.iter().any(|arg| arg == "--network");
    let command_args: Vec<String> = args[2..]
        .iter()
        .filter(|arg| arg.as_str() != "--network")
        .cloned()
        .collect();

    let resource = shell_resource(&global.principal);
    let authorization = engine.authority().authorize_action(
        &global.principal,
        global.mode,
        ActionClass::ShellExec,
        &resource,
        now_secs(),
    );

    if !authorization.is_clear() {
        println!("sandbox denied");
        println!("  reason: {}", authorization.reason);
        std::process::exit(2);
    }

    let profile = SandboxProfile::from(authorization.sandbox);
    let request = ShellRequest {
        principal: global.principal.clone(),
        profile,
        command,
        args: command_args,
        network,
        timeout_secs: 30,
    };

    let spec = plan_shell(&request);

    println!("sandbox plan");
    println!("  principal: {}", spec.principal);
    println!("  profile:   {}", spec.profile.as_str());
    println!("  command:   {} {}", spec.command, spec.args.join(" "));
    println!("  read-only: {}", spec.read_only.join(", "));
    println!("  writable:  {}", spec.read_write.join(", "));
    if !spec.capabilities.is_empty() {
        println!("  caps:      {}", spec.capabilities.join(", "));
    }
    println!(
        "  limits:    memory={:?} pids={:?} network={} timeout={}s",
        spec.limits.memory_bytes, spec.limits.pids, spec.limits.network, spec.timeout_secs
    );

    let root = root_from_env();
    let report = CgroupProvisioner::probe(&root);
    println!("  backend:   {}", report.backend());
    if let Some(reason) = &report.reason {
        println!("  reason:    {reason}");
    }
    Ok(())
}

fn probe() -> Result<(), Box<dyn std::error::Error>> {
    let root = root_from_env();
    println!("cgroup root: {}", root.display());
    let report = CgroupProvisioner::probe(&root);
    println!(
        "cgroup v2:   {}",
        if report.mounted {
            "mounted at /sys/fs/cgroup"
        } else {
            "not mounted"
        }
    );
    println!(
        "controllers: {}",
        if report.controllers.is_empty() {
            "none delegated".to_string()
        } else {
            report.controllers.join(", ")
        }
    );
    println!("backend:     {}", report.backend());
    if let Some(reason) = &report.reason {
        println!("reason:      {reason}");
    }
    println!();
    println!("weights (shares -> cpu.weight):");
    for shares in [1024, 640, 512, 320, 240, 144, 72, 1] {
        println!("  {shares:>4} shares -> {:>5}", weight_for_shares(shares));
    }
    Ok(())
}

/// Measure enforcement instead of asserting it: busy loops in two cgroups,
/// CPU time read back from the kernel. This is the one command whose output is
/// evidence.
///
/// `cpu.weight` only decides contention, so the loops are pinned to a single
/// CPU when the cpuset controller is delegated. Without cpuset we put more
/// loops in flight than there are CPUs, which creates the same contention.
fn bench() -> Result<(), Box<dyn std::error::Error>> {
    let root = root_from_env();
    let report = CgroupProvisioner::probe(&root);
    if !report.ready {
        return Err(format!(
            "cannot measure: {}",
            report
                .reason
                .unwrap_or_else(|| "cgroup v2 is not ready".into())
        )
        .into());
    }
    let provisioner = CgroupProvisioner::unchecked(&root);
    let mut session = bench_spec("session", 640);
    let mut background = bench_spec("background", 72);
    session.command = "sh".into();
    background.command = "sh".into();
    let session_handle = provisioner.create(session)?;
    let background_handle = provisioner.create(background)?;

    let busy = || {
        Command::new("sh")
            .arg("-c")
            .arg("while :; do :; done")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    };
    // One CPU each is decisive and cheap; otherwise saturate the machine so
    // the weights have something to divide.
    let pinned = pin_to_first_cpu(&root, &session_handle.id).is_ok()
        && pin_to_first_cpu(&root, &background_handle.id).is_ok();
    let loops = if pinned {
        1
    } else {
        std::thread::available_parallelism()
            .map(|count| count.get())
            .unwrap_or(4)
    };
    println!(
        "workload: {} busy loop(s) per group{}",
        loops,
        if pinned { ", pinned to one CPU" } else { "" }
    );

    let mut children = Vec::new();
    for _ in 0..loops {
        children.push((busy()?, &session_handle.id));
        children.push((busy()?, &background_handle.id));
    }
    for (child, id) in &children {
        move_into_cgroup(&root, id, child.id())?;
    }

    std::thread::sleep(Duration::from_millis(2500));

    let session_usage = cpu_usage(&root, &session_handle.id)?;
    let background_usage = cpu_usage(&root, &background_handle.id)?;
    let session_weight = provisioner.read_shares(&session_handle)?;
    let background_weight = provisioner.read_shares(&background_handle)?;

    for (child, _) in &mut children {
        let _ = child.kill();
        let _ = child.wait();
    }
    let _ = provisioner.destroy(&session_handle);
    let _ = provisioner.destroy(&background_handle);

    println!("measured CPU split (2.5s, two busy loops)");
    println!(
        "  session    weight {:>4} -> {:>8} us",
        weight_for_shares(session_weight),
        session_usage
    );
    println!(
        "  background weight {:>4} -> {:>8} us",
        weight_for_shares(background_weight),
        background_usage
    );
    let ratio = if background_usage == 0 {
        f64::INFINITY
    } else {
        session_usage as f64 / background_usage as f64
    };
    println!("  observed ratio: {ratio:.2}:1");
    println!(
        "  expected ratio: {:.2}:1 (weights {}:{})",
        weight_for_shares(session_weight) as f64 / weight_for_shares(background_weight) as f64,
        weight_for_shares(session_weight),
        weight_for_shares(background_weight)
    );
    Ok(())
}

fn bench_spec(label: &str, shares: u32) -> SandboxSpec {
    let mut spec = plan_shell(&ShellRequest::new(
        "user:ada",
        SandboxProfile::UserWrite,
        label,
    ));
    spec.limits.cpu_shares = Some(shares);
    spec
}

fn move_into_cgroup(root: &std::path::Path, id: &str, pid: u32) -> std::io::Result<()> {
    std::fs::write(root.join(id).join("cgroup.procs"), pid.to_string())
}

/// Pin a sandbox to the first CPU the parent cgroup allows.
fn pin_to_first_cpu(root: &std::path::Path, id: &str) -> std::io::Result<()> {
    let dir = root.join(id);
    let cpu = first_cpu(root)?;
    let mem = first_memory_node(root)?;
    std::fs::write(dir.join("cpuset.cpus"), cpu)?;
    std::fs::write(dir.join("cpuset.mems"), mem)
}

fn first_cpu(root: &std::path::Path) -> std::io::Result<String> {
    let text = std::fs::read_to_string(root.join("cpuset.cpus.effective"))
        .or_else(|_| std::fs::read_to_string("/sys/fs/cgroup/cpuset.cpus.effective"))?;
    Ok(first_list_value(&text))
}

fn first_memory_node(root: &std::path::Path) -> std::io::Result<String> {
    let text = std::fs::read_to_string(root.join("cpuset.mems.effective"))
        .or_else(|_| std::fs::read_to_string("/sys/fs/cgroup/cpuset.mems.effective"))?;
    Ok(first_list_value(&text))
}

/// "0-7,9" -> "0"
fn first_list_value(text: &str) -> String {
    let first = text
        .trim()
        .split(',')
        .next()
        .unwrap_or("")
        .split('-')
        .next()
        .unwrap_or("")
        .trim();
    if first.is_empty() {
        "0".to_string()
    } else {
        first.to_string()
    }
}

fn cpu_usage(root: &std::path::Path, id: &str) -> Result<u64, Box<dyn std::error::Error>> {
    let text = std::fs::read_to_string(root.join(id).join("cpu.stat"))?;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("usage_usec ") {
            return Ok(value.trim().parse()?);
        }
    }
    Err(format!("{} has no usage_usec", id).into())
}

pub(crate) fn shell_resource(principal: &str) -> String {
    match Principal::parse(principal) {
        Ok(principal) => match principal.effective() {
            Principal::User(id) => format!("user/{id}/shell"),
            Principal::Service(id) => format!("system/services/{id}/shell"),
            _ => "system/shell".to_string(),
        },
        Err(_) => "system/shell".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::first_list_value;

    #[test]
    fn cpuset_lists_resolve_to_their_first_cpu() {
        assert_eq!(first_list_value("0-7\n"), "0");
        assert_eq!(first_list_value("3-5,7"), "3");
        assert_eq!(first_list_value("2\n"), "2");
        // Empty or unexpected content must not panic.
        assert_eq!(first_list_value(""), "0");
    }
}
