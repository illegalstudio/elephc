//! Purpose:
//! Resolves interface hierarchies and checks method/property signature compatibility.
//!
//! Called from:
//! - Class declaration and abstract-requirement validation.
//!
//! Key details:
//! - Arity, by-reference flags, variadics, return types, and hook capabilities remain PHP-compatible.

use super::*;

/// Returns interface names inherited or directly declared by a pending eval class.
pub(super) fn pending_class_interface_names(class: &EvalClass, context: &ElephcEvalContext) -> Vec<String> {
    let mut interfaces = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(parent) = class.parent() {
        for interface in context.class_interface_names(parent) {
            push_pending_class_interface_name(&interface, &mut interfaces, &mut seen);
        }
    }
    for interface in class.interfaces() {
        push_pending_class_interface_tree(interface, context, &mut interfaces, &mut seen);
    }
    interfaces
}

/// Adds one interface and its eval-declared parent interfaces to a pending class list.
pub(super) fn push_pending_class_interface_tree(
    interface: &str,
    context: &ElephcEvalContext,
    interfaces: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
) {
    push_pending_class_interface_name(interface, interfaces, seen);
    for parent in context.interface_parent_names(interface) {
        push_pending_class_interface_name(&parent, interfaces, seen);
    }
}

/// Adds one interface name once using PHP class-name case-insensitive matching.
pub(super) fn push_pending_class_interface_name(
    interface: &str,
    interfaces: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
) {
    let interface = interface.trim_start_matches('\\');
    if seen.insert(interface.to_ascii_lowercase()) {
        interfaces.push(interface.to_string());
    }
}

/// Returns PHP builtin interface method requirements inherited by a pending class.
pub(super) fn pending_class_builtin_interface_method_requirements(
    class: &EvalClass,
    context: &ElephcEvalContext,
) -> Vec<(String, EvalInterfaceMethod)> {
    let mut requirements = Vec::new();
    for interface in pending_class_interface_names(class, context) {
        requirements.extend(builtin_interface_method_requirements(&interface));
    }
    requirements
}

