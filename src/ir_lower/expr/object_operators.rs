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
use crate::ir::RuntimeCallTarget;

/// Distinguishes possible PHP objects from scalar unions that merely use boxed storage.
pub(super) fn can_contain_object(ty: &PhpType) -> bool {
    match ty {
        PhpType::Mixed | PhpType::Object(_) => true,
        PhpType::Union(members) => members.iter().any(can_contain_object),
        _ => false,
    }
}

/// Captures a by-value left operand before later source expressions can replace its storage.
pub(super) fn capture_left_operand(
    ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, right: &Expr, span: Span,
) -> (LoweredValue, Vec<super::call_operand_owners::PinnedInFlightOwner>) {
    if ctx.builder.value_defining_op(value.value) == Some(Op::StrCharAt) {
        let captured = crate::ir_lower::ownership::persist_string_byte_view(ctx, value, Some(span));
        let pins = pin_in_flight_owners(ctx, &[captured.value], span);
        return (captured, pins);
    }
    let ty = ctx.builder.value_php_type(value.value);
    // A scalar literal cannot replace the borrowed numeric box before the operator reads it.
    // Keep owning temporaries and every potentially reentrant RHS on the published-owner path.
    if !ctx.value_is_owning_temporary(value)
        && matches!(right.kind, ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_)
            | ExprKind::BoolLiteral(_) | ExprKind::Null)
        && matches!(&ty, PhpType::Union(members) if members.iter().all(|member|
            matches!(member, PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::False | PhpType::Void)))
    {
        return (value, Vec::new());
    }
    if ctx.classes.keys().any(|name| elephc_builtin_contract::lookup_class(name).is_some_and(|class| class.operators.is_some())) {
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
    if !can_contain_object(&lt) && !can_contain_object(&rt) { return None; }
    let mut plans = ctx.classes.keys().filter_map(|name| {
        crate::types::object_operators::binary_plan(op, &PhpType::Object(name.clone()), &PhpType::Mixed)
    }).collect::<Vec<_>>();
    plans.sort_unstable_by_key(|plan| plan.class.name);
    plans.dedup_by_key(|plan| plan.class.name);
    if plans.is_empty() { return None; }
    Some(lower_dynamic_operator_plans(ctx, lhs, op, rhs, expr, consume, &plans, false, scalar))
}

/// Negates numeric-result classes with multiplication while retaining other classes' subtraction.
pub(super) fn lower_dynamic_object_negate(
    ctx: &mut LoweringContext<'_, '_>, zero: LoweredValue, value: LoweredValue,
    expr: &Expr, scalar: impl FnOnce(&mut LoweringContext<'_, '_>) -> LoweredValue,
) -> Option<LoweredValue> {
    let ty = ctx.builder.value_php_type(value.value);
    if !can_contain_object(&ty) { return None; }
    let mut plans = ctx.classes.keys().filter_map(|name| {
        let ty = PhpType::Object(name.clone());
        let class = crate::types::object_operators::operator_class(&ty)?;
        let operation = if class.operators?.arithmetic_result == elephc_builtin_contract::ObjectArithmeticResult::Number {
            BinOp::Mul
        } else { BinOp::Sub };
        crate::types::object_operators::binary_plan(&operation, &PhpType::Int, &ty)
    }).collect::<Vec<_>>();
    plans.sort_unstable_by_key(|plan| plan.class.name);
    plans.dedup_by_key(|plan| plan.class.name);
    if plans.is_empty() { return None; }
    Some(lower_dynamic_operator_plans(ctx, zero, &BinOp::Sub, value, expr, true, &plans, true, scalar))
}

