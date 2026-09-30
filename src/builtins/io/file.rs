//! Purpose:
//! Home of the PHP `file` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - PHP's signature is `file(string $filename, int $flags = 0, $context = null)`; elephc declares
//!   all three. `context` accepts a stream context resource and is honoured by the lowering, so
//!   HTTP headers and wrapper options set on the context reach the request.
//! - `flags` is an ordinary run-time integer bitmask (`FILE_USE_INCLUDE_PATH`,
//!   `FILE_IGNORE_NEW_LINES`, `FILE_SKIP_EMPTY_LINES`), NOT a shape-changing literal: the result
//!   is `Array<Str>` for every flag combination, so it does not need to be known at compile time.
//! - `check` returns `array<string>|false`. A check hook is required because neither arm
//!   can be expressed through the scalar `returns:` field, and the union is what lets a
//!   caller distinguish a failed read from an EMPTY file — the two used to be the same
//!   empty array.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "file",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::File,
    ),
    // The same reader, so the same libraries: a `compress.*://` filename links the compression
    // library it decodes with, exactly as `file_get_contents()` does for the same URL.
    requirements: crate::builtins::semantics::file_get_contents_requirements,
}

/// Returns `array<string>|false`: the file's lines as strings, or `false` when the read fails.
///
/// The `$flags` bitmask only changes the CONTENT of the lines (trailing newline removal and
/// empty-line skipping), never the container shape, so the result type is flag-independent.
/// Arity (1 to 3) is pre-validated by the registry, and the registry already inferred every
/// argument once for side effects.
///
/// The false arm is `PhpType::False`, not `Bool`: guard narrowing strips an EXACT member, so
/// a `Bool` arm survives `if ($lines !== false)` and every use inside the guard still sees a
/// union. Declaring the union rather than `Mixed` is what carries the information — both box
/// identically, but `Mixed` gives a builtin that needs an array nothing to justify itself with.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    // php's signature is array|false; False (not Bool) is the member a !== false narrowing
    // removes, following fgetcsv and scandir. The array-taking family accepts the union
    // through the argument lowering's unbox-or-throw.
    Ok(PhpType::Union(vec![
        PhpType::Array(Box::new(PhpType::Str)),
        PhpType::False,
    ]))
}
