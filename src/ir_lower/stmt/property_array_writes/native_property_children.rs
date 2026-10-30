//! Purpose:
//! Probes ordinary stored property values for independently owned native XML children.
//!
//! Called from:
//! - Native XML write-fetch lowering for owning temporary property receivers.
//!
//! Key details:
//! - Plain probes read initialized accessible fields or existing dynamic entries without mutation.
//! - Already supported XML-typed get hooks are fetched once, with no retry on a null result.
//! - Non-native values fall through to their original guards and writeback semantics.

use super::*;

/// Identifies ordinary field storage versus a dynamic entry that must already exist.
#[derive(Clone, Copy)]
enum Storage { Declared, Dynamic, NativeHook }

/// Selects a native child by its actual value, returning null for every other storage shape.
pub(super) fn fetch(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, expression: &Expr,
    property: &str, span: Span,
) -> Option<LoweredValue> {
    let ty = ctx.builder.value_php_type(object.value);
    let known = crate::ir_lower::expr::singular_object_class(&ty).and_then(|(class, _)|
        (!class.is_empty() && !ctx.interfaces.contains_key(class)).then(|| class.to_string()));
    let mut classes = ctx.classes.iter().filter_map(|(class, info)| {
        if known.as_ref().is_some_and(|base|
            class != base && !crate::types::class_inherits_from(ctx.classes, class, base)) { return None; }
        storage(ctx, class, info, property).map(|storage| (class.clone(), storage))
    }).collect::<Vec<_>>();
    classes.sort_by(|left, right| left.0.cmp(&right.0));
    if classes.is_empty() { return None; }
    let result = ctx.declare_owned_hidden_temp(PhpType::Mixed);
    let name = ctx.declare_owned_hidden_temp(PhpType::Str);
    let name_slot = ctx.local_slots[&name];
    crate::ir_lower::expr::register_owned_call_operand(ctx, name_slot, span);
    let objects = ctx.builder.create_named_block("xml.property_child.runtime_class", vec![]);
    let missing = ctx.builder.create_named_block("xml.property_child.ordinary", vec![]);
    let done = ctx.builder.create_named_block("xml.property_child.merge", vec![]);
    let initial = ctx.initialized_slots_snapshot();
    let is_object = ctx.emit_value(Op::TypePredicate, vec![object.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Object)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(span));
    ctx.builder.terminate(Terminator::CondBr { cond: is_object.value,
        then_target: objects, then_args: vec![], else_target: missing, else_args: vec![] });
    ctx.builder.position_at_end(objects);
    let target = crate::ir::RuntimeFnId::GetClass;
    let class = ctx.emit_value(Op::RuntimeCall, vec![object.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(target))), PhpType::Str,
        target.effects(), Some(span));
    crate::ir_lower::expr::store_value_into_temp(ctx, &name, PhpType::Str, class, span);
    let mut merged = None;
    for (class, storage) in classes {
        let matched = ctx.builder.create_named_block("xml.property_child.stored", vec![]);
        let next = ctx.builder.create_named_block("xml.property_child.next_class", vec![]);
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
        let concrete = PhpType::Object(class);
        let receiver = if matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
            ctx.emit_value(Op::MixedUnbox, vec![object.value], None, concrete.clone(),
                Op::mixed_unbox_effects(&concrete), Some(span))
        } else {
            ctx.emit_value(Op::Borrow, vec![object.value], None, concrete,
                Op::Borrow.default_effects(), Some(span))
        };
        ctx.builder.set_value_ownership(receiver.value, Ownership::Borrowed);
        let child = read(ctx, receiver, expression, property, storage, missing, span);
        let child = ctx.box_value_as_mixed(child, PhpType::Mixed, Some(span));
        let class = ctx.intern_class_name("SimpleXMLElement");
        let native = ctx.emit_value(Op::InstanceOf, vec![child.value], Some(Immediate::Data(class)),
            PhpType::Bool, Op::InstanceOf.default_effects(), Some(span));
        let selected = ctx.builder.create_named_block("xml.property_child.native", vec![]);
        let ordinary = ctx.builder.create_named_block("xml.property_child.not_native", vec![]);
        let split = ctx.initialized_slots_snapshot();
        ctx.builder.terminate(Terminator::CondBr { cond: native.value,
            then_target: selected, then_args: vec![], else_target: ordinary, else_args: vec![] });
        ctx.builder.position_at_end(selected);
        crate::ir_lower::expr::store_value_into_temp(ctx, &result, PhpType::Mixed, child, span);
        merge_slots(&mut merged, ctx.initialized_slots_snapshot());
        jump(ctx, done);
        ctx.builder.position_at_end(ordinary);
        ctx.restore_initialized_slots(split);
        crate::ir_lower::ownership::release_if_owned(ctx, child, Some(span));
        if matches!(storage, Storage::NativeHook) {
            let PhpType::Object(class) = ctx.builder.value_php_type(receiver.value) else { unreachable!() };
            let owner = ctx.classes.get(&class).and_then(|info| info.property_declaring_classes.get(property))
                .cloned().unwrap_or(class);
            lower_throw_access_exception(ctx, "Error",
                &format!("Indirect modification of {}::${property} is not allowed", owner.trim_start_matches('\\')), span);
        }
        jump(ctx, missing);
        ctx.builder.position_at_end(next);
        ctx.restore_initialized_slots(before);
    }
    jump(ctx, missing);
    ctx.builder.position_at_end(missing);
    ctx.restore_initialized_slots(initial);
    let null = lower_expr(ctx, &Expr::new(ExprKind::Null, span));
    crate::ir_lower::expr::store_value_into_temp(ctx, &result, PhpType::Mixed, null, span);
    merge_slots(&mut merged, ctx.initialized_slots_snapshot());
    jump(ctx, done);
    ctx.builder.position_at_end(done);
    ctx.restore_initialized_slots(merged.expect("ordinary storage always reaches the merge"));
    crate::ir_lower::expr::retire_owned_call_operand(ctx, name_slot, span);
    Some(crate::ir_lower::expr::take_owned_temp(ctx, &result, span))
}

