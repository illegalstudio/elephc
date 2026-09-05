//! Purpose:
//! Home of PHP's `get_class_vars` builtin and its AOT contract.
//!
//! Called from:
//! - Checker, optimizer, ownership, and direct-call EIR specialization through the registry.
//!
//! Key details:
//! - Direct calls, literal `call_user_func` calls, and first-class callables use class metadata.
//! - Runtime-selected callable targets remain unsupported because they cannot be specialized.

use crate::builtins::semantics::{
    BuiltinArgumentLowering, BuiltinCallablePolicy, BuiltinEffects, BuiltinLowering,
    BuiltinLoweringContext, BuiltinLoweringError, BuiltinRequirements, BuiltinResultOwnership,
    BuiltinResultType, BuiltinRuntimeFunctions, BuiltinSemantics, BuiltinTargetStrategy,
    BuiltinTargetSupport, BuiltinValidation, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::ir::Effects;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    contract: "get_class_vars",
    check: check,
    semantics: BuiltinSemantics {
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Checked,
        effects: BuiltinEffects::Static(Effects::from_bits_retain(
            Effects::READS_GLOBAL.bits()
                | Effects::ALLOC_HEAP.bits()
                | Effects::MAY_THROW.bits(),
        )),
        result_ownership: BuiltinResultOwnership::Fresh,
        requirements: BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::EirGraph,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::None,
        argument_lowering: BuiltinArgumentLowering::Standard,
        callable: BuiltinCallablePolicy::StaticOnly(
            "get_class_vars() requires a statically resolved callable target in AOT mode",
        ),
        lowering: BuiltinLowering::Eir(lower_unreachable),
    },
}

/// Requires a string class name and returns a string-keyed Mixed array.
fn check(cx: &mut BuiltinCheckCtx<'_>) -> Result<PhpType, CompileError> {
    let argument = match &cx.args[0].kind {
        ExprKind::NamedArg { name, value } if crate::names::php_symbol_key(name) == "class" => {
            value.as_ref()
        }
        _ => &cx.args[0],
    };
    let ty = cx.checker.infer_type(argument, cx.env)?;
    if ty.codegen_repr() != PhpType::Str {
        return Err(CompileError::new(
            cx.span,
            "get_class_vars() argument must be a string in AOT mode",
        ));
    }
    Ok(PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    })
}

/// Rejects any path that bypassed the statically resolved class metadata specialization.
fn lower_unreachable(
    _ctx: &mut dyn BuiltinLoweringContext,
    _call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    Err(BuiltinLoweringError::new(
        "get_class_vars() bypassed its statically resolved EIR specialization",
    ))
}
