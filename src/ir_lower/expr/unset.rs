//! Purpose:
//! Direct unset lowering for locals, arrays, properties, and magic methods.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers supported `unset(...)` targets without evaluating them as ordinary call args.
pub(super) fn lower_unset_locals(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if !args.iter().all(|arg| unset_target_supported(ctx, arg)) {
        return None;
    }
    let null = lower_null(ctx, expr);
    for arg in args {
        match &arg.kind {
            ExprKind::Variable(name) => {
                ctx.unset_local(name, null, Some(arg.span));
            }
            ExprKind::ArrayAccess { array, index } => {
                lower_unset_array_access(ctx, array, index, arg);
            }
            ExprKind::PropertyAccess { object, property }
            | ExprKind::NullsafePropertyAccess { object, property } => {
                lower_unset_property_access(ctx, object, property, arg);
            }
            ExprKind::DynamicPropertyAccess { object, property } => {
                lower_unset_dynamic_property_access(ctx, object, property, arg);
            }
            _ => {}
        }
    }
    crate::ir_lower::ownership::collect_cycles(ctx, Some(expr.span));
    Some(null)
}

/// Returns true when an `unset(...)` target has direct EIR lowering.
pub(super) fn unset_target_supported(ctx: &LoweringContext<'_, '_>, arg: &Expr) -> bool {
    match &arg.kind {
        ExprKind::Variable(_) => true,
        ExprKind::ArrayAccess { array, .. } => {
            unset_array_access_has_object_receiver(ctx, array)
                || unset_array_access_has_local_array_receiver(ctx, array)
        }
        ExprKind::PropertyAccess { object, property }
        | ExprKind::NullsafePropertyAccess { object, property } => {
            unset_property_access_has_direct_lowering(ctx, object, property)
        }
        // `unset($o->{$k})` lowers to `Op::DynamicPropUnset`, whose backend ladder compares the
        // runtime name against the receiver's declared names and then takes php's answer for the
        // matched name on the receiver's runtime class. The name has to be a string: an integer
        // or a boxed value is the general runtime-name capability, not this one.
        ExprKind::DynamicPropertyAccess { object, property } => {
            unset_dynamic_property_access_has_direct_lowering(ctx, object, property)
        }
        _ => false,
    }
}

/// Returns true when an array-access unset receiver is a plain array/hash local whose element the
/// EIR backend can remove.
///
/// Associative arrays remove the element directly; packed indexed arrays are converted to a hash at
/// the unset site (PHP `unset()` leaves a sparse array). Declared PHP arrays keep a boxed
/// packed-or-hash representation, so their reference aliases can observe sparse mutation too.
/// Raw by-reference arrays still cannot change their caller's storage representation.
pub(super) fn unset_array_access_has_local_array_receiver(
    ctx: &LoweringContext<'_, '_>,
    array: &Expr,
) -> bool {
    let ExprKind::Variable(name) = &array.kind else {
        return false;
    };
    if ctx.local_type(name).is_php_array() {
        return true;
    }
    if ctx.is_ref_bound_local(name) {
        return ctx.local_type(name).codegen_repr() == PhpType::Mixed;
    }
    matches!(
        ctx.local_type(name).codegen_repr(),
        PhpType::AssocArray { .. } | PhpType::Array(_)
    )
}

/// Returns true when an array-access unset receiver is a static ArrayAccess object.
pub(super) fn unset_array_access_has_object_receiver(
    ctx: &LoweringContext<'_, '_>,
    array: &Expr,
) -> bool {
    let ty = match &array.kind {
        ExprKind::Variable(name) => ctx
            .local_types
            .get(name)
            .cloned()
            .unwrap_or_else(|| infer_expr_type_syntactic(array)),
        _ => infer_expr_type_syntactic(array),
    };
    type_satisfies_array_access_for_ir(ctx, &ty)
}

