//! Purpose:
//! Home of the PHP `spl_classes` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - A `check` hook is required because the return type `Array<Str>` cannot be
//!   expressed as a plain `TypeSpec` ident in the `builtin!` macro.
//! - The function takes no arguments; arity is enforced by the registry.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "spl_classes",
    area: Spl,
    params: [],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::SplClasses,
    ),
    summary: "Return available SPL classes.",
    php_manual: "https://www.php.net/manual/en/function.spl-classes.php",
}

/// Returns `Array<Str>` as the precise return type for `spl_classes()`.
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::Array(Box::new(PhpType::Str)))
}
