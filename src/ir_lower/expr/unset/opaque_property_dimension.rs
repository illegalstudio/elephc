//! Purpose:
//! Removes native XML dimensions through opaque property receivers without array value copies.
//!
//! Called from:
//! - Direct unset lowering after known property and static storage paths.
//!
//! Key details:
//! - Computed roots and name values run before the key; writable traversal follows it.
//! - False/null bases remain quiet, and unsupported ordinary storage raises an explicit Error.

use super::*;

/// Recognizes property paths whose erased base may carry native XML property semantics.
pub(super) fn supports(ctx: &LoweringContext<'_, '_>, array: &Expr) -> bool {
    let object = match &array.kind {
        ExprKind::PropertyAccess { object, property } if !property.starts_with("__elephc_") => object,
        ExprKind::DynamicPropertyAccess { object, .. } => object,
        _ => return false,
    };
    let ty = array_cast_source::array_cast_source_type_for_ir(ctx, object);
    ctx.classes.contains_key("SimpleXMLElement")
        && (matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
            || matches!(ty.codegen_repr(), PhpType::Object(class)
                if class.is_empty() || ctx.interfaces.contains_key(&class))
            || singular_object_class(&ty).is_some_and(|(class, _)|
                crate::types::builtin_classes::has_native_dimensions(class, ctx.classes)))
}

/// Keeps the native object branch independent of unsupported ordinary array storage places.
pub(super) fn lower(
    ctx: &mut LoweringContext<'_, '_>, array: &Expr, index: &Expr, expr: &Expr,
) {
    let (object, property) = match &array.kind {
        ExprKind::PropertyAccess { object, property } =>
            (object.as_ref(), Expr::new(ExprKind::StringLiteral(property.clone()), array.span)),
        ExprKind::DynamicPropertyAccess { object, property } =>
            (object.as_ref(), property.as_ref().clone()),
        _ => unreachable!("opaque property unset classification"),
    };
    let mut owners = Vec::new();
    let object = prepare_receiver(ctx, object, &mut owners);
    let property = park_unset_operand(ctx, &property, &mut owners);
    // A variable key must also be snapshotted before name conversion can replace its cell.
    let key = lower_expr(ctx, index);
    let key = park_lowered_unset_operand(ctx, key, index.span, &mut owners);
    let object = finish_receiver(ctx, &object);
    let object = park_lowered_unset_operand(ctx, object, array.span, &mut owners);
    let object = lower_expr(ctx, &object);
    let class = ctx.intern_class_name("SimpleXMLElement");
    let native = ctx.emit_value(Op::InstanceOf, vec![object.value], Some(Immediate::Data(class)),
        PhpType::Bool, Op::InstanceOf.default_effects(), Some(expr.span));
    let yes = ctx.builder.create_named_block("unset.opaque.xml", Vec::new());
    let no = ctx.builder.create_named_block("unset.opaque.other", Vec::new());
    let merge = ctx.builder.create_named_block("unset.opaque.merge", Vec::new());
    let initial = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr { cond: native.value,
        then_target: yes, then_args: Vec::new(), else_target: no, else_args: Vec::new() });
    ctx.builder.position_at_end(yes);
    let receiver = borrow_native_xml_property_receiver(ctx, object, expr.span);
    remove_property_value(ctx, receiver, &property, &key, PropertyFetchMode::QuietRead,
        array, expr, &mut owners);
    jump(ctx, merge);
    let native_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(no);
    ctx.restore_initialized_slots(initial);
    let is_object = ctx.emit_value(Op::TypePredicate, vec![object.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Object)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(expr.span));
    let ordinary = ctx.builder.create_named_block("unset.opaque.ordinary", Vec::new());
    ctx.builder.terminate(Terminator::CondBr { cond: is_object.value,
        then_target: ordinary, then_args: Vec::new(), else_target: merge, else_args: Vec::new() });
    let other_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(ordinary);
    remove_property_value(ctx, object, &property, &key, PropertyFetchMode::QuietRead,
        array, expr, &mut owners);
    jump(ctx, merge);
    let ordinary_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(merge);
    let other_slots = other_slots.intersection(&ordinary_slots).copied().collect();
    ctx.restore_initialized_slots(native_slots.intersection(&other_slots).copied().collect());
    for owner in owners.into_iter().rev() { retire_owned_call_operand(ctx, owner, expr.span); }
}

