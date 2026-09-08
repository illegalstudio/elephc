//! Purpose:
//! Home of the PHP `get_object_vars` builtin and its object-only checker contract.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - Runtime-typed `Mixed` values remain accepted because `unserialize()` can
//!   produce an object through that storage shape.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::builtins::semantics::{
    BuiltinArgumentLowering, BuiltinCallablePolicy, BuiltinEffects, BuiltinLowering,
    BuiltinLoweringContext, BuiltinLoweringError, BuiltinRequirements, BuiltinResultOwnership,
    BuiltinResultType, BuiltinRuntimeFunctions, BuiltinSemantics, BuiltinTargetStrategy,
    BuiltinTargetSupport, BuiltinValidation, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "get_object_vars",
    check: check,
    semantics: BuiltinSemantics {
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Checked,
        effects: BuiltinEffects::Static(
            crate::ir::Effects::READS_HEAP
                .union(crate::ir::Effects::ALLOC_HEAP)
                .union(crate::ir::Effects::MAY_FATAL),
        ),
        result_ownership: BuiltinResultOwnership::Fresh,
        requirements: BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::EirGraph,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::One(crate::ir::RuntimeFnId::GetObjectVars),
        argument_lowering: BuiltinArgumentLowering::Standard,
        callable: BuiltinCallablePolicy::DynamicRuntime(crate::ir::RuntimeFnId::GetObjectVars),
        lowering: BuiltinLowering::Eir(lower),
    },
}

/// Requires an object-shaped value and returns a string-keyed Mixed array.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(ty.codegen_repr(), PhpType::Object(_) | PhpType::Mixed | PhpType::Union(_)) {
        return Err(CompileError::new(
            cx.span,
            "get_object_vars() argument must be an object",
        ));
    }
    Ok(PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    })
}

/// Materializes the caller-visible property map through the shared EIR object projection.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    ctx.emit_get_object_vars(call.operand(0)?, call.span)
}