/// Tries native handlers in operand order before numeric conversions and the scalar path.
fn lower_dynamic_operator_plans(
    ctx: &mut LoweringContext<'_, '_>, lhs: LoweredValue, op: &BinOp, rhs: LoweredValue,
    expr: &Expr, consume: bool, plans: &[crate::types::object_operators::ObjectOperatorPlan], negate: bool,
    scalar: impl FnOnce(&mut LoweringContext<'_, '_>) -> LoweredValue,
) -> LoweredValue {
    let result_type = if plans[0].comparison { plans[0].result_type(op) } else { PhpType::Mixed };
    let temp = ctx.declare_owned_hidden_temp(result_type.clone());
    let initialized = ctx.initialized_slots_snapshot();
    let scalar_block = ctx.builder.create_named_block("object.scalar", vec![]);
    let merge = ctx.builder.create_named_block("object.merge", vec![]);
    let mut call_initialized: Option<HashSet<LocalSlotId>> = None;
    let mut candidates = [true, false].into_iter().flat_map(|left|
        plans.iter().copied().map(move |plan| (left, plan))).collect::<Vec<_>>();
    if !plans[0].comparison {
        // Stable sorting preserves left-first order within each dispatch protocol.
        candidates.sort_by_key(|(_, plan)| plan.class.operators.unwrap().operand_dispatch);
    }
    let checks = candidates.len();
    let mut emitted = 0;
    for (left_precedence, plan) in candidates {
        emitted += 1;
        let operand = if left_precedence { lhs.value } else { rhs.value };
        let class_name = ctx.intern_class_name(plan.class.name);
        let instance = ctx.emit_value(Op::InstanceOf, vec![operand], Some(Immediate::Data(class_name)),
            PhpType::Bool, Op::InstanceOf.default_effects(), Some(expr.span));
        let call_block = ctx.builder.create_named_block("object.operator", vec![]);
        let next_check = if emitted == checks { scalar_block }
            else { ctx.builder.create_named_block("object.check", vec![]) };
        ctx.builder.terminate(Terminator::CondBr { cond: instance.value,
            then_target: call_block, then_args: vec![], else_target: next_check, else_args: vec![] });
        ctx.builder.position_at_end(call_block);
        let (object_lhs, object_op) = if negate && plan.class.operators.is_some_and(|operators|
            operators.arithmetic_result == elephc_builtin_contract::ObjectArithmeticResult::Number
        ) { (lower_int_literal(ctx, -1, expr), &BinOp::Mul) } else { (lhs, op) };
        let value = emit_object_binary_plan(ctx, plan, object_lhs, object_op, rhs, expr, consume);
        store_value_into_temp(ctx, &temp, result_type.clone(), value, expr.span);
        branch_to(ctx, merge);
        let snapshot = ctx.initialized_slots_snapshot();
        call_initialized = Some(match call_initialized {
            Some(previous) => previous.intersection(&snapshot).copied().collect(),
            None => snapshot,
        });
        ctx.builder.position_at_end(next_check);
    }
    ctx.restore_initialized_slots(initialized.clone());
    let scalar_pins = pin_in_flight_owners(ctx, &[lhs.value, rhs.value], expr.span);
    let value = scalar(ctx);
    unpin_in_flight_owners(ctx, scalar_pins, expr.span);
    store_value_into_temp(ctx, &temp, result_type, value, expr.span);
    branch_to(ctx, merge);
    let scalar_initialized = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(merge);
    let call_initialized = call_initialized.unwrap_or_else(|| initialized.clone());
    ctx.restore_initialized_slots(merge_initialized_slots_for_expr(&initialized,
        call_initialized, true, scalar_initialized, true));
    take_owned_temp(ctx, &temp, expr.span)
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
    let plan = crate::types::object_operators::binary_plan_with_classes(op, &lt, &rt, ctx.classes)?;
    if (matches!(lt, PhpType::Union(_)) || matches!(rt, PhpType::Union(_)))
        && !(plan.class.operators.is_some_and(|operators| operators.accepts_any_operands)
            && [&lt, &rt].iter().any(|operand| crate::types::object_operators::operator_class(operand)
                .is_some_and(|class| class.name.eq_ignore_ascii_case(plan.class.name))))
    { return None; }
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
pub(crate) fn append_hidden_arguments(
    ctx: &mut LoweringContext<'_, '_>, signature: Option<&FunctionSig>, operands: &mut Vec<ValueId>,
    actual_count: usize, expr: &Expr,
) -> Vec<LoweredValue> {
    let Some(signature) = signature else { return vec![] };
    if crate::func_args::sig_has_hidden_argc_param(signature) {
        operands.push(lower_int_literal(ctx, actual_count as i64, expr).value);
    }
    if !crate::func_args::sig_collects_surplus_args(signature) { return vec![]; }
    let tail = lower_variadic_tail_array(ctx, signature, &[], actual_count);
    operands.push(tail.value);
    vec![tail]
}

/// Lowers `~$object` for an internal class as `-x - 1`, PHP's `mpz_com` identity.
pub(super) fn lower_object_bitnot(
    ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, expr: &Expr,
) -> Option<LoweredValue> {
    let ty = ctx.builder.value_php_type(value.value);
    if let Some(class) = crate::types::object_operators::operator_class(&ty)
        .or_else(|| crate::types::object_operators::cast_class_with_classes(&ty, ctx.classes)) {
        let operators = class.operators?;
        if let Some(method) = operators.bit_not {
            return Some(emit_guarded_adapter(ctx, value, class.name, method, PhpType::Mixed, Some(expr.span)));
        }
        if operators.operand_dispatch == elephc_builtin_contract::ObjectOperandDispatch::NumericConversion {
            return None;
        }
    } else {
        if !can_contain_object(&ty) { return None; }
        let mut handlers = ctx.classes.keys().filter_map(|name| {
            let class = elephc_builtin_contract::lookup_class(name)?;
            Some((class.name, class.operators?.bit_not?))
        }).collect::<Vec<_>>();
        handlers.sort_unstable(); handlers.dedup();
        if handlers.is_empty() { return None; }
        return Some(lower_dynamic_numeric_cast(ctx, value, PhpType::Mixed, Some(expr.span), &handlers, true));
    }
    let zero = lower_int_literal(ctx, 0, expr);
    let negative = lower_object_binary_values(ctx, zero, &BinOp::Sub, value, expr)?;
    let one = lower_int_literal(ctx, 1, expr);
    lower_object_binary_values(ctx, negative, &BinOp::Sub, one, expr)
}

/// Calls an internal class's scalar cast adapter for an explicit `(int)`/`(float)` cast.
pub(super) fn lower_object_cast(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    target: &crate::parser::ast::CastType,
    expr: &Expr,
) -> Option<LoweredValue> {
    use crate::parser::ast::CastType;
    let cast = match target {
        CastType::Int => crate::types::object_operators::ObjectCast::Int,
        CastType::Float => crate::types::object_operators::ObjectCast::Float,
        _ => return None,
    };
    let result_type = match target {
        CastType::Int => PhpType::Int,
        CastType::Float => PhpType::Float,
        _ => unreachable!("object cast target checked above"),
    };
    let ty = ctx.builder.value_php_type(value.value);
    if let Some(class) = crate::types::object_operators::cast_class_with_classes(&ty, ctx.classes) {
        crate::types::object_operators::cast_method(class, cast)?;
        return Some(emit_cast_method(ctx, class, cast, value, expr));
    }
    if !matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        return None;
    }
    let mut class_names: Vec<String> = ctx.classes.keys().cloned().collect();
    class_names.sort();
    let classes: Vec<_> = class_names
        .iter()
        .filter_map(|name| elephc_builtin_contract::lookup_class(name))
        .filter(|class| crate::types::object_operators::cast_method(class, cast).is_some())
        .collect();
    if classes.is_empty() {
        return None;
    }
    let temp = ctx.declare_owned_hidden_temp(result_type.clone());
    let initialized = ctx.initialized_slots_snapshot();
    let scalar_block = ctx.builder.create_named_block("cast.scalar", vec![]);
    let merge = ctx.builder.create_named_block("cast.merge", vec![]);
    let mut call_initialized: Option<HashSet<LocalSlotId>> = None;
    for (index, class) in classes.iter().enumerate() {
        let class_name = ctx.intern_class_name(class.name);
        let instance = ctx.emit_value(
            Op::InstanceOf,
            vec![value.value],
            Some(Immediate::Data(class_name)),
            PhpType::Bool,
            Op::InstanceOf.default_effects(),
            Some(expr.span),
        );
        let call_block = ctx.builder.create_named_block("cast.object", vec![]);
        let next_check = if index + 1 == classes.len() {
            scalar_block
        } else {
            ctx.builder.create_named_block("cast.check", vec![])
        };
        ctx.builder.terminate(Terminator::CondBr {
            cond: instance.value,
            then_target: call_block,
            then_args: vec![],
            else_target: next_check,
            else_args: vec![],
        });
        ctx.builder.position_at_end(call_block);
        let result = emit_cast_method(ctx, class, cast, value, expr);
        store_value_into_temp(ctx, &temp, result_type.clone(), result, expr.span);
        branch_to(ctx, merge);
        let snapshot = ctx.initialized_slots_snapshot();
        call_initialized = Some(match call_initialized {
            Some(previous) => previous.intersection(&snapshot).copied().collect(),
            None => snapshot,
        });
        ctx.builder.position_at_end(next_check);
    }
    ctx.restore_initialized_slots(initialized.clone());
    let php_type = cast_php_type(target, &ty);
    let result = ctx.emit_value(
        Op::Cast,
        vec![value.value],
        Some(Immediate::CastTarget(value_ir_type(&php_type))),
        php_type,
        Op::Cast.default_effects(),
        Some(expr.span),
    );
    store_value_into_temp(ctx, &temp, result_type, result, expr.span);
    branch_to(ctx, merge);
    let scalar_initialized = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(merge);
    let call_initialized = call_initialized.unwrap_or_else(|| initialized.clone());
    ctx.restore_initialized_slots(merge_initialized_slots_for_expr(
        &initialized,
        call_initialized,
        true,
        scalar_initialized,
        true,
    ));
    Some(take_owned_temp(ctx, &temp, expr.span))
}

