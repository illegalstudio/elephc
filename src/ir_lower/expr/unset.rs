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
                || unset_array_access_has_property_array_receiver(ctx, array)
        }
        ExprKind::PropertyAccess { object, property }
        | ExprKind::NullsafePropertyAccess { object, property } => {
            unset_property_access_has_direct_lowering(ctx, object, property)
        }
        _ => false,
    }
}

/// Returns true when an array-access unset receiver is a plain array/hash local whose element the
/// EIR backend can remove.
///
/// Associative arrays remove the element directly; packed indexed arrays are converted to a hash at
/// the unset site (PHP `unset()` leaves a sparse array). By-reference locals are excluded: their
/// storage is aliased to a caller whose static type would no longer match after a representation
/// change.
pub(super) fn unset_array_access_has_local_array_receiver(
    ctx: &LoweringContext<'_, '_>,
    array: &Expr,
) -> bool {
    let ExprKind::Variable(name) = &array.kind else {
        return false;
    };
    if ctx.is_ref_bound_local(name) {
        return false;
    }
    matches!(
        ctx.local_type(name).codegen_repr(),
        PhpType::AssocArray { .. } | PhpType::Array(_)
    )
}

/// Returns true when an array-access unset receiver is a PROPERTY holding an array.
///
/// `unset($obj->prop[$k])` had no direct lowering at all, so it fell through to the generic
/// builtin call and the backend refused it: `unset target shape with 1 lowered operands`. That is
/// a compile error on ordinary php, and on the container code that made it worth finding.
///
/// ASSOCIATIVE storage only. A PACKED property would have to become a hash at the unset site,
/// because PHP does not renumber what survives — and a property's type is one value for the whole
/// program rather than a flow-sensitive one, so recording that promotion retroactively refuses the
/// `$obj->prop[] = $v` written ABOVE the unset. A local has no such problem, which is why the
/// local path converts and this one does not. Left exactly as it was: still refused, not silently
/// renumbered.
pub(super) fn unset_array_access_has_property_array_receiver(
    ctx: &LoweringContext<'_, '_>,
    array: &Expr,
) -> bool {
    let ExprKind::PropertyAccess { object, property } = &array.kind else {
        return false;
    };
    let Some((class_name, _)) = isset_object_expr_class(ctx, object) else {
        return false;
    };
    let Some(class_info) = ctx.classes.get(class_name.as_str()) else {
        return false;
    };
    // A property reached through `__get` is not storage this can write back to.
    if !property_is_accessible_for_ir(ctx, &class_name, class_info, property) {
        return false;
    }
    class_info
        .visible_property(property)
        .is_some_and(|(_, (_, property_ty))| {
            matches!(property_ty.codegen_repr(), PhpType::AssocArray { .. })
        })
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
/// method like before. By-reference array locals fall through to the object path.
pub(super) fn lower_unset_array_access(
    ctx: &mut LoweringContext<'_, '_>,
    array: &Expr,
    index: &Expr,
    expr: &Expr,
) {
    if let ExprKind::Variable(name) = &array.kind {
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
    if let ExprKind::PropertyAccess { object, property } = &array.kind {
        if lower_unset_property_element(ctx, object, property, index, expr) {
            return;
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

/// Lowers `unset($obj->prop[$key])` on a property that holds an ARRAY.
///
/// The dispatch above only ever recognised a plain LOCAL, so an array held in a property fell
/// through to the `offsetUnset()` synthetic — which a class that does not implement `ArrayAccess`
/// cannot answer, and the backend refused the call outright: `unset target shape with 1 lowered
/// operands`. That refusal is a compile error on ordinary php, and it is the same one
/// `unset($obj->prop[$k])` produces in Symfony's own container code.
///
/// `HashUnset` publishes the possibly-relocated table back through the receiver's PLACE, and a
/// `PropGet` reached through an `Acquire` is one the backend already resolves, so nothing new is
/// needed to make the write land.
///
/// Answers whether it handled the target: an `ArrayAccess` object still belongs to the caller's
/// `offsetUnset()` path below.
fn lower_unset_property_element(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    index: &Expr,
    expr: &Expr,
) -> bool {
    let object_value = lower_expr(ctx, object);
    let Some(property_ty) =
        crate::ir_lower::stmt::static_property_helpers::object_property_type(
            ctx,
            object_value.value,
            property,
        )
    else {
        return false;
    };
    if !matches!(property_ty.codegen_repr(), PhpType::AssocArray { .. }) {
        return false;
    }
    let data = ctx.intern_string(property);
    let loaded = ctx.emit_value(
        Op::PropGet,
        vec![object_value.value],
        Some(Immediate::Data(data)),
        property_ty.codegen_repr(),
        Op::PropGet.default_effects(),
        Some(expr.span),
    );
    let loaded = crate::ir_lower::ownership::acquire_if_refcounted(ctx, loaded, Some(expr.span));
    let index_value = lower_expr(ctx, index);
    ctx.emit_void(
        Op::HashUnset,
        vec![loaded.value, index_value.value],
        None,
        Op::HashUnset.default_effects(),
        Some(expr.span),
    );
    true
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
                | UnsetPropertyAction::ClearTyped
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
    match property_unset_action(ctx, object, property) {
        Some(UnsetPropertyAction::Magic) => {
            let object = lower_expr(ctx, object);
            lower_magic_property_unset(ctx, object, property, expr);
        }
        Some(UnsetPropertyAction::Noop) => {
            lower_expr(ctx, object);
        }
        // Both storage shapes share `Op::PropUnset`: the backend already resolves the
        // receiver's property storage, so it picks the fixed-slot marker or the
        // dynamic-hash removal from the same instruction.
        Some(UnsetPropertyAction::ClearTyped | UnsetPropertyAction::RemoveDynamic) => {
            let object = lower_expr(ctx, object);
            let data = ctx.intern_string(property);
            ctx.emit_void(
                Op::PropUnset,
                vec![object.value],
                Some(Immediate::Data(data)),
                Op::PropUnset.default_effects(),
                Some(expr.span),
            );
        }
        Some(UnsetPropertyAction::Fallback) | None => {}
    }
}

/// Describes how `unset($object->property)` should be lowered for a known receiver class.
pub(super) enum UnsetPropertyAction {
    Fallback,
    Magic,
    Noop,
    /// The property has a DECLARED type, so PHP's `unset()` leaves it uninitialized —
    /// a state elephc's fixed property slots represent exactly.
    ClearTyped,
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
    if class_info.allow_dynamic_properties && class_info.visible_property(property).is_none() {
        return Some(dynamic_property_unset_action(ctx, &class_name));
    }
    if property_is_accessible_for_ir(ctx, &class_name, class_info, property) {
        // PHP does NOT consult `__unset` for a property it can see: it removes the
        // property itself. A DECLARED (typed) property becomes uninitialized, which
        // elephc's fixed slots can represent exactly.
        if class_info.visible_property_is_declared(property) {
            return Some(UnsetPropertyAction::ClearTyped);
        }
        // An UNTYPED fixed slot has no "removed" state and no null-capable storage:
        // PHP's later read must warn and answer `null`, which a slot the checker typed
        // `Int`/`Str`/... cannot represent. Keep the explicit unsupported diagnostic
        // rather than leaving a stale value or a garbage payload behind.
        return Some(UnsetPropertyAction::Fallback);
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

