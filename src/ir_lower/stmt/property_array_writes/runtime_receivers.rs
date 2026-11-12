//! Purpose:
//! Resolves declared array-property writes whose receiver has lost its concrete class.
//!
//! Called from:
//! - Indexed and append property writers after receiver, key and value evaluation.
//!
//! Key details:
//! - Exact runtime classes prevent an ancestor arm from bypassing a subclass hook.
//! - Ordinary declared slots reuse their typed COW writer without invoking a magic setter.
//! - Ordinary dynamic appends reuse attached hash cells; accessor fallbacks retain writeback.

use super::*;
use crate::ir::ValueId;

use super::PropertyDimensionFetchMode as FetchMode;

/// Describes declaration-backed initialization or rejection and optional null-payload checks.
struct SlotGuard {
    action: SlotAction,
    nullable: bool,
    object_only: bool,
}

/// Distinguishes a prohibited container from legal array autovivification on an uninitialized slot.
enum SlotAction {
    Reject { exception: &'static str, message: String },
    InitializeArray,
}

/// Dispatches an erased receiver to an accessible ordinary declared container slot.
pub(super) fn lower(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str,
    index: LoweredValue, value: LoweredValue, span: Span, diagnosed: bool,
) -> bool {
    dispatch(ctx, object, property, span, FetchMode::Write, false, false, |ctx, receiver, _| {
        lower_property_array_assign_value_direct(ctx, receiver, property, index, value, span, diagnosed);
    })
}

/// Selects exact declared or ordinary dynamic storage before an erased receiver append.
pub(super) fn lower_append(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str,
    value: LoweredValue, span: Span,
) -> bool {
    dispatch(ctx, object, property, span, FetchMode::Write, false, true, |ctx, receiver, selected| {
        if selected {
            if !magic_append::lower(ctx, receiver, property, value, span) {
                lower_property_array_push_value_direct(ctx, receiver, property, value, span);
            }
        } else { native_xml::ordinary_append(ctx, receiver, property, value, span); }
    })
}

/// Recognizes undeclared names with ordinary hash storage and no PHP accessor protocol.
pub(super) fn dynamic_slot(info: &crate::types::ClassInfo, class: &str, property: &str) -> bool {
    (crate::types::checker::builtin_stdclass::is_stdclass(class)
        || info.dynamic_property_hash_is_name_addressable())
        && info.visible_property(property).is_none()
        // Property hashes retain numeric names as strings; array helpers normalize such keys.
        && property.parse::<i64>().is_err()
        && !info.methods.contains_key("__get")
        && !info.property_hooks.get(property).is_some_and(|hooks| hooks.any())
}

/// Returns whether a selected concrete receiver addresses ordinary dynamic property storage.
pub(super) fn has_dynamic_slot(
    ctx: &LoweringContext<'_, '_>, object: ValueId, property: &str,
) -> bool {
    let PhpType::Object(class) = ctx.builder.value_php_type(object).codegen_repr() else { return false; };
    let class = class.trim_start_matches('\\');
    ctx.classes.get(class).is_some_and(|info| dynamic_slot(info, class, property))
}

/// Refuses invalid null slots before a compound read can warn or mutate a readonly property.
pub(super) fn guard_readwrite(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str, span: Span,
) {
    dispatch(ctx, object, property, span, FetchMode::ReadWrite, true, false, |_, _, _| {});
}

/// Refuses invalid null initialization before append or nested write traversal selects storage.
pub(super) fn guard_write(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str, span: Span,
) {
    dispatch(ctx, object, property, span, FetchMode::Write, true, false, |_, _, _| {});
}

/// Recognizes work for one fetch mode without evaluating any PHP input.
pub(super) fn may_need_guard(
    ctx: &LoweringContext<'_, '_>, property: &str, mode: FetchMode,
) -> bool {
    ctx.classes.iter().any(|(name, info)|
        slot_guard(ctx, name, info, property, mode).is_some())
}

/// Derives write-fetch errors from declarations and access, independently of storage layout.
fn slot_guard(
    ctx: &LoweringContext<'_, '_>, class: &str, info: &crate::types::ClassInfo,
    property: &str, mode: FetchMode,
) -> Option<SlotGuard> {
    if !info.declared_properties.contains(property)
        || info.property_hooks.get(property).is_some_and(|hooks| hooks.any())
        || !crate::ir_lower::expr::property_is_accessible_for_ir(ctx, class, info, property)
    { return None; }
    let storage = object_type_property_type(ctx, &PhpType::Object(class.into()), property)?;
    // Elephc native storage keeps its dedicated write and initialization semantics.
    if matches!(storage, PhpType::Buffer(_) | PhpType::Pointer(_) | PhpType::Packed(_)) {
        return None;
    }
    let (_, (_, declared)) = info.visible_property(property)?;
    let declaring = info.property_declaring_classes.get(property).map(String::as_str).unwrap_or(class);
    let inaccessible = info.property_set_visibilities.get(property).is_some_and(|visibility|
        !crate::ir_lower::expr::ir_can_access_member(ctx, declaring, visibility));
    if admits_array(declared) && !info.readonly_properties.contains(property) && !inaccessible {
        return (mode == FetchMode::Write).then_some(SlotGuard {
            action: SlotAction::InitializeArray, nullable: false, object_only: false,
        });
    }
    let nullable = matches!(storage, PhpType::Mixed | PhpType::TaggedScalar);
    let object_only = info.readonly_properties.contains(property) || inaccessible;
    if mode == FetchMode::ReadWrite && !nullable && !object_only { return None; }
    let (exception, message) = null_write_error(ctx, class, info, property, declared);
    Some(SlotGuard { action: SlotAction::Reject { exception, message }, nullable, object_only })
}

/// Selects exactly the accessible declared slots that the runtime writer can handle.
fn declared_slot_candidate(
    ctx: &LoweringContext<'_, '_>, name: &str, info: &crate::types::ClassInfo,
    property: &str, mode: FetchMode,
) -> Option<(Option<SlotGuard>, bool)> {
    if info.property_hooks.get(property).is_some_and(|hooks| hooks.any())
        || !crate::ir_lower::expr::property_is_accessible_for_ir(ctx, name, info, property)
    { return None; }
    // Use the writer's own classifier, which excludes inherited native XML properties.
    let property_ty = object_type_property_type(ctx, &PhpType::Object(name.to_string()), property)?;
    let (_, (_, declared)) = info.visible_property(property)?;
    let object_container = type_satisfies_array_access_for_ir(ctx, declared);
    let supported = property_ty.is_php_array()
        || matches!(property_ty.codegen_repr(), PhpType::Mixed | PhpType::Iterable)
        || matches!(property_ty.codegen_repr(), PhpType::Array(_) | PhpType::AssocArray { .. }
            | PhpType::Buffer(_)) || object_container;
    let mut writable = supported && (object_container || !info.readonly_properties.contains(property));
    // Mutating an ArrayAccess object's contents only reads the owning property.
    if let Some(visibility) = info.property_set_visibilities.get(property).filter(|_| !object_container) {
        let declaring = info.property_declaring_classes.get(property).map(String::as_str)
            .unwrap_or(name);
        if !crate::ir_lower::expr::ir_can_access_member(ctx, declaring, visibility) { writable = false; }
    }
    let null_error = slot_guard(ctx, name, info, property, mode);
    if !writable && null_error.is_none() { return None; }
    Some((null_error, writable))
}

/// Lets runtime-name update planning omit names that would produce no declared dispatch arm.
pub(super) fn has_declared_readwrite_candidate(
    ctx: &LoweringContext<'_, '_>, name: &str, info: &crate::types::ClassInfo, property: &str,
) -> bool {
    declared_slot_candidate(ctx, name, info, property, FetchMode::ReadWrite).is_some()
}

/// Shares exact-class selection between final stores and guards that precede element reads.
fn dispatch(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str, span: Span,
    mode: FetchMode, guard_only: bool, include_dynamic: bool,
    mut apply: impl FnMut(&mut LoweringContext<'_, '_>, LoweredValue, bool),
) -> bool {
    let ty = ctx.builder.value_php_type(object.value);
    let concrete_class = match ty.codegen_repr() {
        PhpType::Object(class) if !class.is_empty() && !ctx.interfaces.contains_key(&class) =>
            Some(class.trim_start_matches('\\').to_string()),
        _ => None,
    };
    let dynamic_dispatch = include_dynamic && concrete_class.as_ref().is_some_and(|class|
        object_type_property_type(ctx, &PhpType::Object(class.clone()), property).is_none());
    if !dynamic_dispatch
        && !matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
        && !matches!(ty.codegen_repr(), PhpType::Object(class)
            if class.is_empty() || ctx.interfaces.contains_key(&class)) {
        if let PhpType::Object(class) = ty.codegen_repr() {
            let class = class.trim_start_matches('\\');
            if let Some(guard) = ctx.classes.get(class).and_then(|info|
                slot_guard(ctx, class, info, property, mode)) {
                apply_slot_guard(ctx, object, property, &guard, span, mode);
            }
        }
        return false;
    }
    let mut classes: Vec<_> = ctx.classes.iter().filter_map(|(name, info)| {
        if concrete_class.as_ref().is_some_and(|base|
            name != base && !crate::types::class_inherits_from(ctx.classes, name, base)) {
            return None;
        }
        if include_dynamic && (dynamic_slot(info, name, property)
            || magic_append::is_candidate(ctx, name, info, property)) {
            return Some((name.clone(), None, true));
        }
        let (null_error, writable) = declared_slot_candidate(ctx, name, info, property, mode)?;
        if guard_only && null_error.is_none() { return None; }
        Some((name.clone(), null_error, writable))
    }).collect();
    classes.sort_by(|a, b| a.0.cmp(&b.0));
    if classes.is_empty() { return false; }

    let name = ctx.declare_owned_hidden_temp(PhpType::Str);
    let name_slot = ctx.local_slots[&name];
    crate::ir_lower::expr::register_owned_call_operand(ctx, name_slot, span);
    let ordinary = ctx.builder.create_named_block("property.array.runtime_class", vec![]);
    let missing = ctx.builder.create_named_block("property.array.runtime_fallback", vec![]);
    let done = ctx.builder.create_named_block("property.array.runtime_done", vec![]);
    let initial = ctx.initialized_slots_snapshot();
    let is_object = ctx.emit_value(Op::TypePredicate, vec![object.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Object)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(span));
    ctx.builder.terminate(Terminator::CondBr { cond: is_object.value,
        then_target: ordinary, then_args: vec![], else_target: missing, else_args: vec![] });
    ctx.builder.position_at_end(ordinary);
    let target = crate::ir::RuntimeFnId::GetClass;
    let class = ctx.emit_value(Op::RuntimeCall, vec![object.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(target))),
        PhpType::Str, target.effects(), Some(span));
    crate::ir_lower::expr::store_value_into_temp(ctx, &name, PhpType::Str, class, span);
    let mut merged = initial.clone();
    for (class, null_error, writable) in classes {
        let matched = ctx.builder.create_named_block("property.array.declared", vec![]);
        let next = ctx.builder.create_named_block("property.array.next_class", vec![]);
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
        if let Some(guard) = null_error {
            apply_slot_guard(ctx, receiver, property, &guard, span, mode);
        }
        apply(ctx, if writable { receiver } else { object }, writable);
        if !ctx.builder.insertion_block_is_terminated() {
            merged = merged.intersection(&ctx.initialized_slots_snapshot()).copied().collect();
            ctx.builder.terminate(Terminator::Br { target: done, args: vec![] });
        }
        ctx.builder.position_at_end(next);
        ctx.restore_initialized_slots(before);
    }
    ctx.builder.terminate(Terminator::Br { target: missing, args: vec![] });
    ctx.builder.position_at_end(missing);
    ctx.restore_initialized_slots(initial);
    apply(ctx, object, false);
    merged = merged.intersection(&ctx.initialized_slots_snapshot()).copied().collect();
    ctx.builder.terminate(Terminator::Br { target: done, args: vec![] });
    ctx.builder.position_at_end(done);
    ctx.restore_initialized_slots(merged);
    crate::ir_lower::expr::retire_owned_call_operand(ctx, name_slot, span);
    true
}

