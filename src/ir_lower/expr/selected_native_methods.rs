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
    // An interface that declares the method settles it the same way: every implementor has it, so
    // its `__call` cannot answer, and the interface table dispatches the call.
    if singular_object_class(&object_ty).is_some_and(|(class, _)| !class.is_empty()
        && (class_method_signature(ctx, class, &php_symbol_key(method)).is_some()
            || class_method_signature(ctx, class, "__call").is_some()
            || ctx.interfaces.get(class.trim_start_matches('\\'))
                .is_some_and(|interface| interface.methods.contains_key(&php_symbol_key(method)))))
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
    let scan = |key: &str| crate::ir_lower::class_scans::classes_with_method(ctx, key);
    let select_all = key == "__invoke"
        || scan("__call").iter().any(|name| !ctx.classes[name].methods.contains_key(&key))
        || scan(&key).iter().any(|name| ctx.classes[name].methods.get(&key).is_some_and(|sig|
            sig.variadic.is_none() && args.len() > sig.params.len()));
    if signature.is_some() && !select_all { return None; }
    let candidates = candidates(ctx, &object_ty, method, select_all, args.len());
    if candidates.is_empty() && key != "__invoke" { return None; }
    Some(if op == Op::NullsafeMethodCall {
        lower_guarded_nullsafe_receiver(ctx, object, PhpType::Mixed, expr, |ctx, object|
            dispatch(ctx, object, method, args, &candidates, expr))
    } else { dispatch(ctx, object, method, args, &candidates, expr) })
}

/// Returns whether a receiver of static type `ty` can be an instance of `class` at run time.
fn receiver_admits(ctx: &LoweringContext<'_, '_>, ty: &PhpType, class: &str) -> bool {
    match ty {
        PhpType::Object(bound) if !bound.is_empty() => {
            let bound = bound.trim_start_matches('\\');
            class.eq_ignore_ascii_case(bound)
                || super::call_arg_coercion::nominal_class_implements_or_extends(ctx, class, bound)
        }
        PhpType::Union(members) => members.iter().any(|member| receiver_admits(ctx, member, class)),
        PhpType::Mixed | PhpType::Object(_) => true,
        _ => false,
    }
}

/// Includes accessible native classes while leaving different or eval-only signatures untouched.
///
/// Every candidate lowers the whole argument list again, so a class is selected only when its own
/// argument plan differs from the shared dispatch: a variadic, a call with surplus arguments, or
/// the program's `__call` classes, which all share ONE arm. A Symfony program declares dozens of
/// `__call` classes; one arm each made three-line methods take seconds to lower.
fn candidates(
    ctx: &LoweringContext<'_, '_>, object_ty: &PhpType, method: &str, select_all: bool,
    arg_count: usize,
) -> Vec<Candidate> {
    let key = php_symbol_key(method);
    // Only a class that has the method, or `__call`, can answer; the calling class may also reach
    // a private method of its own that no table lists.
    let mut names = crate::ir_lower::class_scans::classes_with_method(ctx, &key);
    names.extend(crate::ir_lower::class_scans::classes_with_method(ctx, "__call"));
    names.extend(ctx.current_class.iter().filter(|class| ctx.classes.contains_key(*class)).cloned());
    names.sort_unstable();
    names.dedup();
    let mut groups: Vec<Candidate> = Vec::new();
    for name in names {
        let info = &ctx.classes[&name];
        // php hands a method the caller cannot access to `__call`, exactly like a missing one.
        let accessible = |key: &str| {
            let declaring = info.method_declaring_classes.get(key).map(String::as_str).unwrap_or(&name);
            info.method_visibilities.get(key).is_none_or(|visibility|
                ir_can_access_member(ctx, declaring, visibility))
        };
        // The compiler's own helpers are declared by native classes only: `__call` never sees one.
        let helper = method.starts_with("__elephc_");
        let magic = !helper && info.methods.contains_key("__call")
            && (!info.methods.contains_key(&key) || !accessible(&key));
        let selected_key = if magic { "__call" } else { key.as_str() };
        let Some(signature) = class_method_signature(ctx, &name, selected_key) else { continue; };
        let own_plan = (signature.variadic.is_some()
            && !crate::func_args::sig_collects_surplus_args(signature))
            || (select_all && (magic || helper || key == "__invoke"
                || (signature.variadic.is_none() && arg_count > signature.params.len())));
        if !own_plan || !receiver_admits(ctx, object_ty, &name) { continue; }
        if !accessible(selected_key) { continue; }
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
    let (magic, direct): (Vec<&Candidate>, Vec<&Candidate>) =
        candidates.iter().partition(|candidate| candidate.magic);
    // Named and unpacked arguments keep their own arm: `__call` receives their keys.
    let share_magic = !magic.is_empty() && !args.iter().any(|arg|
        matches!(arg.kind, ExprKind::NamedArg { .. } | ExprKind::Spread(_)));
    let shared = share_magic.then(|| Candidate {
        implementation: String::new(),
        classes: magic.iter().flat_map(|candidate| candidate.classes.iter().cloned()).collect(),
        signature: magic[0].signature.clone(),
        magic: true,
    });
    let arms = direct.into_iter()
        .chain(magic.iter().copied().filter(|_| !share_magic))
        .chain(shared.as_ref());
    for candidate in arms {
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
            let shared_magic = candidate.implementation.is_empty();
            let receiver = if shared_magic || matches!(ctx.builder.value_php_type(object.value).codegen_repr(), PhpType::Object(_)) {
                let ty = if shared_magic { ctx.builder.value_php_type(object.value) } else { ty };
                let receiver = ctx.emit_value(Op::Borrow, vec![object.value], None, ty,
                    Op::Borrow.default_effects(), Some(span));
                ctx.builder.set_value_ownership(receiver.value, Ownership::Borrowed);
                receiver
            } else {
                ctx.emit_owned_value(Op::MixedUnbox, vec![object.value], None, ty.clone(),
                    Op::mixed_unbox_effects(&ty), Some(span))
            };
            let call = if shared_magic {
                super::magic_method_calls::lower_shared(
                    ctx, receiver, method, args, &candidate.signature, expr)
            } else {
                super::nullable_method_calls::lower_method_call_with_receiver_direct(
                    ctx, receiver, method, args, Op::MethodCall, expr,
                )
            };
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
