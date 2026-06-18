//! Purpose:
//! Box/unbox cancellation peephole: `MixedUnbox(MixedBox(x)) -> x`.
//!
//! Called from:
//! - `crate::ir_passes::peephole::Peephole::run` via `collect`.
//!
//! Key details:
//! - Only scalar round-trips are folded: both the boxed operand `x` and the
//!   unbox result must be `NonHeap` with matching ir/php type (int/float/bool).
//!   This sidesteps every refcount/ownership hazard from boxing heap payloads
//!   (strings, arrays, objects), where the unbox extracts a borrowed reference
//!   rather than a copy.
//! - `x` is an operand of the box, which defines the unbox's operand, so `x`
//!   dominates the unbox and all its uses — fold-to-operand is dominance-safe.

use crate::ir::{Function, InstId, Op, Ownership};

use super::super::rewrite::defining_instruction;
use super::Rewrites;

/// Collects `MixedUnbox(MixedBox(x))` scalar round-trips as fold-to-operand
/// rewrites: the unbox result is redirected to `x` and the unbox neutralized.
pub(super) fn collect(function: &Function, rewrites: &mut Rewrites) {
    for (index, inst) in function.instructions.iter().enumerate() {
        if inst.op != Op::MixedUnbox {
            continue;
        }
        let Some(result) = inst.result else {
            continue;
        };
        let Some(&boxed) = inst.operands.first() else {
            continue;
        };
        let Some(box_inst) = defining_instruction(function, boxed) else {
            continue;
        };
        if box_inst.op != Op::MixedBox {
            continue;
        }
        let Some(&source) = box_inst.operands.first() else {
            continue;
        };
        if !is_scalar_round_trip(function, source, result) {
            continue;
        }
        rewrites.rauw.insert(result, source);
        rewrites.nops.push(InstId::from_raw(index as u32));
    }
}

/// Returns true when boxing `source` and unboxing back to `result` is a pure
/// scalar round-trip: both values are `NonHeap` and share ir/php type, so
/// redirecting `result` to `source` cannot change ownership or refcounting.
fn is_scalar_round_trip(
    function: &Function,
    source: crate::ir::ValueId,
    result: crate::ir::ValueId,
) -> bool {
    let (Some(source_value), Some(result_value)) =
        (function.value(source), function.value(result))
    else {
        return false;
    };
    source_value.ownership == Ownership::NonHeap
        && result_value.ownership == Ownership::NonHeap
        && source_value.ir_type == result_value.ir_type
        && source_value.php_type == result_value.php_type
}
