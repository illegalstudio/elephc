//! Purpose:
//! ReflectionMethod member construction for classes, interfaces, and traits.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::reflection`.
//!
//! Key details:
//! - Preserves compile-time metadata, target-aware object layout, and ownership.

use super::*;

/// Builds ReflectionMethod array entries for the methods visible on one class.
pub(super) fn reflection_class_method_members(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    info: &crate::types::ClassInfo,
    method_names: &[String],
) -> Result<Vec<ReflectionListedMember>> {
    let mut members = Vec::new();
    for method_name in method_names {
        if let Some(member) = reflection_class_method_member(ctx, class_name, info, method_name)? {
            members.push(member);
        }
    }
    Ok(members)
}

/// Builds one ReflectionMethod array entry from class metadata.
pub(super) fn reflection_class_method_member(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    info: &crate::types::ClassInfo,
    method_name: &str,
) -> Result<Option<ReflectionListedMember>> {
    let method_key = php_symbol_key(method_name);
    if crate::types::php_src_date_method_visible(class_name, &method_key) == Some(false) {
        return Ok(None);
    }
    let sig = info
        .methods
        .get(&method_key)
        .or_else(|| info.static_methods.get(&method_key));
    let Some(sig) = sig else {
        return Ok(None);
    };
    let declaring_class_name =
        reflection_method_declaring_class_name(info, class_name, &method_key);
    let attr_names = info
        .method_attribute_names
        .get(&method_key)
        .cloned()
        .unwrap_or_default();
    let attr_args = info
        .method_attribute_args
        .get(&method_key)
        .cloned()
        .unwrap_or_default();
    let Some(flags) = reflection_method_member_flags(info, &method_key) else {
        return Ok(None);
    };
    let is_php_dateperiod_constructor = class_name
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("DatePeriod")
        && method_key == "__construct";
    let required_parameter_count = if is_php_dateperiod_constructor {
        1
    } else {
        reflection_required_parameter_count(sig)
    };
    let late_static_return = if flags.is_static {
        info.late_static_static_method_returns.get(&method_key)
    } else {
        info.late_static_method_returns.get(&method_key)
    };
    let mut type_metadata = reflection_method_return_type_metadata(sig, late_static_return);
    if let Some(return_type) =
        crate::types::php_src_date_method_return_type(class_name, &method_key)
    {
        type_metadata = reflection_declared_type_metadata(&return_type);
    }
    let is_generator = reflection_method_is_generator(
        ctx,
        declaring_class_name.as_deref().unwrap_or(class_name),
        &method_key,
    );
    let prototype_member =
        reflection_class_method_prototype_member(ctx, class_name, info, &method_key, flags)?;
    let declaring_function = ReflectionDeclaringFunctionMember::Method {
        name: method_key.clone(),
        declaring_class_name: declaring_class_name.clone(),
        attr_names: attr_names.clone(),
        attr_args: attr_args.clone(),
        flags,
        required_parameter_count,
        type_metadata: type_metadata.clone(),
        is_deprecated: sig.deprecation.is_some(),
        is_generator,
    };
    let source_defaults = declaring_class_name
        .as_deref()
        .and_then(|declaring_class| {
            reflection_source_method_defaults(
                ctx,
                declaring_class,
                &method_key,
                flags.is_static,
            )
        });
    let mut parameters = reflection_parameter_members_with_declaring_class(
        ctx,
        sig,
        class_name,
        Some(info),
        declaring_class_name.as_deref(),
        Some(declaring_function),
        &reflection_promoted_constructor_parameter_names(info, &method_key),
        source_defaults.as_deref(),
    )?;
    if is_php_dateperiod_constructor {
        for (index, parameter) in parameters.iter_mut().enumerate() {
            parameter.is_optional = index > 0;
            parameter.has_type = false;
            parameter.allows_null = true;
            parameter.type_metadata = None;
            parameter.default_value = None;
            parameter.default_value_constant_name = None;
        }
    }
    for (index, parameter) in parameters.iter_mut().enumerate() {
        let Some(type_expr) = crate::types::php_src_date_method_parameter_type(
            class_name,
            &method_key,
            index,
        ) else {
            continue;
        };
        parameter.has_type = true;
        parameter.allows_null = false;
        parameter.is_array_type = false;
        parameter.is_callable_type = false;
        parameter.type_metadata = reflection_declared_type_metadata(&type_expr);
    }
    let reflected_method_name = crate::types::php_src_date_method_canonical_name(
        class_name,
        &method_key,
    )
    .unwrap_or(method_name)
    .to_string();
    Ok(Some(ReflectionListedMember {
        name: reflected_method_name,
        declaring_class_name,
        attr_names,
        attr_args,
        constant_value: None,
        backing_value: None,
        is_enum_case: false,
        flags,
        modifiers: reflection_method_modifiers_from_flags(flags),
        type_metadata,
        default_value: None,
        property_hook_members: Vec::new(),
        required_parameter_count,
        is_deprecated: sig.deprecation.is_some(),
        is_generator,
        prototype_member,
        parameters,
    }))
}

/// Builds ReflectionMethod array entries for methods declared by an interface.
pub(super) fn reflection_interface_method_members(
    ctx: &FunctionContext<'_>,
    info: &InterfaceInfo,
    interface_name: &str,
    method_names: &[String],
) -> Result<Vec<ReflectionListedMember>> {
    let mut members = Vec::new();
    for method_name in method_names {
        if let Some(member) =
            reflection_interface_method_member(ctx, info, interface_name, method_name)?
        {
            members.push(member);
        }
    }
    Ok(members)
}

