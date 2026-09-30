//! Purpose:
//! Home of the internal `__elephc_zip_stat_entries` ZIP intrinsic: its declaration, checker contract, and semantic target. Compiler-synthesized; not PHP-visible.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `internal: true` keeps it out of PHP-visible builtin name sets and
//!   `function_exists()`; it is reachable only through the compiler-generated
//!   `ZipArchive` method bodies.
//! - The returned array is the bridge's serialized central directory: element 0 is
//!   the decimal entry count and every later element is one NUL-joined record. An
//!   EMPTY array means the file is unreadable or is no ZIP at all, which is how
//!   `ZipArchive::open()` tells `ER_NOENT`/`ER_NOZIP` from an archive with no
//!   entries.
//! - The `check` hook links the `elephc_phar` bridge (a mandatory side effect);
//!   argument inference is handled by the registry common path, so the hook does not
//!   call `infer_type`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "__elephc_zip_stat_entries",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcZipStatEntries,
    ),
}

/// Links the `elephc_phar` bridge and returns `Array<Str>` for the serialized records.
/// Argument inference is performed by the registry common path before this hook runs.
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::Array(Box::new(PhpType::Str)))
}
