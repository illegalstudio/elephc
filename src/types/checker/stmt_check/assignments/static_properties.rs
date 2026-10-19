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
use crate::parser::ast::{Expr, StaticReceiver};
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
        || matches!(&target.prop_ty, PhpType::Object(name) if name.eq_ignore_ascii_case("WeakMap"))
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
/// Infers the index and value types, resolves the property target, validates integer index,
/// validates element-type compatibility against declared types, merges element types when the
/// property is untyped, and updates the property type. Short-circuits for `ArrayAccess` objects.
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
    let normalized_idx_ty = normalized_array_key_type(index, idx_ty);
    let val_ty = checker.infer_type_with_assignment_effects(value, env)?;
    let target = resolve_static_property_assignment_target(checker, receiver, property, span)?;
    if target.dynamic_eval_target || target.prop_ty.is_php_array() {
        if !is_dynamic_eval_static_property_array_key_type(&normalized_idx_ty) {
            return Err(CompileError::new(span, "Array index must be integer"));
        }
        return Ok(());
    }
    if let PhpType::Object(class_name) = &target.prop_ty {
        if checker.object_type_implements_interface(class_name, "ArrayAccess") {
            return Ok(());
        }
    }
    // An UNTYPED static array written through a key that is not statically an int
    // (`self::$cache[$path] = ...`, `static::$map['k'] = ...`): the indexed `array_set` below
    // would coerce the key to int and silently store `[1]` instead of `{"a":1}`, and a
    // hash-typed slot has no direct static write. Widen the slot to the general PHP array
    // representation, whose runtime write path keeps string keys (the one a declared `array`
    // property already uses).
    if !target.property_has_declared_type
        && normalized_idx_ty != PhpType::Int
        && is_php_array_key_or_mixed(&normalized_idx_ty)
        && matches!(target.prop_ty, PhpType::Array(_) | PhpType::AssocArray { .. })
    {
        update_static_property_type(
            checker,
            property,
            &target.declaring_class,
            PhpType::php_array(),
        );
        return Ok(());
    }
    if !target.property_has_declared_type
        && normalized_idx_ty == PhpType::Int
        && matches!(target.prop_ty, PhpType::AssocArray { .. })
    {
        update_static_property_type(
            checker,
            property,
            &target.declaring_class,
            PhpType::php_array(),
        );
        return Ok(());
    }
    // See `is_php_array_key_type` in the property path: a boxed index has no `array_set`
    // lowering — `array_set index PHP type Mixed`, which is what a genuinely `mixed` index gets
    // here too — so accepting `int|float` would only move the refusal into the backend and lose
    // the clearer message.
    if normalized_idx_ty != PhpType::Int && normalized_idx_ty != PhpType::Mixed {
        return Err(CompileError::new(span, "Array index must be integer"));
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

/// Widens an UNTYPED static property that is the root of a nested write
/// (`self::$map[$k1][$k2] = v`) to the general PHP array representation, for the same reason as
/// its instance twin `widen_property_root_for_nested_write`: the inner level autovivifies, so the
/// slot needs Mixed elements under any key, and only the boxed representation writes through.
pub(super) fn widen_static_property_root_for_nested_write(
    checker: &mut Checker,
    receiver: &StaticReceiver,
    property: &str,
    span: Span,
) -> Result<(), CompileError> {
    let Ok(target) = resolve_static_property_assignment_target(checker, receiver, property, span)
    else {
        // The ordinary inference of the target reports the real error.
        return Ok(());
    };
    if target.dynamic_eval_target
        || target.property_has_declared_type
        || target.prop_ty.is_php_array()
        || target.prop_ty.codegen_repr() == PhpType::Mixed
    {
        return Ok(());
    }
    let widened = match target.prop_ty {
        PhpType::Array(_) | PhpType::AssocArray { .. } => PhpType::php_array(),
        PhpType::Void | PhpType::Never => PhpType::Mixed,
        _ => return Ok(()),
    };
    update_static_property_type(checker, property, &target.declaring_class, widened);
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

/// Widens an UNTYPED static array property whose element is bound by reference (`$r = &C::$p[$k]`,
/// `C::$p[$a] = &C::$p[$b]`, `unset` through the same addressable chain) to the general PHP array
/// representation. The reference path reaches the static slot through a boxed cell, exactly like a
/// declared `array` property; a raw packed or hash slot would be reinterpreted as that box.
pub(super) fn widen_referenced_untyped_static_property(
    checker: &mut Checker,
    receiver: &StaticReceiver,
    property: &str,
    span: Span,
    widen_null_default: bool,
) -> Result<(), CompileError> {
    let Ok(target) = resolve_static_property_assignment_target(checker, receiver, property, span) else {
        return Ok(());
    };
    if target.dynamic_eval_target {
        return Ok(());
    }
    widen_one_referenced_static_property(checker, &target, property, widen_null_default);
    // `static::$p[..]` resolves to the CALLED class at run time, so every subclass that
    // redeclares the property is a possible slot too. Each must take the same
    // boxed representation, or the reference/unset reads a raw layout as a box and crashes.
    if matches!(receiver.written_class_receiver(), StaticReceiver::Static) {
        let base = target.class_name.clone();
        let subclasses: Vec<String> = checker
            .classes
            .keys()
            .filter(|class| crate::types::class_inherits_from(&checker.classes, class, &base))
            .cloned()
            .collect();
        for class in subclasses {
            let declared_here = checker
                .classes
                .get(&class)
                .and_then(|info| info.static_property_declaring_classes.get(property))
                .is_some_and(|declaring| declaring == &class);
            if !declared_here {
                continue;
            }
            let named = StaticReceiver::Named(crate::names::Name::from_parts(
                crate::names::NameKind::FullyQualified,
                vec![class.clone()],
            ));
            let Ok(redeclared) = resolve_static_property_assignment_target(checker, &named, property, span) else {
                continue;
            };
            if redeclared.property_has_declared_type {
                let repr = redeclared.prop_ty.codegen_repr();
                if !(redeclared.prop_ty.is_php_array() || repr == PhpType::Mixed) {
                    return Err(CompileError::new(
                        span,
                        &format!(
                            "A reference to or unset of an element of `static::${property}` is not \
                             supported here: subclass {class} redeclares the property with a type \
                             whose storage is not a boxed array"
                        ),
                    ));
                }
                continue;
            }
            if !widen_one_referenced_static_property(checker, &redeclared, property, widen_null_default)
                && redeclared.prop_ty.codegen_repr() != PhpType::Mixed
                && !redeclared.prop_ty.is_php_array()
            {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "A reference to or unset of an element of `static::${property}` is not \
                         supported here: subclass {class} redeclares the property with a \
                         non-array default"
                    ),
                ));
            }
        }
    }
    Ok(())
}

