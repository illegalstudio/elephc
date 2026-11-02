//! Purpose:
//! Selects dynamic entries and overloaded or inaccessible indexed property receivers.
//!
//! Called from:
//! - Ordinary indexed property writes and compounds before declared-slot COW lowering.
//!
//! Key details:
//! - Exact classes preserve PHP read visibility and one getter invocation per update.
//! - Existing dynamic entries use attached write cells before considering magic access.
//! - ArrayAccess objects mutate directly without publishing a parent-property replacement.

use super::*;

/// Runtime metadata that needs a full read protocol instead of the stdClass-only fallback.
#[derive(Clone, Copy)]
enum Kind { Dynamic, Magic { hash: bool }, Inaccessible }

/// Dispatches only special runtime names and leaves ordinary declared containers to the caller.
pub(super) fn lower(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str,
    index: LoweredValue, value: LoweredValue, compound: Option<(&BinOp, &Expr)>, span: Span, diagnosed: bool,
    fallback: impl FnOnce(&mut LoweringContext<'_, '_>, LoweredValue),
) -> bool {
    let ty = ctx.builder.value_php_type(object.value);
    let base = crate::ir_lower::expr::singular_object_class(&ty).map(|(class, _)| class)
        .filter(|class| !class.is_empty() && !ctx.interfaces.contains_key(*class));
    let mut classes = ctx.classes.iter().filter_map(|(class, info)| {
        if base.is_some_and(|base| !crate::ir_lower::expr::class_extends_class(ctx, class, base)) { return None; }
        kind(ctx, class, info, property).map(|kind| (class.clone(), kind))
    }).collect::<Vec<_>>();
    classes.sort_by(|left, right| left.0.cmp(&right.0));
    if classes.is_empty() { return false; }
    let mut roots = Vec::new();
    let object = if matches!(ty.codegen_repr(), PhpType::Object(_)) {
        let borrowed = ctx.emit_value(Op::Borrow, vec![object.value], None, ty,
            Op::Borrow.default_effects(), Some(span));
        ctx.builder.set_value_ownership(borrowed.value, Ownership::Borrowed);
        let boxed = ctx.box_value_as_mixed(borrowed, PhpType::Mixed, Some(span));
        root_property_write_value(ctx, boxed, span, &mut roots)
    } else { object };
    let name = ctx.declare_owned_hidden_temp(PhpType::Str);
    let name_slot = ctx.local_slots[&name];
    crate::ir_lower::expr::register_owned_call_operand(ctx, name_slot, span);
    let select = ctx.builder.create_named_block("property.index.runtime_class", vec![]);
    let regular = ctx.builder.create_named_block("property.index.regular", vec![]);
    let done = ctx.builder.create_named_block("property.index.merge", vec![]);
    let initial = ctx.initialized_slots_snapshot();
    let condition = ctx.emit_value(Op::TypePredicate, vec![object.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Object)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(span));
    ctx.builder.terminate(Terminator::CondBr { cond: condition.value,
        then_target: select, then_args: vec![], else_target: regular, else_args: vec![] });
    ctx.builder.position_at_end(select);
    let target = crate::ir::RuntimeFnId::GetClass;
    let class_name = ctx.emit_value(Op::RuntimeCall, vec![object.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(target))), PhpType::Str,
        target.effects(), Some(span));
    crate::ir_lower::expr::store_value_into_temp(ctx, &name, PhpType::Str, class_name, span);
    let mut merged = None;
    for (class, kind) in classes {
        let matched = ctx.builder.create_named_block("property.index.special", vec![]);
        let next = ctx.builder.create_named_block("property.index.next", vec![]);
        let current = ctx.load_local(&name, Some(span));
        ctx.builder.set_value_ownership(current.value, Ownership::Borrowed);
        let data = ctx.intern_string(class.trim_start_matches('\\'));
        let expected = ctx.emit_value(Op::ConstStr, vec![], Some(Immediate::Data(data)), PhpType::Str,
            Op::ConstStr.default_effects(), Some(span));
        let condition = ctx.emit_value(Op::StrictEq, vec![current.value, expected.value], None,
            PhpType::Bool, Op::StrictEq.default_effects(), Some(span));
        let before = ctx.initialized_slots_snapshot();
        ctx.builder.terminate(Terminator::CondBr { cond: condition.value,
            then_target: matched, then_args: vec![], else_target: next, else_args: vec![] });
        ctx.builder.position_at_end(matched);
        let concrete = PhpType::Object(class.clone());
        let receiver = ctx.emit_value(Op::MixedUnbox, vec![object.value], None, concrete.clone(),
            Op::mixed_unbox_effects(&concrete), Some(span));
        ctx.builder.set_value_ownership(receiver.value, Ownership::Borrowed);
        write(ctx, object, receiver, &class, property, kind, index, value, compound, span, diagnosed);
        merge(ctx, done, &mut merged);
        ctx.builder.position_at_end(next);
        ctx.restore_initialized_slots(before);
    }
    ctx.builder.terminate(Terminator::Br { target: regular, args: vec![] });
    ctx.builder.position_at_end(regular);
    ctx.restore_initialized_slots(initial);
    fallback(ctx, object);
    merge(ctx, done, &mut merged);
    ctx.builder.position_at_end(done);
    if let Some(merged) = merged { ctx.restore_initialized_slots(merged); }
    crate::ir_lower::expr::retire_owned_call_operand(ctx, name_slot, span);
    retire_property_write_roots(ctx, roots, span);
    true
}

