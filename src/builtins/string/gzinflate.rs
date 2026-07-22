//! Purpose:
//! Home of the PHP `gzinflate` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The checker contract returns the `string|false` union for decompression failure.
//! - The typed runtime target declares the zlib system-library requirement.
//! - Argument types are inferred by the common registry dispatch path before the hook fires.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "gzinflate",
    area: String,
    params: [data: Str, max_length: Int = DefaultSpec::Int(0)],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Gzinflate,
    ),
    summary: "Inflate a deflated string.",
    php_manual: "https://www.php.net/manual/en/function.gzinflate.php",
}

/// Returns `PhpType::Union([Str, Bool])` for a `gzinflate` call.
///
/// The union return (string on success, false on decompression error) cannot be expressed
/// inline in the `builtin!` macro so a check hook is required.
/// Argument types are inferred by the common registry dispatch path before this hook fires;
/// arity (1–2 args) is pre-validated by the registry.
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::Union(vec![PhpType::Str, PhpType::False]))
}
