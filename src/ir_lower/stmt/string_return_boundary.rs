//! Purpose:
//! Verifies declared string returns and preserves PHP's conversion exception chain.
//!
//! Called from:
//! - `return_coercions::coerce_to_return_type()` for a non-string declared return value.
//!
//! Key details:
//! - Validation is separate from casting: null/arrays/resources/non-Stringable objects fail.
//! - A failed implicit Stringable conversion becomes TypeError with the original previous.
//! - Explicit source casts execute before this boundary and retain their ordinary exceptions.

use super::*;
use crate::synthetic_class::{e_binop, e_call, e_int, e_new_fq, e_str, e_var};

/// Validates a declared string return, protects its owners and wraps implicit conversion failures.
pub(super) fn lower(
    ctx: &mut LoweringContext<'_, '_>, source: LoweredValue, span: Span,
) -> LoweredValue {
    let source_slot = hold(ctx, source, span);
    let source = ctx.load_local(&source_slot, Some(span));
    let valid = ctx.emit_value(
        Op::StringBindingAccepted, vec![source.value],
        Some(Immediate::Bool(ctx.argument_strict_types)), PhpType::Bool,
        Op::StringBindingAccepted.default_effects(), Some(span),
    );
    let convert = ctx.builder.create_named_block("return.string_convert", Vec::new());
    let invalid = ctx.builder.create_named_block("return.string_invalid", Vec::new());
    let handler = ctx.builder.create_named_block("return.string_wrap", Vec::new());
    let after = ctx.builder.create_named_block("return.string_after", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: valid.value, then_target: convert, then_args: Vec::new(),
        else_target: invalid, else_args: Vec::new(),
    });
    ctx.builder.position_at_end(invalid);
    throw_type_error(ctx, &source_slot, None, span);

    ctx.builder.position_at_end(convert);
    ctx.emit_void(
        Op::TryPushHandler, Vec::new(), Some(Immediate::I64(handler.as_raw() as i64)),
        Op::TryPushHandler.default_effects(), Some(span),
    );
    let source = ctx.load_local(&source_slot, Some(span));
    let result = coerce_to_string(ctx, source, Some(span));
    let result_slot = hold(ctx, result, span);
    pop_handler(ctx, handler, span);
    release_slot(ctx, &source_slot, span);
    ctx.builder.terminate(Terminator::Br { target: after, args: Vec::new() });

    ctx.builder.position_at_end(handler);
    ctx.clear_static_callable_locals();
    pop_handler(ctx, handler, span);
    let previous = ctx.emit_owned_value(
        Op::CatchBind, Vec::new(), None, PhpType::Object("Throwable".to_string()),
        Op::CatchBind.default_effects(), Some(span),
    );
    let previous_slot = ctx.declare_owned_hidden_temp(PhpType::Object("Throwable".to_string()));
    ctx.store_local(&previous_slot, previous, PhpType::Object("Throwable".to_string()), Some(span));
    throw_type_error(ctx, &source_slot, Some(&previous_slot), span);

    ctx.builder.position_at_end(after);
    ctx.clear_static_callable_locals();
    let result = ctx.load_local(&result_slot, Some(span));
    ctx.clear_owned_hidden_temp(&result_slot, Some(span));
    result
}

/// Holds one balanced source lease in an owned hidden slot across conversion or diagnostic creation.
fn hold(ctx: &mut LoweringContext<'_, '_>, source: LoweredValue, span: Span) -> String {
    let ty = ctx.builder.value_php_type(source.value);
    let slot = ctx.declare_owned_hidden_temp(ty.clone());
    let held = crate::ir_lower::ownership::acquire_if_refcounted(ctx, source, Some(span));
    ctx.store_local(&slot, held, ty, Some(span));
    if held.value != source.value && ctx.value_needs_release_after_retaining_store(source) {
        crate::ir_lower::ownership::release_if_owned(ctx, source, Some(span));
    }
    slot
}

