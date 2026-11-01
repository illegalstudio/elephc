//! Purpose:
//! Registers eval class declarations and validates their direct parent.
//!
//! Called from:
//! - Statement dispatch for class and anonymous-class declarations.
//!
//! Key details:
//! - Registration occurs only after trait expansion and declaration validation succeed.

use std::sync::Arc;
use super::*;

/// Registers an eval-declared class in the dynamic class table.
pub(in crate::interpreter) fn execute_class_decl_stmt(
    class: &Arc<EvalClass>,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let parent = class.parent().map(|parent| ensure_generic_class(parent, None, "", context, scope, values)).transpose()?;
    let interfaces: Vec<String> = class.interfaces().iter().map(|name| ensure_generic_class(name, None, "", context, scope, values)).collect::<Result<_, _>>()?;
    let traits: Vec<String> = class.traits().iter().map(|name| ensure_generic_class(name, None, "", context, scope, values)).collect::<Result<_, _>>()?;
    let adaptations = concrete_trait_adaptations(class.trait_adaptations(), &traits);
    // A class with no generic relation to specialize stays the shared parsed class; only a
    // class whose relations changed is copied.
    let relations_unchanged = parent.as_deref() == class.parent()
        && interfaces.as_slice() == class.interfaces()
        && traits.as_slice() == class.traits()
        && adaptations.as_slice() == class.trait_adaptations();
    let concrete = if relations_unchanged {
        Arc::clone(class)
    } else {
        Arc::new((**class).clone().with_specialized_relations(parent, interfaces, traits, adaptations))
    };
    let class = &concrete;
    let name = class.name().trim_start_matches('\\');
    let context_has_class = context.has_class(name);
    let context_has_interface = context.has_interface(name);
    let context_has_trait = context.has_trait(name);
    let context_has_enum = context.has_enum(name);
    let runtime_has_class = trace_class_decl_result(
        class,
        "runtime_class_exists",
        values.class_exists(name),
    )?;
    let runtime_has_interface = trace_class_decl_result(
        class,
        "runtime_interface_exists",
        eval_runtime_interface_exists(name, values),
    )?;
    let runtime_has_trait =
        trace_class_decl_result(class, "runtime_trait_exists", values.trait_exists(name))?;
    let runtime_has_enum =
        trace_class_decl_result(class, "runtime_enum_exists", values.enum_exists(name))?;
    let materialize_aot_include_class = runtime_has_class
        && !runtime_has_interface
        && !runtime_has_trait
        && !runtime_has_enum
        && context.executing_include();
    if context.generic_template(name).is_some()
        || context_has_class
        || context_has_interface
        || context_has_trait
        || context_has_enum
        || (runtime_has_class && !materialize_aot_include_class)
        || runtime_has_interface
        || runtime_has_trait
        || runtime_has_enum
    {
        trace_class_decl_duplicate(
            class,
            [
                context_has_class,
                context_has_interface,
                context_has_trait,
                context_has_enum,
                runtime_has_class,
                runtime_has_interface,
                runtime_has_trait,
                runtime_has_enum,
            ],
        );
        return Err(EvalStatus::RuntimeFatal);
    }
    // Nothing to expand, nothing to stamp: the parsed class IS the declared class, so it is
    // shared rather than copied. `expand_eval_class_traits` answers `class.clone()` for a
    // trait-free class and both stamps are no-ops in that case, which made every request deep-copy
    // every method body of every class it declared -- and free them all again at the request
    // boundary. The generated Symfony container alone is thousands of methods.
    let prepared = if class.traits().is_empty()
        && !class.is_readonly_class()
        && !context.strict_types()
    {
        Arc::clone(class)
    } else {
        Arc::new(
            trace_class_decl_result(
                class,
                "expand_traits",
                expand_eval_class_traits(class, context, values),
            )?
            .with_readonly_properties()
            .with_strict_types(context.strict_types()),
        )
    };
    // A child that redeclares an inherited property keeps the parent's hooks and backing storage.
    // Only a class that actually changes is copied, so the shared parsed class stays shared.
    let prepared = match resolve_eval_inherited_property_hooks(&prepared, context, values)? {
        Some(resolved) => Arc::new(resolved),
        None => prepared,
    };
    let class = &prepared;
    // The parent-override half of php's variance check runs inside the modifier validation, so
    // the classes an override's signature names must be loaded before it; the interface half is
    // loaded again below once the interfaces themselves are available.
    trace_class_decl_result(
        class,
        "autoload_parent_variance_types",
        autoload_pending_class_variance_types(class, context, values),
    )?;
    trace_class_decl_result(
        class,
        "validate_modifiers",
        validate_eval_class_modifiers(class, context, values),
    )?;
    ensure_eval_class_parent_available(class, context, values)?;
    let native_parent = trace_class_decl_result(
        class,
        "validate_parent",
        validate_eval_class_parent(class, context, values),
    )?;
    for interface in class.interfaces() {
        if !context.has_interface(interface) && !eval_runtime_interface_exists(interface, values)? {
            // PHP resolves an interface named by `implements` through the active
            // autoloaders before rejecting the declaration. Dynamic classes must
            // follow that same rule for interfaces provided by another included
            // source file or by generated/AOT metadata.
            let _ = eval_spl_autoload_class(interface, context, values)?;
        }
        if !context.has_interface(interface) && !eval_runtime_interface_exists(interface, values)? {
            // The autoloader ran and the interface is still absent. For a compiled program that
            // has one ordinary cause: the compiler already included the interface's file, so the
            // include the autoloader performed was skipped on that mark
            // (`interpreter::include_exec`, `phase=compiler_included_skip`). The mark promises the
            // COMPILED world performed the inclusion, and an interface's CONTRACT is not readable
            // from there — it has to be validated against this pending class.
            //
            // Same shape, same retry as `ensure_eval_traits_available`: once, with the mark
            // ignored, and only after the ordinary path has failed. Bypassing it up front would
            // re-interpret files the compiled world already has, and a class declared twice hits
            // the duplicate guard at the top of this function.
            //
            // MEASURED on Symfony's console: `RoutingControllerPass implements CompilerPassInterface`,
            // whose file traced `compiler_included_skip` immediately before this return.
            crate::interpreter::include_exec::with_compiler_included_sources_reincluded(|| {
                eval_spl_autoload_class(interface, context, values)
            })?;
            #[cfg(not(test))]
            context.sync_global_eval_classes();
        }
        if !context.has_interface(interface) && !eval_runtime_interface_exists(interface, values)? {
            // Traced, unlike the bare `return Err` this replaces: an untraced refusal here is what
            // made the failure carry NO `class_decl_*` line at all and look like it happened
            // outside every stage.
            trace_class_decl_result::<()>(
                class,
                "interface_unavailable",
                Err(EvalStatus::RuntimeFatal),
            )?;
        }
    }
    trace_class_decl_result(
        class,
        "validate_throwable_interfaces",
        validate_eval_class_does_not_implement_throwable_interfaces(class, context, values),
    )?;
    trace_class_decl_result(
        class,
        "validate_enum_interfaces",
        validate_eval_class_does_not_implement_enum_interfaces(class, context),
    )?;
    trace_class_decl_result(
        class,
        "autoload_variance_types",
        autoload_pending_class_variance_types(class, context, values),
    )?;
    trace_class_decl_result(
        class,
        "validate_interface_members",
        validate_declared_class_interface_members(class, context, values),
    )?;
    trace_class_decl_result(
        class,
        "validate_builtin_interface_members",
        validate_declared_class_builtin_interface_members(class, context, values),
    )?;
    trace_class_decl_result(
        class,
        "validate_aot_interface_members",
        validate_declared_class_aot_interface_members(class, context, values),
    )?;
    if !class.is_abstract() {
        trace_class_decl_result(
            class,
            "validate_concrete_requirements",
            validate_concrete_class_requirements(class, context, values),
        )?;
        trace_class_decl_result(
            class,
            "validate_builtin_interface_requirements",
            validate_concrete_class_builtin_interface_requirements(class, context, values),
        )?;
        trace_class_decl_result(
            class,
            "validate_aot_parent_requirements",
            validate_concrete_class_aot_parent_requirements(class, context, values),
        )?;
        trace_class_decl_result(
            class,
            "validate_aot_interface_requirements",
            validate_concrete_class_aot_interface_requirements(class, context, values, false),
        )?;
    }
    let declaration_file = eval_declaration_source_file(context);
    if context.define_class(Arc::clone(class)) {
        context.set_class_source_file(class.name(), declaration_file);
        if let Some(parent) = native_parent.as_deref() {
            if !context.define_native_class_parent(class.name(), parent) {
                trace_class_decl_stage(class, "define_native_parent", EvalStatus::RuntimeFatal);
                return Err(EvalStatus::RuntimeFatal);
            }
        }
        trace_class_decl_result(
            class,
            "initialize_constants",
            initialize_eval_declared_constants(
                class.name(),
                class.constants(),
                context,
                scope,
                values,
            ),
        )?;
        let result = trace_class_decl_result(
            class,
            "initialize_static_properties",
            initialize_eval_static_properties(class, context, scope, values),
        );
        if result.is_ok() && crate::eval_trace::enabled() {
            eprintln!(
                "[elephc-eval-trace] phase=class_decl_ok class={:?}",
                class.name(),
            );
        }
        result
    } else {
        trace_class_decl_stage(class, "define_class", EvalStatus::RuntimeFatal);
        Err(EvalStatus::RuntimeFatal)
    }
}