/// Builds one ReflectionMethod array entry from interface metadata.
pub(super) fn reflection_interface_method_member(
    ctx: &FunctionContext<'_>,
    info: &InterfaceInfo,
    interface_name: &str,
    method_name: &str,
) -> Result<Option<ReflectionListedMember>> {
    let method_key = php_symbol_key(method_name);
    let Some((sig, is_static)) = info
        .methods
        .get(&method_key)
        .map(|sig| (sig, false))
        .or_else(|| info.static_methods.get(&method_key).map(|sig| (sig, true)))
    else {
        return Ok(None);
    };
    let declaring_class_name = info
        .method_declaring_interfaces
        .get(&method_key)
        .or_else(|| info.static_method_declaring_interfaces.get(&method_key))
        .cloned()
        .unwrap_or_else(|| interface_name.to_string());
    let required_parameter_count = reflection_required_parameter_count(sig);
    let flags = reflection_member_flags(is_static, &Visibility::Public, false, true, false, false);
    let late_static_return = if is_static {
        info.late_static_static_method_returns.get(&method_key)
    } else {
        info.late_static_method_returns.get(&method_key)
    };
    let type_metadata = reflection_method_return_type_metadata(sig, late_static_return);
    let declaring_function = ReflectionDeclaringFunctionMember::Method {
        name: method_key.clone(),
        declaring_class_name: Some(declaring_class_name.clone()),
        attr_names: Vec::new(),
        attr_args: Vec::new(),
        flags,
        required_parameter_count,
        type_metadata: type_metadata.clone(),
        is_deprecated: sig.deprecation.is_some(),
        is_generator: false,
    };
    let source_defaults = reflection_source_method_defaults(
        ctx,
        declaring_class_name.as_str(),
        &method_key,
        is_static,
    );
    let parameters = reflection_parameter_members_with_declaring_class(
        ctx,
        sig,
        declaring_class_name.as_str(),
        None,
        Some(declaring_class_name.as_str()),
        Some(declaring_function),
        &[],
        source_defaults.as_deref(),
    )?;
    Ok(Some(ReflectionListedMember {
        name: crate::types::php_src_date_method_canonical_name(
            interface_name,
            &method_key,
        )
        .unwrap_or(method_name)
        .to_string(),
        declaring_class_name: Some(declaring_class_name),
        attr_names: Vec::new(),
        attr_args: Vec::new(),
        constant_value: None,
        backing_value: None,
        is_enum_case: false,
        flags,
        modifiers: reflection_method_modifiers_from_flags(flags),
        type_metadata,
        default_value: None,
        property_hook_members: Vec::new(),
        required_parameter_count,
        is_deprecated: sig.deprecation.is_some(),
        is_generator: false,
        prototype_member: None,
        parameters,
    }))
}

/// Builds ReflectionMethod array entries for methods declared by a trait.
pub(super) fn reflection_trait_method_members(
    ctx: &FunctionContext<'_>,
    methods: &std::collections::HashMap<String, TraitMethodInfo>,
    trait_name: &str,
    method_names: &[String],
) -> Result<Vec<ReflectionListedMember>> {
    let mut members = Vec::new();
    for method_name in method_names {
        if let Some(member) = reflection_trait_method_member(ctx, methods, trait_name, method_name)?
        {
            members.push(member);
        }
    }
    Ok(members)
}

/// Builds one ReflectionMethod array entry from retained trait metadata.
pub(super) fn reflection_trait_method_member(
    ctx: &FunctionContext<'_>,
    methods: &std::collections::HashMap<String, TraitMethodInfo>,
    trait_name: &str,
    method_name: &str,
) -> Result<Option<ReflectionListedMember>> {
    let method_key = php_symbol_key(method_name);
    let Some(info) = methods.get(&method_key) else {
        return Ok(None);
    };
    let flags = reflection_member_flags(
        info.is_static,
        &info.visibility,
        info.is_final,
        info.is_abstract,
        false,
        false,
    );
    let required_parameter_count = reflection_required_parameter_count(&info.signature);
    let type_metadata = reflection_return_type_metadata(&info.signature);
    let is_generator = reflection_method_is_generator(ctx, trait_name, &method_key);
    let declaring_function = ReflectionDeclaringFunctionMember::Method {
        name: method_key.clone(),
        declaring_class_name: Some(trait_name.to_string()),
        attr_names: Vec::new(),
        attr_args: Vec::new(),
        flags,
        required_parameter_count,
        type_metadata: type_metadata.clone(),
        is_deprecated: info.signature.deprecation.is_some(),
        is_generator,
    };
    let parameters = reflection_parameter_members_with_declaring_class(
        ctx,
        &info.signature,
        trait_name,
        None,
        Some(trait_name),
        Some(declaring_function),
        &[],
        None,
    )?;
    Ok(Some(ReflectionListedMember {
        name: method_key,
        declaring_class_name: Some(trait_name.to_string()),
        attr_names: Vec::new(),
        attr_args: Vec::new(),
        constant_value: None,
        backing_value: None,
        is_enum_case: false,
        flags,
        modifiers: reflection_method_modifiers_from_flags(flags),
        type_metadata,
        default_value: None,
        property_hook_members: Vec::new(),
        required_parameter_count,
        is_deprecated: info.signature.deprecation.is_some(),
        is_generator,
        prototype_member: None,
        parameters,
    }))
}

/// Returns whether the lowered method body is a generator function.
pub(super) fn reflection_method_is_generator(
    ctx: &FunctionContext<'_>,
    declaring_class_name: &str,
    method_name: &str,
) -> bool {
    let expected_key = php_symbol_key(&format!(
        "{}::{}",
        declaring_class_name.trim_start_matches('\\'),
        method_name
    ));
    ctx.module.class_methods.iter().any(|function| {
        php_symbol_key(function.name.trim_start_matches('\\')) == expected_key
            && function.flags.is_generator
    })
}