/// Recognizes declarations whose null value may become a PHP array during an element write.
fn admits_array(ty: &PhpType) -> bool {
    match ty {
        PhpType::Mixed | PhpType::Iterable | PhpType::Array(_) | PhpType::AssocArray { .. } => true,
        PhpType::Union(members) => members.iter().any(admits_array),
        _ => false,
    }
}

/// Chooses the declaration and access diagnostic before a null typed slot could be changed.
fn null_write_error(
    ctx: &LoweringContext<'_, '_>, class: &str, info: &crate::types::ClassInfo,
    property: &str, declared: &PhpType,
) -> (&'static str, String) {
    let declaring = info.property_declaring_classes.get(property).map(String::as_str)
        .unwrap_or(class).trim_start_matches('\\');
    if info.readonly_properties.contains(property) {
        return ("Error", format!("Cannot indirectly modify readonly property {declaring}::${property}"));
    }
    if let Some(visibility) = info.property_set_visibilities.get(property) {
        if !crate::ir_lower::expr::ir_can_access_member(ctx, declaring, visibility) {
            let visibility = match visibility {
                crate::parser::ast::Visibility::Public => "public",
                crate::parser::ast::Visibility::Protected => "protected",
                crate::parser::ast::Visibility::Private => "private",
            };
            let scope = ctx.current_class.as_deref().map(|class|
                format!("scope {}", class.trim_start_matches('\\')))
                .unwrap_or_else(|| "global scope".to_string());
            return ("Error", format!("Cannot indirectly modify {visibility}(set) property {declaring}::${property} from {scope}"));
        }
    }
    let ty = crate::types::diagnostic_names::php_type_name(declared);
    ("TypeError", format!("Cannot auto-initialize an array inside property {declaring}::${property} of type {ty}"))
}

