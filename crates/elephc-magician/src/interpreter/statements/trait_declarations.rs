//! Purpose:
//! Registers interfaces and traits and composes trait members into classes.
//!
//! Called from:
//! - Class-like declaration execution before validation and registration.
//!
//! Key details:
//! - Trait adaptations, conflicts, aliases, visibility, constants, and properties are resolved here.

use super::*;

/// Registers an eval-declared interface in the dynamic interface table.
pub(in crate::interpreter) fn execute_interface_decl_stmt(
    interface: &EvalInterface,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let parents = interface.parents().iter().map(|name| ensure_generic_class(name, None, "", context, scope, values)).collect::<Result<_, _>>()?;
    let concrete = interface.clone().with_parents(parents);
    let interface = &concrete;
    let name = interface.name().trim_start_matches('\\');
    if context.generic_template(name).is_some()
        || context.has_interface(name)
        || context.has_class(name)
        || context.has_enum(name)
        || eval_runtime_interface_exists(name, values)?
        || values.class_exists(name)?
        || values.enum_exists(name)?
    {
        note_eval_runtime_failure(
            format!("Cannot redeclare interface {}", name),
            context,
        );
        return Err(EvalStatus::RuntimeFatal);
    }
    ensure_eval_interface_parents_available(interface.parents(), context, values)?;
    for parent in interface.parents() {
        if context
            .interface_parent_names(parent)
            .iter()
            .any(|ancestor| ancestor.eq_ignore_ascii_case(name))
        {
            note_eval_runtime_failure(
                format!("Interface {} cannot extend itself through {}", name, parent),
                context,
            );
            return Err(EvalStatus::RuntimeFatal);
        }
        if !context.has_interface(parent) && !eval_runtime_interface_exists(parent, values)? {
            note_eval_runtime_failure(format!("Interface \"{}\" not found", parent), context);
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    validate_eval_interface_attribute_targets(interface)?;
    validate_eval_interface_override_attributes(interface, context, values)?;
    validate_eval_declared_constants(interface.constants())?;
    validate_eval_interface_constants(interface.constants())?;
    validate_interface_constant_parent_redeclarations(interface, context, values)?;
    let declaration_file = eval_declaration_source_file(context);
    if context.define_interface(interface.clone()) {
        // The declaring file is recorded for the same reason a class records it: reflection
        // answers `getFileName()` from it, and without a record it falls back to the generated
        // program's single source file -- the ENTRY point -- so an interface an included file
        // declared claimed to live in the file that included it.
        context.set_class_source_file(interface.name(), declaration_file);
        initialize_eval_declared_constants(
            interface.name(),
            interface.constants(),
            context,
            scope,
            values,
        )
    } else {
        note_eval_runtime_failure(format!("Cannot declare interface {}", name), context);
        Err(EvalStatus::RuntimeFatal)
    }
}

/// Loads missing parent interfaces before registering an eval interface declaration.
///
/// PHP resolves every `extends` target through the normal SPL autoload chain at declaration
/// time. The callback can execute in another request-global eval context, so synchronize the
/// local declaration registry before requiring that the resolved symbol is an interface.
fn ensure_eval_interface_parents_available(
    parents: &[String],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for parent in parents {
        if context.has_interface(parent) || eval_runtime_interface_exists(parent, values)? {
            continue;
        }
        crate::interpreter::eval_spl_autoload_classlike_definition(parent, context, values)?;
        #[cfg(not(test))]
        context.sync_global_eval_classes();
        if !context.has_interface(parent) && !eval_runtime_interface_exists(parent, values)? {
            // Named, because the generic statement fallback reported this as
            // `unsupported InterfaceDecl statement` -- a gap in the interpreter rather than the
            // missing parent PHP itself reports.
            note_eval_runtime_failure(format!("Interface \"{}\" not found", parent), context);
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    Ok(())
}

/// Registers an eval-declared trait in the dynamic trait table.
pub(in crate::interpreter) fn execute_trait_decl_stmt(
    trait_decl: &EvalTrait,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let traits: Vec<String> = trait_decl.traits().iter().map(|name| ensure_generic_class(name, None, "", context, scope, values)).collect::<Result<_, _>>()?;
    let adaptations = concrete_trait_adaptations(trait_decl.trait_adaptations(), &traits);
    let concrete = trait_decl.clone().with_traits(traits, adaptations);
    let trait_decl = &concrete;
    let name = trait_decl.name().trim_start_matches('\\');
    if context.generic_template(name).is_some()
        || context.has_trait(name)
        || context.has_class(name)
        || context.has_interface(name)
        || context.has_enum(name)
        || (values.trait_exists(name)? && !context.executing_include())
        || values.class_exists(name)?
        || eval_runtime_interface_exists(name, values)?
        || values.enum_exists(name)?
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    let trait_decl = expand_eval_trait_traits(trait_decl, context, values)?;
    validate_eval_trait_attribute_targets(&trait_decl)?;
    validate_eval_declared_constants(trait_decl.constants())?;
    validate_eval_magic_methods(trait_decl.methods())?;
    let declaration_file = eval_declaration_source_file(context);
    if context.define_trait(trait_decl.clone()) {
        context.set_class_source_file(trait_decl.name(), declaration_file);
        initialize_eval_declared_constants(
            trait_decl.name(),
            trait_decl.constants(),
            context,
            scope,
            values,
        )
    } else {
        Err(EvalStatus::RuntimeFatal)
    }
}

/// Expands nested eval trait uses into the trait metadata registered by eval.
pub(super) fn expand_eval_trait_traits(
    trait_decl: &EvalTrait,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalTrait, EvalStatus> {
    if trait_decl.traits().is_empty() {
        return Ok(trait_decl.clone());
    }
    ensure_eval_traits_available(trait_decl.traits(), context, values)?;
    validate_eval_trait_decl_adaptations(trait_decl, context)?;
    let trait_method_names = trait_method_name_set(trait_decl);
    let mut imported_method_names = std::collections::HashSet::new();
    let mut imported_properties = std::collections::HashMap::new();
    let mut imported_constants = std::collections::HashMap::new();
    let mut constants = Vec::new();
    let mut properties = Vec::new();
    let mut methods = Vec::new();
    for used_trait_name in trait_decl.traits() {
        let Some(used_trait_decl) = context.trait_decl(used_trait_name) else {
            return Err(EvalStatus::RuntimeFatal);
        };
        append_eval_trait_constants(
            used_trait_decl,
            trait_decl.constants(),
            &mut imported_constants,
            &mut constants,
        )?;
        append_eval_trait_properties(
            used_trait_decl,
            trait_decl.properties(),
            &mut imported_properties,
            &mut properties,
        )?;
        append_eval_trait_methods(
            used_trait_decl,
            trait_decl.trait_adaptations(),
            &trait_method_names,
            &mut imported_method_names,
            &mut methods,
        )?;
    }
    constants.extend(trait_decl.constants().iter().cloned());
    properties.extend(trait_decl.properties().iter().cloned());
    methods.extend(trait_decl.methods().iter().cloned());
    let mut expanded = EvalTrait::with_constants_traits_adaptations(
        trait_decl.name().to_string(),
        constants,
        properties,
        methods,
        trait_decl.traits().to_vec(),
        trait_decl.trait_adaptations().to_vec(),
    )
    .with_attributes(trait_decl.attributes().to_vec());
    if let Some(source_location) = trait_decl.source_location() {
        expanded = expanded.with_source_location(source_location);
    }
    Ok(expanded)
}

/// Validates that trait-level adaptations reference directly used traits and methods.
pub(super) fn validate_eval_trait_decl_adaptations(
    trait_decl: &EvalTrait,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    for adaptation in trait_decl.trait_adaptations() {
        match adaptation {
            EvalTraitAdaptation::Alias {
                trait_name, method, ..
            } => validate_eval_trait_decl_adaptation_method(
                trait_decl,
                context,
                trait_name.as_deref(),
                method,
            )?,
            EvalTraitAdaptation::InsteadOf {
                trait_name,
                method,
                instead_of,
            } => {
                let Some(trait_name) = trait_name.as_deref() else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                validate_eval_trait_decl_adaptation_method(
                    trait_decl,
                    context,
                    Some(trait_name),
                    method,
                )?;
                for suppressed in instead_of {
                    if eval_trait_used_trait_decl(trait_decl, context, suppressed).is_none() {
                        return Err(EvalStatus::RuntimeFatal);
                    }
                    if same_eval_class_name(suppressed, trait_name) {
                        return Err(EvalStatus::RuntimeFatal);
                    }
                }
            }
        }
    }
    Ok(())
}

/// Validates one trait-level adaptation method target.
pub(super) fn validate_eval_trait_decl_adaptation_method(
    trait_decl: &EvalTrait,
    context: &ElephcEvalContext,
    trait_name: Option<&str>,
    method: &str,
) -> Result<(), EvalStatus> {
    if let Some(trait_name) = trait_name {
        let Some(used_trait_decl) = eval_trait_used_trait_decl(trait_decl, context, trait_name)
        else {
            return Err(EvalStatus::RuntimeFatal);
        };
        return trait_has_method(used_trait_decl, method)
            .then_some(())
            .ok_or(EvalStatus::RuntimeFatal);
    }
    trait_decl
        .traits()
        .iter()
        .filter_map(|trait_name| context.trait_decl(trait_name))
        .any(|used_trait_decl| trait_has_method(used_trait_decl, method))
        .then_some(())
        .ok_or(EvalStatus::RuntimeFatal)
}

/// Returns a trait declaration only when the pending trait directly uses that trait.
pub(super) fn eval_trait_used_trait_decl<'a>(
    trait_decl: &EvalTrait,
    context: &'a ElephcEvalContext,
    trait_name: &str,
) -> Option<&'a EvalTrait> {
    trait_decl
        .traits()
        .iter()
        .any(|used_trait| same_eval_class_name(used_trait, trait_name))
        .then(|| context.trait_decl(trait_name))
        .flatten()
}

/// Expands eval trait uses into the class metadata used by dynamic dispatch.
pub(super) fn expand_eval_class_traits(
    class: &EvalClass,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalClass, EvalStatus> {
    if class.traits().is_empty() {
        return Ok(class.clone());
    }
    ensure_eval_traits_available(class.traits(), context, values)?;
    validate_eval_trait_adaptations(class, context)?;
    let class_method_names = class_method_name_set(class);
    let mut trait_method_names = std::collections::HashSet::new();
    let mut trait_properties = std::collections::HashMap::new();
    let mut trait_constants = std::collections::HashMap::new();
    let mut constants = Vec::new();
    let mut properties = Vec::new();
    let mut methods = Vec::new();
    for trait_name in class.traits() {
        let Some(trait_decl) = context.trait_decl(trait_name) else {
            return Err(EvalStatus::RuntimeFatal);
        };
        append_eval_trait_constants(
            trait_decl,
            class.constants(),
            &mut trait_constants,
            &mut constants,
        )?;
        append_eval_trait_properties(
            trait_decl,
            class.properties(),
            &mut trait_properties,
            &mut properties,
        )?;
        append_eval_trait_methods(
            trait_decl,
            class.trait_adaptations(),
            &class_method_names,
            &mut trait_method_names,
            &mut methods,
        )?;
    }
    constants.extend(class.constants().iter().cloned());
    properties.extend(class.properties().iter().cloned());
    methods.extend(class.methods().iter().cloned());
    let mut expanded = EvalClass::with_class_modifiers_traits_adaptations_and_constants(
        class.name().to_string(),
        class.is_abstract(),
        class.is_final(),
        class.is_readonly_class(),
        class.parent().map(str::to_string),
        class.interfaces().to_vec(),
        class.traits().to_vec(),
        class.trait_adaptations().to_vec(),
        constants,
        properties,
        methods,
    )
    .with_attributes(class.attributes().to_vec());
    if class.is_anonymous() {
        expanded = expanded.with_anonymous();
    }
    Ok(expanded)
}

/// Loads every trait required by a pending dynamic declaration before composition.
///
/// Trait declarations execute at class-declaration time in PHP, so a missing trait first runs the
/// regular SPL callback chain. The context is then synchronized because a foreign callback owner
/// can have included the trait.
fn ensure_eval_traits_available(
    traits: &[String],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for trait_name in traits {
        if context.trait_decl(trait_name).is_some() {
            continue;
        }
        // A trait the compiled world has was declared from a file the compiler included, and
        // the include the autoloader performs for it is skipped on that mark (see the retry
        // below): the ordinary round can never produce the declaration, so it is skipped
        // rather than paid -- a Composer `loadClass` and a bridge include per trait per request.
        let compiled = values.trait_exists(trait_name)?;
        if !compiled {
            crate::interpreter::eval_spl_autoload_classlike_definition(
                trait_name,
                context,
                values,
            )?;
            #[cfg(not(test))]
            context.sync_global_eval_classes();
        }
        if context.trait_decl(trait_name).is_none() {
            // The autoloader ran and the trait is still not here, which for a compiled program has
            // ONE ordinary cause: the compiler already included the trait's file, so the include
            // the autoloader performed was skipped on that mark
            // (`interpreter::include_exec`, `phase=compiler_included_skip`). The mark is a promise
            // about the COMPILED world, and a trait does not live there in any form this side can
            // read: its members are copied into the using class at declaration time, so an
            // interpreted class needs the trait's own body.
            //
            // MEASURED on Symfony's console: `RoutingControllerPass` uses
            // `PriorityTaggedServiceTrait`, whose file traced as `compiler_included_skip`
            // immediately before `stage=expand_traits` failed and the whole command died with
            // `class ... could not be declared`.
            //
            // Retry once with the mark ignored. The skip stays in force everywhere else — it is
            // what keeps a request stat-ing its vendor tree instead of re-reading it.
            crate::interpreter::include_exec::with_compiler_included_sources_reincluded(|| {
                crate::interpreter::eval_spl_autoload_classlike_definition(
                    trait_name, context, values,
                )
            })?;
            #[cfg(not(test))]
            context.sync_global_eval_classes();
        }
        if context.trait_decl(trait_name).is_none() {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    Ok(())
}

/// Validates that trait adaptations reference used traits and existing methods.
pub(super) fn validate_eval_trait_adaptations(
    class: &EvalClass,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    for adaptation in class.trait_adaptations() {
        match adaptation {
            EvalTraitAdaptation::Alias {
                trait_name, method, ..
            } => {
                validate_eval_trait_adaptation_method(class, context, trait_name.as_deref(), method)?
            }
            EvalTraitAdaptation::InsteadOf {
                trait_name,
                method,
                instead_of,
            } => {
                let Some(trait_name) = trait_name.as_deref() else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                validate_eval_trait_adaptation_method(class, context, Some(trait_name), method)?;
                for suppressed in instead_of {
                    if eval_used_trait_decl(class, context, suppressed).is_none() {
                        return Err(EvalStatus::RuntimeFatal);
                    }
                    if same_eval_class_name(suppressed, trait_name) {
                        return Err(EvalStatus::RuntimeFatal);
                    }
                }
            }
        }
    }
    Ok(())
}

/// Validates one adaptation method target, allowing unqualified alias targets.
pub(super) fn validate_eval_trait_adaptation_method(
    class: &EvalClass,
    context: &ElephcEvalContext,
    trait_name: Option<&str>,
    method: &str,
) -> Result<(), EvalStatus> {
    if let Some(trait_name) = trait_name {
        let Some(trait_decl) = eval_used_trait_decl(class, context, trait_name) else {
            return Err(EvalStatus::RuntimeFatal);
        };
        return trait_has_method(trait_decl, method)
            .then_some(())
            .ok_or(EvalStatus::RuntimeFatal);
    }
    class
        .traits()
        .iter()
        .filter_map(|trait_name| context.trait_decl(trait_name))
        .any(|trait_decl| trait_has_method(trait_decl, method))
        .then_some(())
        .ok_or(EvalStatus::RuntimeFatal)
}

/// Returns a trait declaration only when the pending class directly uses that trait.
pub(super) fn eval_used_trait_decl<'a>(
    class: &EvalClass,
    context: &'a ElephcEvalContext,
    trait_name: &str,
) -> Option<&'a EvalTrait> {
    class
        .traits()
        .iter()
        .any(|used_trait| same_eval_class_name(used_trait, trait_name))
        .then(|| context.trait_decl(trait_name))
        .flatten()
}

/// Returns whether a trait declares a method by PHP case-insensitive method name.
pub(super) fn trait_has_method(trait_decl: &EvalTrait, method: &str) -> bool {
    trait_decl
        .methods()
        .iter()
        .any(|trait_method| trait_method.name().eq_ignore_ascii_case(method))
}

/// Returns case-insensitive method names declared directly by a pending trait.
pub(super) fn trait_method_name_set(trait_decl: &EvalTrait) -> std::collections::HashSet<String> {
    trait_decl
        .methods()
        .iter()
        .map(|method| method.name().to_ascii_lowercase())
        .collect()
}

/// Returns case-insensitive method names declared directly by a pending class.
pub(super) fn class_method_name_set(class: &EvalClass) -> std::collections::HashSet<String> {
    class
        .methods()
        .iter()
        .map(|method| method.name().to_ascii_lowercase())
        .collect()
}

/// Appends trait constants while enforcing PHP-compatible same-name conflicts.
pub(super) fn append_eval_trait_constants(
    trait_decl: &EvalTrait,
    class_constants: &[EvalClassConstant],
    trait_constants: &mut std::collections::HashMap<String, EvalClassConstant>,
    constants: &mut Vec<EvalClassConstant>,
) -> Result<(), EvalStatus> {
    for constant in trait_decl.constants() {
        if let Some(class_constant) = class_constants
            .iter()
            .find(|class_constant| class_constant.name() == constant.name())
        {
            validate_eval_trait_constant_compatibility(class_constant, constant)?;
            continue;
        }
        if let Some(existing) = trait_constants.get(constant.name()) {
            validate_eval_trait_constant_compatibility(existing, constant)?;
            continue;
        }
        let constant = constant
            .clone()
            .with_trait_origin(trait_decl.name().to_string());
        trait_constants.insert(constant.name().to_string(), constant.clone());
        constants.push(constant);
    }
    Ok(())
}

/// Validates that a same-name trait constant definition is compatible with PHP composition.
pub(super) fn validate_eval_trait_constant_compatibility(
    existing: &EvalClassConstant,
    incoming: &EvalClassConstant,
) -> Result<(), EvalStatus> {
    if existing.visibility() == incoming.visibility()
        && existing.is_final() == incoming.is_final()
        && existing.value() == incoming.value()
    {
        Ok(())
    } else {
        Err(EvalStatus::RuntimeFatal)
    }
}

/// Appends trait properties while enforcing PHP-compatible same-name conflicts.
pub(super) fn append_eval_trait_properties(
    trait_decl: &EvalTrait,
    class_properties: &[EvalClassProperty],
    trait_properties: &mut std::collections::HashMap<String, EvalClassProperty>,
    properties: &mut Vec<EvalClassProperty>,
) -> Result<(), EvalStatus> {
    for property in trait_decl.properties() {
        if let Some(class_property) = class_properties
            .iter()
            .find(|class_property| class_property.name() == property.name())
        {
            validate_eval_trait_property_compatibility(class_property, property)?;
            continue;
        }
        if let Some(existing) = trait_properties.get(property.name()) {
            let resolved = resolve_eval_trait_property_conflict(existing, property)?;
            if &resolved != existing {
                trait_properties.insert(property.name().to_string(), resolved.clone());
                if let Some(slot) = properties
                    .iter_mut()
                    .find(|candidate| candidate.name() == property.name())
                {
                    *slot = resolved;
                }
            }
            continue;
        }
        let property = property
            .clone()
            .with_trait_origin(trait_decl.name().to_string());
        trait_properties.insert(property.name().to_string(), property.clone());
        properties.push(property);
    }
    Ok(())
}

/// Validates that a same-name trait property definition is compatible with PHP composition.
pub(super) fn validate_eval_trait_property_compatibility(
    existing: &EvalClassProperty,
    incoming: &EvalClassProperty,
) -> Result<(), EvalStatus> {
    resolve_eval_trait_property_conflict(existing, incoming).map(|_| ())
}

/// Resolves compatible same-name properties imported from classes and traits.
pub(super) fn resolve_eval_trait_property_conflict(
    existing: &EvalClassProperty,
    incoming: &EvalClassProperty,
) -> Result<EvalClassProperty, EvalStatus> {
    if existing.is_abstract() && !incoming.is_abstract() {
        return class_property_satisfies_abstract_contract(incoming, existing)
            .then(|| incoming.clone())
            .ok_or(EvalStatus::RuntimeFatal);
    }
    if incoming.is_abstract() && !existing.is_abstract() {
        return class_property_satisfies_abstract_contract(existing, incoming)
            .then(|| existing.clone())
            .ok_or(EvalStatus::RuntimeFatal);
    }
    if existing.is_abstract() && incoming.is_abstract() {
        return eval_trait_abstract_properties_are_compatible(existing, incoming)
            .then(|| merge_abstract_property_contracts(existing, incoming))
            .ok_or(EvalStatus::RuntimeFatal);
    }
    if eval_trait_concrete_properties_are_compatible(existing, incoming) {
        Ok(existing.clone())
    } else {
        Err(EvalStatus::RuntimeFatal)
    }
}

/// Returns whether two concrete same-name trait properties are identical enough to deduplicate.
pub(super) fn eval_trait_concrete_properties_are_compatible(
    existing: &EvalClassProperty,
    incoming: &EvalClassProperty,
) -> bool {
    existing.visibility() == incoming.visibility()
        && existing.set_visibility() == incoming.set_visibility()
        && existing.is_static() == incoming.is_static()
        && existing.is_final() == incoming.is_final()
        && existing.is_readonly() == incoming.is_readonly()
        && existing.is_abstract() == incoming.is_abstract()
        && existing.has_get_hook() == incoming.has_get_hook()
        && existing.has_set_hook() == incoming.has_set_hook()
        && existing.requires_get_hook() == incoming.requires_get_hook()
        && existing.requires_set_hook() == incoming.requires_set_hook()
        && existing.is_virtual() == incoming.is_virtual()
        && existing.property_type() == incoming.property_type()
        && existing.set_hook_type() == incoming.set_hook_type()
        && existing.default() == incoming.default()
}

/// Returns whether two abstract trait property contracts can be merged.
pub(super) fn eval_trait_abstract_properties_are_compatible(
    existing: &EvalClassProperty,
    incoming: &EvalClassProperty,
) -> bool {
    existing.visibility() == incoming.visibility()
        && existing.set_visibility() == incoming.set_visibility()
        && existing.is_static() == incoming.is_static()
        && existing.is_final() == incoming.is_final()
        && existing.is_readonly() == incoming.is_readonly()
        && existing.property_type() == incoming.property_type()
        && existing.set_hook_type() == incoming.set_hook_type()
        && existing.default() == incoming.default()
}

/// Appends trait methods unless the class provides a same-name method.
pub(in crate::interpreter) fn append_eval_trait_methods(
    trait_decl: &EvalTrait,
    trait_adaptations: &[EvalTraitAdaptation],
    class_method_names: &std::collections::HashSet<String>,
    trait_method_names: &mut std::collections::HashSet<String>,
    methods: &mut Vec<EvalClassMethod>,
) -> Result<(), EvalStatus> {
    for method in trait_decl.methods() {
        if trait_method_suppressed_by_insteadof(trait_decl.name(), method.name(), trait_adaptations)
        {
            continue;
        }
        let key = method.name().to_ascii_lowercase();
        if class_method_names.contains(&key) {
            continue;
        }
        let method = method
            .clone()
            .with_trait_origin(trait_decl.name().to_string());
        let method = apply_trait_visibility_adaptations(
            trait_decl.name(),
            &method,
            trait_adaptations,
        );
        if !trait_method_names.insert(key.clone()) {
            // An ABSTRACT trait method is a requirement, not a body: it never collides with a
            // concrete one of the same name from another trait, in either order. symfony's
            // `CurlResponse` uses `CommonResponseTrait` (abstract `close()`) and
            // `TransportResponseTrait` (concrete `close()`); refusing the pair left the class
            // undeclared and the Symfony demo dead at boot.
            let existing = methods
                .iter()
                .position(|candidate| candidate.name().eq_ignore_ascii_case(&key));
            match existing {
                _ if method.is_abstract() => continue,
                Some(index) if methods[index].is_abstract() => {
                    methods[index] = method;
                    continue;
                }
                _ => return Err(EvalStatus::RuntimeFatal),
            }
        }
        methods.push(method);
    }
    append_eval_trait_method_aliases(
        trait_decl,
        trait_adaptations,
        class_method_names,
        trait_method_names,
        methods,
    )
}

/// Appends trait method aliases declared with `as`.
pub(super) fn append_eval_trait_method_aliases(
    trait_decl: &EvalTrait,
    trait_adaptations: &[EvalTraitAdaptation],
    class_method_names: &std::collections::HashSet<String>,
    trait_method_names: &mut std::collections::HashSet<String>,
    methods: &mut Vec<EvalClassMethod>,
) -> Result<(), EvalStatus> {
    for adaptation in trait_adaptations {
        let EvalTraitAdaptation::Alias {
            trait_name,
            method,
            alias: Some(alias),
            visibility,
        } = adaptation
        else {
            continue;
        };
        if !trait_adaptation_target_matches(
            trait_name.as_deref(),
            method,
            trait_decl.name(),
            method,
        ) {
            continue;
        }
        let Some(source_method) = trait_decl
            .methods()
            .iter()
            .find(|trait_method| trait_method.name().eq_ignore_ascii_case(method))
        else {
            if trait_name.is_some() {
                return Err(EvalStatus::RuntimeFatal);
            }
            continue;
        };
        let mut alias_method = source_method
            .clone()
            .with_trait_origin(trait_decl.name().to_string())
            .renamed(alias.clone());
        if let Some(visibility) = visibility {
            alias_method = alias_method.with_visibility_override(*visibility);
        }
        let key = alias_method.name().to_ascii_lowercase();
        if class_method_names.contains(&key) {
            continue;
        }
        if trait_method_names.contains(&key)
            && source_method.name().eq_ignore_ascii_case(alias)
            && alias_method.visibility() == source_method.visibility()
        {
            continue;
        }
        if !trait_method_names.insert(key) {
            return Err(EvalStatus::RuntimeFatal);
        }
        methods.push(alias_method);
    }
    Ok(())
}

/// Returns whether an `insteadof` adaptation suppresses this trait method import.
pub(super) fn trait_method_suppressed_by_insteadof(
    trait_name: &str,
    method_name: &str,
    trait_adaptations: &[EvalTraitAdaptation],
) -> bool {
    trait_adaptations.iter().any(|adaptation| {
        let EvalTraitAdaptation::InsteadOf {
            trait_name: selected_trait,
            method,
            instead_of,
        } = adaptation
        else {
            return false;
        };
        method.eq_ignore_ascii_case(method_name)
            && instead_of
                .iter()
                .any(|suppressed| same_eval_class_name(suppressed, trait_name))
            && !selected_trait
                .as_deref()
                .is_some_and(|selected| same_eval_class_name(selected, trait_name))
    })
}

/// Applies visibility-only `as` adaptations to an imported trait method.
pub(super) fn apply_trait_visibility_adaptations(
    trait_name: &str,
    method: &EvalClassMethod,
    trait_adaptations: &[EvalTraitAdaptation],
) -> EvalClassMethod {
    let mut method = method.clone();
    for adaptation in trait_adaptations {
        let EvalTraitAdaptation::Alias {
            trait_name: target_trait,
            method: target_method,
            alias: None,
            visibility: Some(visibility),
        } = adaptation
        else {
            continue;
        };
        if trait_adaptation_target_matches(
            target_trait.as_deref(),
            target_method,
            trait_name,
            method.name(),
        ) {
            method = method.with_visibility_override(*visibility);
        }
    }
    method
}

/// Returns whether an adaptation target selects one trait method.
pub(super) fn trait_adaptation_target_matches(
    target_trait: Option<&str>,
    target_method: &str,
    trait_name: &str,
    method_name: &str,
) -> bool {
    target_method.eq_ignore_ascii_case(method_name)
        && target_trait.map_or(true, |target_trait| {
            same_eval_class_name(target_trait, trait_name)
        })
}

/// Rejects non-enum classes that implement PHP's native enum interfaces.
pub(super) fn validate_eval_class_does_not_implement_enum_interfaces(
    class: &EvalClass,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    if pending_class_interface_names(class, context)
        .iter()
        .any(|interface| eval_builtin_enum_interface_name(interface))
    {
        Err(EvalStatus::RuntimeFatal)
    } else {
        Ok(())
    }
}

/// Rejects eval classes and enums that implement PHP's Throwable contract without already
/// inheriting it from their parent.
///
/// PHP's rule ("Class X cannot implement interface Throwable, extend Exception or Error
/// instead", `php -n` 8.5.6) fires only when the class's ancestry does not already implement
/// Throwable. A subclass of a Throwable-implementing native or eval class may freely implement
/// an interface that itself extends Throwable -- Symfony's exception hierarchy relies on exactly
/// this: `final class FormatException extends \LogicException implements ExceptionInterface`
/// where `ExceptionInterface extends \Throwable`. Rejecting it unconditionally blocked every
/// Symfony-shaped exception subclass at declaration.
pub(super) fn validate_eval_class_does_not_implement_throwable_interfaces(
    class: &EvalClass,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !pending_class_interface_names(class, context)
        .iter()
        .any(|interface| eval_builtin_throwable_interface_name(interface))
    {
        return Ok(());
    }
    if let Some(parent) = class.parent() {
        if eval_class_or_native_parent_is_throwable(parent, context, values)? {
            return Ok(());
        }
    }
    Err(EvalStatus::RuntimeFatal)
}

/// Returns whether a class name -- eval-declared or native -- already implements Throwable.
///
/// Checks the eval class table first (`context.class_is_a`), then falls back to the runtime's
/// own AOT instanceof machinery for a native name never itself eval-declared (e.g. `LogicException`
/// as a bare `extends` target, which has no `native_class_parent` entry of its own since nothing
/// eval-declared has registered it as an ancestor yet). This mirrors exactly what `is_a($name,
/// 'Throwable', true)` does for a native class-name string.
fn eval_class_or_native_parent_is_throwable(
    parent: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let parent = context
        .resolve_class_name(parent)
        .unwrap_or_else(|| parent.trim_start_matches('\\').to_string());
    if context.class_is_a(&parent, "Throwable", false) || native_class_is_a(&parent, "Throwable", context) {
        return Ok(true);
    }
    let name_cell = values.string(&parent)?;
    let result = values.object_is_a(name_cell, "Throwable", false);
    values.release(name_cell)?;
    result
}

/// Validates abstract/final modifiers on an eval-declared class and its methods.
pub(super) fn validate_eval_class_modifiers(
    class: &EvalClass,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let mut stage = "class_flags";
    let mut member = None;
    let result = (|| {
        if class.is_abstract() && class.is_final() {
            return Err(EvalStatus::RuntimeFatal);
        }
        stage = "class_attribute_targets";
        validate_eval_class_attribute_targets(class.attributes())?;
        stage = "readonly_dynamic_properties";
        if class.is_readonly_class() && eval_class_has_allow_dynamic_properties_attribute(class) {
            return Err(EvalStatus::RuntimeFatal);
        }
        stage = "declared_constants";
        validate_eval_declared_constants(class.constants())?;
        for constant in class.constants() {
            stage = "constant_parent_redeclaration";
            validate_constant_parent_redeclaration(class, constant, context, values)?;
        }
        stage = "declared_properties";
        validate_eval_declared_properties(class, context)?;
        for property in class.properties() {
            stage = "property_parent_redeclaration";
            validate_property_parent_redeclaration(class, property, context, values)?;
        }
        for method in class.methods() {
            member = Some(method.name());
            stage = "method_attribute_targets";
            validate_eval_method_attribute_targets(method.attributes())?;
            for attributes in method.parameter_attributes() {
                validate_eval_core_attribute_targets(attributes, 32)?;
            }
            stage = "magic_method";
            validate_eval_magic_method(method)?;
            stage = "method_flags";
            if method.is_abstract() && method.is_final() {
                return Err(EvalStatus::RuntimeFatal);
            }
            if method.is_abstract() && method.visibility() == EvalVisibility::Private {
                return Err(EvalStatus::RuntimeFatal);
            }
            if method.is_static() && method.name().eq_ignore_ascii_case("__construct") {
                return Err(EvalStatus::RuntimeFatal);
            }
            if method.is_abstract() && !class.is_abstract() {
                return Err(EvalStatus::RuntimeFatal);
            }
            stage = "eval_parent_override";
            validate_method_parent_override(class, method, context, values)?;
            stage = "aot_parent_override";
            validate_method_aot_parent_override(class, method, context, values)?;
            stage = "override_attribute";
            validate_eval_override_attribute(class, method, context, values)?;
        }
        Ok(())
    })();
    if let Err(status) = result.as_ref() {
        if crate::eval_trace::enabled() {
            eprintln!(
                "[elephc-eval-trace] phase=class_modifier_error class={:?} member={member:?} stage={stage} status={status:?}",
                class.name(),
            );
        }
    }
    result
}

/// Returns whether a class carries PHP's global `#[AllowDynamicProperties]` attribute.
pub(super) fn eval_class_has_allow_dynamic_properties_attribute(class: &EvalClass) -> bool {
    eval_attributes_have_global_builtin_attribute(class.attributes(), "AllowDynamicProperties")
}
