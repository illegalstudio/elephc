//! Purpose:
//! Emits PHP `intval` conversion calls from scalar expressions.
//! Keeps PHP conversion lowering close to string builtins because string parsing is the dominant path.
//!
//! Called from:
//! - `crate::codegen::builtins::strings::emit()`.
//!
//! Key details:
//! - Conversion behavior must stay aligned with type-checker assumptions for scalar-to-int coercion.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::abi;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("intval()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    match ty {
        PhpType::Str => {
            // -- convert string to integer --
            abi::emit_call_label(emitter, "__rt_atoi");                         // parse the current string result through the target-aware atoi runtime helper
        }
        PhpType::Mixed | PhpType::Union(_) => {
            // -- coerce a boxed Mixed cell to int per PHP's casting rules --
            abi::emit_call_label(emitter, "__rt_mixed_cast_int");                // dispatch on the runtime cell tag and return the integer payload (or coerced equivalent)
        }
        _ => {}
    }
    Some(PhpType::Int)
}
