//! Purpose:
//! Declares the internal builtin `__elephc_refl_word`: one word of a reflection table row.
//!
//! Called from:
//! - The synthesized `ReflectionClass` / `ReflectionObject` / `ReflectionEnum` constructors and
//!   member fillers, which read the module's reflection tables through it.
//!
//! Key details:
//! - The tables are immutable data the compiler emits (`reflection_table`); nothing here
//!   allocates or runs PHP.
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
    contract: "__elephc_refl_word",
    check: check,
    semantics: internal_eir_semantics(
        lower,
        Effects::READS_GLOBAL,
        BuiltinResultOwnership::NonHeap,
    ),
}

/// Checks the operands and types the result.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    for arg in cx.args.iter() {
        cx.checker.infer_type(arg, cx.env)?;
    }
    Ok(PhpType::Int)
}

/// Lowers the call to its dedicated EIR primitive.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, crate::builtins::semantics::BuiltinLoweringError> {
    Ok(ctx.emit_value(
        Op::ReflWord,
        vec![call.operand(0)?, call.operand(1)?],
        None,
        PhpType::Int,
        Op::ReflWord.default_effects(),
        Some(call.span),
    ))
}
