//! Purpose:
//! Captures a stream context's `notification` callback at codegen time into the
//! `_stream_notification_callback` global so `__rt_http_open` can fire it at the
//! `STREAM_NOTIFY_*` transfer milestones. Shared by `stream_context_create` and
//! `stream_context_set_params`.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::stream_context_create::emit()`.
//! - `crate::codegen_support::builtins::io::stream_context_set_params::emit()`.
//!
//! Key details:
//! - v1 captures ONLY a literal `['notification' => <closure|first-class
//!   callable>]` entry of a literal params array. The captured value must be an
//!   expression that evaluates to a callable descriptor (closures and
//!   first-class callables do); a string / `[object, method]` / variable
//!   callback does not produce a descriptor with the invoker at
//!   `CALLABLE_DESC_INVOKER_OFFSET`, so it is not fired in v1 and the global is
//!   cleared instead. The single-global model matches `_stream_context_options`
//!   (one active context at a time).
//! - The captured descriptor is retained via `emit_retain_current_descriptor`
//!   (a null/rodata-safe incref) so the global slot owns a reference that
//!   survives the surrounding owner's scope-exit cleanup.
//! - The params expression is always emitted for its full side effects before
//!   capture, preserving the prior `emit_expr(&args[..])`-for-side-effects
//!   behavior; a capturable closure entry is then re-emitted to materialize the
//!   descriptor stored into the global.

use crate::codegen_support::abi;
use crate::codegen_support::callable_descriptor::emit_retain_current_descriptor;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::parser::ast::{Expr, ExprKind};

/// Emits the params expression for side effects, then captures a literal
/// `notification` closure / first-class callable into
/// `_stream_notification_callback` (or clears the global when none is present).
///
/// `params` is the optional second argument of `stream_context_create` / the
/// second argument of `stream_context_set_params`. When `params` is `None`
/// (omitted) the global is left untouched, so a bare `stream_context_create([])`
/// does not disturb a previously registered callback.
pub(super) fn capture_notification_callback(
    params: Option<&Expr>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let Some(params) = params else {
        return;
    };

    // Evaluate the full params expression for its side effects (and to build the
    // array), matching the prior side-effect-only behavior. The result is discarded.
    emit_expr(params, emitter, ctx, data);

    if let ExprKind::ArrayLiteralAssoc(entries) = &params.kind {
        if let Some(value) = find_notification_value(entries) {
            if value_is_capturable_callable(value) {
                // Re-emit the callable to materialize a fresh descriptor in the
                // result register, retain it for the global, and store it.
                emit_expr(value, emitter, ctx, data);
                emit_retain_current_descriptor(emitter);
                emit_store_descriptor_to_global(emitter);
                return;
            }
        }
    }

    // No capturable notification closure → ensure a stale callback is cleared so
    // a later HTTP transfer on this context does not fire a previous callback.
    emit_clear_notification_global(emitter);
}

/// Returns the value expression for the last literal `'notification'` key in an
/// associative-array literal (PHP last-wins for duplicate keys), or `None`.
fn find_notification_value(entries: &[(Expr, Expr)]) -> Option<&Expr> {
    let mut found = None;
    for (key, value) in entries {
        if let ExprKind::StringLiteral(name) = &key.kind {
            if name == "notification" {
                found = Some(value);
            }
        }
    }
    found
}

/// Returns true when `value` is a literal closure or first-class callable, the
/// expression kinds that reliably evaluate to a callable descriptor with an
/// invoker at `CALLABLE_DESC_INVOKER_OFFSET`.
fn value_is_capturable_callable(value: &Expr) -> bool {
    matches!(
        value.kind,
        ExprKind::Closure { .. } | ExprKind::FirstClassCallable(_)
    )
}

/// Stores the callable descriptor currently in the integer result register into
/// the `_stream_notification_callback` global.
fn emit_store_descriptor_to_global(emitter: &mut Emitter) {
    let addr_reg = abi::symbol_scratch_reg(emitter);
    let result_reg = abi::int_result_reg(emitter);
    abi::emit_symbol_address(emitter, addr_reg, "_stream_notification_callback");
    abi::emit_store_to_address(emitter, result_reg, addr_reg, 0);
}

/// Clears the `_stream_notification_callback` global so no callback is fired.
fn emit_clear_notification_global(emitter: &mut Emitter) {
    let addr_reg = abi::symbol_scratch_reg(emitter);
    let zero_reg = abi::secondary_scratch_reg(emitter);
    abi::emit_symbol_address(emitter, addr_reg, "_stream_notification_callback");
    abi::emit_load_int_immediate(emitter, zero_reg, 0);
    abi::emit_store_to_address(emitter, zero_reg, addr_reg, 0);
}
