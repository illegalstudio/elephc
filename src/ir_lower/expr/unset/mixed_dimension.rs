//! Purpose:
//! Selects native XML, public ArrayAccess, or array removal for erased receiver types.
//!
//! Called from:
//! - Direct unset lowering when XML capability is present and a receiver is boxed Mixed.
//!
//! Key details:
//! - Variable receivers are selected after key effects, matching PHP unset storage timing.
//! - Arrays retain copy-on-write; null, false, and invalid scalar offsets keep PHP diagnostics.

use super::*;

/// Recognizes boxed storage whose runtime value may carry an XML dimension adapter.
pub(super) fn supports(ctx: &LoweringContext<'_, '_>, array: &Expr) -> bool {
    ctx.classes.contains_key("SimpleXMLElement")
        && (!is_storage_receiver(ctx, array) || matches!(array.kind, ExprKind::ArrayAccess { .. }))
        && array_cast_source::array_cast_source_type_for_ir(ctx, array).codegen_repr() == PhpType::Mixed
}

/// Keeps storage expressions out of the value-only temporary array removal path.
fn is_storage_receiver(ctx: &LoweringContext<'_, '_>, mut receiver: &Expr) -> bool {
    let mut wrapped = false;
    while let ExprKind::ErrorSuppress(inner) = &receiver.kind { receiver = inner; wrapped = true; }
    if matches!(receiver.kind, ExprKind::Variable(_)) { return wrapped; }
    let signature = match &receiver.kind {
        ExprKind::FunctionCall { name, .. } =>
            call_signature(ctx, name, source_prefers_extension_builtin(name)),
        ExprKind::MethodCall { object, method, .. }
        | ExprKind::NullsafeMethodCall { object, method, .. } => {
            let ty = array_cast_source::array_cast_source_type_for_ir(ctx, object);
            crate::ir_lower::expr::method_metadata::method_signature_for_receiver_type(ctx, &ty, method)
        }
        ExprKind::StaticMethodCall { receiver, method, .. } =>
            static_method_implementation_signature(ctx, receiver, method)
                .or_else(|| lexical_instance_static_call_signature(ctx, receiver, method)).cloned(),
        _ => None,
    };
    if signature.is_some_and(|signature| signature.by_ref_return) { return true; }
    matches!(receiver.kind,
        ExprKind::ArrayAccess { .. } | ExprKind::PropertyAccess { .. }
        | ExprKind::NullsafePropertyAccess { .. } | ExprKind::DynamicPropertyAccess { .. }
        | ExprKind::StaticPropertyAccess { .. })
}

