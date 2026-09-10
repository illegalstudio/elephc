//! Purpose:
//! Reference assignment and instance or static property reads.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers an object property read.
pub(super) fn lower_property_get(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    op: Op,
    expr: &Expr,
) -> LoweredValue {
    let object = lower_expr(ctx, object);
    lower_property_get_from_value(ctx, object, property, op, expr)
}

/// Lowers `$target = &$obj->prop`: binds the local `$target` to the reference cell
/// stored in the object's reference-property slot, so reads/writes of either side go
/// through the same cell (write-through). The property was promoted to a reference
/// property by the checker, so its slot holds a live cell pointer.
pub(crate) fn lower_ref_assign_property(
    ctx: &mut LoweringContext<'_, '_>,
    target: &str,
    source: &Expr,
    span: Span,
) {
    let ExprKind::PropertyAccess { object, property } = &source.kind else {
        return;
    };
    let object = lower_expr(ctx, object);
    let value_type = property_get_result_type(ctx, object.value, property, Op::PropGet, source);
    let owns_cell = match ctx.builder.value_php_type(object.value).codegen_repr() {
        PhpType::Object(class) => ctx.classes.get(class.as_str())
            .is_some_and(|info| info.owned_reference_properties.contains(property)),
        _ => false,
    };
    let data = ctx.intern_string(property);
    let cell_ptr = ctx.emit_value(
        Op::LoadPropRefCell,
        vec![object.value],
        Some(Immediate::Data(data)),
        PhpType::Pointer(None),
        Op::LoadPropRefCell.default_effects(),
        Some(span),
    );
    if owns_cell {
        ctx.bind_owned_local_ref_cell_ptr(target, cell_ptr, value_type, Some(span));
        release_owning_receiver_temporary(ctx, object, span);
    } else {
        ctx.bind_local_ref_cell_ptr(target, cell_ptr, value_type, Some(span));
    }
}

/// Binds a local alias directly to a native static property's process-lifetime storage slot.
///
/// The static symbol owns its payload, so the alias itself is borrowed. Container replacements
/// written through the alias are immediately visible through `C::$property`, and the same alias
/// gives `IterStart` a local origin that can reload the replacement after hash growth.
pub(crate) fn lower_ref_assign_static_property(
    ctx: &mut LoweringContext<'_, '_>,
    target: &str,
    source: &Expr,
    span: Span,
) {
    let ExprKind::StaticPropertyAccess { receiver, property } = &source.kind else {
        return;
    };
    let name = format!("{}::{}", receiver_name(receiver), property);
    let data = ctx.intern_string(&name);
    let value_type = static_property_result_type(ctx, receiver, property, source);
    let cell_ptr = ctx.emit_value(
        Op::LoadStaticPropertyRefCell,
        Vec::new(),
        Some(Immediate::Data(data)),
        PhpType::Pointer(None),
        Op::LoadStaticPropertyRefCell.default_effects(),
        Some(span),
    );
    ctx.bind_local_ref_cell_ptr(target, cell_ptr, value_type, Some(span));
}

/// Lowers `$target = &call()`: binds `$target` to the reference cell returned by a
/// by-reference-returning callee.
///
/// The staging that adopts the transferred cell is declared and PUBLISHED in the unwind chain
/// before the source expression is lowered. That ordering is what makes the lease survive a
/// same-frame catch: the record nests outside every root the call's own arguments publish, so it
/// is still live while the callee's argument temporaries are retired, while an owning receiver is
/// destroyed, and while this function retires whatever `$target` was bound to before. Any of
/// those steps can run a destructor that throws, and none of them may leave the lease held with
/// nothing to release it.
///
/// Only a call whose SELECTED lowering actually returns a raw cell can be bound, which
/// `finish_reference_return_call` reports by adopting into that staging. A statically declared
/// by-reference signature is NOT sufficient on its own, because a call that reaches a dynamic
/// descriptor invoker gets an ordinary owned `Mixed` copy back and the invoker retires the cell
/// (`codegen::runtime_callable_invoker::reference_return`). Binding that value as a cell pointer
/// would dereference a payload word as an address, so an unadopted result is refused with a
/// compile diagnostic instead, and the target is bound to its own managed cell so the remaining
/// lowering stays well formed.
pub(crate) fn lower_ref_assign_call(
    ctx: &mut LoweringContext<'_, '_>,
    target: &str,
    source: &Expr,
    span: Span,
) {
    let (staged, owner) = ctx.predeclare_returned_ref_cell_staging();
    register_owned_call_operand(ctx, owner, span);
    let previous = ctx
        .reference_call_context
        .replace(crate::ir_lower::context::ReferenceCallContext {
            depth: ctx.expression_depth + 1,
            staged: staged.clone(),
            adopted: false,
        });
    let result = lower_expr(ctx, source);
    let adopted = ctx
        .reference_call_context
        .take()
        .is_some_and(|context| context.adopted);
    ctx.reference_call_context = previous;
    if !adopted {
        // The staging slot stayed zero, so detaching its record releases nothing.
        unregister_owned_call_operand(ctx, owner, span);
        crate::ir_lower::diagnostics::refuse(
            span,
            "Unsupported reference assignment: this compiler transfers a reference only from a \
             call it resolves to a by-reference-returning function, method, static method or \
             closure. PHP also allows the reference here, but the selected lowering hands back a \
             copied value rather than the callee's reference cell",
        );
        let value_type = ctx.builder.value_php_type(result.value);
        ctx.store_local(target, result, value_type, Some(span));
        ctx.promote_local_ref_cell(target, Some(span));
        return;
    }
    // Publishing the alias retains the cell in the target's own owner, so the staging lease can
    // retire. The record is detached first, exactly as `retire_owned_call_operand` does for a
    // value root: `release_ref_cell_owner` clears the slot before releasing, so a throwing
    // payload destructor cannot be retried by a later walk of the chain.
    ctx.alias_local_ref_cell(target, &staged, Some(span));
    unregister_owned_call_operand(ctx, owner, span);
    ctx.release_ref_cell_owner(&staged, Some(span));
}

