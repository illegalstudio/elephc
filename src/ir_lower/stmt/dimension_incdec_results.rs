//! Purpose:
//! Publishes expression results from statement-based dimension incdec kernels.
//!
//! Called from:
//! - Native and ordinary property dimension update lowering.
//!
//! Key details:
//! - Two parser captures distinguish value expressions from discarded updates.
//! - The old value stays independent of the mutable working cell.

use super::*;

/// Parser locals that receive the old value and the detached stepped result.
#[derive(Clone, Copy)]
pub(crate) struct ValueIncDecLocals<'a> { old: &'a str, next: &'a str }

/// Recognizes the parser's old-value and working-copy expression kernel.
pub(crate) fn value_incdec_locals(source: &Expr) -> Option<ValueIncDecLocals<'_>> {
    let ExprKind::Assignment { target, value, prelude, result_target: None,
        conditional_value_temp: None } = &source.kind else { return None; };
    let [old, working] = prelude.as_slice() else { return None; };
    let StmtKind::Assign { name: old, .. } = &old.kind else { return None; };
    let StmtKind::Assign { name: working, value: copy } = &working.kind else { return None; };
    if !matches!(&copy.kind, ExprKind::Variable(name) if name == old)
        || !matches!(&value.kind, ExprKind::PreIncrement(name) | ExprKind::PreDecrement(name)
            if name == working) { return None; }
    let ExprKind::Variable(next) = &target.kind else { return None; };
    if ![old, working, next].into_iter().all(|name| crate::names::is_generated_local_name(name)) {
        return None;
    }
    Some(ValueIncDecLocals { old, next })
}

/// Copies both results into PHP locals before working-slot and receiver cleanup.
pub(crate) fn publish_value_incdec(
    ctx: &mut LoweringContext<'_, '_>, locals: Option<ValueIncDecLocals<'_>>,
    previous: LoweredValue, stepped: LoweredValue, span: Span,
) {
    let Some(locals) = locals else { return; };
    let ty = ctx.builder.value_php_type(previous.value);
    let previous = if Ownership::php_type_needs_lifetime_tracking(&ty) {
        let borrowed = ctx.emit_value(Op::Borrow, vec![previous.value], None, ty.clone(),
            Op::Borrow.default_effects(), Some(span));
        ctx.builder.set_value_ownership(borrowed.value, Ownership::Borrowed);
        borrowed
    } else { previous };
    let previous = crate::ir_lower::ownership::copy_assignment_value(ctx, previous, Some(span));
    ctx.store_call_argument_local(locals.old, previous, ty, Some(span));
    let stepped = crate::ir_lower::ownership::acquire_if_refcounted(ctx, stepped, Some(span));
    let ty = ctx.builder.value_php_type(stepped.value);
    ctx.store_call_argument_local(locals.next, stepped, ty, Some(span));
}
