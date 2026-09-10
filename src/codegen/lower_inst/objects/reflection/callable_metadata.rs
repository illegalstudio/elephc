//! Purpose:
//! ReflectionFunction and ReflectionMethod metadata resolution.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::reflection`.
//!
//! Key details:
//! - Preserves compile-time metadata, target-aware object layout, and ownership.

use super::*;

/// Resolves `ReflectionFunction(function)` metadata.
pub(super) fn reflection_function_metadata(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<ReflectionOwnerMetadata> {
    let Some(function_operand) = inst.operands.first().copied() else {
        return Ok(empty_reflection_metadata());
    };
    let function_name = const_required_string_operand(ctx, function_operand, "ReflectionFunction")?;
    reflection_function_metadata_for_name(ctx, &function_name)
}

/// Resolves ReflectionFunction metadata for a literal or runtime-dispatch candidate name.
pub(super) fn reflection_function_metadata_for_name(
    ctx: &FunctionContext<'_>,
    function_name: &str,
) -> Result<ReflectionOwnerMetadata> {
    if ctx.function_by_name(function_name).is_some() {
        return reflection_registered_function_metadata(ctx, function_name);
    }
    if let Some((builtin_name, signature)) = reflection_builtin_function_signature(&function_name) {
        return reflection_builtin_function_metadata(ctx, &builtin_name, &signature);
    }
    Ok(empty_reflection_metadata())
}

/// Builds metadata for a declared compiler function while preserving its PHP-visible case.
pub(super) fn reflection_registered_function_metadata(
    ctx: &FunctionContext<'_>,
    function_name: &str,
) -> Result<ReflectionOwnerMetadata> {
    let function = ctx
        .function_by_name(function_name)
        .ok_or_else(|| CodegenIrError::missing_entry("function", 0))?;
    let Some(signature) = function.signature.as_ref() else {
        return Ok(empty_reflection_metadata());
    };
    let reflected_name = function.name.trim_start_matches('\\').to_string();
    let required_parameter_count = reflection_required_parameter_count(signature);
    let type_metadata = reflection_return_type_metadata(signature);
    let declaring_function = ReflectionDeclaringFunctionMember::Function {
        name: reflected_name.clone(),
        attr_names: function.attribute_names.clone(),
        attr_args: function.attribute_args.clone(),
        required_parameter_count,
        type_metadata: type_metadata.clone(),
        is_deprecated: signature.deprecation.is_some(),
        is_generator: function.flags.is_generator,
    };
    let mut metadata = empty_reflection_metadata();
    metadata.reflected_name = Some(reflected_name);
    metadata.attr_names = function.attribute_names.clone();
    metadata.attr_args = function.attribute_args.clone();
    metadata.parameter_members = reflection_parameter_members_with_declaring_function(
        ctx,
        signature,
        "",
        None,
        None,
        Some(declaring_function),
        &[],
        None,
    )?;
    metadata.required_parameter_count = required_parameter_count;
    metadata.type_metadata = type_metadata;
    metadata.is_deprecated = signature.deprecation.is_some();
    metadata.is_generator = function.flags.is_generator;
    Ok(metadata)
}

/// Applies source-display forms whose semantics are retained by the locked PHP 8.5.8 snapshot.
///
/// The SimpleXML class-name defaults evaluate to the string `SimpleXMLElement`, while php-src
/// renders their original `SimpleXMLElement::class` expression in ReflectionFunction text. The
/// exact function/parameter/value triples below are intentionally enumerated rather than inferred
/// from arbitrary strings, and do not change ReflectionParameter's constant metadata APIs.
fn apply_locked_internal_function_default_displays(
    function_name: &str,
    parameters: &mut [ReflectionParameterMember],
) {
    let is_simplexml_class_name_default = matches!(
        php_symbol_key(function_name).as_str(),
        "simplexml_load_file" | "simplexml_load_string" | "simplexml_import_dom"
    );
    if !is_simplexml_class_name_default {
        return;
    }
    for parameter in parameters {
        if parameter.name == "class_name"
            && matches!(
                parameter.default_value.as_ref(),
                Some(ReflectionParameterDefaultValue::Str(value)) if value == "SimpleXMLElement"
            )
        {
            parameter.default_value_display = Some(
                ReflectionParameterDefaultDisplay::ClassNameConstant(
                    "SimpleXMLElement".to_string(),
                ),
            );
        }
    }
}

/// Builds metadata for a supported builtin `ReflectionFunction`.
pub(super) fn reflection_builtin_function_metadata(
    ctx: &FunctionContext<'_>,
    function_name: &str,
    signature: &FunctionSig,
) -> Result<ReflectionOwnerMetadata> {
    let required_parameter_count = reflection_required_parameter_count(signature);
    let type_metadata = reflection_return_type_metadata(signature);
    let declaring_function = ReflectionDeclaringFunctionMember::Function {
        name: function_name.to_string(),
        attr_names: Vec::new(),
        attr_args: Vec::new(),
        required_parameter_count,
        type_metadata: type_metadata.clone(),
        is_deprecated: signature.deprecation.is_some(),
        is_generator: false,
    };
    let mut metadata = empty_reflection_metadata();
    metadata.reflected_name = Some(function_name.to_string());
    metadata.parameter_members = reflection_parameter_members_with_declaring_function(
        ctx,
        signature,
        "",
        None,
        None,
        Some(declaring_function),
        &[],
        None,
    )?;
    apply_locked_internal_function_default_displays(function_name, &mut metadata.parameter_members);
    metadata.required_parameter_count = required_parameter_count;
    metadata.type_metadata = type_metadata;
    metadata.is_deprecated = signature.deprecation.is_some();
    Ok(metadata)
}

/// Returns the canonical callable-builtin name and signature for ReflectionFunction.
pub(super) fn reflection_builtin_function_signature(function_name: &str) -> Option<(String, FunctionSig)> {
    let lookup_name = function_name.strip_prefix('\\').unwrap_or(function_name);
    if lookup_name.starts_with('\\') {
        return None;
    }
    if let Some(function) = crate::internal_extensions::registry().function(lookup_name) {
        let signature = crate::internal_extensions::function_signature_for(
            &function.exported_name,
            &function.signature,
        )
        .ok()?;
        return Some((function.exported_name.clone(), signature));
    }
    let builtin_key = php_symbol_key(lookup_name);
    crate::types::first_class_callable_builtin_sig(&builtin_key)
        .map(|signature| (builtin_key, signature))
}

/// Resolves a reflected internal function to its locked PHP extension registry entry.
pub(super) fn reflection_extension_name_for_function(
    function_name: &str,
) -> Option<&'static str> {
    let function_key = php_symbol_key(function_name);
    crate::internal_extensions::registry()
        .extensions()
        .find(|extension| {
            extension
                .functions
                .iter()
                .any(|function| php_symbol_key(&function.exported_name) == function_key)
        })
        .map(|extension| extension.name.as_str())
}

