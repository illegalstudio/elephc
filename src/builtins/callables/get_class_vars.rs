//! Purpose:
//! Home of PHP's `get_class_vars` builtin and its literal-class AOT contract.
//!
//! Called from:
//! - Checker, optimizer, ownership, and direct-call EIR specialization through the registry.
//!
//! Key details:
//! - Direct calls are materialized from class metadata in `ir_lower::expr::class_introspection`.
//! - Runtime-selected callable use is refused because class default expressions need EIR lowering.

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
            Effects::READS_GLOBAL.bits() | Effects::ALLOC_HEAP.bits(),
        )),
        result_ownership: BuiltinResultOwnership::Fresh,
        requirements: BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::EirGraph,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::None,
        argument_lowering: BuiltinArgumentLowering::Standard,
        callable: BuiltinCallablePolicy::StaticOnly(
            "get_class_vars() requires a literal class name in AOT mode",
        ),
        lowering: BuiltinLowering::Eir(lower_unreachable),
    },
}

/// Requires a literal known class name and returns a string-keyed Mixed array.
fn check(cx: &mut BuiltinCheckCtx<'_>) -> Result<PhpType, CompileError> {
    let argument = match &cx.args[0].kind {
        ExprKind::NamedArg { name, value } if crate::names::php_symbol_key(name) == "class" => {
            value.as_ref()
        }
        _ => &cx.args[0],
    };
    let class_name = match &argument.kind {
        ExprKind::StringLiteral(class_name) => Some(class_name.clone()),
        ExprKind::ClassConstant { receiver } => match receiver {
            crate::parser::ast::StaticReceiver::Named(name) => Some(name.as_str().to_string()),
            crate::parser::ast::StaticReceiver::Self_
            | crate::parser::ast::StaticReceiver::Static => cx.checker.current_class.clone(),
            crate::parser::ast::StaticReceiver::Parent => cx
                .checker
                .current_class
                .as_ref()
                .and_then(|current| cx.checker.classes.get(current))
                .and_then(|info| info.parent.clone()),
        },
        _ => None,
    };
    let Some(class_name) = class_name else {
        return Err(CompileError::new(
            cx.span,
            "get_class_vars() argument must be a string literal in AOT mode",
        ));
    };
    let class_name = class_name.trim_start_matches('\\');
    if !cx.checker.classes.contains_key(class_name)
        && !cx.checker.interfaces.contains_key(class_name)
        && !cx.checker.declared_traits.contains(class_name)
        && !cx.checker.enums.contains_key(class_name)
    {
        return Err(CompileError::new(
            cx.span,
            &format!("get_class_vars(): Class \"{}\" not found", class_name),
        ));
    }
    Ok(PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    })
}

/// Rejects any path that bypassed the direct-call class metadata specialization.
fn lower_unreachable(
    _ctx: &mut dyn BuiltinLoweringContext,
    _call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    Err(BuiltinLoweringError::new(
        "get_class_vars() bypassed its literal-class EIR specialization",
    ))
}
