//! Purpose:
//! Builds reflected method and property metadata for eval and native class-like targets.
//!
//! Called from:
//! - Reflection owner construction, class member APIs, and property access.
//!
//! Key details:
//! - Trait composition, enum synthetic methods, defaults, and visibility are resolved here.

use super::*;

/// Returns method metadata for a method-like member on an eval class-like symbol.
pub(super) fn eval_reflection_method_metadata(
    class_name: &str,
    method_name: &str,
    context: &ElephcEvalContext,
) -> Option<EvalReflectionMemberMetadata> {
    if context.has_class(class_name) || context.has_enum(class_name) {
        if let Some((declaring_class, method)) = context.class_method(class_name, method_name) {
            let required_parameter_count = eval_reflection_required_parameter_count(
                method.parameter_defaults(),
                method.parameter_is_variadic(),
            );
            let mut flags = eval_reflection_member_flags(
                method.visibility(),
                method.is_static(),
                method.is_final(),
                method.is_abstract(),
                false,
            );
            flags |= eval_reflection_callable_flags(method.attributes());
            let return_type_metadata = method
                .return_type()
                .and_then(eval_reflection_parameter_type_metadata);
            let declaring_function = EvalReflectionDeclaringFunctionMetadata {
                name: method.name().to_string(),
                declaring_class_name: Some(declaring_class.clone()),
                magic_scope: Some(eval_reflection_eval_method_parameter_magic_scope(
                    &declaring_class,
                    &method,
                    None,
                )),
                attributes: method.attributes().to_vec(),
                flags,
                required_parameter_count,
            };
            let promoted_parameter_names = eval_reflection_promoted_parameter_names(
                &declaring_class,
                method.name(),
                context,
            );
            let parameters = eval_reflection_parameters_from_names_and_type_flags(
                Some(declaring_class.as_str()),
                Some(&declaring_function),
                method.params(),
                method.parameter_has_types(),
                method.parameter_types(),
                method.parameter_attributes(),
                method.parameter_defaults(),
                method.parameter_is_by_ref(),
                method.parameter_is_variadic(),
                &promoted_parameter_names,
            );
            return Some(EvalReflectionMemberMetadata {
                declaring_class_name: Some(declaring_class),
                source_file: None,
                source_location: method.source_location(),
                attributes: method.attributes().to_vec(),
                visibility: method.visibility(),
                is_static: method.is_static(),
                is_final: method.is_final(),
                is_abstract: method.is_abstract(),
                is_readonly: false,
                is_promoted: false,
                is_dynamic: false,
                modifiers: eval_reflection_method_modifiers(
                    method.visibility(),
                    method.is_static(),
                    method.is_final(),
                    method.is_abstract(),
                ),
                type_metadata: None,
                settable_type_metadata: None,
                return_type_metadata,
                default_value: None,
                default_value_trait_origin: None,
                required_parameter_count,
                parameters,
            });
        }
        return eval_reflection_enum_synthetic_method_metadata(class_name, method_name, context);
    }
    if context.has_interface(class_name) {
        return context
            .interface_method_requirements(class_name)
            .into_iter()
            .find(|method| method.name().eq_ignore_ascii_case(method_name))
            .map(|method| {
                let required_parameter_count = eval_reflection_required_parameter_count(
                    method.parameter_defaults(),
                    method.parameter_is_variadic(),
                );
                let mut flags = eval_reflection_member_flags(
                    EvalVisibility::Public,
                    method.is_static(),
                    false,
                    true,
                    false,
                );
                flags |= eval_reflection_callable_flags(method.attributes());
                let return_type_metadata = method
                    .return_type()
                    .and_then(eval_reflection_parameter_type_metadata);
                let declaring_function = EvalReflectionDeclaringFunctionMetadata {
                    name: method.name().to_string(),
                    declaring_class_name: Some(class_name.to_string()),
                    magic_scope: Some(eval_reflection_method_parameter_magic_scope(
                        class_name,
                        method.name(),
                        &format!("{}::{}", class_name.trim_start_matches('\\'), method.name()),
                        None,
                    )),
                    attributes: method.attributes().to_vec(),
                    flags,
                    required_parameter_count,
                };
                let parameters = eval_reflection_parameters_from_names_and_type_flags(
                    Some(class_name),
                    Some(&declaring_function),
                    method.params(),
                    method.parameter_has_types(),
                    method.parameter_types(),
                    method.parameter_attributes(),
                    method.parameter_defaults(),
                    method.parameter_is_by_ref(),
                    method.parameter_is_variadic(),
                    &[],
                );
                EvalReflectionMemberMetadata {
                    declaring_class_name: Some(class_name.to_string()),
                    source_file: None,
                    source_location: method.source_location(),
                    attributes: method.attributes().to_vec(),
                    visibility: EvalVisibility::Public,
                    is_static: method.is_static(),
                    is_final: false,
                    is_abstract: true,
                    is_readonly: false,
                    is_promoted: false,
                    is_dynamic: false,
                    modifiers: eval_reflection_method_modifiers(
                        EvalVisibility::Public,
                        method.is_static(),
                        false,
                        true,
                    ),
                    type_metadata: None,
                    settable_type_metadata: None,
                    return_type_metadata,
                    default_value: None,
                    default_value_trait_origin: None,
                    required_parameter_count,
                    parameters,
                }
            });
    }
    context.trait_decl(class_name).and_then(|trait_decl| {
        trait_decl
            .methods()
            .iter()
            .find(|method| method.name().eq_ignore_ascii_case(method_name))
            .map(|method| {
                let required_parameter_count = eval_reflection_required_parameter_count(
                    method.parameter_defaults(),
                    method.parameter_is_variadic(),
                );
                let mut flags = eval_reflection_member_flags(
                    method.visibility(),
                    method.is_static(),
                    method.is_final(),
                    method.is_abstract(),
                    false,
                );
                flags |= eval_reflection_callable_flags(method.attributes());
                let return_type_metadata = method
                    .return_type()
                    .and_then(eval_reflection_parameter_type_metadata);
                let declaring_function = EvalReflectionDeclaringFunctionMetadata {
                    name: method.name().to_string(),
                    declaring_class_name: Some(trait_decl.name().to_string()),
                    magic_scope: Some(eval_reflection_eval_method_parameter_magic_scope(
                        trait_decl.name(),
                        method,
                        Some(trait_decl.name()),
                    )),
                    attributes: method.attributes().to_vec(),
                    flags,
                    required_parameter_count,
                };
                let promoted_parameter_names =
                    eval_reflection_promoted_trait_parameter_names(trait_decl, method.name());
                let parameters = eval_reflection_parameters_from_names_and_type_flags(
                    Some(trait_decl.name()),
                    Some(&declaring_function),
                    method.params(),
                    method.parameter_has_types(),
                    method.parameter_types(),
                    method.parameter_attributes(),
                    method.parameter_defaults(),
                    method.parameter_is_by_ref(),
                    method.parameter_is_variadic(),
                    &promoted_parameter_names,
                );
                EvalReflectionMemberMetadata {
                    declaring_class_name: Some(trait_decl.name().to_string()),
                    source_file: None,
                    source_location: method.source_location(),
                    attributes: method.attributes().to_vec(),
                    visibility: method.visibility(),
                    is_static: method.is_static(),
                    is_final: method.is_final(),
                    is_abstract: method.is_abstract(),
                    is_readonly: false,
                    is_promoted: false,
                    is_dynamic: false,
                    modifiers: eval_reflection_method_modifiers(
                        method.visibility(),
                        method.is_static(),
                        method.is_final(),
                        method.is_abstract(),
                    ),
                    type_metadata: None,
                    settable_type_metadata: None,
                    return_type_metadata,
                    default_value: None,
                    default_value_trait_origin: None,
                    required_parameter_count,
                    parameters,
                }
            })
    })
}

