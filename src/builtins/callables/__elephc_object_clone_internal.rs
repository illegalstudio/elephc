//! Purpose:
//! Clones compiler-private object state without consuming a PHP-visible object handle.
//!
//! Called from:
//! - Synthetic DatePeriod storage clone helpers for date and interval backing state.
//!
//! Key details:
//! - The clone remains an ordinary refcounted GC object; only its handle-table slot is absent.
//! - User-visible getters clone this backing value normally and therefore receive fresh handles.

use crate::builtins::semantics::{
    BuiltinArgumentLowering, BuiltinCallablePolicy, BuiltinEffects, BuiltinLowering,
    BuiltinLoweringContext, BuiltinLoweringError, BuiltinRequirements, BuiltinResultOwnership,
    BuiltinResultType, BuiltinRuntimeFunctions, BuiltinSemantics, BuiltinTargetStrategy,
    BuiltinTargetSupport, BuiltinValidation, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::ir::Op;
use crate::types::PhpType;

builtin! {
    contract: "__elephc_object_clone_internal",
    check: check,
    semantics: BuiltinSemantics {
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Checked,
        effects: BuiltinEffects::Static(
            crate::ir::Effects::READS_HEAP
                .union(crate::ir::Effects::ALLOC_HEAP)
                .union(crate::ir::Effects::REFCOUNT_OP),
        ),
        result_ownership: BuiltinResultOwnership::Fresh,
        requirements: BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::EirPrimitive,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::None,
        argument_lowering: BuiltinArgumentLowering::Standard,
        callable: BuiltinCallablePolicy::StaticOnly("internal compiler storage primitive"),
        lowering: BuiltinLowering::Eir(lower),
    },
}

/// Requires a statically known object and preserves its exact result class.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(ty.codegen_repr(), PhpType::Object(_)) {
        return Err(CompileError::new(
            cx.span,
            "__elephc_object_clone_internal() argument must be an object",
        ));
    }
    Ok(ty)
}

/// Emits the typed handleless clone primitive for one compiler-private backing object.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    let object = call.operand(0)?;
    let result_type = ctx.value_php_type(object);
    Ok(ctx.emit_value(
        Op::ObjectCloneInternal,
        vec![object],
        None,
        result_type,
        Op::ObjectCloneInternal.default_effects(),
        Some(call.span),
    ))
}