/// Loads a missing parent class before validating an eval class declaration.
///
/// PHP resolves an `extends` target through the active SPL autoload chain at declaration time.
/// The callback can register the parent in another request-global eval context, so synchronize
/// local metadata before requiring that the resolved symbol is specifically a class.
fn ensure_eval_class_parent_available(
    class: &EvalClass,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(parent) = class.parent() else {
        return Ok(());
    };
    let parent = context
        .resolve_class_name(parent)
        .unwrap_or_else(|| parent.trim_start_matches('\\').to_string());
    if context.class(&parent).is_some() || values.class_exists(&parent)? {
        return Ok(());
    }
    crate::interpreter::eval_spl_autoload_classlike_definition(&parent, context, values)?;
    #[cfg(not(test))]
    context.sync_global_eval_classes();
    if context.class(&parent).is_some() || values.class_exists(&parent)? {
        Ok(())
    } else {
        // php throws a catchable `Error` here, not a fatal: Symfony's `FileLoader` loads every
        // class under `src/` and records the ones whose parent is missing (the demo's
        // `AppFixtures` extends a dev-only bundle's `Fixture`). The fatal killed every
        // `debug:container`-family command of the compiled console.
        super::property_validation::eval_throw_class_not_found_error(&parent, context, values)
    }
}

