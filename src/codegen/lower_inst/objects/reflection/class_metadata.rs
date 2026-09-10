//! Purpose:
//! ReflectionClass and ReflectionEnum metadata resolution.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::reflection`.
//!
//! Key details:
//! - Preserves compile-time metadata, target-aware object layout, and ownership.

use super::*;

/// Resolves Reflection constructor operands to captured class/member metadata.
pub(super) fn reflection_owner_metadata(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    inst: &Instruction,
) -> Result<ReflectionOwnerMetadata> {
    match class_name {
        "ReflectionClass" => reflection_class_metadata(ctx, inst),
        "ReflectionEnum" => reflection_enum_metadata(ctx, inst),
        "ReflectionExtension" => reflection_extension_metadata(ctx, inst),
        "ReflectionFunction" => reflection_function_metadata(ctx, inst),
        "ReflectionMethod" => reflection_method_metadata(ctx, inst),
        "ReflectionProperty" => reflection_property_metadata(ctx, inst),
        "ReflectionParameter" => reflection_parameter_metadata(ctx, inst),
        "ReflectionClassConstant" => reflection_class_constant_metadata(ctx, inst),
        "ReflectionEnumUnitCase" | "ReflectionEnumBackedCase" => {
            reflection_enum_case_metadata(ctx, class_name, inst)
        }
        _ => Ok(empty_reflection_metadata()),
    }
}

/// Resolves the frozen DOM bridge registry exposed through `ReflectionExtension`.
pub(super) fn reflection_extension_metadata(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<ReflectionOwnerMetadata> {
    let Some(value) = inst.operands.first().copied() else {
        return Ok(empty_reflection_metadata());
    };
    let name = const_required_string_operand(ctx, value, "ReflectionExtension")?;
    reflection_extension_metadata_for_name(&name)
}

/// Returns the PHP 8.5.8 DOM, libxml, or SimpleXML class-name registry.
pub(super) fn reflection_extension_metadata_for_name(name: &str) -> Result<ReflectionOwnerMetadata> {
    let (canonical, _legacy_classes): (&str, &[&str]) = match php_symbol_key(name).as_str() {
        "dom" => ("dom", &[
            "DOMAttr", "DOMCdataSection", "DOMCharacterData", "DOMChildNode", "DOMComment", "DOMDocument", "DOMDocumentFragment", "DOMDocumentType", "DOMElement", "DOMEntity", "DOMEntityReference", "DOMException", "DOMImplementation", "DOMNameSpaceNode", "DOMNamedNodeMap", "DOMNode", "DOMNodeList", "DOMNotation", "DOMParentNode", "DOMProcessingInstruction", "DOMText", "DOMXPath", "Dom\\AdjacentPosition", "Dom\\Attr", "Dom\\CDATASection", "Dom\\CharacterData", "Dom\\ChildNode", "Dom\\Comment", "Dom\\Document", "Dom\\DocumentFragment", "Dom\\DocumentType", "Dom\\DtdNamedNodeMap", "Dom\\Element", "Dom\\Entity", "Dom\\EntityReference", "Dom\\HTMLCollection", "Dom\\HTMLDocument", "Dom\\HTMLElement", "Dom\\Implementation", "Dom\\NamedNodeMap", "Dom\\NamespaceInfo", "Dom\\Node", "Dom\\NodeList", "Dom\\Notation", "Dom\\ParentNode", "Dom\\ProcessingInstruction", "Dom\\Text", "Dom\\TokenList", "Dom\\XMLDocument", "Dom\\XPath", "dom\\domexception",
        ]),
        "libxml" => ("libxml", &["LibXMLError"]),
        "simplexml" => ("SimpleXML", &["SimpleXMLElement", "SimpleXMLIterator"]),
        _ => return Ok(empty_reflection_metadata()),
    };
    let mut metadata = empty_reflection_metadata();
    metadata.reflected_name = Some(canonical.to_string());
    metadata.interface_names = crate::internal_extensions::registry()
        .extension(canonical)
        .ok_or_else(|| {
            CodegenIrError::unsupported(format!(
                "ReflectionExtension metadata for unknown extension {}",
                canonical
            ))
        })?
        .classes
        .iter()
        .map(|class| class.exported_name.clone())
        .collect();
    Ok(metadata)
}

/// Returns PHP 8.5.8's `ReflectionExtension::info()` module block for one DOM-family extension.
pub(super) fn reflection_extension_info(name: &str) -> Option<&'static str> {
    match php_symbol_key(name).as_str() {
        "dom" => Some(
            "\ndom\n\nDOM/XML => enabled\nDOM/XML API Version => 20031129\nlibxml Version => 2.15.3\nHTML Support => enabled\nXPath Support => enabled\nXPointer Support => enabled\nSchema Support => enabled\nRelaxNG Support => enabled\n",
        ),
        "libxml" => Some(
            "\nlibxml\n\nlibXML support => active\nlibXML Compiled Version => 2.15.3\nlibXML Loaded Version => 21503\nlibXML streams => enabled\n",
        ),
        "simplexml" => Some("\nSimpleXML\n\nSimpleXML support => enabled\nSchema support => enabled\n"),
        _ => None,
    }
}

