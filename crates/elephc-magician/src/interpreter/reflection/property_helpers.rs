//! Purpose:
//! Normalizes ReflectionProperty arguments and materializes property hook metadata.
//!
//! Called from:
//! - Property Reflection APIs before runtime reads, writes, or hook inspection.
//!
//! Key details:
//! - Static defaults, raw-value arguments, and synthetic get/set hook methods converge here.

use super::*;

/// Binds `getStaticPropertyValue()` arguments while preserving whether a default was supplied.
pub(super) fn eval_reflection_static_property_value_args(
    evaluated_args: Vec<EvaluatedCallArg>,
) -> Result<(RuntimeCellHandle, Option<RuntimeCellHandle>), EvalStatus> {
    let params = [String::from("name"), String::from("default")];
    let mut bound_args = [None, None];
    let mut next_positional = 0;
    for arg in evaluated_args {
        if let Some(name) = arg.name {
            let Some(position) = params.iter().position(|param| param == &name) else {
                return Err(EvalStatus::RuntimeFatal);
            };
            if bound_args[position].is_some() {
                return Err(EvalStatus::RuntimeFatal);
            }
            bound_args[position] = Some(arg.value);
        } else {
            while next_positional < bound_args.len() && bound_args[next_positional].is_some() {
                next_positional += 1;
            }
            if next_positional >= bound_args.len() {
                return Err(EvalStatus::RuntimeFatal);
            }
            bound_args[next_positional] = Some(arg.value);
            next_positional += 1;
        }
    }
    let property_name = bound_args[0].ok_or(EvalStatus::RuntimeFatal)?;
    Ok((property_name, bound_args[1]))
}

