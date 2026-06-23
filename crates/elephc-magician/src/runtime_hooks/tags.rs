//! Purpose:
//! Maps EvalIR binary operators onto generated runtime wrapper opcode tables.
//! The runtime wrappers expect compact numeric tags rather than Rust enum
//! discriminants.
//!
//! Called from:
//! - `crate::runtime_hooks::ops` comparison and bitwise operations.
//!
//! Key details:
//! - Non-matching operators map to zero because callers only pass matching groups.

use crate::eval_ir::EvalBinOp;

/// Maps an EvalIR comparison operator to the bridge ABI opcode.
#[cfg(not(test))]
pub(super) fn compare_op_tag(op: EvalBinOp) -> u64 {
    match op {
        EvalBinOp::LooseEq => 0,
        EvalBinOp::LooseNotEq => 1,
        EvalBinOp::Lt => 2,
        EvalBinOp::LtEq => 3,
        EvalBinOp::Gt => 4,
        EvalBinOp::GtEq => 5,
        EvalBinOp::StrictEq => 6,
        EvalBinOp::StrictNotEq => 7,
        EvalBinOp::Add
        | EvalBinOp::Sub
        | EvalBinOp::Mul
        | EvalBinOp::Div
        | EvalBinOp::Mod
        | EvalBinOp::Pow
        | EvalBinOp::BitAnd
        | EvalBinOp::BitOr
        | EvalBinOp::BitXor
        | EvalBinOp::ShiftLeft
        | EvalBinOp::ShiftRight
        | EvalBinOp::Concat
        | EvalBinOp::Spaceship
        | EvalBinOp::LogicalAnd
        | EvalBinOp::LogicalOr
        | EvalBinOp::LogicalXor => 0,
    }
}

/// Maps bitwise EvalIR operators onto the generated runtime wrapper opcode table.
#[cfg(not(test))]
pub(super) fn bitwise_op_tag(op: EvalBinOp) -> u64 {
    match op {
        EvalBinOp::BitAnd => 0,
        EvalBinOp::BitOr => 1,
        EvalBinOp::BitXor => 2,
        EvalBinOp::ShiftLeft => 3,
        EvalBinOp::ShiftRight => 4,
        EvalBinOp::Add
        | EvalBinOp::Sub
        | EvalBinOp::Mul
        | EvalBinOp::Div
        | EvalBinOp::Mod
        | EvalBinOp::Pow
        | EvalBinOp::Concat
        | EvalBinOp::LogicalAnd
        | EvalBinOp::LogicalOr
        | EvalBinOp::LogicalXor
        | EvalBinOp::LooseEq
        | EvalBinOp::LooseNotEq
        | EvalBinOp::StrictEq
        | EvalBinOp::StrictNotEq
        | EvalBinOp::Lt
        | EvalBinOp::LtEq
        | EvalBinOp::Gt
        | EvalBinOp::GtEq
        | EvalBinOp::Spaceship => 0,
    }
}
