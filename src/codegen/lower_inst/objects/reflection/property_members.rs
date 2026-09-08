//! Purpose:
//! ReflectionProperty members, hooks, defaults, and string formatting.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::reflection`.
//!
//! Key details:
//! - Preserves compile-time metadata, target-aware object layout, and ownership.

use super::*;

/// Builds ReflectionProperty array entries for the properties visible on one class.
pub(super) fn reflection_class_property_members(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    info: &crate::types::ClassInfo,
    property_names: &[String],
) -> Vec<ReflectionListedMember> {
    property_names
        .iter()
        .filter_map(|property_name| {
            reflection_class_property_member(ctx, class_name, info, property_name)
        })
        .collect()
}

/// Builds one ReflectionProperty array entry from class or enum metadata.
pub(super) fn reflection_class_property_member(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    info: &crate::types::ClassInfo,
    property_name: &str,
) -> Option<ReflectionListedMember> {
    let flags = reflection_property_member_flags(info, property_name).or_else(|| {
        (is_reflection_enum(ctx, class_name) && property_name == "name").then_some(
            reflection_member_flags(false, &Visibility::Public, false, false, true, false),
        )
    })?;
    let type_metadata = reflection_property_type_metadata(info, property_name);
    let default_value = reflection_property_default_value(info, property_name);
    let declaring_class_name = reflection_property_declaring_class_name(info, property_name)
        .or_else(|| {
            (is_reflection_enum(ctx, class_name) && property_name == "name")
                .then(|| class_name.to_string())
        });
    let property_hook_members = if class_name == "DatePeriod" {
        // php-src implements DatePeriod's seven public virtual properties through
        // object handlers rather than PHP property hooks. Elephc uses hidden
        // synthetic getters for code generation, but Reflection must expose no hooks.
        Vec::new()
    } else {
        reflection_property_hook_members(
            info,
            property_name,
            declaring_class_name.as_deref(),
            flags,
            type_metadata.as_ref(),
        )
    };
    Some(ReflectionListedMember {
        name: property_name.to_string(),
        declaring_class_name,
        attr_names: info
            .property_attribute_names
            .get(property_name)
            .cloned()
            .unwrap_or_default(),
        attr_args: info
            .property_attribute_args
            .get(property_name)
            .cloned()
            .unwrap_or_default(),
        constant_value: None,
        backing_value: None,
        is_enum_case: false,
        flags,
        modifiers: reflection_property_modifiers_for_info(info, property_name)
            .unwrap_or_else(|| reflection_property_modifiers_from_flags(flags)),
        type_metadata,
        default_value,
        property_hook_members,
        required_parameter_count: 0,
        is_deprecated: false,
        is_generator: false,
        prototype_member: None,
        parameters: Vec::new(),
    })
}

/// Returns reflection type metadata for one typed property visible on a class.
pub(super) fn reflection_property_type_metadata(
    info: &crate::types::ClassInfo,
    property_name: &str,
) -> Option<ReflectionParameterTypeMetadata> {
    if info.visible_property_is_declared(property_name) {
        let (_, (_, property_type)) = info.visible_property(property_name)?;
        return reflection_parameter_type_metadata(None, property_type);
    }
    if !info.declared_static_properties.contains(property_name) {
        return None;
    }
    let (_, property_type) = info
        .static_properties
        .iter()
        .find(|(name, _)| name == property_name)?;
    reflection_parameter_type_metadata(None, property_type)
}

