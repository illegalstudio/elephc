//! Purpose:
//! Formats eval-backed Reflection owners using PHP-compatible string layouts.
//! This module owns presentation only; metadata lookup remains in the parent.
//!
//! Called from:
//! - `crate::interpreter::reflection` for Reflection `__toString()` methods.
//!
//! Key details:
//! - Class, callable, property, constant, parameter, and type formatting share
//!   the same visibility, modifier, and default-value conventions.

use super::*;

/// Formats one reflected class-like symbol similarly to PHP's `__toString()` output.
pub(super) fn eval_reflection_class_to_string(
    reflected_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let metadata = eval_reflection_class_to_string_metadata(reflected_name, context, values)?
        .ok_or(EvalStatus::RuntimeFatal)?;
    let constant_lines =
        eval_reflection_class_constant_string_lines(&metadata.resolved_name, context, values)?;
    let property_lines =
        eval_reflection_class_property_string_lines(&metadata, context, values)?;
    let method_lines = eval_reflection_class_method_string_lines(&metadata, context, values)?;

    let mut rendered = format!("{} {{\n", eval_reflection_class_to_string_header(&metadata));
    eval_reflection_class_append_string_section(&mut rendered, "Constants", &constant_lines);
    eval_reflection_class_append_string_section(&mut rendered, "Properties", &property_lines);
    eval_reflection_class_append_string_section(&mut rendered, "Methods", &method_lines);
    rendered.push_str("}\n");
    Ok(rendered)
}

/// Returns eval or AOT class metadata for a ReflectionClass string dump.
pub(super) fn eval_reflection_class_to_string_metadata(
    reflected_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalReflectionClassMetadata>, EvalStatus> {
    if let Some(mut metadata) = eval_reflection_class_like_attributes(reflected_name, context) {
        metadata.interface_names =
            eval_reflection_eval_metadata_interface_names(&metadata, context, values)?;
        metadata.flags = eval_reflection_eval_metadata_flags(&metadata, context, values)?;
        return Ok(Some(metadata));
    }
    let runtime_class_name = reflected_name.trim_start_matches('\\');
    let Some((flags, modifiers)) = eval_reflection_aot_class_flags(runtime_class_name, values)?
    else {
        return Ok(None);
    };
    let method_names = eval_reflection_aot_member_names(
        EVAL_REFLECTION_OWNER_METHOD,
        runtime_class_name,
        values,
    )?;
    let native_interface_property_names =
        eval_reflection_native_interface_property_names(runtime_class_name, context);
    let property_names = if native_interface_property_names.is_empty() {
        eval_reflection_aot_member_names(
            EVAL_REFLECTION_OWNER_PROPERTY,
            runtime_class_name,
            values,
        )?
    } else {
        native_interface_property_names
    };
    let interface_names = eval_reflection_aot_class_interface_names(runtime_class_name, values)?;
    let trait_names = eval_reflection_aot_class_trait_names(runtime_class_name, values)?;
    let parent_class_name = eval_reflection_aot_parent_class_name(runtime_class_name, values)?;
    Ok(Some(EvalReflectionClassMetadata {
        resolved_name: runtime_class_name.to_string(),
        source_location: None,
        attributes: context.native_class_attributes(runtime_class_name),
        flags,
        modifiers,
        interface_names,
        trait_names,
        method_names,
        property_names,
        parent_class_name,
    }))
}

/// Returns the PHP-like header line for `ReflectionClass::__toString()`.
pub(super) fn eval_reflection_class_to_string_header(metadata: &EvalReflectionClassMetadata) -> String {
    let origin = if metadata.flags & EVAL_REFLECTION_CLASS_FLAG_INTERNAL != 0 {
        "<internal>"
    } else {
        "<user>"
    };
    let kind = eval_reflection_class_to_string_kind(metadata.flags);
    let mut parts = Vec::new();
    if metadata.flags & EVAL_REFLECTION_CLASS_FLAG_INTERFACE != 0 {
        parts.push(String::from("interface"));
        parts.push(metadata.resolved_name.clone());
        if !metadata.interface_names.is_empty() {
            parts.push(String::from("extends"));
            parts.push(metadata.interface_names.join(", "));
        }
    } else if metadata.flags & EVAL_REFLECTION_CLASS_FLAG_TRAIT != 0 {
        parts.push(String::from("trait"));
        parts.push(metadata.resolved_name.clone());
    } else {
        if metadata.flags & EVAL_REFLECTION_CLASS_FLAG_ABSTRACT != 0 {
            parts.push(String::from("abstract"));
        }
        if metadata.flags & EVAL_REFLECTION_CLASS_FLAG_FINAL != 0 {
            parts.push(String::from("final"));
        }
        if metadata.flags & EVAL_REFLECTION_CLASS_FLAG_READONLY != 0 {
            parts.push(String::from("readonly"));
        }
        parts.push(String::from("class"));
        parts.push(metadata.resolved_name.clone());
        if let Some(parent_class_name) = metadata.parent_class_name.as_ref() {
            parts.push(String::from("extends"));
            parts.push(parent_class_name.clone());
        }
        if !metadata.interface_names.is_empty() {
            parts.push(String::from("implements"));
            parts.push(metadata.interface_names.join(", "));
        }
    }
    format!("{kind} [ {origin} {} ]", parts.join(" "))
}

/// Returns the ReflectionClass string header owner kind label.
pub(super) fn eval_reflection_class_to_string_kind(flags: u64) -> &'static str {
    if flags & EVAL_REFLECTION_CLASS_FLAG_INTERFACE != 0 {
        "Interface"
    } else if flags & EVAL_REFLECTION_CLASS_FLAG_TRAIT != 0 {
        "Trait"
    } else {
        "Class"
    }
}

