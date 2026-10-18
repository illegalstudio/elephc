//! Purpose:
//! Home of the PHP `constant` builtin: its single-source registry declaration and semantic
//! metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` mirrors `defined()`'s AOT contract: the constant NAME must be a compile-time string
//!   literal. It also resolves the constant so the call's result type is the constant's own type
//!   (`int`, `float`, `string`, `bool`, `null`) instead of `mixed`.
//! - An unknown name is a COMPILE error here, where reference PHP raises
//!   `Error: Undefined constant "X"` at runtime. A binary with no constant table cannot look the
//!   name up, and refusing at compile time is strictly more informative than a runtime fatal.
//! - Class constants and enum cases (`constant('Foo::BAR')`) are NOT supported: the name is
//!   resolved through the global constant table only.
//! - Lowering happens one level up, in
//!   `crate::ir_lower::expr::constants::lower_static_constant_call()`, which rewrites the call
//!   into the same EIR a bare `FOO` reference produces. The registry lowering hook below is a
//!   guard for paths that bypass that rewrite.

use crate::builtins::semantics::{
    BuiltinCallablePolicy, BuiltinEffects, BuiltinLowering, BuiltinLoweringContext,
    BuiltinLoweringError, BuiltinRequirements, BuiltinResultOwnership, BuiltinResultType,
    BuiltinRuntimeFunctions, BuiltinSemantics, BuiltinTargetStrategy, BuiltinTargetSupport,
    BuiltinValidation, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    contract: "constant",
    check: check,
    semantics: BuiltinSemantics {
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Checked,
        effects: BuiltinEffects::Static(crate::ir::Effects::READS_GLOBAL),
        result_ownership: BuiltinResultOwnership::NonHeap,
        requirements: BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::EirPrimitive,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::None,
        argument_lowering: crate::builtins::semantics::BuiltinArgumentLowering::Standard,
        callable: BuiltinCallablePolicy::StaticOnly(
            "constant() needs a compile-time constant name",
        ),
        lowering: BuiltinLowering::Eir(lower),
    },
}

/// Validates the literal name and returns the referenced constant's own PHP type.
///
/// AOT compilation has no runtime constant table, so the name must be a `StringLiteral`. A
/// leading `\` is stripped the way PHP's own global-constant lookup does. Returns a
/// `CompileError` for a dynamic name, a class-constant name, or an unknown constant.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    let literal = match &cx.args[0].kind {
        ExprKind::StringLiteral(name) => Some(name.clone()),
        ExprKind::NamedArg { name, value } if name == "name" => match &value.kind {
            ExprKind::StringLiteral(name) => Some(name.clone()),
            _ => None,
        },
        _ => None,
    };
    let Some(name) = literal else {
        return Err(CompileError::aot_literal_required(
            cx.span,
            "constant() first argument must be a string literal in AOT mode",
        ));
    };
    let name = elephc_builtin_contract::canonical_constant_name(&name);
    if name.contains("::") {
        return Err(CompileError::new(
            cx.span,
            "constant() class constants are not supported; reference the constant directly",
        ));
    }
    match cx.checker.constants.get(name) {
        Some(ty) => Ok(ty.clone()),
        None => Err(CompileError::new(
            cx.span,
            &format!("Undefined constant: {}", name),
        )),
    }
}

/// Rejects a `constant()` call that reached backend-neutral lowering.
///
/// Direct calls are rewritten into a plain constant reference by
/// `crate::ir_lower::expr::constants::lower_static_constant_call()` before the registry
/// lowering runs, so reaching here means the name was not a literal — which `check` already
/// refuses for every path that type-checks.
fn lower(
    _ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    Err(BuiltinLoweringError::new(format!(
        "{}() needs a compile-time constant name",
        call.name,
    )))
}