/// Removes through object handles, leaving unsupported array storage under an explicit Error.
fn remove_property_value(
    ctx: &mut LoweringContext<'_, '_>, receiver: LoweredValue, property: &Expr, key: &Expr,
    mode: PropertyFetchMode, array: &Expr, expr: &Expr, owners: &mut Vec<crate::ir::LocalSlotId>,
) {
    let inner_owners = owners.len();
    let name = lower_expr(ctx, property);
    let name = property_access::coerce_runtime_property_name(ctx, name, array.span);
    let name = park_lowered_unset_operand(ctx, name, array.span, owners);
    let name_value = lower_expr(ctx, &name);
    debug_assert!(matches!(mode, PropertyFetchMode::QuietRead));
    let selected = fetch_unset_property(ctx, receiver, name_value, array);
    let ty = ctx.builder.value_php_type(selected.value);
    let selected = if matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) { selected }
        else { ctx.box_value_as_mixed(selected, PhpType::Mixed, Some(expr.span)) };
    let selected = park_lowered_unset_operand(ctx, selected, array.span, owners);
    let selected = lower_expr(ctx, &selected);
    let is_array = ctx.emit_value(Op::TypePredicate, vec![selected.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Array)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(expr.span));
    let unsupported = ctx.builder.create_named_block("unset.opaque.array", Vec::new());
    let other = ctx.builder.create_named_block("unset.opaque.value", Vec::new());
    let done = ctx.builder.create_named_block("unset.opaque.value.done", Vec::new());
    let initial = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr { cond: is_array.value,
        then_target: unsupported, then_args: Vec::new(), else_target: other, else_args: Vec::new() });
    ctx.builder.position_at_end(unsupported);
    crate::ir_lower::stmt::lower_throw_access_error(ctx,
        "Unsupported unset(): opaque array property storage has no attached removal path", expr.span);
    jump(ctx, done);
    ctx.builder.position_at_end(other);
    ctx.restore_initialized_slots(initial.clone());
    // Native adapters and eval-aware public dispatch preserve object identity without array copies.
    let is_object = ctx.emit_value(Op::TypePredicate, vec![selected.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Object)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(expr.span));
    let object = ctx.builder.create_named_block("unset.opaque.object.value", Vec::new());
    let scalar = ctx.builder.create_named_block("unset.opaque.scalar.value", Vec::new());
    ctx.builder.terminate(Terminator::CondBr { cond: is_object.value,
        then_target: object, then_args: Vec::new(), else_target: scalar, else_args: Vec::new() });
    ctx.builder.position_at_end(object);
    native_dimension_probes::lower(ctx, selected, &PhpType::Mixed, key,
        elephc_builtin_contract::ObjectDimension::Unset, expr);
    jump(ctx, done);
    let object_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(scalar);
    ctx.restore_initialized_slots(initial.clone());
    mixed_dimension::lower_scalar(ctx, selected, expr.span);
    jump(ctx, done);
    let scalar_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(done);
    ctx.restore_initialized_slots(merge_initialized_slots_for_expr(&initial, object_slots, true, scalar_slots, true));
    for owner in owners.split_off(inner_owners).into_iter().rev() { retire_owned_call_operand(ctx, owner, expr.span); }
}

/// Captures computed roots and names now while postponing property traversal until after the key.
pub(super) fn prepare_receiver(
    ctx: &mut LoweringContext<'_, '_>, source: &Expr, owners: &mut Vec<crate::ir::LocalSlotId>,
) -> Expr {
    let kind = match &source.kind {
        ExprKind::StaticPropertyAccess { .. } => return source.clone(),
        ExprKind::PropertyAccess { object, property } => ExprKind::PropertyAccess {
            object: Box::new(prepare_receiver(ctx, object, owners)), property: property.clone(),
        },
        ExprKind::DynamicPropertyAccess { object, property } => ExprKind::DynamicPropertyAccess {
            object: Box::new(prepare_receiver(ctx, object, owners)),
            property: Box::new(park_unset_operand(ctx, property, owners)),
        },
        ExprKind::ArrayAccess { array, index } => ExprKind::ArrayAccess {
            array: Box::new(prepare_receiver(ctx, array, owners)),
            index: Box::new(park_unset_operand(ctx, index, owners)),
        },
        _ => return park_unset_operand(ctx, source, owners),
    };
    Expr::new(kind, source.span)
}

/// Resolves unset receiver properties with materialization but dimensions without creation.
pub(super) fn finish_receiver(ctx: &mut LoweringContext<'_, '_>, source: &Expr) -> LoweredValue {
    finish_receiver_in_context(ctx, source, true)
}

/// Resolves a receiver whose selected value will feed a dimension rather than a property.
pub(super) fn finish_dimension_receiver(ctx: &mut LoweringContext<'_, '_>, source: &Expr) -> LoweredValue {
    finish_receiver_in_context(ctx, source, false)
}