/// Tests an independent property-read lease and retires it before either successor executes.
fn apply_slot_guard(
    ctx: &mut LoweringContext<'_, '_>, receiver: LoweredValue, property: &str,
    guard: &SlotGuard, span: Span, mode: FetchMode,
) {
    let SlotAction::Reject { exception, message } = &guard.action else {
        initialize_array_slot(ctx, receiver, property, span);
        return;
    };
    let data = ctx.intern_string(property);
    let reject = ctx.builder.create_named_block("property.array.null_reject", vec![]);
    let write = ctx.builder.create_named_block("property.array.nonnull_write", vec![]);
    let initialized_slots = ctx.initialized_slots_snapshot();
    if mode == FetchMode::Write {
        // The accepted arm may read the slot before any fetch for write (an `ArrayAccess`
        // object is read, then called), so a null receiver is rejected here, as PHP does.
        reject_null_receiver(ctx, receiver, property, span);
        let initialized = ctx.emit_value(Op::PropInitialized, vec![receiver.value],
            Some(Immediate::Data(data)), PhpType::Bool, Op::PropInitialized.default_effects(), Some(span));
        let read = if guard.nullable || guard.object_only {
            ctx.builder.create_named_block("property.array.initialized_read", vec![])
        } else { write };
        ctx.builder.terminate(Terminator::CondBr { cond: initialized.value,
            then_target: read, then_args: vec![], else_target: reject, else_args: vec![] });
        ctx.builder.position_at_end(read);
    }
    if guard.nullable || guard.object_only {
        let current = ctx.emit_owned_value(Op::PropGet, vec![receiver.value], Some(Immediate::Data(data)),
            PhpType::Mixed, Op::PropGet.default_effects(), Some(span));
        // Readonly and inaccessible-set properties permit child object mutation only.
        let (condition, then_target, else_target) = if guard.object_only {
            let object = ctx.emit_value(Op::TypePredicate, vec![current.value],
                Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Object)),
                PhpType::Bool, Op::TypePredicate.default_effects(), Some(span));
            (object, write, reject)
        } else {
            let null = ctx.emit_value(Op::IsNull, vec![current.value], None, PhpType::Bool,
                Op::IsNull.default_effects(), Some(span));
            (null, reject, write)
        };
        crate::ir_lower::ownership::release_if_owned(ctx, current, Some(span));
        ctx.builder.terminate(Terminator::CondBr { cond: condition.value,
            then_target, then_args: vec![], else_target, else_args: vec![] });
    }
    ctx.builder.position_at_end(reject);
    lower_throw_access_exception(ctx, exception, message, span);
    ctx.builder.position_at_end(write);
    ctx.restore_initialized_slots(initialized_slots);
}

