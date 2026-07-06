//! Purpose:
//! Emits compiler-extension `buffer_len` operations for runtime buffer values.
//! Keeps buffer pointer/length ABI handling near array-like builtin dispatch.
//!
//! Called from:
//! - `crate::codegen_support::builtins::arrays::emit()`.
//!
//! Key details:
//! - Buffer helpers operate on raw runtime handles and must not treat them as PHP arrays.

use crate::codegen_support::abi;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the `buffer_len` builtin call.
///
/// Validates the argument is a buffer type (emits a warning if not), then calls
/// the runtime helper `__rt_buffer_len` to extract the logical element count from
/// the buffer header. Returns `PhpType::Int` unconditionally.
///
/// # Arguments
/// * `name` - Unused, present for dispatcher signature uniformity.
/// * `args` - Must contain exactly one expression evaluating to a buffer.
/// * `emitter` - Target-aware assembly emitter.
/// * `ctx` - Codegen context carrying variable layouts and metadata.
/// * `data` - Data section for constants and metadata tables.
///
/// # Returns
/// Always returns `Some(PhpType::Int)` representing the buffer's element count.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    let buf_ty = emit_expr(&args[0], emitter, ctx, data);
    if !matches!(buf_ty, PhpType::Buffer(_)) {
        emitter.comment("WARNING: buffer_len() received a non-buffer argument");
    }
    abi::emit_call_label(emitter, "__rt_buffer_len");                           // load the logical element count from the buffer header through the target-aware runtime helper
    Some(PhpType::Int)
}
