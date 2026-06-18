//! Purpose:
//! Redundant `Move`/`Borrow` cleanup peephole: fold a pure ownership-forwarding
//! op to its operand when it changes nothing.
//!
//! Called from:
//! - `crate::ir_passes::peephole::Peephole::run` via `collect`.
//!
//! Key details:
//! - `Move`/`Borrow` lower as pure forwarding (`lower_forward` copies the operand
//!   into the result slot). They are folded only when the result and operand
//!   share ownership and ir/php type, so redirecting uses to the operand cannot
//!   shift cleanup responsibility. Ownership-changing forwards are left intact.
//! - Current lowering does not emit these opcodes; the rewrite keeps them correct
//!   if a future lowering path introduces them, and is exercised by unit tests.
//! - The operand defines the forward, so it dominates every use of the result —
//!   fold-to-operand is dominance-safe.

use crate::ir::{Function, InstId, Op};

use super::Rewrites;

/// Collects redundant `Move`/`Borrow` ops as fold-to-operand rewrites when the
/// forward changes neither ownership nor type.
pub(super) fn collect(function: &Function, rewrites: &mut Rewrites) {
    for (index, inst) in function.instructions.iter().enumerate() {
        if !matches!(inst.op, Op::Move | Op::Borrow) {
            continue;
        }
        let Some(result) = inst.result else {
            continue;
        };
        let Some(&source) = inst.operands.first() else {
            continue;
        };
        let (Some(source_value), Some(result_value)) =
            (function.value(source), function.value(result))
        else {
            continue;
        };
        if source_value.ownership != result_value.ownership
            || source_value.ir_type != result_value.ir_type
            || source_value.php_type != result_value.php_type
        {
            continue;
        }
        rewrites.rauw.insert(result, source);
        rewrites.nops.push(InstId::from_raw(index as u32));
    }
}
