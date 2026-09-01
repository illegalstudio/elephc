//! Purpose:
//! Validates property storage compatibility and object assignability.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Nullable, Mixed, array, object, and tagged-scalar representations stay conservative.

use super::*;

/// Verifies that this slice knows how to represent the property type in an object slot.
pub(super) fn ensure_property_type_supported(php_type: &PhpType, inst: &Instruction) -> Result<()> {
    match php_type.codegen_repr() {
        PhpType::Bool
        | PhpType::False
        | PhpType::Int
        | PhpType::Float
        | PhpType::Str
        | PhpType::TaggedScalar
        | PhpType::Void
        | PhpType::Never => Ok(()),
        ref ty if is_pointer_sized_property_type(ty) => Ok(()),
        _ => Err(CodegenIrError::unsupported(format!(
            "{} for property PHP type {:?}",
            inst.op.name(),
            php_type
        ))),
    }
}

/// Verifies the assigned value already has the property storage representation.
pub(super) fn ensure_property_value_supported(
    ctx: &FunctionContext<'_>,
    slot: &PropertySlot,
    value: ValueId,
    value_ty: &PhpType,
    inst: &Instruction,
) -> Result<()> {
    if value_ty == &slot.php_type {
        return Ok(());
    }
    if can_store_object_for_object_property(ctx, value_ty, &slot.php_type) {
        return Ok(());
    }
    if is_pointer_sized_property_type(&slot.php_type)
        && is_pointer_slot_null_sentinel(ctx, value, value_ty)?
    {
        return Ok(());
    }
    if is_empty_array_for_array_property(value_ty, &slot.php_type) {
        return Ok(());
    }
    if can_convert_indexed_array_to_mixed_property(value_ty, &slot.php_type) {
        return Ok(());
    }
    if can_store_assoc_array_as_mixed_property(value_ty, &slot.php_type) {
        return Ok(());
    }
    if can_store_value_as_tagged_scalar_property(value_ty, &slot.php_type) {
        return Ok(());
    }
    if can_coerce_scalar_to_int_property(value_ty, &slot.php_type) {
        return Ok(());
    }
    if can_store_class_default_in_refined_null_property(ctx, value_ty, &slot.php_type) {
        return Ok(());
    }
    if can_box_value_for_mixed_property(value_ty, &slot.php_type) {
        return Ok(());
    }
    if can_store_boxed_value_for_mixed_property(value_ty, &slot.php_type) {
        return Ok(());
    }
    if can_coerce_mixed_to_scalar_property(value_ty, &slot.php_type) {
        return Ok(());
    }
    if property_values::can_unbox_mixed_to_object_property(value_ty, &slot.php_type) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} assigning PHP type {:?} to {}::${} with PHP type {:?}",
        inst.op.name(),
        value_ty,
        slot.class_name,
        slot.property,
        slot.php_type
    )))
}

/// Returns true when a concrete object value is assignable to an object-typed property.
pub(super) fn can_store_object_for_object_property(
    ctx: &FunctionContext<'_>,
    value_ty: &PhpType,
    slot_ty: &PhpType,
) -> bool {
    let value_ty = value_ty.codegen_repr();
    let slot_ty = slot_ty.codegen_repr();
    let (PhpType::Object(value_name), PhpType::Object(slot_name)) = (&value_ty, &slot_ty) else {
        return false;
    };
    object_type_is_a(ctx, value_name, slot_name)
}

/// Returns true when `source_name` is the same class/interface or inherits `target_name`.
pub(super) fn object_type_is_a(ctx: &FunctionContext<'_>, source_name: &str, target_name: &str) -> bool {
    if same_php_type_name(source_name, target_name) {
        return true;
    }
    if interface_info_by_name(ctx, target_name).is_some() {
        return object_type_implements_interface(ctx, source_name, target_name);
    }
    class_extends_class(ctx, source_name, target_name)
}

/// Returns true when a class or interface source satisfies an interface target.
pub(super) fn object_type_implements_interface(
    ctx: &FunctionContext<'_>,
    source_name: &str,
    target_interface: &str,
) -> bool {
    if interface_info_by_name(ctx, source_name).is_some() {
        return interface_extends_interface(ctx, source_name, target_interface);
    }
    let mut current = Some(source_name.to_string());
    while let Some(class_name) = current {
        let Some(class_info) = class_info_by_name(ctx, &class_name) else {
            return false;
        };
        if class_info.interfaces.iter().any(|interface_name| {
            interface_extends_interface(ctx, interface_name, target_interface)
        }) {
            return true;
        }
        current = class_info.parent.clone();
    }
    false
}

/// Returns true when an interface is or extends the target interface.
pub(super) fn interface_extends_interface(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
    target_interface: &str,
) -> bool {
    if same_php_type_name(interface_name, target_interface) {
        return true;
    }
    let Some(interface_info) = interface_info_by_name(ctx, interface_name) else {
        return false;
    };
    interface_info
        .parents
        .iter()
        .any(|parent| interface_extends_interface(ctx, parent, target_interface))
}

/// Returns true when a class is or extends the target class.
pub(super) fn class_extends_class(ctx: &FunctionContext<'_>, class_name: &str, target_class: &str) -> bool {
    let mut current = Some(class_name.to_string());
    while let Some(name) = current {
        if same_php_type_name(&name, target_class) {
            return true;
        }
        current = class_info_by_name(ctx, &name).and_then(|class_info| class_info.parent.clone());
    }
    false
}

/// Finds class metadata by PHP-case-insensitive name.
pub(super) fn class_info_by_name<'a>(ctx: &'a FunctionContext<'_>, class_name: &str) -> Option<&'a ClassInfo> {
    let wanted = php_symbol_key(class_name.trim_start_matches('\\'));
    ctx.module
        .class_infos
        .iter()
        .find(|(name, _)| php_symbol_key(name.trim_start_matches('\\')) == wanted)
        .map(|(_, info)| info)
}