/// Threads the next fetch kind through recursion so string offsets use PHP's contextual Error.
fn finish_receiver_in_context(ctx: &mut LoweringContext<'_, '_>, source: &Expr, as_object: bool) -> LoweredValue {
    match &source.kind {
        ExprKind::PropertyAccess { object, property } => {
            let parent = finish_receiver_in_context(ctx, object, true);
            let name = lower_expr(ctx, &Expr::new(ExprKind::StringLiteral(property.clone()), source.span));
            fetch_unset_property(ctx, parent, name, source)
        }
        ExprKind::DynamicPropertyAccess { object, property } => {
            let parent = finish_receiver_in_context(ctx, object, true);
            select_dynamic_property(ctx, parent, property, source)
        }
        ExprKind::ArrayAccess { array, index } => {
            let parent = finish_receiver_in_context(ctx, array, false);
            let ty = ctx.builder.value_php_type(parent.value);
            if !property_access::may_have_native_xml_dimensions(ctx, &ty) {
                return quiet_index(ctx, parent, index, source, as_object);
            }
            property_access::native_xml_reads::dispatch(ctx, parent, source, PhpType::Mixed, false,
                |ctx, parent| lower_method_call_with_receiver(ctx, parent, "__elephc_dimension_select", &[
                    index.as_ref().clone(), Expr::new(ExprKind::BoolLiteral(false), source.span),
                    Expr::new(ExprKind::BoolLiteral(false), source.span),
                ], Op::MethodCall, source),
                |ctx, parent| quiet_index(ctx, parent, index, source, as_object),
            )
        }
        _ => lower_expr(ctx, source),
    }
}

/// Keeps ordinary arrays and ArrayAccess receivers on their existing quiet read path.
fn quiet_index(
    ctx: &mut LoweringContext<'_, '_>, parent: LoweredValue, index: &Expr, source: &Expr, as_object: bool,
) -> LoweredValue {
    let pins = super::super::pin_in_flight_owners(ctx, &[parent.value], source.span);
    guard_dimension_parent(ctx, parent, source.span, as_object);
    super::super::unpin_in_flight_owners(ctx, pins, source.span);
    let value = if value_is_nullable(ctx, parent.value) {
        lower_nullable_array_access(ctx, parent, index, source, false)
    } else { lower_array_access_from_value(ctx, parent, index, source, false) };
    // Typed object misses carry a null word; boxing normalizes it before later native dispatch.
    if matches!(ctx.builder.value_php_type(value.value).codegen_repr(), PhpType::Object(_)) {
        ctx.box_value_as_mixed(value, PhpType::Mixed, Some(source.span))
    } else { value }
}

/// Distinguishes quiet missing containers from scalar errors before an intermediate offset read.
fn guard_dimension_parent(ctx: &mut LoweringContext<'_, '_>, parent: LoweredValue, span: Span, as_object: bool) {
    let closure = is_unset_closure(ctx, parent, span);
    let closure_error = ctx.builder.create_named_block("unset.dimension.closure", Vec::new());
    let ordinary = ctx.builder.create_named_block("unset.dimension.ordinary", Vec::new());
    let closure_initial = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr { cond: closure.value,
        then_target: closure_error, then_args: Vec::new(), else_target: ordinary, else_args: Vec::new() });
    ctx.builder.position_at_end(closure_error);
    crate::ir_lower::stmt::lower_throw_access_error(ctx, "Cannot use object of type Closure as array", span);
    ctx.builder.position_at_end(ordinary);
    ctx.restore_initialized_slots(closure_initial);
    let mut predicates = Vec::new();
    for predicate in [crate::ir::PhpTypePredicate::Array, crate::ir::PhpTypePredicate::Object] {
        predicates.push(ctx.emit_value(Op::TypePredicate, vec![parent.value],
            Some(Immediate::TypePredicate(predicate)), PhpType::Bool,
            Op::TypePredicate.default_effects(), Some(span)));
    }
    let container = ctx.emit_value(Op::IBitOr, vec![predicates[0].value, predicates[1].value], None,
        PhpType::Bool, Op::IBitOr.default_effects(), Some(span));
    let scalar = ctx.builder.create_named_block("unset.dimension.scalar", Vec::new());
    let done = ctx.builder.create_named_block("unset.dimension.checked", Vec::new());
    let initial = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr { cond: container.value,
        then_target: done, then_args: Vec::new(), else_target: scalar, else_args: Vec::new() });
    ctx.builder.position_at_end(scalar);
    mixed_dimension::lower_intermediate_scalar(ctx, parent, span, as_object);
    jump(ctx, done);
    let scalar_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(done);
    ctx.restore_initialized_slots(merge_initialized_slots_for_expr(&initial,
        initial.clone(), true, scalar_slots, true));
}

