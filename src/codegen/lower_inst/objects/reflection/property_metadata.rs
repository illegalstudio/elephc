//! Purpose:
//! ReflectionProperty and ReflectionParameter metadata resolution.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::reflection`.
//!
//! Key details:
//! - Preserves compile-time metadata, target-aware object layout, and ownership.

use super::*;

/// Resolves `ReflectionProperty(class, property)` metadata.
pub(super) fn reflection_property_metadata(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<ReflectionOwnerMetadata> {
    let Some(class_operand) = inst.operands.first().copied() else {
        return Ok(empty_reflection_metadata());
    };
    let Some(property_operand) = inst.operands.get(1).copied() else {
        return Ok(empty_reflection_metadata());
    };
    let reflected_class = const_string_or_class_operand(ctx, class_operand, "ReflectionProperty")?;
    let property_name = const_required_string_operand(ctx, property_operand, "ReflectionProperty")?;
    Ok(resolve_reflection_class(ctx, &reflected_class)
        .and_then(|(_, info)| {
            let declaring_class_name =
                reflection_property_declaring_class_name(info, &property_name);
            let type_metadata = reflection_property_type_metadata(info, &property_name);
            let member_flags = reflection_property_member_flags(info, &property_name)?;
            let property_hook_members =
                if reflected_class.trim_start_matches('\\') == "DatePeriod" {
                    Vec::new()
                } else {
                    reflection_property_hook_members(
                        info,
                        &property_name,
                        declaring_class_name.as_deref(),
                        member_flags,
                        type_metadata.as_ref(),
                    )
                };
            Some(ReflectionOwnerMetadata {
                reflected_name: Some(property_name.clone()),
                attr_names: info
                    .property_attribute_names
                    .get(&property_name)
                    .cloned()
                    .unwrap_or_default(),
                attr_args: info
                    .property_attribute_args
                    .get(&property_name)
                    .cloned()
                    .unwrap_or_default(),
                interface_names: Vec::new(),
                trait_names: Vec::new(),
                trait_aliases: Vec::new(),
                parent_names: Vec::new(),
                method_names: Vec::new(),
                property_names: Vec::new(),
                constant_names: Vec::new(),
                constant_members: Vec::new(),
                default_property_members: Vec::new(),
                static_property_members: Vec::new(),
                constant_reflection_members: Vec::new(),
                enum_case_members: Vec::new(),
                method_members: Vec::new(),
                property_members: Vec::new(),
                property_hook_members,
                constructor_member: None,
                parent_class_name: declaring_class_name,
                constant_value: None,
                backing_value: None,
                is_enum_case: false,
                parameter_members: Vec::new(),
                type_metadata,
                property_default_value: reflection_property_default_value(info, &property_name),
                required_parameter_count: 0,
                is_deprecated: false,
                is_generator: false,
                prototype_member: None,
                is_final: false,
                is_abstract: false,
                is_interface: false,
                is_trait: false,
                is_enum: false,
                is_readonly: false,
                is_anonymous: false,
                is_instantiable: false,
                is_cloneable: false,
                is_iterable: false,
                modifiers: reflection_property_modifiers_for_info(info, &property_name)?,
                member_flags,
            })
        })
        .unwrap_or_else(empty_reflection_metadata))
}

/// Resolves `ReflectionParameter(target, parameter)` metadata.
pub(super) fn reflection_parameter_metadata(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<ReflectionOwnerMetadata> {
    if inst.operands.len() == 2 {
        return reflection_function_parameter_metadata(ctx, inst);
    }
    let Some(class_operand) = inst.operands.first().copied() else {
        return Ok(empty_reflection_metadata());
    };
    let Some(method_operand) = inst.operands.get(1).copied() else {
        return Ok(empty_reflection_metadata());
    };
    let Some(parameter_operand) = inst.operands.get(2).copied() else {
        return Ok(empty_reflection_metadata());
    };
    let reflected_class = const_string_or_class_operand(ctx, class_operand, "ReflectionParameter")?;
    let method_name = const_required_string_operand(ctx, method_operand, "ReflectionParameter")?;
    let selector = const_parameter_selector_operand(ctx, parameter_operand)?;
    let method_key = php_symbol_key(&method_name);
    let method = reflection_method_member_for_class_like(ctx, &reflected_class, &method_key)?;
    let Some(parameter) = method
        .as_ref()
        .and_then(|method| reflection_parameter_member_for_selector(&method.parameters, selector))
    else {
        return Ok(empty_reflection_metadata());
    };
    Ok(reflection_parameter_owner_metadata(parameter))
}

/// Resolves `ReflectionParameter(function, parameter)` metadata.
pub(super) fn reflection_function_parameter_metadata(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<ReflectionOwnerMetadata> {
    let Some(function_operand) = inst.operands.first().copied() else {
        return Ok(empty_reflection_metadata());
    };
    let Some(parameter_operand) = inst.operands.get(1).copied() else {
        return Ok(empty_reflection_metadata());
    };
    let function_name =
        const_required_string_operand(ctx, function_operand, "ReflectionParameter")?;
    let selector = const_parameter_selector_operand(ctx, parameter_operand)?;
    if let Some((builtin_name, signature)) =
        reflection_builtin_function_signature(&function_name)
    {
        let metadata = reflection_builtin_function_metadata(ctx, &builtin_name, &signature)?;
        let Some(parameter) =
            reflection_parameter_member_for_selector(&metadata.parameter_members, selector)
        else {
            return Ok(empty_reflection_metadata());
        };
        return Ok(reflection_parameter_owner_metadata(parameter));
    }
    let Some(function) = ctx.function_by_name(&function_name) else {
        return Ok(empty_reflection_metadata());
    };
    let Some(signature) = function.signature.as_ref() else {
        return Ok(empty_reflection_metadata());
    };
    let reflected_name = function.name.trim_start_matches('\\').to_string();
    let type_metadata = reflection_return_type_metadata(signature);
    let declaring_function = ReflectionDeclaringFunctionMember::Function {
        name: reflected_name,
        attr_names: function.attribute_names.clone(),
        attr_args: function.attribute_args.clone(),
        required_parameter_count: reflection_required_parameter_count(signature),
        type_metadata,
        is_deprecated: signature.deprecation.is_some(),
        is_generator: function.flags.is_generator,
    };
    let parameters = reflection_parameter_members_with_declaring_function(
        ctx,
        signature,
        "",
        None,
        None,
        Some(declaring_function),
        &[],
        None,
    )?;
    let Some(parameter) = reflection_parameter_member_for_selector(&parameters, selector) else {
        return Ok(empty_reflection_metadata());
    };
    Ok(reflection_parameter_owner_metadata(parameter))
}

/// Builds direct ReflectionParameter constructor metadata from one parameter member.
pub(super) fn reflection_parameter_owner_metadata(
    parameter: ReflectionParameterMember,
) -> ReflectionOwnerMetadata {
    let mut metadata = empty_reflection_metadata();
    metadata.reflected_name = Some(parameter.name.clone());
    metadata.parameter_members.push(parameter);
    metadata
}

/// Resolves a reflected method member on a class, interface, or trait.
pub(super) fn reflection_method_member_for_class_like(
    ctx: &FunctionContext<'_>,
    reflected_class: &str,
    method_key: &str,
) -> Result<Option<ReflectionListedMember>> {
    if let Some((_, info)) = resolve_reflection_class(ctx, reflected_class) {
        return reflection_class_method_member(ctx, reflected_class, info, method_key);
    }
    if let Some(interface_name) = resolve_reflection_interface(ctx, reflected_class) {
        return ctx
            .module
            .interface_infos
            .get(interface_name)
            .map(|info| reflection_interface_method_member(ctx, info, interface_name, method_key))
            .transpose()
            .map(Option::flatten);
    }
    resolve_reflection_trait(ctx, reflected_class)
        .and_then(|trait_name| {
            ctx.module
                .declared_trait_methods
                .get(trait_name)
                .map(|methods| (trait_name, methods))
        })
        .map(|(trait_name, methods)| {
            reflection_trait_method_member(ctx, methods, trait_name, method_key)
        })
        .transpose()
        .map(Option::flatten)
}

/// Returns the selected parameter member by PHP name or zero-based position.
pub(super) fn reflection_parameter_member_for_selector(
    parameters: &[ReflectionParameterMember],
    selector: ReflectionParameterSelector,
) -> Option<ReflectionParameterMember> {
    match selector {
        ReflectionParameterSelector::Name(name) => parameters
            .iter()
            .find(|parameter| parameter.name == name)
            .cloned(),
        ReflectionParameterSelector::Position(position) if position >= 0 => {
            parameters.get(position as usize).cloned()
        }
        ReflectionParameterSelector::Position(_) => None,
    }
}
