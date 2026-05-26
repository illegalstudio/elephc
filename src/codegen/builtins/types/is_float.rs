//! Purpose:
//! Emits PHP `is_float` type predicate calls.
//! Inspects static or boxed runtime value representation and returns a PHP boolean.
//!
//! Called from:
//! - `crate::codegen::builtins::types::emit()`.
//!
//! Key details:
//! - Predicate behavior must match PHP sentinel, Mixed tag, and object/interface layout conventions.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::abi;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `is_float` builtin call.
///
/// Inspects the compile-time type of `args[0]`. Returns a PHP boolean in the
/// active integer result register: 1 if the resolved type is `PhpType::Float`,
/// 0 otherwise. Always returns `Some(PhpType::Bool)`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("is_float()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    // -- return true/false based on compile-time type --
    let val = if ty == PhpType::Float { 1 } else { 0 };
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), val);  // return the compile-time type predicate result in the active integer result register
    Some(PhpType::Bool)
}
