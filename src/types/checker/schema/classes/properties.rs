//! Purpose:
//! Validates class schema properties rules.
//! Owns one slice of class metadata construction used by object inference and method checking.
//!
//! Called from:
//! - `crate::types::checker::schema::classes`
//!
//! Key details:
//! - Class metadata is shared globally after construction, so validation must reject inconsistent inheritance early.

use crate::errors::CompileError;
use crate::parser::ast::{ClassProperty, Visibility};
use crate::span::Span;
use crate::types::traits::FlattenedClass;
use crate::types::PhpType;

use super::super::super::{infer_expr_type_syntactic, Checker};
use super::super::validation::visibility_rank;
use super::super::interfaces::{build_property_contract, merge_property_contract};
use super::state::{collect_attribute_args, collect_attribute_names, ClassBuildState};

/// Applies property schema validation and metadata for all static and instance
/// properties declared in `class`. Static properties are validated for PHP
/// inheritance rules (final, visibility, override) and inserted into
/// `state.static_prop_types`. Instance properties are validated and inserted
/// into `state.prop_types`. Each property runs either `apply_static_property`
/// or `apply_instance_property` based on `prop.is_static`.
pub(super) fn apply_properties(
    state: &mut ClassBuildState,
    class: &FlattenedClass,
    checker: &Checker,
) -> Result<(), CompileError> {
    for prop in &class.properties {
        if prop.is_static {
            apply_static_property(state, class, checker, prop)?;
        } else {
            apply_instance_property(state, class, checker, prop)?;
        }
    }
    Ok(())
}

/// Validates a static property declaration against PHP inheritance rules and
/// records it in `state`. Rejects by-reference static properties, final private
/// combinations, and property type/redeclare conflicts. Computes the property
/// type from the declared hint, the default value, or defaults to `PhpType::Int`.
/// Updates `state.static_prop_types`, `state.static_property_declaring_classes`,
/// `state.final_static_properties`, and attribute maps.
fn apply_static_property(
    state: &mut ClassBuildState,
    class: &FlattenedClass,
    checker: &Checker,
    prop: &ClassProperty,
) -> Result<(), CompileError> {
    // PHP semantics: `readonly` only applies to instance properties. A `readonly class`
    // does not propagate `readonly` to its static properties — they remain mutable, and
    // an explicit `public readonly static` declaration is rejected at parse time.
    if prop.by_ref {
        return Err(CompileError::new(
            prop.span,
            "Static by-reference properties are not supported",
        ));
    }
    if prop.is_final && prop.visibility == Visibility::Private {
        return Err(CompileError::new(
            prop.span,
            "Property cannot be both final and private",
        ));
    }
    if state.property_declaring_classes.contains_key(&prop.name) {
        return Err(CompileError::new(
            prop.span,
            &format!(
                "Cannot redeclare instance property as static property: {}::{}",
                class.name, prop.name
            ),
        ));
    }

    let inherited_static_declaring_class =
        state.static_property_declaring_classes.get(&prop.name).cloned();
    let declared_ty = resolve_property_declared_type(checker, &class.name, prop)?;
    if let Some(parent_declaring_class) = inherited_static_declaring_class.as_ref() {
        validate_static_property_override(state, class, prop, declared_ty.as_ref(), parent_declaring_class)?;
    }

    let ty = if let Some(declared_ty) = declared_ty {
        checker.validate_declared_default_type(
            &declared_ty,
            prop.default.as_ref(),
            prop.span,
            &format!("Static property {}::${} default", class.name, prop.name),
        )?;
        state.declared_static_properties.insert(prop.name.clone());
        declared_ty
    } else if let Some(default) = &prop.default {
        state.declared_static_properties.remove(&prop.name);
        infer_expr_type_syntactic(default)
    } else {
        state.declared_static_properties.remove(&prop.name);
        PhpType::Int
    };

    if let Some(slot) = state
        .static_prop_types
        .iter()
        .position(|(name, _)| name == &prop.name)
    {
        state.static_prop_types[slot] = (prop.name.clone(), ty);
        state.static_defaults[slot] = prop.default.clone();
    } else {
        state.static_prop_types.push((prop.name.clone(), ty));
        state.static_defaults.push(prop.default.clone());
    }
    state
        .static_property_declaring_classes
        .insert(prop.name.clone(), class.name.clone());
    state
        .property_attribute_names
        .insert(prop.name.clone(), collect_attribute_names(&prop.attributes));
    state
        .property_attribute_args
        .insert(prop.name.clone(), collect_attribute_args(&prop.attributes));
    state
        .static_property_visibilities
        .insert(prop.name.clone(), prop.visibility.clone());
    if prop.is_final {
        state.final_static_properties.insert(prop.name.clone());
    } else {
        state.final_static_properties.remove(&prop.name);
    }
    Ok(())
}

