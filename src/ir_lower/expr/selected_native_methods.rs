//! Purpose:
//! Selects native method signatures before opaque receivers lose argument planning.
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
    magic: bool,
}

/// Selects native argument plans for variadics, magic hooks, closures and surplus arguments.
pub(super) fn lower(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, method: &str,
    args: &[Expr], op: Op, expr: &Expr,
) -> Option<LoweredValue> {
    let object_ty = ctx.builder.value_php_type(object.value);
    // A nullable concrete receiver already has its own argument plan and dispatch target.
    // Unrelated classes with the same method name must not force a global class ladder.
    if singular_object_class(&object_ty).is_some_and(|(class, _)| !class.is_empty()
        && (class_method_signature(ctx, class, &php_symbol_key(method)).is_some()
            || class_method_signature(ctx, class, "__call").is_some()))
    { return None; }
    if !matches!(op, Op::MethodCall | Op::NullsafeMethodCall)
        || !(dynamic_method_receiver_needs_mixed_fallback(&object_ty)
            || singular_object_class(&object_ty).is_some_and(|(class, _)|
                class_method_signature(ctx, class, &php_symbol_key(method)).is_none()
                    && class_method_signature(ctx, class, "__call").is_none()))
        || ctx.reference_call_context.as_ref().is_some_and(|context| context.depth == ctx.expression_depth)
    { return None; }
    let key = php_symbol_key(method);
    let signature = method_signature(ctx, object.value, method);
    let select_all = key == "__invoke" || ctx.classes.values().any(|info|
        !info.methods.contains_key(&key) && info.methods.contains_key("__call"))
        || ctx.classes.values().any(|info| info.methods.get(&key).is_some_and(|sig|
            sig.variadic.is_none() && args.len() > sig.params.len()));
    if signature.is_some() && !select_all { return None; }
    let candidates = candidates(ctx, method, select_all);
    if candidates.is_empty() && key != "__invoke" { return None; }
    Some(if op == Op::NullsafeMethodCall {
        lower_guarded_nullsafe_receiver(ctx, object, PhpType::Mixed, expr, |ctx, object|
            dispatch(ctx, object, method, args, &candidates, expr))
    } else { dispatch(ctx, object, method, args, &candidates, expr) })
}

/// Includes accessible native classes while leaving different or eval-only signatures untouched.
fn candidates(ctx: &LoweringContext<'_, '_>, method: &str, select_all: bool) -> Vec<Candidate> {
    let key = php_symbol_key(method);
    let mut names = ctx.classes.keys().cloned().collect::<Vec<_>>();
    names.sort();
    let mut groups: Vec<Candidate> = Vec::new();
    for name in names {
        let info = &ctx.classes[&name];
        let magic = !info.methods.contains_key(&key) && info.methods.contains_key("__call");
        let selected_key = if magic { "__call" } else { key.as_str() };
        let Some(signature) = class_method_signature(ctx, &name, selected_key) else { continue; };
        if !select_all && (signature.variadic.is_none()
            || crate::func_args::sig_collects_surplus_args(signature)) {
            continue;
        }
        let declaring = info.method_declaring_classes.get(selected_key).map(String::as_str).unwrap_or(&name);
        if info.method_visibilities.get(selected_key).is_some_and(|visibility|
            !ir_can_access_member(ctx, declaring, visibility)) { continue; }
        let implementation = info.method_impl_classes.get(selected_key)
            .filter(|implementation| ctx.classes.contains_key(*implementation))
            .cloned().unwrap_or_else(|| name.clone());
        if let Some(group) = groups.iter_mut().find(|group|
            group.implementation == implementation && group.signature == *signature && group.magic == magic)
        {
            group.classes.push(name);
        } else {
            groups.push(Candidate { implementation, classes: vec![name], signature: signature.clone(), magic });
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
    emit_native_factory_method_lookup(ctx, object, method, span);
    let class_name = published_slot(ctx, PhpType::Str, span);
    let initial = ctx.initialized_slots_snapshot();
    let select = ctx.builder.create_named_block("method.selected.select", vec![]);
    let fallback = ctx.builder.create_named_block("method.selected.fallback", vec![]);
    let done = ctx.builder.create_named_block("method.selected.done", vec![]);
    let closure_branch = php_symbol_key(method) == "__invoke";
    if closure_branch {
        let closure = ctx.builder.create_named_block("method.closure.invoke", vec![]);
        let native = ctx.builder.create_named_block("method.closure.native", vec![]);
        let class = ctx.intern_class_name("Closure");
        let condition = ctx.emit_value(Op::InstanceOf, vec![object.value],
            Some(Immediate::Data(class)), PhpType::Bool, Op::InstanceOf.default_effects(), Some(span));
        ctx.builder.terminate(Terminator::CondBr { cond: condition.value,
            then_target: closure, then_args: vec![], else_target: native, else_args: vec![] });
        ctx.builder.position_at_end(closure);
        let receiver = ctx.emit_value(Op::Borrow, vec![object.value], None,
            ctx.builder.value_php_type(object.value), Op::Borrow.default_effects(), Some(span));
        ctx.builder.set_value_ownership(receiver.value, Ownership::Borrowed);
        let call = lower_expr_call_from_value(ctx, receiver, args, expr);
        store_value_into_temp(ctx, &result, PhpType::Mixed, call, span);
        branch_to(ctx, done);
        ctx.builder.position_at_end(native);
        ctx.restore_initialized_slots(initial.clone());
    }
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
    let mut merged: Option<HashSet<LocalSlotId>> = closure_branch.then(|| initial.clone());
    for candidate in candidates {
        let selected = ctx.builder.create_named_block("method.selected.selected", vec![]);
        let next_group = ctx.builder.create_named_block("method.selected.next_group", vec![]);
        for (index, class) in candidate.classes.iter().enumerate() {
            let next = if index + 1 == candidate.classes.len() { next_group }
                else { ctx.builder.create_named_block("method.selected.next", vec![]) };
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
        if candidate.magic || !super::selected_method_arguments::reject_invalid(
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
