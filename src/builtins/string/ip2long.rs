//! Purpose:
//! Home of the PHP `ip2long` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns the `int|false` union: `ip2long` returns `false` for invalid
//!   IPv4 address strings. A check hook is required because the `builtin!` macro cannot
//!   express a union return type inline.
//! - Argument types are inferred by the common registry dispatch path before the hook fires.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "ip2long",
    area: String,
    params: [ip: Str],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Ip2long,
    ),
    summary: "Converts a string containing an IPv4 address into a long integer.",
    php_manual: "https://www.php.net/manual/en/function.ip2long.php",
}

/// Returns `PhpType::Union([Int, Bool])` for an `ip2long` call.
///
/// The union return (integer on success, false on invalid input) cannot be expressed
/// inline in the `builtin!` macro so a check hook is required.
/// Argument types are inferred by the common registry dispatch path before this hook fires;
/// arity (exactly 1 arg) is pre-validated by the registry.
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::Union(vec![PhpType::Int, PhpType::False]))
}