/// Lowers `$target =& $arr[idx]` using the ownership represented by the container.
///
/// The addressable receiver is first represented by a local alias and normalized to hash storage.
/// Its entry then owns a managed tag-11 cell, so the new local can outlive replacement or
/// destruction of the parent. This also gives copy-on-write and growth a writable place where
/// they can publish a replacement reached through a static property or nested element.
pub(crate) fn lower_ref_assign_array_elem(
    ctx: &mut LoweringContext<'_, '_>,
    target: &str,
    source: &Expr,
    span: Span,
) {
    let ExprKind::ArrayAccess { array, index } = &source.kind else {
        return;
    };
    let prepared_array = prepare_addressable_ref_array_receiver(ctx, array);
    let array = prepared_array.as_ref().unwrap_or(array);
    crate::ir_lower::stmt::promote_by_ref_foreach_source(ctx, array, true);
    let array_value = lower_expr(ctx, array);
    let container_type = ctx.builder.value_php_type(array_value.value).codegen_repr();
    let mut index_value = lower_expr(ctx, index);
    let value_type = match container_type {
        PhpType::Array(elem_ty) => {
            if elem_ty.codegen_repr() == PhpType::Mixed {
                PhpType::Mixed
            } else {
                index_value = coerce_array_key_to_int_at_span(ctx, index_value, Some(index.span), false);
                normalize_value_php_type(*elem_ty)
            }
        }
        PhpType::AssocArray { .. } | PhpType::Mixed | PhpType::Union(_) => PhpType::Mixed,
        _ => array_access_result_type(ctx, array_value.value, Op::ArrayGet, source),
    };
    let cell_ptr = ctx.emit_value(
        Op::LoadArrayElemRefCell,
        vec![array_value.value, index_value.value],
        None,
        value_type.clone(),
        Op::LoadArrayElemRefCell.default_effects(),
        Some(span),
    );
    ctx.bind_owned_local_ref_cell_ptr(target, cell_ptr, value_type, Some(span));
    // A property read that is not addressable storage (a `mixed` property, so no synthetic
    // alias was reified above) arrives as an OWNING temporary -- `PropGet` acquires -- and
    // nothing else releases it: the binding above retains the entry's managed cell, not the
    // container, so the container's owner was simply leaked, one boxed array per
    // `$r = &$obj->m[k]`. Only the property reads are released here. A concrete container
    // loaded from a Mixed-widened local also counts as an owning temporary, but that detached
    // owner is what the backend's consuming store-back transfers, and releasing it too freed
    // the table under the local.
    if matches!(
        ctx.builder.value_defining_op(array_value.value),
        Some(Op::PropGet | Op::DynamicPropGet | Op::NullsafePropGet)
    ) && ctx.value_is_owning_temporary(array_value)
    {
        crate::ir_lower::ownership::release_if_owned(ctx, array_value, Some(span));
    }
}

/// Reifies a static property, stable declared property, or nested element as a local receiver.
///
/// Element reference lowering can only republish COW or growth through `ReceiverPlace` when its
/// receiver comes from a local or ref-cell slot. Each synthetic alias names the original storage,
/// and recursively normalizing element parents to hash storage gives every level a managed cell.
pub(crate) fn prepare_addressable_ref_array_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    source: &Expr,
) -> Option<Expr> {
    prepare_addressable_ref_array_receiver_impl(ctx, source, None)
}

/// Prepares a nested ref-argument receiver and records its managed expression aliases.
pub(crate) fn prepare_scoped_addressable_ref_array_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    source: &Expr,
) -> Option<(Expr, Vec<String>)> {
    let mut aliases = Vec::new();
    let receiver = prepare_addressable_ref_array_receiver_impl(ctx, source, Some(&mut aliases))?;
    Some((receiver, aliases))
}

fn prepare_addressable_ref_array_receiver_impl(
    ctx: &mut LoweringContext<'_, '_>,
    source: &Expr,
    mut scoped_aliases: Option<&mut Vec<String>>,
) -> Option<Expr> {
    match &source.kind {
        ExprKind::Variable(_) => Some(source.clone()),
        ExprKind::PropertyAccess { object, property }
            if by_ref_foreach_property_source_is_addressable(ctx, object, property) =>
        {
            let alias = ctx.declare_synthetic_php_local(PhpType::Mixed);
            lower_ref_assign_property(ctx, &alias, source, source.span);
            // This alias exists only to make the property's storage addressable for a nested
            // reference. Detach an exact PHP array zval before the child promotion can update it
            // in place, while stores through the ref-bound local still publish into the property.
            crate::ir_lower::stmt::load_array_local_for_write(ctx, &alias, source.span);
            if let Some(aliases) = scoped_aliases.as_deref_mut() {
                if publish_scoped_ref_receiver_alias(ctx, &alias, source.span) {
                    aliases.push(alias.clone());
                }
            }
            Some(Expr::new(ExprKind::Variable(alias), source.span))
        }
        ExprKind::StaticPropertyAccess { .. } => {
            let alias = ctx.declare_synthetic_php_local(PhpType::Mixed);
            lower_ref_assign_static_property(ctx, &alias, source, source.span);
            // Static array values are boxed. Clone the zval through the synthetic ref-bound
            // alias so an earlier by-value copy keeps its own cell while nested writes publish
            // the detached cell back into the process-lifetime static slot.
            crate::ir_lower::stmt::load_array_local_for_write(ctx, &alias, source.span);
            if let Some(aliases) = scoped_aliases.as_deref_mut() {
                if publish_scoped_ref_receiver_alias(ctx, &alias, source.span) {
                    aliases.push(alias.clone());
                }
            }
            Some(Expr::new(ExprKind::Variable(alias), source.span))
        }
        ExprKind::ArrayAccess { array, index } => {
            let call_scoped = scoped_aliases.is_some();
            let parent = prepare_addressable_ref_array_receiver_impl(
                ctx,
                array,
                scoped_aliases.as_deref_mut(),
            )?;
            let element = Expr::new(
                ExprKind::ArrayAccess {
                    array: Box::new(parent),
                    index: index.clone(),
                },
                source.span,
            );
            let alias = ctx.declare_synthetic_php_local(PhpType::Mixed);
            lower_ref_assign_array_elem(ctx, &alias, &element, source.span);
            // A persistent `=&` alias must detach a child still shared with a by-value snapshot
            // before a deeper reference binds one of its slots. A call-scoped argument does not:
            // the callee's mutating ref-cell path separates and republishes the payload, while a
            // second argument naming the same element must retain that exact cell. Cloning here
            // would replace the child between the two leases and split one PHP reference set.
            if !call_scoped {
                crate::ir_lower::stmt::load_array_local_for_write(ctx, &alias, source.span);
            }
            if let Some(aliases) = scoped_aliases.as_deref_mut() {
                if publish_scoped_ref_receiver_alias(ctx, &alias, source.span) {
                    aliases.push(alias.clone());
                }
            }
            Some(Expr::new(ExprKind::Variable(alias), source.span))
        }
        _ => None,
    }
}

