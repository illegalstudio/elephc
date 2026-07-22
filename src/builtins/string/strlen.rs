//! Purpose:
//! Home of the PHP `strlen` builtin: declaration and backend-neutral EIR semantics.
//!
//! Called from:
//! - The builtin registry, checker, optimizer, and AST-to-EIR builtin lowering path.
//!
//! Key details:
//! - Shared validation accepts `Str`, `Mixed`, and `Union` types (PHP coerces the argument to a
//!   string per standard type-juggling rules); other types are rejected.
//! - Lowering emits `StrLen` directly or `Cast(Str) -> StrLen` for dynamic operands.

use crate::builtins::semantics::{
    BuiltinCallablePolicy, BuiltinEffects, BuiltinLowering, BuiltinLoweringContext,
    BuiltinResultOwnership, BuiltinResultType, BuiltinRuntimeFunctions, BuiltinSemanticInput,
    BuiltinSemantics, BuiltinRequirements, BuiltinTargetStrategy, BuiltinTargetSupport,
    BuiltinValidation, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::errors::CompileError;
use crate::ir::{Immediate, IrType, Op};
use crate::types::PhpType;

builtin! {
    name: "strlen",
    area: String,
    params: [string: Str],
    returns: Int,
    semantics: BuiltinSemantics {
        validation: BuiltinValidation::Shared(validate),
        result_type: BuiltinResultType::Declared,
        effects: BuiltinEffects::Shared(effects),
        result_ownership: BuiltinResultOwnership::NonHeap,
        requirements: BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::EirGraph,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::None,
        argument_lowering: crate::builtins::semantics::BuiltinArgumentLowering::Standard,
        callable: BuiltinCallablePolicy::Dynamic(
            crate::builtins::semantics::callable_accepts_strlen_source,
        ),
        lowering: BuiltinLowering::Eir(lower),
    },
    summary: "Returns the length of a string.",
    php_manual: "function.strlen",
}

/// Validates the inferred `strlen` argument without depending on checker internals.
fn validate(input: &BuiltinSemanticInput<'_>) -> Result<(), CompileError> {
    let Some(ty) = input.arg_types.first() else {
        return Err(CompileError::new(
            input.span,
            "strlen() takes exactly 1 argument",
        ));
    };
    // Accept Str, Mixed, and Union types — PHP's strlen() coerces its
    // argument to a string per the standard PHP type juggling rules
    // (numbers become their decimal representation, true → "1",
    // false/null → ""). Dynamic inputs first use the ordinary EIR
    // string-cast operation, then the same string-length operation.
    if !matches!(ty, PhpType::Str | PhpType::Mixed | PhpType::Union(_)) {
        return Err(CompileError::new(
            input.span,
            "strlen() argument must be string",
        ));
    }
    Ok(())
}

/// Resolves observable effects for concrete strings versus dynamic string coercion.
fn effects(input: &BuiltinSemanticInput<'_>) -> crate::ir::Effects {
    match input.arg_types.first().map(PhpType::codegen_repr) {
        Some(PhpType::Str) => Op::StrLen.default_effects(),
        _ => Op::Cast.default_effects() | Op::StrLen.default_effects(),
    }
}

/// Lowers `strlen` to reusable EIR operations with no assembly or ABI knowledge.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, crate::builtins::semantics::BuiltinLoweringError> {
    let value = call.operand(0)?;
    let string = match ctx.value_php_type(value).codegen_repr() {
        PhpType::Str => value,
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.emit_value(
                Op::Cast,
                vec![value],
                Some(Immediate::CastTarget(IrType::Str)),
                PhpType::Str,
                Op::Cast.default_effects(),
                Some(call.span),
            )
            .value
        }
        other => {
            return Err(crate::builtins::semantics::BuiltinLoweringError::new(
                format!("strlen cannot lower checked operand type {:?}", other),
            ));
        }
    };
    Ok(ctx.emit_value(
        Op::StrLen,
        vec![string],
        None,
        call.result_type.clone(),
        Op::StrLen.default_effects(),
        Some(call.span),
    ))
}
