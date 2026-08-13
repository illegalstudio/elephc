//! Purpose:
//! Home of the PHP `file` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - PHP's signature is `file(string $filename, int $flags = 0, $context = null)`; elephc declares
//!   `filename` and `flags`. The stream-context parameter is not modelled, so it is left out
//!   rather than accepted and ignored.
//! - `flags` is an ordinary run-time integer bitmask (`FILE_USE_INCLUDE_PATH`,
//!   `FILE_IGNORE_NEW_LINES`, `FILE_SKIP_EMPTY_LINES`), NOT a shape-changing literal: the result
//!   is `Array<Str>` for every flag combination, so it does not need to be known at compile time.
//! - `check` returns `array<string>|false`. A check hook is required because neither arm
//!   can be expressed through the scalar `returns:` field, and the union is what lets a
//!   caller distinguish a failed read from an EMPTY file — the two used to be the same
//!   empty array.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "file",
    area: Io,
    params: [filename: Str, flags: Int = DefaultSpec::Int(0)],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::File,
    ),
    summary: "Reads an entire file into an array.",
    php_manual: "function.file",
}

/// Returns `array<string>|false`: the file's lines as strings, or `false` when the read fails.
///
/// The `$flags` bitmask only changes the CONTENT of the lines (trailing newline removal and
/// empty-line skipping), never the container shape, so the array arm is flag-independent.
/// Arity (1 or 2) is pre-validated by the registry, and the registry already inferred every
/// argument once for side effects.
///
/// The false arm is `PhpType::False`, not `Bool`: guard narrowing strips an EXACT member, so
/// a `Bool` arm survives `if ($lines !== false)` and every use inside the guard still sees a
/// union. Declaring the union rather than `Mixed` is what carries the information — both box
/// identically, but `Mixed` gives a builtin that needs an array nothing to justify itself with.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    Ok(PhpType::Union(vec![
        PhpType::Array(Box::new(PhpType::Str)),
        PhpType::False,
    ]))
}
