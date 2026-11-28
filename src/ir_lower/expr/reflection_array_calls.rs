//! Purpose:
//! Evaluates Reflection argument-array calls once before forwarding their values.
//!
//! Called from:
//! - Reflection invocation and constructor argument-array lowering.
//!
//! Key details:
//! - The shared call planner preserves source order before parameter ordering.
//! - Array validation inspects original kinds without converting a Traversable.
//! - Source owners outlive dispatch and the independent boxed result.

use super::*;

/// Refuses unresolved strict invocation contracts instead of entering a permissive Reflection stub.
pub(super) fn refuse_unresolved_reflection_call(
    ctx: &mut LoweringContext<'_, '_>, receiver: LoweredValue, class: &str, method: &str, expr: &Expr,
) -> Option<LoweredValue> {
    if !matches!(method, "invoke" | "invokeargs") { return None; }
    let user = crate::ir_lower::semantic_guards::source_feature_enabled(ctx, elephc_semantics::StrictFeature::Nullability);
    let feature = crate::semantics::reflection::unresolved_invocation_feature(ctx.semantic_profile, user)?;
    let ty = ctx.builder.value_php_type(receiver.value).codegen_repr();
    let message = crate::semantics::reflection::unresolved_invocation_message(feature, class);
    if matches!(&ty, PhpType::Object(name) if php_symbol_key(name) == php_symbol_key(class)) {
        crate::ir_lower::diagnostics::refuse(expr.span, &message);
        release_owning_receiver_temporary(ctx, receiver, expr.span);
        return Some(lower_boxed_null(ctx, expr));
    }
    if matches!(receiver.ir_type, IrType::Heap(IrHeapKind::Object | IrHeapKind::Mixed | IrHeapKind::Union)) {
        let pins = pin_in_flight_owners(ctx, &[receiver.value], expr.span);
        let class = ctx.intern_class_name(class);
        let unsupported = ctx.emit_value(Op::InstanceOf, vec![receiver.value], Some(Immediate::Data(class)),
            PhpType::Bool, Op::InstanceOf.default_effects(), Some(expr.span));
        crate::ir_lower::semantic_guards::require_false(ctx, unsupported, Op::ThrowError, &message, Some(expr.span));
        unpin_in_flight_owners(ctx, pins, expr.span);
    }
    None
}

/// Holds evaluated parameters and their cleanup records through reflected dispatch.
pub(super) struct ReflectionArrayCall {
    reflector: LoweredValue,
    arguments: Vec<Expr>,
    owners: Vec<crate::ir::LocalSlotId>,
    intermediates: Vec<CallArgumentIntermediate>,
    reflector_owner: Option<crate::ir::LocalSlotId>,
    result: Option<PrepublishedCallResult>,
}

impl ReflectionArrayCall {
    /// Borrows the original receiver without transferring its published cleanup owner.
    pub(super) fn reflector(&self, ctx: &mut LoweringContext<'_, '_>, span: Span) -> LoweredValue {
        match self.reflector_owner {
            Some(slot) => load_published_container(ctx, slot,
                ctx.builder.value_php_type(self.reflector.value), span),
            None => self.reflector,
        }
    }

    /// Borrows one already-evaluated parameter without replaying its source expression.
    pub(super) fn argument(&self, index: usize) -> Expr {
        self.arguments[index].clone()
    }

    /// Forwards the actual array so runtime iteration observes collapsed typed keys.
    pub(super) fn unpack(&self, index: usize, span: Span) -> Expr {
        Expr::new(ExprKind::Spread(Box::new(self.argument(index))), span)
    }

    /// Publishes another dispatch operand above the already-rooted source arguments.
    pub(super) fn root_value(
        &mut self, ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Span,
    ) -> Expr {
        let ty = ctx.builder.value_php_type(value.value);
        let name = ctx.declare_hidden_temp(ty.clone());
        store_value_into_temp(ctx, &name, ty.clone(), value, span);
        if Ownership::php_type_needs_lifetime_tracking(&ty) {
            let slot = ctx.local_slots[&name];
            register_owned_call_operand(ctx, slot, span);
            self.owners.push(slot);
        }
        Expr::new(ExprKind::Variable(name), span)
    }
}

/// Evaluates all Reflection parameters before validating the argument array.
pub(super) fn begin_reflection_array_call(
    ctx: &mut LoweringContext<'_, '_>, reflector: LoweredValue, class: &str,
    method: &str, args: &[Expr], array_index: usize, expr: &Expr,
) -> Option<ReflectionArrayCall> {
    let mut signature = ctx.classes.get(class)?.methods.get(&php_symbol_key(method))?.clone();
    // The Reflection API validates its array without selecting its element storage.
    // The reflected callee applies its own parameter conversions after unpacking.
    for (_, ty) in &mut signature.params { *ty = PhpType::Mixed; }
    signature.param_type_exprs.fill(None);
    signature.declared_params.fill(false);
    let result = prepublish_call_result(ctx, &PhpType::Mixed, expr.span);
    let (reflector, reflector_owner) = root_method_receiver(ctx, reflector, expr.span);
    begin_call_argument_evaluation(ctx);
    let mut values = lower_args_with_signature(ctx, Some(&signature), args);
    let intermediates = finish_call_argument_evaluation(ctx, &mut values);
    let mut call = ReflectionArrayCall {
        reflector, arguments: Vec::with_capacity(values.len()), owners: Vec::new(),
        intermediates, reflector_owner, result,
    };
    for value in values {
        let value = lowered_value_from_id(ctx, value);
        let argument = call.root_value(ctx, value, expr.span);
        call.arguments.push(argument);
    }
    let array = lower_expr(ctx, &call.argument(array_index));
    let accepted = crate::ir_lower::semantic_guards::matches_any_kind(
        ctx, array, &[crate::ir::PhpTypePredicate::Array], Some(expr.span),
    );
    crate::ir_lower::semantic_guards::require(ctx, accepted, Op::ThrowTypeError,
        &format!("{class}::{method}(): argument $args must be of type array"), Some(expr.span));
    Some(call)
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