/// Emits one class's cast adapter call for an already selected class.
fn emit_cast_method(
    ctx: &mut LoweringContext<'_, '_>,
    class: &'static elephc_builtin_contract::ClassContract,
    cast: crate::types::object_operators::ObjectCast,
    value: LoweredValue,
    expr: &Expr,
) -> LoweredValue {
    let method = crate::types::object_operators::cast_method(class, cast)
        .expect("cast adapter was selected by cast_method");
    let result_type = match cast {
        crate::types::object_operators::ObjectCast::Int => PhpType::Int,
        crate::types::object_operators::ObjectCast::Float => PhpType::Float,
        _ => unreachable!("explicit cast only selects int/float"),
    };
    emit_guarded_adapter(ctx, value, class.name, method, result_type, Some(expr.span))
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
    if !can_contain_object(&ty) { return None; }
    let mut handlers = ctx.classes.keys().filter_map(|name| {
        let class = elephc_builtin_contract::lookup_class(name)?;
        Some((class.name, class.operators?.boolean?))
    }).collect::<Vec<_>>();
    handlers.sort_unstable();
    handlers.dedup();
    if handlers.is_empty() { return None; }
    Some(lower_dynamic_truthiness(ctx, input, span, &handlers))
}

/// Branches through available boolean handlers without requiring a comparison operator.
fn lower_dynamic_truthiness(
    ctx: &mut LoweringContext<'_, '_>, input: LoweredValue, span: Option<Span>,
    handlers: &[(&str, &str)],
) -> LoweredValue {
    let Some((class, method)) = handlers.first() else {
        return ctx.emit_value(Op::IsTruthy, vec![input.value], None,
            PhpType::Bool, Op::IsTruthy.default_effects(), span);
    };
    let at = span.unwrap_or_else(Span::dummy);
    let data = ctx.intern_class_name(class);
    let condition = ctx.emit_value(Op::InstanceOf, vec![input.value], Some(Immediate::Data(data)),
        PhpType::Bool, Op::InstanceOf.default_effects(), span);
    let temp = ctx.declare_owned_hidden_temp(PhpType::Bool);
    let initialized = ctx.initialized_slots_snapshot();
    let object = ctx.builder.create_named_block("object.truthy", vec![]);
    let fallback = ctx.builder.create_named_block("object.truthy.fallback", vec![]);
    let merge = ctx.builder.create_named_block("object.truthy.merge", vec![]);
    ctx.builder.terminate(Terminator::CondBr { cond: condition.value,
        then_target: object, then_args: vec![], else_target: fallback, else_args: vec![] });
    ctx.builder.position_at_end(object);
    let value = emit_guarded_adapter(ctx, input, class, method, PhpType::Bool, span);
    store_value_into_temp(ctx, &temp, PhpType::Bool, value, at);
    branch_to(ctx, merge);
    let object_initialized = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(fallback);
    ctx.restore_initialized_slots(initialized.clone());
    let value = lower_dynamic_truthiness(ctx, input, span, &handlers[1..]);
    store_value_into_temp(ctx, &temp, PhpType::Bool, value, at);
    branch_to(ctx, merge);
    let fallback_initialized = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(merge);
    ctx.restore_initialized_slots(merge_initialized_slots_for_expr(&initialized,
        object_initialized, true, fallback_initialized, true));
    take_owned_temp(ctx, &temp, at)
}