/// Lowers `unset($array[$key])`, dispatching on the receiver kind.
///
/// An associative-array local removes the element in place through `Op::HashUnset`. A packed
/// indexed-array local is first converted to a hash (PHP keeps the surviving keys without
/// renumbering) and then removed. An `ArrayAccess` object dispatches to its `offsetUnset($key)`
/// method. Declared PHP arrays use boxed sparse storage without changing their reference ABI.
pub(super) fn lower_unset_array_access(
    ctx: &mut LoweringContext<'_, '_>,
    array: &Expr,
    index: &Expr,
    expr: &Expr,
) {
    if let ExprKind::Variable(name) = &array.kind {
        if ctx.local_type(name).is_php_array()
            || (ctx.is_ref_bound_local(name)
                && ctx.local_type(name).codegen_repr() == PhpType::Mixed)
        {
            lower_unset_boxed_array_element(ctx, name, array.span, index, expr);
            return;
        }
        if !ctx.is_ref_bound_local(name) {
            match ctx.local_type(name).codegen_repr() {
                PhpType::AssocArray { .. } => {
                    lower_unset_hash_element(ctx, name, array.span, index, expr);
                    return;
                }
                PhpType::Array(elem_ty) => {
                    let elem_ty = if *elem_ty == PhpType::Never {
                        PhpType::Mixed
                    } else {
                        *elem_ty
                    };
                    lower_unset_indexed_element(ctx, name, elem_ty, array.span, index, expr);
                    return;
                }
                _ => {}
            }
        }
    }
    let synthetic = Expr::new(
        ExprKind::MethodCall {
            object: Box::new(array.clone()),
            method: "offsetUnset".to_string(),
            args: vec![index.clone()],
        },
        expr.span,
    );
    lower_expr(ctx, &synthetic);
}

/// Detaches the declared array cell before sparse removal and roots an owned key across callbacks.
fn lower_unset_boxed_array_element(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    array_span: Span,
    index: &Expr,
    expr: &Expr,
) {
    let index_value = lower_expr(ctx, index);
    // Clearing a key temp through a PHP null assignment widens its storage to Mixed.
    // Reloading it as Str then creates an unrooted copy before the destructor can throw.
    // Keep the concrete operand and clear its scoped owner without changing the slot type.
    let (index_value, key_owner) = root_owned_call_operand(ctx, index_value, index.span);
    let array_value = crate::ir_lower::stmt::load_array_local_for_write(ctx, name, array_span);
    ctx.emit_void(
        Op::OffsetUnset,
        vec![array_value.value, index_value.value],
        None,
        Op::OffsetUnset.default_effects(),
        Some(expr.span),
    );
    if let Some(slot) = key_owner {
        retire_owned_call_operand(ctx, slot, expr.span);
    }
}

/// Lowers `unset($hash[$key])` for an associative-array local as a `HashUnset` instruction.
///
/// Loads the array local, lowers the key, and emits the removal. The backend (`lower_hash_unset`)
/// copy-on-write splits the table, releases the removed key/value payloads, and stores the unique
/// table pointer back into the local slot, so no explicit store-back is needed here.
pub(super) fn lower_unset_hash_element(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    array_span: Span,
    index: &Expr,
    expr: &Expr,
) {
    let array_value = ctx.load_local(name, Some(array_span));
    let index_value = lower_expr(ctx, index);
    ctx.emit_void(
        Op::HashUnset,
        vec![array_value.value, index_value.value],
        None,
        Op::HashUnset.default_effects(),
        Some(expr.span),
    );
}

/// Lowers `unset($arr[$key])` for a packed indexed-array local.
///
/// PHP's `unset()` removes a key without renumbering, so the array can no longer be a contiguous
/// packed list (e.g. `unset([1,2,3][1])` leaves keys `0` and `2`). The local is converted to a hash
/// (`Op::ArrayToHash`) and retyped as `AssocArray<Int, T>`, after which the element is removed
/// through `HashUnset`. Subsequent uses of the local therefore see the associative representation.
pub(super) fn lower_unset_indexed_element(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    elem_ty: PhpType,
    array_span: Span,
    index: &Expr,
    expr: &Expr,
) {
    let array_value = ctx.load_local(name, Some(array_span));
    let assoc_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Int),
        value: Box::new(elem_ty),
    };
    let hash = ctx.emit_value(
        Op::ArrayToHash,
        vec![array_value.value],
        None,
        assoc_ty.clone(),
        Op::ArrayToHash.default_effects(),
        Some(array_span),
    );
    ctx.store_mutated_local(name, hash, assoc_ty, Some(array_span));
    lower_unset_hash_element(ctx, name, array_span, index, expr);
}