/// Formats all constants visible to `ReflectionClass::__toString()`.
pub(super) fn eval_reflection_class_constant_string_lines(
    reflected_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    let constant_names = eval_reflection_constant_names(reflected_name, context, values)?;
    let mut lines = Vec::with_capacity(constant_names.len());
    for constant_name in constant_names {
        let line = eval_reflection_class_constant_to_string(
            reflected_name,
            &constant_name,
            EVAL_REFLECTION_OWNER_CLASS_CONSTANT,
            context,
            values,
        )?;
        lines.push(line.trim_end_matches('\n').to_string());
    }
    Ok(lines)
}

/// Formats all properties visible to `ReflectionClass::__toString()`.
pub(super) fn eval_reflection_class_property_string_lines(
    metadata: &EvalReflectionClassMetadata,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    let mut lines = Vec::with_capacity(metadata.property_names.len());
    for property_name in &metadata.property_names {
        let Some(member) = eval_reflection_reflected_property_metadata(
            &metadata.resolved_name,
            property_name,
            context,
            values,
        )?
        else {
            continue;
        };
        lines.push(eval_reflection_property_to_string(property_name, &member));
    }
    Ok(lines)
}

/// Formats all methods visible to `ReflectionClass::__toString()`.
pub(super) fn eval_reflection_class_method_string_lines(
    metadata: &EvalReflectionClassMetadata,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    let mut lines = Vec::with_capacity(metadata.method_names.len());
    for method_name in &metadata.method_names {
        let member =
            if let Some(member) =
                eval_reflection_method_metadata(&metadata.resolved_name, method_name, context)
            {
                Some(member)
            } else {
                eval_reflection_aot_method_metadata_with_signature_if_exists(
                    &metadata.resolved_name,
                    method_name,
                    context,
                    values,
                )?
            };
        let Some(member) = member else {
            continue;
        };
        lines.push(eval_reflection_method_summary_to_string(method_name, &member));
    }
    Ok(lines)
}