/// Throws PHP's `Attempt to modify property "p" on null` when `receiver` is the null sentinel.
///
/// PHP fetches the property of an element write, a compound update or an increment for write
/// (`FETCH_OBJ_W` / `FETCH_OBJ_RW`) and rejects a null receiver there, before any property
/// read, `offsetGet` call or readonly check. Paths that read the property first call this once
/// their operands are evaluated. `$this` and receivers that cannot be the sentinel get no check.
pub(super) fn reject_null_receiver(
    ctx: &mut LoweringContext<'_, '_>, receiver: LoweredValue, property: &str, span: Span,
) {
    if !receiver_may_be_null_sentinel(ctx, receiver) { return; }
    let slots = ctx.initialized_slots_snapshot();
    let null_receiver = ctx.builder.create_named_block("property.array.null_receiver", vec![]);
    skip_guard_for_null_receiver(ctx, receiver, null_receiver, "property.array.receiver_checked", span);
    let checked = ctx.builder.insertion_block();
    ctx.builder.position_at_end(null_receiver);
    lower_throw_access_exception(ctx, "Error",
        &format!("Attempt to modify property \"{property}\" on null"), span);
    if let Some(checked) = checked { ctx.builder.position_at_end(checked); }
    ctx.restore_initialized_slots(slots);
}