/// Applies a PHP internal-class numeric cast through target-independent method calls.
pub(super) fn lower_object_numeric_cast(
    ctx: &mut LoweringContext<'_, '_>, input: LoweredValue,
    target: &CastType, span: Option<Span>,
) -> Option<LoweredValue> {
    let cast = match target {
        CastType::Int => elephc_builtin_contract::ObjectCast::Int,
        CastType::Float => elephc_builtin_contract::ObjectCast::Float,
        _ => return None,
    };
    let ty = ctx.builder.value_php_type(input.value);
    if !can_contain_object(&ty) { return None; }
    let result_type = if matches!(target, CastType::Int) { PhpType::Int } else { PhpType::Float };
    let mut handlers = ctx.classes.keys().filter_map(|name| {
        let class = elephc_builtin_contract::lookup_class(name)?;
        Some((class.name, class.operators?.cast_method(cast)?))
    }).collect::<Vec<_>>();
    handlers.sort_unstable(); handlers.dedup();
    if handlers.is_empty() { return None; }
    Some(lower_dynamic_numeric_cast(ctx, input, result_type, span, &handlers, false))
}

/// Selects a native unary adapter while preserving the operation's existing scalar fallback.
fn lower_dynamic_numeric_cast(
    ctx: &mut LoweringContext<'_, '_>, input: LoweredValue, result_type: PhpType,
    span: Option<Span>, handlers: &[(&str, &str)], bit_not: bool,
) -> LoweredValue {
    let Some((class, method)) = handlers.first() else {
        if bit_not {
            let at = span.unwrap_or_else(Span::dummy);
            let operand = box_operand(ctx, input, at);
            let pins = pin_in_flight_owners(ctx, &[input.value, operand.value], at);
            let result = ctx.emit_owned_value(Op::RuntimeCall, vec![operand.value],
                Some(Immediate::RuntimeCall(RuntimeCallTarget::MixedBitNot)), PhpType::Mixed,
                Effects::all(), span);
            unpin_in_flight_owners(ctx, pins, at);
            if operand.value != input.value { release_binary_operand_temporary(ctx, operand, at); }
            release_binary_operand_temporary(ctx, input, at);
            return result;
        }
        let at = span.unwrap_or_else(Span::dummy);
        let operand = if matches!(ctx.builder.value_php_type(input.value), PhpType::Object(_)) {
            box_operand(ctx, input, at)
        } else { input };
        let result = ctx.emit_value(Op::Cast, vec![operand.value],
            Some(Immediate::CastTarget(value_ir_type(&result_type))), result_type,
            Op::Cast.default_effects(), span);
        if operand.value != input.value { release_binary_operand_temporary(ctx, operand, at); }
        return result;
    };
    let at = span.unwrap_or_else(Span::dummy);
    let data = ctx.intern_class_name(class);
    let condition = ctx.emit_value(Op::InstanceOf, vec![input.value], Some(Immediate::Data(data)),
        PhpType::Bool, Op::InstanceOf.default_effects(), span);
    let temp = ctx.declare_owned_hidden_temp(result_type.clone());
    let initialized = ctx.initialized_slots_snapshot();
    let object = ctx.builder.create_named_block("object.cast", vec![]);
    let fallback = ctx.builder.create_named_block("object.cast.fallback", vec![]);
    let merge = ctx.builder.create_named_block("object.cast.merge", vec![]);
    ctx.builder.terminate(Terminator::CondBr { cond: condition.value,
        then_target: object, then_args: vec![], else_target: fallback, else_args: vec![] });
    ctx.builder.position_at_end(object);
    let value = emit_guarded_adapter(ctx, input, class, method, result_type.clone(), span);
    store_value_into_temp(ctx, &temp, result_type.clone(), value, at);
    branch_to(ctx, merge);
    let object_initialized = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(fallback);
    ctx.restore_initialized_slots(initialized.clone());
    let value = lower_dynamic_numeric_cast(ctx, input, result_type.clone(), span, &handlers[1..], bit_not);
    store_value_into_temp(ctx, &temp, result_type, value, at);
    branch_to(ctx, merge);
    let fallback_initialized = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(merge);
    ctx.restore_initialized_slots(merge_initialized_slots_for_expr(&initialized,
        object_initialized, true, fallback_initialized, true));
    take_owned_temp(ctx, &temp, at)
}