/// Binds a synthetic foreach source alias to an element after its receiver was promoted to hash.
///
/// Unlike an ordinary indexed `=&` binding, a promoted hash entry owns a managed reference cell.
/// Retaining that cell gives the synthetic origin a stable writeback address across table growth,
/// source replacement, and destruction of the enclosing container.
pub(crate) fn lower_owned_ref_assign_array_elem(
    ctx: &mut LoweringContext<'_, '_>,
    target: &str,
    source: &Expr,
    span: Span,
) {
    let ExprKind::ArrayAccess { array, index } = &source.kind else {
        return;
    };
    let array_value = lower_expr(ctx, array);
    let index_value = lower_expr(ctx, index);
    let value_type = match ctx.builder.value_php_type(array_value.value).codegen_repr() {
        PhpType::Array(elem_ty) => normalize_value_php_type(*elem_ty),
        PhpType::AssocArray { value, .. } => normalize_value_php_type(*value),
        _ => PhpType::Mixed,
    };
    let cell_ptr = ctx.emit_value(
        Op::LoadArrayElemRefCellExisting,
        vec![array_value.value, index_value.value],
        None,
        value_type.clone(),
        Op::LoadArrayElemRefCellExisting.default_effects(),
        Some(span),
    );
    ctx.bind_owned_local_ref_cell_ptr(target, cell_ptr, value_type, Some(span));
}

/// Boxes a local indexed array before exposing an element as a dynamically writable reference.
fn prepare_array_reference_source(
    ctx: &mut LoweringContext<'_, '_>, array: &Expr, span: Span,
) -> LoweredValue {
    let value = lower_expr(ctx, array);
    let ExprKind::Variable(name) = &array.kind else { return value; };
    let PhpType::Array(_) = ctx.builder.value_php_type(value.value).codegen_repr() else { return value; };
    // Captured references may later store any PHP type. Their source array must
    // use the same boxed element representation before a cell address escapes.
    // ArrayToMixed also separates preexisting COW copies of an already boxed array.
    let ty = PhpType::Array(Box::new(PhpType::Mixed));
    let converted = ctx.emit_value(Op::ArrayToMixed, vec![value.value], None,
        ty.clone(), Op::ArrayToMixed.default_effects(), Some(span));
    ctx.store_mutated_local(name, converted, ty, Some(span));
    converted
}

/// Lowers a named property read once the receiver is already evaluated.
pub(super) fn lower_property_get_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    op: Op,
    expr: &Expr,
) -> LoweredValue {
    if op == Op::NullsafePropGet && value_is_definitely_null(ctx, object.value) {
        return lower_boxed_null(ctx, expr);
    }
    // Route a read of a get-hooked property to its synthetic accessor, except inside that property's
    // own accessor, where `$this->prop` must read the raw backing slot to avoid infinite recursion.
    // A nullsafe read (`$obj?->prop`) routes to a nullsafe call so the null short-circuit is kept.
    if matches!(op, Op::PropGet | Op::NullsafePropGet)
        && class_declares_hook_accessor(ctx, object.value, &property_hook_get_method(property))
        && !ctx.in_own_property_accessor(property)
    {
        let accessor = property_hook_get_method(property);
        let call_op = if op == Op::NullsafePropGet {
            Op::NullsafeMethodCall
        } else {
            Op::MethodCall
        };
        return lower_method_call_with_receiver(ctx, object, &accessor, &[], call_op, expr);
    }
    let data = ctx.intern_string(property);
    let result_type = property_get_result_type(ctx, object.value, property, op, expr);
    let result = ctx.emit_value(
        op,
        vec![object.value],
        Some(Immediate::Data(data)),
        result_type,
        op.default_effects(),
        Some(expr.span),
    );
    stabilize_borrowed_result_and_release_receiver(ctx, object, result, expr.span)
}

/// Returns true when value metadata proves the runtime value is PHP null.
pub(super) fn value_is_definitely_null(ctx: &LoweringContext<'_, '_>, value: crate::ir::ValueId) -> bool {
    matches!(ctx.builder.value_php_type(value), PhpType::Void | PhpType::Never)
}

/// Returns true when value metadata permits PHP null at runtime.
pub(super) fn value_is_nullable(ctx: &LoweringContext<'_, '_>, value: crate::ir::ValueId) -> bool {
    match ctx.builder.value_php_type(value) {
        PhpType::Void | PhpType::Never => true,
        PhpType::Union(members) => members.iter().any(|member| matches!(member, PhpType::Void)),
        _ => false,
    }
}

