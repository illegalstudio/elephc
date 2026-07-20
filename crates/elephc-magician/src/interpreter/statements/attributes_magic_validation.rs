//! Purpose:
//! Validates class-like attributes, modifiers, override markers, and magic methods.
//!
//! Called from:
//! - Eval class, interface, trait, and enum declaration validation.
//!
//! Key details:
//! - AOT member flags and magic signature contracts are shared with later validators.

use super::*;

/// Bridge reflection flag for static generated/AOT members.
pub(super) const EVAL_REFLECTION_MEMBER_FLAG_STATIC: u64 = 1;

/// Bridge reflection flag for public generated/AOT members.
pub(super) const EVAL_REFLECTION_MEMBER_FLAG_PUBLIC: u64 = 2;

/// Bridge reflection flag for protected generated/AOT members.
pub(super) const EVAL_REFLECTION_MEMBER_FLAG_PROTECTED: u64 = 4;

/// Bridge reflection flag for private generated/AOT members.
pub(super) const EVAL_REFLECTION_MEMBER_FLAG_PRIVATE: u64 = 8;

/// Bridge reflection flag for final generated/AOT members.
pub(super) const EVAL_REFLECTION_MEMBER_FLAG_FINAL: u64 = 16;

/// Bridge reflection flag for abstract generated/AOT members.
pub(super) const EVAL_REFLECTION_MEMBER_FLAG_ABSTRACT: u64 = 32;

/// Bridge reflection flag for readonly generated/AOT properties.
pub(super) const EVAL_REFLECTION_MEMBER_FLAG_READONLY: u64 = 64;

/// Bridge reflection flag for protected-set generated/AOT properties.
pub(super) const EVAL_REFLECTION_MEMBER_FLAG_PROTECTED_SET: u64 = 2048;

/// Bridge reflection flag for private-set generated/AOT properties.
pub(super) const EVAL_REFLECTION_MEMBER_FLAG_PRIVATE_SET: u64 = 4096;

/// Method requirement discovered from generated/AOT interface metadata.
pub(super) struct EvalAotInterfaceMethodRequirement {
    pub(super) owner: String,
    pub(super) name: String,
    pub(super) is_static: bool,
    pub(super) signature: Option<EvalInterfaceMethod>,
}

/// Abstract method requirement discovered from generated/AOT parent metadata.
pub(super) struct EvalAotAbstractMethodRequirement {
    pub(super) owner: String,
    pub(super) is_static: bool,
    pub(super) visibility: EvalVisibility,
    pub(super) signature: Option<EvalInterfaceMethod>,
}

/// Abstract property requirement discovered from generated/AOT parent metadata.
pub(super) struct EvalAotAbstractPropertyRequirement {
    pub(super) owner: String,
    pub(super) property: EvalClassProperty,
}

/// Rejects builtin attributes that cannot target an eval-declared class.
pub(super) fn validate_eval_class_attribute_targets(
    attributes: &[EvalAttribute],
) -> Result<(), EvalStatus> {
    if eval_attributes_have_global_builtin_attribute(attributes, "Override") {
        Err(EvalStatus::RuntimeFatal)
    } else {
        Ok(())
    }
}

/// Rejects builtin attributes that cannot target eval-declared interfaces.
pub(super) fn validate_eval_interface_attribute_targets(
    interface: &EvalInterface,
) -> Result<(), EvalStatus> {
    validate_eval_non_class_attribute_targets(interface.attributes())?;
    for property in interface.properties() {
        validate_eval_non_method_attribute_targets(property.attributes())?;
    }
    for method in interface.methods() {
        validate_eval_method_attribute_targets(method.attributes())?;
    }
    Ok(())
}

