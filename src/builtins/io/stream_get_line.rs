//! Purpose:
//! Home of the PHP `stream_get_line` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the first argument is a stream resource before returning `Mixed`,
//!   which is how the registry spells PHP's `string|false`: the call reports false once the
//!   stream has nothing left, and an empty string for a segment that is genuinely empty.
//! - `ending` is optional (defaults to empty string). Arguments are pre-inferred by the registry.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "stream_get_line",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamGetLine,
    ),
}

/// Validates the stream resource argument and returns `Mixed` for the `string|false` EOF pattern.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(
        cx.checker,
        cx.name,
        &cx.args[0],
        cx.env,
    )?;
    Ok(PhpType::Mixed)
}
