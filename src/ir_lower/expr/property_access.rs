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
    let data = ctx.intern_string(property);
    let cell_ptr = ctx.emit_value(
        Op::LoadPropRefCell,
        vec![object.value],
        Some(Immediate::Data(data)),
        value_type.clone(),
        Op::LoadPropRefCell.default_effects(),
        Some(span),
    );
    ctx.bind_local_ref_cell_ptr(target, cell_ptr, value_type, Some(span));
}

/// Lowers `$target = &call()`: binds `$target` to the reference cell returned by a
/// by-reference-returning callee. The call yields the cell pointer; the target shares it
/// non-owning (the owner is the object property the callee returned a reference to).
pub(crate) fn lower_ref_assign_call(
    ctx: &mut LoweringContext<'_, '_>,
    target: &str,
    source: &Expr,
    span: Span,
) {
    let cell_ptr = lower_expr(ctx, source);
    let value_type = ctx.builder.value_php_type(cell_ptr.value);
    ctx.bind_local_ref_cell_ptr(target, cell_ptr, value_type, Some(span));
}

/// Lowers `$target =& $arr[idx]`: promotes the indexed-array element's inline storage to a
/// reference cell and binds `$target` to it non-owning. The returned cell pointer addresses
/// the element within the array payload, so writes through `$target` propagate to `$arr[idx]`
/// and vice versa. The array must remain live while the alias is in use (the local does not
/// own the storage). Operands: the lowered array value and the lowered index value.
pub(crate) fn lower_ref_assign_array_elem(
    ctx: &mut LoweringContext<'_, '_>,
    target: &str,
    source: &Expr,
    span: Span,
) {
    let ExprKind::ArrayAccess { array, index } = &source.kind else {
        return;
    };
    let array_value = lower_expr(ctx, array);
    let mut index_value = lower_expr(ctx, index);
    index_value = coerce_to_int_at_span(ctx, index_value, Some(index.span));
    // Use the array's declared element type (the inline storage shape), not the
    // null-capable `TaggedScalar` result type that `array_access_result_type` widens
    // Int elements to. The ref-cell aliases the raw element slot, so loads and stores
    // through the alias must match the element's storage width, not the read result.
    let value_type = match ctx.builder.value_php_type(array_value.value).codegen_repr() {
        PhpType::Array(elem_ty) => normalize_value_php_type(*elem_ty),
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
    ctx.bind_local_ref_cell_ptr(target, cell_ptr, value_type, Some(span));
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
    if op == Op::PropGet {
        let object_type = ctx.builder.value_php_type(object.value);
        if let Some(wrapper_result_type) =
            crate::ir_lower::internal_extensions::simplexml_object_result_type(ctx, &object_type)
        {
            let opcode = crate::ir_lower::internal_extensions::simplexml_object_handler_opcode_for_type(
                ctx,
                &object_type,
                "read_property",
            )
            .expect("SimpleXML wrapper types have a locked read_property handler");
            let name = lower_string_literal(ctx, property, expr);
            let read_mode = lower_int_literal(ctx, 0, expr);
            let property_address = lower_bool_literal(ctx, false, expr);
            let result = crate::ir_lower::internal_extensions::emit_call(
                ctx,
                opcode,
                crate::ir_lower::internal_extensions::FLAG_RECEIVER
                    | crate::ir_lower::internal_extensions::FLAG_WRAPPER_RESULT,
                vec![object.value, name.value, read_mode.value, property_address.value],
                wrapper_result_type,
                expr.span,
            );
            return stabilize_borrowed_result_and_release_receiver(ctx, object, result, expr.span);
        }
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
    if op == Op::PropGet {
        let object_type = ctx.builder.value_php_type(object.value);
        if let Some(opcode) = crate::ir_lower::internal_extensions::property_opcode_for_type(
            ctx,
            &object_type,
            property,
            false,
        ) {
            let result_type = property_get_result_type(ctx, object.value, property, op, expr);
            let result = crate::ir_lower::internal_extensions::emit_call(
                ctx,
                opcode,
                crate::ir_lower::internal_extensions::FLAG_RECEIVER
                    | internal_extension_result_flags(&result_type),
                vec![object.value],
                result_type,
                expr.span,
            );
            return stabilize_borrowed_result_and_release_receiver(ctx, object, result, expr.span);
        }
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

/// Reads one SimpleXML property as an addressable nested-assignment parent.
///
/// The object handler's third operand asks the bridge to materialize a missing
/// named child before a following dimension or property write. This is distinct
/// from an ordinary property read, which deliberately preserves an empty view.
pub(crate) fn lower_simplexml_property_read_for_write_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: LoweredValue,
    property: &str,
    expr: &Expr,
    append_target: bool,
) -> LoweredValue {
    let receiver_type = ctx.builder.value_php_type(receiver.value);
    let opcode = crate::ir_lower::internal_extensions::simplexml_object_handler_opcode_for_type(
        ctx,
        &receiver_type,
        "read_property",
    )
    .expect("SimpleXML property write lowering requires the locked read handler");
    let wrapper_type = crate::ir_lower::internal_extensions::simplexml_object_result_type(
        ctx,
        &receiver_type,
    )
    .expect("SimpleXML property write lowering requires one exact wrapper class");
    let name = lower_string_literal(ctx, property, expr);
    let read_mode = lower_int_literal(ctx, 1, expr);
    let property_address = lower_bool_literal(ctx, true, expr);
    let append_target = lower_bool_literal(ctx, append_target, expr);
    let result = crate::ir_lower::internal_extensions::emit_call(
        ctx,
        opcode,
        crate::ir_lower::internal_extensions::FLAG_RECEIVER
            | crate::ir_lower::internal_extensions::FLAG_WRAPPER_RESULT,
        vec![
            receiver.value,
            name.value,
            read_mode.value,
            property_address.value,
            append_target.value,
        ],
        wrapper_type,
        expr.span,
    );
    stabilize_borrowed_result_and_release_receiver(ctx, receiver, result, expr.span)
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
    let nullable = nullable || value_may_carry_container_miss(ctx, object);
    let normalized = class_name.trim_start_matches('\\');
    if crate::ir_lower::internal_extensions::is_simplexml_element_class(ctx, normalized) {
        let property_ty = PhpType::Object(class_name.to_string());
        return if nullable {
            nullable_result_type(property_ty)
        } else {
            property_ty
        };
    }
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
    let Some((_, (_, property_ty))) = class_info.visible_property(property) else {
        if let Some(magic_ty) = magic_get_result_type(ctx, normalized) {
            return if nullable {
                nullable_result_type(magic_ty)
            } else {
                magic_ty
            };
        }
        if class_info.allow_dynamic_properties {
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
/// A property is uninitialized only while it is DECLARED WITH A TYPE and has no default:
/// `public ?P $p;` and `public string $s;` both start uninitialized, and PHP fatals on a plain
/// read of either. A default makes the slot live before the constructor body runs, and an
/// untyped property is plain null, so neither can ever be in that state — which is what keeps
/// this gate off the overwhelmingly common shapes.
///
/// The one case it misses is `unset($this->s)`, which returns an already-initialized typed
/// property to the uninitialized state in PHP.
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
    let Some(index) = info.properties.iter().position(|(name, _)| name == property) else {
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

/// Lowers a dynamic property read.
pub(super) fn lower_dynamic_property_get(ctx: &mut LoweringContext<'_, '_>, object: &Expr, property: &Expr, expr: &Expr) -> LoweredValue {
    let object = lower_expr(ctx, object);
    lower_dynamic_property_get_from_value(ctx, object, property, expr)
}

/// Lowers a dynamic property read once the receiver is already evaluated.
pub(super) fn lower_dynamic_property_get_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &Expr,
    expr: &Expr,
) -> LoweredValue {
    let result_type = dynamic_property_get_result_type(ctx, object.value, property, expr);
    let property = lower_expr(ctx, property);
    let result = ctx.emit_value(
        Op::DynamicPropGet,
        vec![object.value, property.value],
        None,
        result_type,
        Op::DynamicPropGet.default_effects(),
        Some(expr.span),
    );
    stabilize_borrowed_result_and_release_receiver(ctx, object, result, expr.span)
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
pub(super) fn static_property_result_type(
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
