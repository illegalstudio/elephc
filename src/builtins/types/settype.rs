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
    name: "settype",
    area: Types,
    params: [ref var: Mixed, type: Str],
    returns: Bool,
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Settype,
    ),
    summary: "Sets the type of a variable.",
    php_manual: "function.settype",
}

/// Validates the `settype` arguments: infers both in source order and rejects a non-string type.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    let ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    if ty != PhpType::Str {
        return Err(CompileError::new(
            cx.span,
            "settype() second argument must be a string",
        ));
    }
    Ok(PhpType::Bool)
}
