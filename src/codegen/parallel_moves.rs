//! Purpose:
//! Copies scalar branch arguments into their target block parameters with direct moves,
//! ordered as a parallel move, instead of round-tripping every value through the stack.
//!
//! Called from:
//! - `crate::codegen::lower_term` before an edge jumps to a block with parameters.
//!
//! Key details:
//! - Only `int` / `bool` / `float` edges qualify: their copies need no ownership normalization.
//! - A move is emitted once no other pending move still reads its destination; a remaining
//!   cycle is reported back so the caller's general stack-based copy handles it.
//! - Moves only use the result and scratch registers, which are never allocation homes.

use crate::ir::ValueId;
use crate::types::PhpType;

use super::context::FunctionContext;
use super::Result;

/// Where an SSA value lives at a block edge.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Home {
    Register(&'static str),
    Slot(usize),
    Constant,
}

/// Emits direct moves for an all-scalar edge and returns false when it does not qualify.
///
/// On `false` nothing has been emitted, so the caller can fall back to its general copy.
pub(super) fn try_emit_scalar_block_moves(
    ctx: &mut FunctionContext<'_>,
    params: &[ValueId],
    args: &[ValueId],
) -> Result<bool> {
    let mut pending = Vec::with_capacity(args.len());
    for (&param, &arg) in params.iter().zip(args) {
        if !is_plain_scalar(&ctx.value_php_type(param)?) || !is_plain_scalar(&ctx.value_php_type(arg)?) {
            return Ok(false);
        }
        let destination = home(ctx, param);
        if destination == Home::Constant {
            return Ok(false);
        }
        if destination != home(ctx, arg) {
            pending.push((param, arg));
        }
    }
    let Some(order) = sequential_order(ctx, &pending) else {
        return Ok(false);
    };
    for index in order {
        let (param, arg) = pending[index];
        match home(ctx, param) {
            Home::Register(reg) => {
                ctx.load_value_to_reg(arg, reg)?;
            }
            _ => {
                ctx.load_value_to_result(arg)?;
                ctx.store_result_value(param)?;
            }
        }
    }
    Ok(true)
}

/// Returns a move order in which no destination is overwritten while still read, if one exists.
fn sequential_order(ctx: &FunctionContext<'_>, pending: &[(ValueId, ValueId)]) -> Option<Vec<usize>> {
    let homes: Vec<(Home, Home)> = pending
        .iter()
        .map(|&(param, arg)| (home(ctx, param), home(ctx, arg)))
        .collect();
    let mut done = vec![false; pending.len()];
    let mut order = Vec::with_capacity(pending.len());
    while order.len() < pending.len() {
        let ready = (0..pending.len()).find(|&candidate| {
            !done[candidate]
                && (0..pending.len()).all(|other| {
                    other == candidate || done[other] || homes[other].1 != homes[candidate].0
                })
        })?;
        done[ready] = true;
        order.push(ready);
    }
    Some(order)
}

/// Returns true for a scalar whose block-edge copy is a plain one-register move.
fn is_plain_scalar(ty: &PhpType) -> bool {
    matches!(ty.codegen_repr(), PhpType::Int | PhpType::Bool | PhpType::Float)
}

/// Returns the home of `value`: a constant recipe, an allocated register, or a stack slot.
fn home(ctx: &FunctionContext<'_>, value: ValueId) -> Home {
    if ctx.allocation.rematerialized(value).is_some() {
        Home::Constant
    } else if let Some(reg) = ctx.allocation.register_of(value) {
        Home::Register(reg)
    } else {
        ctx.placement.slot(value).map_or(Home::Constant, Home::Slot)
    }
}
