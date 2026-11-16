//! Purpose:
//! Declares the internal dynamic abstract-class predicate used by the tokenizer.
//!
//! Called from:
//! - The tokenizer prelude before late-static token construction.
//!
//! Key details:
//! - The result is derived from the AOT class table and does not construct an object.
//! - `internal: true` keeps this compiler primitive out of PHP-visible builtin catalogs.

use crate::builtins::semantics::{
    internal_eir_semantics, BuiltinLoweringContext, BuiltinResultOwnership,
    LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::ir::{Effects, Op};

builtin! {
    contract: "__elephc_class_is_abstract",
    semantics: internal_eir_semantics(lower, Effects::PURE, BuiltinResultOwnership::NonHeap),
}

/// Lowers the class-name predicate to the dedicated AOT metadata EIR primitive.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, crate::builtins::semantics::BuiltinLoweringError> {
    Ok(ctx.emit_value(
        Op::DynamicClassIsAbstract,
        vec![call.operand(0)?],
        None,
        call.result_type.clone(),
        Op::DynamicClassIsAbstract.default_effects(),
        Some(call.span),
    ))
}
