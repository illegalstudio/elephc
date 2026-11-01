//! Purpose:
//! Validates declared methods and properties against interface member contracts.
//!
//! Called from:
//! - Class declaration validation before abstract-requirement collection.
//!
//! Key details:
//! - Eval, builtin, and AOT interface signatures use the same compatibility checks.

use super::*;

/// Validates declared or inherited class members that already cover eval interface contracts.
pub(super) fn validate_declared_class_interface_members(
    class: &EvalClass,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for interface in pending_class_contract_interface_names(class, context) {
        if !context.has_interface(&interface) {
            continue;
        }
        validate_declared_class_interface_methods(class, &interface, context, values)?;
        validate_declared_class_interface_properties(class, &interface, context)?;
    }
    Ok(())
}

/// Loads the classes a pending class's method signatures name before those signatures are checked
/// against the interfaces and parent it inherits from.
///
/// php performs a variance check at declaration time, and a check that meets a class it has not
/// seen yet AUTOLOADS it: `IdleConnectionMiddleware::wrap(Driver $driver): IdleConnectionDriver`
/// implements `Middleware::wrap(Driver $driver): Driver`, and php loads `IdleConnectionDriver`
/// to learn that it is a `Driver`. Eval checked the unloaded name, could not prove the relation,
/// and refused the declaration -- the Symfony demo's Doctrine connection factory died there on
/// every page that touches the database.
///
/// Only methods that actually meet a contract are considered (an interface requirement or a
/// parent method of the same name), and only class-like names nothing knows yet are loaded, so a
/// class whose signatures need no proof loads exactly what it loaded before.
pub(super) fn autoload_pending_class_variance_types(
    class: &EvalClass,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let mut contracted: std::collections::HashSet<String> = std::collections::HashSet::new();
    for interface in pending_class_contract_interface_names(class, context) {
        for (_, requirement) in context.interface_method_requirements_with_owners(&interface) {
            contracted.insert(requirement.name().to_ascii_lowercase());
        }
    }
    for requirement in pending_class_aot_interface_method_requirements(class, context, values)? {
        contracted.insert(requirement.name.to_ascii_lowercase());
    }
    let mut names: Vec<String> = Vec::new();
    for method in class.methods() {
        let key = method.name().to_ascii_lowercase();
        // php exempts constructors from signature-compatibility checks, so it never resolves
        // their parameter types at declaration time. Autoloading them anyway killed the dev
        // container build: `ProfilerExtension::__construct(?Stopwatch $stopwatch = null)` has no
        // Stopwatch package installed, and php declares the class lazily without ever asking.
        if key == "__construct" {
            continue;
        }
        let meets_parent = class
            .parent()
            .is_some_and(|parent| context.class_method(parent, method.name()).is_some());
        if !meets_parent && !contracted.contains(&key) {
            continue;
        }
        let signature_types = method
            .return_type()
            .into_iter()
            .chain(method.parameter_types().iter().flatten());
        for declared in signature_types {
            for variant in declared.variants() {
                let EvalParameterTypeVariant::Class(name) = variant else {
                    continue;
                };
                let name = name.trim_start_matches('\\');
                if matches!(
                    name.to_ascii_lowercase().as_str(),
                    "self" | "static" | "parent"
                ) || name.eq_ignore_ascii_case(class.name())
                    || names.iter().any(|seen| seen.eq_ignore_ascii_case(name))
                {
                    continue;
                }
                names.push(name.to_string());
            }
        }
    }
    for name in names {
        // A name the COMPILED world already provides counts as loaded too: php never runs the
        // autoloader for a class it has. Running it anyway sent every compiled type a Laravel
        // signature names (`Container`, `Request`, ...) through the Composer loader and an
        // include that the compiler-included mark then skipped -- one wasted round per name
        // per declared class per request.
        if eval_class_relation_name_exists(&name, context, values)? {
            continue;
        }
        // The autoloader answers whether it produced the class; a name nothing supplies is left
        // for the signature check itself to judge, exactly as before.
        let _ = eval_spl_autoload_class(&name, context, values)?;
    }
    #[cfg(not(test))]
    context.sync_global_eval_classes();
    Ok(())
}