/// Returns generated/AOT interface method requirements inherited by a pending class.
pub(super) fn pending_class_aot_interface_method_requirements(
    class: &EvalClass,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvalAotInterfaceMethodRequirement>, EvalStatus> {
    let mut requirements = Vec::new();
    for interface in pending_class_interface_names(class, context) {
        if context.has_interface(&interface) || !values.interface_exists(&interface)? {
            continue;
        }
        requirements.extend(eval_aot_interface_method_requirements(
            &interface, context, values,
        )?);
    }
    Ok(requirements)
}

/// Returns generated/AOT interface property requirements inherited by a pending class.
pub(super) fn pending_class_aot_interface_property_requirements(
    class: &EvalClass,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<(String, EvalInterfaceProperty)>, EvalStatus> {
    let mut requirements = Vec::new();
    for interface in pending_class_interface_names(class, context) {
        if context.has_interface(&interface) || !values.interface_exists(&interface)? {
            continue;
        }
        requirements.extend(context.native_interface_property_requirements(&interface));
    }
    Ok(requirements)
}

/// Returns generated/AOT method requirements for one runtime interface.
pub(super) fn eval_aot_interface_method_requirements(
    interface: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvalAotInterfaceMethodRequirement>, EvalStatus> {
    let interface = interface.trim_start_matches('\\');
    let method_names = eval_aot_interface_method_names(interface, values)?;
    let mut requirements = Vec::new();
    for method_name in method_names {
        if let Some(requirement) =
            eval_aot_interface_method_requirement(interface, &method_name, context, values)?
        {
            requirements.push(requirement);
        }
    }
    Ok(requirements)
}

/// Returns generated/AOT method names for one runtime interface.
pub(super) fn eval_aot_interface_method_names(
    interface: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    eval_aot_method_names(interface, values)
}

/// Returns generated/AOT method names for one runtime class-like symbol.
pub(super) fn eval_aot_method_names(
    class_like: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    let method_names = values.reflection_method_names(class_like)?;
    let names = eval_runtime_string_array_to_vec(method_names, values)?;
    values.release(method_names)?;
    Ok(names)
}

/// Builds one generated/AOT abstract parent method requirement from metadata.
pub(super) fn eval_aot_abstract_method_requirement(
    class_name: &str,
    method_name: &str,
    flags: u64,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalAotAbstractMethodRequirement, EvalStatus> {
    let is_static = flags & EVAL_REFLECTION_MEMBER_FLAG_STATIC != 0;
    let owner = eval_aot_method_declaring_class(class_name, method_name, values)?;
    let signature = eval_aot_method_signature_requirement(
        class_name,
        method_name,
        is_static,
        context,
        values,
    )?;
    Ok(EvalAotAbstractMethodRequirement {
        owner,
        is_static,
        visibility: eval_aot_method_visibility(flags),
        signature,
    })
}

/// Builds one generated/AOT interface method requirement from reflection and signature metadata.
pub(super) fn eval_aot_interface_method_requirement(
    interface: &str,
    method_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalAotInterfaceMethodRequirement>, EvalStatus> {
    let Some(flags) = values.reflection_method_flags(interface, method_name)? else {
        return Ok(None);
    };
    let is_static = flags & EVAL_REFLECTION_MEMBER_FLAG_STATIC != 0;
    let owner = values
        .reflection_method_declaring_class(interface, method_name)?
        .unwrap_or_else(|| interface.to_string());
    let signature = if is_static {
        context.native_static_method_signature(&owner, method_name)
    } else {
        context.native_method_signature(&owner, method_name)
    };
    Ok(Some(EvalAotInterfaceMethodRequirement {
        owner: owner.clone(),
        name: method_name.to_string(),
        is_static,
        signature: signature.map(|signature| {
            eval_native_signature_interface_method(method_name, is_static, &signature)
        }),
    }))
}

/// Converts generated/AOT callable metadata into an eval interface method requirement.
pub(super) fn eval_native_signature_interface_method(
    method_name: &str,
    is_static: bool,
    signature: &NativeCallableSignature,
) -> EvalInterfaceMethod {
    let param_count = signature.param_count();
    EvalInterfaceMethod::new(
        method_name,
        (0..param_count)
            .map(|index| {
                signature
                    .param_names()
                    .get(index)
                    .filter(|name| !name.is_empty())
                    .cloned()
                    .unwrap_or_else(|| format!("arg{index}"))
            })
            .collect(),
    )
    .with_static(is_static)
    .with_parameter_types(
        (0..param_count)
            .map(|index| signature.param_type(index).cloned())
            .collect(),
    )
    .with_parameter_defaults(
        (0..param_count)
            .map(|index| {
                signature
                    .param_default(index)
                    .map(|_| EvalExpr::Const(EvalConst::Null))
            })
            .collect(),
    )
    .with_parameter_by_ref_flags(
        (0..param_count)
            .map(|index| signature.param_by_ref(index))
            .collect(),
    )
    .with_parameter_variadic_flags(
        (0..param_count)
            .map(|index| signature.param_variadic(index))
            .collect(),
    )
    .with_return_type(signature.return_type().cloned())
}

/// Copies a runtime string array into Rust-owned strings for declaration validation.
pub(super) fn eval_runtime_string_array_to_vec(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = Vec::with_capacity(len);
    for position in 0..len {
        let key = values.int(position as i64)?;
        let value = values.array_get(array, key)?;
        result.push(eval_runtime_string_value(value, values)?);
    }
    Ok(result)
}

/// Reads one runtime string cell as UTF-8 metadata.
pub(super) fn eval_runtime_string_value(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Validates that one eval class provides methods required by one eval interface.
pub(super) fn validate_class_implements_eval_interface(
    class: &EvalClass,
    interface_name: &str,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    for (requirement_owner, requirement) in
        context.interface_method_requirements_with_owners(interface_name)
    {
        if !class_has_interface_method(class, &requirement_owner, &requirement, context) {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    for (requirement_owner, requirement) in
        context.interface_property_requirements_with_owners(interface_name)
    {
        if !class_has_interface_property(class, &requirement_owner, &requirement, context) {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    Ok(())
}

/// Returns whether a class or its eval parents satisfy one builtin interface method signature.
pub(super) fn class_has_builtin_interface_method(
    class: &EvalClass,
    requirement_owner: &str,
    requirement: &EvalInterfaceMethod,
    context: &ElephcEvalContext,
) -> bool {
    if let Some(method) = class.method(requirement.name()) {
        return method.visibility() == EvalVisibility::Public
            && method.is_static() == requirement.is_static()
            && !method.is_abstract()
            && class_method_satisfies_builtin_interface_signature(
                method,
                class.name(),
                requirement,
                requirement_owner,
                Some(class),
                context,
            );
    }
    class
        .parent()
        .and_then(|parent| context.class_method(parent, requirement.name()))
        .is_some_and(|(declaring_class, method)| {
            method.visibility() == EvalVisibility::Public
                && method.is_static() == requirement.is_static()
                && !method.is_abstract()
                && class_method_satisfies_builtin_interface_signature(
                    &method,
                    &declaring_class,
                    requirement,
                    requirement_owner,
                    Some(class),
                    context,
                )
        })
}

/// Returns whether a class or its eval parents satisfy one generated/AOT interface method.
pub(super) fn class_has_aot_interface_method(
    class: &EvalClass,
    requirement: &EvalAotInterfaceMethodRequirement,
    context: &ElephcEvalContext,
) -> bool {
    if let Some((declaring_class, method)) = pending_class_method(class, &requirement.name, context)
    {
        return class_method_satisfies_aot_interface_requirement(
            &method,
            &declaring_class,
            requirement,
            Some(class),
            context,
            true,
        );
    }
    false
}

/// Returns whether a class or its eval parents satisfy one interface method signature.
pub(super) fn class_has_interface_method(
    class: &EvalClass,
    requirement_owner: &str,
    requirement: &EvalInterfaceMethod,
    context: &ElephcEvalContext,
) -> bool {
    if let Some(method) = class.method(requirement.name()) {
        return method.visibility() == EvalVisibility::Public
            && method.is_static() == requirement.is_static()
            && !method.is_abstract()
            && class_method_satisfies_interface_signature(
                method,
                class.name(),
                requirement,
                requirement_owner,
                Some(class),
                context,
            );
    }
    class
        .parent()
        .and_then(|parent| context.class_method(parent, requirement.name()))
        .is_some_and(|(declaring_class, method)| {
            method.visibility() == EvalVisibility::Public
                && method.is_static() == requirement.is_static()
                && !method.is_abstract()
                && class_method_satisfies_interface_signature(
                    &method,
                    &declaring_class,
                    requirement,
                    requirement_owner,
                    Some(class),
                    context,
                )
        })
}

/// Returns whether one method satisfies a generated/AOT interface requirement.
pub(super) fn class_method_satisfies_aot_interface_requirement(
    method: &EvalClassMethod,
    method_owner: &str,
    requirement: &EvalAotInterfaceMethodRequirement,
    pending_class: Option<&EvalClass>,
    context: &ElephcEvalContext,
    require_concrete: bool,
) -> bool {
    if method.visibility() != EvalVisibility::Public
        || method.is_static() != requirement.is_static
        || (require_concrete && method.is_abstract())
    {
        return false;
    }
    requirement.signature.as_ref().is_none_or(|signature| {
        class_method_satisfies_interface_signature(
            method,
            method_owner,
            signature,
            &requirement.owner,
            pending_class,
            context,
        )
    })
}

/// Returns whether one method satisfies a generated/AOT abstract parent requirement.
pub(super) fn class_method_satisfies_aot_abstract_parent_requirement(
    method: &EvalClassMethod,
    method_owner: &str,
    requirement: &EvalAotAbstractMethodRequirement,
    pending_class: Option<&EvalClass>,
    context: &ElephcEvalContext,
    require_concrete: bool,
) -> bool {
    if method.is_static() != requirement.is_static
        || method_visibility_rank(method.visibility())
            < method_visibility_rank(requirement.visibility)
        || (require_concrete && method.is_abstract())
    {
        return false;
    }
    requirement.signature.as_ref().is_none_or(|signature| {
        class_method_satisfies_interface_signature(
            method,
            method_owner,
            signature,
            &requirement.owner,
            pending_class,
            context,
        )
    })
}

/// Returns whether one class method can accept every call required by an interface method.
pub(super) fn class_method_satisfies_interface_signature(
    method: &EvalClassMethod,
    method_owner: &str,
    requirement: &EvalInterfaceMethod,
    requirement_owner: &str,
    pending_class: Option<&EvalClass>,
    context: &ElephcEvalContext,
) -> bool {
    class_method_satisfies_interface_signature_with_return_mode(
        method,
        method_owner,
        requirement,
        requirement_owner,
        pending_class,
        context,
        false,
    )
}

/// Returns whether one class method can satisfy a PHP builtin interface method contract.
pub(super) fn class_method_satisfies_builtin_interface_signature(
    method: &EvalClassMethod,
    method_owner: &str,
    requirement: &EvalInterfaceMethod,
    requirement_owner: &str,
    pending_class: Option<&EvalClass>,
    context: &ElephcEvalContext,
) -> bool {
    class_method_satisfies_interface_signature_with_return_mode(
        method,
        method_owner,
        requirement,
        requirement_owner,
        pending_class,
        context,
        true,
    )
}

/// Returns whether one class method satisfies an interface signature with configurable return checks.
pub(super) fn class_method_satisfies_interface_signature_with_return_mode(
    method: &EvalClassMethod,
    method_owner: &str,
    requirement: &EvalInterfaceMethod,
    requirement_owner: &str,
    pending_class: Option<&EvalClass>,
    context: &ElephcEvalContext,
    allow_missing_return_type: bool,
) -> bool {
    method_signature_accepts(
        method.params().len(),
        method.parameter_defaults(),
        method.parameter_is_by_ref(),
        method.parameter_is_variadic(),
        requirement.params().len(),
        requirement.parameter_defaults(),
        requirement.parameter_is_by_ref(),
        requirement.parameter_is_variadic(),
    ) && method_parameter_type_signature_accepts(
        method.parameter_types(),
        method.parameter_is_variadic(),
        method_owner,
        requirement.parameter_types(),
        requirement.parameter_is_variadic(),
        requirement_owner,
        requirement.params().len(),
        pending_class,
        context,
    ) && ((allow_missing_return_type && method.return_type().is_none())
        || method_return_type_signature_accepts(
            method.return_type(),
            method_owner,
            requirement.return_type(),
            requirement_owner,
            pending_class,
            context,
        ))
}

/// Returns whether one class property is compatible with one interface property contract.
pub(super) fn class_property_can_cover_interface_contract(
    property: &EvalClassProperty,
    property_owner: &str,
    requirement: &EvalInterfaceProperty,
    requirement_owner: &str,
    pending_class: Option<&EvalClass>,
    context: &ElephcEvalContext,
) -> bool {
    if property.visibility() != EvalVisibility::Public || property.is_static() {
        return false;
    }
    if !class_property_type_satisfies_interface_contract(
        property.property_type(),
        property.settable_type(),
        property_owner,
        requirement,
        requirement_owner,
        pending_class,
        context,
    ) {
        return false;
    }
    if requirement.requires_get() && !class_property_supports_interface_get(property) {
        return false;
    }
    if requirement.requires_set() {
        return requirement.set_visibility() != Some(EvalVisibility::Private)
            && property_visibility_rank(property.write_visibility())
                >= property_visibility_rank(requirement.write_visibility())
            && class_property_supports_interface_set(property);
    }
    requirement.requires_get()
}

/// Returns whether one property type is compatible with interface get/set hook signatures.
pub(super) fn class_property_type_satisfies_interface_contract(
    property_type: Option<&EvalParameterType>,
    settable_type: Option<&EvalParameterType>,
    property_owner: &str,
    requirement: &EvalInterfaceProperty,
    requirement_owner: &str,
    pending_class: Option<&EvalClass>,
    context: &ElephcEvalContext,
) -> bool {
    if requirement.requires_get()
        && !method_return_type_signature_accepts(
            property_type,
            property_owner,
            requirement.property_type(),
            requirement_owner,
            pending_class,
            context,
        )
    {
        return false;
    }
    if requirement.requires_set() {
        let property_types = vec![settable_type.cloned()];
        let requirement_types = vec![requirement.property_type().cloned()];
        return method_parameter_type_signature_accepts(
            &property_types,
            &[],
            property_owner,
            &requirement_types,
            &[],
            requirement_owner,
            1,
            pending_class,
            context,
        );
    }
    true
}

/// Returns whether one property can satisfy an interface `get` hook.
pub(super) fn class_property_supports_interface_get(property: &EvalClassProperty) -> bool {
    property.has_get_hook() || property.requires_get_hook() || !property.is_virtual()
}

/// Returns whether one property can satisfy an interface `set` hook.
pub(super) fn class_property_supports_interface_set(property: &EvalClassProperty) -> bool {
    property.has_set_hook()
        || property.requires_set_hook()
        || (!property.is_virtual() && !property.is_readonly())
}

/// Returns whether an implementing method accepts the full required arity range.
pub(super) fn method_signature_accepts(
    implementation_param_count: usize,
    implementation_defaults: &[Option<EvalExpr>],
    implementation_by_refs: &[bool],
    implementation_variadics: &[bool],
    required_param_count: usize,
    required_defaults: &[Option<EvalExpr>],
    required_by_refs: &[bool],
    required_variadics: &[bool],
) -> bool {
    let implementation_min = method_signature_min_arity(
        implementation_param_count,
        implementation_defaults,
        implementation_variadics,
    );
    let required_min =
        method_signature_min_arity(required_param_count, required_defaults, required_variadics);
    if implementation_min > required_min {
        return false;
    }

    let implementation_max =
        method_signature_max_arity(implementation_param_count, implementation_variadics);
    let required_max = method_signature_max_arity(required_param_count, required_variadics);
    let arity_accepted = match (implementation_max, required_max) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some(implementation_max), Some(required_max)) => implementation_max >= required_max,
    };
    arity_accepted
        && method_signature_by_refs_accept(
            implementation_by_refs,
            implementation_variadics,
            required_param_count,
            required_by_refs,
            required_variadics,
        )
}

/// Returns whether pass-by-reference requirements are compatible across accepted args.
pub(super) fn method_signature_by_refs_accept(
    implementation_by_refs: &[bool],
    implementation_variadics: &[bool],
    required_param_count: usize,
    required_by_refs: &[bool],
    required_variadics: &[bool],
) -> bool {
    (0..required_param_count).all(|position| {
        method_signature_effective_by_ref(
            implementation_by_refs,
            implementation_variadics,
            position,
        ) == method_signature_effective_by_ref(required_by_refs, required_variadics, position)
    })
}

/// Returns the by-reference mode that one signature applies at an argument position.
pub(super) fn method_signature_effective_by_ref(
    by_refs: &[bool],
    variadics: &[bool],
    position: usize,
) -> bool {
    if let Some(variadic_index) = variadics.iter().position(|is_variadic| *is_variadic) {
        if position >= variadic_index {
            return by_refs.get(variadic_index).copied().unwrap_or(false);
        }
    }
    by_refs.get(position).copied().unwrap_or(false)
}

/// Returns the minimum argument count accepted by one eval method signature.
pub(super) fn method_signature_min_arity(
    param_count: usize,
    defaults: &[Option<EvalExpr>],
    variadics: &[bool],
) -> usize {
    let fixed_count = variadics
        .iter()
        .position(|is_variadic| *is_variadic)
        .unwrap_or(param_count);
    (0..fixed_count)
        .rfind(|position| !defaults.get(*position).is_some_and(Option::is_some))
        .map_or(0, |position| position + 1)
}

/// Returns the maximum argument count accepted by one eval method signature.
pub(super) fn method_signature_max_arity(param_count: usize, variadics: &[bool]) -> Option<usize> {
    if variadics.iter().any(|is_variadic| *is_variadic) {
        None
    } else {
        Some(param_count)
    }
}

/// Returns whether a class or its eval parents satisfy one interface property contract.
pub(super) fn class_has_interface_property(
    class: &EvalClass,
    requirement_owner: &str,
    requirement: &EvalInterfaceProperty,
    context: &ElephcEvalContext,
) -> bool {
    pending_class_property_with_owner(class, requirement.name(), context).is_some_and(
        |(declaring_class, property)| {
            !property.is_abstract()
                && class_property_can_cover_interface_contract(
                    &property,
                    &declaring_class,
                    requirement,
                    requirement_owner,
                    Some(class),
                    context,
                )
        },
    )
}

/// Returns a property from a pending class or one of its already registered parents.
pub(super) fn pending_class_property_with_owner(
    class: &EvalClass,
    property_name: &str,
    context: &ElephcEvalContext,
) -> Option<(String, EvalClassProperty)> {
    if let Some(property) = class
        .properties()
        .iter()
        .find(|property| property.name() == property_name)
    {
        return Some((class.name().to_string(), property.clone()));
    }
    class
        .parent()
        .and_then(|parent| context.class_property(parent, property_name))
}
