//! Purpose:
//! Home of the PHP `file_put_contents` builtin: its declaration, type-check hook, and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `int|false`: the byte count, or `false` for a path that cannot be
//!   opened. Declaring `Int` alone made `file_put_contents($p, $d) === false` — the manual's
//!   own failure test — unreachable, and the runtime never produced the `false` either.
//! - The `check` hook links the PHAR bridge: a literal `phar://` URL writes through
//!   the read-modify-write bridge and links `elephc_phar` plus `elephc_crypto` (the
//!   assembly SHA1 path remains a fallback); any non-literal path links `elephc_phar`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "file_put_contents",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::FilePutContents,
    ),
    requirements: crate::builtins::semantics::file_put_contents_requirements,
}

/// Returns `int|false` and records the PHAR libraries the write may need.
///
/// `False`, not `Bool`: the member a `!== false` narrowing removes, following `fgetcsv`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    for arg in cx.args {
        cx.checker.infer_type(arg, cx.env)?;
    }
    Ok(PhpType::Union(vec![PhpType::Int, PhpType::False]))
}