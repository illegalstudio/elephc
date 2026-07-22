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

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    name: "preg_match",
    area: System,
    params: [pattern: Str, subject: Str, ref matches: Mixed = DefaultSpec::EmptyArray],
    returns: Int,
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::with_argument_lowering(
        crate::builtins::semantics::runtime_fn_semantics(crate::ir::RuntimeFnId::PregMatch),
        crate::builtins::semantics::BuiltinArgumentLowering::PositionalRegex,
    ),
    summary: "Performs a regular expression match.",
}

/// Validates that `$matches`, when supplied, is a variable expression.
///
/// Infers args[0] (pattern) and args[1] (subject) to trigger type-environment side
/// effects, but deliberately skips inference of args[2] (`$matches`) because it is a
/// write-only output parameter that is undefined before the call. Passing a non-variable
/// (such as a literal or function call) to `$matches` is a compile-time error.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    cx.checker.infer_type(&cx.args[1], cx.env)?;
    if cx.args.len() == 3 && !matches!(cx.args[2].kind, ExprKind::Variable(_)) {
        return Err(CompileError::new(
            cx.args[2].span,
            "preg_match() parameter $matches must be passed a variable",
        ));
    }
    Ok(PhpType::Int)
}
