//! Purpose:
//! Home of the PHP `sscanf` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Accepts required `string` and `format` params plus a by-reference variadic `vars` list.
//! - Calls without output variables return `array<mixed>`; calls with outputs return the
//!   assignment count after writing parsed values into caller storage.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "sscanf",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Sscanf,
    ),
}

/// Returns the output-variable assignment count or a parsed Mixed array.
///
/// A check hook is required because the `builtin!` macro cannot express a
/// parameterized array return type or the call-shape-dependent integer result inline.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    if cx.args.len() > 2 {
        Ok(PhpType::Int)
    } else {
        Ok(PhpType::Array(Box::new(PhpType::Mixed)))
    }
}
