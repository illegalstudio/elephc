//! Purpose:
//! Lowers safe instance-property value probes for coalesce and isset.
//!
//! Called from:
//! - `super::lazy_branches` and runtime-name probes in `super::property_access`.
//! - Nullsafe-chain initialization decisions through the expression module.
//!
//! Key details:
//! - Boxed single-class unions verify the object tag before probing a declared slot.
//! - Virtual getters use object-presence guards, never their uninitialized backing slots.
//! - Owned merge storage is cleared when consumed; unboxed getter receivers own their lease.

use super::*;

/// Returns true when reading `property` on `object` can hit PHP's
/// "must not be accessed before initialization" fatal.
///
/// A property is uninitialized while it is DECLARED WITH A TYPE and has no default:
/// `public ?P $p;` and `public string $s;` both start uninitialized, and PHP fatals on a plain
/// read of either. A default makes the slot live before the constructor body runs, and an
/// untyped property is plain null, so neither can ever be in that state, which is what keeps
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
    let Some(class_name) = property_probe_object_class(&object_ty) else {
        return false;
    };
    let Some(info) = ctx.classes.get(class_name) else {
        return false;
    };
    // The backend's boxed PropInitialized probe checks the object tag before inspecting
    // a slot. C|false therefore has the same safe missing-value arm as C|null. Opaque Mixed
    // and multiple-class unions keep their runtime-name probe. Inaccessible names also stay
    // on that silent probe rather than turning a coalesce read into a visibility Error.
    if singular_object_class(&object_ty).is_none()
        && !property_is_accessible_for_ir(ctx, class_name, info, property) { return false; }
    // A get-HOOKED property has no slot to probe: its value comes from the synthetic accessor
    // that `lower_property_get_from_value` routes to, and the backing slot behind it is
    // legitimately uninitialized. Probing it answers "not initialized" and sends `??` to its
    // default, so `$p?->full ?? "(none)"` on a real object answered `(none)` instead of running
    // the hook. Inside the accessor itself `$this->full` IS the raw slot, which is the one place
    // the probe applies, the same exception the read makes.
    if info.methods.contains_key(&php_symbol_key(&property_hook_get_method(property)))
        && !ctx.in_own_property_accessor(property)
    {
        return false;
    }
    let Some((index, (_, property_ty))) = info.visible_property(property) else {
        return false;
    };
    // Whether the slot was DECLARED with a type, asked of the schema, not inferred from the
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

/// Identifies one known object class in a probe union without treating unknown objects as that class.
fn property_probe_object_class(php_type: &PhpType) -> Option<&str> {
    if let Some((class, _)) = singular_object_class(php_type) { return Some(class); }
    let PhpType::Union(members) = php_type else { return None; };
    let mut class = None;
    for member in members {
        match member {
            PhpType::Object(name) => {
                if class.is_some_and(|existing| existing != name.as_str()) { return None; }
                class = Some(name.as_str());
            }
            PhpType::Void | PhpType::False | PhpType::Bool | PhpType::Int
            | PhpType::Float | PhpType::Str | PhpType::Array(_) | PhpType::AssocArray { .. }
            | PhpType::Resource(_) => {}
            _ => return None,
        }
    }
    class
}

/// Selects a known union getter whose raw virtual slot must never be initialization-probed.
pub(super) fn union_hook_probe_class(
    ctx: &LoweringContext<'_, '_>, object: ValueId, property: &str,
) -> Option<String> {
    let ty = ctx.builder.value_php_type(object);
    if singular_object_class(&ty).is_some() || ctx.in_own_property_accessor(property) { return None; }
    let class = property_probe_object_class(&ty)?;
    let info = ctx.classes.get(class)?;
    (property_is_accessible_for_ir(ctx, class, info, property)
        && info.methods.contains_key(&php_symbol_key(&property_hook_get_method(property))))
        .then(|| class.to_string())
}

/// Reads `property` the way `isset()` does: yields null instead of raising when the slot is
/// still uninitialized.
///
/// `??` must not fatal on `$o->p`: PHP answers the default, but the ordinary read does. The
/// initialized-aware read already exists for `isset()`, which produces a BOOLEAN; this is its
/// value-producing twin, also used for guarded union getters. Virtual slots are not probed:
/// the object arm calls its getter through an owned unboxed receiver instead.
pub(super) fn lower_initialized_property_value(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    expr: &Expr,
) -> LoweredValue {
    let hook_class = union_hook_probe_class(ctx, object.value, property);
    // The probe transfers its boxed result to the consumer. Clearing its owner slot
    // prevents a later Error allocation from being released through a stale pointer.
    let temp_name = ctx.declare_owned_hidden_temp(PhpType::Mixed);
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
    // A hooked union probes only whether it holds an object. The getter, not the raw
    // uninitialized backing slot, supplies its value on that arm.
    let (probe_op, probe_immediate) = if hook_class.is_some() {
        (Op::TypePredicate, Immediate::TypePredicate(crate::ir::PhpTypePredicate::Object))
    } else { (Op::PropInitialized, Immediate::Data(data)) };
    let initialized = ctx.emit_value(
        probe_op,
        vec![object.value],
        Some(probe_immediate),
        PhpType::Bool,
        probe_op.default_effects(),
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
    // This path never reads the property, so nothing downstream consumes the receiver. An
    // OWNING one has to be released here. The read path below hands it to
    // `lower_property_get_from_value`, which disposes of it the way an ordinary read does.
    // `mk()->p ?? "none"` leaked one object per call without this.
    //
    // Only an OWNING one: a plain `$c` receiver is BORROWED from its slot, and releasing it
    // hands back a reference this expression never took. `$c->p ??= 42` through a `?C`
    // parameter died with "Attempt to assign property on null": the release freed the boxed
    // receiver, and the write that followed read the freed cell. `guard_initialized_chain_property`
    // gates its own cleanup block the same way.
    if ctx.value_is_owning_temporary(object) {
        crate::ir_lower::ownership::release_if_owned(ctx, object, Some(expr.span));
    }
    let null_value = lower_boxed_null(ctx, expr);
    store_value_into_temp(ctx, &temp_name, PhpType::Mixed, null_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(read_block);
    let read_object = if let Some(class) = hook_class {
        let ty = PhpType::Object(class);
        let unboxed = ctx.emit_owned_value(Op::MixedUnbox, vec![object.value], None,
            ty.clone(), Op::mixed_unbox_effects(&ty), Some(expr.span));
        crate::ir_lower::ownership::release_if_owned(ctx, object, Some(expr.span));
        unboxed
    } else { object };
    let read_value = lower_property_get_from_value(ctx, read_object, property, Op::PropGet, expr);
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
