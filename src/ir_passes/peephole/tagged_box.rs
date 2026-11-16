//! Purpose:
//! Scalar box elision peephole: drops a `MixedBox` of a scalar that nothing needs as a heap cell.
//!
//! Called from:
//! - `crate::ir_passes::peephole::Peephole::run` via `collect`.
//!
//! Key details:
//! - A box whose only uses are its own `Release`s is dead: allocate-then-free with nothing
//!   observing the cell. The box and every release are neutralized.
//! - A box of an inline `?int` (`TaggedScalar`) whose other uses are only `php_rel_cmp`
//!   operands is redirected to the inline value: the ordering comparison builds a tagged
//!   scalar's runtime triple in registers, so the heap cell bought nothing.
//! - The source is a `NonHeap` scalar, so dropping the box can neither leak nor release an
//!   owner, and it is the box's operand, so it dominates every redirected use.
//! - A box flowing into a terminator (branch argument, return) is never touched.

use std::collections::HashSet;

use crate::ir::{Function, InstId, IrType, Op, Ownership, ValueId};

use super::Rewrites;

/// Collects dead scalar boxes and tagged-scalar boxes that feed only ordering comparisons.
pub(super) fn collect(function: &Function, rewrites: &mut Rewrites) {
    let mut terminator_uses = HashSet::new();
    for block in &function.blocks {
        if let Some(term) = &block.terminator {
            terminator_uses.extend(term.used_values());
        }
    }
    let mut users: std::collections::HashMap<ValueId, Vec<usize>> = std::collections::HashMap::new();
    for (index, inst) in function.instructions.iter().enumerate() {
        for &operand in &inst.operands {
            users.entry(operand).or_default().push(index);
        }
    }
    for (index, inst) in function.instructions.iter().enumerate() {
        if inst.op != Op::MixedBox {
            continue;
        }
        let (Some(result), Some(&source)) = (inst.result, inst.operands.first()) else {
            continue;
        };
        if terminator_uses.contains(&result) {
            continue;
        }
        let Some(source_value) = function.value(source) else {
            continue;
        };
        if source_value.ownership != Ownership::NonHeap
            || !matches!(source_value.ir_type, IrType::I64 | IrType::F64 | IrType::TaggedScalar)
        {
            continue;
        }
        let uses = users.get(&result).map(Vec::as_slice).unwrap_or(&[]);
        let releases: Vec<usize> = uses
            .iter()
            .copied()
            .filter(|&user| function.instructions[user].op == Op::Release)
            .collect();
        let comparisons = uses.len() - releases.len();
        let foldable = comparisons == 0
            || (source_value.ir_type == IrType::TaggedScalar
                && uses.iter().all(|&user| {
                    let user = &function.instructions[user];
                    user.op == Op::Release
                        || (user.op == Op::PhpRelCmp && user.operands.len() == 2)
                }));
        if !foldable {
            continue;
        }
        rewrites.nops.push(InstId::from_raw(index as u32));
        rewrites
            .nops
            .extend(releases.into_iter().map(|user| InstId::from_raw(user as u32)));
        if comparisons > 0 {
            rewrites.rauw.insert(result, source);
        }
    }
}
