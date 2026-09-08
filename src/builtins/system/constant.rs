//! Purpose:
//! Home of the PHP `constant` builtin: its single-source registry declaration and semantic
//! metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Literal names retain the referenced constant's precise type and lower through the ordinary
//!   constant fast path; dynamic strings select from the prescanned global constant table.
//! - An unknown literal is rejected during checking, while an unknown dynamic name raises from
//!   the generated selection graph.
//! - Class constants and enum cases (`constant('Foo::BAR')`) are NOT supported: the name is
//!   resolved through the global constant table only.
//! - Literal lowering happens one level up, in
//!   `crate::ir_lower::expr::constants::lower_static_constant_call()`, which rewrites the call
//!   into the same EIR a bare `FOO` reference produces.

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
        effects: BuiltinEffects::Static(
            crate::ir::Effects::READS_GLOBAL
                .union(crate::ir::Effects::ALLOC_HEAP)
                .union(crate::ir::Effects::MAY_FATAL),
        ),
        result_ownership: BuiltinResultOwnership::Fresh,
        requirements: BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::EirGraph,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::None,
        argument_lowering: crate::builtins::semantics::BuiltinArgumentLowering::Standard,
        callable: BuiltinCallablePolicy::StaticOnly(
            "constant() needs a compile-time constant name",
        ),
        lowering: BuiltinLowering::Eir(lower),
    },
}

/// Validates the name and returns a literal constant's own PHP type.
///
/// A leading `\` is stripped the way PHP's global-constant lookup does. Dynamic strings return
/// `Mixed` because the selected prescanned constant is only known at runtime. Class constants are
/// still rejected here and use their dedicated access syntax.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let argument_type = cx.checker.infer_type(&cx.args[0], cx.env)?;
    let literal = match &cx.args[0].kind {
        ExprKind::StringLiteral(name) => Some(name.clone()),
        ExprKind::NamedArg { name, value } if name == "name" => match &value.kind {
            ExprKind::StringLiteral(name) => Some(name.clone()),
            _ => None,
        },
        _ => None,
    };
    let Some(name) = literal else {
        return if argument_type.codegen_repr() == PhpType::Str {
            Ok(PhpType::Mixed)
        } else {
            Err(CompileError::new(
                cx.span,
                "constant() argument must be a string",
            ))
        };
    };
    let name = name.trim_start_matches('\\').to_string();
    if name.contains("::") {
        return Err(CompileError::new(
            cx.span,
            "constant() class constants are not supported; reference the constant directly",
        ));
    }
    match cx.checker.constants.get(&name) {
        Some(ty) => Ok(ty.clone()),
        None => Err(CompileError::new(
            cx.span,
            &format!("Undefined constant: {}", name),
        )),
    }
}

/// Lowers a dynamic string through the compilation's prescanned global constant table.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    ctx.emit_constant_fetch(call.operand(0)?, call.span)
}