/// Returns true when a property unset target can be lowered without normal property storage support.
pub(super) fn unset_property_access_has_direct_lowering(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
) -> bool {
    matches!(
        property_unset_action(ctx, object, property),
        Some(
            UnsetPropertyAction::Magic
                | UnsetPropertyAction::Noop
                | UnsetPropertyAction::ClearSlot
                | UnsetPropertyAction::RemoveDynamic
        )
    )
}

/// Lowers `unset($object->property)` for magic and no-op property targets.
/// Lowers `unset($object->property)` for magic, no-op, fixed-slot and dynamic property targets.
pub(super) fn lower_unset_property_access(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    expr: &Expr,
) {
    let action = property_unset_action(ctx, object, property);
    match action {
        Some(UnsetPropertyAction::Magic) => {
            let object = lower_expr(ctx, object);
            lower_magic_property_unset(ctx, object, property, expr);
        }
        Some(UnsetPropertyAction::Noop) => {
            let object = lower_expr(ctx, object);
            // A runtime SUBCLASS can declare `__unset` where the static class does not, and php
            // calls it on such an instance instead of doing nothing. The receiver is lowered once,
            // before the guard, so php's evaluation order is unchanged either way.
            lower_guarded_magic_property_unset(ctx, object, property, expr, |_, _| {});
        }
        // Both storage shapes share `Op::PropUnset`: the backend already resolves the
        // receiver's property storage, so it picks the fixed-slot marker or the
        // dynamic-hash removal from the same instruction.
        Some(UnsetPropertyAction::ClearSlot | UnsetPropertyAction::RemoveDynamic) => {
            let object = lower_expr(ctx, object);
            lower_guarded_magic_property_unset(ctx, object, property, expr, |ctx, object| {
                let data = ctx.intern_string(property);
                ctx.emit_void(
                    Op::PropUnset,
                    vec![object.value],
                    Some(Immediate::Data(data)),
                    Op::PropUnset.default_effects(),
                    Some(expr.span),
                );
            });
        }
        Some(UnsetPropertyAction::Fallback) | None => {}
    }
}

/// Returns true when `unset($object->{$name})` has a direct EIR lowering.
///
/// The receiver must be an object whose class the module knows, or a boxed `Mixed`, and the name
/// expression must be a string. Everything else keeps the shared unsupported diagnostic rather
/// than a lowering that would have to guess which storage the name addresses.
pub(super) fn unset_dynamic_property_access_has_direct_lowering(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
    property: &Expr,
) -> bool {
    // `unset($o->$k)` almost always spells the name as a LOCAL, and `infer_expr_type_syntactic`
    // has no `Variable` arm at all: it answers from the expression's own shape. Asking it alone
    // therefore rejected the ordinary form and kept only a literal or a concatenation, so the
    // whole runtime-name lowering was unreachable from the syntax php programs actually use.
    if !matches!(unset_operand_type(ctx, property).codegen_repr(), PhpType::Str) {
        return false;
    }
    if isset_object_expr_class(ctx, object).is_some() {
        return true;
    }
    // A boxed receiver and a union both reach the Mixed ladder in the backend, which unboxes,
    // keeps non-objects out and then dispatches on the runtime class id, so both are lowerable.
    matches!(
        unset_operand_type(ctx, object).codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    )
}

/// Returns an unset operand's inferred type, reading a local's checked type when there is one.
///
/// Same shape as `unset_array_access_has_object_receiver`: the checker's local type is the
/// authority when the operand is a plain variable, and the syntactic answer is the fallback.
fn unset_operand_type(ctx: &LoweringContext<'_, '_>, expr: &Expr) -> PhpType {
    match &expr.kind {
        ExprKind::Variable(name) => ctx
            .local_types
            .get(name)
            .cloned()
            .unwrap_or_else(|| infer_expr_type_syntactic(expr)),
        _ => infer_expr_type_syntactic(expr),
    }
}