/// Returns PHP 8.5.8's ordered DOM-family dependency maps for one extension.
pub(super) fn reflection_extension_dependencies(name: &str) -> Option<&'static [(&'static str, &'static str)]> {
    match php_symbol_key(name).as_str() {
        "dom" => Some(&[
            ("libxml", "Required"),
            ("lexbor", "Required"),
            ("domxml", "Conflicts"),
        ]),
        "libxml" => Some(&[("standard", "Required")]),
        "simplexml" => Some(&[("libxml", "Required"), ("spl", "Required")]),
        _ => None,
    }
}

/// Returns PHP 8.5.8's ordered, typed DOM-family extension constants.
pub(super) fn reflection_extension_constant_members(
    name: &str,
) -> Result<Vec<ReflectionConstantMember>> {
    let extension = crate::internal_extensions::registry()
        .extension(name)
        .ok_or_else(|| {
            CodegenIrError::unsupported(format!(
                "ReflectionExtension::getConstants for unknown extension {}",
                name
            ))
        })?;
    extension
        .constants
        .iter()
        .map(|(name, value)| {
            let value = match value {
                serde_json::Value::Number(value) => value
                    .as_i64()
                    .map(ReflectionConstantValue::Int)
                    .ok_or_else(|| {
                        CodegenIrError::unsupported(format!(
                            "ReflectionExtension::getConstants has non-integer numeric {}",
                            name
                        ))
                    })?,
                serde_json::Value::String(value) => ReflectionConstantValue::Str(value.clone()),
                _ => {
                    return Err(CodegenIrError::unsupported(format!(
                        "ReflectionExtension::getConstants has unsupported constant {}",
                        name
                    )));
                }
            };
            Ok(ReflectionConstantMember {
                name: name.clone(),
                value,
            })
        })
        .collect()
}

/// Maps the bounded DOM bridge classes to their PHP extension names.
pub(super) fn reflection_extension_name_for_class(class_name: &str) -> Option<&'static str> {
    if class_name.eq_ignore_ascii_case("LibXMLError") {
        Some("libxml")
    } else if class_name.eq_ignore_ascii_case("SimpleXMLElement")
        || class_name.eq_ignore_ascii_case("SimpleXMLIterator")
    {
        Some("SimpleXML")
    } else if class_name.starts_with("DOM") || class_name.starts_with("Dom\\") {
        Some("dom")
    } else {
        None
    }
}