/// Returns whether a reflected function or method represents compiler builtin metadata.
pub(super) fn reflection_function_or_method_is_internal(
    class_name: &str,
    metadata: &ReflectionOwnerMetadata,
) -> bool {
    if class_name == "ReflectionFunction" {
        return metadata
            .reflected_name
            .as_deref()
            .and_then(reflection_builtin_function_signature)
            .is_some();
    }
    metadata
        .parent_class_name
        .as_deref()
        .is_some_and(reflection_class_like_is_internal)
}

/// Resolves `ReflectionMethod(class, method)` metadata.
pub(super) fn reflection_method_metadata(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<ReflectionOwnerMetadata> {
    let Some(class_operand) = inst.operands.first().copied() else {
        return Ok(empty_reflection_metadata());
    };
    let Some(method_operand) = inst.operands.get(1).copied() else {
        return Ok(empty_reflection_metadata());
    };
    let reflected_class = const_string_or_class_operand(ctx, class_operand, "ReflectionMethod")?;
    let method_name = const_required_string_operand(ctx, method_operand, "ReflectionMethod")?;
    let method_key = php_symbol_key(&method_name);
    if let Some((_, info)) = resolve_reflection_class(ctx, &reflected_class) {
        if let Some(member) =
            reflection_class_method_member(ctx, &reflected_class, info, &method_key)?
        {
            return Ok(reflection_method_owner_metadata(&method_name, member));
        }
    }
    if let Some(interface_name) = resolve_reflection_interface(ctx, &reflected_class) {
        if let Some(info) = ctx.module.interface_infos.get(interface_name) {
            if let Some(member) =
                reflection_interface_method_member(ctx, info, interface_name, &method_key)?
            {
                return Ok(reflection_method_owner_metadata(&method_name, member));
            }
        }
    }
    if let Some(trait_name) = resolve_reflection_trait(ctx, &reflected_class) {
        if let Some(methods) = ctx.module.declared_trait_methods.get(trait_name) {
            if let Some(member) =
                reflection_trait_method_member(ctx, methods, trait_name, &method_key)?
            {
                return Ok(reflection_method_owner_metadata(&method_name, member));
            }
        }
    }
    Ok(empty_reflection_metadata())
}

/// Builds direct ReflectionMethod constructor metadata from one reflected method member.
pub(super) fn reflection_method_owner_metadata(
    method_name: &str,
    member: ReflectionListedMember,
) -> ReflectionOwnerMetadata {
    ReflectionOwnerMetadata {
        reflected_name: Some(method_name.to_string()),
        attr_names: member.attr_names,
        attr_args: member.attr_args,
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
        property_hook_members: Vec::new(),
        constructor_member: None,
        parent_class_name: member.declaring_class_name,
        constant_value: member.constant_value,
        backing_value: member.backing_value,
        is_enum_case: member.is_enum_case,
        parameter_members: member.parameters,
        type_metadata: member.type_metadata,
        property_default_value: None,
        required_parameter_count: member.required_parameter_count,
        is_deprecated: member.is_deprecated,
        is_generator: member.is_generator,
        prototype_member: member.prototype_member,
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
        modifiers: reflection_method_modifiers_from_flags(member.flags),
        member_flags: member.flags,
    }
}
