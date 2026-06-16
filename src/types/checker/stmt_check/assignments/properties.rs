//! Purpose:
//! Type-checks assignment properties forms.
//! Updates type environments and validates storage-specific rules for locals, arrays, and properties.
//!
//! Called from:
//! - `crate::types::checker::stmt_check::assignments`
//!
//! Key details:
//! - Assignment checking must distinguish value writes, by-reference mutation, nullable access, and declared property contracts.

use crate::errors::CompileError;
use crate::names::{php_symbol_key, property_hook_get_method, property_hook_set_method};
use crate::parser::ast::Expr;
use crate::span::Span;
use crate::types::{
    merge_array_key_types, normalized_array_key_type, static_array_key_forces_hash_storage,
    PhpType, TypeEnv,
};

use super::super::super::Checker;
use super::properties_null_coalesce::null_coalesce_property_keeps_non_null;

/// Type-checks a direct property assignment (`$obj->prop = value`).
///
/// Infers the object and value types, then validates write access for `Object` and `Pointer` types.
/// For objects, also refines the property's inferred type based on the assigned value.
pub(super) fn check_property_assign(
    checker: &mut Checker,
    object: &Expr,
    property: &str,
    value: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let obj_ty = checker.infer_type_with_assignment_effects(object, env)?;
    let val_ty = checker.infer_type_with_assignment_effects(value, env)?;
    if let PhpType::Object(class_name) = &obj_ty {
        check_object_property_write(checker, object, class_name, property, value, &val_ty, span)?;
        refine_object_property_type(checker, class_name, property, &val_ty);
    }
    if let PhpType::Pointer(Some(class_name)) = &obj_ty {
        check_pointer_property_write(checker, class_name, property, &val_ty, span)?;
    }
    Ok(())
}

/// Type-checks an array-push property operation (`$obj->prop[] = value`).
///
/// Infers object and value types, then validates that the property is an array (not a buffer).
/// For `PhpType::Object`, resolves the property type and computes the updated array element type,
/// merging element types if the property has no declared type.
pub(super) fn check_property_array_push(
    checker: &mut Checker,
    object: &Expr,
    property: &str,
    value: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let obj_ty = checker.infer_type_with_assignment_effects(object, env)?;
    let val_ty = checker.infer_type_with_assignment_effects(value, env)?;
    match &obj_ty {
        PhpType::Object(class_name) => {
            let (prop_ty, property_has_declared_type) =
                resolve_object_array_property(checker, class_name, property, span)?;
            let updated_prop_ty = updated_array_property_push_type(
                checker,
                &prop_ty,
                property_has_declared_type,
                class_name,
                property,
                &val_ty,
                span,
            )?;
            update_object_property_type(
                checker,
                class_name,
                property,
                property_has_declared_type,
                updated_prop_ty,
            );
            Ok(())
        }
        PhpType::Pointer(Some(class_name)) => {
            let field_ty = resolve_pointer_field_type(checker, class_name, property, span, "Array push")?;
            match field_ty {
                PhpType::Array(_) => Ok(()),
                PhpType::Buffer(_) => Err(CompileError::new(
                    span,
                    "buffer<T> does not support push; allocate with buffer_new<T>(len)",
                )),
                other => Err(CompileError::new(
                    span,
                    &format!("Array push requires an array property, got {}", other),
                )),
            }
        }
        _ => Err(CompileError::new(
            span,
            "Array push requires an object or typed pointer",
        )),
    }
}