/// Captures the key before loading current storage and joins the three runtime receiver kinds.
pub(super) fn lower(
    ctx: &mut LoweringContext<'_, '_>, array: &Expr, index: &Expr, expr: &Expr,
) {
    // Resolve before receiver/argument/key effects can change the source's current type facts.
    let storage_receiver = is_storage_receiver(ctx, array);
    let detached_array_allowed = !storage_receiver && temporary_array_is_by_value(ctx, array);
    let mut traversal_owners = Vec::new();
    let staged_receiver = matches!(array.kind, ExprKind::ArrayAccess { .. }).then(||
        super::opaque_property_dimension::prepare_receiver(ctx, array, &mut traversal_owners));
    let name = if let ExprKind::Variable(name) = &array.kind { Some(name.as_str()) } else { None };
    // A computed receiver runs before its key; a variable selects its current storage afterwards.
    let early_receiver = (name.is_none() && staged_receiver.is_none())
        .then(|| capture_receiver(ctx, array, expr.span));
    let (key, key_temp) = evaluate_once(ctx, index, expr);
    let key_owner = key_temp.as_ref().filter(|name|
        Ownership::php_type_needs_lifetime_tracking(&ctx.local_type(name)))
        .map(|name| ctx.local_slots[name]);
    if let Some(slot) = key_owner { register_owned_call_operand(ctx, slot, expr.span); }
    let (object, owner) = if let Some(staged) = staged_receiver {
        let object = super::opaque_property_dimension::finish_receiver(ctx, &staged);
        root_receiver(ctx, object, expr.span)
    } else { early_receiver.unwrap_or_else(|| capture_receiver(ctx, array, expr.span)) };
    let borrowed = ctx.emit_value(Op::Borrow, vec![object.value], None, PhpType::Mixed,
        Op::Borrow.default_effects(), Some(expr.span));
    ctx.builder.set_value_ownership(borrowed.value, Ownership::Borrowed);
    let is_object = predicate(ctx, borrowed, crate::ir::PhpTypePredicate::Object, expr.span);
    let object_block = ctx.builder.create_named_block("unset.mixed.object", vec![]);
    let other_block = ctx.builder.create_named_block("unset.mixed.other", vec![]);
    let array_block = ctx.builder.create_named_block("unset.mixed.array", vec![]);
    let scalar_block = ctx.builder.create_named_block("unset.mixed.scalar", vec![]);
    let merge = ctx.builder.create_named_block("unset.mixed.merge", vec![]);
    let initial = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr { cond: is_object.value,
        then_target: object_block, then_args: vec![], else_target: other_block, else_args: vec![] });
    ctx.builder.position_at_end(object_block);
    native_dimension_probes::lower(ctx, borrowed, &PhpType::Mixed, &key,
        elephc_builtin_contract::ObjectDimension::Unset, expr);
    branch_to(ctx, merge);
    let object_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(other_block);
    ctx.restore_initialized_slots(initial.clone());
    let is_array = predicate(ctx, borrowed, crate::ir::PhpTypePredicate::Array, expr.span);
    ctx.builder.terminate(Terminator::CondBr { cond: is_array.value,
        then_target: array_block, then_args: vec![], else_target: scalar_block, else_args: vec![] });
    ctx.builder.position_at_end(scalar_block);
    lower_scalar(ctx, borrowed, expr.span);
    branch_to(ctx, merge);
    let scalar_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(array_block);
    ctx.restore_initialized_slots(initial.clone());
    if let Some(name) = name {
        if ctx.local_type(name).codegen_repr() == PhpType::Mixed {
            lower_unset_boxed_array_element(ctx, name, array.span, &key, expr);
        } else {
            // A key expression can assign a concrete array before current storage is loaded.
            lower_unset_array_access(ctx, array, &key, expr);
        }
    } else if detached_array_allowed {
        let detached = ctx.emit_owned_value(Op::MixedClone, vec![borrowed.value], None,
            PhpType::Mixed, Op::MixedClone.default_effects(), Some(expr.span));
        let (detached, detached_owner) = root_owned_call_operand(ctx, detached, expr.span);
        let key_value = lower_expr(ctx, &key);
        ctx.emit_void(Op::OffsetUnset, vec![detached.value, key_value.value], None,
            Op::OffsetUnset.default_effects(), Some(expr.span));
        if let Some(slot) = detached_owner { retire_owned_call_operand(ctx, slot, expr.span); }
    } else {
        let message = if storage_receiver {
            "Unsupported unset(): erased array storage has no attached removal path"
        } else {
            "Unsupported unset(): array result of an unresolved or overridable call has no attached removal path"
        };
        crate::ir_lower::stmt::lower_throw_access_error(ctx,
            message, expr.span);
    }
    branch_to(ctx, merge);
    let array_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(merge);
    let non_object_slots = merge_initialized_slots_for_expr(&initial,
        scalar_slots, true, array_slots, true);
    ctx.restore_initialized_slots(merge_initialized_slots_for_expr(&initial,
        object_slots, true, non_object_slots, true));
    if let Some(slot) = owner { retire_owned_call_operand(ctx, slot, expr.span); }
    if let Some(slot) = key_owner { unregister_owned_call_operand(ctx, slot, expr.span); }
    release_evaluated_once(ctx, key_temp, expr.span);
    for owner in traversal_owners.into_iter().rev() { retire_owned_call_operand(ctx, owner, expr.span); }
}

/// Proves that discarding a copied array cannot hide a reference-returning runtime implementation.
fn temporary_array_is_by_value(ctx: &LoweringContext<'_, '_>, mut source: &Expr) -> bool {
    while let ExprKind::ErrorSuppress(inner) = &source.kind { source = inner; }
    match &source.kind {
        ExprKind::FunctionCall { name, .. } =>
            call_signature(ctx, name, source_prefers_extension_builtin(name))
                .is_some_and(|signature| !signature.by_ref_return),
        ExprKind::MethodCall { object, method, .. }
        | ExprKind::NullsafeMethodCall { object, method, .. } => {
            let ty = array_cast_source::array_cast_source_type_for_ir(ctx, object);
            let Some((class, _)) = singular_object_class(&ty) else { return false; };
            let class = class.trim_start_matches('\\');
            let Some(info) = ctx.classes.get(class) else { return false; };
            let key = php_symbol_key(method);
            let Some(signature) = info.methods.get(&key) else { return false; };
            if signature.by_ref_return { return false; }
            if info.is_final || info.final_methods.contains(&key) { return true; }
            !ctx.has_eval_barrier() && ctx.classes.iter().all(|(candidate, info)|
                !class_extends_class(ctx, candidate, class)
                    || info.methods.get(&key).is_none_or(|signature| !signature.by_ref_return))
        }
        ExprKind::StaticMethodCall { receiver, method, .. } =>
            !matches!(receiver, StaticReceiver::Static)
                && static_method_implementation_signature(ctx, receiver, method)
                    .or_else(|| lexical_instance_static_call_signature(ctx, receiver, method))
                    .is_some_and(|signature| !signature.by_ref_return),
        ExprKind::ClosureCall { .. } | ExprKind::ExprCall { .. }
        | ExprKind::NullsafeDynamicMethodCall { .. } => false,
        _ => true,
    }
}