/// Preserves a class-declaration result while tracing the stage that rejected it.
fn trace_class_decl_result<T>(
    class: &EvalClass,
    stage: &str,
    result: Result<T, EvalStatus>,
) -> Result<T, EvalStatus> {
    if let Err(status) = result.as_ref() {
        if crate::eval_trace::enabled() {
            eprintln!(
                "[elephc-eval-trace] phase=class_decl_error class={:?} stage={stage} status={status:?}",
                class.name(),
            );
        }
    }
    result
}

/// Emits one opt-in class-declaration stage failure without changing control flow.
fn trace_class_decl_stage(class: &EvalClass, stage: &str, status: EvalStatus) {
    if !crate::eval_trace::enabled() {
        return;
    }
    eprintln!(
        "[elephc-eval-trace] phase=class_decl_error class={:?} stage={stage} status={status:?}",
        class.name(),
    );
}

/// Traces which declaration registry already contains a class-like name.
fn trace_class_decl_duplicate(class: &EvalClass, matches: [bool; 8]) {
    if !crate::eval_trace::enabled() {
        return;
    }
    eprintln!(
        "[elephc-eval-trace] phase=class_decl_duplicate class={:?} context_class={} context_interface={} context_trait={} context_enum={} runtime_class={} runtime_interface={} runtime_trait={} runtime_enum={}",
        class.name(),
        matches[0],
        matches[1],
        matches[2],
        matches[3],
        matches[4],
        matches[5],
        matches[6],
        matches[7],
    );
}