/// Rejects a null receiver read only to be tested, releasing that read before either successor.
///
/// An update whose receiver is an element (`$x = $nodes[5]->list[0]++`) replays the element
/// read for its own read and write; this extra read exists for the null test alone, so its
/// owner is retired as soon as the test is computed.
pub(super) fn reject_null_receiver_probe(
    ctx: &mut LoweringContext<'_, '_>, probe: LoweredValue, property: &str, span: Span,
) {
    if !receiver_may_be_null_sentinel(ctx, probe) {
        crate::ir_lower::ownership::release_if_owned(ctx, probe, Some(span));
        return;
    }
    let slots = ctx.initialized_slots_snapshot();
    let null = ctx.emit_value(Op::IsNull, vec![probe.value], None, PhpType::Bool,
        Op::IsNull.default_effects(), Some(span));
    crate::ir_lower::ownership::release_if_owned(ctx, probe, Some(span));
    let null_receiver = ctx.builder.create_named_block("property.array.null_receiver", vec![]);
    let checked = ctx.builder.create_named_block("property.array.receiver_checked", vec![]);
    ctx.builder.terminate(Terminator::CondBr { cond: null.value,
        then_target: null_receiver, then_args: vec![], else_target: checked, else_args: vec![] });
    ctx.builder.position_at_end(null_receiver);
    lower_throw_access_exception(ctx, "Error",
        &format!("Attempt to modify property \"{property}\" on null"), span);
    ctx.builder.position_at_end(checked);
    ctx.restore_initialized_slots(slots);
}

/// Sends a null receiver straight to `fetch`, past the initialization guard of a write fetch.
///
/// A class-typed receiver can be the null sentinel of a missed element read
/// (`$nodes[5]->items[] = 1`). The initialization probe answers false for it, which would send
/// the write into the initializer or the guard's rejection, and report those instead of PHP's
/// `Attempt to modify property ... on null`. That diagnostic belongs to the fetch-for-write
/// that follows the guard, so a null receiver branches there directly. `$this` is never null
/// and gets no check.
fn skip_guard_for_null_receiver(
    ctx: &mut LoweringContext<'_, '_>, receiver: LoweredValue, fetch: crate::ir::BlockId,
    probe_block: &str, span: Span,
) {
    if !receiver_may_be_null_sentinel(ctx, receiver) { return; }
    let null = ctx.emit_value(Op::IsNull, vec![receiver.value], None, PhpType::Bool,
        Op::IsNull.default_effects(), Some(span));
    let probe = ctx.builder.create_named_block(probe_block, vec![]);
    ctx.builder.terminate(Terminator::CondBr { cond: null.value,
        then_target: fetch, then_args: vec![], else_target: probe, else_args: vec![] });
    ctx.builder.position_at_end(probe);
}

