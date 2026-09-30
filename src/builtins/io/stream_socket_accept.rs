//! Purpose:
//! Home of the PHP `stream_socket_accept` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates arg[0] is a stream resource. `peer_name` is declared `ref(Str)`, which is
//!   what requires a variable there and binds it to `string`.
//! - Arguments are pre-inferred by the registry before the hook runs, except `peer_name`, which
//!   is written rather than read.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "stream_socket_accept",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamSocketAccept,
    ),
}

/// Validates arg[0] is a stream resource, then returns PHP's `resource|false` result.
///
/// `peer_name` needs no check here: its `ref(Str)` declaration carries the rule.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(cx.checker, cx.name, &cx.args[0], cx.env)?;
    Ok(cx.checker.normalize_union_type(vec![PhpType::stream_resource(), PhpType::False]))
}