/// Narrows a raw object receiver after an instanceof guard and preserves hidden call arguments.
fn emit_guarded_adapter(
    ctx: &mut LoweringContext<'_, '_>, input: LoweredValue, class: &str, method: &str,
    result_type: PhpType, span: Option<Span>,
) -> LoweredValue {
    let receiver = if matches!(ctx.builder.value_php_type(input.value), PhpType::Object(_)) {
        let value = ctx.emit_value(Op::Borrow, vec![input.value], None,
            PhpType::Object(class.to_string()), Op::Borrow.default_effects(), span);
        ctx.builder.set_value_ownership(value.value, Ownership::Borrowed);
        value
    } else { input };
    let signature = ctx.classes.get(class).and_then(|info| info.methods.get(method)).cloned();
    let expr = Expr::new(ExprKind::Null, span.unwrap_or_else(Span::dummy));
    let mut operands = vec![receiver.value];
    let extra = append_hidden_arguments(ctx, signature.as_ref(), &mut operands, 0, &expr);
    let pins = pin_in_flight_owners(ctx, &operands, expr.span);
    let data = ctx.intern_string(method);
    let result = ctx.emit_value(Op::MethodCall, operands, Some(Immediate::Data(data)),
        result_type, Op::MethodCall.default_effects(), span);
    unpin_in_flight_owners(ctx, pins, expr.span);
    for value in extra { release_binary_operand_temporary(ctx, value, expr.span); }
    result
}