/// Appends one named section to a ReflectionClass string dump.
pub(super) fn eval_reflection_class_append_string_section(
    rendered: &mut String,
    label: &str,
    lines: &[String],
) {
    rendered.push_str(&format!("  - {label} [{}] {{\n", lines.len()));
    for line in lines {
        rendered.push_str("    ");
        rendered.push_str(line);
        rendered.push('\n');
    }
    rendered.push_str("  }\n");
}

/// Formats one reflected method line for `ReflectionClass::__toString()`.
pub(super) fn eval_reflection_method_summary_to_string(
    method_name: &str,
    member: &EvalReflectionMemberMetadata,
) -> String {
    let mut parts = Vec::new();
    if member.is_abstract {
        parts.push(String::from("abstract"));
    }
    if member.is_final {
        parts.push(String::from("final"));
    }
    if member.is_static {
        parts.push(String::from("static"));
    }
    parts.push(eval_reflection_visibility_label(member.visibility).to_string());
    parts.push(String::from("method"));
    parts.push(method_name.to_string());
    format!("Method [ <user> {} ]", parts.join(" "))
}

/// Formats one reflected function or method similarly to PHP's `__toString()` output.
pub(super) fn eval_reflection_function_method_to_string(
    target: &EvalReflectionFunctionMethodTarget,
) -> String {
    let mut rendered = format!(
        "{} {{\n  - Parameters [{}] {{\n",
        eval_reflection_function_method_header(target),
        eval_reflection_function_method_parameters(target).len()
    );
    for parameter in eval_reflection_function_method_parameters(target) {
        rendered.push_str("    ");
        rendered.push_str(&eval_reflection_function_method_parameter_to_string(parameter));
        rendered.push('\n');
    }
    rendered.push_str("  }\n");
    if let Some(return_type) = eval_reflection_function_method_return_type(target) {
        rendered.push_str("  - Return [ ");
        rendered.push_str(&eval_reflection_type_metadata_to_string(return_type));
        rendered.push_str(" ]\n");
    }
    rendered.push_str("}\n");
    rendered
}

/// Returns the PHP-like header line for a reflected function or method.
pub(super) fn eval_reflection_function_method_header(
    target: &EvalReflectionFunctionMethodTarget,
) -> String {
    match target {
        EvalReflectionFunctionMethodTarget::Function { name, .. } => {
            format!(
                "Function [ <user> function {} ]",
                name.trim_start_matches('\\')
            )
        }
        EvalReflectionFunctionMethodTarget::Method {
            name,
            visibility,
            is_static,
            is_final,
            is_abstract,
            ..
        } => {
            let mut parts = Vec::new();
            if *is_abstract {
                parts.push(String::from("abstract"));
            }
            if *is_final {
                parts.push(String::from("final"));
            }
            if *is_static {
                parts.push(String::from("static"));
            }
            if let Some(visibility) = visibility {
                parts.push(eval_reflection_visibility_label(*visibility).to_string());
            }
            parts.push(String::from("method"));
            parts.push(name.clone());
            format!("Method [ <user> {} ]", parts.join(" "))
        }
    }
}

/// Returns retained parameters for a reflected function or method target.
pub(super) fn eval_reflection_function_method_parameters(
    target: &EvalReflectionFunctionMethodTarget,
) -> &[EvalReflectionParameterMetadata] {
    match target {
        EvalReflectionFunctionMethodTarget::Function { parameters, .. }
        | EvalReflectionFunctionMethodTarget::Method { parameters, .. } => parameters,
    }
}

/// Formats one parameter line for function-like `__toString()` output.
pub(super) fn eval_reflection_function_method_parameter_to_string(
    parameter: &EvalReflectionParameterMetadata,
) -> String {
    let requiredness = if parameter.is_optional {
        "optional"
    } else {
        "required"
    };
    let mut signature_parts = Vec::new();
    if let Some(type_metadata) = parameter.type_metadata.as_ref() {
        signature_parts.push(eval_reflection_type_metadata_to_string(type_metadata));
    }
    let mut variable = String::new();
    if parameter.is_passed_by_reference {
        variable.push('&');
    }
    if parameter.is_variadic {
        variable.push_str("...");
    }
    variable.push('$');
    variable.push_str(&parameter.name);
    signature_parts.push(variable);
    let default = parameter
        .default_value
        .as_ref()
        .and_then(eval_reflection_default_expr_to_string)
        .map(|value| format!(" = {value}"))
        .unwrap_or_default();
    format!(
        "Parameter #{} [ <{}> {}{} ]",
        parameter.position,
        requiredness,
        signature_parts.join(" "),
        default
    )
}