/// Builds ReflectionMethod metadata for PHP's enum-provided synthetic methods.
pub(super) fn eval_reflection_enum_synthetic_method_metadata(
    class_name: &str,
    method_name: &str,
    context: &ElephcEvalContext,
) -> Option<EvalReflectionMemberMetadata> {
    let synthetic_name = eval_enum_static_builtin_applies(class_name, method_name, context)?;
    let enum_decl = context.enum_decl(class_name)?;
    let declaring_class_name = enum_decl.name().trim_start_matches('\\').to_string();
    let flags = eval_reflection_member_flags(EvalVisibility::Public, true, false, false, false);
    let (parameter_names, parameter_types, return_type_metadata) = match synthetic_name {
        "cases" => (
            Vec::new(),
            Vec::new(),
            Some(eval_reflection_parameter_type_metadata(&EvalParameterType::new(
                vec![EvalParameterTypeVariant::Array],
                false,
            ))?),
        ),
        "from" | "tryFrom" => {
            let return_type = EvalParameterType::new(
                vec![EvalParameterTypeVariant::Class(String::from("static"))],
                synthetic_name == "tryFrom",
            );
            (
                vec![String::from("value")],
                vec![Some(EvalParameterType::new(
                    vec![EvalParameterTypeVariant::String, EvalParameterTypeVariant::Int],
                    false,
                ))],
                Some(eval_reflection_parameter_type_metadata(&return_type)?),
            )
        }
        _ => return None,
    };
    let parameter_count = parameter_names.len();
    let parameter_has_types = parameter_types
        .iter()
        .map(Option::is_some)
        .collect::<Vec<_>>();
    let parameter_attributes = vec![Vec::new(); parameter_count];
    let parameter_defaults = vec![None; parameter_count];
    let parameter_is_by_ref = vec![false; parameter_count];
    let parameter_is_variadic = vec![false; parameter_count];
    let required_parameter_count =
        eval_reflection_required_parameter_count(&parameter_defaults, &parameter_is_variadic);
    let declaring_function = EvalReflectionDeclaringFunctionMetadata {
        name: synthetic_name.to_string(),
        declaring_class_name: Some(declaring_class_name.clone()),
        magic_scope: None,
        attributes: Vec::new(),
        flags,
        required_parameter_count,
    };
    let parameters = eval_reflection_parameters_from_names_and_type_flags(
        Some(&declaring_class_name),
        Some(&declaring_function),
        &parameter_names,
        &parameter_has_types,
        &parameter_types,
        &parameter_attributes,
        &parameter_defaults,
        &parameter_is_by_ref,
        &parameter_is_variadic,
        &[],
    );
    Some(EvalReflectionMemberMetadata {
        declaring_class_name: Some(declaring_class_name),
        source_file: None,
        source_location: None,
        attributes: Vec::new(),
        visibility: EvalVisibility::Public,
        is_static: true,
        is_final: false,
        is_abstract: false,
        is_readonly: false,
        is_promoted: false,
        is_dynamic: false,
        modifiers: eval_reflection_method_modifiers(EvalVisibility::Public, true, false, false),
        type_metadata: None,
        settable_type_metadata: None,
        return_type_metadata,
        default_value: None,
        default_value_trait_origin: None,
        required_parameter_count,
        parameters,
    })
}