/// Retires the hidden slot's owned value and clears its frame-cleanup ownership record.
fn release_slot(ctx: &mut LoweringContext<'_, '_>, slot: &str, span: Span) {
    let value = ctx.load_local(slot, Some(span));
    crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    ctx.clear_owned_hidden_temp(slot, Some(span));
}

/// Removes the conversion boundary's exception handler on normal and exceptional paths.
fn pop_handler(ctx: &mut LoweringContext<'_, '_>, handler: BlockId, span: Span) {
    ctx.emit_void(
        Op::TryPopHandler, Vec::new(), Some(Immediate::I64(handler.as_raw() as i64)),
        Op::TryPopHandler.default_effects(), Some(span),
    );
}

/// Constructs the return TypeError through borrowed aliases, then retires source and cause leases.
fn throw_type_error(ctx: &mut LoweringContext<'_, '_>, source: &str, previous: Option<&str>, span: Span) {
    let prefix = format!("{}(): Return value must be of type string, ", ctx.owner_name());
    // Ordinary AST call lowering may consume owned temporary reads. Expose only
    // borrowed aliases while this boundary remains the owner of source/cause.
    let source_alias = borrowed_alias(ctx, source, span);
    let previous_alias = previous.map(|previous| borrowed_alias(ctx, previous, span));
    let message = e_binop(
        e_binop(e_str(&prefix), crate::parser::ast::BinOp::Concat,
            debug_type_expr(&source_alias)),
        crate::parser::ast::BinOp::Concat, e_str(" returned"),
    );
    let mut args = vec![message, e_int(0)];
    if let Some(previous) = &previous_alias { args.push(e_var(previous)); }
    let error = lower_expr(ctx, &e_new_fq("TypeError", args));
    clear_borrowed_alias(ctx, &source_alias, span);
    if let Some(alias) = previous_alias { clear_borrowed_alias(ctx, &alias, span); }
    release_slot(ctx, source, span);
    if let Some(previous) = previous { release_slot(ctx, previous, span); }
    ctx.builder.terminate(Terminator::Throw { value: error.value });
}

/// Exposes an owned boundary value to ordinary AST call lowering without transferring its lease.
fn borrowed_alias(ctx: &mut LoweringContext<'_, '_>, owner: &str, span: Span) -> String {
    let value = ctx.load_local(owner, Some(span));
    let ty = ctx.builder.value_php_type(value.value);
    let alias = ctx.declare_hidden_temp(ty.clone());
    ctx.store_local(&alias, value, ty, Some(span));
    alias
}

/// Zeros a borrowed hidden slot without releasing the boundary owner's reference a second time.
fn clear_borrowed_alias(ctx: &mut LoweringContext<'_, '_>, alias: &str, span: Span) {
    let slot = ctx.local_slots[alias];
    // HiddenTemp participates in frame cleanup too. This slot only borrowed
    // the boundary's owner; zero it without releasing the same reference twice.
    ctx.emit_void(Op::ZeroLocalSlot, Vec::new(), Some(Immediate::LocalSlot(slot)),
        Op::ZeroLocalSlot.default_effects(), Some(span));
}

/// Builds PHP debug-type names from existing type/class primitives.
fn debug_type_expr(source: &str) -> Expr {
    let ty = e_call("gettype", vec![e_var(source)]);
    let mut result = ty.clone();
    for (legacy, name) in [("integer", "int"), ("double", "float"), ("boolean", "bool"), ("NULL", "null")] {
        result = Expr::new(ExprKind::Ternary {
            condition: Box::new(e_binop(ty.clone(), crate::parser::ast::BinOp::StrictEq, e_str(legacy))),
            then_expr: Box::new(e_str(name)), else_expr: Box::new(result),
        }, Span::synthetic());
    }
    Expr::new(ExprKind::Ternary {
        condition: Box::new(e_call("is_object", vec![e_var(source)])),
        then_expr: Box::new(e_call("get_class", vec![e_var(source)])),
        else_expr: Box::new(result),
    }, Span::synthetic())
}