/// Validates PHP's global `#[Override]` marker on eval-declared interface methods.
pub(super) fn validate_eval_interface_override_attributes(
    interface: &EvalInterface,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let parent_requirements = eval_interface_parent_method_requirements(interface, context);
    for method in interface.methods() {
        if !eval_interface_method_has_global_builtin_attribute(method, "Override") {
            continue;
        }
        if parent_requirements
            .iter()
            .any(|(_, requirement)| eval_interface_method_matches_requirement(method, requirement))
        {
            continue;
        }
        if eval_aot_interface_parent_method_matches(interface, method, values)? {
            continue;
        }
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(())
}

/// Returns method requirements inherited by one eval interface declaration.
pub(super) fn eval_interface_parent_method_requirements(
    interface: &EvalInterface,
    context: &ElephcEvalContext,
) -> Vec<(String, EvalInterfaceMethod)> {
    let mut requirements = Vec::new();
    for parent in interface.parents() {
        if context.has_interface(parent) {
            requirements.extend(context.interface_method_requirements_with_owners(parent));
        }
        requirements.extend(builtin_interface_method_requirements(parent));
    }
    requirements
}

/// Returns whether a generated/AOT parent interface exposes a matching method.
pub(super) fn eval_aot_interface_parent_method_matches(
    interface: &EvalInterface,
    method: &EvalInterfaceMethod,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    for parent in interface.parents() {
        if !values.interface_exists(parent)? {
            continue;
        }
        let parent = parent.trim_start_matches('\\');
        if let Some(flags) = values.reflection_method_flags(parent, method.name())? {
            let parent_method_is_static = flags & EVAL_REFLECTION_MEMBER_FLAG_STATIC != 0;
            return Ok(parent_method_is_static == method.is_static());
        }
    }
    Ok(false)
}

/// Returns whether an interface method matches one inherited requirement signature kind.
pub(super) fn eval_interface_method_matches_requirement(
    method: &EvalInterfaceMethod,
    requirement: &EvalInterfaceMethod,
) -> bool {
    requirement.name().eq_ignore_ascii_case(method.name())
        && requirement.is_static() == method.is_static()
}

/// Rejects builtin attributes that cannot target eval-declared traits.
pub(super) fn validate_eval_trait_attribute_targets(trait_decl: &EvalTrait) -> Result<(), EvalStatus> {
    validate_eval_non_class_attribute_targets(trait_decl.attributes())?;
    for property in trait_decl.properties() {
        validate_eval_non_method_attribute_targets(property.attributes())?;
    }
    for method in trait_decl.methods() {
        validate_eval_method_attribute_targets(method.attributes())?;
    }
    Ok(())
}

/// Rejects builtin attributes that cannot target eval-declared enums.
pub(super) fn validate_eval_enum_attribute_targets(enum_decl: &EvalEnum) -> Result<(), EvalStatus> {
    validate_eval_non_class_attribute_targets(enum_decl.attributes())
}

/// Rejects class-only or method-only builtin attributes on non-class declarations.
pub(super) fn validate_eval_non_class_attribute_targets(
    attributes: &[EvalAttribute],
) -> Result<(), EvalStatus> {
    if eval_attributes_have_global_builtin_attribute(attributes, "AllowDynamicProperties")
        || eval_attributes_have_global_builtin_attribute(attributes, "Override")
    {
        Err(EvalStatus::RuntimeFatal)
    } else {
        Ok(())
    }
}

/// Rejects class-only or method-only builtin attributes on non-method members.
pub(super) fn validate_eval_non_method_attribute_targets(
    attributes: &[EvalAttribute],
) -> Result<(), EvalStatus> {
    if eval_attributes_have_global_builtin_attribute(attributes, "AllowDynamicProperties")
        || eval_attributes_have_global_builtin_attribute(attributes, "Override")
    {
        Err(EvalStatus::RuntimeFatal)
    } else {
        Ok(())
    }
}

/// Rejects class-only builtin attributes on method declarations.
pub(super) fn validate_eval_method_attribute_targets(
    attributes: &[EvalAttribute],
) -> Result<(), EvalStatus> {
    if eval_attributes_have_global_builtin_attribute(attributes, "AllowDynamicProperties") {
        Err(EvalStatus::RuntimeFatal)
    } else {
        Ok(())
    }
}

/// Returns whether the attribute list contains one global builtin attribute.
pub(super) fn eval_attributes_have_global_builtin_attribute(
    attributes: &[EvalAttribute],
    builtin: &str,
) -> bool {
    attributes
        .iter()
        .any(|attribute| eval_attribute_is_global_builtin(attribute, builtin))
}

/// Returns whether one attribute names a global builtin attribute class.
pub(super) fn eval_attribute_is_global_builtin(attribute: &EvalAttribute, builtin: &str) -> bool {
    attribute
        .name()
        .trim_start_matches('\\')
        .eq_ignore_ascii_case(builtin)
}

/// Validates PHP's global `#[Override]` marker on one eval-declared method.
pub(super) fn validate_eval_override_attribute(
    class: &EvalClass,
    method: &EvalClassMethod,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !eval_method_has_global_builtin_attribute(method, "Override") {
        return Ok(());
    }
    if eval_method_overrides_parent(class, method, context)
        || eval_method_overrides_aot_parent(class, method, context, values)?
        || eval_method_implements_interface(class, method, context, values)?
    {
        Ok(())
    } else {
        Err(EvalStatus::RuntimeFatal)
    }
}

/// Returns whether a method has a global builtin marker attribute.
pub(super) fn eval_method_has_global_builtin_attribute(method: &EvalClassMethod, builtin: &str) -> bool {
    eval_attributes_have_global_builtin_attribute(method.attributes(), builtin)
}

/// Returns whether an interface method has a global builtin marker attribute.
pub(super) fn eval_interface_method_has_global_builtin_attribute(
    method: &EvalInterfaceMethod,
    builtin: &str,
) -> bool {
    eval_attributes_have_global_builtin_attribute(method.attributes(), builtin)
}

/// Returns whether one method overrides a non-private parent method.
pub(super) fn eval_method_overrides_parent(
    class: &EvalClass,
    method: &EvalClassMethod,
    context: &ElephcEvalContext,
) -> bool {
    class
        .parent()
        .and_then(|parent| context.class_method(parent, method.name()))
        .is_some_and(|(_, parent_method)| {
            parent_method.visibility() != EvalVisibility::Private
                && parent_method.is_static() == method.is_static()
        })
}

/// Returns whether one method overrides a visible generated/AOT parent method.
pub(super) fn eval_method_overrides_aot_parent(
    class: &EvalClass,
    method: &EvalClassMethod,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Some(parent) = pending_class_native_parent_name(class, context) else {
        return Ok(false);
    };
    if !values.class_exists(&parent)? {
        return Ok(false);
    }
    let Some(flags) = values.reflection_method_flags(&parent, method.name())? else {
        return Ok(false);
    };
    let parent_method_is_static = flags & EVAL_REFLECTION_MEMBER_FLAG_STATIC != 0;
    let parent_method_is_private = flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE != 0;
    Ok(!parent_method_is_private && parent_method_is_static == method.is_static())
}

/// Returns the nearest generated/AOT parent for a class not yet registered in context.
pub(super) fn pending_class_native_parent_name(
    class: &EvalClass,
    context: &ElephcEvalContext,
) -> Option<String> {
    let mut current = class.parent()?.to_string();
    let mut seen = std::collections::HashSet::new();
    loop {
        let resolved = context
            .resolve_class_name(&current)
            .unwrap_or_else(|| current.trim_start_matches('\\').to_string());
        if !seen.insert(resolved.to_ascii_lowercase()) {
            return None;
        }
        let Some(parent_class) = context.class(&resolved) else {
            return Some(resolved.trim_start_matches('\\').to_string());
        };
        current = parent_class.parent()?.to_string();
    }
}

/// Returns whether one method implements a direct or inherited interface method.
pub(super) fn eval_method_implements_interface(
    class: &EvalClass,
    method: &EvalClassMethod,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if pending_class_interface_names(class, context)
        .iter()
        .filter(|interface| context.has_interface(interface))
        .any(|interface| {
            context
                .interface_method_requirements_with_owners(interface)
                .into_iter()
                .any(|(_, requirement)| {
                    requirement.name().eq_ignore_ascii_case(method.name())
                        && requirement.is_static() == method.is_static()
                })
        })
    {
        return Ok(true);
    }
    Ok(pending_class_aot_interface_method_requirements(class, context, values)?
        .iter()
        .any(|requirement| {
            requirement.name.eq_ignore_ascii_case(method.name())
                && requirement.is_static == method.is_static()
        }))
}

/// Validates PHP magic-method contracts for one eval class-like method list.
pub(super) fn validate_eval_magic_methods(methods: &[EvalClassMethod]) -> Result<(), EvalStatus> {
    for method in methods {
        validate_eval_magic_method(method)?;
    }
    Ok(())
}

/// Validates staticness, visibility, arity, and declared return type for one eval magic method.
pub(super) fn validate_eval_magic_method(method: &EvalClassMethod) -> Result<(), EvalStatus> {
    let name = method.name().to_ascii_lowercase();
    if validated_eval_magic_method_rejects_by_ref_params(&name) {
        validate_magic_no_by_ref_params(method)?;
    }
    match name.as_str() {
        "__tostring" => {
            validate_magic_non_static(method)?;
            validate_magic_public(method)?;
            validate_magic_arity(method, 0)?;
            validate_magic_declared_return_type(method, MagicReturnType::String)?;
        }
        "__get" | "__isset" | "__unset" => {
            validate_magic_non_static(method)?;
            validate_magic_arity(method, 1)?;
            validate_magic_declared_param_type(method, 0, MagicParamType::String)?;
            if method.name().eq_ignore_ascii_case("__isset") {
                validate_magic_declared_return_type(method, MagicReturnType::Bool)?;
            } else if method.name().eq_ignore_ascii_case("__unset") {
                validate_magic_declared_return_type(method, MagicReturnType::Void)?;
            }
        }
        "__set" | "__call" => {
            validate_magic_non_static(method)?;
            validate_magic_arity(method, 2)?;
            validate_magic_declared_param_type(method, 0, MagicParamType::String)?;
            if method.name().eq_ignore_ascii_case("__set") {
                validate_magic_declared_return_type(method, MagicReturnType::Void)?;
            } else {
                validate_magic_declared_param_type(method, 1, MagicParamType::Array)?;
            }
        }
        "__callstatic" => {
            validate_magic_static(method)?;
            validate_magic_arity(method, 2)?;
            validate_magic_declared_param_type(method, 0, MagicParamType::String)?;
            validate_magic_declared_param_type(method, 1, MagicParamType::Array)?;
        }
        "__sleep" | "__serialize" => {
            validate_magic_non_static(method)?;
            validate_magic_arity(method, 0)?;
            validate_magic_declared_return_type(method, MagicReturnType::Array)?;
        }
        "__wakeup" => {
            validate_magic_non_static(method)?;
            validate_magic_arity(method, 0)?;
            validate_magic_declared_return_type(method, MagicReturnType::Void)?;
        }
        "__unserialize" => {
            validate_magic_non_static(method)?;
            validate_magic_arity(method, 1)?;
            validate_magic_declared_param_type(method, 0, MagicParamType::Array)?;
            validate_magic_declared_return_type(method, MagicReturnType::Void)?;
        }
        "__debuginfo" => {
            validate_magic_non_static(method)?;
            validate_magic_arity(method, 0)?;
            validate_magic_declared_return_type(method, MagicReturnType::NullableArray)?;
        }
        "__set_state" => {
            validate_magic_static(method)?;
            validate_magic_arity(method, 1)?;
            validate_magic_declared_param_type(method, 0, MagicParamType::Array)?;
        }
        "__invoke" => {
            validate_magic_non_static(method)?;
        }
        "__clone" | "__destruct" => {
            validate_magic_non_static(method)?;
            validate_magic_arity(method, 0)?;
            if method.name().eq_ignore_ascii_case("__clone") {
                validate_magic_declared_return_type(method, MagicReturnType::Void)?;
            } else {
                validate_magic_no_declared_return_type(method)?;
            }
        }
        "__construct" => {
            if method.is_static() {
                return Err(EvalStatus::RuntimeFatal);
            }
            validate_magic_no_declared_return_type(method)?;
        }
        _ => {}
    }
    Ok(())
}

/// Returns whether PHP rejects by-reference parameters for this magic method.
pub(super) fn validated_eval_magic_method_rejects_by_ref_params(name: &str) -> bool {
    is_validated_eval_magic_method(name) && !matches!(name, "__construct" | "__invoke")
}

/// Returns whether eval knows PHP declaration-time rules for this magic method.
pub(super) fn is_validated_eval_magic_method(name: &str) -> bool {
    matches!(
        name,
        "__tostring"
            | "__get"
            | "__isset"
            | "__unset"
            | "__set"
            | "__call"
            | "__callstatic"
            | "__sleep"
            | "__serialize"
            | "__wakeup"
            | "__unserialize"
            | "__debuginfo"
            | "__set_state"
            | "__invoke"
            | "__clone"
            | "__destruct"
            | "__construct"
    )
}

/// Magic method return types that eval can validate from retained declarations.
#[derive(Clone, Copy)]
pub(super) enum MagicReturnType {
    Array,
    Bool,
    NullableArray,
    String,
    Void,
}

/// Magic method parameter types that eval can validate from retained declarations.
#[derive(Clone, Copy)]
pub(super) enum MagicParamType {
    Array,
    String,
}

/// Rejects static declarations for magic methods that must be instance methods.
pub(super) fn validate_magic_non_static(method: &EvalClassMethod) -> Result<(), EvalStatus> {
    if method.is_static() {
        Err(EvalStatus::RuntimeFatal)
    } else {
        Ok(())
    }
}

/// Rejects instance declarations for magic methods that must be static methods.
pub(super) fn validate_magic_static(method: &EvalClassMethod) -> Result<(), EvalStatus> {
    if method.is_static() {
        Ok(())
    } else {
        Err(EvalStatus::RuntimeFatal)
    }
}

/// Rejects non-public declarations for public-only PHP magic methods.
pub(super) fn validate_magic_public(method: &EvalClassMethod) -> Result<(), EvalStatus> {
    if method.visibility() == EvalVisibility::Public {
        Ok(())
    } else {
        Err(EvalStatus::RuntimeFatal)
    }
}

/// Rejects magic methods whose arity differs from PHP's required shape.
pub(super) fn validate_magic_arity(method: &EvalClassMethod, expected: usize) -> Result<(), EvalStatus> {
    let has_variadic = method
        .parameter_is_variadic()
        .iter()
        .any(|is_variadic| *is_variadic);
    if method.params().len() == expected && !has_variadic {
        Ok(())
    } else {
        Err(EvalStatus::RuntimeFatal)
    }
}

/// Rejects by-reference parameters on PHP magic methods.
pub(super) fn validate_magic_no_by_ref_params(method: &EvalClassMethod) -> Result<(), EvalStatus> {
    if method
        .parameter_is_by_ref()
        .iter()
        .any(|is_by_ref| *is_by_ref)
    {
        Err(EvalStatus::RuntimeFatal)
    } else {
        Ok(())
    }
}

/// Rejects incompatible explicit parameter types on PHP magic methods.
pub(super) fn validate_magic_declared_param_type(
    method: &EvalClassMethod,
    position: usize,
    expected: MagicParamType,
) -> Result<(), EvalStatus> {
    let Some(Some(parameter_type)) = method.parameter_types().get(position) else {
        return Ok(());
    };
    if magic_param_type_matches(parameter_type, expected) {
        Ok(())
    } else {
        Err(EvalStatus::RuntimeFatal)
    }
}

/// Returns whether one retained eval parameter type is exactly the expected magic atom.
pub(super) fn magic_param_type_matches(
    parameter_type: &EvalParameterType,
    expected: MagicParamType,
) -> bool {
    if parameter_type.allows_null() || parameter_type.is_intersection() {
        return false;
    }
    let [variant] = parameter_type.variants() else {
        return false;
    };
    matches!(
        (expected, variant),
        (MagicParamType::Array, EvalParameterTypeVariant::Array)
            | (MagicParamType::String, EvalParameterTypeVariant::String)
    )
}

/// Rejects PHP magic methods that cannot declare any return type.
pub(super) fn validate_magic_no_declared_return_type(method: &EvalClassMethod) -> Result<(), EvalStatus> {
    if method.return_type().is_some() {
        Err(EvalStatus::RuntimeFatal)
    } else {
        Ok(())
    }
}

/// Rejects incompatible explicit return types on PHP magic methods.
pub(super) fn validate_magic_declared_return_type(
    method: &EvalClassMethod,
    expected: MagicReturnType,
) -> Result<(), EvalStatus> {
    method.return_type().map_or(Ok(()), |return_type| {
        if magic_return_type_matches(return_type, expected) {
            Ok(())
        } else {
            Err(EvalStatus::RuntimeFatal)
        }
    })
}

/// Returns whether one retained eval return type is exactly the expected magic return atom.
pub(super) fn magic_return_type_matches(
    return_type: &EvalParameterType,
    expected: MagicReturnType,
) -> bool {
    if return_type.is_intersection() {
        return false;
    }
    if return_type.allows_null() && !matches!(expected, MagicReturnType::NullableArray) {
        return false;
    }
    let [variant] = return_type.variants() else {
        return false;
    };
    matches!(
        (expected, variant),
        (MagicReturnType::Array, EvalParameterTypeVariant::Array)
            | (MagicReturnType::Bool, EvalParameterTypeVariant::Bool)
            | (MagicReturnType::NullableArray, EvalParameterTypeVariant::Array)
            | (MagicReturnType::String, EvalParameterTypeVariant::String)
            | (MagicReturnType::Void, EvalParameterTypeVariant::Void)
    )
}
