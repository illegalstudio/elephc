//! Purpose:
//! Declares the internal builtin `__elephc_refl_callable_name`: the PHP name a callable carries.
//!
//! Called from:
//! - The synthesized `ReflectionFunction` constructor, which finds its function row by the name
//!   of the closure it was given.
//!
//! Key details:
//! - Reads the descriptor of a boxed callable; a value that is not a callable answers "".
//! - `internal: true` keeps this compiler primitive out of PHP-visible builtin catalogs.

use crate::builtins::semantics::{
    internal_eir_semantics, BuiltinLoweringContext, BuiltinResultOwnership,
    LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::ir::{Effects, Op};
use crate::types::PhpType;

builtin! {
    contract: "__elephc_refl_callable_name",
    check: check,
    semantics: internal_eir_semantics(
        lower,
        Effects::READS_GLOBAL,
        BuiltinResultOwnership::Borrowed,
    ),
}

/// Checks the operand and types the result.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    for arg in cx.args.iter() {
        cx.checker.infer_type(arg, cx.env)?;
    }
    Ok(PhpType::Str)
}

/// Lowers the call to its dedicated EIR primitive.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, crate::builtins::semantics::BuiltinLoweringError> {
    Ok(ctx.emit_value(
        Op::ReflCallableName,
        vec![call.operand(0)?],
        None,
        PhpType::Str,
        Op::ReflCallableName.default_effects(),
        Some(call.span),
    ))
}
