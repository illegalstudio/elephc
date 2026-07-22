//! Purpose:
//! Home of the PHP `str_split` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `PhpType::Array(Box::new(PhpType::Str))`. A check hook is
//!   required because the `builtin!` macro `returns:` field only accepts a simple
//!   type identifier and cannot express `ArrayOf(Str)` inline. Argument types are
//!   inferred by the common registry dispatch path before the hook fires.
//! - Arity is validated by the registry's `check_arity` before the check hook fires;
//!   the inline arity check from the legacy arm is therefore not reproduced here.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::builtins::spec::DefaultSpec;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "str_split",
    area: String,
    params: [
        string: Str,
        length: Int = DefaultSpec::Int(1),
    ],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StrSplit,
    ),
    summary: "Converts a string into an array of chunks of the given length.",
    php_manual: "https://www.php.net/manual/en/function.str-split.php",
}

/// Returns `PhpType::Array(Box::new(PhpType::Str))` for a `str_split` call.
///
/// A check hook is required because the `builtin!` macro cannot express array
/// return types inline. Argument types are inferred by the common registry
/// dispatch path before this hook fires; arity is pre-validated by the registry.
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::Array(Box::new(PhpType::Str)))
}