/// Roots computed owners before key effects can throw or replace their source storage.
fn capture_receiver(
    ctx: &mut LoweringContext<'_, '_>, array: &Expr, span: Span,
) -> (LoweredValue, Option<crate::ir::LocalSlotId>) {
    let object = lower_expr(ctx, array);
    root_receiver(ctx, object, span)
}

/// Gives the selected receiver one owned box after delayed storage traversal.
fn root_receiver(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, span: Span,
) -> (LoweredValue, Option<crate::ir::LocalSlotId>) {
    let object = if ctx.builder.value_php_type(object.value).codegen_repr() == PhpType::Mixed {
        object
    } else { ctx.box_value_as_mixed(object, PhpType::Mixed, Some(span)) };
    root_owned_call_operand(ctx, object, span)
}

/// Preserves null no-ops, false deprecations, and catchable errors without mutating scalar storage.
pub(super) fn lower_scalar(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Span) {
    let done = ctx.builder.create_named_block("unset.scalar.done", vec![]);
    let nonnull = ctx.builder.create_named_block("unset.scalar.nonnull", vec![]);
    let is_null = ctx.emit_value(Op::IsNull, vec![value.value], None, PhpType::Bool,
        Op::IsNull.default_effects(), Some(span));
    ctx.builder.terminate(Terminator::CondBr { cond: is_null.value,
        then_target: done, then_args: vec![], else_target: nonnull, else_args: vec![] });
    ctx.builder.position_at_end(nonnull);
    let boolean = predicate(ctx, value, crate::ir::PhpTypePredicate::Bool, span);
    let truth = ctx.emit_value(Op::IsTruthy, vec![value.value], None, PhpType::Bool,
        Op::IsTruthy.default_effects(), Some(span));
    let zero = ctx.builder.emit_const_i64(0);
    let falsy = ctx.emit_value(Op::ICmp, vec![truth.value, zero],
        Some(Immediate::CmpPredicate(CmpPredicate::Eq)), PhpType::Bool,
        Op::ICmp.default_effects(), Some(span));
    let is_false = ctx.emit_value(Op::IBitAnd, vec![boolean.value, falsy.value], None,
        PhpType::Bool, Op::IBitAnd.default_effects(), Some(span));
    let deprecated = ctx.builder.create_named_block("unset.scalar.false", vec![]);
    let invalid = ctx.builder.create_named_block("unset.scalar.invalid", vec![]);
    ctx.builder.terminate(Terminator::CondBr { cond: is_false.value,
        then_target: deprecated, then_args: vec![], else_target: invalid, else_args: vec![] });
    ctx.builder.position_at_end(deprecated);
    let message = lower_expr(ctx, &Expr::new(ExprKind::StringLiteral(
        "Deprecated: Automatic conversion of false to array is deprecated\n".into()), span));
    ctx.emit_void(Op::RuntimeCall, vec![message.value],
        Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::WriteDiagnostic)),
        effects_lookup::runtime_effects(), Some(span));
    branch_to(ctx, done);
    ctx.builder.position_at_end(invalid);
    let is_string = predicate(ctx, value, crate::ir::PhpTypePredicate::String, span);
    let string = ctx.builder.create_named_block("unset.scalar.string", vec![]);
    let other = ctx.builder.create_named_block("unset.scalar.other", vec![]);
    ctx.builder.terminate(Terminator::CondBr { cond: is_string.value,
        then_target: string, then_args: vec![], else_target: other, else_args: vec![] });
    ctx.builder.position_at_end(string);
    crate::ir_lower::stmt::lower_throw_access_error(ctx, "Cannot unset string offsets", span);
    ctx.builder.position_at_end(other);
    crate::ir_lower::stmt::lower_throw_access_error(ctx, "Cannot unset offset in a non-array variable", span);
    ctx.builder.position_at_end(done);
}

/// Tests a captured boxed value without consuming its independently rooted owner.
fn predicate(
    ctx: &mut LoweringContext<'_, '_>, value: LoweredValue,
    predicate: crate::ir::PhpTypePredicate, span: Span,
) -> LoweredValue {
    ctx.emit_value(Op::TypePredicate, vec![value.value], Some(Immediate::TypePredicate(predicate)),
        PhpType::Bool, Op::TypePredicate.default_effects(), Some(span))
}
