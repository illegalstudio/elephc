//! Purpose:
//! Home of the PHP `stream_socket_recvfrom` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates arg[0] is a stream resource. `address` needs no check: its by-ref
//!   declaration is what requires a variable, and what binds that variable to php's `?string` —
//!   the null being the answer for a receive that FAILED.
//! - Arguments are pre-inferred by the registry before the hook runs, except `address`, which is
//!   written rather than read.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "stream_socket_recvfrom",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamSocketRecvfrom,
    ),
}

/// Validates arg[0] is a stream resource, then returns PHP's `string|false` result.
///
/// `$address` needs no check here: the by-ref declaration is what requires it to be a variable
/// and what binds it to `?string`, including when the caller passes it undeclared as PHP's own
/// idiom does.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(cx.checker, cx.name, &cx.args[0], cx.env)?;
    Ok(cx.checker.normalize_union_type(vec![PhpType::Str, PhpType::False]))
}
