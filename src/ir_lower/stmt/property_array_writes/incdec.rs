//! Purpose:
//! Applies indexed property incdec after selecting its writable receiver.
//!
//! Called from:
//! - Property array assignment lowering when the XML capability is present.
//!
//! Key details:
//! - Native XML selects a node without writing the stepped scalar back.
//! - Ordinary property arrays retain the shared PHP incdec kernel and store path.

use super::*;

/// Selects the native node in write context and discards the PHP scalar update.
pub(in crate::ir_lower::stmt) fn native(
    ctx: &mut LoweringContext<'_, '_>, child: LoweredValue, index: LoweredValue,
    increment: bool, results: Option<ValueIncDecLocals<'_>>, span: Span,
) {
    let expression = Expr::new(ExprKind::Null, span);
    let selected = crate::ir_lower::expr::lower_native_dimension_incdec_select_from_values(
        ctx, child, index, results.is_none(), &expression,
    );
    step(ctx, selected, increment, results.is_some(), span, |ctx, stepped, previous| {
        publish_value_incdec(ctx, results, previous, stepped, span);
    });
}

/// Reads and writes one ordinary property element without replaying its receiver or key.
pub(super) fn ordinary(
    ctx: &mut LoweringContext<'_, '_>, receiver: LoweredValue, property: &str,
    index: LoweredValue, key: &Expr, increment: bool, results: Option<ValueIncDecLocals<'_>>, span: Span,
) {
    // The update reads the element first; PHP rejects a null receiver before that read.
    runtime_receivers::reject_null_receiver(ctx, receiver, property, span);
    runtime_receivers::guard_readwrite(ctx, receiver, property, span);
    let ty = ctx.builder.value_php_type(receiver.value);
    let borrowed = ctx.emit_value(Op::Borrow, vec![receiver.value], None, ty.clone(),
        Op::Borrow.default_effects(), Some(span));
    ctx.builder.set_value_ownership(borrowed.value, Ownership::Borrowed);
    let boxed = matches!(ty.codegen_repr(), PhpType::Object(class)
        if class.is_empty() || ctx.interfaces.contains_key(&class));
    let object = if boxed { ctx.box_value_as_mixed(borrowed, PhpType::Mixed, Some(span)) } else { borrowed };
    let mut roots = Vec::new();
    let object = root_property_write_value(ctx, object, span, &mut roots);
    let expression = Expr::new(ExprKind::StringLiteral(property.into()), span);
    let array = crate::ir_lower::expr::lower_property_get_from_value(ctx, object, property, Op::PropGet, &expression);
    let array = root_property_write_value(ctx, array, span, &mut roots);
    if !super::super::native_dimension_incdec::lower_from_values(ctx, array, index, increment, results, span,
        |ctx, array| ordinary_from_array(ctx, object, property, array, index, key, increment, results, span))
    {
        ordinary_from_array(ctx, object, property, array, index, key, increment, results, span);
    }
    retire_property_write_roots(ctx, roots, span);
}

/// Completes an ordinary property-array update through its already retained container.
fn ordinary_from_array(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str,
    array: LoweredValue, index: LoweredValue, key: &Expr, increment: bool,
    results: Option<ValueIncDecLocals<'_>>, span: Span,
) {
    let expression = Expr::new(ExprKind::ArrayAccess {
        array: Box::new(Expr::new(ExprKind::Null, span)), index: Box::new(key.clone()),
    }, span);
    let selected = crate::ir_lower::expr::lower_array_access_from_values(
        ctx, array, index, key, &expression, true, false,
    );
    step(ctx, selected, increment, results.is_some(), span, |ctx, stored, previous| {
        lower_property_array_assign_value(ctx, object, property, index, stored, span, true);
        publish_value_incdec(ctx, results, previous, stored, span);
    });
}

