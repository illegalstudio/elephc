//! Purpose:
//! Builds flattened class schema metadata from parsed declarations and inherited members.
//! Coordinates property, method, interface, and state validation for class declarations.
//!
//! Called from:
//! - `crate::types::checker::schema`
//!
//! Key details:
//! - Flattening must preserve visibility, overrides, readonly/final constraints, and interface obligations.

use std::collections::{HashMap, HashSet};

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::types::traits::FlattenedClass;
use crate::types::ClassInfo;

mod constants;
mod enum_interfaces;
mod interfaces;
mod methods;
mod narrowed_slots;
mod properties;
mod property_defaults;
mod state;
mod trait_requirements;

pub(crate) use property_defaults::validate_trait_property_defaults;
pub(crate) use property_defaults::{collect_trait_default_origins, collect_trait_property_origins, TraitPropertyOrigins};
pub(crate) use trait_requirements::{collect_trait_method_requirements, TraitMethodRequirements};
pub(crate) use constants::validate_compile_time_default_in_scope;
pub(crate) use constants::normalize_lexical_default_in_scope;
pub(crate) use constants::normalize_free_function_default_in_scope;
pub(crate) use constants::deferred_default_error;

use super::super::Checker;
use super::validation::build_constructor_param_map;
use state::ClassBuildState;

pub(super) use crate::types::{collect_attribute_args, collect_attribute_names};

pub(crate) use enum_interfaces::{expand_enum_interfaces, validate_enum_interface_contracts};
pub(super) use constants::resolve_lexical_class_value;
pub(super) use methods::validate_method_shape;

/// Recursively builds and registers `ClassInfo` for `class_name` and its inheritance chain.
///
/// Uses `building` set to detect circular inheritance. Validates modifiers, resolves the parent
/// class recursively, collects properties and methods, validates interface contracts, and finally
/// inserts the completed `ClassInfo` into `checker.classes`. Each class gets a unique `next_class_id`.
pub(crate) fn build_class_info_recursive(
    class_name: &str,
    class_map: &HashMap<String, FlattenedClass>,
    checker: &mut Checker,
    next_class_id: &mut u64,
    building: &mut HashSet<String>,
) -> Result<(), CompileError> {
    if checker.classes.contains_key(class_name) {
        return Ok(());
    }

    if !building.insert(class_name.to_string()) {
        return Err(CompileError::new(
            class_map
                .get(class_name)
                .map(|class| class.span)
                .unwrap_or_else(crate::span::Span::dummy),
            &format!(
                "Circular inheritance detected involving class {}",
                class_name
            ),
        ));
    }

    let class = load_class(class_name, class_map)?;
    let result = build_loaded_class_info(class_name, class, class_map, checker, next_class_id, building)
        .map_err(|error| locate_at_declaration(error, class_span_of(class_name, class_map)));
    // A failed build must not poison later sibling builds with a stale "circular inheritance"
    // diagnostic: the class is genuinely absent, but it is not still being built.
    building.remove(class_name);
    // The hint exists only while this class's own ancestry is unresolved. Once the build ends,
    // `is_subclass_of` reads the registered `ClassInfo`, so a stale entry could otherwise answer
    // for a later same-named class after a failed build.
    checker.pending_class_parents.remove(class_name);
    result
}

/// The declaration span of `class_name`, when the program declares it.
fn class_span_of(
    class_name: &str,
    class_map: &HashMap<String, FlattenedClass>,
) -> crate::span::Span {
    class_map
        .get(class_name)
        .map(|class| class.span)
        .unwrap_or_else(crate::span::Span::dummy)
}

/// Points every location-less error of one class's schema build at the class declaration.
///
/// The schema checks report against `Span::dummy()`, so on a program of thousands of classes
/// `error: Class A cannot extend final class B` named neither a file nor a line. The class
/// being built is the declaration the error is about; an error a parent's build already
/// located keeps its own position.
fn locate_at_declaration(mut error: CompileError, span: crate::span::Span) -> CompileError {
    if span.line == 0 {
        return error;
    }
    if error.span.line == 0 && error.file.is_none() {
        error.span = span;
    }
    error.related = error
        .related
        .into_iter()
        .map(|related| locate_at_declaration(related, span))
        .collect();
    error
}