/// Returns precise PHP metadata for a named property read when class metadata is available.
pub(super) fn property_get_result_type(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    property: &str,
    op: Op,
    expr: &Expr,
) -> PhpType {
    property_result_type(ctx, object, property, op, expr, true)
}

/// Returns the property's slot type, IGNORING the null a receiver that may be a container miss
/// would otherwise add.
///
/// The by-reference `foreach` fetch-for-write read uses this because it never produces that
/// null: PHP evaluates such a source in a WRITE context, where a null receiver is a fatal
/// `Error`, and `PropGetForWrite` raises exactly that instead of answering (issue #690).
pub(super) fn property_slot_result_type(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    property: &str,
    op: Op,
    expr: &Expr,
) -> PhpType {
    property_result_type(ctx, object, property, op, expr, false)
}

/// Shared body of the two property result-type queries.
///
/// `receiver_miss_is_null` says whether a receiver read that can MISS its container -- an array
/// or hash element -- makes the answer nullable. A read does produce null there; a
/// fetch-for-write does not.
fn property_result_type(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    property: &str,
    op: Op,
    expr: &Expr,
    receiver_miss_is_null: bool,
) -> PhpType {
    if op == Op::NullsafePropGet {
        return PhpType::Mixed;
    }
    let object_ty = ctx.builder.value_php_type(object);
    let Some((class_name, nullable)) = singular_object_class(&object_ty) else {
        if matches!(object_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
            return PhpType::Mixed;
        }
        if let PhpType::Packed(class_name) = object_ty.codegen_repr() {
            let normalized = class_name.trim_start_matches('\\');
            let Some(class_info) = ctx.packed_classes.get(normalized) else {
                return fallback_expr_type(expr);
            };
            let Some(field) = class_info.fields.iter().find(|field| field.name == property) else {
                return fallback_expr_type(expr);
            };
            return normalize_value_php_type(field.php_type.codegen_repr());
        }
        return fallback_expr_type(expr);
    };
    let nullable =
        nullable || (receiver_miss_is_null && value_may_carry_container_miss(ctx, object));
    let normalized = class_name.trim_start_matches('\\');
    if is_builtin_stdclass_name(normalized) {
        return if nullable {
            nullable_result_type(PhpType::Mixed)
        } else {
            PhpType::Mixed
        };
    }
    let Some(class_info) = ctx.classes.get(normalized) else {
        return fallback_expr_type(expr);
    };
    if let Some(property_ty) = runtime_property_type_override(ctx, normalized, property) {
        let property_ty = normalize_value_php_type(property_ty);
        return if nullable {
            nullable_result_type(property_ty)
        } else {
            property_ty
        };
    }
    // `visible_property` still answers for a strict ancestor's PRIVATE slot, which php resolves
    // to a dynamic property everywhere but the class that declared it. Typing the read from that
    // slot handed the backend a non-null-capable result for a name whose answer is the dynamic
    // hash, or php `null`.
    if class_info.visible_property(property).is_some()
        && property_name_is_dynamic_in_scope(ctx, normalized, property)
    {
        return if nullable {
            nullable_result_type(PhpType::Mixed)
        } else {
            PhpType::Mixed
        };
    }
    let Some((_, (_, property_ty))) = class_info.visible_property(property) else {
        if let Some(magic_ty) = magic_get_result_type(ctx, normalized) {
            return if nullable {
                nullable_result_type(magic_ty)
            } else {
                magic_ty
            };
        }
        // The clone-override hash answers an undeclared name exactly like the attribute's does,
        // so both reserve the same boxed `mixed` result rather than a declared slot type.
        if class_info.dynamic_property_hash_is_name_addressable() {
            return if nullable {
                nullable_result_type(PhpType::Mixed)
            } else {
                PhpType::Mixed
            };
        }
        return fallback_expr_type(expr);
    };
    let property_ty = normalize_value_php_type(property_ty.clone());
    if nullable {
        nullable_result_type(property_ty)
    } else {
        property_ty
    }
}

/// Returns whether a container read can carry PHP null in a statically non-null pointer type.
pub(super) fn value_may_carry_container_miss(
    ctx: &LoweringContext<'_, '_>,
    value: crate::ir::ValueId,
) -> bool {
    let Some(inst) = ctx.builder.value_defining_instruction(value) else {
        return false;
    };
    match inst.op {
        Op::ArrayGet | Op::ArrayGetSilent | Op::HashGet | Op::HashGetSilent => true,
        Op::Acquire => inst
            .operands
            .first()
            .copied()
            .is_some_and(|source| value_may_carry_container_miss(ctx, source)),
        _ => false,
    }
}

/// Returns the normalized return type for a class `__get` magic property hook.
pub(super) fn magic_get_result_type(ctx: &LoweringContext<'_, '_>, class_name: &str) -> Option<PhpType> {
    class_method_signature(ctx, class_name, &php_symbol_key("__get"))
        .map(|signature| normalize_value_php_type(signature.return_type.clone()))
}

/// Adds nullability to a result type without nesting existing union metadata.
pub(super) fn nullable_result_type(php_type: PhpType) -> PhpType {
    match php_type {
        PhpType::Union(mut members) => {
            if !members.iter().any(|member| matches!(member, PhpType::Void)) {
                members.push(PhpType::Void);
            }
            PhpType::Union(members)
        }
        other => PhpType::Union(vec![other, PhpType::Void]),
    }
}

/// Returns true when the runtime class of `object` declares the synthetic property-hook accessor
/// `accessor_method` (`__propget_<p>` / `__propset_<p>`). Drives the decision to route a property
/// read/write to a hook; inherited (flattened) methods count, so subclasses inherit hooks.
pub(super) fn class_declares_hook_accessor(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    accessor_method: &str,
) -> bool {
    let object_ty = ctx.builder.value_php_type(object);
    let Some((class_name, _nullable)) = singular_object_class(&object_ty) else {
        return false;
    };
    let key = php_symbol_key(accessor_method);
    ctx.classes
        .get(class_name)
        .is_some_and(|info| info.methods.contains_key(&key))
}

