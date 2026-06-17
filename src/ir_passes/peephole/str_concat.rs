//! Purpose:
//! String-literal concat folding peephole:
//! `StrConcat(ConstStr a, ConstStr b) -> ConstStr "ab"`.
//!
//! Called from:
//! - `crate::ir_passes::peephole::Peephole::run` via `collect`.
//!
//! Key details:
//! - Only concats whose *both* operands are `ConstStr` literals fold. The
//!   concatenated text is read from the shared data pool; the apply phase interns
//!   the joined literal and converts the instruction in place to `ConstStr`,
//!   marking the result `Persistent` so cleanup never frees the data-section
//!   literal.
//! - Nested concats (`concat(concat(a,b),c)`) fold across driver sweeps: once the
//!   inner concat becomes a `ConstStr`, the outer concat qualifies next sweep.
//! - Dead `ConstStr` operands left behind are removed by later dead-code passes.

use crate::ir::{DataPool, Function, Immediate, InstId, Op};

use super::super::rewrite::defining_instruction;
use super::Rewrites;

/// Collects `StrConcat(ConstStr, ConstStr)` instructions as string-fold rewrites,
/// recording the concatenated literal text for the apply phase to intern.
pub(super) fn collect(function: &Function, data: &DataPool, rewrites: &mut Rewrites) {
    for (index, inst) in function.instructions.iter().enumerate() {
        if inst.op != Op::StrConcat || inst.operands.len() != 2 {
            continue;
        }
        let (Some(lhs), Some(rhs)) = (
            const_str_literal(function, data, inst.operands[0]),
            const_str_literal(function, data, inst.operands[1]),
        ) else {
            continue;
        };
        let mut joined = String::with_capacity(lhs.len() + rhs.len());
        joined.push_str(lhs);
        joined.push_str(rhs);
        rewrites
            .str_folds
            .push((InstId::from_raw(index as u32), joined));
    }
}

/// Returns the literal text of `value` when it is defined by a `ConstStr`
/// instruction with a data-pool immediate, or `None` otherwise.
fn const_str_literal<'a>(
    function: &Function,
    data: &'a DataPool,
    value: crate::ir::ValueId,
) -> Option<&'a str> {
    let inst = defining_instruction(function, value)?;
    if inst.op != Op::ConstStr {
        return None;
    }
    let Some(Immediate::Data(id)) = inst.immediate else {
        return None;
    };
    data.strings.get(id.as_raw() as usize).map(String::as_str)
}
