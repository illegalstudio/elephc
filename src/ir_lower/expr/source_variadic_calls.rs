//! Purpose:
//! Selects source variadic method signatures before opaque receivers lose collector planning.
//!
//! Called from:
//! - Ordinary and nullsafe method lowering before source argument evaluation.
//!
//! Key details:
//! - Exact class checks preserve eval subclass overrides and unknown-class fallback dispatch.
//! - Selected calls reuse the shared argument planner, reference places and normal method ABI.
//! - The common receiver and merged result remain rooted across argument effects and cleanup.

use super::*;
use crate::ir::{PhpTypePredicate, RuntimeCallTarget, RuntimeFnId};

/// Classes sharing one implementation and signature can use the same argument-lowering arm.
struct Candidate {
    implementation: String,
    classes: Vec<String>,
    signature: FunctionSig,
}

/// Plans ordinary result calls whose opaque receiver could select a source variadic method.
pub(super) fn lower(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, method: &str,
    args: &[Expr], op: Op, expr: &Expr,
) -> Option<LoweredValue> {
    if !matches!(op, Op::MethodCall | Op::NullsafeMethodCall)
        || !dynamic_method_receiver_needs_mixed_fallback(&ctx.builder.value_php_type(object.value))
        || method_signature(ctx, object.value, method).is_some()
        || ctx.reference_call_context.as_ref().is_some_and(|context| context.depth == ctx.expression_depth)
    { return None; }
    let candidates = candidates(ctx, method);
    if candidates.is_empty() { return None; }
    Some(if op == Op::NullsafeMethodCall {
        lower_guarded_nullsafe_receiver(ctx, object, PhpType::Mixed, expr, |ctx, object|
            dispatch(ctx, object, method, args, &candidates, expr))
    } else { dispatch(ctx, object, method, args, &candidates, expr) })
}

/// Includes accessible native classes while leaving different or eval-only signatures untouched.
fn candidates(ctx: &LoweringContext<'_, '_>, method: &str) -> Vec<Candidate> {
    let key = php_symbol_key(method);
    let mut names = ctx.classes.keys().cloned().collect::<Vec<_>>();
    names.sort();
    let mut groups: Vec<Candidate> = Vec::new();
    for name in names {
        let info = &ctx.classes[&name];
        let Some(signature) = class_method_signature(ctx, &name, &key) else { continue; };
        if signature.variadic.is_none() || crate::func_args::sig_collects_surplus_args(signature) {
            continue;
        }
        let declaring = info.method_declaring_classes.get(&key).map(String::as_str).unwrap_or(&name);
        if info.method_visibilities.get(&key).is_some_and(|visibility|
            !ir_can_access_member(ctx, declaring, visibility)) { continue; }
        let implementation = info.method_impl_classes.get(&key)
            .filter(|implementation| ctx.classes.contains_key(*implementation))
            .cloned().unwrap_or_else(|| name.clone());
        if let Some(group) = groups.iter_mut().find(|group|
            group.implementation == implementation && group.signature == *signature)
        {
            group.classes.push(name);
        } else {
            groups.push(Candidate { implementation, classes: vec![name], signature: signature.clone() });
        }
    }
    groups
}

/// Publishes empty result and class-name slots before the receiver or selected call can throw.
fn published_slot(ctx: &mut LoweringContext<'_, '_>, ty: PhpType, span: Span) -> String {
    let name = ctx.declare_owned_hidden_temp(ty);
    let slot = ctx.local_slots[&name];
    ctx.emit_void(Op::ZeroLocalSlot, vec![], Some(Immediate::LocalSlot(slot)),
        Op::ZeroLocalSlot.default_effects(), Some(span));
    register_owned_call_operand(ctx, slot, span);
    name
}

