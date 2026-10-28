//! Purpose:
//! Selects native XML property write-fetch after dimension keys and values.
//!
//! Called from:
//! - Instance/static property array append and indexed assignment lowering.
//!
//! Key details:
//! - Opaque receivers return a null sentinel on the ordinary-object branch.
//! - The parent owns receiver/key/value cleanup; the writer roots its selected child.

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
    ctx: &mut LoweringContext<'_, '_>, receiver: LoweredValue, property: &str,
    span: Span,
) -> Option<Fetch> {
    if property.starts_with("__elephc_") || !ctx.classes.contains_key("SimpleXMLElement") { return None; }
    let ty = ctx.builder.value_php_type(receiver.value);
    let known = crate::ir_lower::expr::singular_object_class(&ty).is_some_and(|(class, _)|
        crate::types::builtin_classes::native_property_method(class, property,
            elephc_builtin_contract::ObjectProperty::WriteFetch, ctx.classes).is_some());
    let opaque = matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
        || matches!(ty.codegen_repr(), PhpType::Object(class)
            if class.is_empty() || ctx.interfaces.contains_key(&class));
    if !known && !opaque { return None; }
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
    let null = lower_expr(ctx, &Expr::new(ExprKind::Null, span));
    crate::ir_lower::expr::store_value_into_temp(ctx, &temp, PhpType::Mixed, null, span);
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

/// Publishes the child's independent owner and lends the writer a non-consuming operand.
fn root(ctx: &mut LoweringContext<'_, '_>, child: LoweredValue, roots: &mut Vec<LocalSlotId>, span: Span) -> LoweredValue {
    let (child, owner) = crate::ir_lower::expr::root_owned_call_operand(ctx, child, span);
    let Some(owner) = owner else { return child; };
    roots.push(owner);
    let ty = ctx.builder.value_php_type(child.value);
    let value = ctx.emit_value(Op::Borrow, vec![child.value], None, ty,
        Op::Borrow.default_effects(), Some(span));
    ctx.builder.set_value_ownership(value.value, Ownership::Borrowed);
    value
}

/// Runs one update arm while the selected child remains independently rooted.
pub(super) fn dispatch(
    ctx: &mut LoweringContext<'_, '_>, fetch: Fetch, span: Span,
    native_update: impl FnOnce(&mut LoweringContext<'_, '_>, LoweredValue),
    ordinary_update: impl FnOnce(&mut LoweringContext<'_, '_>),
) {
    let mut roots = Vec::new();
    let child = root(ctx, fetch.value, &mut roots, span);
    if !fetch.conditional {
        native_update(ctx, child);
    } else {
        let absent = ctx.emit_value(Op::IsNull, vec![child.value], None, PhpType::Bool,
            Op::IsNull.default_effects(), Some(span));
        let native = ctx.builder.create_named_block("xml.dimension.incdec.native", vec![]);
        let ordinary = ctx.builder.create_named_block("xml.dimension.incdec.ordinary", vec![]);
        let merge = ctx.builder.create_named_block("xml.dimension.incdec.merge", vec![]);
        let initial = ctx.initialized_slots_snapshot();
        ctx.builder.terminate(Terminator::CondBr { cond: absent.value,
            then_target: ordinary, then_args: vec![], else_target: native, else_args: vec![] });
        ctx.builder.position_at_end(native);
        native_update(ctx, child);
        jump(ctx, merge);
        let native_slots = ctx.initialized_slots_snapshot();
        ctx.builder.position_at_end(ordinary);
        ctx.restore_initialized_slots(initial);
        ordinary_update(ctx);
        jump(ctx, merge);
        let ordinary_slots = ctx.initialized_slots_snapshot();
        ctx.builder.position_at_end(merge);
        ctx.restore_initialized_slots(native_slots.intersection(&ordinary_slots).copied().collect());
    }
    retire_property_write_roots(ctx, roots, span);
}

/// Chooses native offsetSet or the unchanged ordinary property writer after operands are staged.
pub(super) fn write(
    ctx: &mut LoweringContext<'_, '_>, receiver: LoweredValue, fetch: Fetch, property: &str,
    index: Option<LoweredValue>, value: LoweredValue, compound: Option<(&BinOp, &Expr)>, span: Span, diagnosed: bool,
) {
    let mut roots = Vec::new();
    let fetch = Fetch { value: root(ctx, fetch.value, &mut roots, span), conditional: fetch.conditional };
    write_rooted(ctx, receiver, fetch, property, index, value, compound, span, diagnosed);
    retire_property_write_roots(ctx, roots, span);
}

/// Keeps the selected child alive through native writes, fallback calls and warning handlers.
fn write_rooted(
    ctx: &mut LoweringContext<'_, '_>, receiver: LoweredValue, fetch: Fetch, property: &str,
    index: Option<LoweredValue>, value: LoweredValue, compound: Option<(&BinOp, &Expr)>, span: Span, diagnosed: bool,
) {
    if !fetch.conditional { native_write(ctx, fetch.value, index, value, compound, span, diagnosed); return; }
    let absent = ctx.emit_value(Op::IsNull, vec![fetch.value.value], None, PhpType::Bool,
        Op::IsNull.default_effects(), Some(span));
    let native = ctx.builder.create_named_block("xml.dimension.write.native", vec![]);
    let ordinary = ctx.builder.create_named_block("xml.dimension.write.ordinary", vec![]);
    let merge = ctx.builder.create_named_block("xml.dimension.write.merge", vec![]);
    let initial = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr { cond: absent.value,
        then_target: ordinary, then_args: vec![], else_target: native, else_args: vec![] });
    ctx.builder.position_at_end(native);
    native_write(ctx, fetch.value, index, value, compound, span, diagnosed);
    jump(ctx, merge);
    let native_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(ordinary);
    ctx.restore_initialized_slots(initial);
    let ty = ctx.builder.value_php_type(receiver.value);
    let boxed = matches!(ty.codegen_repr(), PhpType::Object(class)
        if class.is_empty() || ctx.interfaces.contains_key(&class));
    let object = if boxed {
        let borrowed = ctx.emit_value(Op::Borrow, vec![receiver.value], None, ty,
            Op::Borrow.default_effects(), Some(span));
        ctx.builder.set_value_ownership(borrowed.value, Ownership::Borrowed);
        ctx.box_value_as_mixed(borrowed, PhpType::Mixed, Some(span))
    } else { receiver };
    let pins = if boxed { crate::ir_lower::expr::pin_in_flight_owners(ctx, &[object.value], span) } else { vec![] };
    if let Some(index) = index {
        write_ordinary_compound(ctx, object, property, index, value, compound, span, diagnosed);
    } else { ordinary_append(ctx, object, property, value, span); }
    crate::ir_lower::expr::unpin_in_flight_owners(ctx, pins, span);
    if boxed { crate::ir_lower::ownership::release_if_owned(ctx, object, Some(span)); }
    jump(ctx, merge);
    let ordinary_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(merge);
    ctx.restore_initialized_slots(native_slots.intersection(&ordinary_slots).copied().collect());
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

/// Appends through an ordinary runtime property with an independent COW cell and explicit writeback.
pub(super) fn ordinary_append(ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str, value: LoweredValue, span: Span) {
    let data = ctx.intern_string(property);
    let name = ctx.emit_value(Op::ConstStr, vec![], Some(Immediate::Data(data)), PhpType::Str,
        Op::ConstStr.default_effects(), Some(span));
    let current = ctx.emit_value(Op::DynamicPropGet, vec![object.value, name.value],
        Some(Immediate::PropertyFetchMode(crate::ir::PropertyFetchMode::Probe)), PhpType::Mixed,
        Op::DynamicPropGet.default_effects(), Some(span));
    let array = ctx.emit_owned_value(Op::RuntimeCall, vec![current.value],
        Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::MixedCellClone)), PhpType::Mixed,
        effects_lookup::runtime_effects(), Some(span));
    crate::ir_lower::ownership::release_if_owned(ctx, current, Some(span));
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
    ctx.emit_void(Op::DynamicPropSet, vec![object.value, name.value, borrowed.value], None,
        Op::DynamicPropSet.default_effects(), Some(span));
    if let Some(owner) = owner { crate::ir_lower::expr::retire_owned_call_operand(ctx, owner, span); }
}

/// Completes one arm of a native receiver guard.
fn jump(ctx: &mut LoweringContext<'_, '_>, target: BlockId) {
    ctx.builder.terminate(Terminator::Br { target, args: vec![] });
}
