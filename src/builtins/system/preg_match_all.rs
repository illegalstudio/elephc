//! Purpose:
//! Home of the PHP `preg_match_all` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - The third param `matches` is by-reference (`ref matches: Mixed = DefaultSpec::EmptyArray`).
//! - Optional `$flags` accepts `PREG_PATTERN_ORDER`, `PREG_SET_ORDER`,
//!   `PREG_OFFSET_CAPTURE`, and `PREG_UNMATCHED_AS_NULL`.
//! - `lazy_check: true` suppresses the registry's default pre-inference loop so the hook
//!   can infer pattern, subject, and flags while skipping write-only `$matches`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "preg_match_all",
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::with_argument_lowering(
        crate::builtins::semantics::runtime_fn_semantics(crate::ir::RuntimeFnId::PregMatchAll),
        crate::builtins::semantics::BuiltinArgumentLowering::PositionalRegex,
    ),
}

/// Validates optional `$matches` / `$flags` and returns the match-count type.
///
/// Infers the pattern, subject, and flags arguments. `$matches` is write-only and
/// is not inferred here; passing a non-variable for that parameter is rejected by
/// the shared by-reference lvalue check before this hook runs.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    cx.checker.infer_type(&cx.args[1], cx.env)?;
    if cx.args.len() >= 4 {
        cx.checker.infer_type(&cx.args[3], cx.env)?;
    }
    Ok(PhpType::Int)
}