/// Skips name conversion after a missing unset dimension, protecting the selected owner on unwind.
fn select_dynamic_property(
    ctx: &mut LoweringContext<'_, '_>, parent: LoweredValue, property: &Expr, source: &Expr,
) -> LoweredValue {
    let staging = prepublish_call_result(ctx, &PhpType::Mixed, source.span);
    let (parent, owner) = root_owned_call_operand(ctx, parent, source.span);
    let temp = ctx.declare_owned_hidden_temp(PhpType::Mixed);
    let present = ctx.builder.create_named_block("unset.receiver.property", Vec::new());
    let absent = ctx.builder.create_named_block("unset.receiver.absent", Vec::new());
    let merge = ctx.builder.create_named_block("unset.receiver.merge", Vec::new());
    let object = ctx.emit_value(Op::TypePredicate, vec![parent.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Object)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(source.span));
    let initial = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr { cond: object.value,
        then_target: present, then_args: Vec::new(), else_target: absent, else_args: Vec::new() });
    ctx.builder.position_at_end(present);
    let name = lower_expr(ctx, property);
    let name = property_access::coerce_runtime_property_name(ctx, name, source.span);
    let value = fetch_unset_property(ctx, parent, name, source);
    store_value_into_temp(ctx, &temp, PhpType::Mixed, value, source.span);
    jump(ctx, merge);
    let present_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(absent);
    ctx.restore_initialized_slots(initial.clone());
    let value = lower_null(ctx, source);
    let value = ctx.box_value_as_mixed(value, PhpType::Mixed, Some(source.span));
    store_value_into_temp(ctx, &temp, PhpType::Mixed, value, source.span);
    jump(ctx, merge);
    let absent_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(merge);
    ctx.restore_initialized_slots(merge_initialized_slots_for_expr(&initial, present_slots, true, absent_slots, true));
    let result = take_owned_temp(ctx, &temp, source.span);
    stage_call_result(ctx, staging.as_ref(), result, source.span);
    if let Some(owner) = owner { retire_owned_call_operand(ctx, owner, source.span); }
    take_prepublished_call_result(ctx, staging, result, source.span)
}

/// Protects a converted property name and receiver while validating Closure materialization.
fn fetch_unset_property(
    ctx: &mut LoweringContext<'_, '_>, parent: LoweredValue, name: LoweredValue, source: &Expr,
) -> LoweredValue {
    let staging = prepublish_call_result(ctx, &PhpType::Mixed, source.span);
    let (parent, parent_owner) = root_owned_call_operand(ctx, parent, source.span);
    let parent_type = ctx.builder.value_php_type(parent.value);
    let borrowed = if Ownership::php_type_needs_lifetime_tracking(&parent_type) {
        let borrowed = ctx.emit_value(Op::Borrow, vec![parent.value], None, parent_type,
            Op::Borrow.default_effects(), Some(source.span));
        ctx.builder.set_value_ownership(borrowed.value, Ownership::Borrowed);
        borrowed
    } else { parent };
    let mut names = Vec::new();
    let name = park_lowered_unset_operand(ctx, name, source.span, &mut names);
    reject_unset_closure_property(ctx, borrowed, &name, source.span);
    let key = lower_expr(ctx, &name);
    let value = property_access::lower_dynamic_property_fetch_or_select(ctx, borrowed, key,
        PhpType::Mixed, PropertyFetchMode::QuietRead, false, true, source);
    let value = if ctx.builder.value_php_type(value.value).codegen_repr() == PhpType::Mixed { value }
        else { ctx.box_value_as_mixed(value, PhpType::Mixed, Some(source.span)) };
    // A later throwing hop must see an independent producer, not an already-cleared temp load.
    let result = crate::ir_lower::ownership::acquire_lifetime_pin_if_refcounted(ctx, value, Some(source.span));
    crate::ir_lower::ownership::release_if_owned(ctx, value, Some(source.span));
    stage_call_result(ctx, staging.as_ref(), result, source.span);
    for owner in names.into_iter().rev() { retire_owned_call_operand(ctx, owner, source.span); }
    if let Some(owner) = parent_owner { retire_owned_call_operand(ctx, owner, source.span); }
    take_prepublished_call_result(ctx, staging, result, source.span)
}

/// Joins an arm only when its preceding operation still has a normal continuation.
fn jump(ctx: &mut LoweringContext<'_, '_>, target: BlockId) {
    if !ctx.builder.insertion_block_is_terminated() {
        ctx.builder.terminate(Terminator::Br { target, args: Vec::new() });
    }
}
