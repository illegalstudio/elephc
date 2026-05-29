//! Purpose:
//! Emits PHP `defined()` checks for constants known to the ahead-of-time compiler.
//! Connects predefined and user-discovered constants to PHP boolean introspection.
//!
//! Called from:
//! - `crate::codegen::builtins::system::emit()`.
//!
//! Key details:
//! - AOT mode requires a string-literal constant name so the result can be resolved during codegen.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// Emits a boolean result for `defined("CONSTANT")`.
///
/// The type checker rejects non-literal source calls. This emitter still handles
/// non-literal generated calls defensively by evaluating the argument and
/// returning `false` instead of panicking during deferred codegen.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    _data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("defined()");
    let constant_name = match &args[0].kind {
        ExprKind::StringLiteral(name) => name.trim_start_matches('\\'),
        _ => {
            emit_expr(&args[0], emitter, ctx, _data);
            abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
            return Some(PhpType::Bool);
        }
    };
    let exists = ctx.constants.contains_key(constant_name);
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), i64::from(exists));
    Some(PhpType::Bool)
}
