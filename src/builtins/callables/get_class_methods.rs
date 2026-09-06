//! Purpose:
//! Home of PHP's `get_class_methods` builtin and its AOT checker contract.
//!
//! Called from:
//! - Checker, optimizer, ownership, and class-introspection specialization through the registry.
//!
//! Key details:
//! - AOT accepts an object or a runtime class-name string.
//! - Direct, spread, and statically resolved callable paths use the AOT metadata specializer.

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
    contract: "get_class_methods",
    check: check,
    semantics: BuiltinSemantics {
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Checked,
        effects: BuiltinEffects::Static(Effects::from_bits_retain(
            Effects::READS_HEAP.bits()
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
            "get_class_methods() requires a statically resolved callable target in AOT mode",
        ),
        lowering: BuiltinLowering::Eir(lower_unreachable),
    },
}

/// Accepts an object or string class name and returns an indexed string array.
fn check(cx: &mut BuiltinCheckCtx<'_>) -> Result<PhpType, CompileError> {
    let argument = match &cx.args[0].kind {
        ExprKind::NamedArg { name, value }
            if crate::names::php_symbol_key(name) == "object_or_class" =>
        {
            value.as_ref()
        }
        _ => &cx.args[0],
    };
    let ty = cx.checker.infer_type(argument, cx.env)?;
    if !matches!(ty.codegen_repr(), PhpType::Object(_) | PhpType::Str) {
        return Err(CompileError::new(
            cx.span,
            "get_class_methods() argument must be an object or string in AOT mode",
        ));
    }
    Ok(PhpType::Array(Box::new(PhpType::Str)))
}

/// Rejects any path that bypassed the AOT class-metadata specialization.
fn lower_unreachable(
    _ctx: &mut dyn BuiltinLoweringContext,
    _call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    Err(BuiltinLoweringError::new(
        "get_class_methods() bypassed its statically resolved EIR specialization",
    ))
}
