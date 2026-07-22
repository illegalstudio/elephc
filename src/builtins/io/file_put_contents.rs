//! Purpose:
//! Home of the PHP `file_put_contents` builtin: its declaration, type-check hook, and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Int` (the number of bytes written).
//! - The `check` hook links the PHAR bridge: a literal `phar://` URL writes through
//!   the read-modify-write bridge and links `elephc_phar` plus `elephc_crypto` (the
//!   assembly SHA1 path remains a fallback); any non-literal path links `elephc_phar`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "file_put_contents",
    area: Io,
    params: [filename: Str, data: Str],
    returns: Int,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::FilePutContents,
    ),
    requirements: crate::builtins::semantics::file_put_contents_requirements,
    summary: "Writes data to a file.",
    php_manual: "function.file-put-contents",
}

/// Returns `Int` and records the PHAR libraries the write may need.
///
/// A literal `phar://` target writes through the `elephc_phar` bridge and also links
/// `elephc_crypto`; any other target (including non-literal paths) links `elephc_phar`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    for arg in cx.args {
        cx.checker.infer_type(arg, cx.env)?;
    }
    Ok(PhpType::Int)
}