/// Preserves inherited hooks and backing storage when a child redeclares a property.
///
/// Answers `None` when no property needs adjusting, so a class without such a redeclaration is
/// never copied.
fn resolve_eval_inherited_property_hooks(
    class: &EvalClass,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalClass>, EvalStatus> {
    let Some(parent) = class.parent() else { return Ok(None); };
    let abstract_class = class.is_abstract();
    let mut properties = class.properties().to_vec();
    let mut changed = false;
    for property in &mut properties {
        if property.is_static() { continue; }
        if let Some((_, inherited)) = context.class_property(parent, property.name()) {
            if inherited.is_static() || inherited.visibility() == EvalVisibility::Private {
                continue;
            }
            *property = property.clone()
                .with_hooks(property.has_get_hook() || inherited.has_get_hook(),
                    property.has_set_hook() || inherited.has_set_hook())
                .with_virtual(property.is_virtual() && inherited.is_virtual());
            if abstract_class && property.is_readonly() && !property.has_set_hook()
                && inherited.requires_set_hook()
            {
                *property = property.clone().with_inherited_abstract_setter();
            }
            changed = true;
        } else {
            let native_parent = context.class_native_parent_name(parent)
                .unwrap_or_else(|| parent.to_string());
            if abstract_class && property.is_readonly() && !property.has_set_hook()
                && context.native_abstract_property_requirements(&native_parent).iter()
                    .any(|(_, contract)| contract.name() == property.name() && contract.requires_set())
            {
                *property = property.clone().with_inherited_abstract_setter();
                changed = true;
            }
            if values.reflection_property_flags(&native_parent, property.name())?
                .is_some_and(|flags| flags & (EVAL_REFLECTION_MEMBER_FLAG_STATIC
                    | EVAL_REFLECTION_MEMBER_FLAG_PRIVATE | EVAL_REFLECTION_MEMBER_FLAG_VIRTUAL) == 0)
            {
                *property = property.clone().with_virtual(false);
                changed = true;
            }
        }
    }
    if !changed { return Ok(None); }
    Ok(Some(class.clone().with_properties(properties)))
}

/// Validates an eval class parent and returns an AOT parent name when the parent is runtime-backed.
pub(super) fn validate_eval_class_parent(
    class: &EvalClass,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<String>, EvalStatus> {
    let Some(parent) = class.parent() else {
        return Ok(None);
    };
    let parent = context
        .resolve_class_name(parent)
        .unwrap_or_else(|| parent.trim_start_matches('\\').to_string());
    if let Some(parent_class) = context.class(&parent) {
        if parent_class.is_final()
            || parent_class.is_readonly_class() != class.is_readonly_class()
            || context.class_is_a(&parent, class.name(), false)
        {
            return Err(EvalStatus::RuntimeFatal);
        }
        return Ok(None);
    }
    let Some((parent_is_final, parent_is_readonly)) =
        eval_reflection_aot_class_inheritance_modifiers(&parent, values)?
    else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if parent_is_final
        || parent_is_readonly != class.is_readonly_class()
        || native_class_is_a(&parent, class.name(), context)
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(Some(parent))
}

/// Registers one eval anonymous class expression if this execution has not seen it yet.
pub(in crate::interpreter) fn ensure_eval_anonymous_class_decl(
    class: &EvalClass,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !class.is_anonymous() {
        return Err(EvalStatus::RuntimeFatal);
    }
    if let Some(existing) = context.class(class.name()) {
        return if existing.is_anonymous() {
            Ok(())
        } else {
            Err(EvalStatus::RuntimeFatal)
        };
    }
    // An anonymous class comes from an expression node that owns its class, so there is nothing
    // to share with; this one allocation per `new class` is what the declaration path expects.
    execute_class_decl_stmt(&Arc::new(class.clone()), context, scope, values)
}

/// Returns the file php names as the declaring file for a class-like the interpreter declares.
///
/// php spells it two ways and `__FILE__` only separates them at the OUTERMOST level: inside eval'd
/// code `__FILE__` is the file the `eval()` was written in, while `getFileName()` is
/// `FILE(LINE) : eval()'d code`. An `eval()` inside an INCLUDED file keeps the include's file
/// magic, so the two cannot be told apart from it; the innermost include-or-eval frame can, and an
/// eval frame carries the very file and line php prints.
///
/// Shared by the class, interface, trait and enum declarations so the four cannot answer
/// differently -- reflection reads this for `getFileName()`, and a class-like with no record falls
/// back to the generated program's single source file, which is the ENTRY point.
pub(in crate::interpreter) fn eval_declaration_source_file(context: &ElephcEvalContext) -> String {
    // A class-like declared while an INCLUDED file executes is named after that file. The frame
    // scan below used to answer first and found the OUTER eval frame -- the dev container's
    // autoloader requires vendor files from inside `eval()`, so every vendor declaration was
    // renamed to `... : eval()'d code` and an exception thrown in a trait method reported the
    // using class's file with the trait's line. An eval that runs *inside* an include clears the
    // include's `__FILE__` override and pushes a non-include execution marker, so it still takes
    // the frame scan below.
    if context.executing_include() {
        if let Some(file) = context.call_site().3 {
            return file;
        }
    }
    context
        .with_call_frames(|frames| {
            frames.iter().rev().find_map(|frame| match frame.function.as_str() {
                // A frame's `file` is php's `__FILE__` for the code inside it, which for an eval
                // called from COMPILED code is already the marked form; for an eval written inside
                // an included file it is the include's plain path and the marker is added here.
                "eval" if frame.file.ends_with(") : eval()'d code") => {
                    Some(Some(frame.file.clone()))
                }
                "eval" => Some(Some(format!(
                    "{}({}) : eval()'d code",
                    frame.file, frame.line
                ))),
                "include" | "include_once" | "require" | "require_once" => Some(None),
                _ => None,
            })
        })
        .flatten()
        .unwrap_or_else(|| context.eval_file_magic())
}
