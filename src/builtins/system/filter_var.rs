//! Purpose:
//! Declares PHP's `filter_var` builtin and validates the source-sensitive filter/options subset
//! implemented by the specialized EIR lowering.
//!
//! Called from:
//! - Checker, optimizer, ownership, callable, and EIR consumers through the builtin registry.
//! - `crate::ir_lower::expr::filter` before ordinary registry lowering.
//!
//! Key details:
//! - Literal filter ids select dedicated target-aware runtime parsers; a dynamic id is routed
//!   through the conditionally injected PHP dispatch prelude.
//! - Unsupported filters and flag combinations remain compile-time errors instead of silently
//!   applying incomplete validation semantics.

use crate::builtins::semantics::{
    BuiltinCallablePolicy, BuiltinEffects, BuiltinLowering, BuiltinLoweringContext,
    BuiltinLoweringError, BuiltinRequirements, BuiltinResultOwnership, BuiltinResultType,
    BuiltinRuntimeFunctions, BuiltinSemanticInput, BuiltinSemantics, BuiltinTargetStrategy,
    BuiltinTargetSupport, BuiltinValidation, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::filter_constants::{
    static_filter_int, static_filter_int_range_options, static_filter_options_flags,
};
use crate::types::PhpType;

builtin! {
    contract: "filter_var",
    check: check,
    lazy_check: true,
    semantics: BuiltinSemantics {
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Checked,
        effects: BuiltinEffects::Shared(effects),
        result_ownership: BuiltinResultOwnership::Fresh,
        requirements: BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::EirGraph,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::None,
        argument_lowering: crate::builtins::semantics::BuiltinArgumentLowering::Standard,
        callable: BuiltinCallablePolicy::StaticOnly(
            "filter_var() needs source-sensitive filter and option specialization",
        ),
        lowering: BuiltinLowering::Eir(lower_unspecialized),
    },
}

/// Returns the conservative effects of scalar conversion, boxed-result allocation, and failure.
fn effects(_input: &BuiltinSemanticInput<'_>) -> crate::ir::Effects {
    crate::ir::Effects::READS_HEAP
        | crate::ir::Effects::ALLOC_HEAP
        | crate::ir::Effects::REFCOUNT_OP
        | crate::ir::Effects::MAY_FATAL
        | crate::ir::Effects::MAY_WARN
}

/// Validates the supported filter id and option matrix and records a boxed `Mixed` result.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let value_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    // URL, EMAIL, DOMAIN and CALLBACK lower to `__elephc_filter_var_ext` (`filter_ext_prelude`),
    // which takes any value (FILTER_CALLBACK walks arrays) and any options shape.
    if cx
        .args
        .get(1)
        .and_then(static_filter_int)
        .is_some_and(|id| crate::filter_ext_prelude::EXTENDED_FILTER_IDS.contains(&id))
    {
        cx.checker.infer_type(&cx.args[1], cx.env)?;
        if let Some(options) = cx.args.get(2) {
            cx.checker.infer_type(options, cx.env)?;
        }
        return Ok(PhpType::Mixed);
    }
    if !is_supported_value_type(&value_ty) {
        return Err(CompileError::new(
            cx.span,
            &format!(
                "filter_var(): unsupported value type {:?} is not supported yet",
                value_ty
            ),
        ));
    }

    let filter_id = if let Some(filter) = cx.args.get(1) {
        cx.checker.infer_type(filter, cx.env)?;
        match static_filter_int(filter) {
            Some(value) => value,
            None => {
                if let Some(options) = cx.args.get(2) {
                    cx.checker.infer_type(options, cx.env)?;
                }
                return Ok(PhpType::Mixed);
            }
        }
    } else {
        516
    };

    if !matches!(filter_id, 516 | 257 | 258 | 259 | 275) {
        return Err(CompileError::new(
            cx.span,
            &format!("filter_var(): filter {} is not supported yet", filter_id),
        ));
    }

    if filter_id == 257 {
        if let Some(options) = cx.args.get(2) {
            if let Some(range) = static_filter_int_range_options(options) {
                const RANGE_ALLOWED_FLAGS: i64 = 134_217_728 | 33_554_432;
                if range.flags & !RANGE_ALLOWED_FLAGS != 0 {
                    return Err(CompileError::new(
                        cx.span,
                        &format!(
                            "filter_var(): flag combination {} is not supported yet",
                            range.flags
                        ),
                    ));
                }
                return Ok(PhpType::Mixed);
            }
        }
    }

    let flags = if let Some(options) = cx.args.get(2) {
        match static_filter_options_flags(options) {
            Some(value) => value,
            None => {
                let options_ty = cx.checker.infer_type(options, cx.env)?;
                let message = if matches!(
                    &options.kind,
                    ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_)
                ) || matches!(options_ty, PhpType::Array(_) | PhpType::AssocArray { .. })
                {
                    "filter_var(): array-form $options (['flags' => ..., 'options' => ...]) is not supported yet"
                } else {
                    // Run-time int flags (symfony/http-client's `NativeHttpClient::dnsResolve()`
                    // picks `FILTER_FLAG_IPV4` or `FILTER_FLAG_IPV6` for `FILTER_VALIDATE_IP`):
                    // lowering routes the call through `__elephc_filter_var_dyn`, which applies
                    // the same literal per-filter lowering under each flag value.
                    if matches!(options_ty.codegen_repr(), PhpType::Int | PhpType::Mixed) {
                        return Ok(PhpType::Mixed);
                    }
                    "filter_var(): a dynamic (non-compile-time-constant) $options is not supported yet"
                };
                return Err(CompileError::new(cx.span, message));
            }
        }
    } else {
        0
    };

    const ALLOWED_FLAGS: i64 = 134_217_728 | 33_554_432;
    const IP_ALLOWED_FLAGS: i64 = ALLOWED_FLAGS | 1_048_576 | 2_097_152;
    let allowed_flags = if filter_id == 275 {
        IP_ALLOWED_FLAGS
    } else {
        ALLOWED_FLAGS
    };
    if flags & !allowed_flags != 0 {
        return Err(CompileError::new(
            cx.span,
            &format!(
                "filter_var(): flag combination {} is not supported yet",
                flags
            ),
        ));
    }
    Ok(PhpType::Mixed)
}

/// Returns whether the specialized backend has a faithful conversion/failure path for the type.
fn is_supported_value_type(ty: &PhpType) -> bool {
    matches!(
        ty,
        PhpType::Int
            | PhpType::Float
            | PhpType::Str
            | PhpType::Bool
            | PhpType::False
            | PhpType::Void
            | PhpType::Mixed
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Union(_)
    )
}

/// Rejects any call that bypassed `crate::ir_lower::expr::filter` specialization.
fn lower_unspecialized(
    _ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    Err(BuiltinLoweringError::new(format!(
        "{} reached registry lowering without filter specialization",
        call.name
    )))
}