/// Validates that a static property override maintains PHP inheritance constraints:
/// final properties cannot be overridden, visibility cannot be reduced, and the
/// type must be invariant with the parent declaration. Retrieves inherited
/// visibility and finality from `state` to produce precise error messages.
fn validate_static_property_override(
    state: &ClassBuildState,
    class: &FlattenedClass,
    prop: &ClassProperty,
    declared_ty: Option<&PhpType>,
    parent_declaring_class: &str,
) -> Result<(), CompileError> {
    let inherited_visibility = state
        .static_property_visibilities
        .get(&prop.name)
        .cloned()
        .unwrap_or(Visibility::Public);
    let inherited_is_private = inherited_visibility == Visibility::Private;
    if state.final_static_properties.contains(&prop.name) {
        let declaring_class = state
            .static_property_declaring_classes
            .get(&prop.name)
            .cloned()
            .unwrap_or_else(|| class.name.clone());
        return Err(CompileError::new(
            prop.span,
            &format!(
                "Cannot override final static property {}::${}",
                declaring_class, prop.name
            ),
        ));
    }
    if inherited_is_private {
        return Ok(());
    }
    if visibility_rank(&prop.visibility) < visibility_rank(&inherited_visibility) {
        return Err(CompileError::new(
            prop.span,
            &format!(
                "Cannot reduce visibility when overriding static property: {}::{}",
                class.name, prop.name
            ),
        ));
    }

    let parent_declared = state.declared_static_properties.contains(&prop.name);
    validate_property_type_invariance(
        parent_declared,
        || inherited_static_property_type(state, &prop.name),
        declared_ty,
        &class.name,
        &prop.name,
        parent_declaring_class,
        prop.span,
    )
}

/// Validates and records an instance property declaration. Rejects final+private
/// combinations and static/redeclare conflicts. Handles readonly classes, promoted
/// parameters, by-reference semantics, abstract properties, and property contracts.
/// Computes the property type and assigns a slot index (offset). Updates
/// `state.prop_types`, `state.property_offsets`, `state.property_declaring_classes`,
/// `state.readonly_properties`, `state.reference_properties`, `state.abstract_properties`,
/// and attribute maps. On redeclaration, delegates to `apply_instance_property_redeclaration`.
fn apply_instance_property(
    state: &mut ClassBuildState,
    class: &FlattenedClass,
    checker: &Checker,
    prop: &ClassProperty,
) -> Result<(), CompileError> {
    if prop.is_final && prop.visibility == Visibility::Private {
        return Err(CompileError::new(
            prop.span,
            "Property cannot be both final and private",
        ));
    }
    if state.static_property_declaring_classes.contains_key(&prop.name) {
        return Err(CompileError::new(
            prop.span,
            &format!(
                "Cannot redeclare static property as instance property: {}::{}",
                class.name, prop.name
            ),
        ));
    }
    if prop.by_ref && (class.is_readonly_class || prop.readonly) {
        return Err(CompileError::new(
            prop.span,
            &format!(
                "Readonly promoted property cannot be by-reference: {}::${}",
                class.name, prop.name
            ),
        ));
    }
    if let Some(parent_declaring_class) =
        state.property_declaring_classes.get(&prop.name).cloned()
    {
        return apply_instance_property_redeclaration(
            state,
            class,
            checker,
            prop,
            &parent_declaring_class,
        );
    }

    let ty = if let Some(declared_ty) = resolve_property_declared_type(checker, &class.name, prop)? {
        checker.validate_declared_default_type(
            &declared_ty,
            prop.default.as_ref(),
            prop.span,
            &format!("Property {}::${} default", class.name, prop.name),
        )?;
        state.declared_properties.insert(prop.name.clone());
        declared_ty
    } else if let Some(default) = &prop.default {
        infer_expr_type_syntactic(default)
    } else {
        PhpType::Int
    };

    let slot_index = state.prop_types.len();
    state.prop_types.push((prop.name.clone(), ty));
    state
        .property_offsets
        .insert(prop.name.clone(), 8 + slot_index * 16);
    state
        .property_declaring_classes
        .insert(prop.name.clone(), class.name.clone());
    state
        .property_attribute_names
        .insert(prop.name.clone(), collect_attribute_names(&prop.attributes));
    state
        .property_attribute_args
        .insert(prop.name.clone(), collect_attribute_args(&prop.attributes));
    state.defaults.push(prop.default.clone());
    state
        .property_visibilities
        .insert(prop.name.clone(), prop.visibility.clone());
    if prop.is_final {
        state.final_properties.insert(prop.name.clone());
    } else {
        state.final_properties.remove(&prop.name);
    }
    if class.is_readonly_class || prop.readonly {
        state.readonly_properties.insert(prop.name.clone());
    }
    if prop.by_ref {
        state.reference_properties.insert(prop.name.clone());
    }
    // Fresh declarations only ever add to `abstract_properties`. Concrete
    // declarations of a brand-new property never appear there in the first
    // place, so there is nothing to remove; the only path that clears a
    // property from `abstract_properties` is `apply_instance_property_redeclaration`
    // when a concrete child overrides an inherited abstract slot.
    if prop.is_abstract {
        state.abstract_properties.insert(prop.name.clone());
        let contract = build_property_contract(checker, &class.name, prop)?;
        state
            .abstract_property_hooks
            .insert(prop.name.clone(), contract);
    }
    Ok(())
}

