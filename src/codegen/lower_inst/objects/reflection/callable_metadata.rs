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
    if let Some((builtin_name, signature)) =
        reflection_builtin_function_signature(&function_name)
    {
        return reflection_builtin_function_metadata(ctx, &builtin_name, &signature);
    }
    let Some(function) = ctx.function_by_name(&function_name) else {
        return Ok(empty_reflection_metadata());
    };
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

/// Builds metadata for a supported builtin `ReflectionFunction`.
pub(super) fn reflection_builtin_function_metadata(
    ctx: &FunctionContext<'_>,
    function_name: &str,
    signature: &FunctionSig,
) -> Result<ReflectionOwnerMetadata> {
    let required_parameter_count = reflection_required_parameter_count(signature);
    let type_metadata = reflection_return_type_metadata(signature);
    let (attr_names, attr_args) =
        reflection_builtin_function_attributes(function_name, signature);
    let is_deprecated = signature.deprecation.is_some();
    let declaring_function = ReflectionDeclaringFunctionMember::Function {
        name: function_name.to_string(),
        attr_names: attr_names.clone(),
        attr_args: attr_args.clone(),
        required_parameter_count,
        type_metadata: type_metadata.clone(),
        is_deprecated,
        is_generator: false,
    };
    let mut metadata = empty_reflection_metadata();
    metadata.reflected_name = Some(function_name.to_string());
    metadata.attr_names = attr_names;
    metadata.attr_args = attr_args;
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
    metadata.is_deprecated = is_deprecated;
    Ok(metadata)
}

/// Builds Reflection attribute metadata for deprecated internal date functions.
fn reflection_builtin_function_attributes(
    function_name: &str,
    signature: &FunctionSig,
) -> (Vec<String>, Vec<Option<Vec<AttrArgEntry>>>) {
    let Some(message) = signature.deprecation.as_ref() else {
        return (Vec::new(), Vec::new());
    };
    let since = match function_name {
        "strptime" => "8.2",
        "strftime" | "gmstrftime" | "date_sunrise" | "date_sunset" => "8.1",
        _ => "",
    };
    let mut args = Vec::new();
    if !since.is_empty() {
        args.push(AttrArgEntry {
            key: Some(AttrKey::Str("since".to_string())),
            value: AttrArgValue::Str(since.to_string()),
        });
    }
    if !message.is_empty() {
        args.push(AttrArgEntry {
            key: Some(AttrKey::Str("message".to_string())),
            value: AttrArgValue::Str(message.clone()),
        });
    }
    (vec!["Deprecated".to_string()], vec![Some(args)])
}

/// Returns the canonical callable-builtin name and signature for ReflectionFunction.
pub(super) fn reflection_builtin_function_signature(function_name: &str) -> Option<(String, FunctionSig)> {
    let builtin_key = php_symbol_key(function_name.trim_start_matches('\\'));
    crate::types::reflection_builtin_function_sig(&builtin_key)
        .map(|signature| (builtin_key, signature))
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

/// Returns whether php-src exposes a date/time method's declared type as tentative.
///
/// PHP's ext/date stubs retain tentative return types on legacy methods for inheritance
/// compatibility. Newer methods and serialization hooks use ordinary declared return types.
pub(super) fn reflection_datetime_method_has_tentative_return_type(
    declaring_class_name: Option<&str>,
    method_name: Option<&str>,
) -> bool {
    let Some(class_name) = declaring_class_name else {
        return false;
    };
    let Some(method_key) = method_name.map(php_symbol_key) else {
        return false;
    };
    match class_name.trim_start_matches('\\') {
        "DateTimeInterface" => !matches!(
            method_key.as_str(),
            "getmicrosecond" | "__serialize" | "__unserialize"
        ),
        "DateTime" | "DateTimeImmutable" => !matches!(
            method_key.as_str(),
            "__construct"
                | "__serialize"
                | "__unserialize"
                | "createfrominterface"
                | "getmicrosecond"
                | "setmicrosecond"
        ),
        "DateTimeZone" | "DateInterval" => !matches!(
            method_key.as_str(),
            "__construct" | "__serialize" | "__unserialize"
        ),
        "DatePeriod" => !matches!(
            method_key.as_str(),
            "__construct"
                | "createfromiso8601string"
                | "__serialize"
                | "__unserialize"
                | "getiterator"
        ),
        _ => false,
    }
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
    let reflected_name = member
        .declaring_class_name
        .as_deref()
        .and_then(|class_name| {
            crate::types::php_src_date_method_canonical_name(class_name, method_name)
        })
        .unwrap_or(method_name)
        .to_string();
    ReflectionOwnerMetadata {
        reflected_name: Some(reflected_name),
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