/// Resolves `ReflectionClass(class)` metadata.
pub(super) fn reflection_class_metadata(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<ReflectionOwnerMetadata> {
    let Some(class_operand) = inst.operands.first().copied() else {
        return Ok(empty_reflection_metadata());
    };
    let reflected_class = const_string_or_class_operand(ctx, class_operand, "ReflectionClass")?;
    reflection_class_metadata_for_name(ctx, &reflected_class)
}

/// Resolves `ReflectionEnum(enum)` metadata for a known enum name.
pub(super) fn reflection_enum_metadata(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<ReflectionOwnerMetadata> {
    let Some(enum_operand) = inst.operands.first().copied() else {
        return Ok(empty_reflection_metadata());
    };
    let reflected_enum = const_string_or_class_operand(ctx, enum_operand, "ReflectionEnum")?;
    let mut metadata = reflection_class_metadata_for_name(ctx, &reflected_enum)?;
    let Some(enum_name) = metadata.reflected_name.as_deref() else {
        return Ok(empty_reflection_metadata());
    };
    let Some(enum_info) = ctx.module.enum_infos.get(enum_name) else {
        return Ok(empty_reflection_metadata());
    };
    metadata.type_metadata = enum_info
        .backing_type
        .as_ref()
        .and_then(reflection_named_type_metadata)
        .map(ReflectionParameterTypeMetadata::Named);
    Ok(metadata)
}

/// Resolves `ReflectionClass(name)` metadata for a known class-like name.
pub(super) fn reflection_class_metadata_for_name(
    ctx: &FunctionContext<'_>,
    reflected_class: &str,
) -> Result<ReflectionOwnerMetadata> {
    if let Some((class_name, info)) = resolve_reflection_class(ctx, &reflected_class) {
        let is_enum = is_reflection_enum(ctx, class_name);
        let method_names = reflection_class_method_names(ctx, class_name);
        let property_names = reflection_class_property_names(ctx, class_name, info);
        let constant_names = reflection_class_constant_names(ctx, class_name, info);
        let constant_members = reflection_class_constant_members(ctx, class_name, info)?;
        let default_property_members =
            reflection_class_default_property_members(info, &property_names);
        let static_property_members = reflection_class_static_property_members(class_name, info);
        let constant_reflection_members =
            reflection_class_constant_reflection_members(ctx, class_name, info)?;
        let enum_case_members = if is_enum {
            reflection_enum_case_members(ctx, class_name)
        } else {
            Vec::new()
        };
        let method_members = reflection_class_method_members(ctx, class_name, info, &method_names)?;
        let property_members =
            reflection_class_property_members(ctx, class_name, info, &property_names);
        let constructor_member = reflection_constructor_member(&method_members);
        let is_instantiable =
            reflection_class_is_instantiable(info, is_enum, constructor_member.as_ref());
        let is_cloneable = reflection_class_is_cloneable(class_name, info, is_enum);
        let is_iterable = reflection_class_is_iterable(info, is_enum);
        return Ok(ReflectionOwnerMetadata {
            reflected_name: Some(class_name.to_string()),
            attr_names: info.attribute_names.clone(),
            attr_args: info.attribute_args.clone(),
            interface_names: info.interfaces.clone(),
            trait_names: info.used_traits.clone(),
            trait_aliases: info.trait_aliases.clone(),
            parent_names: reflection_parent_class_names(ctx, info),
            method_names,
            property_names,
            constant_names,
            constant_members,
            default_property_members,
            static_property_members,
            constant_reflection_members,
            enum_case_members,
            method_members,
            property_members,
            property_hook_members: Vec::new(),
            constructor_member,
            parent_class_name: reflection_parent_class_name(ctx, info),
            constant_value: None,
            backing_value: None,
            is_enum_case: false,
            parameter_members: Vec::new(),
            type_metadata: None,
            property_default_value: None,
            required_parameter_count: 0,
            is_deprecated: false,
            is_generator: false,
            prototype_member: None,
            is_final: info.is_final,
            is_abstract: info.is_abstract,
            is_interface: false,
            is_trait: false,
            is_enum,
            is_readonly: info.is_readonly_class && !is_enum,
            is_anonymous: is_reflection_anonymous_class_name(class_name),
            is_instantiable,
            is_cloneable,
            is_iterable,
            modifiers: reflection_class_modifiers(
                info.is_final,
                info.is_abstract,
                info.is_readonly_class,
                is_enum,
            ),
            member_flags: ReflectionMemberFlags::default(),
        });
    }
    if let Some(interface_name) = resolve_reflection_interface(ctx, &reflected_class) {
        let method_names = reflection_interface_method_names(ctx, interface_name);
        let property_names = reflection_interface_property_names(ctx, interface_name);
        let constant_names = reflection_interface_constant_names(ctx, interface_name);
        let constant_members = reflection_interface_constant_members(ctx, interface_name)?;
        let constant_reflection_members =
            reflection_interface_constant_reflection_members(ctx, interface_name)?;
        let method_members = ctx
            .module
            .interface_infos
            .get(interface_name)
            .map(|info| {
                reflection_interface_method_members(ctx, info, interface_name, &method_names)
            })
            .transpose()?
            .unwrap_or_else(|| default_method_members(&method_names, true, interface_name));
        let property_members = default_property_members(&property_names, true, interface_name);
        let constructor_member = reflection_constructor_member(&method_members);
        return Ok(ReflectionOwnerMetadata {
            reflected_name: Some(interface_name.to_string()),
            attr_names: Vec::new(),
            attr_args: Vec::new(),
            interface_names: reflection_interface_parent_names(ctx, interface_name),
            trait_names: Vec::new(),
            trait_aliases: Vec::new(),
            parent_names: Vec::new(),
            method_names,
            property_names,
            constant_names,
            constant_members,
            default_property_members: Vec::new(),
            static_property_members: Vec::new(),
            constant_reflection_members,
            enum_case_members: Vec::new(),
            method_members,
            property_members,
            property_hook_members: Vec::new(),
            constructor_member,
            parent_class_name: None,
            constant_value: None,
            backing_value: None,
            is_enum_case: false,
            parameter_members: Vec::new(),
            type_metadata: None,
            property_default_value: None,
            required_parameter_count: 0,
            is_deprecated: false,
            is_generator: false,
            prototype_member: None,
            is_final: false,
            is_abstract: false,
            is_interface: true,
            is_trait: false,
            is_enum: false,
            is_readonly: false,
            is_anonymous: false,
            is_instantiable: false,
            is_cloneable: false,
            is_iterable: false,
            modifiers: 0,
            member_flags: reflection_member_flags(
                false,
                &Visibility::Public,
                false,
                false,
                false,
                false,
            ),
        });
    }
    if let Some(trait_name) = resolve_reflection_trait(ctx, &reflected_class) {
        let trait_names = ctx
            .module
            .declared_trait_uses
            .get(trait_name)
            .cloned()
            .unwrap_or_default();
        let method_names = reflection_trait_method_names(ctx, trait_name);
        let property_names = reflection_trait_property_names(ctx, trait_name);
        let constant_names = reflection_trait_constant_names(ctx, trait_name);
        let constant_members = reflection_trait_constant_members(ctx, trait_name)?;
        let constant_reflection_members =
            reflection_trait_constant_reflection_members(ctx, trait_name)?;
        let method_members = ctx
            .module
            .declared_trait_methods
            .get(trait_name)
            .map(|methods| reflection_trait_method_members(ctx, methods, trait_name, &method_names))
            .transpose()?
            .unwrap_or_else(|| default_method_members(&method_names, false, trait_name));
        let property_members = default_property_members(&property_names, false, trait_name);
        let constructor_member = reflection_constructor_member(&method_members);
        return Ok(ReflectionOwnerMetadata {
            reflected_name: Some(trait_name.to_string()),
            attr_names: Vec::new(),
            attr_args: Vec::new(),
            interface_names: Vec::new(),
            trait_names,
            trait_aliases: Vec::new(),
            parent_names: Vec::new(),
            method_names,
            property_names,
            constant_names,
            constant_members,
            default_property_members: Vec::new(),
            static_property_members: Vec::new(),
            constant_reflection_members,
            enum_case_members: Vec::new(),
            method_members,
            property_members,
            property_hook_members: Vec::new(),
            constructor_member,
            parent_class_name: None,
            constant_value: None,
            backing_value: None,
            is_enum_case: false,
            parameter_members: Vec::new(),
            type_metadata: None,
            property_default_value: None,
            required_parameter_count: 0,
            is_deprecated: false,
            is_generator: false,
            prototype_member: None,
            is_final: false,
            is_abstract: false,
            is_interface: false,
            is_trait: true,
            is_enum: false,
            is_readonly: false,
            is_anonymous: false,
            is_instantiable: false,
            is_cloneable: false,
            is_iterable: false,
            modifiers: 0,
            member_flags: reflection_member_flags(
                false,
                &Visibility::Public,
                false,
                false,
                false,
                false,
            ),
        });
    }
    Ok(empty_reflection_metadata())
}

/// Resolves class metadata for nested declaring-class slots without recursive member objects.
pub(super) fn reflection_shallow_class_metadata_for_name(
    ctx: &FunctionContext<'_>,
    reflected_class: &str,
) -> Result<ReflectionOwnerMetadata> {
    let mut metadata = reflection_class_metadata_for_name(ctx, reflected_class)?;
    metadata.method_names.clear();
    metadata.property_names.clear();
    metadata.constant_names.clear();
    metadata.constant_members.clear();
    metadata.constant_reflection_members.clear();
    metadata.enum_case_members.clear();
    metadata.method_members.clear();
    metadata.property_members.clear();
    metadata.constructor_member = None;
    metadata.parent_class_name = None;
    Ok(metadata)
}

/// Resolves `ReflectionEnum` metadata for nested enum-case slots.
pub(super) fn reflection_enum_metadata_for_name(
    ctx: &FunctionContext<'_>,
    reflected_enum: &str,
) -> Result<ReflectionOwnerMetadata> {
    let mut metadata = reflection_class_metadata_for_name(ctx, reflected_enum)?;
    let Some(enum_name) = metadata.reflected_name.as_deref() else {
        return Ok(empty_reflection_metadata());
    };
    let Some(enum_info) = ctx.module.enum_infos.get(enum_name) else {
        return Ok(empty_reflection_metadata());
    };
    metadata.type_metadata = enum_info
        .backing_type
        .as_ref()
        .and_then(reflection_named_type_metadata)
        .map(ReflectionParameterTypeMetadata::Named);
    metadata.method_names.clear();
    metadata.property_names.clear();
    metadata.constant_names.clear();
    metadata.constant_members.clear();
    metadata.constant_reflection_members.clear();
    metadata.enum_case_members.clear();
    metadata.method_members.clear();
    metadata.property_members.clear();
    metadata.constructor_member = None;
    metadata.parent_class_name = None;
    Ok(metadata)
}