/// Validates declared class methods against PHP builtin runtime interface contracts.
pub(super) fn validate_declared_class_builtin_interface_members(
    class: &EvalClass,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for (requirement_owner, requirement) in
        pending_class_builtin_interface_method_requirements(class, context)
    {
        let Some((declaring_class, method)) =
            pending_class_method(class, requirement.name(), context)
        else {
            continue;
        };
        let accepts = |context: &ElephcEvalContext| {
            class_method_satisfies_builtin_interface_signature(
                &method,
                &declaring_class,
                &requirement,
                &requirement_owner,
                Some(class),
                context,
            )
        };
        if method.visibility() != EvalVisibility::Public
            || method.is_static() != requirement.is_static()
            || (!accepts(context)
                && (!autoload_override_signature_classes(
                    &method,
                    requirement.parameter_types(),
                    class,
                    context,
                    values,
                )? || !accepts(context)))
        {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    Ok(())
}

/// Validates declared class methods against generated/AOT interface contracts.
pub(super) fn validate_declared_class_aot_interface_members(
    class: &EvalClass,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for requirement in pending_class_aot_interface_method_requirements(class, context, values)? {
        let Some((declaring_class, method)) = pending_class_method(class, &requirement.name, context)
        else {
            continue;
        };
        let accepts = |context: &ElephcEvalContext| {
            class_method_satisfies_aot_interface_requirement(
                &method,
                &declaring_class,
                &requirement,
                Some(class),
                context,
                false,
            )
        };
        let required_parameter_types = requirement
            .signature
            .as_ref()
            .map(|signature| signature.parameter_types().to_vec())
            .unwrap_or_default();
        if !accepts(context)
            && (!autoload_override_signature_classes(
                &method,
                &required_parameter_types,
                class,
                context,
                values,
            )? || !accepts(context))
        {
            if crate::eval_trace::enabled() {
                eprintln!(
                    "[elephc-eval-trace] phase=aot_interface_method_mismatch class={:?} owner={:?} method={:?} declaring_class={declaring_class:?} actual_visibility={:?} required_static={} actual_static={} has_signature={}",
                    class.name(),
                    requirement.owner,
                    requirement.name,
                    method.visibility(),
                    requirement.is_static,
                    method.is_static(),
                    requirement.signature.is_some(),
                );
                if std::env::var_os("ELEPHC_EVAL_TRACE_SIGNATURES").is_some() {
                    eprintln!(
                        "[elephc-eval-trace] required_signature={:?}\n[elephc-eval-trace] actual_params={:?} defaults={:?} by_ref={:?} variadic={:?} types={:?} return={:?}",
                        requirement.signature,
                        method.params(),
                        method.parameter_defaults(),
                        method.parameter_is_by_ref(),
                        method.parameter_is_variadic(),
                        method.parameter_types(),
                        method.return_type(),
                    );
                }
            }
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    for (requirement_owner, requirement) in
        pending_class_aot_interface_property_requirements(class, context, values)?
    {
        let Some((declaring_class, property)) =
            pending_class_property_with_owner(class, requirement.name(), context)
        else {
            continue;
        };
        if !class_property_can_cover_interface_contract(
            &property,
            &declaring_class,
            &requirement,
            &requirement_owner,
            Some(class),
            context,
        ) {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    Ok(())
}

/// Validates class methods present for an eval interface, even on abstract classes.
pub(super) fn validate_declared_class_interface_methods(
    class: &EvalClass,
    interface_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for (requirement_owner, requirement) in
        context.interface_method_requirements_with_owners(interface_name)
    {
        let Some((declaring_class, method)) =
            pending_class_method(class, requirement.name(), context)
        else {
            continue;
        };
        let accepts = |context: &ElephcEvalContext| {
            class_method_satisfies_interface_signature(
                &method,
                &declaring_class,
                &requirement,
                &requirement_owner,
                Some(class),
                context,
            )
        };
        if method.visibility() != EvalVisibility::Public
            || method.is_static() != requirement.is_static()
            || (!accepts(context)
                && (!autoload_override_signature_classes(
                    &method,
                    requirement.parameter_types(),
                    class,
                    context,
                    values,
                )? || !accepts(context)))
        {
            trace_declared_eval_interface_method_mismatch(
                class,
                interface_name,
                &requirement_owner,
                &requirement,
                &declaring_class,
                &method,
            );
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    Ok(())
}

/// Emits an opt-in trace when an eval class method violates an eval interface contract.
fn trace_declared_eval_interface_method_mismatch(
    class: &EvalClass,
    interface_name: &str,
    requirement_owner: &str,
    requirement: &EvalInterfaceMethod,
    declaring_class: &str,
    method: &EvalClassMethod,
) {
    if !crate::eval_trace::enabled() {
        return;
    }
    eprintln!(
        "[elephc-eval-trace] phase=interface_method_mismatch class={:?} interface={interface_name:?} owner={requirement_owner:?} method={:?} declaring_class={declaring_class:?} actual_visibility={:?} required_static={} actual_static={}",
        class.name(),
        requirement.name(),
        method.visibility(),
        requirement.is_static(),
        method.is_static(),
    );
}

/// Validates class properties present for an eval interface, even on abstract classes.
pub(super) fn validate_declared_class_interface_properties(
    class: &EvalClass,
    interface_name: &str,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    for (requirement_owner, requirement) in
        context.interface_property_requirements_with_owners(interface_name)
    {
        let Some((declaring_class, property)) =
            pending_class_property_with_owner(class, requirement.name(), context)
        else {
            continue;
        };
        if !class_property_can_cover_interface_contract(
            &property,
            &declaring_class,
            &requirement,
            &requirement_owner,
            Some(class),
            context,
        ) {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    Ok(())
}

/// Returns a method from a pending class or one of its already registered parents.
pub(super) fn pending_class_method(
    class: &EvalClass,
    method_name: &str,
    context: &ElephcEvalContext,
) -> Option<(String, EvalClassMethod)> {
    if let Some(method) = class.method(method_name) {
        return Some((class.name().to_string(), method.clone()));
    }
    // Owned to match the arm above, which borrows from a class the context does not hold yet;
    // declaration validation runs once per class, not per call.
    class
        .parent()
        .and_then(|parent| context.class_method(parent, method_name))
        .map(|(declaring_class, method)| (declaring_class, (*method).clone()))
}

/// Validates one method declaration against inherited eval method metadata.
pub(super) fn validate_method_parent_override(
    class: &EvalClass,
    method: &EvalClassMethod,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(parent) = class.parent() else {
        return Ok(());
    };
    let Some((parent_declaring_class, parent_method)) = context.class_method(parent, method.name())
    else {
        return Ok(());
    };
    let is_constructor = method.name().eq_ignore_ascii_case("__construct");
    if parent_method.visibility() == EvalVisibility::Private && !is_constructor {
        return Ok(());
    }
    if parent_method.is_static() != method.is_static() {
        return Err(EvalStatus::RuntimeFatal);
    }
    if parent_method.is_final() {
        return Err(EvalStatus::RuntimeFatal);
    }
    if method.is_abstract() && !parent_method.is_abstract() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let prototype = if is_constructor {
        // A concrete implementation retains the original abstract constructor
        // contract. Interface contracts are validated independently below.
        let mut prototype = None;
        for owner in std::iter::once(parent_declaring_class.clone())
            .chain(context.class_parent_names(&parent_declaring_class))
        {
            if let Some((owner, candidate)) = context.class_own_method(&owner, method.name()) {
                if candidate.is_abstract() {
                    prototype = Some((owner, candidate));
                }
            }
        }
        prototype
    } else {
        None
    };
    let (prototype_owner, prototype_method) = match prototype.as_ref() {
        Some((owner, method)) => (owner, method),
        None if is_constructor => return Ok(()),
        None => (&parent_declaring_class, &parent_method),
    };
    if method_visibility_rank(method.visibility())
        < method_visibility_rank(parent_method.visibility())
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    let accepts = |context: &ElephcEvalContext| {
        class_method_signature_accepts(
            method,
            class.name(),
            prototype_method,
            prototype_owner,
            Some(class),
            context,
        )
    };
    if !accepts(context)
        && (!autoload_override_signature_classes(
            method,
            prototype_method.parameter_types(),
            class,
            context,
            values,
        )? || !accepts(context))
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(())
}

/// Autoloads the classes an override's signature names that no world has declared yet.
///
/// PHP decides variance with every class it compares loaded: `diff(): CarbonInterval` over
/// `DateTime::diff(): DateInterval` makes php autoload `CarbonInterval` to learn that it extends
/// `DateInterval`. The eval check read "no such class" as "no subtype" and refused a declaration
/// php accepts. The atoms that matter are the override's own return and parameter types (a return
/// class is walked UP to the required one) and the required parameter types (walked up to the
/// override's wider ones).
///
/// Called only after a check has FAILED, so a declaration php accepts without loading anything
/// loads nothing here either. Answers whether any autoload ran, so the caller re-checks only then.
fn autoload_override_signature_classes(
    method: &EvalClassMethod,
    required_parameter_types: &[Option<EvalParameterType>],
    class: &EvalClass,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let mut names = Vec::new();
    for ty in method
        .return_type()
        .into_iter()
        .chain(method.parameter_types().iter().flatten())
        .chain(required_parameter_types.iter().flatten())
    {
        for variant in ty.variants() {
            let EvalParameterTypeVariant::Class(name) = variant else {
                continue;
            };
            let name = name.trim_start_matches('\\');
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "self" | "static" | "parent"
            ) || name.eq_ignore_ascii_case(class.name().trim_start_matches('\\'))
            {
                continue;
            }
            let resolved = context
                .resolve_class_like_name(name)
                .unwrap_or_else(|| name.to_string());
            if context.has_class(&resolved)
                || context.has_interface(&resolved)
                || context.has_enum(&resolved)
                || names
                    .iter()
                    .any(|seen: &String| seen.eq_ignore_ascii_case(&resolved))
            {
                continue;
            }
            names.push(resolved);
        }
    }
    let mut loaded = false;
    for name in names {
        loaded |= eval_spl_autoload_class(&name, context, values)?;
    }
    #[cfg(not(test))]
    if loaded {
        context.sync_global_eval_classes();
    }
    Ok(loaded)
}

/// Validates one method declaration against inherited generated/AOT method metadata.
pub(super) fn validate_method_aot_parent_override(
    class: &EvalClass,
    method: &EvalClassMethod,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(parent) = pending_class_native_parent_name(class, context) else {
        return Ok(());
    };
    if !values.class_exists(&parent)? {
        return Ok(());
    }
    let Some(flags) = values.reflection_method_flags(&parent, method.name())? else {
        return Ok(());
    };
    if flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE != 0
        && !method.name().eq_ignore_ascii_case("__construct")
    {
        return Ok(());
    }
    let parent_is_static = flags & EVAL_REFLECTION_MEMBER_FLAG_STATIC != 0;
    if parent_is_static != method.is_static() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let parent_visibility = eval_aot_method_visibility(flags);
    if method_visibility_rank(method.visibility()) < method_visibility_rank(parent_visibility) {
        return Err(EvalStatus::RuntimeFatal);
    }
    if flags & EVAL_REFLECTION_MEMBER_FLAG_FINAL != 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    if method.is_abstract() && flags & EVAL_REFLECTION_MEMBER_FLAG_ABSTRACT == 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    // Property validation already checks hook types. Eval accessors have no explicit return
    // hint, unlike the generated AOT hook signature; do not compare them as ordinary methods.
    //
    // php also exempts a constructor from signature compatibility unless the parent's constructor
    // is abstract: Symfony's generated `App_KernelProdContainer::__construct()` takes no
    // arguments where `Container::__construct(?ParameterBagInterface $parameterBag = null)` takes
    // one, and refusing it killed every `--web` worker of symfony/demo at its first request.
    if (flags & EVAL_REFLECTION_METHOD_FLAG_PROPERTY_HOOK != 0
        && class.properties().iter().any(|property| property.matches_hook_method(method.name())))
        || (method.name().eq_ignore_ascii_case("__construct")
            && flags & EVAL_REFLECTION_MEMBER_FLAG_ABSTRACT == 0)
    {
        return Ok(());
    }
    let Some(required) = eval_aot_method_signature_requirement(
        &parent,
        method.name(),
        parent_is_static,
        context,
        values,
    )? else {
        return Ok(());
    };
    // `#[\ReturnTypeWillChange]` is php's own waiver for a return type that differs from an
    // internal parent's TENTATIVE one (`DateTime::createFromFormat(): DateTime|false`, which
    // Carbon overrides as `?static`). Without it php only deprecates the mismatch; with it php
    // says nothing. The parameters are still checked.
    let required = if eval_attributes_have_global_builtin_attribute(
        method.attributes(),
        "ReturnTypeWillChange",
    ) {
        required.with_return_type(None)
    } else {
        required
    };
    let declaring_class = eval_aot_method_declaring_class(&parent, method.name(), values)?;
    let accepts = |context: &ElephcEvalContext| {
        class_method_satisfies_interface_signature(
            method,
            class.name(),
            &required,
            &declaring_class,
            Some(class),
            context,
        )
    };
    // An INTERNAL class's return types are tentative to php: an override whose return type
    // differs gets a deprecation, not a fatal (a missing or different `#[\ReturnTypeWillChange]`
    // only decides whether the deprecation is shown). The compiled builtins also carry methods
    // php's class does not have -- elephc's `DatePeriod` is itself an `Iterator` -- so Carbon's
    // `CarbonPeriod::key(): ?int` met a `key(): int` php never declares.
    let tentative_return = elephc_builtin_contract::lookup_class(&declaring_class).is_some();
    let accepts_with_tentative_return = |context: &ElephcEvalContext| {
        tentative_return
            && class_method_satisfies_interface_signature(
                method,
                class.name(),
                &required.clone().with_return_type(None),
                &declaring_class,
                Some(class),
                context,
            )
    };
    if !accepts(context)
        && (!autoload_override_signature_classes(
            method,
            required.parameter_types(),
            class,
            context,
            values,
        )? || !accepts(context))
        && !accepts_with_tentative_return(context)
    {
        if crate::eval_trace::enabled()
            && std::env::var_os("ELEPHC_EVAL_TRACE_SIGNATURES").is_some()
        {
            eprintln!(
                "[elephc-eval-trace] phase=aot_parent_override_mismatch class={:?} method={:?} declaring_class={declaring_class:?}\n[elephc-eval-trace] required_params={:?} types={:?} defaults={:?} return={:?}\n[elephc-eval-trace] actual_params={:?} types={:?} defaults={:?} return={:?}",
                class.name(),
                method.name(),
                required.params(),
                required.parameter_types(),
                required.parameter_defaults(),
                required.return_type(),
                method.params(),
                method.parameter_types(),
                method.parameter_defaults(),
                method.return_type(),
            );
        }
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(())
}

/// Returns the eval visibility represented by generated/AOT reflection flags.
pub(super) fn eval_aot_method_visibility(flags: u64) -> EvalVisibility {
    if flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE != 0 {
        EvalVisibility::Private
    } else if flags & EVAL_REFLECTION_MEMBER_FLAG_PROTECTED != 0 {
        EvalVisibility::Protected
    } else if flags & EVAL_REFLECTION_MEMBER_FLAG_PUBLIC != 0 {
        EvalVisibility::Public
    } else {
        EvalVisibility::Public
    }
}

/// Returns the generated/AOT declaring class for one reflected method.
pub(super) fn eval_aot_method_declaring_class(
    class_name: &str,
    method_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    values
        .reflection_method_declaring_class(class_name, method_name)
        .map(|declaring_class| declaring_class.unwrap_or_else(|| class_name.to_string()))
}

/// Returns a generated/AOT parent method signature as an eval method requirement.
pub(super) fn eval_aot_method_signature_requirement(
    class_name: &str,
    method_name: &str,
    is_static: bool,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalInterfaceMethod>, EvalStatus> {
    let declaring_class = eval_aot_method_declaring_class(class_name, method_name, values)?;
    let signature = if is_static {
        context.native_static_method_signature(&declaring_class, method_name)
    } else {
        context.native_method_signature(&declaring_class, method_name)
    };
    Ok(signature.map(|signature| {
        eval_native_signature_interface_method(method_name, is_static, &signature)
    }))
}

/// Returns whether an override may stand in for a declaration that returns BY REFERENCE.
///
/// php's rule is one-directional: an override may ADD `&` where the declaration it replaces does
/// not ask for it, but may never DROP one it does. `php -n` 8.5.6 refuses that with
/// `Declaration of C::f() must be compatible with & P::f()` -- note the `&` in front of the
/// PARENT signature, which is how the message says which side wanted it.
///
/// It lives here, called from both the interface path and the parent-class path, because it was
/// written once for interfaces and the class path silently had no rule at all: a subclass could
/// replace a by-reference parent method with a by-value one and nothing said so.
pub(super) fn override_by_ref_return_is_compatible(
    override_returns_by_ref: bool,
    required_returns_by_ref: bool,
) -> bool {
    override_returns_by_ref || !required_returns_by_ref
}

/// Returns whether one eval class method can accept every call accepted by its parent method.
pub(super) fn class_method_signature_accepts(
    method: &EvalClassMethod,
    method_owner: &str,
    required: &EvalClassMethod,
    required_owner: &str,
    pending_class: Option<&EvalClass>,
    context: &ElephcEvalContext,
) -> bool {
    override_by_ref_return_is_compatible(method.returns_by_ref(), required.returns_by_ref())
        && method_signature_accepts(
        method.params().len(),
        method.parameter_defaults(),
        method.parameter_is_by_ref(),
        method.parameter_is_variadic(),
        required.params().len(),
        required.parameter_defaults(),
        required.parameter_is_by_ref(),
        required.parameter_is_variadic(),
    ) && method_parameter_type_signature_accepts(
        method.parameter_types(),
        method.parameter_is_variadic(),
        method_owner,
        required.parameter_types(),
        required.parameter_is_variadic(),
        required_owner,
        required.params().len(),
        pending_class,
        context,
    ) && method_return_type_signature_accepts(
        method.return_type(),
        method_owner,
        required.return_type(),
        required_owner,
        pending_class,
        context,
    )
}

/// Returns a comparable rank where larger means less restrictive visibility.
pub(super) fn method_visibility_rank(visibility: EvalVisibility) -> u8 {
    match visibility {
        EvalVisibility::Private => 1,
        EvalVisibility::Protected => 2,
        EvalVisibility::Public => 3,
    }
}
