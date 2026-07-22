//! Purpose:
//! Home of the PHP `unlink` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Bool`. Unlike `mkdir`/`rmdir`/`chdir`, `unlink` carries a PHAR
//!   side effect: a literal `phar://` URL or any non-literal path links `elephc_phar`
//!   because deletion may target an entry inside a PHAR archive.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "unlink",
    area: Io,
    params: [filename: Str],
    returns: Bool,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Unlink,
    ),
    requirements: crate::builtins::semantics::unlink_requirements,
    summary: "Deletes a file.",
    php_manual: "function.unlink",
}

/// Returns `Bool` and links `elephc_phar` when the target may live in a PHAR archive.
///
/// A literal `phar://` URL links `elephc_phar`; a non-literal path also links it
/// because the scheme is unknown at compile time. A literal non-`phar://` path
/// needs no PHAR bridge.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    Ok(PhpType::Bool)
}
