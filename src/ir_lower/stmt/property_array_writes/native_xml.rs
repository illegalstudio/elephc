//! Purpose:
//! Selects native XML property write-fetch after dimension keys and values.
//!
//! Called from:
//! - Instance/static property array append and indexed assignment lowering.
//!
//! Key details:
//! - Opaque receivers return a null sentinel on the ordinary-object branch.
//! - Native updates transfer the parent lease to the selected child before mutation.
//! - Ordinary updates keep their parent alive through writeback.

use super::*;

/// One owned XML child, optionally guarded by a runtime null sentinel.
pub(super) struct Fetch {
    value: LoweredValue,
    conditional: bool,
}

impl Fetch {
    /// Uses an already selected concrete native value without a runtime sentinel.
    pub(super) fn known(value: LoweredValue) -> Self { Self { value, conditional: false } }
}

/// Captures a native write-fetch or a null sentinel after every dimension operand runs.
pub(super) fn fetch(
    ctx: &mut LoweringContext<'_, '_>, receiver: &property_write_receiver::PropertyWriteReceiver,
    expression: &Expr, property: &str, span: Span,
) -> Option<Fetch> {
    if property.starts_with("__elephc_") || !ctx.classes.contains_key("SimpleXMLElement") { return None; }
    let probe = receiver.temporary_may_destroy();
    let receiver = receiver.value;
    let ty = ctx.builder.value_php_type(receiver.value);
    let known = crate::ir_lower::expr::singular_object_class(&ty).is_some_and(|(class, _)|
        crate::types::builtin_classes::native_property_method(class, property,
            elephc_builtin_contract::ObjectProperty::WriteFetch, ctx.classes).is_some());
    let opaque = matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
        || matches!(ty.codegen_repr(), PhpType::Object(class)
            if class.is_empty() || ctx.interfaces.contains_key(&class));
    if !known && !opaque {
        if !probe { return None; }
        let child = ctx.with_independent_write_receiver(|ctx|
            native_property_children::fetch(ctx, receiver, expression, property, span))?;
        return Some(Fetch { value: child, conditional: true });
    }
    if known {
        let child = call_fetch(ctx, receiver, property, span);
        return Some(Fetch { value: child, conditional: false });
    }
    let class = ctx.intern_class_name("SimpleXMLElement");
    let condition = ctx.emit_value(Op::InstanceOf, vec![receiver.value], Some(Immediate::Data(class)),
        PhpType::Bool, Op::InstanceOf.default_effects(), Some(span));
    let native = ctx.builder.create_named_block("xml.write_fetch.native", vec![]);
    let ordinary = ctx.builder.create_named_block("xml.write_fetch.ordinary", vec![]);
    let merge = ctx.builder.create_named_block("xml.write_fetch.merge", vec![]);
    let temp = ctx.declare_owned_hidden_temp(PhpType::Mixed);
    let initial = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr { cond: condition.value,
        then_target: native, then_args: vec![], else_target: ordinary, else_args: vec![] });
    ctx.builder.position_at_end(native);
    let child = call_fetch(ctx, receiver, property, span);
    crate::ir_lower::expr::store_value_into_temp(ctx, &temp, PhpType::Mixed, child, span);
    jump(ctx, merge);
    let native_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(ordinary);
    ctx.restore_initialized_slots(initial);
    let child = if probe { ctx.with_independent_write_receiver(|ctx|
        native_property_children::fetch(ctx, receiver, expression, property, span)) } else { None };
    let child = child.unwrap_or_else(|| lower_expr(ctx, &Expr::new(ExprKind::Null, span)));
    crate::ir_lower::expr::store_value_into_temp(ctx, &temp, PhpType::Mixed, child, span);
    jump(ctx, merge);
    let ordinary_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(merge);
    ctx.restore_initialized_slots(native_slots.intersection(&ordinary_slots).copied().collect());
    let child = crate::ir_lower::expr::take_owned_temp(ctx, &temp, span);
    Some(Fetch { value: child, conditional: true })
}

/// Invokes the final native adapter with a borrowed or freshly boxed receiver.
fn call_fetch(ctx: &mut LoweringContext<'_, '_>, receiver: LoweredValue, property: &str, span: Span) -> LoweredValue {
    let ty = ctx.builder.value_php_type(receiver.value);
    let object = ctx.emit_value(Op::Borrow, vec![receiver.value], None, ty.clone(),
        Op::Borrow.default_effects(), Some(span));
    ctx.builder.set_value_ownership(object.value, Ownership::Borrowed);
    let object = if matches!(ty.codegen_repr(), PhpType::Object(class)
        if class.is_empty() || ctx.interfaces.contains_key(&class))
    { ctx.box_value_as_mixed(object, PhpType::Mixed, Some(span)) } else { object };
    let expr = Expr::new(ExprKind::StringLiteral(property.into()), span);
    crate::ir_lower::expr::lower_native_xml_property_write_fetch(ctx, object, property, &expr)
}

