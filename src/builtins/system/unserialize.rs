//! Purpose:
//! Home of the PHP `unserialize` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The check hook validates the data argument is string-compatible.
//!   The optional options argument is accepted without type restriction.
//!   Type errors are reported at the offending argument's span.
//! - `options` default is `DefaultSpec::EmptyArray` (matches legacy `ArrayLiteral([])`
//!   for parity gate comparison).

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::builtins::system::json_support;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "unserialize",
    area: System,
    params: [
        data: Str,
        options: Mixed = DefaultSpec::EmptyArray,
    ],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Unserialize,
    ),
    summary: "Creates a PHP value from a stored representation.",
}

/// Validates that the data argument is string-compatible.
///
/// The optional options argument is inferred but not type-restricted.
/// Reports type errors at the span of the offending argument, not the call span.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let data_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !json_support::is_json_string_arg_type(&data_ty) {
        return Err(CompileError::new(
            cx.args[0].span,
            "unserialize() data argument must be string-compatible",
        ));
    }
    if let Some(options) = cx.args.get(1) {
        cx.checker.infer_type(options, cx.env)?;
    }
    Ok(PhpType::Mixed)
}
