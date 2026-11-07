//! Purpose:
//! Type-checks assignment static properties forms.
//! Updates type environments and validates storage-specific rules for locals, arrays, and properties.
//!
//! Called from:
//! - `crate::types::checker::stmt_check::assignments`
//!
//! Key details:
//! - Assignment checking must distinguish value writes, by-reference mutation, nullable access, and declared property contracts.

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind, StaticReceiver};
use crate::span::Span;
use crate::types::{normalized_array_key_type, PhpType, TypeEnv};

use super::super::super::Checker;

/// Internal data for static property assignment resolution.
/// Holds the resolved class, declaring class, declared-type status, and current property type.
struct StaticPropertyAssignmentTarget {
    class_name: String,
    declaring_class: String,
    property_has_declared_type: bool,
    prop_ty: PhpType,
    dynamic_eval_target: bool,
}

/// Gives an inferred static array a boxed PHP contract before nested write-context traversal.
/// Concrete child reads otherwise detach boxes, and homogeneous children reject legal writes.
pub(super) fn promote_static_nested_write_root(
    checker: &mut Checker, target: &Expr, span: Span,
) -> Result<(), CompileError> {
    let mut root = target;
    while let ExprKind::ArrayAccess { array, .. } = &root.kind { root = array; }
    let ExprKind::StaticPropertyAccess { receiver, property } = &root.kind else { return Ok(()); };
    let target = resolve_static_property_assignment_target(checker, receiver, property, span)?;
    if !target.property_has_declared_type
        && matches!(target.prop_ty, PhpType::Array(_) | PhpType::AssocArray { .. })
    {
        update_static_property_type(checker, property, &target.declaring_class, PhpType::php_array());
    }
    Ok(())
}

/// Type-checks a direct static property assignment `Class::$prop = value`.
///
/// Infers the value type, resolves the property target via `resolve_static_property_assignment_target`,
/// validates type compatibility against declared types, and refines the property type when no
/// declared type is present.
pub(super) fn check_static_property_assign(
    checker: &mut Checker,
    receiver: &StaticReceiver,
    property: &str,
    value: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let val_ty = checker.infer_type_with_assignment_effects(value, env)?;
    let target = resolve_static_property_assignment_target(checker, receiver, property, span)?;

    if target.property_has_declared_type {
        checker.require_compatible_arg_type(
            &target.prop_ty,
            &val_ty,
            span,
            &format!("Static property {}::${}", target.class_name, property),
        )?;
    }

    if !target.property_has_declared_type {
        refine_static_property_assignment_type(
            checker,
            property,
            &target.declaring_class,
            &val_ty,
        );
    }
    Ok(())
}

/// Type-checks an array-push assignment `Class::$prop[] = value`.
///
/// Infers the value type, resolves the property target, validates element-type compatibility
/// against declared types, merges element types when the property is untyped, and updates the
/// property type. Rejects buffer properties and non-array static properties.
pub(super) fn check_static_property_array_push(
    checker: &mut Checker,
    receiver: &StaticReceiver,
    property: &str,
    value: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let val_ty = checker.infer_type_with_assignment_effects(value, env)?;
    let target = resolve_static_property_assignment_target(checker, receiver, property, span)?;
    if target.dynamic_eval_target || target.prop_ty.is_php_array()
        || target.prop_ty == PhpType::Mixed
        || matches!(&target.prop_ty, PhpType::Object(name) if name.eq_ignore_ascii_case("WeakMap")
            || crate::types::builtin_classes::has_native_dimensions(name, &checker.classes))
    {
        return Ok(());
    }
    let updated_prop_ty = match target.prop_ty {
        PhpType::Array(elem_ty) => {
            if target.property_has_declared_type {
                checker.require_compatible_arg_type(
                    elem_ty.as_ref(),
                    &val_ty,
                    span,
                    &format!("Static property {}::${}[]", target.class_name, property),
                )?;
                PhpType::Array(elem_ty)
            } else if *elem_ty == val_ty {
                PhpType::Array(elem_ty)
            } else {
                let merged_ty = checker
                    .merge_array_element_type(&elem_ty, &val_ty)
                    .unwrap_or(PhpType::Mixed);
                PhpType::Array(Box::new(merged_ty))
            }
        }
        PhpType::Int | PhpType::Void if !target.property_has_declared_type => {
            PhpType::Array(Box::new(val_ty.clone()))
        }
        PhpType::Buffer(_) => {
            return Err(CompileError::new(
                span,
                "buffer<T> does not support push; allocate with buffer_new<T>(len)",
            ))
        }
        other => {
            return Err(CompileError::new(
                span,
                &format!("Array push requires an array static property, got {}", other),
            ))
        }
    };

    if !target.property_has_declared_type {
        update_static_property_type(
            checker,
            property,
            &target.declaring_class,
            updated_prop_ty,
        );
    }
    Ok(())
}