/// Handles a child-class redeclaration of an instance property inherited from a
/// parent. Validates final, readonly, by-reference, and visibility constraints via
/// `validate_instance_property_override`. Updates the slot with the child's type
/// or default, merges abstract property contracts if applicable, and syncs
/// `state.prop_types`, `state.property_declaring_classes`, `state.final_properties`,
/// `state.readonly_properties`, `state.abstract_properties`, and
/// `state.reference_properties`.
fn apply_instance_property_redeclaration(
    state: &mut ClassBuildState,
    class: &FlattenedClass,
    checker: &Checker,
    prop: &ClassProperty,
    parent_declaring_class: &str,
) -> Result<(), CompileError> {
    if state.final_properties.contains(&prop.name) {
        return Err(CompileError::new(
            prop.span,
            &format!(
                "Cannot override final property {}::${}",
                parent_declaring_class, prop.name
            ),
        ));
    }
    let declared_ty = resolve_property_declared_type(checker, &class.name, prop)?;
        validate_instance_property_override(
            state,
            class,
            checker,
            prop,
            declared_ty.as_ref(),
            parent_declaring_class,
    )?;

    let ty = if let Some(declared_ty) = declared_ty {
        checker.validate_declared_default_type(
            &declared_ty,
            prop.default.as_ref(),
            prop.span,
            &format!("Property {}::${} default", class.name, prop.name),
        )?;
        state.declared_properties.insert(prop.name.clone());
        declared_ty
    } else if let Some(default) = &prop.default {
        infer_expr_type_syntactic(default)
    } else {
        PhpType::Int
    };

    let slot = find_instance_property_slot(state, &prop.name);
    state.prop_types[slot] = (prop.name.clone(), ty);
    state.defaults[slot] = prop.default.clone();
    state
        .property_declaring_classes
        .insert(prop.name.clone(), class.name.clone());
    state
        .property_visibilities
        .insert(prop.name.clone(), prop.visibility.clone());
    if prop.is_final {
        state.final_properties.insert(prop.name.clone());
    }
    if class.is_readonly_class || prop.readonly {
        state.readonly_properties.insert(prop.name.clone());
    }
    if prop.is_abstract {
        state.abstract_properties.insert(prop.name.clone());
        let mut contract = build_property_contract(checker, &class.name, prop)?;
        if let Some(existing) = state.abstract_property_hooks.get(&prop.name) {
            merge_property_contract(
                &mut contract,
                existing,
                checker,
                prop.span,
                &class.name,
                &prop.name,
                "redeclaring abstract property",
            )?;
        }
        state
            .abstract_property_hooks
            .insert(prop.name.clone(), contract);
    } else {
        state.abstract_properties.remove(&prop.name);
        state.abstract_property_hooks.remove(&prop.name);
    }
    if prop.by_ref {
        state.reference_properties.insert(prop.name.clone());
    }
    Ok(())
}

