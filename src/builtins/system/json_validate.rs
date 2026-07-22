//! Purpose:
//! Home of the PHP `json_validate` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The check hook validates the json argument type, that depth/flags are integers,
//!   and that the flags value (if statically known) is 0 or JSON_INVALID_UTF8_IGNORE.
//!   Type errors are reported at the offending argument's span.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::builtins::system::json_support;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "json_validate",
    area: System,
    params: [
        json: Str,
        depth: Int = DefaultSpec::Int(512),
        flags: Int = DefaultSpec::Int(0),
    ],
    returns: Bool,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::JsonValidate,
    ),
    summary: "Checks if a string contains valid JSON.",
}

/// Validates the json argument is string-compatible, depth/flags are integers,
/// and the static flags value (if known) is 0 or JSON_INVALID_UTF8_IGNORE.
///
/// Reports type errors at the span of the offending argument, not the call span.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let json_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !json_support::is_json_string_arg_type(&json_ty) {
        return Err(CompileError::new(
            cx.args[0].span,
            "json_validate() json argument must be string-compatible",
        ));
    }
    for extra in &cx.args[1..] {
        let ty = cx.checker.infer_type(extra, cx.env)?;
        if ty != PhpType::Int {
            return Err(CompileError::new(
                extra.span,
                "json_validate() depth and flags must be integers",
            ));
        }
    }
    if let Some(flags) = cx.args.get(2) {
        if let Some(value) = json_support::json_static_int_value(flags) {
            const JSON_INVALID_UTF8_IGNORE: i64 = 1_048_576;
            if value & !JSON_INVALID_UTF8_IGNORE != 0 {
                return Err(CompileError::new(
                    flags.span,
                    "json_validate() flags must be 0 or JSON_INVALID_UTF8_IGNORE",
                ));
            }
        }
    }
    Ok(PhpType::Bool)
}
