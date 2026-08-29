//! Purpose:
//! Home of the PHP `readline` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Union(Str, Bool)` to match PHP's false-on-failure pattern for
//!   end-of-input. The `prompt` argument is optional and pre-inferred by the registry.
//! - `arity_error` is overridden to "readline() takes 0 or 1 arguments" because the
//!   registry's default message for min0/max1 ("takes at most 1 argument") does not
//!   match the legacy error text.
//! - `returns: Mixed` is used because the union cannot be expressed through the scalar
//!   `returns:` field.
//! - The trailing newline is stripped by the LOWERING, not here: `__rt_fgets`
//!   keeps it (which is right for `fgets`) and `readline` must not.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "readline",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Readline,
    ),
}

/// Returns `Union(Str, False)`: the line, or `false` at end of input.
///
/// The EIR result carries the same union. It used to be overridden to plain
/// `Str` "the string representation produced by the current line-reader
/// backend", which collapsed the two answers: end of input came back as `""`,
/// indistinguishable from a line the user left empty, so `while (($l =
/// readline()) !== false)` never ended.
///
/// `False` rather than `Bool`: `readline` never answers `true`, and a union that
/// admits it makes every caller handle a value that cannot occur.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(cx.checker.normalize_union_type(vec![PhpType::Str, PhpType::False]))
}
