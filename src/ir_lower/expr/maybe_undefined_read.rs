//! Purpose:
//! Lowers a read of a local the checker recorded as possibly undefined at that point (see
//! `crate::ir_lower::maybe_undefined`).
//!
//! Called from:
//! - `crate::ir_lower::expr::lower_expr` for `ExprKind::Variable`.
//!
//! Key details:
//! - The result is a boxed `mixed`: the value when the local's defined-flag is set, PHP null
//!   otherwise (after an `Undefined variable` warning for a plain read).

use super::*;

/// Lowers a recorded read of a maybe-undefined local, or returns `None` for an ordinary read.
pub(super) fn lower_flagged_read(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    expr: &Expr,
) -> Option<LoweredValue> {
    let warns = crate::ir_lower::maybe_undefined::read_decision(expr.span, name)?;
    let flag = ctx.maybe_undefined_flags.get(name).cloned()?;
    let flag_value = ctx.load_local(&flag, Some(expr.span));
    let temp = ctx.declare_owned_hidden_temp(PhpType::Mixed);
    let defined_block = ctx.builder.create_named_block("maybe_undef.defined", Vec::new());
    let undefined_block = ctx.builder.create_named_block("maybe_undef.undefined", Vec::new());
    let merge = ctx.builder.create_named_block("maybe_undef.merge", Vec::new());
    let split_initialized = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr {
        cond: flag_value.value,
        then_target: defined_block,
        then_args: Vec::new(),
        else_target: undefined_block,
        else_args: Vec::new(),
    });

    // Defined: the slot holds the value the last store left, in the representation the flow
    // types name here.
    ctx.builder.position_at_end(defined_block);
    // A loop body may read, before its own store, what an earlier iteration stored: the slot
    // is declared here as boxed storage, which the later stores widen to.
    if !ctx.local_slots.contains_key(name) && !ctx.loop_stack.is_empty() {
        ctx.declare_local(name, PhpType::Mixed);
    }
    ctx.mark_local_initialized(name);
    let value = if ctx.local_slots.contains_key(name) {
        let loaded = ctx.load_local(name, Some(expr.span));
        if ctx.builder.value_php_type(loaded.value).codegen_repr() == PhpType::Mixed {
            loaded
        } else {
            ctx.box_value_as_mixed(loaded, PhpType::Mixed, Some(expr.span))
        }
    } else {
        null_mixed(ctx, expr.span)
    };
    store_value_into_temp(ctx, &temp, PhpType::Mixed, value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(undefined_block);
    ctx.restore_initialized_slots(split_initialized.clone());
    if warns {
        let warning = Expr::new(
            ExprKind::FunctionCall {
                name: crate::names::Name::unqualified("trigger_error"),
                args: vec![
                    Expr::new(ExprKind::StringLiteral(format!("Undefined variable ${}", name)), expr.span),
                    Expr::new(
                        ExprKind::ConstRef(crate::names::Name::unqualified("E_USER_WARNING")),
                        expr.span,
                    ),
                ],
            },
            expr.span,
        );
        let _ = lower_expr(ctx, &warning);
    }
    let null = null_mixed(ctx, expr.span);
    store_value_into_temp(ctx, &temp, PhpType::Mixed, null, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    ctx.restore_initialized_slots(split_initialized);
    Some(take_owned_temp(ctx, &temp, expr.span))
}

/// A boxed PHP null.
fn null_mixed(ctx: &mut LoweringContext<'_, '_>, span: Span) -> LoweredValue {
    let null = ctx.builder.emit_const_null();
    let null = LoweredValue { value: null, ir_type: crate::ir::IrType::I64 };
    ctx.box_value_as_mixed(null, PhpType::Mixed, Some(span))
}

/// Gives a maybe-undefined local boxed `mixed` storage and stores PHP null into it when its
/// defined-flag is clear: what `use (&$x)` does to an undefined `$x`.
/// The local is defined afterwards (the store sets the flag).
pub(super) fn define_null_if_undefined(ctx: &mut LoweringContext<'_, '_>, name: &str, span: Span) {
    let Some(flag) = ctx.maybe_undefined_flags.get(name).cloned() else {
        return;
    };
    if ctx.is_ref_bound_local(name) {
        return;
    }
    // Frame layout applies the final slot representation to every earlier store.
    let slot = ctx.declare_local(name, PhpType::Mixed);
    ctx.builder.widen_local_storage_type(slot, PhpType::Mixed);
    ctx.set_local_type(name, PhpType::Mixed);
    let flag_value = ctx.load_local(&flag, Some(span));
    let undefined_block = ctx.builder.create_named_block("maybe_undef.define_null", Vec::new());
    let merge = ctx.builder.create_named_block("maybe_undef.defined_merge", Vec::new());
    let split_initialized = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr {
        cond: flag_value.value,
        then_target: merge,
        then_args: Vec::new(),
        else_target: undefined_block,
        else_args: Vec::new(),
    });
    ctx.builder.position_at_end(undefined_block);
    let mut undefined_initialized = split_initialized.clone();
    undefined_initialized.remove(&slot);
    ctx.restore_initialized_slots(undefined_initialized);
    let null = null_mixed(ctx, span);
    ctx.store_local(name, null, PhpType::Mixed, Some(span));
    branch_to(ctx, merge);
    ctx.builder.position_at_end(merge);
    ctx.restore_initialized_slots(split_initialized);
    ctx.mark_local_initialized(name);
}