/// Returns true when reading `property` on `object` can hit PHP's
/// "must not be accessed before initialization" fatal.
///
/// A property is uninitialized while it is DECLARED WITH A TYPE and has no default:
/// `public ?P $p;` and `public string $s;` both start uninitialized, and PHP fatals on a plain
/// read of either. A default makes the slot live before the constructor body runs, and an
/// untyped property is plain null, so neither can ever be in that state — which is what keeps
/// this gate off the overwhelmingly common shapes.
///
/// A reachable `unset()` also widens an untyped fixed slot to boxed `Mixed` storage. Its high
/// word carries the same marker, but a later value read warns and answers null instead of raising
/// the typed-property error.
pub(super) fn property_can_be_uninitialized(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    property: &str,
) -> bool {
    let object_ty = ctx.builder.value_php_type(object);
    let Some((class_name, nullable)) = singular_object_class(&object_ty) else {
        return false;
    };
    // `Op::PropInitialized` reads a slot, so it needs an object pointer. A concrete `C`
    // receiver already is one; a `?C` one represents as a boxed `Mixed` and the backend
    // unboxes it, answering FALSE for a null receiver — which is the answer `??` wants there
    // anyway, since `null->p ?? "d"` is the default. Every other boxed shape (plain `Mixed`, a
    // union carrying a scalar arm or two classes) has no single slot to probe and is turned
    // away here, keeping the ordinary read it had before.
    if !nullable && !matches!(object_ty.codegen_repr(), PhpType::Object(_)) {
        return false;
    }
    // A get-HOOKED property has no slot to probe: its value comes from the synthetic accessor
    // that `lower_property_get_from_value` routes to, and the backing slot behind it is
    // legitimately uninitialized. Probing it answers "not initialized" and sends `??` to its
    // default, so `$p?->full ?? "(none)"` on a real object answered `(none)` instead of running
    // the hook. Inside the accessor itself `$this->full` IS the raw slot, which is the one place
    // the probe applies — the same exception the read makes.
    if class_declares_hook_accessor(ctx, object, &property_hook_get_method(property))
        && !ctx.in_own_property_accessor(property)
    {
        return false;
    }
    let Some(info) = ctx.classes.get(class_name) else {
        return false;
    };
    let Some((index, (_, property_ty))) = info.visible_property(property) else {
        return false;
    };
    // Whether the slot was DECLARED with a type — asked of the schema, not inferred from the
    // stored `PhpType`. Both questions agree on `?string`, which represents as `Mixed` and is
    // still declared, and on an untyped `public $x;`, which is plain null from the start and
    // must stay on the ordinary path. They disagree on `public mixed $x;`: it IS declared and
    // starts uninitialized, but its type is literally `Mixed`, so a test on the representation
    // read it as untyped and `$o->x ?? "d"` raised where PHP answers the default.
    //
    // A DEFAULT does not exclude the property. It used to: a defaulted slot is live from
    // construction, so it looked as though it could never be uninitialized. `unset($o->x)`
    // returns a typed property to the uninitialized state whatever its default, and the
    // ordinary read then raises where PHP's `??` answers the default. The runtime probe
    // settles both cases, so the gate asks only whether the property is TYPED.
    info.property_slot_is_declared(index, property)
        || (!info.property_slot_is_reference(index, property)
            && property_ty.codegen_repr() == PhpType::Mixed)
}