/// Lowers `unset($object->{$name})` for a property name only known at run time.
///
/// The receiver and the name are evaluated once, in source order, exactly as php does, and the
/// backend decides the rest: it is the only place that can compare the runtime name against the
/// receiver's declared names and then ask php's answer for the RUNTIME class.
pub(super) fn lower_unset_dynamic_property_access(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &Expr,
    expr: &Expr,
) {
    let object = lower_expr(ctx, object);
    let property = lower_expr(ctx, property);
    let property = crate::ir_lower::expr::property_access::coerce_runtime_property_name(
        ctx, property, expr.span,
    );
    // The removal CAN THROW: a name this scope may not reach raises php's catchable access
    // `Error`. Retiring the owning temporaries only afterwards meant a caught refusal skipped
    // both releases and leaked the receiver and the key. Pinning parks them for the window the
    // throwing instruction occupies, so the unwind path retires them through the record, and the
    // normal path unpins and retires them exactly once as before. Same contract call lowering
    // uses for every throwing instruction that holds owned operands.
    let pins = crate::ir_lower::expr::pin_in_flight_owners(
        ctx,
        &[object.value, property.value],
        expr.span,
    );
    ctx.emit_void(
        Op::DynamicPropUnset,
        vec![object.value, property.value],
        None,
        Op::DynamicPropUnset.default_effects(),
        Some(expr.span),
    );
    crate::ir_lower::expr::unpin_in_flight_owners(ctx, pins, expr.span);
    if ctx.value_needs_release_after_use(property) {
        crate::ir_lower::ownership::release_if_owned(ctx, property, Some(expr.span));
    }
    stabilize_unset_receiver(ctx, object, expr.span);
}

/// Releases an owning receiver temporary once the removal has run.
fn stabilize_unset_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    span: Span,
) {
    if ctx.value_is_owning_temporary(object) {
        crate::ir_lower::ownership::release_if_owned(ctx, object, Some(span));
    }
}

/// Wraps an `unset()` lowering in the `instanceof` chain a runtime subclass's `__unset` needs.
///
/// The receiver is already lowered, so the guard adds no evaluation and every branch converges on
/// one merge block. When no subclass adds the accessor the chain is empty and `lower_ordinary`
/// runs exactly where it ran before, with no extra block at all.
fn lower_guarded_magic_property_unset(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    expr: &Expr,
    lower_ordinary: impl FnOnce(&mut LoweringContext<'_, '_>, LoweredValue),
) {
    let magic_classes =
        crate::ir_lower::stmt::magic_accessor_subclasses(ctx, object.value, property, "__unset");
    if magic_classes.is_empty() {
        lower_ordinary(ctx, object);
        return;
    }
    let merge = ctx
        .builder
        .create_named_block("unset.property.magic.merge", Vec::new());
    for class_name in &magic_classes {
        let magic_block = ctx
            .builder
            .create_named_block("unset.property.magic.call", Vec::new());
        let next_block = ctx
            .builder
            .create_named_block("unset.property.magic.next", Vec::new());
        let matched = crate::ir_lower::stmt::emit_receiver_instanceof(
            ctx,
            object.value,
            class_name,
            expr.span,
        );
        ctx.builder.terminate(Terminator::CondBr {
            cond: matched,
            then_target: magic_block,
            then_args: Vec::new(),
            else_target: next_block,
            else_args: Vec::new(),
        });
        ctx.builder.position_at_end(magic_block);
        // Resolved against the class the guard proved: the receiver's static type does not
        // declare `__unset`, which is the whole reason this branch exists.
        let guarded = crate::ir_lower::stmt::borrow_receiver_as_runtime_class(
            ctx, object, class_name, expr.span,
        );
        lower_magic_property_unset(ctx, guarded, property, expr);
        branch_to(ctx, merge);
        ctx.builder.position_at_end(next_block);
    }
    lower_ordinary(ctx, object);
    branch_to(ctx, merge);
    ctx.builder.position_at_end(merge);
}

/// Describes how `unset($object->property)` should be lowered for a known receiver class.
pub(super) enum UnsetPropertyAction {
    Fallback,
    Magic,
    Noop,
    /// The property has a fixed value slot, so PHP's `unset()` leaves an observable removed state.
    /// Typed slots use the uninitialized-property error state. Untyped slots selected during type
    /// checking use boxed `Mixed` storage and answer a later read with PHP null plus a warning.
    ClearSlot,
    /// The property lives in the receiver's dynamic-property hash (`stdClass`, or an
    /// undeclared name on an `#[AllowDynamicProperties]` class), where PHP's `unset()`
    /// really is a key removal.
    RemoveDynamic,
}

