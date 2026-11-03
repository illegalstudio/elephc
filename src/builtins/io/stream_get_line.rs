//! Purpose:
//! Home of the PHP `stream_get_line` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the first argument is a stream resource before returning
//!   php's `string|false`.
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

/// Validates the stream resource argument and returns php's `string|false`.
///
/// `false` is what EOF answers when nothing could be read — and it is genuinely distinct from
/// the empty string a leading delimiter produces, measured on php-src 8.5.6: reading `"\nx"`
/// gives `""`, then `"x"`, then `false`. Declaring a plain `Str` here collapsed the two.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(
        cx.checker,
        cx.name,
        &cx.args[0],
        cx.env,
    )?;
    Ok(cx.checker.normalize_union_type(vec![PhpType::Str, PhpType::False]))
}
