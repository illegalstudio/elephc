//! Purpose:
//! Lowers typed internal object arithmetic and comparisons through ordinary EIR calls.
//!
//! Called from:
//! - Numeric binary, comparison, unary, and immutable replacement lowering.
//!
//! Key details:
//! - Operands have already been evaluated in source order and are borrowed by adapters.
//! - Fresh results cannot alias operands; strict identity never invokes these handlers.

use super::*;

/// Captures a by-value left operand before later source expressions can replace its storage.
pub(super) fn capture_left_operand(
    ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Span,
) -> (LoweredValue, Vec<super::call_operand_owners::PinnedInFlightOwner>) {
    if ctx.classes.keys().any(|name| elephc_builtin_contract::lookup_class(name).is_some_and(|class| class.operators.is_some())) {
        let ty = ctx.builder.value_php_type(value.value);
        let captured = if matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
            ctx.emit_owned_value(Op::MixedClone, vec![value.value], None, ty,
                Op::MixedClone.default_effects(), Some(span))
        } else {
            crate::ir_lower::ownership::acquire_lifetime_pin_if_refcounted(ctx, value, Some(span))
        };
        if captured.value != value.value && ctx.value_is_owning_temporary(value) {
            crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
        }
        let pins = pin_in_flight_owners(ctx, &[captured.value], span);
        (captured, pins)
    } else { (value, Vec::new()) }
}

/// Dispatches a boxed operand through available class contracts before scalar lowering.
pub(crate) fn lower_dynamic_object_binary(
    ctx: &mut LoweringContext<'_, '_>, lhs: LoweredValue, op: &BinOp, rhs: LoweredValue,
    expr: &Expr, consume: bool, scalar: impl FnOnce(&mut LoweringContext<'_, '_>) -> LoweredValue,
) -> Option<LoweredValue> {
    let lt = ctx.builder.value_php_type(lhs.value);
    let rt = ctx.builder.value_php_type(rhs.value);
    if !matches!(lt.codegen_repr(), PhpType::Mixed | PhpType::Union(_) | PhpType::Object(_))
        && !matches!(rt.codegen_repr(), PhpType::Mixed | PhpType::Union(_) | PhpType::Object(_)) { return None; }
    let plan = ctx.classes.keys().find_map(|name| {
        crate::types::object_operators::binary_plan(op, &PhpType::Object(name.clone()), &PhpType::Mixed)
    })?;
    let result_type = if plan.comparison { plan.result_type(op) } else { PhpType::Mixed };
    let class_name = ctx.intern_class_name(plan.class.name);
    let left_instance = ctx.emit_value(Op::InstanceOf, vec![lhs.value], Some(Immediate::Data(class_name)),
        PhpType::Bool, Op::InstanceOf.default_effects(), Some(expr.span));
    let right_instance = ctx.emit_value(Op::InstanceOf, vec![rhs.value], Some(Immediate::Data(class_name)),
        PhpType::Bool, Op::InstanceOf.default_effects(), Some(expr.span));
    let condition = ctx.emit_value(Op::IBitOr, vec![left_instance.value, right_instance.value], None,
        PhpType::Bool, Op::IBitOr.default_effects(), Some(expr.span));
    let temp = ctx.declare_owned_hidden_temp(result_type.clone());
    let initialized = ctx.initialized_slots_snapshot();
    let object_block = ctx.builder.create_named_block("object.operator", vec![]);
    let scalar_block = ctx.builder.create_named_block("object.scalar", vec![]);
    let merge = ctx.builder.create_named_block("object.merge", vec![]);
    ctx.builder.terminate(Terminator::CondBr { cond: condition.value,
        then_target: object_block, then_args: vec![], else_target: scalar_block, else_args: vec![] });
    ctx.builder.position_at_end(object_block);
    let value = emit_object_binary_plan(ctx, plan, lhs, op, rhs, expr, consume);
    store_value_into_temp(ctx, &temp, result_type.clone(), value, expr.span);
    branch_to(ctx, merge);
    let object_initialized = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(scalar_block);
    ctx.restore_initialized_slots(initialized.clone());
    let scalar_pins = pin_in_flight_owners(ctx, &[lhs.value, rhs.value], expr.span);
    let value = scalar(ctx);
    unpin_in_flight_owners(ctx, scalar_pins, expr.span);
    store_value_into_temp(ctx, &temp, result_type, value, expr.span);
    branch_to(ctx, merge);
    let scalar_initialized = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(merge);
    ctx.restore_initialized_slots(merge_initialized_slots_for_expr(&initialized,
        object_initialized, true, scalar_initialized, true));
    Some(take_owned_temp(ctx, &temp, expr.span))
}

