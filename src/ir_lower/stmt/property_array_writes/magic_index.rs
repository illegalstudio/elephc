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
        // An interface has no instances: `get_class()` never names it.
        if ctx.interfaces.contains_key(class) { return None; }
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
    // A program that loads no code at run time knows every class, so a class is its id: one load and
    // a compare per class, where `get_class()` builds a string that every arm then compares. The
    // name is only needed by the overloaded tail's notice, which asks for it there.
    let by_id = !ctx.program_loads_code_at_run_time();
    let class_id = if by_id {
        let generic = PhpType::Object(String::new());
        let unboxed = ctx.emit_value(Op::MixedUnbox, vec![object.value], None, generic.clone(),
            Op::mixed_unbox_effects(&generic), Some(span));
        ctx.builder.set_value_ownership(unboxed.value, Ownership::Borrowed);
        Some(ctx.emit_value(Op::ObjectClassId, vec![unboxed.value], None, PhpType::Int,
            Op::ObjectClassId.default_effects(), Some(span)))
    } else {
        let target = crate::ir::RuntimeFnId::GetClass;
        let class_name = ctx.emit_value(Op::RuntimeCall, vec![object.value],
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(target))), PhpType::Str,
            target.effects(), Some(span));
        crate::ir_lower::expr::store_value_into_temp(ctx, &name, PhpType::Str, class_name, span);
        None
    };
    let mut merged = None;
    // The overloaded and inaccessible reads address the object only through the runtime property
    // dispatcher, so every class of one of those kinds shares one tail; only the class-name test
    // (and the class name in the notice, taken from the selected name) is its own.
    let mut tails: [Option<BlockId>; 2] = [None, None];
    // With class ids the members of a shared tail are tested together, after the classes with an arm
    // of their own: a run of consecutive ids is two comparisons.
    let mut members: [Vec<i64>; 2] = [Vec::new(), Vec::new()];
    for (class, kind) in classes {
        let flavor = match kind { Kind::Magic { hash: false } => Some(0), Kind::Inaccessible => Some(1), _ => None };
        if let (Some(flavor), true) = (flavor, class_id.is_some()) {
            tails[flavor].get_or_insert_with(|| ctx.builder.create_named_block("property.index.shared", vec![]));
            members[flavor].push(ctx.classes.get(class.trim_start_matches('\\')).map_or(0, |info| info.class_id) as i64);
            continue;
        }
        let matched = ctx.builder.create_named_block("property.index.special", vec![]);
        let next = ctx.builder.create_named_block("property.index.next", vec![]);
        let condition = if let Some(class_id) = class_id {
            let wanted = ctx.classes.get(class.trim_start_matches('\\')).map_or(0, |info| info.class_id) as i64;
            let wanted = ctx.emit_value(Op::ConstI64, vec![], Some(Immediate::I64(wanted)), PhpType::Int,
                Op::ConstI64.default_effects(), Some(span));
            ctx.emit_value(Op::ICmp, vec![class_id.value, wanted.value],
                Some(Immediate::CmpPredicate(crate::ir::CmpPredicate::Eq)), PhpType::Bool,
                Op::ICmp.default_effects(), Some(span))
        } else {
            let current = ctx.load_local(&name, Some(span));
            ctx.builder.set_value_ownership(current.value, Ownership::Borrowed);
            let data = ctx.intern_string(class.trim_start_matches('\\'));
            let expected = ctx.emit_value(Op::ConstStr, vec![], Some(Immediate::Data(data)), PhpType::Str,
                Op::ConstStr.default_effects(), Some(span));
            ctx.emit_value(Op::StrictEq, vec![current.value, expected.value], None,
                PhpType::Bool, Op::StrictEq.default_effects(), Some(span))
        };
        let before = ctx.initialized_slots_snapshot();
        ctx.builder.terminate(Terminator::CondBr { cond: condition.value,
            then_target: matched, then_args: vec![], else_target: next, else_args: vec![] });
        ctx.builder.position_at_end(matched);
        if let Some(flavor) = flavor {
            let tail = *tails[flavor].get_or_insert_with(||
                ctx.builder.create_named_block("property.index.shared", vec![]));
            ctx.builder.terminate(Terminator::Br { target: tail, args: vec![] });
            ctx.builder.position_at_end(next);
            ctx.restore_initialized_slots(before);
            continue;
        }
        let concrete = PhpType::Object(class.clone());
        let receiver = ctx.emit_value(Op::MixedUnbox, vec![object.value], None, concrete.clone(),
            Op::mixed_unbox_effects(&concrete), Some(span));
        ctx.builder.set_value_ownership(receiver.value, Ownership::Borrowed);
        write(ctx, object, receiver, &class, property, kind, index, value, compound, span, diagnosed);
        merge(ctx, done, &mut merged);
        ctx.builder.position_at_end(next);
        ctx.restore_initialized_slots(before);
    }
    let mut tests: Vec<(i64, i64, BlockId)> = Vec::new();
    for (flavor, ids) in members.iter_mut().enumerate() {
        ids.sort_unstable();
        for &id in ids.iter() {
            match tests.last_mut() {
                Some((_, last, tail)) if *last + 1 == id && Some(*tail) == tails[flavor] => *last = id,
                _ => tests.push((id, id, tails[flavor].expect("a member has its tail"))),
            }
        }
    }
    let count = tests.len();
    for (at, (first, last, tail)) in tests.into_iter().enumerate() {
        let miss = if at + 1 == count { regular } else {
            ctx.builder.create_named_block("property.index.member", vec![])
        };
        let id = class_id.expect("members are tested by class id").value;
        let mut compare = |ctx: &mut LoweringContext<'_, '_>, bound: i64, predicate, then_target, else_target| {
            let bound = ctx.emit_value(Op::ConstI64, vec![], Some(Immediate::I64(bound)), PhpType::Int,
                Op::ConstI64.default_effects(), Some(span));
            let hit = ctx.emit_value(Op::ICmp, vec![id, bound.value], Some(Immediate::CmpPredicate(predicate)),
                PhpType::Bool, Op::ICmp.default_effects(), Some(span));
            ctx.builder.terminate(Terminator::CondBr { cond: hit.value,
                then_target, then_args: vec![], else_target, else_args: vec![] });
        };
        if first == last {
            compare(ctx, first, crate::ir::CmpPredicate::Eq, tail, miss);
        } else {
            let upper = ctx.builder.create_named_block("property.index.member_upper", vec![]);
            compare(ctx, first, crate::ir::CmpPredicate::Sge, upper, miss);
            ctx.builder.position_at_end(upper);
            compare(ctx, last, crate::ir::CmpPredicate::Sle, tail, miss);
        }
        if miss != regular { ctx.builder.position_at_end(miss); }
    }
    if count == 0 { ctx.builder.terminate(Terminator::Br { target: regular, args: vec![] }); }
    let before_tails = initial.clone();
    for (flavor, tail) in tails.into_iter().enumerate() {
        let Some(tail) = tail else { continue };
        ctx.builder.position_at_end(tail);
        ctx.restore_initialized_slots(before_tails.clone());
        if by_id && flavor == 0 {
            let target = crate::ir::RuntimeFnId::GetClass;
            let class_name = ctx.emit_value(Op::RuntimeCall, vec![object.value],
                Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(target))), PhpType::Str,
                target.effects(), Some(span));
            crate::ir_lower::expr::store_value_into_temp(ctx, &name, PhpType::Str, class_name, span);
        }
        let notice = if flavor == 0 {
            let current = ctx.load_local(&name, Some(span));
            ctx.builder.set_value_ownership(current.value, Ownership::Borrowed);
            let prefix = string(ctx, "Notice: Indirect modification of overloaded property ", span);
            let head = ctx.emit_value(Op::StrConcat, vec![prefix.value, current.value], None, PhpType::Str,
                Op::StrConcat.default_effects(), Some(span));
            let suffix = string(ctx, &format!("::${property} has no effect\n"), span);
            Some(ctx.emit_value(Op::StrConcat, vec![head.value, suffix.value], None, PhpType::Str,
                Op::StrConcat.default_effects(), Some(span)))
        } else { None };
        let name_value = string(ctx, property, span);
        fetched(ctx, object, name_value, notice, flavor == 0, index, value, compound, span, diagnosed);
        merge(ctx, done, &mut merged);
    }
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
    // A parent's lexical scope keeps its private slot on every descendant. Its
    // physical metadata must not turn that slot into the child's dynamic hash.
    if matches!(crate::types::resolve_property_name(ctx.classes, class, property, ctx.current_class.as_deref()),
        crate::types::PropertyNameResolution::Visible | crate::types::PropertyNameResolution::ScopePrivate { .. }) {
        return None;
    }
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
        let notice = string(ctx, &format!("Notice: Indirect modification of overloaded property {class}::${property} has no effect\n"), span);
        fetched(ctx, boxed, name, Some(notice), true, index, value, compound, span, diagnosed);
        merge(ctx, done, &mut merged);
        ctx.builder.position_at_end(done);
        if let Some(merged) = merged { ctx.restore_initialized_slots(merged); }
    } else {
        let notice = matches!(kind, Kind::Magic { .. }).then(|| string(ctx, &format!(
            "Notice: Indirect modification of overloaded property {class}::${property} has no effect\n"), span));
        fetched(ctx, boxed, name, notice, matches!(kind, Kind::Magic { .. }),
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
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, name: LoweredValue, notice: Option<LoweredValue>,
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
        let notice = notice.expect("an overloaded read carries its notice");
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
fn merge(ctx: &mut LoweringContext<'_, '_>, done: BlockId, merged: &mut Option<crate::fast_hash::FastSet<LocalSlotId>>) {
    if ctx.builder.insertion_block_is_terminated() { return; }
    let state = ctx.initialized_slots_snapshot();
    *merged = Some(match merged.take() {
        Some(previous) => previous.intersection(&state).copied().collect(), None => state,
    });
    ctx.builder.terminate(Terminator::Br { target: done, args: vec![] });
}
