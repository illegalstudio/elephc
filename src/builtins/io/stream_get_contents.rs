//! Purpose:
//! Home of the PHP `stream_get_contents` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates the stream resource, then validates `length` (int|null) and `offset`
//!   (int) via `stream_support` helpers. Returns `Union(Str, Bool)`.
//! - `length` and `offset` are optional with defaults `null` and `-1` respectively.
//! - `returns: Mixed` is used because the union cannot be expressed through the scalar field.
//! - `lower` is a thin wrapper over `io::lower_stream_get_contents` in the EIR backend.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::builtins::io::stream_support;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;
use crate::types::checker::builtins::io::common;

builtin! {
    name: "stream_get_contents",
    area: Io,
    params: [
        stream: Mixed,
        length: Int = DefaultSpec::Null,
        offset: Int = DefaultSpec::Int(-1)
    ],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Reads remainder of a stream into a string.",
    php_manual: "function.stream-get-contents",
}

/// Validates the stream resource, optional length (int|null), and optional offset (int).
/// Returns `Union(Str, Bool)` reflecting PHP's false-on-failure return.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    common::ensure_stream_resource(cx.checker, cx.name, &cx.args[0], cx.env)?;
    if let Some(length) = cx.args.get(1) {
        stream_support::ensure_optional_int(cx.checker, cx.name, "length", length, cx.env)?;
    }
    if let Some(offset) = cx.args.get(2) {
        stream_support::ensure_int(cx.checker, cx.name, "offset", offset, cx.env)?;
    }
    Ok(cx.checker.normalize_union_type(vec![PhpType::Str, PhpType::False]))
}

/// Lowers a `stream_get_contents` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_stream_get_contents(ctx, inst)
}
