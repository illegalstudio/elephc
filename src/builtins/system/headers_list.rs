//! Purpose:
//! Home of PHP's `headers_list` builtin: the pending response headers as `Name: value` lines.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Under `--web` the list is the one `header()` and `header_remove()` edit in the web bridge, in
//!   send order; every other build answers an empty array, as the php CLI does.
//! - The declared contract return is `array`; `check` narrows it to `array<string>`, the shape
//!   `__rt_headers_list` builds.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "headers_list",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::HeadersList,
    ),
}

/// Narrows the declared `array` return to the string list the runtime helper builds.
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::Array(Box::new(PhpType::Str)))
}