/// Type-checks an indexed array assignment `Class::$prop[index] = value`.
///
/// Infers the index and value types, resolves the property target, validates the key type,
/// validates element-type compatibility against declared types, promotes the slot to hash
/// storage for a string key, merges element types when the property is untyped, and updates
/// the property type. Short-circuits for `ArrayAccess` objects.
/// A `buffer<T>` property takes the whole-element write rules of a local or instance-property
/// buffer and keeps its type.
pub(super) fn check_static_property_array_assign(
    checker: &mut Checker,
    receiver: &StaticReceiver,
    property: &str,
    index: &Expr,
    value: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let idx_ty = checker.infer_type_with_assignment_effects(index, env)?;
    let normalized_idx_ty =
        super::properties::property_write_key_type(normalized_array_key_type(index, idx_ty.clone()));
    let val_ty = checker.infer_type_with_assignment_effects(value, env)?;
    let target = resolve_static_property_assignment_target(checker, receiver, property, span)?;
    let mixed_elements = match target.prop_ty.codegen_repr() {
        PhpType::Array(element) => element.codegen_repr() == PhpType::Mixed,
        PhpType::AssocArray { value, .. } => value.codegen_repr() == PhpType::Mixed,
        _ => false,
    };
    if target.dynamic_eval_target || target.prop_ty.is_php_array() || mixed_elements
        || target.prop_ty == PhpType::Mixed
    {
        if !is_dynamic_eval_static_property_array_key_type(&normalized_idx_ty) {
            return Err(CompileError::new(span, "Array index must be integer"));
        }
        return Ok(());
    }
    if let PhpType::Object(class_name) = &target.prop_ty {
        if checker.object_type_supports_dimensions(class_name) {
            return Ok(());
        }
    }
    if let PhpType::Buffer(elem_ty) = &target.prop_ty {
        super::arrays::validate_buffer_element_write(elem_ty, &idx_ty, &val_ty, span)?;
        return Ok(());
    }
    // A boxed `int|float` index has no element-write lowering (`array_set index PHP type
    // Mixed`), so the shared key rule refuses it here with the clearer message.
    if !super::properties::is_php_array_key_type(&normalized_idx_ty) {
        return Err(CompileError::new(span, "Array index must be integer"));
    }

    // A string key (or a non-sequential literal key on `[]`) promotes the slot to hash storage
    // exactly as an instance property does; keeping the indexed slot lowered the write as an
    // `ArraySet` that dropped the key, so `C::$map['k'] = 6` printed `[6]`.
    let updated_prop_ty = match &target.prop_ty {
        // A declared packed `array<T>` slot refuses a key that is not provably an integer
        // inside the shared helper, instead of coercing it into an index.
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            super::properties::updated_array_property_assign_type(
                checker,
                &target.prop_ty,
                target.property_has_declared_type,
                false,
                &format!("Static property {}::${}[]", target.class_name, property),
                index,
                &normalized_idx_ty,
                &val_ty,
                span,
            )?
        }
        PhpType::Int | PhpType::Void if !target.property_has_declared_type => {
            if normalized_idx_ty == PhpType::Int {
                PhpType::Array(Box::new(val_ty.clone()))
            } else {
                PhpType::AssocArray {
                    key: Box::new(normalized_idx_ty.clone()),
                    value: Box::new(val_ty.clone()),
                }
            }
        }
        other => {
            return Err(CompileError::new(
                span,
                &format!(
                    "Array index assignment requires an array static property, got {}",
                    other
                ),
            ))
        }
    };

    if !target.property_has_declared_type {
        update_static_property_type(
            checker,
            property,
            &target.declaring_class,
            updated_prop_ty,
        );
    }
    Ok(())
}

