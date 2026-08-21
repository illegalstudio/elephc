//! Purpose:
//! Home of the PHP `closedir` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates the `dir_handle` argument is a stream resource and returns `Void`.
//! - Arguments are pre-inferred by the registry before the hook runs.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "closedir",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Closedir,
    ),
}

/// Validates the directory handle is a stream resource and returns `Void`.
///
/// `$dir_handle` is OPTIONAL — omitted, it means php's last opened directory stream — so the
/// argument is only type-checked when one is actually written.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    if let Some(arg) = cx.args.first() {
        crate::types::checker::builtins::io::common::ensure_optional_stream_resource(
            cx.checker,
            cx.name,
            arg,
            cx.env,
        )?;
    }
    Ok(PhpType::Void)
}