/// Selects ordinary storage and preserves the existing XML-typed getter surface.
fn storage(
    ctx: &LoweringContext<'_, '_>, class: &str, info: &crate::types::ClassInfo, property: &str,
) -> Option<Storage> {
    if crate::types::builtin_classes::native_property_method(class, property,
        elephc_builtin_contract::ObjectProperty::Set, ctx.classes).is_some() { return None; }
    let accessible = crate::ir_lower::expr::property_is_accessible_for_ir(ctx, class, info, property);
    if info.property_is_virtual(property)
        && !info.property_hooks.get(property).is_some_and(|hooks| hooks.requires_get()) { return None; }
    if info.property_hooks.get(property).is_some_and(|hooks| hooks.requires_get()) {
        let (_, (_, ty)) = info.visible_property(property)?;
        return (accessible && crate::ir_lower::expr::singular_object_class(ty).is_some_and(|(child, _)|
            crate::types::builtin_classes::has_native_dimensions(child, ctx.classes)))
            .then_some(Storage::NativeHook);
    }
    if accessible {
        if let Some((_, (_, ty))) = info.visible_property(property) {
            return may_contain_native(ctx, ty).then_some(Storage::Declared);
        }
    }
    let hash = crate::types::checker::builtin_stdclass::is_stdclass(class)
        || info.dynamic_property_hash_is_name_addressable();
    (hash && (runtime_receivers::dynamic_slot(info, class, property)
        || magic_append::is_candidate(ctx, class, info, property))).then_some(Storage::Dynamic)
}

/// Includes general object-bearing PHP types without treating scalar or array payloads as objects.
fn may_contain_native(ctx: &LoweringContext<'_, '_>, ty: &PhpType) -> bool {
    match ty {
        PhpType::Mixed | PhpType::Iterable => true,
        PhpType::Object(class) => class.is_empty() || ctx.interfaces.contains_key(class)
            || crate::types::builtin_classes::has_native_dimensions(class, ctx.classes),
        PhpType::Union(members) => members.iter().any(|ty| may_contain_native(ctx, ty)),
        _ => false,
    }
}

/// Fetches one selected field, existing entry or supported XML-typed hook result.
fn read(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, expression: &Expr,
    property: &str, storage: Storage, missing: BlockId, span: Span,
) -> LoweredValue {
    if matches!(storage, Storage::NativeHook) {
        let expr = Expr::new(ExprKind::PropertyAccess {
            object: Box::new(expression.clone()), property: property.into(),
        }, span);
        return crate::ir_lower::expr::lower_property_get_from_value(ctx, object, property, Op::PropGet, &expr);
    }
    let data = ctx.intern_string(property);
    let present = match storage {
        Storage::NativeHook => unreachable!(),
        Storage::Declared => ctx.emit_value(Op::PropInitialized, vec![object.value],
            Some(Immediate::Data(data)), PhpType::Bool, Op::PropInitialized.default_effects(), Some(span)),
        Storage::Dynamic => {
            let name = ctx.emit_value(Op::ConstStr, vec![], Some(Immediate::Data(data)), PhpType::Str,
                Op::ConstStr.default_effects(), Some(span));
            ctx.emit_value(Op::RuntimeCall, vec![object.value, name.value],
                Some(Immediate::RuntimeCall(RuntimeCallTarget::PropertyHashContains)), PhpType::Bool,
                effects_lookup::runtime_effects(), Some(span))
        }
    };
    let stored = ctx.builder.create_named_block("xml.property_child.read", vec![]);
    ctx.builder.terminate(Terminator::CondBr { cond: present.value,
        then_target: stored, then_args: vec![], else_target: missing, else_args: vec![] });
    ctx.builder.position_at_end(stored);
    match storage {
        Storage::NativeHook => unreachable!(),
        Storage::Declared => {
            let expr = Expr::new(ExprKind::PropertyAccess {
                object: Box::new(expression.clone()), property: property.into(),
            }, span);
            crate::ir_lower::expr::lower_property_get_from_value(ctx, object, property, Op::PropGet, &expr)
        }
        Storage::Dynamic => {
            let name = ctx.emit_value(Op::ConstStr, vec![], Some(Immediate::Data(data)), PhpType::Str,
                Op::ConstStr.default_effects(), Some(span));
            ctx.emit_owned_value(Op::RuntimeCall, vec![object.value, name.value],
                Some(Immediate::RuntimeCall(RuntimeCallTarget::PropertyHashRead)), PhpType::Mixed,
                effects_lookup::runtime_effects(), Some(span))
        }
    }
}

/// Intersects only reachable arm snapshots after each arm has stored its selected value.
fn merge_slots(
    merged: &mut Option<std::collections::HashSet<LocalSlotId>>,
    state: std::collections::HashSet<LocalSlotId>,
) {
    *merged = Some(match merged.take() {
        Some(previous) => previous.intersection(&state).copied().collect(), None => state,
    });
}

/// Ends a selected child arm without adding an edge after an exception.
fn jump(ctx: &mut LoweringContext<'_, '_>, target: BlockId) {
    if !ctx.builder.insertion_block_is_terminated() {
        ctx.builder.terminate(Terminator::Br { target, args: vec![] });
    }
}