/// Formats one reflected property similarly to PHP's `ReflectionProperty::__toString()`.
pub(super) fn eval_reflection_property_to_string(
    property_name: &str,
    member: &EvalReflectionMemberMetadata,
) -> String {
    if member.is_dynamic {
        return format!("Property [ <dynamic> public ${property_name} ]\n");
    }
    let mut parts = Vec::new();
    if member.is_abstract {
        parts.push(String::from("abstract"));
    }
    if member.is_final {
        parts.push(String::from("final"));
    }
    parts.push(eval_reflection_visibility_label(member.visibility).to_string());
    if member.is_static {
        parts.push(String::from("static"));
    }
    if member.is_readonly {
        parts.push(String::from("readonly"));
    }
    if let Some(type_name) = member
        .type_metadata
        .as_ref()
        .map(eval_reflection_type_metadata_to_string)
    {
        parts.push(type_name);
    }
    parts.push(format!("${property_name}"));

    let default = if member.modifiers & 512 != 0 {
        String::new()
    } else {
        member
            .default_value
            .as_ref()
            .and_then(eval_reflection_default_expr_to_string)
            .map(|value| format!(" = {value}"))
            .unwrap_or_default()
    };
    format!("Property [ {}{} ]", parts.join(" "), default)
}

/// Formats one class constant or enum case like PHP's `ReflectionClassConstant::__toString()`.
pub(super) fn eval_reflection_class_constant_to_string(
    declaring_class: &str,
    constant_name: &str,
    owner_kind: u64,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let (_, _, visibility, is_final, is_enum_case) =
        eval_reflection_class_constant_metadata(declaring_class, constant_name, context, values)?
            .ok_or(EvalStatus::RuntimeFatal)?;
    let value = eval_reflection_constant_value(declaring_class, constant_name, context, values)?
        .ok_or(EvalStatus::RuntimeFatal)?;
    let mut parts = Vec::new();
    if is_final {
        parts.push(String::from("final"));
    }
    parts.push(eval_reflection_visibility_label(visibility).to_string());
    parts.push(eval_reflection_class_constant_type_label(
        declaring_class,
        value,
        is_enum_case
            || matches!(
                owner_kind,
                EVAL_REFLECTION_OWNER_ENUM_UNIT_CASE | EVAL_REFLECTION_OWNER_ENUM_BACKED_CASE
            ),
        values,
    )?);
    parts.push(constant_name.to_string());
    let value = eval_reflection_class_constant_display_value(value, values)?;
    Ok(format!("Constant [ {} ] {{ {} }}\n", parts.join(" "), value))
}

/// Returns the type label PHP prints for a reflected class constant value.
pub(super) fn eval_reflection_class_constant_type_label(
    declaring_class: &str,
    value: RuntimeCellHandle,
    is_enum_case: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    if is_enum_case {
        return Ok(declaring_class.trim_start_matches('\\').to_string());
    }
    Ok(match values.type_tag(value)? {
        EVAL_TAG_INT => String::from("int"),
        EVAL_TAG_STRING => String::from("string"),
        EVAL_TAG_FLOAT => String::from("float"),
        EVAL_TAG_BOOL => String::from("bool"),
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => String::from("array"),
        EVAL_TAG_OBJECT => String::from("object"),
        EVAL_TAG_NULL => String::from("null"),
        _ => String::from("mixed"),
    })
}