/// Builds concrete or abstract property-hook ReflectionMethod metadata for one property.
pub(super) fn reflection_property_hook_members(
    info: &crate::types::ClassInfo,
    property_name: &str,
    declaring_class_name: Option<&str>,
    property_flags: ReflectionMemberFlags,
    property_type_metadata: Option<&ReflectionParameterTypeMetadata>,
) -> Vec<(String, ReflectionListedMember)> {
    let mut members = Vec::new();
    let declaring_class_name = declaring_class_name.map(str::to_string).or_else(|| {
        info.abstract_property_hooks
            .get(property_name)
            .map(|contract| contract.declaring_type.clone())
    });
    let has_concrete_get = info
        .methods
        .contains_key(&php_symbol_key(&property_hook_get_method(property_name)));
    let has_concrete_set = info
        .methods
        .contains_key(&php_symbol_key(&property_hook_set_method(property_name)));
    let contract = info.abstract_property_hooks.get(property_name);
    if has_concrete_get || contract.and_then(|contract| contract.get_type.as_ref()).is_some() {
        let return_type = contract
            .and_then(|contract| contract.get_type.as_ref())
            .and_then(|ty| reflection_parameter_type_metadata(None, ty))
            .or_else(|| property_type_metadata.cloned());
        members.push((
            String::from("get"),
            reflection_property_hook_method_member(
                property_name,
                "get",
                declaring_class_name.clone(),
                property_flags,
                !has_concrete_get,
                return_type,
                None,
            ),
        ));
    }
    if has_concrete_set || contract.and_then(|contract| contract.set_type.as_ref()).is_some() {
        let parameter_type = contract
            .and_then(|contract| contract.set_type.as_ref())
            .and_then(|ty| reflection_parameter_type_metadata(None, ty))
            .or_else(|| property_type_metadata.cloned());
        members.push((
            String::from("set"),
            reflection_property_hook_method_member(
                property_name,
                "set",
                declaring_class_name,
                property_flags,
                !has_concrete_set,
                Some(ReflectionParameterTypeMetadata::Named(
                    reflection_builtin_named_type("void", false),
                )),
                parameter_type,
            ),
        ));
    }
    members
}

/// Builds one ReflectionMethod metadata record for a property hook.
pub(super) fn reflection_property_hook_method_member(
    property_name: &str,
    hook_name: &str,
    declaring_class_name: Option<String>,
    property_flags: ReflectionMemberFlags,
    is_abstract: bool,
    return_type: Option<ReflectionParameterTypeMetadata>,
    parameter_type: Option<ReflectionParameterTypeMetadata>,
) -> ReflectionListedMember {
    let visibility = reflection_visibility_from_member_flags(property_flags);
    let flags = reflection_member_flags(false, &visibility, false, is_abstract, false, false);
    let name = format!("${property_name}::{hook_name}");
    let required_parameter_count = i64::from(hook_name == "set");
    let declaring_function = ReflectionDeclaringFunctionMember::Method {
        name: name.clone(),
        declaring_class_name: declaring_class_name.clone(),
        attr_names: Vec::new(),
        attr_args: Vec::new(),
        flags,
        required_parameter_count,
        type_metadata: return_type.clone(),
        is_deprecated: false,
        is_generator: false,
    };
    let parameters = if hook_name == "set" {
        vec![reflection_property_hook_parameter_member(
            declaring_class_name.clone(),
            declaring_function.clone(),
            parameter_type,
        )]
    } else {
        Vec::new()
    };
    ReflectionListedMember {
        name,
        declaring_class_name,
        attr_names: Vec::new(),
        attr_args: Vec::new(),
        constant_value: None,
        backing_value: None,
        is_enum_case: false,
        flags,
        modifiers: reflection_method_modifiers_from_flags(flags),
        type_metadata: return_type,
        default_value: None,
        property_hook_members: Vec::new(),
        required_parameter_count,
        is_deprecated: false,
        is_generator: false,
        prototype_member: None,
        parameters,
    }
}

/// Builds the synthetic `value` parameter exposed by set-hook ReflectionMethod objects.
pub(super) fn reflection_property_hook_parameter_member(
    declaring_class_name: Option<String>,
    declaring_function: ReflectionDeclaringFunctionMember,
    type_metadata: Option<ReflectionParameterTypeMetadata>,
) -> ReflectionParameterMember {
    let has_type = type_metadata.is_some();
    let allows_null = type_metadata.as_ref().is_some_and(reflection_type_allows_null);
    let is_array_type = reflection_parameter_has_named_type(type_metadata.as_ref(), "array");
    let is_callable_type = reflection_parameter_has_named_type(type_metadata.as_ref(), "callable");
    ReflectionParameterMember {
        name: String::from("value"),
        declaring_class_name,
        declaring_function: Some(declaring_function),
        attr_names: Vec::new(),
        attr_args: Vec::new(),
        position: 0,
        is_optional: false,
        is_variadic: false,
        is_passed_by_reference: false,
        is_promoted: false,
        has_type,
        allows_null,
        is_array_type,
        is_callable_type,
        type_metadata,
        default_value: None,
        default_value_constant_name: None,
    }
}