/// Emits the reviewed static adapter for two evaluated operands and retires its arguments.
pub(super) fn lower_object_binary_values(
    ctx: &mut LoweringContext<'_, '_>,
    lhs: LoweredValue,
    op: &BinOp,
    rhs: LoweredValue,
    expr: &Expr,
) -> Option<LoweredValue> {
    let lt = ctx.builder.value_php_type(lhs.value);
    let rt = ctx.builder.value_php_type(rhs.value);
    let plan = crate::types::object_operators::binary_plan(op, &lt, &rt)?;
    Some(emit_object_binary_plan(ctx, plan, lhs, op, rhs, expr, true))
}

/// Emits an already resolved adapter, including a runtime-proven class branch.
fn emit_object_binary_plan(
    ctx: &mut LoweringContext<'_, '_>, plan: crate::types::object_operators::ObjectOperatorPlan,
    lhs: LoweredValue, op: &BinOp, rhs: LoweredValue, expr: &Expr, consume: bool,
) -> LoweredValue {
    let left_arg = box_operand(ctx, lhs, expr.span);
    let right_arg = box_operand(ctx, rhs, expr.span);
    let name = ctx.intern_string(&format!("{}::{}", plan.class.name, plan.method));
    let signature = ctx.classes.get(plan.class.name)
        .and_then(|class| class.static_methods.get(plan.method)).cloned();
    let mut operands = vec![left_arg.value, right_arg.value];
    let extra = append_hidden_arguments(ctx, signature.as_ref(), &mut operands, 2, expr);
    let result_type = if plan.comparison { PhpType::Int } else { plan.result_type(op) };
    let effects = Op::StaticMethodCall.default_effects();
    let mut pin_values = vec![lhs.value, rhs.value];
    pin_values.extend(operands.iter().copied());
    let pins = pin_in_flight_owners(ctx, &pin_values, expr.span);
    let result = if plan.comparison {
        ctx.emit_value(Op::StaticMethodCall, operands,
            Some(Immediate::Data(name)), result_type, effects, Some(expr.span))
    } else {
        ctx.emit_owned_value(Op::StaticMethodCall, operands,
            Some(Immediate::Data(name)), result_type, effects, Some(expr.span))
    };
    unpin_in_flight_owners(ctx, pins, expr.span);
    let mut retired = HashSet::new();
    for argument in [left_arg, right_arg, lhs, rhs].into_iter().chain(extra) {
        if !consume && (argument.value == lhs.value || argument.value == rhs.value) { continue; }
        if retired.insert(argument.value) {
            release_binary_operand_temporary(ctx, argument, expr.span);
        }
    }
    if !plan.comparison { return result; }
    if matches!(op, BinOp::Spaceship) {
        let unordered = lower_int_literal(ctx, elephc_builtin_contract::ObjectOrdering::Unordered as i64, expr);
        let adjust = ctx.emit_value(Op::ICmp, vec![result.value, unordered.value],
            Some(Immediate::CmpPredicate(CmpPredicate::Eq)), PhpType::Bool,
            Op::ICmp.default_effects(), Some(expr.span));
        return ctx.emit_value(Op::ISub, vec![result.value, adjust.value], None, PhpType::Int,
            Op::ISub.default_effects(), Some(expr.span));
    }
    let zero = lower_int_literal(ctx, 0, expr);
    if matches!(op, BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq) {
        let ordered = lower_int_literal(ctx, if matches!(op, BinOp::Lt | BinOp::LtEq) { -1 } else { 1 }, expr);
        let equal_order = ctx.emit_value(Op::ICmp, vec![result.value, ordered.value],
            Some(Immediate::CmpPredicate(CmpPredicate::Eq)), PhpType::Bool,
            Op::ICmp.default_effects(), Some(expr.span));
        if matches!(op, BinOp::Lt | BinOp::Gt) { return equal_order; }
        let equal = ctx.emit_value(Op::ICmp, vec![result.value, zero.value],
            Some(Immediate::CmpPredicate(CmpPredicate::Eq)), PhpType::Bool,
            Op::ICmp.default_effects(), Some(expr.span));
        return ctx.emit_value(Op::IBitOr, vec![equal.value, equal_order.value], None, PhpType::Bool,
            Op::IBitOr.default_effects(), Some(expr.span));
    }
    ctx.emit_value(Op::ICmp, vec![result.value, zero.value],
        Some(Immediate::CmpPredicate(cmp_predicate(op))), PhpType::Bool,
        Op::ICmp.default_effects(), Some(expr.span))
}

