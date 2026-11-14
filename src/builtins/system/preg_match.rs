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
//! - A call that passes `$flags` or `$offset` (see `uses_extended_capture()`) fills
//!   `$matches` with boxed Mixed cells, because `PREG_OFFSET_CAPTURE` and
//!   `PREG_UNMATCHED_AS_NULL` produce arrays and nulls. Checker, EIR lowering and the
//!   backend all key the output representation on that one predicate.

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
    if cx.args.len() >= 3
        && !cx.argument_was_omitted(2)
        && !matches!(cx.args[2].kind, ExprKind::Variable(_))
    {
        return Err(CompileError::new(
            cx.args[2].span,
            "preg_match() parameter $matches must be passed a variable",
        ));
    }
    for index in 3..cx.args.len().min(5) {
        cx.checker.infer_type(&cx.args[index], cx.env)?;
    }
    Ok(PhpType::Union(vec![PhpType::Int, PhpType::False]))
}

/// Returns true when a source-order `preg_match()` argument list passes `$flags` or `$offset`.
///
/// Such calls run through the boxed-Mixed capture helper, so `$matches` becomes
/// `array<mixed>` instead of the `array<string>` of the three-argument form.
pub(crate) fn uses_extended_capture(args: &[crate::parser::ast::Expr]) -> bool {
    passes_parameter_from(args, &["flags", "offset"], 3)
}

/// Returns true when a source-order argument list may pass a parameter at or after
/// `first_index`, positionally or through one of the `named` parameters.
///
/// Literal indexed spreads are flattened and static string-keyed spreads count as the
/// named arguments they expand to; any other spread conservatively counts as passing.
pub(crate) fn passes_parameter_from(
    args: &[crate::parser::ast::Expr],
    named: &[&str],
    first_index: usize,
) -> bool {
    let positional = crate::types::call_args::expand_planned_positional_spreads(args);
    let args = crate::types::call_args::expand_static_assoc_spread_args(
        positional.as_deref().unwrap_or(args),
    );
    args.iter().enumerate().any(|(index, arg)| match &arg.kind {
        ExprKind::NamedArg { name, .. } => named.contains(&name.as_str()),
        ExprKind::Spread(_) => true,
        _ => index >= first_index,
    })
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
