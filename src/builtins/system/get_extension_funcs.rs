//! Purpose:
//! Home of the PHP `get_extension_funcs` builtin: its registry declaration and typed semantic
//! target for extension-function introspection.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Extension names are compared case-insensitively, as in php-src.
//! - The date extension inventory is emitted in php-src declaration order; unsupported extension
//!   names return `false`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "get_extension_funcs",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::GetExtensionFuncs,
    ),
}

/// Validates the extension name and returns PHP's `array<string>|false` result type.
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::Union(vec![
        PhpType::Array(Box::new(PhpType::Str)),
        PhpType::False,
    ]))
}
