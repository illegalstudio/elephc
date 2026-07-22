//! Purpose:
//! Home of the PHP `fgetc` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` calls `ensure_stream_resource` on the stream argument for validation and
//!   returns `Union(Str, Bool)` reflecting PHP behaviour where `fgetc` returns a
//!   single character or `false` on EOF. `returns: Mixed` is used because the union
//!   cannot be expressed through the scalar `returns:` field.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "fgetc",
    area: Io,
    params: [stream: Mixed],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Fgetc,
    ),
    summary: "Gets a character from the given file pointer.",
    php_manual: "function.fgetc",
}

/// Validates the stream argument and returns `Union(Str, Bool)` for the EOF pattern.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(
        cx.checker,
        cx.name,
        &cx.args[0],
        cx.env,
    )?;
    Ok(cx.checker.normalize_union_type(vec![PhpType::Str, PhpType::False]))
}
