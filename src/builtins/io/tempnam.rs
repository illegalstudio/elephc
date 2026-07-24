//! Purpose:
//! Home of the PHP `tempnam` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - `tempnam` returns either an owned string or `false` when both its requested
//!   directory and platform fallback directory cannot create a temporary file.
//! - The registry common path infers the arguments and enforces the exactly-2-
//!   argument arity before calling the check hook.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "tempnam",
    area: Io,
    params: [directory: Str, prefix: Str],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Tempnam,
    ),
    summary: "Creates a file with a unique filename.",
    php_manual: "function.tempnam",
}

/// Returns `string|false`, matching PHP when temporary-file creation fails.
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::Union(vec![PhpType::Str, PhpType::False]))
}