/// Resolves `receiver` to a class name and fetches static property metadata.
///
/// Returns `StaticPropertyAssignmentTarget` with class name, declaring class,
/// declared-type flag, and current property type. Checks that the property exists
/// and that the current context can access it per PHP visibility rules.
fn resolve_static_property_assignment_target(
    checker: &Checker,
    receiver: &StaticReceiver,
    property: &str,
    span: Span,
) -> Result<StaticPropertyAssignmentTarget, CompileError> {
    // A receiver written `Box<int>::of()` names `Box` until instantiation renames
    // it, and this pass can run on a generic function's template body — which is
    // walked and then stripped, never instantiated.
    let receiver = &receiver.written_class_receiver();
    let class_name = match receiver {
        // A generic receiver is instantiated into an ordinary named one before type checking;
        // a template has no class to reach through.
        StaticReceiver::Generic(_) => unreachable!(
            "StaticReceiver::Generic must be instantiated by generics::classes"
        ),
        StaticReceiver::Named(class_name) => class_name.as_str().to_string(),
        StaticReceiver::Self_ => checker
            .current_class
            .as_ref()
            .cloned()
            .ok_or_else(|| CompileError::new(span, "Cannot use self:: outside class method scope"))?,
        StaticReceiver::Static => checker
            .current_class
            .as_ref()
            .cloned()
            .ok_or_else(|| CompileError::new(span, "Cannot use static:: outside class method scope"))?,
        StaticReceiver::Parent => {
            let current_class = checker.current_class.as_ref().ok_or_else(|| {
                CompileError::new(span, "Cannot use parent:: outside class method scope")
            })?;
            let current_info = checker.classes.get(current_class).ok_or_else(|| {
                CompileError::new(span, &format!("Undefined class: {}", current_class))
            })?;
            current_info.parent.as_ref().cloned().ok_or_else(|| {
                CompileError::new(
                    span,
                    &format!("Class {} has no parent class", current_class),
                )
            })?
        }
    };

    if checker.eval_barrier_active
        && matches!(receiver, StaticReceiver::Named(_))
        && !checker.classes.contains_key(&class_name)
    {
        return Ok(StaticPropertyAssignmentTarget {
            class_name: class_name.clone(),
            declaring_class: class_name,
            property_has_declared_type: false,
            prop_ty: PhpType::Mixed,
            dynamic_eval_target: true,
        });
    }

    let class_info = checker
        .classes
        .get(&class_name)
        .ok_or_else(|| CompileError::new(span, &format!("Undefined class: {}", class_name)))?;
    if !class_info
        .static_properties
        .iter()
        .any(|(name, _)| name == property)
    {
        return Err(CompileError::new(
            span,
            &format!("Undefined static property: {}::{}", class_name, property),
        ));
    }
    if let Some(visibility) = class_info.static_property_visibilities.get(property) {
        let declaring_class = class_info
            .static_property_declaring_classes
            .get(property)
            .map(String::as_str)
            .unwrap_or(class_name.as_str());
        if !checker.can_access_member(declaring_class, visibility) {
            return Err(CompileError::new(
                span,
                &format!(
                    "Cannot access {} static property: {}::{}",
                    Checker::visibility_label(visibility),
                    class_name,
                    property
                ),
            ));
        }
    }
    let declaring_class = class_info
        .static_property_declaring_classes
        .get(property)
        .cloned()
        .unwrap_or_else(|| class_name.clone());
    let property_has_declared_type = class_info.declared_static_properties.contains(property);
    let prop_ty = class_info
        .static_properties
        .iter()
        .find(|(name, _)| name == property)
        .map(|(_, ty)| ty.clone())
        .unwrap_or(PhpType::Int);

    Ok(StaticPropertyAssignmentTarget {
        class_name,
        declaring_class,
        property_has_declared_type,
        prop_ty,
        dynamic_eval_target: false,
    })
}