/// Reads `property` the way `isset()` does: yields null instead of raising when the slot is
/// still uninitialized.
///
/// `??` must not fatal on `$o->p` — PHP answers the default — but the ordinary read does. The
/// initialized-aware read already exists for `isset()`, which produces a BOOLEAN; this is its
/// value-producing twin, and it is entered only for the properties
/// `property_can_be_uninitialized` admits, so every other read keeps its exact slot type.
pub(super) fn lower_initialized_property_value(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    expr: &Expr,
) -> LoweredValue {
    let temp_name = ctx.declare_hidden_temp(PhpType::Mixed);
    let uninitialized_block = ctx
        .builder
        .create_named_block("coalesce.property.uninitialized", Vec::new());
    let read_block = ctx
        .builder
        .create_named_block("coalesce.property.read", Vec::new());
    let merge = ctx
        .builder
        .create_named_block("coalesce.property.merge", Vec::new());
    let data = ctx.intern_string(property);
    let initialized = ctx.emit_value(
        Op::PropInitialized,
        vec![object.value],
        Some(Immediate::Data(data)),
        PhpType::Bool,
        Op::PropInitialized.default_effects(),
        Some(expr.span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: initialized.value,
        then_target: read_block,
        then_args: Vec::new(),
        else_target: uninitialized_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(uninitialized_block);
    // This path never reads the property, so nothing downstream consumes the receiver — an
    // OWNING one has to be released here. The read path below hands it to
    // `lower_property_get_from_value`, which disposes of it the way an ordinary read does.
    // `mk()->p ?? "none"` leaked one object per call without this.
    //
    // Only an OWNING one: a plain `$c` receiver is BORROWED from its slot, and releasing it
    // hands back a reference this expression never took. `$c->p ??= 42` through a `?C`
    // parameter died with "Attempt to assign property on null" — the release freed the boxed
    // receiver, and the write that followed read the freed cell. `guard_initialized_chain_property`
    // gates its own cleanup block the same way.
    if ctx.value_is_owning_temporary(object) {
        crate::ir_lower::ownership::release_if_owned(ctx, object, Some(expr.span));
    }
    let null_value = lower_boxed_null(ctx, expr);
    store_value_into_temp(ctx, &temp_name, PhpType::Mixed, null_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(read_block);
    let read_value = lower_property_get_from_value(ctx, object, property, Op::PropGet, expr);
    // Both arms store into one Mixed temporary, so a slot that is not already boxed has to be.
    let read_value = if matches!(
        ctx.builder.value_php_type(read_value.value).codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        read_value
    } else {
        ctx.emit_value(
            Op::MixedBox,
            vec![read_value.value],
            None,
            PhpType::Mixed,
            Op::MixedBox.default_effects(),
            Some(expr.span),
        )
    };
    store_value_into_temp(ctx, &temp_name, PhpType::Mixed, read_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    take_owned_temp(ctx, &temp_name, expr.span)
}

/// Returns true when reading `property` on `receiver` can hit PHP's "must not be accessed
/// before initialization" fatal for a STATIC slot.
///
/// The rule is the instance one: a static property is uninitialized only while it is DECLARED
/// WITH A TYPE. `public static $u;` is plain null from the start, and a receiver whose class
/// is not known statically cannot be probed at all.
///
/// Unlike the instance gate there is no defaulted-slot question to settle here: `unset()` does
/// not apply to a static property, so a default really does make the slot live for good — but
/// asking only "is it typed" costs one sentinel compare on a slot that can never carry the
/// sentinel, and keeps the two gates reading the same way.
pub(super) fn static_property_can_be_uninitialized(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
) -> bool {
    let Some(class_name) = static_receiver_class_name(ctx, receiver) else {
        return false;
    };
    let Some(class_info) = ctx.classes.get(class_name.as_str()) else {
        return false;
    };
    if !class_info
        .static_properties
        .iter()
        .any(|(name, _)| name == property)
    {
        return false;
    }
    // Whether the slot was DECLARED with a type, asked of the schema rather than inferred from
    // the stored `PhpType`. `public static mixed $s;` is declared AND stores `Mixed`, so a test
    // on the representation read it as untyped and `S::$s ?? "d"` raised where PHP answers the
    // default — the same confusion the instance predicate above had.
    class_info.declared_static_properties.contains(property)
}

/// Reads a static `property` the way `isset()` does: yields null instead of raising when the
/// slot is still uninitialized.
///
/// The instance twin (`lower_initialized_property_value`) branches on `Op::PropInitialized`.
/// The static path had no such operation — its guard is emitted straight into the read — so
/// `S::$s ?? "d"` raised where PHP answers the default. `Op::StaticPropInitialized` is that
/// operation; the probe it lowers to already existed for Reflection and only needed a
/// visibility-enforcing entry point.
///
/// There is no receiver to own or release here, which is the whole difference from the
/// instance form.
pub(super) fn lower_initialized_static_property_value(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
    expr: &Expr,
) -> LoweredValue {
    let temp_name = ctx.declare_hidden_temp(PhpType::Mixed);
    let uninitialized_block = ctx
        .builder
        .create_named_block("coalesce.static_property.uninitialized", Vec::new());
    let read_block = ctx
        .builder
        .create_named_block("coalesce.static_property.read", Vec::new());
    let merge = ctx
        .builder
        .create_named_block("coalesce.static_property.merge", Vec::new());
    let name = format!("{}::{}", receiver_name(receiver), property);
    let data = ctx.intern_string(&name);
    let initialized = ctx.emit_value(
        Op::StaticPropInitialized,
        Vec::new(),
        Some(Immediate::Data(data)),
        PhpType::Bool,
        Op::StaticPropInitialized.default_effects(),
        Some(expr.span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: initialized.value,
        then_target: read_block,
        then_args: Vec::new(),
        else_target: uninitialized_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(uninitialized_block);
    let null_value = lower_boxed_null(ctx, expr);
    store_value_into_temp(ctx, &temp_name, PhpType::Mixed, null_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(read_block);
    let read_value = lower_static_property_get(ctx, receiver, property, expr);
    // Both arms store into one Mixed temporary, so a slot that is not already boxed has to be.
    let read_value = if matches!(
        ctx.builder.value_php_type(read_value.value).codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        read_value
    } else {
        ctx.emit_value(
            Op::MixedBox,
            vec![read_value.value],
            None,
            PhpType::Mixed,
            Op::MixedBox.default_effects(),
            Some(expr.span),
        )
    };
    store_value_into_temp(ctx, &temp_name, PhpType::Mixed, read_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    take_owned_temp(ctx, &temp_name, expr.span)
}

/// Returns the class name and nullability if `php_type` is a single object type (optionally
/// nullable). Heterogeneous unions and non-object types return `None`.
pub(super) fn singular_object_class(php_type: &PhpType) -> Option<(&str, bool)> {
    match php_type {
        PhpType::Object(name) => Some((name.as_str(), false)),
        PhpType::Union(members) => {
            let mut found = None;
            let mut nullable = false;
            for member in members {
                match member {
                    PhpType::Void => nullable = true,
                    PhpType::Object(name) => {
                        if found.is_some_and(|existing| existing != name.as_str()) {
                            return None;
                        }
                        found = Some(name.as_str());
                    }
                    _ => return None,
                }
            }
            found.map(|class_name| (class_name, nullable))
        }
        _ => None,
    }
}

/// Returns precise runtime storage types for inherited SPL callback-filter internals.
pub(super) fn runtime_property_type_override(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    property: &str,
) -> Option<PhpType> {
    if !class_extends_class(ctx, class_name, "CallbackFilterIterator") {
        return None;
    }
    match property {
        "callback" => Some(PhpType::Callable),
        "callbackEnv" => Some(PhpType::Pointer(None)),
        _ => None,
    }
}

/// Returns true when a class is or extends the target class.
pub(super) fn class_extends_class(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    target_class: &str,
) -> bool {
    let target_key = php_symbol_key(target_class);
    let mut current = Some(class_name.trim_start_matches('\\').to_string());
    while let Some(name) = current {
        if php_symbol_key(&name) == target_key {
            return true;
        }
        current = ctx
            .classes
            .get(name.as_str())
            .and_then(|class_info| class_info.parent.clone());
    }
    false
}

/// Casts a runtime property name to the `Str` pair the backend property ladders index by.
///
/// PHP resolves `$o->{$e}` through a string cast of `$e`, so any scalar name is legal source.
/// The checker cannot always hand the backend a narrowed one: an `eval()` anywhere in a scope
/// widens every local there to `Mixed`, which is how `$o->{$name}` after `eval('...')` reached
/// codegen boxed and was refused outright. Casting here keeps one string-name contract for the
/// get, set and unset ladders, and leaves an already-`Str` name untouched so its producer stays
/// visible to the owned-temporary cleanup.
pub(super) fn coerce_runtime_property_name(
    ctx: &mut LoweringContext<'_, '_>,
    property: LoweredValue,
    span: Span,
) -> LoweredValue {
    if ctx.builder.value_php_type(property.value).codegen_repr() == PhpType::Str {
        return property;
    }
    let name = ctx.emit_value(
        Op::Cast,
        vec![property.value],
        Some(Immediate::CastTarget(IrType::Str)),
        PhpType::Str,
        Op::Cast.default_effects(),
        Some(span),
    );
    release_coerced_source_if_owned(ctx, property, Some(span));
    name
}

/// Lowers a dynamic property read.
pub(super) fn lower_dynamic_property_get(ctx: &mut LoweringContext<'_, '_>, object: &Expr, property: &Expr, expr: &Expr) -> LoweredValue {
    lower_dynamic_property_fetch(ctx, object, property, PropertyFetchMode::Read, expr)
}

/// Lowers a dynamic property fetch in php's read or silent-probe mode.
pub(super) fn lower_dynamic_property_fetch(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &Expr,
    mode: PropertyFetchMode,
    expr: &Expr,
) -> LoweredValue {
    let object = lower_expr(ctx, object);
    lower_dynamic_property_fetch_from_value(ctx, object, property, mode, expr)
}

/// Lowers a dynamic property fetch, in either mode, once the receiver is already evaluated.
///
/// The mode travels on the instruction because php's answer for a name this scope may not reach,
/// or that has never been created, depends on it and on nothing the backend can recover later:
/// a value read raises `Cannot access private property D::$n` or warns `Undefined property`,
/// while `isset()`, `empty()` and `??` answer `null` in silence.
///
/// The mode is NOT an effect narrowing. It suppresses php's own access and miss diagnostics only;
/// a probe still reaches `__isset`, `__get` and property hooks, which are user code that can throw
/// or warn. php 8.5 propagates an exception thrown from `__isset` straight out of `isset()`, so
/// both modes keep the opcode's conservative contract.
pub(super) fn lower_dynamic_property_fetch_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &Expr,
    mode: PropertyFetchMode,
    expr: &Expr,
) -> LoweredValue {
    let result_type = dynamic_property_get_result_type(ctx, object.value, property, expr);
    let property = lower_expr(ctx, property);
    let property = coerce_runtime_property_name(ctx, property, expr.span);
    let result = ctx.emit_value(
        Op::DynamicPropGet,
        vec![object.value, property.value],
        Some(Immediate::PropertyFetchMode(mode)),
        result_type,
        Op::DynamicPropGet.default_effects(),
        Some(expr.span),
    );
    stabilize_borrowed_result_and_release_receiver(ctx, object, result, expr.span)
}

/// Lowers `$object->name` as a silent probe by reusing the runtime-name fetch with a literal name.
///
/// `Op::PropGet` already spends its immediate on the property name, so the probe distinction
/// cannot ride along with it. A literal-name `DynamicPropGet` reaches exactly the same backend
/// ladders (`lower_const_dynamic_prop_get` dispatches to the same stdClass, magic, hash and
/// declared-slot lowerings `lower_prop_get_nonnull` does) with the same result type, ownership
/// and effects, so routing the probe through it keeps one set of read semantics.
pub(super) fn lower_property_probe_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    expr: &Expr,
) -> LoweredValue {
    let name = Expr::new(ExprKind::StringLiteral(property.to_string()), expr.span);
    lower_dynamic_property_fetch_from_value(ctx, object, &name, PropertyFetchMode::Probe, expr)
}

/// Returns whether `$object->name` must be probed through the runtime-name form.
///
/// Only a receiver whose class is unknown at compile time needs it: `property_isset_action`
/// answers for every singular object class, and a name the checker refused never reaches
/// lowering. A boxed `Mixed` receiver has neither, so its declared-slot ladder is the one place
/// where a probe would otherwise take the raising value-read arm.
pub(super) fn property_probe_needs_runtime_name_form(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
) -> bool {
    if isset_object_expr_class(ctx, object).is_some() {
        return false;
    }
    property_probe_needs_runtime_name_form_for_type(&expr_receiver_type_for_probe(ctx, object))
}

/// The receiver-TYPE half of the decision above, for a receiver already lowered to a value.
///
/// A nullable or union receiver that still resolves to one object class keeps the ordinary named
/// read: its declared slot is known and the checker has already ruled on the name.
pub(super) fn property_probe_needs_runtime_name_form_for_type(receiver_ty: &PhpType) -> bool {
    singular_object_class(receiver_ty).is_none()
        && matches!(
            receiver_ty.codegen_repr(),
            PhpType::Mixed | PhpType::Union(_)
        )
}

/// Returns the lowering-visible PHP type of a probe receiver expression.
fn expr_receiver_type_for_probe(ctx: &LoweringContext<'_, '_>, object: &Expr) -> PhpType {
    match &object.kind {
        ExprKind::Variable(name) => ctx.local_type(name),
        _ => infer_expr_type_syntactic(object),
    }
}

/// Returns precise metadata for dynamic property reads when class slots are statically known.
pub(super) fn dynamic_property_get_result_type(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    property: &Expr,
    expr: &Expr,
) -> PhpType {
    if let ExprKind::StringLiteral(name) = &property.kind {
        return property_get_result_type(ctx, object, name, Op::DynamicPropGet, expr);
    }
    let object_ty = ctx.builder.value_php_type(object);
    if matches!(object_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        return PhpType::Mixed;
    }
    let Some((class_name, nullable)) = singular_object_class(&object_ty) else {
        return fallback_expr_type(expr);
    };
    let nullable = nullable || value_may_carry_container_miss(ctx, object);
    let normalized = class_name.trim_start_matches('\\');
    if is_builtin_stdclass_name(normalized) {
        return if nullable {
            nullable_result_type(PhpType::Mixed)
        } else {
            PhpType::Mixed
        };
    }
    let Some(class_info) = ctx.classes.get(normalized) else {
        return fallback_expr_type(expr);
    };
    // A class with a per-instance property hash can answer a name no declaration mentions, so the
    // declared-slot union below would describe the wrong storage: the hash holds boxed `mixed`.
    // Reading the union type instead re-interpreted a boxed cell as the single declared slot type,
    // which printed the null sentinel as an int for `clone($plain, ["zz" => "x"])`.
    // A name this scope resolves to a DYNAMIC property is dropped from the backend's declared-slot
    // ladder, so the runtime name can miss and answer php `null`. Typing the read from the
    // remaining declared slots printed the backend's own miss sentinel as an ordinary value.
    if class_info.dynamic_property_hash_is_name_addressable()
        || class_runtime_name_read_can_miss(ctx, normalized)
    {
        return if nullable {
            nullable_result_type(PhpType::Mixed)
        } else {
            PhpType::Mixed
        };
    }
    let members = class_info
        .properties
        .iter()
        .map(|(_, property_ty)| {
            let property_ty = normalize_value_php_type(property_ty.clone());
            if nullable {
                nullable_result_type(property_ty)
            } else {
                property_ty
            }
        })
        .collect::<Vec<_>>();
    normalize_union_members(members).unwrap_or_else(|| fallback_expr_type(expr))
}

/// Returns whether php resolves `property` on `class_name` to a DYNAMIC property in this scope.
///
/// `crate::types::resolve_property_name` is the authority: `ClassInfo::properties` is the
/// PHYSICAL slot table, so it still carries a strict ancestor's private slot under its plain
/// name even though php 7.4 removed shadow properties and the child's by-name table no longer
/// contains it. The lowering scope is the same one `ir_can_access_member` uses.
pub(super) fn property_name_is_dynamic_in_scope(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    property: &str,
) -> bool {
    crate::types::resolve_property_name(
        ctx.classes,
        class_name.trim_start_matches('\\'),
        property,
        ctx.current_class.as_deref(),
    ) == crate::types::PropertyNameResolution::Dynamic
}

/// Returns whether a RUNTIME name on `class_name` can miss every slot the backend will dispatch.
///
/// A name this scope resolves to a dynamic property answers from the per-instance hash, and one
/// php refuses is dropped from a silent probe's ladder. Both reach the ladder's miss arm, whose
/// answer is php `null`, so a result type built only from the remaining declared slots would
/// describe storage the read never produced.
fn class_runtime_name_read_can_miss(ctx: &LoweringContext<'_, '_>, class_name: &str) -> bool {
    let normalized = class_name.trim_start_matches('\\');
    let Some(class_info) = ctx.classes.get(normalized) else {
        return false;
    };
    class_info.properties.iter().any(|(name, _)| {
        !matches!(
            crate::types::resolve_property_name(
                ctx.classes,
                normalized,
                name,
                ctx.current_class.as_deref(),
            ),
            crate::types::PropertyNameResolution::Visible
                | crate::types::PropertyNameResolution::ScopePrivate { .. }
        )
    })
}

/// Returns true when the normalized class name refers to PHP's builtin stdClass.
pub(super) fn is_builtin_stdclass_name(class_name: &str) -> bool {
    crate::types::checker::builtin_stdclass::is_stdclass(class_name)
}

/// Flattens and deduplicates union candidates, with `Mixed` absorbing all members.
pub(super) fn normalize_union_members(members: Vec<PhpType>) -> Option<PhpType> {
    let mut deduped = Vec::new();
    for member in members {
        match member {
            PhpType::Union(inner) => {
                for inner_member in inner {
                    if inner_member == PhpType::Mixed {
                        return Some(PhpType::Mixed);
                    }
                    if !deduped.iter().any(|existing| existing == &inner_member) {
                        deduped.push(inner_member);
                    }
                }
            }
            PhpType::Mixed => return Some(PhpType::Mixed),
            other => {
                if !deduped.iter().any(|existing| existing == &other) {
                    deduped.push(other);
                }
            }
        }
    }
    match deduped.len() {
        0 => None,
        1 => deduped.pop(),
        _ => Some(PhpType::Union(deduped)),
    }
}

/// Lowers a static property read.
pub(super) fn lower_static_property_get(ctx: &mut LoweringContext<'_, '_>, receiver: &StaticReceiver, property: &str, expr: &Expr) -> LoweredValue {
    let name = format!("{}::{}", receiver_name(receiver), property);
    let data = ctx.intern_string(&name);
    let result_type = static_property_result_type(ctx, receiver, property, expr);
    ctx.emit_value(
        Op::LoadStaticProperty,
        Vec::new(),
        Some(Immediate::Data(data)),
        result_type,
        Op::LoadStaticProperty.default_effects(),
        Some(expr.span),
    )
}

/// Returns precise PHP metadata for a static property read when class metadata is available.
pub(crate) fn static_property_result_type(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
    _expr: &Expr,
) -> PhpType {
    let Some(class_name) = static_receiver_class_name(ctx, receiver) else {
        return PhpType::Mixed;
    };
    let Some(class_info) = ctx.classes.get(class_name.as_str()) else {
        return PhpType::Mixed;
    };
    let Some((_, property_ty)) = class_info
        .static_properties
        .iter()
        .find(|(name, _)| name == property)
    else {
        return PhpType::Mixed;
    };
    normalize_value_php_type(property_ty.codegen_repr())
}