/// Keeps hook-backed and native XML fields on their dedicated paths.
fn kind(ctx: &LoweringContext<'_, '_>, class: &str, info: &crate::types::ClassInfo, property: &str) -> Option<Kind> {
    if info.property_hooks.get(property).is_some_and(|hooks| hooks.any())
        || crate::types::builtin_classes::has_native_dimensions(class, ctx.classes) { return None; }
    if magic_append::is_candidate(ctx, class, info, property) {
        let hash = info.visible_property(property).is_none() && property.parse::<i64>().is_err()
            && (crate::types::checker::builtin_stdclass::is_stdclass(class)
                || info.dynamic_property_hash_is_name_addressable());
        return Some(Kind::Magic { hash });
    }
    if runtime_receivers::dynamic_slot(info, class, property) { return Some(Kind::Dynamic); }
    if info.property_visibilities.get(property) == Some(&crate::parser::ast::Visibility::Private)
        && info.property_declaring_classes.get(property).is_some_and(|declaring| declaring != class) {
        return info.dynamic_property_hash_is_name_addressable().then_some(Kind::Dynamic);
    }
    (info.visible_property(property).is_some()
        && !crate::ir_lower::expr::property_is_accessible_for_ir(ctx, class, info, property))
        .then_some(Kind::Inaccessible)
}

/// Uses attached entries first, otherwise performs one visibility-aware property read.
fn write(
    ctx: &mut LoweringContext<'_, '_>, boxed: LoweredValue, object: LoweredValue,
    class: &str, property: &str, kind: Kind, index: LoweredValue, value: LoweredValue,
    compound: Option<(&BinOp, &Expr)>, span: Span, diagnosed: bool,
) {
    let name = string(ctx, property, span);
    if matches!(kind, Kind::Dynamic) {
        attached(ctx, object, name, index, value, compound, span, diagnosed);
        return;
    }
    if matches!(kind, Kind::Magic { hash: true }) {
        let present = ctx.emit_value(Op::RuntimeCall, vec![object.value, name.value],
            Some(Immediate::RuntimeCall(RuntimeCallTarget::PropertyHashContains)), PhpType::Bool,
            effects_lookup::runtime_effects(), Some(span));
        let stored = ctx.builder.create_named_block("property.index.dynamic", vec![]);
        let getter = ctx.builder.create_named_block("property.index.getter", vec![]);
        let done = ctx.builder.create_named_block("property.index.fetched", vec![]);
        let initial = ctx.initialized_slots_snapshot();
        let mut merged = None;
        ctx.builder.terminate(Terminator::CondBr { cond: present.value,
            then_target: stored, then_args: vec![], else_target: getter, else_args: vec![] });
        ctx.builder.position_at_end(stored);
        attached(ctx, object, name, index, value, compound, span, diagnosed);
        merge(ctx, done, &mut merged);
        ctx.builder.position_at_end(getter);
        ctx.restore_initialized_slots(initial);
        fetched(ctx, boxed, name, class, property, true, index, value, compound, span, diagnosed);
        merge(ctx, done, &mut merged);
        ctx.builder.position_at_end(done);
        if let Some(merged) = merged { ctx.restore_initialized_slots(merged); }
    } else {
        fetched(ctx, boxed, name, class, property, matches!(kind, Kind::Magic { .. }),
            index, value, compound, span, diagnosed);
    }
}

