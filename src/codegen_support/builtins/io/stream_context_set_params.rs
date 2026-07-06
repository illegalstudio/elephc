//! Purpose:
//! Emits PHP `stream_context_set_params($ctx, $params)` calls. Used
//! primarily for `notification` callbacks attached to a context.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - Evaluates the context resource for side effects, then captures a literal
//!   `['notification' => <closure|first-class callable>]` entry from the params
//!   array into the `_stream_notification_callback` global (see
//!   `stream_notification::capture_notification_callback`). `__rt_http_open`
//!   fires that callback at the `STREAM_NOTIFY_*` HTTP transfer milestones.
//!   Always returns true (params accepted). v1 fires for `http://` only; HTTPS
//!   and FTP milestones are deferred.

use crate::codegen_support::abi;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits codegen for PHP `stream_context_set_params()` stream and I/O builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_context_set_params()");
    // Evaluate the context resource (args[0]) for its side effects, then capture
    // a literal closure / first-class-callable `notification` entry from the
    // params array (args[1]) into the global so __rt_http_open can fire it at the
    // STREAM_NOTIFY_* milestones.
    if let Some(context_arg) = args.first() {
        emit_expr(context_arg, emitter, ctx, data);
    }
    super::stream_notification::capture_notification_callback(args.get(1), emitter, ctx, data);
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 1);     // return true (params accepted)
    Some(PhpType::Bool)
}