/// Type-checks an indexed property array assignment (`$obj->prop[$index] = value`).
///
/// Infers object, index, and value types. Validates array key types and property mutability.
/// For `PhpType::Object`, handles both `Array` and `AssocArray` storage, merging element types
/// when the property lacks a declared type. For `PhpType::Pointer`, requires integer indexing
/// and that the field is an array type.
pub(super) fn check_property_array_assign(
    checker: &mut Checker,
    object: &Expr,
    property: &str,
    index: &Expr,
    value: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let obj_ty = checker.infer_type_with_assignment_effects(object, env)?;
    let idx_ty = checker.infer_type_with_assignment_effects(index, env)?;
    let normalized_idx_ty = normalized_array_key_type(index, idx_ty.clone());
    let val_ty = checker.infer_type_with_assignment_effects(value, env)?;
    if matches!(obj_ty, PhpType::Mixed) {
        if !is_php_array_key_type(&normalized_idx_ty) {
            return Err(CompileError::new(span, "Array index must be integer"));
        }
        return Ok(());
    }
    match &obj_ty {
        PhpType::Object(class_name) => {
            let (prop_ty, property_has_declared_type) =
                resolve_object_array_property(checker, class_name, property, span)?;
            if let PhpType::Object(prop_class_name) = &prop_ty {
                if checker.object_type_implements_interface(prop_class_name, "ArrayAccess") {
                    return Ok(());
                }
            }
            if !is_php_array_key_type(&normalized_idx_ty) {
                return Err(CompileError::new(span, "Array index must be integer"));
            }

            let updated_prop_ty = updated_array_property_assign_type(
                checker,
                &prop_ty,
                property_has_declared_type,
                class_name,
                property,
                index,
                &normalized_idx_ty,
                &val_ty,
                span,
            )?;
            update_object_property_type(
                checker,
                class_name,
                property,
                property_has_declared_type,
                updated_prop_ty,
            );
            Ok(())
        }
        PhpType::Pointer(Some(class_name)) => {
            let field_ty = resolve_pointer_field_type(
                checker,
                class_name,
                property,
                span,
                "Array index assignment",
            )?;

            if !matches!(normalized_idx_ty, PhpType::Int) {
                return Err(CompileError::new(span, "Array index must be integer"));
            }

            match field_ty {
                PhpType::Array(_) => Ok(()),
                other => Err(CompileError::new(
                    span,
                    &format!(
                        "Array index assignment requires an array property, got {}",
                        other
                    ),
                )),
            }
        }
        _ => Err(CompileError::new(
            span,
            "Array index assignment requires an object or typed pointer",
        )),
    }
}

/// Validates a write to a named property of a class instance.
///
/// Checks: property existence, `__set` magic method fallback, dynamic properties (`#[\AllowDynamicProperties]`),
/// readonly modifier restrictions (disallows writes outside `__construct` except via null-coalesce),
/// visibility via `can_access_member`, and declared-type compatibility via `require_compatible_arg_type`.
/// StdClass properties are allowed unconditionally.
fn check_object_property_write(
    checker: &Checker,
    object: &Expr,
    class_name: &str,
    property: &str,
    value: &Expr,
    val_ty: &PhpType,
    span: Span,
) -> Result<(), CompileError> {
    if crate::types::checker::builtin_stdclass::is_stdclass(class_name) {
        return Ok(());
    }
    if let Some(class_info) = checker.classes.get(class_name) {
        if !class_info.properties.iter().any(|(n, _)| n == property) {
            if class_info.methods.contains_key("__set") {
                return Ok(());
            }
            if class_info.allow_dynamic_properties {
                // PHP 8.2 #[\AllowDynamicProperties]: writes to undeclared
                // properties are routed at codegen time to a per-object
                // hashtable side-table. The value is stored as `Mixed`.
                return Ok(());
            }
            return Err(CompileError::new(
                span,
                &format!("Undefined property: {}::{}", class_name, property),
            ));
        }
        validate_object_property_access(checker, class_name, property, true, span)?;
        let expected_ty = class_info
            .properties
            .iter()
            .find(|(n, _)| n == property)
            .map(|(_, ty)| ty.clone())
            .unwrap_or(PhpType::Int);
        let readonly_non_null_coalesce_keep =
            null_coalesce_property_keeps_non_null(object, property, value, &expected_ty);
        if class_info.readonly_properties.contains(property)
            && !(checker.current_class.as_deref()
                == class_info
                    .property_declaring_classes
                    .get(property)
                    .map(String::as_str)
                && checker.current_method.as_deref() == Some("__construct"))
            && !readonly_non_null_coalesce_keep
        {
            return Err(CompileError::new(
                span,
                &format!(
                    "Cannot assign to readonly property outside constructor: {}::{}",
                    class_name, property
                ),
            ));
        }
        // A property with a `get` hook but no `set` hook is read-only: external writes are an error
        // (PHP rejects writing a virtual/get-only hooked property). Writes from inside the property's
        // own accessor target the raw backing slot and are allowed.
        let has_get_hook = class_info
            .methods
            .contains_key(&php_symbol_key(&property_hook_get_method(property)));
        let has_set_hook = class_info
            .methods
            .contains_key(&php_symbol_key(&property_hook_set_method(property)));
        // `current_method` is stored as a lowercased symbol key, so compare against the lowercased
        // accessor names too — otherwise a mixed-case property (e.g. `$Total`) spuriously trips
        // the read-only check from inside its own accessor.
        let in_own_accessor = checker.current_class.as_deref() == Some(class_name)
            && checker.current_method.as_deref().is_some_and(|method| {
                method == php_symbol_key(&property_hook_get_method(property))
                    || method == php_symbol_key(&property_hook_set_method(property))
            });
        if has_get_hook && !has_set_hook && !in_own_accessor {
            return Err(CompileError::new(
                span,
                &format!(
                    "Cannot write to read-only hooked property {}::{} (it declares a get hook but no set hook)",
                    class_name, property
                ),
            ));
        }
        if class_info.declared_properties.contains(property) {
            checker.require_compatible_arg_type(
                &expected_ty,
                val_ty,
                span,
                &format!("Property {}::${}", class_name, property),
            )?;
        }
    }
    Ok(())
}

