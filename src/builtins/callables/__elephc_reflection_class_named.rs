//! Purpose:
//! Home of the internal `__elephc_reflection_class_named` builtin: the name-only
//! `ReflectionClass` that `ReflectionFunction::getClosureScopeClass()` returns for a Closure
//! reflected at run time.
//!
//! Called from:
//! - The synthetic `ReflectionFunction::getClosureScopeClass()` body.
//!
//! Key details:
//! - AOT ReflectionClass metadata is materialized per class at compile time; a class named
//!   only at run time gets a reflector that answers `getName()` and `->name` and throws `Error`
//!   from every other method (see the synthetic class's `__name_only` guard).
//! - Lowers to `ObjectNew ReflectionClass` with a second marker operand that selects the
//!   name-only form in codegen; user code cannot spell that two-operand constructor.

use crate::builtins::semantics::{
    internal_eir_semantics, BuiltinLoweringContext, BuiltinLoweringError, BuiltinResultOwnership,
    LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::ir::{Effects, Immediate, Op};
use crate::types::PhpType;

builtin! {
    contract: "__elephc_reflection_class_named",
    check: check,
    semantics: internal_eir_semantics(
        lower,
        Effects::READS_HEAP.union(Effects::ALLOC_HEAP),
        BuiltinResultOwnership::Fresh,
    ),
}

/// Types the result as a `ReflectionClass` object.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    Ok(PhpType::Object("ReflectionClass".to_string()))
}

/// Lowers to the two-operand name-only `ReflectionClass` allocation.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    let marker = ctx.emit_value(
        Op::ConstI64,
        Vec::new(),
        Some(Immediate::I64(0)),
        PhpType::Int,
        Op::ConstI64.default_effects(),
        Some(call.span),
    );
    let class_name = ctx.intern_class_name("ReflectionClass");
    Ok(ctx.emit_value(
        Op::ObjectNew,
        vec![call.operand(0)?, marker.value],
        Some(Immediate::Data(class_name)),
        PhpType::Object("ReflectionClass".to_string()),
        Op::ObjectNew.default_effects(),
        Some(call.span),
    ))
}