/// Selects the PHP-visible `unset()` behavior for a statically known object property operand.
pub(super) fn property_unset_action(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
) -> Option<UnsetPropertyAction> {
    let (class_name, _) = isset_object_expr_class(ctx, object)?;
    // Every `stdClass` property is a hash entry, so `unset()` is a plain key removal and
    // `stdClass` declares no magic methods that could intercept it.
    if is_builtin_stdclass_name(&class_name) {
        return Some(UnsetPropertyAction::RemoveDynamic);
    }
    let class_info = ctx.classes.get(class_name.as_str())?;
    // php 7.4 removed shadow properties: a strict ancestor's `private $p` is not in this class's
    // by-name table, so `unset($child->p)` removes the DISTINCT dynamic entry of that name and
    // leaves the ancestor's slot alone. The accessibility ladder below still answers for that
    // slot under its plain name and would have chosen `__unset` or a no-op, so this arm comes
    // first. Without reserved hash storage there is no entry to remove and php's answer really
    // is a no-op, which is what `dynamic_property_unset_action` falls back to.
    if crate::types::property_name_shadows_ancestor_private_slot(
        ctx.classes,
        &class_name,
        property,
        ctx.current_class.as_deref(),
    ) {
        // php consults `__unset` for such a name exactly as it does for one the class never
        // declared, measured on php 8.5.10 from the child scope and from global scope alike.
        if class_method_signature(ctx, &class_name, &php_symbol_key("__unset")).is_some() {
            return Some(UnsetPropertyAction::Magic);
        }
        return Some(UnsetPropertyAction::RemoveDynamic);
    }
    if class_info.allow_dynamic_properties && class_info.visible_property(property).is_none() {
        return Some(dynamic_property_unset_action(ctx, &class_name));
    }
    if property_is_accessible_for_ir(ctx, &class_name, class_info, property) {
        // PHP does NOT consult `__unset` for a property it can see: it removes the property
        // itself. Type checking widens an untyped slot this operation can reach to boxed `Mixed`,
        // while typed slots already own an uninitialized marker, so both can now preserve the
        // removed state in their fixed storage.
        return Some(UnsetPropertyAction::ClearSlot);
    }
    if class_method_signature(ctx, &class_name, &php_symbol_key("__unset")).is_some() {
        Some(UnsetPropertyAction::Magic)
    } else {
        Some(UnsetPropertyAction::Noop)
    }
}

/// Lowers a magic `__unset($name)` call, guarding nullable receivers as a no-op.
pub(super) fn lower_magic_property_unset(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    expr: &Expr,
) {
    if value_is_nullable(ctx, object.value) {
        lower_nullable_magic_property_unset(ctx, object, property, expr);
        return;
    }
    let args = vec![Expr::new(
        ExprKind::StringLiteral(property.to_string()),
        expr.span,
    )];
    lower_method_call_with_receiver(ctx, object, "__unset", &args, Op::MethodCall, expr);
}

/// Lowers `__unset` for nullable receivers, doing nothing when the receiver is null.
pub(super) fn lower_nullable_magic_property_unset(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    expr: &Expr,
) {
    let null_block = ctx
        .builder
        .create_named_block("unset.property.null", Vec::new());
    let call_block = ctx
        .builder
        .create_named_block("unset.property.call", Vec::new());
    let merge = ctx
        .builder
        .create_named_block("unset.property.merge", Vec::new());
    let is_null = ctx.emit_value(
        Op::IsNull,
        vec![object.value],
        None,
        PhpType::Bool,
        Op::IsNull.default_effects(),
        Some(expr.span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_null.value,
        then_target: null_block,
        then_args: Vec::new(),
        else_target: call_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(null_block);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(call_block);
    let args = vec![Expr::new(
        ExprKind::StringLiteral(property.to_string()),
        expr.span,
    )];
    lower_method_call_with_receiver(ctx, object, "__unset", &args, Op::MethodCall, expr);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
}