/// Lowers only the runtime-selected argument plan and joins its independently owned result.
fn dispatch(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, method: &str,
    args: &[Expr], candidates: &[Candidate], expr: &Expr,
) -> LoweredValue {
    let span = expr.span;
    let result = published_slot(ctx, PhpType::Mixed, span);
    let (object, owner) = root_method_receiver(ctx, object, span);
    let class_name = published_slot(ctx, PhpType::Str, span);
    let initial = ctx.initialized_slots_snapshot();
    let select = ctx.builder.create_named_block("method.variadic.select", vec![]);
    let fallback = ctx.builder.create_named_block("method.variadic.fallback", vec![]);
    let done = ctx.builder.create_named_block("method.variadic.done", vec![]);
    let is_object = ctx.emit_value(Op::TypePredicate, vec![object.value],
        Some(Immediate::TypePredicate(PhpTypePredicate::Object)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(span));
    ctx.builder.terminate(Terminator::CondBr { cond: is_object.value,
        then_target: select, then_args: vec![], else_target: fallback, else_args: vec![] });
    ctx.builder.position_at_end(select);
    let target = RuntimeFnId::GetClass;
    let name = ctx.emit_value(Op::RuntimeCall, vec![object.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(target))),
        PhpType::Str, target.effects(), Some(span));
    store_value_into_temp(ctx, &class_name, PhpType::Str, name, span);
    let mut merged: Option<HashSet<LocalSlotId>> = None;
    for candidate in candidates {
        let selected = ctx.builder.create_named_block("method.variadic.selected", vec![]);
        let next_group = ctx.builder.create_named_block("method.variadic.next_group", vec![]);
        for (index, class) in candidate.classes.iter().enumerate() {
            let next = if index + 1 == candidate.classes.len() { next_group }
                else { ctx.builder.create_named_block("method.variadic.next", vec![]) };
            let name = ctx.load_local(&class_name, Some(span));
            ctx.builder.set_value_ownership(name.value, Ownership::Borrowed);
            let data = ctx.intern_string(class.trim_start_matches('\\'));
            let expected = ctx.emit_value(Op::ConstStr, vec![], Some(Immediate::Data(data)), PhpType::Str,
                Op::ConstStr.default_effects(), Some(span));
            let condition = ctx.emit_value(Op::StrictEq, vec![name.value, expected.value], None,
                PhpType::Bool, Op::StrictEq.default_effects(), Some(span));
            ctx.builder.terminate(Terminator::CondBr { cond: condition.value,
                then_target: selected, then_args: vec![], else_target: next, else_args: vec![] });
            ctx.builder.position_at_end(next);
        }
        let next = next_group;
        let before = ctx.initialized_slots_snapshot();
        ctx.builder.position_at_end(selected);
        if !super::selected_method_arguments::reject_invalid(
            ctx, &candidate.implementation, method, &candidate.signature, args, span,
        ) {
            let ty = PhpType::Object(candidate.implementation.clone());
            let receiver = if matches!(ctx.builder.value_php_type(object.value).codegen_repr(), PhpType::Object(_)) {
                let receiver = ctx.emit_value(Op::Borrow, vec![object.value], None, ty,
                    Op::Borrow.default_effects(), Some(span));
                ctx.builder.set_value_ownership(receiver.value, Ownership::Borrowed);
                receiver
            } else {
                ctx.emit_owned_value(Op::MixedUnbox, vec![object.value], None, ty.clone(),
                    Op::mixed_unbox_effects(&ty), Some(span))
            };
            let call = super::nullable_method_calls::lower_method_call_with_receiver_direct(
                ctx, receiver, method, args, Op::MethodCall, expr,
            );
            store_value_into_temp(ctx, &result, PhpType::Mixed, call, span);
            branch_to(ctx, done);
            let slots = ctx.initialized_slots_snapshot();
            merged = Some(match merged {
                Some(previous) => previous.intersection(&slots).copied().collect(), None => slots,
            });
        }
        ctx.builder.position_at_end(next);
        ctx.restore_initialized_slots(before);
    }
    branch_to(ctx, fallback);
    ctx.builder.position_at_end(fallback);
    ctx.restore_initialized_slots(initial);
    let receiver = ctx.emit_value(Op::Borrow, vec![object.value], None,
        ctx.builder.value_php_type(object.value), Op::Borrow.default_effects(), Some(span));
    ctx.builder.set_value_ownership(receiver.value, Ownership::Borrowed);
    let call = super::nullable_method_calls::lower_method_call_with_receiver_direct(
        ctx, receiver, method, args, Op::MethodCall, expr,
    );
    store_value_into_temp(ctx, &result, PhpType::Mixed, call, span);
    branch_to(ctx, done);
    let slots = ctx.initialized_slots_snapshot();
    let merged = merged.map_or_else(|| slots.clone(), |previous|
        previous.intersection(&slots).copied().collect());
    ctx.builder.position_at_end(done);
    ctx.restore_initialized_slots(merged);
    retire_owned_call_operand(ctx, ctx.local_slots[&class_name], span);
    if let Some(owner) = owner { retire_owned_call_operand(ctx, owner, span); }
    unregister_owned_call_operand(ctx, ctx.local_slots[&result], span);
    take_owned_temp(ctx, &result, span)
}