/// [`build_class_info_recursive`] for a class whose declaration is loaded and not yet building.
fn build_loaded_class_info(
    class_name: &str,
    class: FlattenedClass,
    class_map: &HashMap<String, FlattenedClass>,
    checker: &mut Checker,
    next_class_id: &mut u64,
    building: &mut HashSet<String>,
) -> Result<(), CompileError> {
    validate_class_modifiers(&class)?;
    let parent_info = resolve_parent_info(
        class_name,
        &class,
        class_map,
        checker,
        next_class_id,
        building,
    )?;
    validate_parent_constraints(&class, parent_info.as_ref())?;

    let mut state = ClassBuildState::from_parent(parent_info.as_ref());
    properties::apply_properties(&mut state, &class, class_map, checker)?;
    methods::apply_methods(&mut state, &class, checker)?;
    interfaces::collect_interfaces(&mut state, &class, class_map, checker)?;
    validate_final_constant_constraints(&class, parent_info.as_ref(), checker)?;
    // PHP binds abstract trait methods after the parent inheritance checked above, and before
    // interfaces (`final_abstract_import_overrides`).
    if let Some(failure) =
        checker.final_abstract_binding_failures.get(&php_symbol_key(&class.name))
    {
        // The abstract trait methods bound before the failure are checked first, against the
        // class's own method (abstract redeclarations included) or the concrete import that
        // satisfies them.
        for required in &failure.earlier_requirements {
            if let Some(actual) = class.methods.iter().find(|method| {
                method.is_static == required.is_static
                    && method.name.eq_ignore_ascii_case(&required.name)
            }) {
                super::enum_traits::validate_class_requirement(checker, &class, required, actual)?;
            }
        }
        return Err(CompileError::new(failure.span, &failure.message));
    }
    validate_interface_final_constants(&class, &state, checker)?;
    interfaces::validate_interface_contracts(
        &mut state,
        &class,
        class_map,
        checker,
        next_class_id,
        building,
    )?;
    interfaces::ensure_concrete_class_implements_abstracts(&state, &class)?;

    let constructor_param_to_prop = constructor_param_to_prop_for(&class, parent_info.as_ref());
    // Builtins registered without source carry no source span; a prelude's classes do, but come
    // from compiler-generated statements.
    let is_internal =
        !class.span.is_from_source() || checker.internal_class_decls.contains(&class.name);
    let mut class_info = state.into_class_info(
        *next_class_id,
        &class,
        constructor_param_to_prop,
        is_internal,
    )?;
    // php-src evaluates a class's own property defaults in source order, then the ones its
    // traits import. Flattening puts trait properties first and lets a compatible class
    // redeclaration take the trait slot, so the class's source order comes from the
    // declaration itself.
    let imported = checker.trait_imported_properties.get(&class.name);
    let is_imported = |name: &str| imported.is_some_and(|names| names.contains(name));
    let own: Vec<String> = match checker.trait_property_origins.direct_order.get(&class.name) {
        Some(direct) => direct.clone(),
        None => class
            .properties
            .iter()
            .filter(|property| !is_imported(&property.name))
            .map(|property| property.name.clone())
            .collect(),
    };
    class_info.property_initialization_order = own
        .into_iter()
        .chain(
            class
                .properties
                .iter()
                .filter(|property| is_imported(&property.name))
                .map(|property| property.name.clone()),
        )
        .collect();
    class_info.trait_collision_defaults = checker
        .trait_property_origins
        .collision_defaults
        .get(&class.name)
        .cloned()
        .unwrap_or_default();
    checker.classes.insert(class.name.clone(), class_info);
    *next_class_id += 1;
    building.remove(class_name);
    Ok(())
}

/// Looks up `class_name` in `class_map` and returns a cloned `FlattenedClass`.
/// Returns an error if the class is not found.
fn load_class(
    class_name: &str,
    class_map: &HashMap<String, FlattenedClass>,
) -> Result<FlattenedClass, CompileError> {
    class_map.get(class_name).cloned().ok_or_else(|| {
        CompileError::new(
            crate::span::Span::dummy(),
            &format!(
                "Unknown class referenced during inheritance flattening: {}",
                class_name
            ),
        )
    })
}

/// Validates that `class` is not both `abstract` and `final` — a class cannot be simultaneously abstract and final.
fn validate_class_modifiers(class: &FlattenedClass) -> Result<(), CompileError> {
    if class.is_abstract && class.is_final {
        return Err(CompileError::new(
            crate::span::Span::dummy(),
            "Cannot use the final modifier on an abstract class",
        ));
    }
    Ok(())
}

/// Resolves the parent class for `class` by recursively building its `ClassInfo`.
///
/// Returns `Ok(None)` if `class` has no `extends` clause. If the parent name refers to an interface,
/// returns an error. Recursively invokes `build_class_info_recursive` to ensure the parent is registered
/// in `checker.classes` before returning its `ClassInfo`.
fn resolve_parent_info(
    class_name: &str,
    class: &FlattenedClass,
    class_map: &HashMap<String, FlattenedClass>,
    checker: &mut Checker,
    next_class_id: &mut u64,
    building: &mut HashSet<String>,
) -> Result<Option<ClassInfo>, CompileError> {
    let Some(parent_name) = &class.extends else {
        return Ok(None);
    };
    if checker.interfaces.contains_key(parent_name) {
        return Err(CompileError::new(
            crate::span::Span::dummy(),
            &format!(
                "Class {} cannot extend interface {}; use implements instead",
                class_name, parent_name
            ),
        ));
    }
    checker
        .pending_class_parents
        .insert(class_name.to_string(), parent_name.clone());
    build_class_info_recursive(parent_name, class_map, checker, next_class_id, building)?;
    checker
        .classes
        .get(parent_name)
        .cloned()
        .map(Some)
        .ok_or_else(|| {
            CompileError::new(
                crate::span::Span::dummy(),
                &format!("Unknown parent class: {}", parent_name),
            )
        })
}