/// Returns property metadata for a property-like member on an eval class-like symbol.
pub(super) fn eval_reflection_property_metadata(
    class_name: &str,
    property_name: &str,
    context: &ElephcEvalContext,
) -> Option<EvalReflectionMemberMetadata> {
    if context.has_class(class_name) || context.has_enum(class_name) {
        return context.class_property(class_name, property_name).map(
            |(declaring_class, property)| {
                let default_value = eval_reflection_property_default_value(&property);
                EvalReflectionMemberMetadata {
                    declaring_class_name: Some(declaring_class),
                    source_file: None,
                    source_location: None,
                    attributes: property.attributes().to_vec(),
                    visibility: property.visibility(),
                    is_static: property.is_static(),
                    is_final: property.is_final(),
                    is_abstract: property.is_abstract(),
                    is_readonly: property.is_readonly(),
                    is_promoted: property.is_promoted(),
                    is_dynamic: false,
                    modifiers: eval_reflection_property_modifiers(
                        property.visibility(),
                        property.set_visibility(),
                        property.is_static(),
                        property.is_final(),
                        property.is_abstract(),
                        property.is_readonly(),
                        eval_reflection_property_is_virtual(&property),
                    ),
                    type_metadata: property
                        .property_type()
                        .and_then(eval_reflection_parameter_type_metadata),
                    settable_type_metadata: property
                        .settable_type()
                        .and_then(eval_reflection_parameter_type_metadata),
                    return_type_metadata: None,
                    default_value,
                    default_value_trait_origin: property.trait_origin().map(str::to_string),
                    required_parameter_count: 0,
                    parameters: Vec::new(),
                }
            },
        );
    }
    if context.has_interface(class_name) {
        return context
            .interface_property_requirements(class_name)
            .into_iter()
            .find(|property| property.name() == property_name)
            .map(|property| {
                eval_reflection_interface_property_metadata(class_name.to_string(), &property)
            });
    }
    if let Some((declaring_class, property)) =
        eval_reflection_native_interface_property_requirement(class_name, property_name, context)
    {
        return Some(eval_reflection_interface_property_metadata(
            declaring_class,
            &property,
        ));
    }
    context.trait_decl(class_name).and_then(|trait_decl| {
        trait_decl
            .properties()
            .iter()
            .find(|property| property.name() == property_name)
            .map(|property| {
                let default_value = eval_reflection_property_default_value(property);
                EvalReflectionMemberMetadata {
                    declaring_class_name: Some(trait_decl.name().to_string()),
                    source_file: None,
                    source_location: None,
                    attributes: property.attributes().to_vec(),
                    visibility: property.visibility(),
                    is_static: property.is_static(),
                    is_final: property.is_final(),
                    is_abstract: property.is_abstract(),
                    is_readonly: property.is_readonly(),
                    is_promoted: property.is_promoted(),
                    is_dynamic: false,
                    modifiers: eval_reflection_property_modifiers(
                        property.visibility(),
                        property.set_visibility(),
                        property.is_static(),
                        property.is_final(),
                        property.is_abstract(),
                        property.is_readonly(),
                        eval_reflection_property_is_virtual(property),
                    ),
                    type_metadata: property
                        .property_type()
                        .and_then(eval_reflection_parameter_type_metadata),
                    settable_type_metadata: property
                        .settable_type()
                        .and_then(eval_reflection_parameter_type_metadata),
                    return_type_metadata: None,
                    default_value,
                    default_value_trait_origin: property
                        .trait_origin()
                        .map(str::to_string)
                        .or_else(|| Some(trait_decl.name().to_string())),
                    required_parameter_count: 0,
                    parameters: Vec::new(),
                }
        })
    })
}

