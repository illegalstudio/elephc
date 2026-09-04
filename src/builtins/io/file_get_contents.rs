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
//! - The signature matches reference PHP 8.4 exactly:
//!   `file_get_contents(string $filename, bool $use_include_path = false,
//!   ?resource $context = null, int $offset = 0, ?int $length = null)`.
//!   `$context` has no `TypeSpec` for `resource`, so it is declared `Mixed` with a
//!   `null` default exactly like `fopen()`'s `$context`; the backend rejects a
//!   non-null one instead of ignoring it.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "file_get_contents",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::FileGetContents,
    ),
    requirements: crate::builtins::semantics::file_get_contents_requirements,
}

/// Returns `Union(Str, Bool)` and records the runtime libraries the call may need.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    Ok(PhpType::Union(vec![PhpType::Str, PhpType::False]))
}