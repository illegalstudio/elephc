//! Purpose:
//! Home of the PHP `file_get_contents` builtin: its declaration, type-check hook, and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Union(Str, Bool)` reflecting PHP behaviour where the read
//!   returns the file contents or `false` on failure.
//! - The typed runtime target marks both result branches as caller-owned: successful
//!   reads return an owned string in a fresh Mixed box, while failures return a fresh
//!   boxed `false`.
//! - The `check` hook has a library-linking side effect: a literal `https://` /
//!   `ftps://` URL links `elephc_tls`; a non-literal path conservatively links
//!   `elephc_tls`, `elephc_phar`, `z`, and `bz2` because the scheme and PHAR entry
//!   flags are unknown until run time.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "file_get_contents",
    area: Io,
    params: [filename: Str],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::FileGetContents,
    ),
    requirements: crate::builtins::semantics::file_get_contents_requirements,
    summary: "Reads an entire file into a string.",
    php_manual: "function.file-get-contents",
}

/// Returns `Union(Str, Bool)` and records the runtime libraries the call may need.
///
/// A literal `https://`/`ftps://` URL is read over TLS, so it links `elephc_tls`.
/// A non-literal path routes through the runtime URL dispatcher, whose scheme and
/// PHAR entry flags are unknown at compile time, so it conservatively links TLS
/// plus the PHAR bridge and decompression libraries (`z`, `bz2`).
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    Ok(PhpType::Union(vec![PhpType::Str, PhpType::False]))
}