/// Preserves stable operator result types and roots heap-bearing working slots through the store.
pub(crate) fn step(
    ctx: &mut LoweringContext<'_, '_>, selected: LoweredValue, increment: bool, post: bool, span: Span,
    finish: impl FnOnce(&mut LoweringContext<'_, '_>, LoweredValue, LoweredValue),
) {
    let selected_type = ctx.builder.value_php_type(selected.value);
    let op = if increment { BinOp::Add } else { BinOp::Sub };
    let stable_object_result = crate::types::object_operators::binary_plan_with_classes(
        &op, &selected_type, &PhpType::Int, ctx.classes,
    ).is_some_and(|plan| !plan.result_type(&op).is_int_float_union());
    let temp_type = if stable_object_result { selected_type }
        else if selected_type.codegen_repr() == PhpType::Float { PhpType::Float }
        else { PhpType::Mixed };
    let rooted = Ownership::php_type_needs_lifetime_tracking(&temp_type);
    // The incdec kernel borrows a variable until its overwrite retires the old owner.
    // A one-shot OwnedTemp load would also be consumed by object arithmetic.
    let name = ctx.declare_synthetic_php_local(temp_type.clone());
    let slot = ctx.local_slots[&name];
    if rooted { crate::ir_lower::expr::register_owned_call_operand(ctx, slot, span); }
    crate::ir_lower::expr::store_value_into_temp(ctx, &name, temp_type, selected, span);
    let previous = if post {
        let old = ctx.load_local(&name, Some(span));
        let old = ctx.box_value_as_mixed(old, PhpType::Mixed, Some(span));
        let old = crate::ir_lower::ownership::copy_assignment_value(ctx, old, Some(span));
        Some(crate::ir_lower::expr::root_owned_call_operand(ctx, old, span))
    } else { None };
    warn_null_decrement(ctx, &name, increment, span);
    // Publish a postfix snapshot before the prefix kernel can invoke user code or throw.
    let operation = if increment { ExprKind::PreIncrement(name) } else { ExprKind::PreDecrement(name) };
    let expression = Expr::new(operation, span);
    let result = lower_expr(ctx, &expression);
    let returned = previous.map(|(value, _)| value).unwrap_or(result);
    let ty = ctx.builder.value_php_type(returned.value);
    let borrowed = if Ownership::php_type_needs_lifetime_tracking(&ty) {
        let borrowed = ctx.emit_value(Op::Borrow, vec![returned.value], None, ty,
            Op::Borrow.default_effects(), Some(span));
        ctx.builder.set_value_ownership(borrowed.value, Ownership::Borrowed);
        borrowed
    } else { returned };
    let stepped = borrow_step_result(ctx, result, span);
    finish(ctx, stepped, borrowed);
    crate::ir_lower::ownership::release_if_owned(ctx, result, Some(span));
    if let Some((_, Some(owner))) = previous {
        crate::ir_lower::expr::retire_owned_call_operand(ctx, owner, span);
    }
    if rooted { crate::ir_lower::expr::unregister_owned_call_operand(ctx, slot, span); }
    ctx.emit_void(Op::ReleaseLocalSlot, vec![], Some(Immediate::LocalSlot(slot)),
        Op::ReleaseLocalSlot.default_effects(), Some(span));
}

/// Borrows the working result so retaining stores cannot consume its pending cleanup owner.
fn borrow_step_result(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Span) -> LoweredValue {
    let ty = ctx.builder.value_php_type(value.value);
    if !Ownership::php_type_needs_lifetime_tracking(&ty) { return value; }
    let borrowed = ctx.emit_value(Op::Borrow, vec![value.value], None, ty,
        Op::Borrow.default_effects(), Some(span));
    ctx.builder.set_value_ownership(borrowed.value, Ownership::Borrowed);
    borrowed
}

/// Reports PHP 8.3 null decrement through the warning callback while the operand is rooted.
fn warn_null_decrement(
    ctx: &mut LoweringContext<'_, '_>, name: &str, increment: bool, span: Span,
) {
    if increment || crate::codegen_support::compile_php_version().version_id() < 80300 { return; }
    let value = ctx.load_local(name, Some(span));
    let null = ctx.emit_value(Op::IsNull, vec![value.value], None, PhpType::Bool,
        Op::IsNull.default_effects(), Some(span));
    let warn = ctx.builder.create_named_block("dimension.decrement.null.warning", vec![]);
    let done = ctx.builder.create_named_block("dimension.decrement.null.done", vec![]);
    let initial = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr { cond: null.value,
        then_target: warn, then_args: vec![], else_target: done, else_args: vec![] });
    ctx.builder.position_at_end(warn);
    let diagnostic = Expr::new(ExprKind::FunctionCall {
        name: "__elephc_builtin_warning".into(),
        args: vec![Expr::new(ExprKind::StringLiteral(
            "Decrement on type null has no effect, this will change in the next major version of PHP".into()), span)],
    }, span);
    let result = lower_expr(ctx, &diagnostic);
    crate::ir_lower::ownership::release_if_owned(ctx, result, Some(span));
    let warned = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::Br { target: done, args: vec![] });
    ctx.builder.position_at_end(done);
    ctx.restore_initialized_slots(initial.intersection(&warned).copied().collect());
}