/// Validates that the current context can access a named property, checking visibility and class scope.
///
/// Looks up the property's declared class and visibility modifier, then delegates to
/// `Checker::can_access_member` to enforce access control rules.
fn validate_object_property_access(
    checker: &Checker,
    class_name: &str,
    property: &str,
    is_write: bool,
    span: Span,
) -> Result<(), CompileError> {
    let class_info = checker.classes.get(class_name).ok_or_else(|| {
        CompileError::new(span, &format!("Undefined class: {}", class_name))
    })?;
    // A write to a property with PHP 8.4 asymmetric visibility uses its `set` visibility; reads
    // and ordinary properties fall back to the regular (read) visibility.
    let asymmetric_write = if is_write {
        class_info.property_set_visibilities.get(property)
    } else {
        None
    };
    if let Some(visibility) =
        asymmetric_write.or_else(|| class_info.property_visibilities.get(property))
    {
        let declaring_class = class_info
            .property_declaring_classes
            .get(property)
            .map(String::as_str)
            .unwrap_or(class_name);
        if !checker.can_access_member(declaring_class, visibility) {
            return Err(CompileError::new(
                span,
                &format!(
                    "Cannot access {} property: {}::{}",
                    Checker::visibility_label(visibility),
                    class_name,
                    property
                ),
            ));
        }
    }
    Ok(())
}

/// Returns `true` if `ty` is the empty-array placeholder `Array(Never)` produced by an `[]` literal.
///
/// Such a property has no known element type yet, so the first concrete array assignment should
/// adopt the assigned value's element type rather than stay pinned at `Never`.
fn is_empty_array_placeholder(ty: &PhpType) -> bool {
    matches!(ty, PhpType::Array(inner) if matches!(inner.as_ref(), PhpType::Never))
}

/// Refines the inferred type of an object property after a write, for properties without declared types.
///
/// For untyped `Int` or `Void` properties, replaces the type with the assigned value's type.
/// For an `[]`-initialized property (`Array(Never)`), adopts the first concrete array assigned.
/// For generic arrays, calls `Checker::specialize_generic_array_hint` to narrow the element type
/// based on the assigned value. Only updates when the refined type differs from the current type.
fn refine_object_property_type(
    checker: &mut Checker,
    class_name: &str,
    property: &str,
    val_ty: &PhpType,
) {
    if let Some(class_info) = checker.classes.get_mut(class_name) {
        let property_has_declared_type = class_info.declared_properties.contains(property);
        if let Some(prop) = class_info
            .properties
            .iter_mut()
            .find(|(n, _)| n == property)
        {
            if !property_has_declared_type {
                if matches!(prop.1, PhpType::Int | PhpType::Void) && prop.1 != *val_ty {
                    prop.1 = val_ty.clone();
                } else if is_empty_array_placeholder(&prop.1)
                    && matches!(val_ty, PhpType::Array(_) | PhpType::AssocArray { .. })
                {
                    // An `[]` initializer types the property as `Array(Never)`; the
                    // first concrete array assignment fixes the element type, exactly
                    // as a plain local reassignment overwrites its type. Without this,
                    // the element type stays `Never` and codegen mis-emits later reads.
                    prop.1 = val_ty.clone();
                } else {
                    let refined_ty = Checker::specialize_generic_array_hint(&prop.1, val_ty);
                    if refined_ty != prop.1 {
                        prop.1 = refined_ty;
                    }
                }
            }
        }
    }
}

