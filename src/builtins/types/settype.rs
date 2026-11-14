//! Purpose:
//! Home of the PHP `settype` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The first parameter `var` is passed by reference (mutating builtin); `ref_params[0]`
//!   is set by the `ref` marker in the `builtin!` declaration.
//! - `lazy_check: true` so the check hook controls argument inference order: it infers
//!   `var` then `type` in source order (once each), matching legacy exactly-once inference.
//! - `check` validates that the second argument is a string and returns `Bool`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "settype",
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Settype,
    ),
}

/// Validates the `settype` arguments: infers both in source order and rejects a non-string type.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let source = cx.checker.infer_type(&cx.args[0], cx.env)?;
    let ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    if ty != PhpType::Str {
        return Err(CompileError::new(
            cx.span,
            "settype() second argument must be a string",
        ));
    }
    if let crate::parser::ast::ExprKind::StringLiteral(name) = &cx.args[1].kind {
        if crate::semantics::conversions::checked_casts(cx.checker.semantic_profile, cx.source_mode) {
            elephc_semantics::settype_target::SettypeTarget::parse(name)
                .map_err(|failure| CompileError::new(cx.span, failure.message()))?;
        }
        if let Some(target) = elephc_semantics::numeric_cast::NumericCastTarget::from_type_name(name) {
            let target = match target {
                elephc_semantics::numeric_cast::NumericCastTarget::Int => crate::parser::ast::CastType::Int,
                elephc_semantics::numeric_cast::NumericCastTarget::Float => crate::parser::ast::CastType::Float,
            };
            cx.checker.validate_explicit_numeric_cast(cx.source_mode, &target, &source, cx.span)?;
        }
    }
    Ok(PhpType::Bool)
}
