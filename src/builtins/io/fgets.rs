//! Purpose:
//! Home of the PHP `fgets` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` calls `ensure_stream_resource` on the stream argument for validation and
//!   returns `Mixed` (reflecting PHP's `string|false` on EOF). `returns: Mixed` is used
//!   because the precise union cannot be expressed through the scalar `returns:` field.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "fgets",
    area: Io,
    params: [stream: Mixed],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Fgets,
    ),
    summary: "Gets line from file pointer.",
    php_manual: "function.fgets",
}

/// Validates the stream argument and returns `Mixed` for the `string|false` EOF pattern.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(
        cx.checker,
        cx.name,
        &cx.args[0],
        cx.env,
    )?;
    Ok(PhpType::Mixed)
}