/// Returns property metadata for a property contract declared on an interface.
pub(super) fn eval_reflection_interface_property_metadata(
    declaring_class: String,
    property: &EvalInterfaceProperty,
) -> EvalReflectionMemberMetadata {
    EvalReflectionMemberMetadata {
        declaring_class_name: Some(declaring_class),
        source_file: None,
        source_location: None,
        attributes: property.attributes().to_vec(),
        visibility: EvalVisibility::Public,
        is_static: false,
        is_final: false,
        is_abstract: true,
        is_readonly: false,
        is_promoted: false,
        is_dynamic: false,
        modifiers: eval_reflection_property_modifiers(
            EvalVisibility::Public,
            property.set_visibility(),
            false,
            false,
            true,
            false,
            true,
        ),
        type_metadata: property
            .property_type()
            .and_then(eval_reflection_parameter_type_metadata),
        settable_type_metadata: property
            .property_type()
            .and_then(eval_reflection_parameter_type_metadata),
        return_type_metadata: None,
        default_value: None,
        default_value_trait_origin: None,
        required_parameter_count: 0,
        parameters: Vec::new(),
    }
}

/// Returns property names that can contribute to `ReflectionClass::getDefaultProperties()`.
pub(super) fn eval_reflection_default_property_names(
    reflected_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    if context.has_class(reflected_name)
        || context.has_enum(reflected_name)
        || context.has_trait(reflected_name)
        || context.has_interface(reflected_name)
    {
        return Ok(eval_reflection_eval_property_names(reflected_name, context));
    }
    eval_reflection_aot_member_names(EVAL_REFLECTION_OWNER_PROPERTY, reflected_name, values)
}

