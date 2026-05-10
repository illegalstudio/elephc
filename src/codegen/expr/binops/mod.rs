//! Purpose:
//! Dispatches binary operator lowering to arithmetic, comparison, array-union, and target helper modules.
//! Selects PHP-compatible result types and special operator paths before instruction emission.
//!
//! Called from:
//! - `crate::codegen::expr::emit_expr()`
//!
//! Key details:
//! - Operator precedence is resolved by the parser; this layer preserves PHP value semantics and result registers.

use super::super::context::Context;
use super::super::data_section::DataSection;
use super::super::emit::Emitter;
use super::compare::{emit_null_coalesce, emit_strict_compare};
use super::{BinOp, Expr, PhpType};

mod arithmetic;
mod array_union;
mod comparison;
mod target;

use arithmetic::{
    emit_concat_binop, emit_logical_binop, emit_numeric_binop, emit_pow_binop,
};
use array_union::{emit_array_union_binop, is_array_union_candidate};
use comparison::{emit_loose_equality_binop, emit_order_compare_binop, emit_spaceship_binop};

pub(super) fn emit_binop(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    match op {
        BinOp::And | BinOp::Or | BinOp::Xor => {
            emit_logical_binop(left, op, right, emitter, ctx, data)
        }
        BinOp::Pow => emit_pow_binop(left, right, emitter, ctx, data),
        BinOp::Add if is_array_union_candidate(left, right, ctx) => {
            emit_array_union_binop(left, right, emitter, ctx, data)
        }
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
            emit_numeric_binop(left, op, right, emitter, ctx, data)
        }
        BinOp::Eq | BinOp::NotEq => emit_loose_equality_binop(left, op, right, emitter, ctx, data),
        BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq => {
            emit_order_compare_binop(left, op, right, emitter, ctx, data)
        }
        BinOp::StrictEq | BinOp::StrictNotEq => {
            emit_strict_compare(left, op, right, emitter, ctx, data)
        }
        BinOp::Concat => emit_concat_binop(left, right, emitter, ctx, data),
        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::ShiftLeft | BinOp::ShiftRight => {
            arithmetic::emit_bitwise_binop(left, op, right, emitter, ctx, data)
        }
        BinOp::Spaceship => emit_spaceship_binop(left, right, emitter, ctx, data),
        BinOp::NullCoalesce => emit_null_coalesce(left, right, emitter, ctx, data),
    }
}