/// Returns true when a static-property key can be handled by the boxed runtime array write.
///
/// That write converts the key with PHP's rules, so a scalar union such as `?int` or `?bool`
/// (null becomes `""`, a bool its integer) is accepted beside `int`, `string` and `mixed`; its
/// refusal used to be dropped as a stale initial error inside function bodies, and the
/// runtime write handled the key correctly.
fn is_dynamic_eval_static_property_array_key_type(ty: &PhpType) -> bool {
    match ty {
        PhpType::Int | PhpType::Str | PhpType::Mixed => true,
        PhpType::Union(members) => members.iter().all(|member| {
            matches!(
                member,
                PhpType::Int
                    | PhpType::Str
                    | PhpType::Bool
                    | PhpType::False
                    | PhpType::Float
                    | PhpType::Void
            )
        }),
        _ => false,
    }
}

/// Updates the type of a static property in all classes that declare it.
///
/// Iterates over all classes and mutates the type entry for `property` on the
/// declaring class `declaring_class`. Used after array-push or index assignment
/// to propagate the new element type.
fn update_static_property_type(
    checker: &mut Checker,
    property: &str,
    declaring_class: &str,
    updated_ty: PhpType,
) {
    // A tagged-scalar element is stored boxed, as the instance slot update does.
    let updated_ty = crate::types::box_tagged_scalar_array_elements(updated_ty);
    for class_info in checker.classes.values_mut() {
        if class_info
            .static_property_declaring_classes
            .get(property)
            .map(String::as_str)
            != Some(declaring_class)
        {
            continue;
        }
        if let Some(prop) = class_info
            .static_properties
            .iter_mut()
            .find(|(name, _)| name == property)
        {
            prop.1 = updated_ty.clone();
        }
    }
}

/// Refines the type of a static property based on an assigned value.
///
/// Delegates the type decision to `refined_untyped_property_assignment_type` (shared with
/// instance properties), which stores scalar/array writes to untyped null-defaulted slots
/// as nullable unions so the null default stays observable. Only updates when the refined
/// type differs from the current.
fn refine_static_property_assignment_type(
    checker: &mut Checker,
    property: &str,
    declaring_class: &str,
    val_ty: &PhpType,
) {
    let refined_ty = {
        let current = checker.classes.values().find_map(|class_info| {
            if class_info
                .static_property_declaring_classes
                .get(property)
                .map(String::as_str)
                != Some(declaring_class)
            {
                return None;
            }
            class_info
                .static_properties
                .iter()
                .find(|(name, _)| name == property)
                .map(|(_, ty)| ty.clone())
        });
        let Some(current) = current else {
            return;
        };
        let Some(refined_ty) = super::properties::refined_untyped_property_assignment_type(
            checker, &current, val_ty, false, true,
        ) else {
            return;
        };
        refined_ty
    };
    // A tagged-scalar element is stored boxed, as the instance slot update does.
    let refined_ty = crate::types::box_tagged_scalar_array_elements(refined_ty);
    for class_info in checker.classes.values_mut() {
        if class_info
            .static_property_declaring_classes
            .get(property)
            .map(String::as_str)
            != Some(declaring_class)
        {
            continue;
        }
        if let Some(prop) = class_info
            .static_properties
            .iter_mut()
            .find(|(name, _)| name == property)
        {
            prop.1 = refined_ty.clone();
        }
    }
}