/// Validates a write to a property of a typed pointer (extern or packed class).
///
/// Checks that the field exists in `extern_field_type` or `packed_field_type` and that the
/// assigned value's type is compatible with the field's declared type.
fn check_pointer_property_write(
    checker: &Checker,
    class_name: &str,
    property: &str,
    val_ty: &PhpType,
    span: Span,
) -> Result<(), CompileError> {
    if let Some(field_ty) = checker.extern_field_type(class_name, property) {
        if field_ty == PhpType::Int && val_ty != &PhpType::Int {
            return Err(CompileError::new(
                span,
                &format!(
                    "Type error: cannot assign {:?} to extern field {}::{} of type {:?}",
                    val_ty, class_name, property, field_ty
                ),
            ));
        }
    } else if let Some(field_ty) = checker.packed_field_type(class_name, property) {
        if &field_ty != val_ty {
            return Err(CompileError::new(
                span,
                &format!(
                    "Type error: cannot assign {:?} to packed field {}::{} of type {:?}",
                    val_ty, class_name, property, field_ty
                ),
            ));
        }
    } else if checker.extern_classes.contains_key(class_name) {
        return Err(CompileError::new(
            span,
            &format!("Undefined extern field: {}::{}", class_name, property),
        ));
    } else if checker.packed_classes.contains_key(class_name) {
        return Err(CompileError::new(
            span,
            &format!("Undefined packed field: {}::{}", class_name, property),
        ));
    }
    Ok(())
}

/// Resolves the type of a named property on a class object, for array property operations.
///
/// Looks up the property in `checker.classes`, validates existence and access, and returns
/// the property type along with a flag indicating whether the property has a declared type.
/// Returns an error if the class or property is undefined.
fn resolve_object_array_property(
    checker: &Checker,
    class_name: &str,
    property: &str,
    span: Span,
) -> Result<(PhpType, bool), CompileError> {
    let class_info = checker
        .classes
        .get(class_name)
        .ok_or_else(|| CompileError::new(span, &format!("Undefined class: {}", class_name)))?;
    if !class_info.properties.iter().any(|(n, _)| n == property) {
        return Err(CompileError::new(
            span,
            &format!("Undefined property: {}::{}", class_name, property),
        ));
    }
    // Indirect array modification (`$obj->prop[] = x` / `$obj->prop[$k] = x`) is a write, so it
    // must honor PHP 8.4 asymmetric `set` visibility — not the read visibility.
    validate_object_property_access(checker, class_name, property, true, span)?;
    let property_has_declared_type = class_info.declared_properties.contains(property);
    let prop_ty = class_info
        .properties
        .iter()
        .find(|(name, _)| name == property)
        .map(|(_, ty)| ty.clone())
        .unwrap_or(PhpType::Int);
    Ok((prop_ty, property_has_declared_type))
}

