//! agentd command: Grants and revocations.
use crate::{Global, Ledger};
use agentic_capability::{ActionClass, Grant, Principal};

pub(crate) fn grant(
    engine: &mut Ledger,
    global: &Global,
    args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() < 4 {
        return Err(
            "usage: agentd grant <principal> <action> <resource> [--approval] [--max-per-minute N]"
                .into(),
        );
    }

    let holder = Principal::parse(&args[1])?;
    let action =
        ActionClass::parse(&args[2]).ok_or_else(|| format!("unknown action '{}'", args[2]))?;
    let resource = args[3].clone();

    if !engine.authority().can_grant(&global.principal, &resource) {
        return Err(format!(
            "{} may not grant access to '{}'",
            global.principal, resource
        )
        .into());
    }

    let mut requires_approval = false;
    let mut max_per_minute = None;
    let mut index = 4;
    while index < args.len() {
        match args[index].as_str() {
            "--approval" => {
                requires_approval = true;
                index += 1;
            }
            "--max-per-minute" => {
                let value = args
                    .get(index + 1)
                    .ok_or("--max-per-minute needs a value")?;
                max_per_minute = Some(
                    value
                        .parse()
                        .map_err(|_| format!("invalid rate limit '{value}'"))?,
                );
                index += 2;
            }
            other => return Err(format!("unknown argument '{other}'").into()),
        }
    }

    let mut grant = Grant::new(holder, action, resource.clone(), global.principal.clone());
    if requires_approval {
        grant = grant.requiring_approval();
    }
    if let Some(limit) = max_per_minute {
        grant = grant.limited_to(limit);
    }

    engine.authority_mut().add(grant);
    engine.authority().save()?;
    println!(
        "granted {} {} on {}{}{}",
        args[1],
        args[2],
        resource,
        if requires_approval {
            " (approval required)"
        } else {
            ""
        },
        max_per_minute
            .map(|limit| format!(" (max {limit}/min)"))
            .unwrap_or_default()
    );
    Ok(())
}

pub(crate) fn revoke(
    engine: &mut Ledger,
    global: &Global,
    args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() < 4 {
        return Err("usage: agentd revoke <principal> <action> <resource>".into());
    }

    let action =
        ActionClass::parse(&args[2]).ok_or_else(|| format!("unknown action '{}'", args[2]))?;
    let resource = &args[3];

    if !engine.authority().can_grant(&global.principal, resource) {
        return Err(format!(
            "{} may not revoke access to '{}'",
            global.principal, resource
        )
        .into());
    }

    if engine.authority_mut().revoke(&args[1], action, resource) {
        engine.authority().save()?;
        println!("revoked {} {} on {}", args[1], args[2], resource);
        Ok(())
    } else {
        Err(format!("no matching grant for {} on {}", args[1], resource).into())
    }
}
