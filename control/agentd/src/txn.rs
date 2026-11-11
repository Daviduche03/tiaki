//! agentd command: Ledger transactions: check, apply, revert.
use crate::parse_invocation;
use crate::status::{describe_impact, print_report, status_label};
use crate::{now_nanos, Global, Ledger};
use agentic_core_log::Status;
use agentic_transaction::Operation;

pub(crate) fn check(
    engine: &Ledger,
    global: &Global,
    args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let invocation = parse_invocation(args)?;
    let operations = vec![Operation::Set {
        path: invocation.path,
        value: invocation.value,
    }];
    let report = engine.check(
        &global.principal,
        global.mode,
        &invocation.intent,
        &operations,
    );
    print_report(&report);
    if !report.ok {
        std::process::exit(2);
    }
    Ok(())
}

pub(crate) fn apply(
    engine: &mut Ledger,
    global: &Global,
    args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let invocation = parse_invocation(args)?;
    let operations = vec![Operation::Set {
        path: invocation.path,
        value: invocation.value,
    }];

    if invocation.dry_run {
        let report = engine.check(
            &global.principal,
            global.mode,
            &invocation.intent,
            &operations,
        );
        print_report(&report);
        if !report.ok {
            std::process::exit(2);
        }
        println!("dry run: no changes applied");
        return Ok(());
    }

    let id = format!("txn_{:x}", now_nanos());
    match engine.apply(
        &id,
        &global.principal,
        global.mode,
        &invocation.intent,
        operations,
    ) {
        Ok(transaction) => {
            println!("committed {}", transaction.id);
            println!("  {}", transaction.summary.one_line);
            println!("  impact: {}", describe_impact(&transaction.impact));
        }
        Err(error) => {
            let label = status_label(error.transaction.status);
            println!("{label} {}", error.transaction.id);
            println!("  {}", error.transaction.summary.one_line);
            println!("  reason: {}", error.reason);
            let code = match error.transaction.status {
                Status::PendingApproval => 3,
                _ => 2,
            };
            std::process::exit(code);
        }
    }

    Ok(())
}

pub(crate) fn revert(
    engine: &mut Ledger,
    global: &Global,
    args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(id) = args.get(1) else {
        return Err("usage: agentd revert <transaction-id>".into());
    };

    match engine.revert(id, &global.principal, global.mode) {
        Ok(transaction) => {
            println!("reverted {}", id);
            println!("  {}", transaction.summary.one_line);
        }
        Err(error) => {
            println!("could not revert {id}");
            println!("  reason: {}", error.reason);
            std::process::exit(2);
        }
    }

    Ok(())
}
