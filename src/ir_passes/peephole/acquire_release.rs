//! Purpose:
//! Paired acquire/release cancellation peephole: drop an `Acquire` whose result
//! is consumed only by a matching `Release`.
//!
//! Called from:
//! - `crate::ir_passes::peephole::Peephole::run` via `collect`.
//!
//! Key details:
//! - `Acquire` persists/increfs a value; `Release` frees/decrefs it. When the
//!   acquired result is used exactly once and that single use is its `Release`,
//!   the pair is refcount-neutral on every path (allocate-then-free or
//!   incref-then-decref with nothing observing the copy in between), and the
//!   acquired operand is untouched. Both instructions are neutralized.
//! - The single-use guard makes this safe regardless of how far apart the two
//!   ops are or which path the `Release` sits on: the value flows to exactly one
//!   `Release`, so removing both cannot leak or double-free.

use std::collections::HashMap;

use crate::ir::{Function, InstId, Op, ValueId};

use super::super::rewrite::count_value_uses;
use super::Rewrites;

/// Collects single-use `Acquire`/`Release` pairs, neutralizing both and
/// redirecting the (now unused) acquired result to the acquired operand.
pub(super) fn collect(function: &Function, rewrites: &mut Rewrites) {
    let uses = count_value_uses(function);
    let release_of = release_targets(function);
    for (index, inst) in function.instructions.iter().enumerate() {
        if inst.op != Op::Acquire {
            continue;
        }
        let Some(acquired) = inst.result else {
            continue;
        };
        if uses.get(&acquired).copied().unwrap_or(0) != 1 {
            continue;
        }
        let Some(&release_index) = release_of.get(&acquired) else {
            continue;
        };
        let Some(&source) = inst.operands.first() else {
            continue;
        };
        rewrites.rauw.insert(acquired, source);
        rewrites.nops.push(InstId::from_raw(index as u32));
        rewrites.nops.push(release_index);
    }
}

/// Maps each value released by a `Release` instruction to that instruction's id.
/// A value released more than once keeps the first mapping; the single-use guard
/// in `collect` means only single-release values are ever cancelled anyway.
fn release_targets(function: &Function) -> HashMap<ValueId, InstId> {
    let mut targets = HashMap::new();
    for (index, inst) in function.instructions.iter().enumerate() {
        if inst.op != Op::Release {
            continue;
        }
        if let Some(&released) = inst.operands.first() {
            targets.entry(released).or_insert(InstId::from_raw(index as u32));
        }
    }
    targets
}
