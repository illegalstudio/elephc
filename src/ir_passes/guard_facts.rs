//! Purpose:
//! Simplifies repeated branches whose boolean SSA condition was already tested on every path.
//!
//! Called from:
//! - `branch_simplify`, under its exception-handler exclusion.
//!
//! Key details:
//! - Only unique incoming guard edges contribute facts, so joins and loop re-entry fail closed.
//! - Facts concern immutable SSA values, never mutable PHP storage or fresh heap reads.

use std::collections::{HashMap, HashSet};

use crate::ir::{Function, Terminator};

/// Reuses conditions whose selected single-predecessor successor dominates the later branch.
pub(super) fn fold(function: &mut Function) -> bool {
    let mut seen = HashSet::new();
    let mut repeated = HashSet::new();
    for block in &function.blocks {
        if let Some(Terminator::CondBr { cond, .. }) = block.terminator {
            if !seen.insert(cond) { repeated.insert(cond); }
        }
    }
    if repeated.is_empty() { return false; }
    let predecessors = super::cfg::predecessors(function);
    let dominance = super::dominance::compute_dominance(function);
    let mut conditions: HashMap<_, Vec<_>> = HashMap::new();
    for block in &function.blocks {
        let Some(Terminator::CondBr { cond, then_target, else_target, .. }) = block.terminator else { continue; };
        if !repeated.contains(&cond) { continue; }
        if then_target == else_target { continue; }
        for (target, taken) in [(then_target, true), (else_target, false)] {
            if predecessors[target.as_raw() as usize] == [block.id]
                && dominance.strictly_dominates(block.id, target)
            {
                conditions.entry(cond).or_default().push((target, taken));
            }
        }
    }
    let mut changed = false;
    for block in &mut function.blocks {
        let Some(Terminator::CondBr { cond, then_target, then_args, else_target, else_args }) = block.terminator.as_ref() else { continue; };
        let Some(taken) = conditions.get(cond).and_then(|facts| facts.iter()
            .find(|(target, _)| dominance.dominates(*target, block.id)).map(|(_, taken)| *taken))
        else { continue; };
        block.terminator = Some(if taken {
            Terminator::Br { target: *then_target, args: then_args.clone() }
        } else {
            Terminator::Br { target: *else_target, args: else_args.clone() }
        });
        changed = true;
    }
    changed
}