/// Returns whether a write receiver is a concrete class value other than a load of `$this`.
///
/// Interface-typed and classless object values travel boxed, where null is not the sentinel.
fn receiver_may_be_null_sentinel(ctx: &LoweringContext<'_, '_>, receiver: LoweredValue) -> bool {
    let PhpType::Object(class) = ctx.builder.value_php_type(receiver.value).codegen_repr() else {
        return false;
    };
    if class.is_empty() || ctx.interfaces.contains_key(&class) { return false; }
    let this_slot = ctx.local_slots.get("this").copied();
    let mut value = receiver.value;
    loop {
        let Some(inst) = ctx.builder.value_defining_instruction(value) else { return true; };
        match inst.op {
            Op::Acquire | Op::Borrow | Op::Move => {
                let Some(source) = inst.operands.first().copied() else { return true; };
                value = source;
            }
            Op::LoadLocal => {
                return !matches!(inst.immediate,
                    Some(Immediate::LocalSlot(slot)) if Some(slot) == this_slot);
            }
            _ => return true,
        }
    }
}

/// Initializes legal array-bearing storage only for a W fetch, preserving ordinary RW reads.
fn initialize_array_slot(
    ctx: &mut LoweringContext<'_, '_>, receiver: LoweredValue, property: &str, span: Span,
) {
    let data = ctx.intern_string(property);
    let initialize = ctx.builder.create_named_block("property.array.initialize", vec![]);
    let done = ctx.builder.create_named_block("property.array.initialized", vec![]);
    skip_guard_for_null_receiver(ctx, receiver, done, "property.array.receiver_checked", span);
    let initialized = ctx.emit_value(Op::PropInitialized, vec![receiver.value],
        Some(Immediate::Data(data)), PhpType::Bool, Op::PropInitialized.default_effects(), Some(span));
    let slots = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr { cond: initialized.value,
        then_target: done, then_args: vec![], else_target: initialize, else_args: vec![] });
    ctx.builder.position_at_end(initialize);
    let source = Expr::new(ExprKind::ArrayLiteral(Vec::new()), span);
    let array = lower_expr(ctx, &source);
    super::super::instance_property_writes::lower_property_assign_without_native(
        ctx, receiver, property, &source, array, false, span,
    );
    ctx.builder.terminate(Terminator::Br { target: done, args: vec![] });
    ctx.builder.position_at_end(done);
    ctx.restore_initialized_slots(slots);
}

/// Keeps the existing runtime property writer for receivers without an ordinary declared slot.
pub(super) fn fallback(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str,
    index: LoweredValue, value: LoweredValue, span: Span,
) {
    let mut roots = Vec::new();
    let object = box_opaque_receiver(ctx, object, span, &mut roots);
    let data = ctx.intern_string(property);
    ctx.emit_void(Op::RuntimeCall, vec![object.value, index.value, value.value],
        Some(Immediate::Data(data)), effects_lookup::runtime_effects(), Some(span));
    retire_property_write_roots(ctx, roots, span);
}

/// Boxes erased object handles once and roots the box across the ordinary runtime fallback.
pub(super) fn box_opaque_receiver(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, span: Span,
    roots: &mut Vec<LocalSlotId>,
) -> LoweredValue {
    let ty = ctx.builder.value_php_type(object.value);
    if matches!(&ty, PhpType::Object(class)
        if class.is_empty() || ctx.interfaces.contains_key(class)) {
        let borrowed = ctx.emit_value(Op::Borrow, vec![object.value], None, ty,
            Op::Borrow.default_effects(), Some(span));
        ctx.builder.set_value_ownership(borrowed.value, Ownership::Borrowed);
        let boxed = ctx.box_value_as_mixed(borrowed, PhpType::Mixed, Some(span));
        root_property_write_value(ctx, boxed, span, roots)
    } else { object }
}