/// Fetches an independently guarded attached hash cell before changing its array payload.
fn attached(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, name: LoweredValue,
    index: LoweredValue, value: LoweredValue, compound: Option<(&BinOp, &Expr)>, span: Span, diagnosed: bool,
) {
    let array = ctx.emit_owned_value(Op::RuntimeCall, vec![object.value, name.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::PropertyHashFetchForWrite)), PhpType::Mixed,
        effects_lookup::runtime_effects(), Some(span));
    let (array, owner) = crate::ir_lower::expr::root_owned_call_operand(ctx, array, span);
    mutate(ctx, array, index, value, compound, span, diagnosed);
    if let Some(owner) = owner { crate::ir_lower::expr::retire_owned_call_operand(ctx, owner, span); }
}

/// Reads inaccessible and magic names through the complete runtime class dispatcher.
fn fetched(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, name: LoweredValue, class: &str, property: &str,
    magic: bool, index: LoweredValue, value: LoweredValue, compound: Option<(&BinOp, &Expr)>, span: Span, diagnosed: bool,
) {
    let current = ctx.emit_owned_value(Op::DynamicPropGet, vec![object.value, name.value],
        Some(Immediate::PropertyFetchMode(crate::ir::PropertyFetchMode::Read)), PhpType::Mixed,
        Op::DynamicPropGet.default_effects(), Some(span));
    // A by-value getter may expose storage shared with its backing field. Its temporary
    // zval must detach before an array write, while an object keeps its identity.
    let array = ctx.emit_owned_value(Op::MixedClone, vec![current.value], None, PhpType::Mixed,
        Op::MixedClone.default_effects(), Some(span));
    crate::ir_lower::ownership::release_if_owned(ctx, current, Some(span));
    let (array, owner) = crate::ir_lower::expr::root_owned_call_operand(ctx, array, span);
    if magic {
        let condition = ctx.emit_value(Op::TypePredicate, vec![array.value],
            Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Object)), PhpType::Bool,
            Op::TypePredicate.default_effects(), Some(span));
        let supported = ctx.builder.create_named_block("property.index.getter_object", vec![]);
        let unsupported = ctx.builder.create_named_block("property.index.getter_value", vec![]);
        ctx.builder.terminate(Terminator::CondBr { cond: condition.value,
            then_target: supported, then_args: vec![], else_target: unsupported, else_args: vec![] });
        ctx.builder.position_at_end(unsupported);
        let notice = string(ctx,
            &format!("Notice: Indirect modification of overloaded property {class}::${property} has no effect\n"), span);
        ctx.emit_void(Op::RuntimeCall, vec![notice.value],
            Some(Immediate::RuntimeCall(RuntimeCallTarget::WriteDiagnostic)), crate::ir::Effects::all(), Some(span));
        ctx.builder.terminate(Terminator::Br { target: supported, args: vec![] });
        ctx.builder.position_at_end(supported);
    }
    mutate(ctx, array, index, value, compound, span, diagnosed);
    if let Some(owner) = owner { crate::ir_lower::expr::retire_owned_call_operand(ctx, owner, span); }
}

/// Shares a single captured container and key between the compound read and write.
fn mutate(
    ctx: &mut LoweringContext<'_, '_>, array: LoweredValue, index: LoweredValue,
    value: LoweredValue, compound: Option<(&BinOp, &Expr)>, span: Span, diagnosed: bool,
) {
    let borrowed = ctx.emit_value(Op::Borrow, vec![array.value], None, PhpType::Mixed,
        Op::Borrow.default_effects(), Some(span));
    ctx.builder.set_value_ownership(borrowed.value, Ownership::Borrowed);
    if let Some((op, key)) = compound {
        with_compound_value(ctx, borrowed, index, value, op, key, span, |ctx, stored|
            array_write_core::lower_boxed_array_set_values(ctx, borrowed, index, stored, span, diagnosed));
    } else { array_write_core::lower_boxed_array_set_values(ctx, borrowed, index, value, span, diagnosed); }
}

/// Interns one immutable PHP property name for hash and full property dispatch.
fn string(ctx: &mut LoweringContext<'_, '_>, value: &str, span: Span) -> LoweredValue {
    let data = ctx.intern_string(value);
    ctx.emit_value(Op::ConstStr, vec![], Some(Immediate::Data(data)), PhpType::Str,
        Op::ConstStr.default_effects(), Some(span))
}

/// Intersects initialized owners from the normal arms only.
fn merge(ctx: &mut LoweringContext<'_, '_>, done: BlockId, merged: &mut Option<std::collections::HashSet<LocalSlotId>>) {
    if ctx.builder.insertion_block_is_terminated() { return; }
    let state = ctx.initialized_slots_snapshot();
    *merged = Some(match merged.take() {
        Some(previous) => previous.intersection(&state).copied().collect(), None => state,
    });
    ctx.builder.terminate(Terminator::Br { target: done, args: vec![] });
}
