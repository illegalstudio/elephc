//! Purpose:
//! Home of the PHP `stream_socket_server` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Union(stream_resource, Bool)` reflecting PHP's false-on-failure return.
//! - `returns: Mixed` is used because the union cannot be expressed through the scalar field.
//! - `lazy_check: true` suppresses the registry's default pre-inference loop so the hook infers
//!   only `$address`. The by-reference `$error_code`/`$error_message` outputs are write-only and
//!   undefined before the call; inferring them would report "Undefined variable".

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "stream_socket_server",
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamSocketServer,
    ),
}

/// Returns `Union(stream_resource, Bool)` reflecting PHP's false-on-failure return.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    Ok(cx.checker.normalize_union_type(vec![PhpType::stream_resource(), PhpType::False]))
}
