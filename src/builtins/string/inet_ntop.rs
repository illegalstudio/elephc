//! Purpose:
//! Home of the PHP `inet_ntop` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns the `string|false` union: `inet_ntop` returns `false` for invalid
//!   packed IP addresses. A check hook is required because the `builtin!` macro cannot
//!   express a union return type inline.
//! - Argument types are inferred by the common registry dispatch path before the hook fires.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "inet_ntop",
    area: String,
    params: [ip: Str],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::InetNtop,
    ),
    summary: "Converts a packed internet address to a human-readable representation.",
    php_manual: "https://www.php.net/manual/en/function.inet-ntop.php",
}

/// Returns `PhpType::Union([Str, Bool])` for an `inet_ntop` call.
///
/// The union return (string on success, false on invalid input) cannot be expressed
/// inline in the `builtin!` macro so a check hook is required.
/// Argument types are inferred by the common registry dispatch path before this hook fires;
/// arity (exactly 1 arg) is pre-validated by the registry.
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::Union(vec![PhpType::Str, PhpType::False]))
}