/// Validates an instance property override against PHP inheritance rules:
/// visibility reduction, final override attempts, readonly removal, by-reference
/// toggling, and making a concrete property abstract. For abstract parent
/// properties, delegates to `validate_abstract_property_contract`; otherwise
/// validates type invariance. Private parent properties are rejected as
/// shadowing is not yet supported.
fn validate_instance_property_override(
    state: &ClassBuildState,
    class: &FlattenedClass,
    checker: &Checker,
    prop: &ClassProperty,
    declared_ty: Option<&PhpType>,
    parent_declaring_class: &str,
) -> Result<(), CompileError> {
    let inherited_visibility = state
        .property_visibilities
        .get(&prop.name)
        .cloned()
        .unwrap_or(Visibility::Public);
    if inherited_visibility == Visibility::Private {
        // PHP allows shadowing a private parent property with a fresh slot in the child,
        // but our property layout uses one slot per name. Reject until proper scoping is added.
        return Err(CompileError::new(
            prop.span,
            &format!(
                "Cannot redeclare property {}::${}: parent class {} has a private property with the same name (shadowing private parent properties is not yet supported)",
                class.name, prop.name, parent_declaring_class
            ),
        ));
    }
    if visibility_rank(&prop.visibility) < visibility_rank(&inherited_visibility) {
        return Err(CompileError::new(
            prop.span,
            &format!(
                "Cannot reduce visibility when overriding property: {}::${}",
                class.name, prop.name
            ),
        ));
    }

    let parent_abstract = state.abstract_properties.contains(&prop.name);
    if parent_abstract {
        validate_abstract_property_contract(
            state,
            checker,
            class,
            prop,
            declared_ty,
            parent_declaring_class,
        )?;
    } else {
        let parent_declared = state.declared_properties.contains(&prop.name);
        validate_property_type_invariance(
            parent_declared,
            || inherited_instance_property_type(state, &prop.name),
            declared_ty,
            &class.name,
            &prop.name,
            parent_declaring_class,
            prop.span,
        )?;
    }

    let parent_readonly = state.readonly_properties.contains(&prop.name);
    let child_readonly = class.is_readonly_class || prop.readonly;
    if parent_readonly && !child_readonly {
        return Err(CompileError::new(
            prop.span,
            &format!(
                "Cannot remove readonly modifier when redeclaring property: {}::${}",
                class.name, prop.name
            ),
        ));
    }

    let parent_ref = state.reference_properties.contains(&prop.name);
    if parent_ref != prop.by_ref {
        return Err(CompileError::new(
            prop.span,
            &format!(
                "Cannot change by-reference qualifier when redeclaring property: {}::${}",
                class.name, prop.name
            ),
        ));
    }

    if !parent_abstract && prop.is_abstract {
        return Err(CompileError::new(
            prop.span,
            &format!(
                "Cannot make concrete property abstract: {}::${}",
                class.name, prop.name
            ),
        ));
    }

    Ok(())
}

/// Validates that a concrete property satisfies the `get`/`set` type contract
/// inherited from an abstract property declaration in the parent. For abstract
/// child declarations, merges the child's contract with the parent's contract.
/// For concrete overrides, checks that the declared or inferred type is
/// compatible with the required getter and, unless readonly, the required setter.
/// Returns `Ok` if no contract exists or validation passes.
fn validate_abstract_property_contract(
    state: &ClassBuildState,
    checker: &Checker,
    class: &FlattenedClass,
    prop: &ClassProperty,
    declared_ty: Option<&PhpType>,
    parent_declaring_class: &str,
) -> Result<(), CompileError> {
    let Some(contract) = state.abstract_property_hooks.get(&prop.name) else {
        return Ok(());
    };
    if prop.is_abstract {
        let mut child_contract = build_property_contract(checker, &class.name, prop)?;
        merge_property_contract(
            &mut child_contract,
            contract,
            checker,
            prop.span,
            &class.name,
            &prop.name,
            "redeclaring abstract property",
        )?;
        return Ok(());
    }

    let actual_ty = declared_ty.cloned().unwrap_or(PhpType::Mixed);
    if let Some(required_get) = contract.get_type.as_ref() {
        if !checker.type_accepts(required_get, &actual_ty) {
            return Err(CompileError::new(
                prop.span,
                &format!(
                    "Type of {}::${} must be compatible with get property contract {} from {}",
                    class.name, prop.name, required_get, parent_declaring_class
                ),
            ));
        }
    }
    if let Some(required_set) = contract.set_type.as_ref() {
        if class.is_readonly_class || prop.readonly {
            return Err(CompileError::new(
                prop.span,
                &format!(
                    "Readonly property {}::${} cannot satisfy set property contract from {}",
                    class.name, prop.name, parent_declaring_class
                ),
            ));
        }
        if !checker.type_accepts(&actual_ty, required_set) {
            return Err(CompileError::new(
                prop.span,
                &format!(
                    "Type of {}::${} must accept set property contract {} from {}",
                    class.name, prop.name, required_set, parent_declaring_class
                ),
            ));
        }
    }
    Ok(())
}

