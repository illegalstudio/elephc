//! Purpose:
//! Emits PHP `opendir` calls.
//! Opens a directory stream and yields it as a PHP stream resource.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - The `__rt_opendir` helper returns the directory descriptor or -1; the
//!   result is boxed by the shared `box_socket_result` helper as `resource|false`.

use crate::codegen_support::abi;
use crate::codegen_support::builtins::io::stream_socket_server::box_socket_result;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits codegen for PHP `opendir()` stream and I/O builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("opendir()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_opendir");
    box_socket_result(emitter, ctx);
    Some(PhpType::Mixed)
}