/// Binds the optional `ReflectionProperty::getValue()` object argument.
pub(super) fn eval_reflection_property_get_value_arg(
    evaluated_args: Vec<EvaluatedCallArg>,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let params = [String::from("object")];
    let mut bound_arg = None;
    for arg in evaluated_args {
        if let Some(name) = arg.name {
            if params.iter().all(|param| param != &name) || bound_arg.is_some() {
                return Err(EvalStatus::RuntimeFatal);
            }
            bound_arg = Some(arg.value);
        } else if bound_arg.is_none() {
            bound_arg = Some(arg.value);
        } else {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    Ok(bound_arg)
}

/// Binds `ReflectionProperty::setValue()` arguments while allowing PHP's static shorthand.
pub(super) fn eval_reflection_property_set_value_args(
    evaluated_args: Vec<EvaluatedCallArg>,
) -> Result<(RuntimeCellHandle, Option<RuntimeCellHandle>), EvalStatus> {
    let params = [String::from("objectOrValue"), String::from("value")];
    let mut bound_args = [None, None];
    let mut next_positional = 0;
    for arg in evaluated_args {
        if let Some(name) = arg.name {
            let Some(position) = params.iter().position(|param| param == &name) else {
                return Err(EvalStatus::RuntimeFatal);
            };
            if bound_args[position].is_some() {
                return Err(EvalStatus::RuntimeFatal);
            }
            bound_args[position] = Some(arg.value);
        } else {
            while next_positional < bound_args.len() && bound_args[next_positional].is_some() {
                next_positional += 1;
            }
            if next_positional >= bound_args.len() {
                return Err(EvalStatus::RuntimeFatal);
            }
            bound_args[next_positional] = Some(arg.value);
            next_positional += 1;
        }
    }
    let object_or_value = bound_args[0].ok_or(EvalStatus::RuntimeFatal)?;
    Ok((object_or_value, bound_args[1]))
}

/// Binds the required object argument for `ReflectionProperty::getRawValue()`.
pub(super) fn eval_reflection_property_raw_value_arg(
    evaluated_args: Vec<EvaluatedCallArg>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let args = bind_evaluated_function_args(&[String::from("object")], evaluated_args)?;
    Ok(args[0])
}

/// Binds the object and value arguments for `ReflectionProperty::setRawValue()`.
pub(super) fn eval_reflection_property_set_raw_value_args(
    evaluated_args: Vec<EvaluatedCallArg>,
) -> Result<(RuntimeCellHandle, RuntimeCellHandle), EvalStatus> {
    let args = bind_evaluated_function_args(
        &[String::from("object"), String::from("value")],
        evaluated_args,
    )?;
    Ok((args[0], args[1]))
}

/// Returns the eval property metadata eligible for ReflectionProperty hook APIs.
pub(super) fn eval_reflection_property_for_hooks(
    declaring_class: &str,
    property_name: &str,
    context: &ElephcEvalContext,
) -> Option<(String, EvalClassProperty)> {
    if context.has_class(declaring_class) || context.has_enum(declaring_class) {
        return context.class_property(declaring_class, property_name);
    }
    context
        .trait_decl(declaring_class)
        .and_then(|trait_decl| {
            trait_decl
                .properties()
                .iter()
                .find(|property| property.name() == property_name)
                .map(|property| (trait_decl.name().to_string(), property.clone()))
        })
        .or_else(|| {
            context
                .interface_property_requirements(declaring_class)
                .into_iter()
                .find(|property| property.name() == property_name)
                .map(|property| {
                    let property = EvalClassProperty::new(property.name(), None)
                        .with_type(property.property_type().cloned())
                        .with_attributes(property.attributes().to_vec())
                        .with_abstract_hook_contract(
                            property.requires_get(),
                            property.requires_set(),
                        );
                    (declaring_class.to_string(), property)
                })
        })
        .or_else(|| {
            eval_reflection_native_interface_property_requirement(
                declaring_class,
                property_name,
                context,
            )
            .map(|(owner, property)| {
                let property = EvalClassProperty::new(property.name(), None)
                    .with_type(property.property_type().cloned())
                    .with_attributes(property.attributes().to_vec())
                    .with_abstract_hook_contract(property.requires_get(), property.requires_set());
                (owner, property)
            })
        })
}

/// Returns one generated/AOT interface property contract registered for eval reflection.
pub(super) fn eval_reflection_native_interface_property_requirement(
    interface_name: &str,
    property_name: &str,
    context: &ElephcEvalContext,
) -> Option<(String, EvalInterfaceProperty)> {
    context
        .native_interface_property_requirements(interface_name)
        .into_iter()
        .find(|(_, property)| property.name() == property_name)
}

/// Returns generated/AOT interface property names registered for eval reflection.
pub(super) fn eval_reflection_native_interface_property_names(
    interface_name: &str,
    context: &ElephcEvalContext,
) -> Vec<String> {
    context
        .native_interface_property_requirements(interface_name)
        .into_iter()
        .map(|(_, property)| property.name().to_string())
        .collect()
}

/// Binds the `PropertyHookType $type` argument used by ReflectionProperty hook APIs.
pub(super) fn eval_reflection_property_hook_arg(
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalReflectionPropertyHook, EvalStatus> {
    let args = bind_evaluated_function_args(&[String::from("type")], evaluated_args)?;
    eval_reflection_property_hook_type(args[0], context, values)
}

/// Converts one synthetic `PropertyHookType` object into an eval reflection hook kind.
pub(super) fn eval_reflection_property_hook_type(
    value: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalReflectionPropertyHook, EvalStatus> {
    if values.type_tag(value)? != EVAL_TAG_OBJECT {
        return Err(EvalStatus::RuntimeFatal);
    }
    let identity = values.object_identity(value)?;
    if !context.dynamic_object_is_class(identity, "PropertyHookType") {
        return Err(EvalStatus::RuntimeFatal);
    }
    let hook_value = values.property_get(value, "value")?;
    match eval_reflection_string_arg(hook_value, values)?.as_str() {
        "get" => Ok(EvalReflectionPropertyHook::Get),
        "set" => Ok(EvalReflectionPropertyHook::Set),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Returns concrete hook kinds declared on one eval property.
pub(super) fn eval_reflection_property_hook_kinds(
    property: &EvalClassProperty,
) -> Vec<EvalReflectionPropertyHook> {
    let mut hooks = Vec::new();
    if property.has_get_hook() || property.requires_get_hook() {
        hooks.push(EvalReflectionPropertyHook::Get);
    }
    if property.has_set_hook() || property.requires_set_hook() {
        hooks.push(EvalReflectionPropertyHook::Set);
    }
    hooks
}

/// Returns whether one eval property exposes the requested concrete hook.
pub(super) fn eval_reflection_property_has_hook(
    property: &EvalClassProperty,
    hook: EvalReflectionPropertyHook,
) -> bool {
    match hook {
        EvalReflectionPropertyHook::Get => property.has_get_hook() || property.requires_get_hook(),
        EvalReflectionPropertyHook::Set => property.has_set_hook() || property.requires_set_hook(),
    }
}

/// Builds PHP's string-keyed ReflectionMethod map returned by `getHooks()`.
pub(super) fn eval_reflection_property_hook_method_array(
    declaring_class: &str,
    property: &EvalClassProperty,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let hooks = eval_reflection_property_hook_kinds(property);
    let mut result = values.assoc_new(hooks.len())?;
    for hook in hooks {
        let key = values.string(hook.key())?;
        let method = eval_reflection_property_hook_method_object(
            declaring_class,
            property,
            hook,
            context,
            values,
        )?;
        result = values.array_set(result, key, method)?;
    }
    Ok(result)
}

/// Materializes a ReflectionMethod object for one concrete property hook.
pub(super) fn eval_reflection_property_hook_method_object(
    declaring_class: &str,
    property: &EvalClassProperty,
    hook: EvalReflectionPropertyHook,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let metadata = eval_reflection_property_hook_method_metadata(declaring_class, property, hook);
    eval_reflection_member_object_result(
        EVAL_REFLECTION_OWNER_METHOD,
        &hook.reflected_method_name(property.name()),
        &metadata,
        context,
        values,
    )
}

/// Builds ReflectionMethod metadata for one eval property hook accessor.
pub(super) fn eval_reflection_property_hook_method_metadata(
    declaring_class: &str,
    property: &EvalClassProperty,
    hook: EvalReflectionPropertyHook,
) -> EvalReflectionMemberMetadata {
    let parameters = eval_reflection_property_hook_parameters(declaring_class, property, hook);
    let required_parameter_count = parameters.len();
    EvalReflectionMemberMetadata {
        declaring_class_name: Some(declaring_class.to_string()),
        source_file: None,
        source_location: None,
        attributes: Vec::new(),
        visibility: property.visibility(),
        is_static: false,
        is_final: false,
        is_abstract: property.is_abstract(),
        is_readonly: false,
        is_promoted: false,
        is_dynamic: false,
        modifiers: eval_reflection_method_modifiers(
            property.visibility(),
            false,
            false,
            property.is_abstract(),
        ),
        type_metadata: None,
        settable_type_metadata: None,
        return_type_metadata: eval_reflection_property_hook_return_type(property, hook),
        default_value: None,
        default_value_trait_origin: None,
        required_parameter_count,
        parameters,
    }
}

/// Builds the synthetic setter parameter metadata exposed by PHP hook reflection.
pub(super) fn eval_reflection_property_hook_parameters(
    declaring_class: &str,
    property: &EvalClassProperty,
    hook: EvalReflectionPropertyHook,
) -> Vec<EvalReflectionParameterMetadata> {
    if !matches!(hook, EvalReflectionPropertyHook::Set) {
        return Vec::new();
    }
    let type_metadata = property
        .settable_type()
        .and_then(eval_reflection_parameter_type_metadata);
    let has_type = type_metadata.is_some();
    let is_array_type = eval_reflection_parameter_has_named_type(type_metadata.as_ref(), "array");
    let is_callable_type =
        eval_reflection_parameter_has_named_type(type_metadata.as_ref(), "callable");
    let declaring_function = EvalReflectionDeclaringFunctionMetadata {
        name: hook.reflected_method_name(property.name()),
        declaring_class_name: Some(declaring_class.to_string()),
        magic_scope: None,
        attributes: Vec::new(),
        flags: eval_reflection_member_flags(property.visibility(), false, false, false, false),
        required_parameter_count: 1,
    };
    vec![EvalReflectionParameterMetadata {
        name: "value".to_string(),
        declaring_class_name: Some(declaring_class.to_string()),
        declaring_function: Some(declaring_function),
        attributes: Vec::new(),
        position: 0,
        is_optional: false,
        is_variadic: false,
        is_passed_by_reference: false,
        is_promoted: false,
        has_type,
        allows_null: type_metadata
            .as_ref()
            .is_some_and(eval_reflection_type_allows_null),
        is_array_type,
        is_callable_type,
        type_metadata,
        default_value: None,
        default_value_constant_name: None,
    }]
}

/// Returns the ReflectionMethod return type metadata for a property hook.
pub(super) fn eval_reflection_property_hook_return_type(
    property: &EvalClassProperty,
    hook: EvalReflectionPropertyHook,
) -> Option<EvalReflectionParameterTypeMetadata> {
    match hook {
        EvalReflectionPropertyHook::Get => property
            .property_type()
            .and_then(eval_reflection_parameter_type_metadata),
        EvalReflectionPropertyHook::Set => Some(EvalReflectionParameterTypeMetadata {
            kind: EvalReflectionParameterTypeKind::Named(eval_reflection_builtin_named_type(
                "void", false,
            )),
        }),
    }
}

/// Maps PHP-visible property-hook method names back to eval's synthetic method names.
pub(super) fn eval_reflection_property_hook_synthetic_method_name(method_name: &str) -> Option<String> {
    let body = method_name.strip_prefix('$')?;
    let (property_name, hook_name) = body.rsplit_once("::")?;
    match hook_name {
        "get" => Some(EvalReflectionPropertyHook::Get.synthetic_method_name(property_name)),
        "set" => Some(EvalReflectionPropertyHook::Set.synthetic_method_name(property_name)),
        _ => None,
    }
}
