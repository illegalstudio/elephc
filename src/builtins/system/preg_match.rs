//! Purpose:
//! Home of the PHP `preg_match` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The third param `matches` is by-reference (`ref matches: Mixed = DefaultSpec::EmptyArray`),
//!   matching the golden signature where `ref_params[2] = true`.
//! - `lazy_check: true` suppresses the registry's default pre-inference loop so the hook
//!   can infer args[0] and args[1] (pattern and subject) while deliberately skipping
//!   inference of args[2] (`$matches`). `$matches` is a write-only output parameter;
//!   it is not declared before the call and inferring it would produce an
//!   "Undefined variable" error.
//! - `check` validates that args[2] (when present) is a `Variable` expression; passing
//!   a non-variable to the by-ref `$matches` param is a compile error.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    contract: "preg_match",
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::with_argument_lowering(
        crate::builtins::semantics::runtime_fn_semantics(crate::ir::RuntimeFnId::PregMatch),
        crate::builtins::semantics::BuiltinArgumentLowering::PositionalRegex,
    ),
}

/// Validates that `$matches`, when supplied, is a variable expression.
///
/// Infers args[0] (pattern) and args[1] (subject) to trigger type-environment side
/// effects, but deliberately skips inference of args[2] (`$matches`) because it is a
/// write-only output parameter that is undefined before the call. Passing a non-variable
/// (such as a literal or function call) to `$matches` is a compile-time error.
/// The result is PHP `int|false`: `1` or `0` for a completed search, and `false` when
/// the pattern is invalid or uses the removed `/e` modifier.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    check_capture_output_storage(cx)?;
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    cx.checker.infer_type(&cx.args[1], cx.env)?;
    if cx.args.len() == 3 && !matches!(cx.args[2].kind, ExprKind::Variable(_)) {
        return Err(CompileError::new(
            cx.args[2].span,
            "preg_match() parameter $matches must be passed a variable",
        ));
    }
    Ok(PhpType::Union(vec![PhpType::Int, PhpType::False]))
}

/// Rejects capture writes that would replace a frozen scalar reference with an array pointer.
pub(super) fn check_capture_output_storage(cx: &BuiltinCheckCtx) -> Result<(), CompileError> {
    let Some(argument) = cx.args.get(2).filter(|_| !cx.argument_was_omitted(2)) else { return Ok(()); };
    let ExprKind::Variable(name) = &argument.kind else { return Ok(()); };
    if cx.checker.active_ref_params.contains(name)
        && !cx.checker.boxed_ref_aliased_locals.contains(name)
        && cx.env.get(name).is_some_and(|ty| matches!(ty.codegen_repr(),
            PhpType::Int | PhpType::Bool | PhpType::Float | PhpType::Str))
    {
        return Err(CompileError::new(argument.span,
            "regex capture output cannot retype a native reference"));
    }
    Ok(())
}