/// Returns eval or generated/AOT property metadata for default-property materialization.
pub(super) fn eval_reflection_default_property_metadata(
    reflected_name: &str,
    property_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalReflectionMemberMetadata>, EvalStatus> {
    if let Some(member) = eval_reflection_property_metadata(reflected_name, property_name, context) {
        return Ok(Some(member));
    }
    if let Some((declaring_class, property)) =
        eval_reflection_native_interface_property_requirement(
            reflected_name,
            property_name,
            context,
        )
    {
        return Ok(Some(eval_reflection_interface_property_metadata(
            declaring_class,
            &property,
        )));
    }
    eval_reflection_aot_property_metadata_if_exists(reflected_name, property_name, context, values)
}

/// Returns eval or generated/AOT metadata for a materialized `ReflectionProperty`.
pub(super) fn eval_reflection_reflected_property_metadata(
    declaring_class: &str,
    property_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalReflectionMemberMetadata>, EvalStatus> {
    if let Some(member) = eval_reflection_property_metadata(declaring_class, property_name, context) {
        return Ok(Some(member));
    }
    if let Some((declaring_class, property)) =
        eval_reflection_native_interface_property_requirement(
            declaring_class,
            property_name,
            context,
        )
    {
        return Ok(Some(eval_reflection_interface_property_metadata(
            declaring_class,
            &property,
        )));
    }
    eval_reflection_aot_property_metadata_if_exists(declaring_class, property_name, context, values)
}

/// Returns eval-declared property names for reflection APIs that do not use AOT lists.
pub(super) fn eval_reflection_eval_property_names(
    reflected_name: &str,
    context: &ElephcEvalContext,
) -> Vec<String> {
    if context.has_trait(reflected_name) {
        return context.trait_property_names(reflected_name);
    }
    if context.has_interface(reflected_name) {
        return context.interface_property_names(reflected_name);
    }
    context.class_property_names(reflected_name)
}

/// Returns property names that can contribute to `ReflectionClass::getStaticProperties()`.
pub(super) fn eval_reflection_static_property_names(
    reflected_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    if eval_reflection_class_like_exists(reflected_name, context) {
        return Ok(eval_reflection_eval_property_names(reflected_name, context)
            .into_iter()
            .filter(|name| {
                eval_reflection_property_metadata(reflected_name, name, context)
                    .is_some_and(|property| property.is_static)
            })
            .collect());
    }
    let names =
        eval_reflection_aot_member_names(EVAL_REFLECTION_OWNER_PROPERTY, reflected_name, values)?;
    let mut result = Vec::new();
    for name in names {
        if eval_reflection_aot_property_metadata_if_exists(reflected_name, &name, context, values)?
            .is_some_and(|property| property.is_static)
        {
            result.push(name);
        }
    }
    Ok(result)
}

/// Returns eval or generated/AOT property metadata for static-property reflection.
pub(super) fn eval_reflection_static_property_metadata(
    reflected_name: &str,
    property_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalReflectionMemberMetadata>, EvalStatus> {
    eval_reflection_reflected_property_metadata(reflected_name, property_name, context, values)
}

/// Returns the current eval or generated/AOT static property value.
pub(super) fn eval_reflection_static_property_value(
    reflected_name: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(member) =
        eval_reflection_static_property_metadata(reflected_name, property_name, context, values)?
    else {
        return Ok(None);
    };
    if !member.is_static {
        return Ok(None);
    }
    if eval_reflection_class_like_exists(reflected_name, context) {
        let declaring_class = member
            .declaring_class_name
            .as_deref()
            .ok_or(EvalStatus::RuntimeFatal)?;
        if let Some(value) = context.static_property(declaring_class, property_name) {
            return Ok(Some(value));
        }
        return member
            .default_value
            .as_ref()
            .map(|default| eval_reflection_member_default_value(&member, default, context, values))
            .transpose();
    }
    let declaring_class = member
        .declaring_class_name
        .as_deref()
        .unwrap_or(reflected_name);
    eval_reflection_with_declaring_class_scope(declaring_class, context, |_| {
        values.static_property_get(reflected_name, property_name)
    })
}
