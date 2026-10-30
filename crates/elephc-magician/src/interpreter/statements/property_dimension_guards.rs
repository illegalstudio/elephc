//! Purpose:
//! Guards ordinary typed property containers before dimension writes can replace their storage.
//!
//! Called from:
//! - Direct property dimension writes and attached nested dimension resolution.
//!
//! Key details:
//! - Native XML selection keeps priority over ordinary declared-property metadata.
//! - Write fetches may inspect uninitialized storage; read-modify-write fetches may not.
//! - Object contents remain mutable through readable readonly or restricted-set properties.

use super::*;

/// Selects PHP write-fetch initialization rules independently of key diagnostic suppression.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum DimensionFetchMode { Write, ReadWrite }

/// Metadata for an accessible ordinary backing slot, excluding accessor hooks.
struct Slot {
    owner: String,
    ty: Option<EvalParameterType>,
    readonly: bool,
    write_visibility: EvalVisibility,
}

/// Reads and validates one ordinary property, returning an independently owned container lease.
pub(super) fn ordinary_property(
    object: RuntimeCellHandle, property: &str, mode: DimensionFetchMode,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let slot = slot(object, property, context, values)?;
    let current = if slot.is_some() && mode == DimensionFetchMode::Write {
        instance_property_access::eval_property_read_result(object, property, context, values, None, true)?
    } else { eval_property_get_result(object, property, context, values)? };
    let current = if current.is_borrowed() { values.retain(current)? } else { current };
    let result = match slot {
        Some(slot) => validate(&slot, property, current, context, values),
        None => Ok(()),
    };
    match result {
        Ok(()) => Ok(current),
        Err(status) => { let _ = eval_release_value(context, values, current); Err(status) }
    }
}

/// Preserves native write selection before applying declaration guards to an ordinary slot.
pub(super) fn selected_property(
    object: RuntimeCellHandle, property: &str, mode: DimensionFetchMode,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    validate_property_write_receiver(object, property, "modify", context, values)?;
    match super::super::runtime_ops::simplexml_properties::invoke(
        values, object, property, elephc_builtin_contract::ObjectProperty::WriteSelect, None,
    )? {
        Some(value) => Ok(value),
        None => ordinary_property(object, property, mode, context, values),
    }
}

/// Resolves lexical private slots and inherited native fields without bypassing accessors.
fn slot(
    object: RuntimeCellHandle, property: &str, context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<Slot>, EvalStatus> {
    let Ok(identity) = values.object_identity(object) else { return Ok(None); };
    let dynamic = context.dynamic_object_class(identity).map(|class| class.name().to_string());
    let class = match &dynamic {
        Some(class) => class.clone(),
        None => eval_runtime_object_class_name(object, values)?,
    };
    if dynamic.is_some() {
        if let Some((owner, declared)) = eval_dynamic_property_for_access(&class, property, context) {
            if declared.is_static() || declared.has_get_hook() || declared.has_set_hook()
                || validate_eval_member_access(&owner, declared.visibility(), context).is_err()
            { return Ok(None); }
            return Ok(Some(Slot { owner, ty: declared.property_type().cloned(),
                readonly: declared.is_readonly(), write_visibility: declared.write_visibility() }));
        }
    }
    let native = if dynamic.is_some() {
        eval_dynamic_class_native_property_metadata(&class, property, context, values)?
    } else {
        eval_native_instance_property_metadata_for_access(&class, &class, property, context, values)?
    };
    let Some((owner, read, write_visibility, false)) = native else { return Ok(None); };
    if validate_eval_member_access(&owner, read, context).is_err() { return Ok(None); }
    for method in [property_hook_get_method(property), property_hook_set_method(property)] {
        if values.reflection_method_flags(&owner, &method)?.is_some_and(|flags|
            flags & EVAL_REFLECTION_METHOD_FLAG_PROPERTY_HOOK != 0) { return Ok(None); }
    }
    let flags = values.reflection_property_flags(&owner, property)?.unwrap_or_default();
    Ok(Some(Slot { ty: context.native_property_type(&owner, property), owner,
        readonly: flags & EVAL_REFLECTION_MEMBER_FLAG_READONLY != 0, write_visibility }))
}

/// Rejects null storage mutation before warning-producing element reads or autovivification.
fn validate(
    slot: &Slot, property: &str, current: RuntimeCellHandle,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let tag = values.type_tag(current)?;
    if tag != EVAL_TAG_NULL { return Ok(()); }
    let owner = slot.owner.trim_start_matches('\\');
    if slot.readonly {
        return eval_throw_error(&format!("Cannot indirectly modify readonly property {owner}::${property}"), context, values);
    }
    if validate_eval_member_access(&slot.owner, slot.write_visibility, context).is_err() {
        let visibility = eval_visibility_label(slot.write_visibility);
        let scope = eval_native_constructor_scope_label(context);
        return eval_throw_error(&format!("Cannot indirectly modify {visibility}(set) property {owner}::${property} from {scope}"), context, values);
    }
    if let Some(ty) = slot.ty.as_ref().filter(|ty| !admits_array(ty)) {
        let ty = type_label(ty, &slot.owner, context)?;
        return eval_throw_type_error(&format!("Cannot auto-initialize an array inside property {owner}::${property} of type {ty}"), context, values);
    }
    Ok(())
}

/// Recognizes PHP array-bearing declarations before a null property becomes an array.
fn admits_array(ty: &EvalParameterType) -> bool {
    !ty.is_intersection() && ty.variants().iter().any(|variant| match variant {
        EvalParameterTypeVariant::Mixed | EvalParameterTypeVariant::Array | EvalParameterTypeVariant::Iterable => true,
        EvalParameterTypeVariant::Generic(ty) => generic_admits_array(ty),
        _ => false,
    })
}

/// Keeps specialized generic arrays and nullable/union wrappers on the array initialization path.
fn generic_admits_array(ty: &elephc_generics::Type) -> bool {
    use elephc_generics::Type;
    match ty {
        Type::Atom(name) | Type::Apply(name, _) => ["array", "iterable", "mixed"].iter()
            .any(|allowed| name.eq_ignore_ascii_case(allowed)),
        Type::Nullable(inner) => generic_admits_array(inner),
        Type::Union(members) => members.iter().any(generic_admits_array),
        _ => false,
    }
}

/// Resolves lexical class names while preserving PHP's nullable shorthand in diagnostics.
fn type_label(ty: &EvalParameterType, owner: &str, context: &ElephcEvalContext) -> Result<String, EvalStatus> {
    let label = eval_null_boundary_type_label(ty, Some(owner), Some(owner), context)?;
    if ty.allows_null() && !ty.is_intersection() {
        let names = label.split('|').map(String::from).collect::<Vec<_>>();
        if names.len() == 1 {
            return Ok(elephc_builtin_contract::reflection_format::named_type(&names[0], true));
        }
        return Ok(elephc_builtin_contract::reflection_format::union(names, true));
    }
    Ok(label)
}