/// Returns supported default metadata for one reflected property.
pub(super) fn reflection_property_default_value(
    info: &crate::types::ClassInfo,
    property_name: &str,
) -> Option<ReflectionParameterDefaultValue> {
    if let Some((index, (name, _))) = info.visible_property(property_name) {
        return reflection_property_slot_default_value(
            info.property_slot_is_declared(index, name),
            info.defaults.get(index).and_then(Option::as_ref),
        );
    }
    info.static_properties
        .iter()
        .position(|(name, _)| name == property_name)
        .and_then(|index| {
            reflection_property_slot_default_value(
                info.declared_static_properties.contains(property_name),
                info.static_defaults.get(index).and_then(Option::as_ref),
            )
        })
}

/// Converts one physical property slot default into PHP Reflection metadata.
pub(super) fn reflection_property_slot_default_value(
    is_declared: bool,
    default: Option<&Expr>,
) -> Option<ReflectionParameterDefaultValue> {
    match default {
        Some(default) => reflection_literal_parameter_default_value(default),
        None if !is_declared => Some(ReflectionParameterDefaultValue::Null),
        None => None,
    }
}

/// Formats retained generated property metadata for `ReflectionProperty::__toString()`.
pub(super) fn reflection_property_to_string(
    property_name: &str,
    flags: ReflectionMemberFlags,
    type_metadata: Option<&ReflectionParameterTypeMetadata>,
    default_value: Option<&ReflectionParameterDefaultValue>,
) -> String {
    let mut parts = Vec::new();
    if flags.is_abstract {
        parts.push(String::from("abstract"));
    }
    if flags.is_final {
        parts.push(String::from("final"));
    }
    parts.push(reflection_property_visibility_label(flags).to_string());
    if flags.is_static {
        parts.push(String::from("static"));
    }
    if flags.is_readonly {
        parts.push(String::from("readonly"));
    }
    if let Some(type_name) = type_metadata.map(reflection_type_metadata_to_string) {
        parts.push(type_name);
    }
    parts.push(format!("${property_name}"));

    let default = if flags.is_virtual {
        String::new()
    } else {
        default_value
            .and_then(reflection_default_value_to_string)
            .map(|value| format!(" = {value}"))
            .unwrap_or_default()
    };
    format!("Property [ {}{} ]", parts.join(" "), default)
}

/// Returns PHP's lowercase visibility label for one reflected property.
pub(super) fn reflection_property_visibility_label(flags: ReflectionMemberFlags) -> &'static str {
    if flags.is_private {
        "private"
    } else if flags.is_protected {
        "protected"
    } else {
        "public"
    }
}

/// Formats retained ReflectionType metadata for property string output.
pub(super) fn reflection_type_metadata_to_string(type_metadata: &ReflectionParameterTypeMetadata) -> String {
    match type_metadata {
        ReflectionParameterTypeMetadata::Named(named) => {
            if named.allows_null && named.name != "mixed" {
                format!("?{}", named.name)
            } else {
                named.name.clone()
            }
        }
        ReflectionParameterTypeMetadata::Union(union) => {
            let mut names = union
                .types
                .iter()
                .map(|type_metadata| type_metadata.name.clone())
                .collect::<Vec<_>>();
            if union.allows_null && names.iter().all(|name| name != "null") {
                names.push(String::from("null"));
            }
            names.join("|")
        }
        ReflectionParameterTypeMetadata::Intersection(intersection) => intersection
            .types
            .iter()
            .map(|type_metadata| type_metadata.name.clone())
            .collect::<Vec<_>>()
            .join("&"),
    }
}

/// Formats retained scalar defaults for property string output.
pub(super) fn reflection_default_value_to_string(
    default: &ReflectionParameterDefaultValue,
) -> Option<String> {
    match default {
        ReflectionParameterDefaultValue::Int(value) => Some(value.to_string()),
        ReflectionParameterDefaultValue::Bool(value) => Some(value.to_string()),
        ReflectionParameterDefaultValue::Float(value) => Some(value.to_string()),
        ReflectionParameterDefaultValue::Str(value) => Some(format!("'{value}'")),
        ReflectionParameterDefaultValue::Null => Some(String::from("NULL")),
        ReflectionParameterDefaultValue::Object { .. }
        | ReflectionParameterDefaultValue::Array(_)
        | ReflectionParameterDefaultValue::AssocArray(_) => None,
    }
}
