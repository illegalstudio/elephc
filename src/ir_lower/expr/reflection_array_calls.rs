//! Purpose:
//! Evaluates Reflection argument-array calls once before forwarding their values.
//!
//! Called from:
//! - ReflectionFunction and ReflectionMethod invokeArgs lowering.
//!
//! Key details:
//! - The shared call planner preserves source order before parameter ordering.
//! - Array validation inspects original kinds without converting a Traversable.
//! - Source owners outlive dispatch and the independent boxed result.

use super::*;

/// Holds evaluated parameters and their cleanup records through reflected dispatch.
pub(super) struct ReflectionArrayCall {
    arguments: Vec<Expr>,
    owners: Vec<crate::ir::LocalSlotId>,
    intermediates: Vec<CallArgumentIntermediate>,
    reflector_owner: Option<crate::ir::LocalSlotId>,
    result: Option<PrepublishedCallResult>,
}

impl ReflectionArrayCall {
    /// Borrows one already-evaluated parameter without replaying its source expression.
    pub(super) fn argument(&self, index: usize) -> Expr {
        self.arguments[index].clone()
    }

    /// Forwards the actual array so runtime iteration observes collapsed typed keys.
    pub(super) fn unpack(&self, index: usize, span: Span) -> Expr {
        Expr::new(ExprKind::Spread(Box::new(self.argument(index))), span)
    }
}

/// Evaluates all Reflection parameters before validating the argument array.
pub(super) fn begin_reflection_array_call(
    ctx: &mut LoweringContext<'_, '_>, reflector: LoweredValue, class: &str,
    args: &[Expr], array_index: usize, expr: &Expr,
) -> Option<ReflectionArrayCall> {
    let mut signature = ctx.classes.get(class)?.methods.get("invokeargs")?.clone();
    // The Reflection API validates its array without selecting its element storage.
    // The reflected callee applies its own parameter conversions after unpacking.
    for (_, ty) in &mut signature.params { *ty = PhpType::Mixed; }
    signature.param_type_exprs.fill(None);
    signature.declared_params.fill(false);
    let result = prepublish_call_result(ctx, &PhpType::Mixed, expr.span);
    let (_, reflector_owner) = root_method_receiver(ctx, reflector, expr.span);
    begin_call_argument_evaluation(ctx);
    let mut values = lower_args_with_signature(ctx, Some(&signature), args);
    let intermediates = finish_call_argument_evaluation(ctx, &mut values);
    let mut arguments = Vec::with_capacity(values.len());
    let mut owners = Vec::new();
    for value in values {
        let value = lowered_value_from_id(ctx, value);
        let ty = ctx.builder.value_php_type(value.value);
        let name = ctx.declare_hidden_temp(ty.clone());
        store_value_into_temp(ctx, &name, ty.clone(), value, expr.span);
        if Ownership::php_type_needs_lifetime_tracking(&ty) {
            let slot = ctx.local_slots[&name];
            register_owned_call_operand(ctx, slot, expr.span);
            owners.push(slot);
        }
        arguments.push(Expr::new(ExprKind::Variable(name), expr.span));
    }
    let array = lower_expr(ctx, &arguments[array_index]);
    let accepted = crate::ir_lower::semantic_guards::matches_any_kind(
        ctx, array, &[crate::ir::PhpTypePredicate::Array], Some(expr.span),
    );
    crate::ir_lower::semantic_guards::require(ctx, accepted, Op::ThrowTypeError,
        &format!("{class}::invokeArgs(): argument $args must be of type array"), Some(expr.span));
    Some(ReflectionArrayCall { arguments, owners, intermediates, reflector_owner, result })
}

/// Gives the Reflection result an independent value before retiring its array and receiver.
pub(super) fn finish_reflection_array_call(
    ctx: &mut LoweringContext<'_, '_>, call: ReflectionArrayCall,
    value: LoweredValue, span: Span,
) -> LoweredValue {
    let value = ctx.box_value_as_mixed(value, PhpType::Mixed, Some(span));
    stage_call_result(ctx, call.result.as_ref(), value, span);
    for owner in call.owners.into_iter().rev() {
        retire_owned_call_operand(ctx, owner, span);
    }
    retire_call_argument_intermediates(ctx, &call.intermediates);
    if let Some(owner) = call.reflector_owner { retire_owned_call_operand(ctx, owner, span); }
    take_prepublished_call_result(ctx, call.result, value, span)
}