/// Computes the updated type of an array property after a push operation (`$prop[] = value`).
///
/// For typed arrays: validates the pushed value against the element type via `require_compatible_arg_type`.
/// For untyped arrays: merges the pushed value's type into the element type via `merge_array_element_type`.
/// For untyped `Int` or `Void` base types: converts the property to `array<value_type>`.
/// Returns an error for buffer types or non-array property types.
fn updated_array_property_push_type(
    checker: &Checker,
    prop_ty: &PhpType,
    property_has_declared_type: bool,
    class_name: &str,
    property: &str,
    val_ty: &PhpType,
    span: Span,
) -> Result<PhpType, CompileError> {
    match prop_ty {
        PhpType::Array(elem_ty) => {
            if property_has_declared_type {
                checker.require_compatible_arg_type(
                    elem_ty.as_ref(),
                    val_ty,
                    span,
                    &format!("Property {}::${}[]", class_name, property),
                )?;
                Ok(PhpType::Array(elem_ty.clone()))
            } else if elem_ty.as_ref() == val_ty {
                Ok(PhpType::Array(elem_ty.clone()))
            } else {
                let merged_ty = checker
                    .merge_array_element_type(elem_ty, val_ty)
                    .unwrap_or(PhpType::Mixed);
                Ok(PhpType::Array(Box::new(merged_ty)))
            }
        }
        PhpType::Int | PhpType::Void if !property_has_declared_type => {
            Ok(PhpType::Array(Box::new(val_ty.clone())))
        }
        PhpType::Buffer(_) => Err(CompileError::new(
            span,
            "buffer<T> does not support push; allocate with buffer_new<T>(len)",
        )),
        other => Err(CompileError::new(
            span,
            &format!("Array push requires an array property, got {}", other),
        )),
    }
}

/// Computes the updated type of an array property after an indexed write (`$prop[$index] = value`).
///
/// Handles `PhpType::Array` and `PhpType::AssocArray` storage:
/// - For `Array`: validates element type compatibility, handles `Never`-element arrays specially
///   (converts to `AssocArray` when a static key forces hash storage), and merges element types
///   for untyped properties.
/// - For `AssocArray`: merges the key type with the index type and merges the value type with
///   the assigned value, preserving declared-type constraints.
fn updated_array_property_assign_type(
    checker: &Checker,
    prop_ty: &PhpType,
    property_has_declared_type: bool,
    class_name: &str,
    property: &str,
    index: &Expr,
    normalized_idx_ty: &PhpType,
    val_ty: &PhpType,
    span: Span,
) -> Result<PhpType, CompileError> {
    match prop_ty {
        PhpType::Array(elem_ty) => {
            if !matches!(normalized_idx_ty, PhpType::Int)
                || (matches!(elem_ty.as_ref(), PhpType::Never)
                    && static_array_key_forces_hash_storage(index))
            {
                if property_has_declared_type {
                    checker.require_compatible_arg_type(
                        elem_ty.as_ref(),
                        val_ty,
                        span,
                        &format!("Property {}::${}[]", class_name, property),
                    )?;
                }
                return Ok(assoc_property_type_after_keyed_write(
                    checker,
                    elem_ty,
                    property_has_declared_type,
                    normalized_idx_ty,
                    val_ty,
                ));
            }
            if property_has_declared_type {
                checker.require_compatible_arg_type(
                    elem_ty.as_ref(),
                    val_ty,
                    span,
                    &format!("Property {}::${}[]", class_name, property),
                )?;
                Ok(PhpType::Array(elem_ty.clone()))
            } else if elem_ty.as_ref() == val_ty {
                Ok(PhpType::Array(elem_ty.clone()))
            } else {
                let merged_ty = checker
                    .merge_array_element_type(elem_ty, val_ty)
                    .unwrap_or(PhpType::Mixed);
                Ok(PhpType::Array(Box::new(merged_ty)))
            }
        }
        PhpType::AssocArray {
            key,
            value: existing_value,
        } => {
            if property_has_declared_type {
                checker.require_compatible_arg_type(
                    existing_value.as_ref(),
                    val_ty,
                    span,
                    &format!("Property {}::${}[]", class_name, property),
                )?;
            }
            let merged_key = merge_array_key_types(*key.clone(), normalized_idx_ty.clone());
            let merged_value = if property_has_declared_type || existing_value.as_ref() == val_ty {
                *existing_value.clone()
            } else {
                checker
                    .merge_array_element_type(existing_value, val_ty)
                    .unwrap_or(PhpType::Mixed)
            };
            Ok(PhpType::AssocArray {
                key: Box::new(merged_key),
                value: Box::new(merged_value),
            })
        }
        other => Err(CompileError::new(
            span,
            &format!(
                "Array index assignment requires an array property, got {}",
                other
            ),
        )),
    }
}

/// Returns true if `ty` is a valid PHP array key type (Int, Str, or Mixed).
fn is_php_array_key_type(ty: &PhpType) -> bool {
    matches!(ty, PhpType::Int | PhpType::Str | PhpType::Mixed)
}