/// Looks up the declared type of a static property named `property` from the
/// parent's resolved types in `state.static_prop_types`. Returns `PhpType::Int`
/// if the property is not found, matching the undeclared-property default.
fn inherited_static_property_type(state: &ClassBuildState, property: &str) -> PhpType {
    state
        .static_prop_types
        .iter()
        .find(|(name, _)| name == property)
        .map(|(_, ty)| ty.clone())
        .unwrap_or(PhpType::Int)
}

/// Looks up the declared type of an instance property named `property` from the
/// parent's resolved types in `state.prop_types`. Returns `PhpType::Int`
/// if the property is not found, matching the undeclared-property default.
fn inherited_instance_property_type(state: &ClassBuildState, property: &str) -> PhpType {
    state
        .prop_types
        .iter()
        .find(|(name, _)| name == property)
        .map(|(_, ty)| ty.clone())
        .unwrap_or(PhpType::Int)
}

/// Finds the slot index of an instance property by name in `state.prop_types`.
/// Panics if the property is not found; the caller is responsible for ensuring
/// the property exists via prior checks on `state.property_declaring_classes`.
fn find_instance_property_slot(state: &ClassBuildState, name: &str) -> usize {
    state
        .prop_types
        .iter()
        .position(|(prop_name, _)| prop_name == name)
        .expect("redeclaration path: property must exist in prop_types when declaring_classes has it")
}

/// Checks that a redeclared property's typed/untyped status and declared type match the
/// parent's. Static and instance properties share these PHP rules: a typed parent must
/// be redeclared with the identical type, and an untyped parent cannot gain a type in
/// the child (and vice versa). The `parent_ty` closure is invoked only when an actual
/// type lookup is needed for the error message.
fn validate_property_type_invariance<F>(
    parent_declared: bool,
    parent_ty: F,
    child_ty: Option<&PhpType>,
    class_name: &str,
    prop_name: &str,
    parent_declaring_class: &str,
    span: Span,
) -> Result<(), CompileError>
where
    F: FnOnce() -> PhpType,
{
    match (parent_declared, child_ty) {
        (true, None) => Err(CompileError::new(
            span,
            &format!(
                "Type of {}::${} must be {} (as in class {})",
                class_name,
                prop_name,
                parent_ty(),
                parent_declaring_class
            ),
        )),
        (false, Some(_)) => Err(CompileError::new(
            span,
            &format!(
                "Type of {}::${} must not be defined (as in class {})",
                class_name, prop_name, parent_declaring_class
            ),
        )),
        (true, Some(child)) => {
            let parent = parent_ty();
            if &parent != child {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "Type of {}::${} must be {}, not {} (as in class {})",
                        class_name, prop_name, parent, child, parent_declaring_class
                    ),
                ));
            }
            Ok(())
        }
        (false, None) => Ok(()),
    }
}

/// Resolves the declared type for a property from its `type_expr` using
/// `checker.resolve_declared_property_hint`. Returns `Ok(None)` if the
/// property has no type declaration. Errors are mapped to the property's
/// span and name for clear diagnostics.
fn resolve_property_declared_type(
    checker: &Checker,
    class_name: &str,
    prop: &ClassProperty,
) -> Result<Option<PhpType>, CompileError> {
    prop.type_expr
        .as_ref()
        .map(|type_expr| {
            checker.resolve_declared_property_type_hint(
                type_expr,
                prop.span,
                &format!("Property {}::${}", class_name, prop.name),
            )
        })
        .transpose()
}