/// Validates that `class` does not extend a `final` parent and that both have matching `readonly` class status.
///
/// Returns an error if the parent is final or if the readonly modifier differs between the child and parent.
fn validate_parent_constraints(
    class: &FlattenedClass,
    parent_info: Option<&ClassInfo>,
) -> Result<(), CompileError> {
    let (Some(parent), Some(parent_name)) = (parent_info, class.extends.as_ref()) else {
        return Ok(());
    };
    if parent.is_final {
        return Err(CompileError::new(
            crate::span::Span::dummy(),
            &format!(
                "Class {} cannot extend final class {}",
                class.name, parent_name
            ),
        ));
    }
    if class.is_readonly_class != parent.is_readonly_class {
        let relation = if class.is_readonly_class {
            "readonly class cannot extend non-readonly parent"
        } else {
            "non-readonly class cannot extend readonly parent"
        };
        return Err(CompileError::new(
            crate::span::Span::dummy(),
            &format!("{}: {} extends {}", relation, class.name, parent_name),
        ));
    }
    Ok(())
}

/// Validates PHP final class-constant inheritance constraints for one class against its own
/// declarations and its parent, including the final constants of the interfaces the parent
/// already implements (inherited with it). Interfaces this class adds are checked later
/// (`validate_interface_final_constants`).
fn validate_final_constant_constraints(
    class: &FlattenedClass,
    parent_info: Option<&ClassInfo>,
    checker: &Checker,
) -> Result<(), CompileError> {
    for constant in &class.constants {
        if constant.is_final && constant.visibility == crate::parser::ast::Visibility::Private {
            return Err(CompileError::new(
                constant.span,
                &format!(
                    "Private constant {}::{} cannot be final",
                    class.name, constant.name
                ),
            ));
        }
        if parent_info.is_some_and(|parent| parent.final_constants.contains(&constant.name)) {
            return Err(CompileError::new(
                constant.span,
                &format!(
                    "{}::{} cannot override final constant",
                    class.name, constant.name
                ),
            ));
        }
    }
    // PHP checks the parent class's own final constants before those it inherited from its
    // interfaces, whatever order the child declares them in.
    let parent_interfaces = parent_info.map_or(&[][..], |parent| &parent.interfaces[..]);
    for constant in &class.constants {
        if let Some(error) = interface_final_constant_error(class, constant, parent_interfaces, checker)
        {
            return Err(error);
        }
    }
    Ok(())
}

/// Returns the error for `constant` redeclaring a final constant of one of `interfaces`.
fn interface_final_constant_error(
    class: &FlattenedClass,
    constant: &crate::parser::ast::ClassConst,
    interfaces: &[String],
    checker: &Checker,
) -> Option<CompileError> {
    interfaces
        .iter()
        .filter_map(|interface_name| checker.interfaces.get(interface_name))
        .any(|interface_info| interface_info.final_constants.contains(&constant.name))
        .then(|| {
            CompileError::new(
                constant.span,
                &format!(
                    "{}::{} cannot override final interface constant",
                    class.name, constant.name
                ),
            )
        })
}

/// Rejects a class constant that redeclares a final constant of an implemented interface. PHP
/// checks interfaces after the parent inheritance and the abstract trait binding, so this runs
/// after `validate_final_constant_constraints` and the recorded binding failure.
fn validate_interface_final_constants(
    class: &FlattenedClass,
    state: &ClassBuildState,
    checker: &Checker,
) -> Result<(), CompileError> {
    for constant in &class.constants {
        if let Some(error) = interface_final_constant_error(class, constant, &state.interfaces, checker)
        {
            return Err(error);
        }
    }
    Ok(())
}

/// Builds a mapping from constructor parameter position to promoted property name for `class`.
///
/// If `class` defines `__construct`, maps each parameter (by order) to the property it promotes,
/// returning `Some` for promoted params and `None` for non-promoted params. If there is no
/// constructor, falls back to the parent's `constructor_param_to_prop`. Returns an empty vector
/// if there is no constructor and no parent. The result is stored in `ClassInfo` for codegen wiring.
fn constructor_param_to_prop_for(
    class: &FlattenedClass,
    parent_info: Option<&ClassInfo>,
) -> Vec<Option<String>> {
    if class
        .methods
        .iter()
        .any(|m| php_symbol_key(&m.name) == "__construct")
    {
        build_constructor_param_map(&class.methods)
    } else if let Some(parent) = parent_info {
        parent.constructor_param_to_prop.clone()
    } else {
        Vec::new()
    }
}