/// Returns the value display PHP prints inside ReflectionClassConstant braces.
pub(super) fn eval_reflection_class_constant_display_value(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    Ok(match values.type_tag(value)? {
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => String::from("Array"),
        EVAL_TAG_OBJECT => String::from("Object"),
        EVAL_TAG_NULL => String::new(),
        _ => String::from_utf8_lossy(&values.string_bytes(value)?).into_owned(),
    })
}

/// Returns PHP's lowercase label for one reflected visibility.
pub(super) fn eval_reflection_visibility_label(visibility: EvalVisibility) -> &'static str {
    match visibility {
        EvalVisibility::Public => "public",
        EvalVisibility::Protected => "protected",
        EvalVisibility::Private => "private",
    }
}

/// Formats retained ReflectionType metadata for `ReflectionProperty::__toString()`.
pub(super) fn eval_reflection_type_metadata_to_string(
    type_metadata: &EvalReflectionParameterTypeMetadata,
) -> String {
    match &type_metadata.kind {
        EvalReflectionParameterTypeKind::Named(named) => {
            if named.allows_null && named.name != "mixed" {
                format!("?{}", named.name)
            } else {
                named.name.clone()
            }
        }
        EvalReflectionParameterTypeKind::Union(union) => {
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
        EvalReflectionParameterTypeKind::Intersection(intersection) => intersection
            .types
            .iter()
            .map(|type_metadata| type_metadata.name.clone())
            .collect::<Vec<_>>()
            .join("&"),
    }
}

/// Formats retained literal defaults for `ReflectionProperty::__toString()`.
pub(super) fn eval_reflection_default_expr_to_string(default: &EvalExpr) -> Option<String> {
    match default {
        EvalExpr::Const(EvalConst::Null) => Some(String::from("NULL")),
        EvalExpr::Const(EvalConst::Bool(value)) => Some(value.to_string()),
        EvalExpr::Const(EvalConst::Int(value)) => Some(value.to_string()),
        EvalExpr::Const(EvalConst::Float(value)) => Some(value.to_string()),
        EvalExpr::Const(EvalConst::String(value)) => Some(format!("'{value}'")),
        EvalExpr::Unary {
            op: EvalUnaryOp::Plus,
            expr,
        } => eval_reflection_default_expr_to_string(expr),
        EvalExpr::Unary {
            op: EvalUnaryOp::Negate,
            expr,
        } => eval_reflection_default_expr_to_string(expr).map(|value| format!("-{value}")),
        EvalExpr::ConstFetch(name) => Some(name.clone()),
        EvalExpr::NamespacedConstFetch { name, .. } => Some(name.clone()),
        EvalExpr::ClassConstantFetch {
            class_name,
            constant,
        } => Some(format!("{class_name}::{constant}")),
        _ => None,
    }
}

/// Returns whether eval retained this property as virtual rather than backed.
pub(super) fn eval_reflection_property_is_virtual(property: &EvalClassProperty) -> bool {
    property.is_virtual()
}

/// Computes PHP's `ReflectionMethod::getModifiers()` bitmask from eval member flags.
pub(super) fn eval_reflection_method_modifiers_from_flags(flags: u64) -> u64 {
    let mut modifiers = 0;
    if (flags & EVAL_REFLECTION_MEMBER_FLAG_PUBLIC) != 0 {
        modifiers |= 1;
    }
    if (flags & EVAL_REFLECTION_MEMBER_FLAG_PROTECTED) != 0 {
        modifiers |= 2;
    }
    if (flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE) != 0 {
        modifiers |= 4;
    }
    if (flags & EVAL_REFLECTION_MEMBER_FLAG_STATIC) != 0 {
        modifiers |= 16;
    }
    if (flags & EVAL_REFLECTION_MEMBER_FLAG_FINAL) != 0 {
        modifiers |= 32;
    }
    if (flags & EVAL_REFLECTION_MEMBER_FLAG_ABSTRACT) != 0 {
        modifiers |= 64;
    }
    modifiers
}