/// Retires a native parent after publishing its child, or keeps an ordinary parent for writeback.
pub(super) fn dispatch(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: property_write_receiver::PropertyWriteReceiver, fetch: Fetch, span: Span,
    native_update: impl FnOnce(&mut LoweringContext<'_, '_>, LoweredValue),
    ordinary_update: impl FnOnce(&mut LoweringContext<'_, '_>, LoweredValue),
) {
    if !fetch.conditional {
        let child = handoff_child(ctx, receiver, fetch.value, span);
        let borrowed = borrow_child(ctx, child.value, span);
        native_update(ctx, borrowed);
        child.finish(ctx, span);
        return;
    }
    let absent = ctx.emit_value(Op::IsNull, vec![fetch.value.value], None, PhpType::Bool,
        Op::IsNull.default_effects(), Some(span));
    let native = ctx.builder.create_named_block("xml.dimension.update.native", vec![]);
    let ordinary = ctx.builder.create_named_block("xml.dimension.update.ordinary", vec![]);
    let merge = ctx.builder.create_named_block("xml.dimension.update.merge", vec![]);
    let initial = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr { cond: absent.value,
        then_target: ordinary, then_args: vec![], else_target: native, else_args: vec![] });
    ctx.builder.position_at_end(native);
    // These lowering records describe mutually exclusive paths, not additional runtime leases.
    let child = handoff_child(ctx, receiver.clone(), fetch.value, span);
    let borrowed = borrow_child(ctx, child.value, span);
    native_update(ctx, borrowed);
    child.finish(ctx, span);
    jump(ctx, merge);
    let native_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(ordinary);
    ctx.restore_initialized_slots(initial);
    crate::ir_lower::ownership::release_if_owned(ctx, fetch.value, Some(span));
    ordinary_update(ctx, receiver.value);
    receiver.finish(ctx, span);
    jump(ctx, merge);
    let ordinary_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(merge);
    ctx.restore_initialized_slots(native_slots.intersection(&ordinary_slots).copied().collect());
}

/// Acquires a declared property's borrowed child before its temporary owning parent can die.
fn handoff_child(
    ctx: &mut LoweringContext<'_, '_>, receiver: property_write_receiver::PropertyWriteReceiver,
    child: LoweredValue, span: Span,
) -> property_write_receiver::PropertyWriteReceiver {
    ctx.with_independent_write_receiver(|ctx| {
        let child = if ctx.value_is_owning_temporary(child) { child } else {
            crate::ir_lower::ownership::acquire_lifetime_pin_if_refcounted(ctx, child, Some(span))
        };
        receiver.handoff(ctx, child, span)
    })
}

/// Lends the native writer a non-consuming view of its independently held child.
fn borrow_child(ctx: &mut LoweringContext<'_, '_>, child: LoweredValue, span: Span) -> LoweredValue {
    let ty = ctx.builder.value_php_type(child.value);
    let value = ctx.emit_value(Op::Borrow, vec![child.value], None, ty,
        Op::Borrow.default_effects(), Some(span));
    ctx.builder.set_value_ownership(value.value, Ownership::Borrowed);
    value
}

/// Chooses native offsetSet or ordinary writeback while retiring the proper receiver in each arm.
pub(super) fn write(
    ctx: &mut LoweringContext<'_, '_>, receiver: property_write_receiver::PropertyWriteReceiver,
    fetch: Fetch, property: &str, index: Option<LoweredValue>, late_key: Option<&Expr>, value: LoweredValue,
    compound: Option<(&BinOp, &Expr)>, span: Span, diagnosed: bool,
) {
    dispatch(ctx, receiver, fetch, span,
        |ctx, child| {
            if let Some(index) = index {
                with_late_property_key(ctx, late_key, index, span,
                    |ctx, index| native_write(ctx, child, Some(index), value, compound, span, diagnosed));
            } else { native_write(ctx, child, None, value, compound, span, diagnosed); }
        },
        |ctx, receiver| {
            let mut roots = Vec::new();
            let object = runtime_receivers::box_opaque_receiver(ctx, receiver, span, &mut roots);
            if let Some(index) = index {
                with_late_property_key(ctx, late_key, index, span,
                    |ctx, index| write_ordinary_compound(ctx, object, property, index, value, compound, span, diagnosed));
            } else {
                lower_property_array_push_value(ctx, object, property, value, span);
            }
            retire_property_write_roots(ctx, roots, span);
        });
}