/// Computes the resulting `PhpType::AssocArray` type after writing to an array property with a
/// non-integer or static-computed key.
///
/// Special-cases `PhpType::Never` element types (treats the key and value as derived from the
/// write operands). Otherwise merges the element type with the assigned value type via
/// `merge_array_element_type`. If the property has a declared `Mixed` element type, returns
/// a fully `Mixed` `AssocArray` to preserve type soundness.
fn assoc_property_type_after_keyed_write(
    checker: &Checker,
    elem_ty: &PhpType,
    property_has_declared_type: bool,
    normalized_idx_ty: &PhpType,
    val_ty: &PhpType,
) -> PhpType {
    if property_has_declared_type && matches!(elem_ty, PhpType::Mixed) {
        return PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(PhpType::Mixed),
        };
    }

    let merged_key = if matches!(elem_ty, PhpType::Never) {
        normalized_idx_ty.clone()
    } else {
        merge_array_key_types(PhpType::Int, normalized_idx_ty.clone())
    };
    let merged_value = if matches!(elem_ty, PhpType::Never) {
        val_ty.clone()
    } else if elem_ty == val_ty {
        elem_ty.clone()
    } else {
        checker
            .merge_array_element_type(elem_ty, val_ty)
            .unwrap_or(PhpType::Mixed)
    };
    PhpType::AssocArray {
        key: Box::new(merged_key),
        value: Box::new(merged_value),
    }
}

/// Updates the in-memory type of an object property after an array operation.
///
/// Writes the updated type back to `checker.classes` only if the property has no declared type,
/// or if the update satisfies `declared_generic_array_can_use_assoc_storage` (permits converting
/// a `Mixed` element array to `AssocArray` storage when appropriate).
fn update_object_property_type(
    checker: &mut Checker,
    class_name: &str,
    property: &str,
    property_has_declared_type: bool,
    updated_prop_ty: PhpType,
) {
    if let Some(class_info) = checker.classes.get_mut(class_name) {
        if let Some(prop) = class_info
            .properties
            .iter_mut()
            .find(|(name, _)| name == property)
        {
            if !property_has_declared_type
                || declared_generic_array_can_use_assoc_storage(&prop.1, &updated_prop_ty)
            {
                prop.1 = updated_prop_ty;
            }
        }
    }
}

/// Returns true if a declared generic array property may be updated to `AssocArray` storage.
///
/// Currently returns true only when the property is declared as `array<PhpType::Mixed>` and the
/// updated type is an `AssocArray` with `PhpType::Mixed` values. This guards against widening
/// a typed array to an associative storage with a narrower element type.
fn declared_generic_array_can_use_assoc_storage(current: &PhpType, updated: &PhpType) -> bool {
    matches!(
        (current, updated),
        (
            PhpType::Array(elem_ty),
            PhpType::AssocArray { key: _, value }
        ) if matches!(elem_ty.as_ref(), PhpType::Mixed)
            && matches!(value.as_ref(), PhpType::Mixed)
    )
}

/// Resolves the type of a field on a typed pointer (extern class or packed struct) for array operations.
///
/// Checks `extern_field_type` then `packed_field_type`. Returns the field type if found.
/// If the class is known as extern/packed but the field is not defined, returns an error
/// describing the undefined field. The `operation` string is used only in error messages.
fn resolve_pointer_field_type(
    checker: &Checker,
    class_name: &str,
    property: &str,
    span: Span,
    operation: &str,
) -> Result<PhpType, CompileError> {
    if let Some(field_ty) = checker.extern_field_type(class_name, property) {
        Ok(field_ty)
    } else if let Some(field_ty) = checker.packed_field_type(class_name, property) {
        Ok(field_ty)
    } else if checker.extern_classes.contains_key(class_name) {
        Err(CompileError::new(
            span,
            &format!("Undefined extern field: {}::{}", class_name, property),
        ))
    } else if checker.packed_classes.contains_key(class_name) {
        Err(CompileError::new(
            span,
            &format!("Undefined packed field: {}::{}", class_name, property),
        ))
    } else {
        Err(CompileError::new(
            span,
            &format!("{} requires an object or typed pointer", operation),
        ))
    }
}