/// Finds interface metadata by PHP-case-insensitive name.
pub(super) fn interface_info_by_name<'a>(
    ctx: &'a FunctionContext<'_>,
    interface_name: &str,
) -> Option<&'a InterfaceInfo> {
    let wanted = php_symbol_key(interface_name.trim_start_matches('\\'));
    ctx.module
        .interface_infos
        .iter()
        .find(|(name, _)| php_symbol_key(name.trim_start_matches('\\')) == wanted)
        .map(|(_, info)| info)
}

/// Compares class/interface names using PHP's case-insensitive symbol rules.
pub(super) fn same_php_type_name(left: &str, right: &str) -> bool {
    php_symbol_key(left.trim_start_matches('\\')) == php_symbol_key(right.trim_start_matches('\\'))
}

/// Returns true when a concrete value can be boxed into Mixed-shaped property storage.
pub(super) fn can_box_value_for_mixed_property(value_ty: &PhpType, slot_ty: &PhpType) -> bool {
    slot_ty.codegen_repr() == PhpType::Mixed && value_ty.codegen_repr() != PhpType::Mixed
}

/// Returns true when a boxed Mixed/Union value already matches Mixed-shaped property storage.
pub(super) fn can_store_boxed_value_for_mixed_property(value_ty: &PhpType, slot_ty: &PhpType) -> bool {
    matches!(value_ty.codegen_repr(), PhpType::Mixed)
        && matches!(slot_ty.codegen_repr(), PhpType::Mixed)
}

/// Returns true when a boxed Mixed value can be coerced before a scalar typed-property store.
pub(super) fn can_coerce_mixed_to_scalar_property(value_ty: &PhpType, slot_ty: &PhpType) -> bool {
    matches!(value_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
        && matches!(
            slot_ty.codegen_repr(),
            PhpType::Int | PhpType::Bool | PhpType::Float | PhpType::Str
        )
}

/// Returns true when a value can materialize nullable-int tagged-scalar property storage.
pub(super) fn can_store_value_as_tagged_scalar_property(value_ty: &PhpType, slot_ty: &PhpType) -> bool {
    if slot_ty.codegen_repr() != PhpType::TaggedScalar {
        return false;
    }
    matches!(
        value_ty.codegen_repr(),
        PhpType::Int
            | PhpType::Bool
            | PhpType::Callable
            | PhpType::Void
            | PhpType::Never
            | PhpType::TaggedScalar
            | PhpType::Mixed
            | PhpType::Union(_)
    )
}

/// Returns true when an inline boolean or nullable tagged scalar can use int property storage.
pub(super) fn can_coerce_scalar_to_int_property(value_ty: &PhpType, slot_ty: &PhpType) -> bool {
    matches!(
        value_ty.codegen_repr(),
        PhpType::Bool | PhpType::False | PhpType::TaggedScalar
    ) && slot_ty.codegen_repr() == PhpType::Int
}

/// Returns true when a class default initializer writes into an untyped property later refined to null.
pub(super) fn can_store_class_default_in_refined_null_property(
    ctx: &FunctionContext<'_>,
    value_ty: &PhpType,
    slot_ty: &PhpType,
) -> bool {
    if !ctx.function.name.starts_with("_class_propinit_") {
        return false;
    }
    if slot_ty.codegen_repr() != PhpType::Void {
        return false;
    }
    matches!(value_ty.codegen_repr(), PhpType::Int | PhpType::Bool)
}

/// Returns true when an empty array literal initializes a typed array property.
pub(super) fn is_empty_array_for_array_property(value_ty: &PhpType, slot_ty: &PhpType) -> bool {
    matches!(
        (value_ty, slot_ty),
        (PhpType::Array(elem_ty), PhpType::Array(_))
            if matches!(elem_ty.as_ref(), PhpType::Never | PhpType::Void)
    )
}

/// Returns true when an indexed array can be widened into array<Mixed> storage.
pub(super) fn can_convert_indexed_array_to_mixed_property(value_ty: &PhpType, slot_ty: &PhpType) -> bool {
    let (PhpType::Array(value_elem), PhpType::Array(slot_elem)) =
        (value_ty.codegen_repr(), slot_ty.codegen_repr())
    else {
        return false;
    };
    slot_elem.codegen_repr() == PhpType::Mixed && value_elem.codegen_repr() != PhpType::Mixed
}

/// Returns true when associative-array storage can satisfy a generic `array` property.
pub(super) fn can_store_assoc_array_as_mixed_property(value_ty: &PhpType, slot_ty: &PhpType) -> bool {
    let PhpType::AssocArray { .. } = value_ty.codegen_repr() else {
        return false;
    };
    match slot_ty.codegen_repr() {
        PhpType::Array(slot_elem) => slot_elem.codegen_repr() == PhpType::Mixed,
        PhpType::AssocArray { value, .. } => value.codegen_repr() == PhpType::Mixed,
        _ => false,
    }
}

/// Returns true when a value can initialize a pointer-sized slot as null.
pub(super) fn is_pointer_slot_null_sentinel(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    value_ty: &PhpType,
) -> Result<bool> {
    if matches!(value_ty, PhpType::Void) {
        return Ok(true);
    }
    if !matches!(value_ty, PhpType::Int) {
        return Ok(false);
    }
    let metadata = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = metadata.def else {
        return Ok(false);
    };
    let instruction = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    Ok(instruction.op == Op::ConstI64 && instruction.immediate == Some(Immediate::I64(0)))
}