/// Boxes a borrowed operand without transferring or releasing its original owner.
fn box_operand(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Span) -> LoweredValue {
    if matches!(value.ir_type, IrType::Heap(IrHeapKind::Mixed)) { return value; }
    ctx.emit_value(Op::MixedBox, vec![value.value], None, PhpType::Mixed,
        Op::MixedBox.default_effects(), Some(span))
}

/// Uses the ordinary argument collector contract for adapters retained by eval/introspection.
fn append_hidden_arguments(
    ctx: &mut LoweringContext<'_, '_>, signature: Option<&FunctionSig>, operands: &mut Vec<ValueId>,
    actual_count: usize, expr: &Expr,
) -> Vec<LoweredValue> {
    let Some(signature) = signature else { return vec![] };
    if !crate::func_args::sig_collects_surplus_args(signature) { return vec![]; }
    if crate::func_args::sig_has_hidden_argc_param(signature) {
        operands.push(lower_int_literal(ctx, actual_count as i64, expr).value);
    }
    let tail = lower_variadic_tail_array(ctx, signature, &[], actual_count);
    operands.push(tail.value);
    vec![tail]
}

/// Calls an internal class's truthiness adapter while retaining the input for its caller.
pub(crate) fn lower_object_truthy(
    ctx: &mut LoweringContext<'_, '_>, input: LoweredValue, span: Option<Span>,
) -> Option<LoweredValue> {
    let ty = ctx.builder.value_php_type(input.value);
    if let Some(class) = crate::types::object_operators::operator_class(&ty) {
        let method = class.operators?.boolean?;
        let data = ctx.intern_string(method);
        let signature = ctx.classes.get(class.name).and_then(|class| class.methods.get(method)).cloned();
        let expr = Expr::new(ExprKind::Null, span.unwrap_or_else(Span::dummy));
        let mut operands = vec![input.value];
        let extra = append_hidden_arguments(ctx, signature.as_ref(), &mut operands, 0, &expr);
        let pins = pin_in_flight_owners(ctx, &operands, expr.span);
        let result = ctx.emit_value(Op::MethodCall, operands, Some(Immediate::Data(data)),
            PhpType::Bool, Op::MethodCall.default_effects(), span);
        unpin_in_flight_owners(ctx, pins, expr.span);
        for value in extra { release_binary_operand_temporary(ctx, value, expr.span); }
        return Some(result);
    }
    if !matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_) | PhpType::Object(_)) { return None; }
    let expr = Expr::new(ExprKind::Null, span.unwrap_or_else(Span::dummy));
    let zero = lower_int_literal(ctx, 0, &expr);
    lower_dynamic_object_binary(ctx, input, &BinOp::NotEq, zero, &expr, false, |ctx| {
        ctx.emit_value(Op::IsTruthy, vec![input.value], None,
            PhpType::Bool, Op::IsTruthy.default_effects(), span)
    })
}