/// Uses the distinct append entry or computes a compound value from the selected native dimension.
fn native_write(ctx: &mut LoweringContext<'_, '_>, child: LoweredValue, index: Option<LoweredValue>, value: LoweredValue, compound: Option<(&BinOp, &Expr)>, span: Span, diagnosed: bool) {
    let Some(index) = index else {
        let handled = super::super::native_dimension_append::lower(ctx, child, value, span, |_, _, _| {});
        assert!(handled, "native XML child append has native dimensions");
        return;
    };
    if let Some((op, key)) = compound {
        with_compound_value(ctx, child, index, value, op, key, span, |ctx, stored| {
            array_write_core::lower_boxed_array_set_values(ctx, child, index, stored, span, diagnosed);
        });
    } else { array_write_core::lower_boxed_array_set_values(ctx, child, index, value, span, diagnosed); }
}

/// Fetches an attached dynamic cell with hash and payload COW before retaining its write lease.
pub(super) fn attached_append(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str, value: LoweredValue, span: Span,
) {
    let data = ctx.intern_string(property);
    let name = ctx.emit_value(Op::ConstStr, vec![], Some(Immediate::Data(data)), PhpType::Str,
        Op::ConstStr.default_effects(), Some(span));
    let array = ctx.emit_owned_value(Op::RuntimeCall, vec![object.value, name.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::PropertyHashFetchForWrite)), PhpType::Mixed,
        effects_lookup::runtime_effects(), Some(span));
    let (array, owner) = crate::ir_lower::expr::root_owned_call_operand(ctx, array, span);
    let borrowed = ctx.emit_value(Op::Borrow, vec![array.value], None, PhpType::Mixed,
        Op::Borrow.default_effects(), Some(span));
    ctx.builder.set_value_ownership(borrowed.value, Ownership::Borrowed);
    if !super::super::native_dimension_append::lower(ctx, borrowed, value, span, |ctx, array, value| {
        ctx.emit_void(Op::MixedArrayAppend, vec![array.value, value.value], None,
            Op::MixedArrayAppend.default_effects(), Some(span));
    }) {
        ctx.emit_void(Op::MixedArrayAppend, vec![borrowed.value, value.value], None,
            Op::MixedArrayAppend.default_effects(), Some(span));
    }
    if let Some(owner) = owner { crate::ir_lower::expr::retire_owned_call_operand(ctx, owner, span); }
}

/// Appends through an ordinary runtime property with an independent COW cell and explicit writeback.
pub(super) fn ordinary_append(ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str, value: LoweredValue, span: Span) {
    let mut roots = Vec::new();
    let object = runtime_receivers::box_opaque_receiver(ctx, object, span, &mut roots);
    let data = ctx.intern_string(property);
    let name = ctx.emit_value(Op::ConstStr, vec![], Some(Immediate::Data(data)), PhpType::Str,
        Op::ConstStr.default_effects(), Some(span));
    let current = ctx.emit_value(Op::DynamicPropGet, vec![object.value, name.value],
        Some(Immediate::PropertyFetchMode(crate::ir::PropertyFetchMode::Probe)), PhpType::Mixed,
        Op::DynamicPropGet.default_effects(), Some(span));
    let array = ctx.emit_owned_value(Op::MixedClone, vec![current.value], None, PhpType::Mixed,
        Op::MixedClone.default_effects(), Some(span));
    crate::ir_lower::ownership::release_if_owned(ctx, current, Some(span));
    let (array, owner) = crate::ir_lower::expr::root_owned_call_operand(ctx, array, span);
    let borrowed = ctx.emit_value(Op::Borrow, vec![array.value], None, PhpType::Mixed,
        Op::Borrow.default_effects(), Some(span));
    ctx.builder.set_value_ownership(borrowed.value, Ownership::Borrowed);
    let writeback = |ctx: &mut LoweringContext<'_, '_>, array: LoweredValue, value: LoweredValue| {
        ctx.emit_void(Op::MixedArrayAppend, vec![array.value, value.value], None,
            Op::MixedArrayAppend.default_effects(), Some(span));
        ctx.emit_void(Op::DynamicPropSet, vec![object.value, name.value, array.value], None,
            Op::DynamicPropSet.default_effects(), Some(span));
    };
    if !super::super::native_dimension_append::lower(ctx, borrowed, value, span, |ctx, array, value| writeback(ctx, array, value)) {
        writeback(ctx, borrowed, value);
    }
    if let Some(owner) = owner { crate::ir_lower::expr::retire_owned_call_operand(ctx, owner, span); }
    retire_property_write_roots(ctx, roots, span);
}

/// Completes one arm of a native receiver guard.
fn jump(ctx: &mut LoweringContext<'_, '_>, target: BlockId) {
    ctx.builder.terminate(Terminator::Br { target, args: vec![] });
}