/// Widens one untyped static property slot to boxed `array`; returns whether it was array-like.
fn widen_one_referenced_static_property(
    checker: &mut Checker,
    target: &StaticPropertyAssignmentTarget,
    property: &str,
    widen_null_default: bool,
) -> bool {
    if target.property_has_declared_type {
        return false;
    }
    let array_like = matches!(target.prop_ty, PhpType::Array(_) | PhpType::AssocArray { .. });
    let null_default = matches!(target.prop_ty, PhpType::Void | PhpType::Never);
    if array_like || (widen_null_default && null_default) {
        update_static_property_type(checker, property, &target.declaring_class, PhpType::php_array());
        return true;
    }
    false
}

/// Returns true for a key type a PHP array write accepts (`int`, `string`, or `mixed`).
fn is_php_array_key_or_mixed(ty: &PhpType) -> bool {
    matches!(ty, PhpType::Int | PhpType::Str | PhpType::Mixed)
}

/// Returns true when a dynamic eval static-property key can be handled by runtime array writes.
fn is_dynamic_eval_static_property_array_key_type(ty: &PhpType) -> bool {
    matches!(ty, PhpType::Int | PhpType::Str | PhpType::Mixed)
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
            checker, &current, val_ty, false,
        ) else {
            return;
        };
        refined_ty
    };
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
