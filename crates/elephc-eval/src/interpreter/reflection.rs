//! Purpose:
//! Handles eval-aware construction of builtin reflection owner objects.
//! These objects need private metadata slots populated from eval-declared class
//! metadata, which ordinary public property writes cannot express.
//!
//! Called from:
//! - `crate::interpreter::expressions::eval_expr()` for `new Reflection*`.
//!
//! Key details:
//! - Only eval-declared classes/interfaces/traits/enums are handled here.
//! - Non-eval targets fall back to the generated AOT runtime bridge.

use super::*;

const EVAL_REFLECTION_CLASS_FLAG_FINAL: u64 = 1;
const EVAL_REFLECTION_CLASS_FLAG_ABSTRACT: u64 = 2;
const EVAL_REFLECTION_CLASS_FLAG_INTERFACE: u64 = 4;
const EVAL_REFLECTION_CLASS_FLAG_TRAIT: u64 = 8;
const EVAL_REFLECTION_CLASS_FLAG_ENUM: u64 = 16;
const EVAL_REFLECTION_CLASS_FLAG_READONLY: u64 = 32;
const EVAL_REFLECTION_CLASS_FLAG_INSTANTIABLE: u64 = 64;
const EVAL_REFLECTION_MEMBER_FLAG_STATIC: u64 = 1;
const EVAL_REFLECTION_MEMBER_FLAG_PUBLIC: u64 = 2;
const EVAL_REFLECTION_MEMBER_FLAG_PROTECTED: u64 = 4;
const EVAL_REFLECTION_MEMBER_FLAG_PRIVATE: u64 = 8;
const EVAL_REFLECTION_MEMBER_FLAG_FINAL: u64 = 16;
const EVAL_REFLECTION_MEMBER_FLAG_ABSTRACT: u64 = 32;
const EVAL_REFLECTION_PARAMETER_FLAG_OPTIONAL: u64 = 1;
const EVAL_REFLECTION_PARAMETER_FLAG_VARIADIC: u64 = 2;
const EVAL_REFLECTION_PARAMETER_FLAG_BY_REF: u64 = 4;
const EVAL_REFLECTION_PARAMETER_FLAG_HAS_TYPE: u64 = 8;
const EVAL_REFLECTION_PARAMETER_FLAG_HAS_DEFAULT_VALUE: u64 = 16;
const EVAL_REFLECTION_NAMED_TYPE_FLAG_ALLOWS_NULL: u64 = 1;
const EVAL_REFLECTION_NAMED_TYPE_FLAG_BUILTIN: u64 = 2;

/// Eval metadata needed to materialize one `ReflectionClass` owner object.
struct EvalReflectionClassMetadata {
    resolved_name: String,
    attributes: Vec<EvalAttribute>,
    flags: u64,
    modifiers: u64,
    interface_names: Vec<String>,
    trait_names: Vec<String>,
    method_names: Vec<String>,
    property_names: Vec<String>,
    parent_class_name: Option<String>,
}

/// Eval metadata needed to materialize one `ReflectionMethod` or `ReflectionProperty` owner object.
struct EvalReflectionMemberMetadata {
    declaring_class_name: Option<String>,
    attributes: Vec<EvalAttribute>,
    visibility: EvalVisibility,
    is_static: bool,
    is_final: bool,
    is_abstract: bool,
    required_parameter_count: usize,
    parameters: Vec<EvalReflectionParameterMetadata>,
}

/// Eval metadata needed to materialize one `ReflectionParameter` object.
struct EvalReflectionParameterMetadata {
    name: String,
    attributes: Vec<EvalAttribute>,
    position: usize,
    is_optional: bool,
    is_variadic: bool,
    is_passed_by_reference: bool,
    has_type: bool,
    type_metadata: Option<EvalReflectionParameterTypeMetadata>,
    default_value: Option<EvalExpr>,
}

/// Eval metadata needed to materialize one parameter `ReflectionType` object.
struct EvalReflectionParameterTypeMetadata {
    kind: EvalReflectionParameterTypeKind,
}

/// Eval reflection parameter type object variants.
enum EvalReflectionParameterTypeKind {
    Named(EvalReflectionNamedTypeMetadata),
    Union(EvalReflectionUnionTypeMetadata),
    Intersection(EvalReflectionIntersectionTypeMetadata),
}

/// Eval metadata needed to materialize one `ReflectionNamedType` object.
struct EvalReflectionNamedTypeMetadata {
    name: String,
    allows_null: bool,
    is_builtin: bool,
}

/// Eval metadata needed to materialize one `ReflectionUnionType` object.
struct EvalReflectionUnionTypeMetadata {
    types: Vec<EvalReflectionNamedTypeMetadata>,
    allows_null: bool,
}

/// Eval metadata needed to materialize one `ReflectionIntersectionType` object.
struct EvalReflectionIntersectionTypeMetadata {
    types: Vec<EvalReflectionNamedTypeMetadata>,
}

/// Attempts to construct a ReflectionClass/Method/Property object for eval metadata.
pub(in crate::interpreter) fn eval_reflection_owner_new_object(
    class_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    match reflection_owner_kind(class_name) {
        Some(EVAL_REFLECTION_OWNER_CLASS) => {
            eval_reflection_class_new(evaluated_args, context, values)
        }
        Some(EVAL_REFLECTION_OWNER_METHOD) => {
            eval_reflection_method_new(evaluated_args, context, values)
        }
        Some(EVAL_REFLECTION_OWNER_PROPERTY) => {
            eval_reflection_property_new(evaluated_args, context, values)
        }
        Some(EVAL_REFLECTION_OWNER_CLASS_CONSTANT) => {
            eval_reflection_class_constant_new(evaluated_args, context, values)
        }
        Some(EVAL_REFLECTION_OWNER_ENUM_UNIT_CASE) => eval_reflection_enum_case_new(
            EVAL_REFLECTION_OWNER_ENUM_UNIT_CASE,
            evaluated_args,
            context,
            values,
        ),
        Some(EVAL_REFLECTION_OWNER_ENUM_BACKED_CASE) => eval_reflection_enum_case_new(
            EVAL_REFLECTION_OWNER_ENUM_BACKED_CASE,
            evaluated_args,
            context,
            values,
        ),
        Some(_) => Err(EvalStatus::RuntimeFatal),
        None => Ok(None),
    }
}

/// Handles eval-backed `ReflectionClass::implementsInterface()` calls.
pub(in crate::interpreter) fn eval_reflection_class_implements_interface_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("implementsInterface") {
        return Ok(None);
    }
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let args = bind_evaluated_function_args(&[String::from("interface")], evaluated_args)?;
    let interface_name = eval_reflection_string_arg(args[0], values)?;
    if !eval_reflection_interface_exists(&interface_name, context, values)? {
        if eval_reflection_non_interface_exists(&interface_name, context, values)? {
            return eval_throw_reflection_exception(
                &format!("{} is not an interface", interface_name),
                context,
                values,
            );
        }
        return eval_throw_reflection_exception(
            &format!("Interface \"{}\" does not exist", interface_name),
            context,
            values,
        );
    }
    values
        .bool_value(eval_reflection_class_implements_interface_name(
            &reflected_name,
            &interface_name,
            context,
        ))
        .map(Some)
}

/// Handles eval-backed `ReflectionClass::hasConstant()` calls.
pub(in crate::interpreter) fn eval_reflection_class_has_constant_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("hasConstant") {
        return Ok(None);
    }
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let args = bind_evaluated_function_args(&[String::from("name")], evaluated_args)?;
    let constant_name = eval_reflection_string_arg(args[0], values)?;
    let constant_names = if context.has_interface(&reflected_name) {
        context.interface_constant_names(&reflected_name)
    } else if context.has_trait(&reflected_name) {
        context.trait_constant_names(&reflected_name)
    } else {
        context.class_constant_names(&reflected_name)
    };
    values
        .bool_value(constant_names.iter().any(|name| name == &constant_name))
        .map(Some)
}

/// Handles eval-backed `ReflectionClass::getConstant()` calls.
pub(in crate::interpreter) fn eval_reflection_class_get_constant_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("getConstant") {
        return Ok(None);
    }
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let args = bind_evaluated_function_args(&[String::from("name")], evaluated_args)?;
    let constant_name = eval_reflection_string_arg(args[0], values)?;
    if let Some(value) = eval_reflection_constant_value(&reflected_name, &constant_name, context) {
        return Ok(Some(value));
    }
    values.bool_value(false).map(Some)
}

/// Handles eval-backed `ReflectionClass::getConstants()` calls.
pub(in crate::interpreter) fn eval_reflection_class_get_constants_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("getConstants") {
        return Ok(None);
    }
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let names = eval_reflection_constant_names(&reflected_name, context);
    let mut result = values.assoc_new(names.len())?;
    for name in names {
        let Some(value) = eval_reflection_constant_value(&reflected_name, &name, context) else {
            continue;
        };
        let key = values.string(&name)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(Some(result))
}

/// Handles eval-backed `ReflectionClass::getReflectionConstant()` calls.
pub(in crate::interpreter) fn eval_reflection_class_get_reflection_constant_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("getReflectionConstant") {
        return Ok(None);
    }
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let args = bind_evaluated_function_args(&[String::from("name")], evaluated_args)?;
    let requested_name = eval_reflection_string_arg(args[0], values)?;
    if !eval_reflection_constant_names(&reflected_name, context)
        .iter()
        .any(|name| name == &requested_name)
    {
        return values.bool_value(false).map(Some);
    }
    eval_reflection_class_constant_object_result(&reflected_name, &requested_name, context, values)
        .map(Some)
}

/// Handles eval-backed `ReflectionClass::getReflectionConstants()` calls.
pub(in crate::interpreter) fn eval_reflection_class_get_reflection_constants_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("getReflectionConstants") {
        return Ok(None);
    }
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let names = eval_reflection_constant_names(&reflected_name, context);
    let mut result = values.array_new(names.len())?;
    for (index, name) in names.iter().enumerate() {
        let object =
            eval_reflection_class_constant_object_result(&reflected_name, name, context, values)?;
        let key = values.int(index as i64)?;
        result = values.array_set(result, key, object)?;
    }
    Ok(Some(result))
}

/// Handles eval-backed `ReflectionClass::getMethod()` and `getProperty()` calls.
pub(in crate::interpreter) fn eval_reflection_class_get_member_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let owner_kind = if method_name.eq_ignore_ascii_case("getMethod") {
        EVAL_REFLECTION_OWNER_METHOD
    } else if method_name.eq_ignore_ascii_case("getProperty") {
        EVAL_REFLECTION_OWNER_PROPERTY
    } else {
        return Ok(None);
    };
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let args = bind_evaluated_function_args(&[String::from("name")], evaluated_args)?;
    let requested_name = eval_reflection_string_arg(args[0], values)?;
    let Some(member_name) =
        eval_reflection_member_name(owner_kind, &reflected_name, &requested_name, context)
    else {
        let message_name = eval_reflection_class_like_attributes(&reflected_name, context)
            .map(|metadata| metadata.resolved_name)
            .unwrap_or_else(|| reflected_name.clone());
        let message =
            eval_reflection_missing_member_message(owner_kind, &message_name, &requested_name);
        return eval_throw_reflection_exception(&message, context, values);
    };
    let member =
        eval_reflection_member_metadata(owner_kind, &reflected_name, &member_name, context)
            .ok_or(EvalStatus::RuntimeFatal)?;
    let flags = eval_reflection_member_flags(
        member.visibility,
        member.is_static,
        member.is_final,
        member.is_abstract,
    );
    eval_reflection_owner_object(
        owner_kind,
        &member_name,
        &member.attributes,
        &[],
        &[],
        &[],
        &[],
        member.declaring_class_name.as_deref(),
        &member.parameters,
        flags,
        member.required_parameter_count as u64,
        context,
        values,
    )
    .map(Some)
}

/// Returns the constant names visible through eval-backed `ReflectionClass`.
fn eval_reflection_constant_names(
    reflected_name: &str,
    context: &ElephcEvalContext,
) -> Vec<String> {
    if context.has_interface(reflected_name) {
        context.interface_constant_names(reflected_name)
    } else if context.has_trait(reflected_name) {
        context.trait_constant_names(reflected_name)
    } else {
        context.class_constant_names(reflected_name)
    }
}

/// Returns a materialized eval constant value for Reflection without visibility checks.
fn eval_reflection_constant_value(
    reflected_name: &str,
    constant_name: &str,
    context: &ElephcEvalContext,
) -> Option<RuntimeCellHandle> {
    if let Some(case) = context.enum_case(reflected_name, constant_name) {
        return Some(case);
    }
    let (declaring_class, constant) = context.class_constant(reflected_name, constant_name)?;
    context.class_constant_cell(&declaring_class, constant.name())
}

/// Builds one eval-backed `ReflectionClassConstant` object for a visible constant name.
fn eval_reflection_class_constant_object_result(
    reflected_name: &str,
    constant_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (declaring_class_name, attributes) =
        eval_reflection_class_constant_metadata(reflected_name, constant_name, context)
            .ok_or(EvalStatus::RuntimeFatal)?;
    eval_reflection_owner_object(
        EVAL_REFLECTION_OWNER_CLASS_CONSTANT,
        constant_name,
        &attributes,
        &[],
        &[],
        &[],
        &[],
        Some(&declaring_class_name),
        &[],
        0,
        0,
        context,
        values,
    )
}

/// Resolves the declared member spelling for eval `ReflectionClass` single-member lookups.
fn eval_reflection_member_name(
    owner_kind: u64,
    reflected_name: &str,
    requested_name: &str,
    context: &ElephcEvalContext,
) -> Option<String> {
    let metadata = eval_reflection_class_like_attributes(reflected_name, context)?;
    let names = if owner_kind == EVAL_REFLECTION_OWNER_METHOD {
        metadata.method_names
    } else {
        metadata.property_names
    };
    names.into_iter().find(|name| match owner_kind {
        EVAL_REFLECTION_OWNER_METHOD => name.eq_ignore_ascii_case(requested_name),
        EVAL_REFLECTION_OWNER_PROPERTY => name == requested_name,
        _ => false,
    })
}

/// Builds PHP-compatible missing-member messages for eval ReflectionClass lookups.
fn eval_reflection_missing_member_message(
    owner_kind: u64,
    reflected_name: &str,
    requested_name: &str,
) -> String {
    if owner_kind == EVAL_REFLECTION_OWNER_METHOD {
        format!(
            "Method {}::{}() does not exist",
            reflected_name, requested_name
        )
    } else {
        format!(
            "Property {}::${} does not exist",
            reflected_name, requested_name
        )
    }
}

/// Builds an eval-backed `ReflectionClass` object when the reflected class-like exists in eval.
fn eval_reflection_class_new(
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let args = bind_evaluated_function_args(&[String::from("class_name")], evaluated_args)?;
    let class_name = eval_reflection_string_arg(args[0], values)?;
    let Some(metadata) = eval_reflection_class_like_attributes(&class_name, context) else {
        return Ok(None);
    };
    eval_reflection_owner_object(
        EVAL_REFLECTION_OWNER_CLASS,
        &metadata.resolved_name,
        &metadata.attributes,
        &metadata.interface_names,
        &metadata.trait_names,
        &metadata.method_names,
        &metadata.property_names,
        metadata.parent_class_name.as_deref(),
        &[],
        metadata.flags,
        metadata.modifiers,
        context,
        values,
    )
    .map(Some)
}

/// Builds an eval-backed `ReflectionMethod` object when the reflected method exists in eval.
fn eval_reflection_method_new(
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let args = bind_evaluated_function_args(
        &[String::from("class_name"), String::from("method_name")],
        evaluated_args,
    )?;
    let class_name = eval_reflection_string_arg(args[0], values)?;
    if !eval_reflection_class_like_exists(&class_name, context) {
        return Ok(None);
    }
    let method_name = eval_reflection_string_arg(args[1], values)?;
    let method = eval_reflection_method_metadata(&class_name, &method_name, context)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let flags = eval_reflection_member_flags(
        method.visibility,
        method.is_static,
        method.is_final,
        method.is_abstract,
    );
    eval_reflection_owner_object(
        EVAL_REFLECTION_OWNER_METHOD,
        &method_name,
        &method.attributes,
        &[],
        &[],
        &[],
        &[],
        method.declaring_class_name.as_deref(),
        &method.parameters,
        flags,
        method.required_parameter_count as u64,
        context,
        values,
    )
    .map(Some)
}

/// Builds an eval-backed `ReflectionProperty` object when the reflected property exists in eval.
fn eval_reflection_property_new(
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let args = bind_evaluated_function_args(
        &[String::from("class_name"), String::from("property_name")],
        evaluated_args,
    )?;
    let class_name = eval_reflection_string_arg(args[0], values)?;
    if !eval_reflection_class_like_exists(&class_name, context) {
        return Ok(None);
    }
    let property_name = eval_reflection_string_arg(args[1], values)?;
    let property = eval_reflection_property_metadata(&class_name, &property_name, context)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let flags = eval_reflection_member_flags(property.visibility, property.is_static, false, false);
    eval_reflection_owner_object(
        EVAL_REFLECTION_OWNER_PROPERTY,
        &property_name,
        &property.attributes,
        &[],
        &[],
        &[],
        &[],
        property.declaring_class_name.as_deref(),
        &[],
        flags,
        0,
        context,
        values,
    )
    .map(Some)
}

/// Builds an eval-backed `ReflectionClassConstant` object for a class constant or enum case.
fn eval_reflection_class_constant_new(
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let args = bind_evaluated_function_args(
        &[String::from("class_name"), String::from("constant_name")],
        evaluated_args,
    )?;
    let class_name = eval_reflection_string_arg(args[0], values)?;
    if !eval_reflection_class_like_exists(&class_name, context) {
        return Ok(None);
    }
    let constant_name = eval_reflection_string_arg(args[1], values)?;
    let (declaring_class_name, attributes) =
        eval_reflection_class_constant_metadata(&class_name, &constant_name, context)
            .ok_or(EvalStatus::RuntimeFatal)?;
    eval_reflection_owner_object(
        EVAL_REFLECTION_OWNER_CLASS_CONSTANT,
        &constant_name,
        &attributes,
        &[],
        &[],
        &[],
        &[],
        Some(&declaring_class_name),
        &[],
        0,
        0,
        context,
        values,
    )
    .map(Some)
}

/// Builds an eval-backed ReflectionEnumUnitCase/BackedCase object for an enum case.
fn eval_reflection_enum_case_new(
    owner_kind: u64,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let args = bind_evaluated_function_args(
        &[String::from("class_name"), String::from("constant_name")],
        evaluated_args,
    )?;
    let enum_name = eval_reflection_string_arg(args[0], values)?;
    let Some(enum_decl) = context.enum_decl(&enum_name) else {
        return if eval_reflection_class_like_exists(&enum_name, context) {
            Err(EvalStatus::RuntimeFatal)
        } else {
            Ok(None)
        };
    };
    if owner_kind == EVAL_REFLECTION_OWNER_ENUM_BACKED_CASE && enum_decl.backing_type().is_none() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let case_name = eval_reflection_string_arg(args[1], values)?;
    let declaring_class_name = enum_decl.name().to_string();
    let attributes = enum_decl
        .case(&case_name)
        .map(|case| case.attributes().to_vec())
        .ok_or(EvalStatus::RuntimeFatal)?;
    eval_reflection_owner_object(
        owner_kind,
        &case_name,
        &attributes,
        &[],
        &[],
        &[],
        &[],
        Some(&declaring_class_name),
        &[],
        0,
        0,
        context,
        values,
    )
    .map(Some)
}

/// Materializes one Reflection owner object and transfers the temporary attribute array.
fn eval_reflection_owner_object(
    owner_kind: u64,
    reflected_name: &str,
    attributes: &[EvalAttribute],
    interface_names: &[String],
    trait_names: &[String],
    method_names: &[String],
    property_names: &[String],
    parent_class_name: Option<&str>,
    parameter_metadata: &[EvalReflectionParameterMetadata],
    flags: u64,
    modifiers: u64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_reflection_owner_object_with_members(
        owner_kind,
        reflected_name,
        attributes,
        interface_names,
        trait_names,
        method_names,
        property_names,
        parent_class_name,
        parameter_metadata,
        flags,
        modifiers,
        true,
        context,
        values,
    )
}

/// Materializes one Reflection owner object with optional nested class member objects.
fn eval_reflection_owner_object_with_members(
    owner_kind: u64,
    reflected_name: &str,
    attributes: &[EvalAttribute],
    interface_names: &[String],
    trait_names: &[String],
    method_names: &[String],
    property_names: &[String],
    parent_class_name: Option<&str>,
    parameter_metadata: &[EvalReflectionParameterMetadata],
    flags: u64,
    modifiers: u64,
    include_class_members: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let attrs = eval_reflection_attribute_array_result(attributes, context, values)?;
    let interface_names_array = eval_reflection_string_array_result(interface_names, values)?;
    let trait_names_array = eval_reflection_string_array_result(trait_names, values)?;
    let method_names_array = eval_reflection_string_array_result(method_names, values)?;
    let property_names_array = eval_reflection_string_array_result(property_names, values)?;
    let method_objects = if owner_kind == EVAL_REFLECTION_OWNER_CLASS && include_class_members {
        eval_reflection_member_object_array_result(
            EVAL_REFLECTION_OWNER_METHOD,
            reflected_name,
            &method_names,
            context,
            values,
        )?
    } else if owner_kind == EVAL_REFLECTION_OWNER_METHOD {
        eval_reflection_parameter_object_array_result(parameter_metadata, context, values)?
    } else {
        values.array_new(0)?
    };
    let property_objects = if owner_kind == EVAL_REFLECTION_OWNER_CLASS && include_class_members {
        eval_reflection_member_object_array_result(
            EVAL_REFLECTION_OWNER_PROPERTY,
            reflected_name,
            &property_names,
            context,
            values,
        )?
    } else {
        values.array_new(0)?
    };
    let parent_class = eval_reflection_related_class_result(
        owner_kind,
        parent_class_name,
        include_class_members,
        context,
        values,
    )?;
    let object = values.reflection_owner_new(
        owner_kind,
        reflected_name,
        attrs,
        interface_names_array,
        trait_names_array,
        method_names_array,
        property_names_array,
        method_objects,
        property_objects,
        parent_class,
        flags,
        modifiers,
    )?;
    if owner_kind == EVAL_REFLECTION_OWNER_CLASS {
        let identity = values.object_identity(object)?;
        context.register_eval_reflection_class(identity, reflected_name);
    }
    values.release(attrs)?;
    values.release(interface_names_array)?;
    values.release(trait_names_array)?;
    values.release(method_names_array)?;
    values.release(property_names_array)?;
    values.release(method_objects)?;
    values.release(property_objects)?;
    values.release(parent_class)?;
    Ok(object)
}

/// Builds the `ReflectionClass|false` value stored in parent or declaring-class slots.
fn eval_reflection_related_class_result(
    owner_kind: u64,
    related_class_name: Option<&str>,
    include_class_members: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(related_class_name) = related_class_name else {
        return values.bool_value(false);
    };
    if owner_kind == EVAL_REFLECTION_OWNER_CLASS && include_class_members {
        return eval_reflection_full_class_object_result(related_class_name, context, values);
    }
    if matches!(
        owner_kind,
        EVAL_REFLECTION_OWNER_METHOD
            | EVAL_REFLECTION_OWNER_PROPERTY
            | EVAL_REFLECTION_OWNER_CLASS_CONSTANT
            | EVAL_REFLECTION_OWNER_ENUM_UNIT_CASE
            | EVAL_REFLECTION_OWNER_ENUM_BACKED_CASE
    ) {
        return eval_reflection_shallow_class_object_result(related_class_name, context, values);
    }
    values.bool_value(false)
}

/// Builds a full `ReflectionClass` object for parent-class metadata.
fn eval_reflection_full_class_object_result(
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(metadata) = eval_reflection_class_like_attributes(class_name, context) else {
        return values.bool_value(false);
    };
    eval_reflection_owner_object(
        EVAL_REFLECTION_OWNER_CLASS,
        &metadata.resolved_name,
        &metadata.attributes,
        &metadata.interface_names,
        &metadata.trait_names,
        &metadata.method_names,
        &metadata.property_names,
        metadata.parent_class_name.as_deref(),
        &[],
        metadata.flags,
        metadata.modifiers,
        context,
        values,
    )
}

/// Builds a shallow `ReflectionClass` object for member declaring-class metadata.
fn eval_reflection_shallow_class_object_result(
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(metadata) = eval_reflection_class_like_attributes(class_name, context) else {
        return values.bool_value(false);
    };
    eval_reflection_owner_object_with_members(
        EVAL_REFLECTION_OWNER_CLASS,
        &metadata.resolved_name,
        &metadata.attributes,
        &metadata.interface_names,
        &metadata.trait_names,
        &[],
        &[],
        None,
        &[],
        metadata.flags,
        metadata.modifiers,
        false,
        context,
        values,
    )
}

/// Builds an indexed PHP string array for ReflectionClass metadata names.
fn eval_reflection_string_array_result(
    names: &[String],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.string_array_new(names.len())?;
    for name in names {
        result = values.string_array_push(result, name)?;
    }
    Ok(result)
}

/// Builds an indexed array of populated ReflectionParameter objects.
fn eval_reflection_parameter_object_array_result(
    parameters: &[EvalReflectionParameterMetadata],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(parameters.len())?;
    for parameter in parameters {
        let parameter_object = eval_reflection_parameter_object_result(parameter, context, values)?;
        let key = values.int(parameter.position as i64)?;
        result = values.array_set(result, key, parameter_object)?;
    }
    Ok(result)
}

/// Materializes one ReflectionParameter object through the shared reflection helper.
fn eval_reflection_parameter_object_result(
    parameter: &EvalReflectionParameterMetadata,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let attrs = eval_reflection_attribute_array_result(&parameter.attributes, context, values)?;
    let interface_names = values.array_new(0)?;
    let trait_names = values.array_new(0)?;
    let method_names = values.array_new(0)?;
    let property_names = values.array_new(0)?;
    let method_objects = values.array_new(0)?;
    let parent_class = values.bool_value(false)?;
    let type_value = match parameter.type_metadata.as_ref() {
        Some(type_metadata) => eval_reflection_type_object_result(type_metadata, values)?,
        None => values.null()?,
    };
    let default_value = match parameter.default_value.as_ref() {
        Some(default) => eval_method_parameter_default(default, context, values)?,
        None => values.null()?,
    };
    let flags = eval_reflection_parameter_flags(parameter);
    let object = values.reflection_owner_new(
        EVAL_REFLECTION_OWNER_PARAMETER,
        &parameter.name,
        attrs,
        interface_names,
        trait_names,
        method_names,
        property_names,
        type_value,
        default_value,
        parent_class,
        flags,
        parameter.position as u64,
    )?;
    values.release(attrs)?;
    values.release(interface_names)?;
    values.release(trait_names)?;
    values.release(method_names)?;
    values.release(property_names)?;
    values.release(method_objects)?;
    values.release(type_value)?;
    values.release(default_value)?;
    values.release(parent_class)?;
    Ok(object)
}

/// Materializes one parameter ReflectionType object through the shared reflection helper.
fn eval_reflection_type_object_result(
    type_metadata: &EvalReflectionParameterTypeMetadata,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match &type_metadata.kind {
        EvalReflectionParameterTypeKind::Named(named_type) => {
            eval_reflection_named_type_object_result(named_type, values)
        }
        EvalReflectionParameterTypeKind::Union(union_type) => {
            eval_reflection_union_type_object_result(union_type, values)
        }
        EvalReflectionParameterTypeKind::Intersection(intersection_type) => {
            eval_reflection_intersection_type_object_result(intersection_type, values)
        }
    }
}

/// Materializes one ReflectionNamedType object through the shared reflection helper.
fn eval_reflection_named_type_object_result(
    type_metadata: &EvalReflectionNamedTypeMetadata,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let attrs = values.array_new(0)?;
    let interface_names = values.array_new(0)?;
    let trait_names = values.array_new(0)?;
    let method_names = values.array_new(0)?;
    let property_names = values.array_new(0)?;
    let method_objects = values.array_new(0)?;
    let property_objects = values.array_new(0)?;
    let parent_class = values.bool_value(false)?;
    let flags = eval_reflection_named_type_flags(type_metadata);
    let object = values.reflection_owner_new(
        EVAL_REFLECTION_OWNER_NAMED_TYPE,
        &type_metadata.name,
        attrs,
        interface_names,
        trait_names,
        method_names,
        property_names,
        method_objects,
        property_objects,
        parent_class,
        flags,
        0,
    )?;
    values.release(attrs)?;
    values.release(interface_names)?;
    values.release(trait_names)?;
    values.release(method_names)?;
    values.release(property_names)?;
    values.release(method_objects)?;
    values.release(property_objects)?;
    values.release(parent_class)?;
    Ok(object)
}

/// Materializes one ReflectionUnionType object through the shared reflection helper.
fn eval_reflection_union_type_object_result(
    type_metadata: &EvalReflectionUnionTypeMetadata,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let attrs = values.array_new(0)?;
    let interface_names = values.array_new(0)?;
    let trait_names = values.array_new(0)?;
    let method_names = values.array_new(0)?;
    let property_names = values.array_new(0)?;
    let types = eval_reflection_named_type_object_array_result(&type_metadata.types, values)?;
    let property_objects = values.array_new(0)?;
    let parent_class = values.bool_value(false)?;
    let flags = eval_reflection_union_type_flags(type_metadata);
    let object = values.reflection_owner_new(
        EVAL_REFLECTION_OWNER_UNION_TYPE,
        "",
        attrs,
        interface_names,
        trait_names,
        method_names,
        property_names,
        types,
        property_objects,
        parent_class,
        flags,
        0,
    )?;
    values.release(attrs)?;
    values.release(interface_names)?;
    values.release(trait_names)?;
    values.release(method_names)?;
    values.release(property_names)?;
    values.release(types)?;
    values.release(property_objects)?;
    values.release(parent_class)?;
    Ok(object)
}

/// Materializes one ReflectionIntersectionType object through the shared reflection helper.
fn eval_reflection_intersection_type_object_result(
    type_metadata: &EvalReflectionIntersectionTypeMetadata,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let attrs = values.array_new(0)?;
    let interface_names = values.array_new(0)?;
    let trait_names = values.array_new(0)?;
    let method_names = values.array_new(0)?;
    let property_names = values.array_new(0)?;
    let types = eval_reflection_named_type_object_array_result(&type_metadata.types, values)?;
    let property_objects = values.array_new(0)?;
    let parent_class = values.bool_value(false)?;
    let object = values.reflection_owner_new(
        EVAL_REFLECTION_OWNER_INTERSECTION_TYPE,
        "",
        attrs,
        interface_names,
        trait_names,
        method_names,
        property_names,
        types,
        property_objects,
        parent_class,
        0,
        0,
    )?;
    values.release(attrs)?;
    values.release(interface_names)?;
    values.release(trait_names)?;
    values.release(method_names)?;
    values.release(property_names)?;
    values.release(types)?;
    values.release(property_objects)?;
    values.release(parent_class)?;
    Ok(object)
}

/// Builds an indexed array of populated ReflectionNamedType objects.
fn eval_reflection_named_type_object_array_result(
    types: &[EvalReflectionNamedTypeMetadata],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(types.len())?;
    for (position, type_metadata) in types.iter().enumerate() {
        let type_object = eval_reflection_named_type_object_result(type_metadata, values)?;
        let key = values.int(position as i64)?;
        result = values.array_set(result, key, type_object)?;
    }
    Ok(result)
}

/// Builds an indexed array of ReflectionMethod or ReflectionProperty objects for a ReflectionClass.
fn eval_reflection_member_object_array_result(
    owner_kind: u64,
    class_name: &str,
    names: &[String],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(names.len())?;
    let mut index = 0;
    for name in names {
        let Some(member) = eval_reflection_member_metadata(owner_kind, class_name, name, context)
        else {
            continue;
        };
        let flags = eval_reflection_member_flags(
            member.visibility,
            member.is_static,
            member.is_final,
            member.is_abstract,
        );
        let member_object = eval_reflection_owner_object(
            owner_kind,
            name,
            &member.attributes,
            &[],
            &[],
            &[],
            &[],
            member.declaring_class_name.as_deref(),
            &member.parameters,
            flags,
            member.required_parameter_count as u64,
            context,
            values,
        )?;
        let key = values.int(index)?;
        result = values.array_set(result, key, member_object)?;
        index += 1;
    }
    Ok(result)
}

/// Returns member metadata for one ReflectionClass member-array entry.
fn eval_reflection_member_metadata(
    owner_kind: u64,
    class_name: &str,
    name: &str,
    context: &ElephcEvalContext,
) -> Option<EvalReflectionMemberMetadata> {
    match owner_kind {
        EVAL_REFLECTION_OWNER_METHOD => eval_reflection_method_metadata(class_name, name, context),
        EVAL_REFLECTION_OWNER_PROPERTY => {
            eval_reflection_property_metadata(class_name, name, context)
        }
        _ => None,
    }
}

/// Returns the eval-retained class-like attributes plus canonical reflected name.
fn eval_reflection_class_like_attributes(
    name: &str,
    context: &ElephcEvalContext,
) -> Option<EvalReflectionClassMetadata> {
    if let Some(class) = context.class(name) {
        let is_enum = context.has_enum(class.name());
        let mut flags = 0;
        if class.is_final() {
            flags |= EVAL_REFLECTION_CLASS_FLAG_FINAL;
        }
        if class.is_abstract() {
            flags |= EVAL_REFLECTION_CLASS_FLAG_ABSTRACT;
        }
        if is_enum {
            flags |= EVAL_REFLECTION_CLASS_FLAG_ENUM;
        }
        if class.is_readonly_class() && !is_enum {
            flags |= EVAL_REFLECTION_CLASS_FLAG_READONLY;
        }
        if eval_reflection_class_is_instantiable(class, is_enum, context) {
            flags |= EVAL_REFLECTION_CLASS_FLAG_INSTANTIABLE;
        }
        let modifiers = eval_reflection_class_modifiers(
            class.is_final(),
            class.is_abstract(),
            class.is_readonly_class(),
            is_enum,
        );
        return Some(EvalReflectionClassMetadata {
            resolved_name: class.name().trim_start_matches('\\').to_string(),
            attributes: class.attributes().to_vec(),
            interface_names: context.class_interface_names(class.name()),
            trait_names: context.class_trait_names(class.name()),
            method_names: context.class_method_names(class.name()),
            property_names: context.class_property_names(class.name()),
            parent_class_name: eval_reflection_parent_class_name(class, context),
            flags,
            modifiers,
        });
    }
    if let Some(interface) = context.interface(name) {
        return Some(EvalReflectionClassMetadata {
            resolved_name: interface.name().trim_start_matches('\\').to_string(),
            attributes: interface.attributes().to_vec(),
            interface_names: context.interface_parent_names(interface.name()),
            trait_names: Vec::new(),
            method_names: context.interface_method_names(interface.name()),
            property_names: context.interface_property_names(interface.name()),
            parent_class_name: None,
            flags: EVAL_REFLECTION_CLASS_FLAG_INTERFACE,
            modifiers: 0,
        });
    }
    if let Some(trait_decl) = context.trait_decl(name) {
        return Some(EvalReflectionClassMetadata {
            resolved_name: trait_decl.name().trim_start_matches('\\').to_string(),
            attributes: trait_decl.attributes().to_vec(),
            interface_names: Vec::new(),
            trait_names: Vec::new(),
            method_names: context.trait_method_names(trait_decl.name()),
            property_names: context.trait_property_names(trait_decl.name()),
            parent_class_name: None,
            flags: EVAL_REFLECTION_CLASS_FLAG_TRAIT,
            modifiers: 0,
        });
    }
    context
        .enum_decl(name)
        .map(|enum_decl| EvalReflectionClassMetadata {
            resolved_name: enum_decl.name().trim_start_matches('\\').to_string(),
            attributes: enum_decl.attributes().to_vec(),
            interface_names: context.class_interface_names(enum_decl.name()),
            trait_names: Vec::new(),
            method_names: context.class_method_names(enum_decl.name()),
            property_names: context.class_property_names(enum_decl.name()),
            parent_class_name: None,
            flags: EVAL_REFLECTION_CLASS_FLAG_FINAL | EVAL_REFLECTION_CLASS_FLAG_ENUM,
            modifiers: 32,
        })
}

/// Returns the canonical eval parent class name for ReflectionClass metadata.
fn eval_reflection_parent_class_name(
    class: &EvalClass,
    context: &ElephcEvalContext,
) -> Option<String> {
    let parent = class.parent()?;
    context
        .class(parent)
        .map(|parent_class| parent_class.name().trim_start_matches('\\').to_string())
        .or_else(|| Some(parent.trim_start_matches('\\').to_string()))
}

/// Returns PHP's `ReflectionClass::isInstantiable()` value for eval class metadata.
fn eval_reflection_class_is_instantiable(
    class: &EvalClass,
    is_enum: bool,
    context: &ElephcEvalContext,
) -> bool {
    if class.is_abstract() || is_enum {
        return false;
    }
    context
        .class_method(class.name(), "__construct")
        .map(|(_, method)| method.visibility() == EvalVisibility::Public)
        .unwrap_or(true)
}

/// Computes PHP's `ReflectionClass::getModifiers()` bitmask for eval metadata.
fn eval_reflection_class_modifiers(
    is_final: bool,
    is_abstract: bool,
    is_readonly_class: bool,
    is_enum: bool,
) -> u64 {
    let mut modifiers = 0;
    if is_final {
        modifiers |= 32;
    }
    if is_abstract {
        modifiers |= 64;
    }
    if is_readonly_class && !is_enum {
        modifiers |= 65_536;
    }
    modifiers
}

/// Returns declaring class and attributes attached to an eval class constant or enum case.
fn eval_reflection_class_constant_metadata(
    class_name: &str,
    constant_name: &str,
    context: &ElephcEvalContext,
) -> Option<(String, Vec<EvalAttribute>)> {
    if let Some(enum_decl) = context.enum_decl(class_name) {
        if let Some(case) = enum_decl.case(constant_name) {
            return Some((enum_decl.name().to_string(), case.attributes().to_vec()));
        }
    }
    context
        .class_constant(class_name, constant_name)
        .map(|(declaring_class, constant)| (declaring_class, constant.attributes().to_vec()))
}

/// Returns true when a name resolves to an eval-declared class-like symbol.
fn eval_reflection_class_like_exists(name: &str, context: &ElephcEvalContext) -> bool {
    context.has_class(name)
        || context.has_interface(name)
        || context.has_trait(name)
        || context.has_enum(name)
}

/// Returns true when one name exists as an eval or runtime interface.
fn eval_reflection_interface_exists(
    name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    Ok(context.has_interface(name) || values.interface_exists(name)?)
}

/// Returns true when one name exists as a non-interface class-like symbol.
fn eval_reflection_non_interface_exists(
    name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if context.has_class(name)
        || context.has_trait(name)
        || context.has_enum(name)
        || values.class_exists(name)?
        || values.trait_exists(name)?
    {
        return Ok(true);
    }
    values.enum_exists(name)
}

/// Returns true when reflected eval metadata implements or extends an interface name.
fn eval_reflection_class_implements_interface_name(
    reflected_name: &str,
    interface_name: &str,
    context: &ElephcEvalContext,
) -> bool {
    if context.has_interface(reflected_name) {
        return eval_reflection_same_class_like_name(reflected_name, interface_name)
            || context
                .interface_parent_names(reflected_name)
                .iter()
                .any(|parent| eval_reflection_same_class_like_name(parent, interface_name));
    }
    if context.has_class(reflected_name) || context.has_enum(reflected_name) {
        return context
            .class_interface_names(reflected_name)
            .iter()
            .any(|interface| eval_reflection_same_class_like_name(interface, interface_name));
    }
    false
}

/// Returns true when two PHP class-like names match case-insensitively.
fn eval_reflection_same_class_like_name(left: &str, right: &str) -> bool {
    left.trim_start_matches('\\')
        .eq_ignore_ascii_case(right.trim_start_matches('\\'))
}

/// Creates a catchable `ReflectionException` and propagates it through eval throw state.
fn eval_throw_reflection_exception(
    message: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let exception = values.new_object("ReflectionException")?;
    let message = values.string(message)?;
    let code = values.int(0)?;
    values.construct_object(exception, vec![message, code])?;
    context.set_pending_throw(exception);
    Err(EvalStatus::UncaughtThrowable)
}

/// Returns method metadata for a method-like member on an eval class-like symbol.
fn eval_reflection_method_metadata(
    class_name: &str,
    method_name: &str,
    context: &ElephcEvalContext,
) -> Option<EvalReflectionMemberMetadata> {
    if context.has_class(class_name) || context.has_enum(class_name) {
        return context
            .class_method(class_name, method_name)
            .map(|(declaring_class, method)| EvalReflectionMemberMetadata {
                declaring_class_name: Some(declaring_class),
                attributes: method.attributes().to_vec(),
                visibility: method.visibility(),
                is_static: method.is_static(),
                is_final: method.is_final(),
                is_abstract: method.is_abstract(),
                required_parameter_count: eval_reflection_required_parameter_count(
                    method.parameter_defaults(),
                    method.parameter_is_variadic(),
                ),
                parameters: eval_reflection_parameters_from_names_and_type_flags(
                    method.params(),
                    method.parameter_has_types(),
                    method.parameter_types(),
                    method.parameter_attributes(),
                    method.parameter_defaults(),
                    method.parameter_is_by_ref(),
                    method.parameter_is_variadic(),
                ),
            });
    }
    if context.has_interface(class_name) {
        return context
            .interface_method_requirements(class_name)
            .into_iter()
            .find(|method| method.name().eq_ignore_ascii_case(method_name))
            .map(|method| EvalReflectionMemberMetadata {
                declaring_class_name: Some(class_name.to_string()),
                attributes: method.attributes().to_vec(),
                visibility: EvalVisibility::Public,
                is_static: method.is_static(),
                is_final: false,
                is_abstract: true,
                required_parameter_count: eval_reflection_required_parameter_count(
                    method.parameter_defaults(),
                    method.parameter_is_variadic(),
                ),
                parameters: eval_reflection_parameters_from_names_and_type_flags(
                    method.params(),
                    method.parameter_has_types(),
                    method.parameter_types(),
                    method.parameter_attributes(),
                    method.parameter_defaults(),
                    method.parameter_is_by_ref(),
                    method.parameter_is_variadic(),
                ),
            });
    }
    context.trait_decl(class_name).and_then(|trait_decl| {
        trait_decl
            .methods()
            .iter()
            .find(|method| method.name().eq_ignore_ascii_case(method_name))
            .map(|method| EvalReflectionMemberMetadata {
                declaring_class_name: Some(trait_decl.name().to_string()),
                attributes: method.attributes().to_vec(),
                visibility: method.visibility(),
                is_static: method.is_static(),
                is_final: method.is_final(),
                is_abstract: method.is_abstract(),
                required_parameter_count: eval_reflection_required_parameter_count(
                    method.parameter_defaults(),
                    method.parameter_is_variadic(),
                ),
                parameters: eval_reflection_parameters_from_names_and_type_flags(
                    method.params(),
                    method.parameter_has_types(),
                    method.parameter_types(),
                    method.parameter_attributes(),
                    method.parameter_defaults(),
                    method.parameter_is_by_ref(),
                    method.parameter_is_variadic(),
                ),
            })
    })
}

/// Returns property metadata for a property-like member on an eval class-like symbol.
fn eval_reflection_property_metadata(
    class_name: &str,
    property_name: &str,
    context: &ElephcEvalContext,
) -> Option<EvalReflectionMemberMetadata> {
    if context.has_class(class_name) || context.has_enum(class_name) {
        return context
            .class_property(class_name, property_name)
            .map(|(declaring_class, property)| EvalReflectionMemberMetadata {
                declaring_class_name: Some(declaring_class),
                attributes: property.attributes().to_vec(),
                visibility: property.visibility(),
                is_static: property.is_static(),
                is_final: false,
                is_abstract: property.is_abstract(),
                required_parameter_count: 0,
                parameters: Vec::new(),
            });
    }
    if context.has_interface(class_name) {
        return context
            .interface_property_requirements(class_name)
            .into_iter()
            .find(|property| property.name() == property_name)
            .map(|property| EvalReflectionMemberMetadata {
                declaring_class_name: Some(class_name.to_string()),
                attributes: property.attributes().to_vec(),
                visibility: EvalVisibility::Public,
                is_static: false,
                is_final: false,
                is_abstract: true,
                required_parameter_count: 0,
                parameters: Vec::new(),
            });
    }
    context.trait_decl(class_name).and_then(|trait_decl| {
        trait_decl
            .properties()
            .iter()
            .find(|property| property.name() == property_name)
            .map(|property| EvalReflectionMemberMetadata {
                declaring_class_name: Some(trait_decl.name().to_string()),
                attributes: property.attributes().to_vec(),
                visibility: property.visibility(),
                is_static: property.is_static(),
                is_final: false,
                is_abstract: property.is_abstract(),
                required_parameter_count: 0,
                parameters: Vec::new(),
            })
    })
}

/// Returns PHP's required parameter count for a reflected method signature.
fn eval_reflection_required_parameter_count(
    defaults: &[Option<EvalExpr>],
    variadic_flags: &[bool],
) -> usize {
    let fixed_count = variadic_flags
        .iter()
        .position(|is_variadic| *is_variadic)
        .unwrap_or(defaults.len());
    (0..fixed_count)
        .rfind(|position| !defaults.get(*position).is_some_and(Option::is_some))
        .map_or(0, |position| position + 1)
}

/// Builds parameter reflection metadata from eval parameter names and type flags.
fn eval_reflection_parameters_from_names_and_type_flags(
    names: &[String],
    has_type_flags: &[bool],
    parameter_types: &[Option<EvalParameterType>],
    parameter_attributes: &[Vec<EvalAttribute>],
    defaults: &[Option<EvalExpr>],
    by_ref_flags: &[bool],
    variadic_flags: &[bool],
) -> Vec<EvalReflectionParameterMetadata> {
    names
        .iter()
        .enumerate()
        .map(|(position, name)| EvalReflectionParameterMetadata {
            name: name.clone(),
            attributes: parameter_attributes
                .get(position)
                .cloned()
                .unwrap_or_default(),
            position,
            is_optional: defaults.get(position).is_some_and(Option::is_some)
                || variadic_flags.get(position).copied().unwrap_or(false),
            is_variadic: variadic_flags.get(position).copied().unwrap_or(false),
            is_passed_by_reference: by_ref_flags.get(position).copied().unwrap_or(false),
            has_type: has_type_flags.get(position).copied().unwrap_or(false),
            type_metadata: parameter_types
                .get(position)
                .and_then(Option::as_ref)
                .and_then(eval_reflection_parameter_type_metadata)
                .filter(|_| has_type_flags.get(position).copied().unwrap_or(false)),
            default_value: defaults.get(position).and_then(Clone::clone),
        })
        .collect()
}

/// Converts eval parameter type metadata into the supported ReflectionType subset.
fn eval_reflection_parameter_type_metadata(
    parameter_type: &EvalParameterType,
) -> Option<EvalReflectionParameterTypeMetadata> {
    let variants = parameter_type.variants();
    if variants.is_empty() {
        return None;
    }
    let allows_null = parameter_type.allows_null();
    let mut types = variants
        .iter()
        .map(|variant| eval_reflection_named_type_variant_metadata(variant, false))
        .collect::<Option<Vec<_>>>()?;
    if types.len() == 1 {
        let mut named = types.pop()?;
        named.allows_null = allows_null;
        return Some(EvalReflectionParameterTypeMetadata {
            kind: EvalReflectionParameterTypeKind::Named(named),
        });
    }
    if parameter_type.is_intersection() {
        return Some(EvalReflectionParameterTypeMetadata {
            kind: EvalReflectionParameterTypeKind::Intersection(
                EvalReflectionIntersectionTypeMetadata { types },
            ),
        });
    }
    Some(EvalReflectionParameterTypeMetadata {
        kind: EvalReflectionParameterTypeKind::Union(EvalReflectionUnionTypeMetadata {
            types,
            allows_null,
        }),
    })
}

/// Converts one eval parameter type variant into `ReflectionNamedType` metadata.
fn eval_reflection_named_type_variant_metadata(
    variant: &EvalParameterTypeVariant,
    allows_null: bool,
) -> Option<EvalReflectionNamedTypeMetadata> {
    match variant {
        EvalParameterTypeVariant::Array => {
            Some(eval_reflection_builtin_named_type("array", allows_null))
        }
        EvalParameterTypeVariant::Bool => {
            Some(eval_reflection_builtin_named_type("bool", allows_null))
        }
        EvalParameterTypeVariant::Callable => {
            Some(eval_reflection_builtin_named_type("callable", allows_null))
        }
        EvalParameterTypeVariant::Class(name) => Some(EvalReflectionNamedTypeMetadata {
            name: name.clone(),
            allows_null,
            is_builtin: false,
        }),
        EvalParameterTypeVariant::Float => {
            Some(eval_reflection_builtin_named_type("float", allows_null))
        }
        EvalParameterTypeVariant::Int => {
            Some(eval_reflection_builtin_named_type("int", allows_null))
        }
        EvalParameterTypeVariant::Iterable => {
            Some(eval_reflection_builtin_named_type("iterable", allows_null))
        }
        EvalParameterTypeVariant::Mixed => Some(eval_reflection_builtin_named_type("mixed", true)),
        EvalParameterTypeVariant::Object => {
            Some(eval_reflection_builtin_named_type("object", allows_null))
        }
        EvalParameterTypeVariant::String => {
            Some(eval_reflection_builtin_named_type("string", allows_null))
        }
    }
}

/// Builds metadata for one builtin eval `ReflectionNamedType`.
fn eval_reflection_builtin_named_type(
    name: &str,
    allows_null: bool,
) -> EvalReflectionNamedTypeMetadata {
    EvalReflectionNamedTypeMetadata {
        name: name.to_string(),
        allows_null,
        is_builtin: true,
    }
}

/// Packs ReflectionMethod/ReflectionProperty predicate flags for the runtime owner factory.
fn eval_reflection_member_flags(
    visibility: EvalVisibility,
    is_static: bool,
    is_final: bool,
    is_abstract: bool,
) -> u64 {
    let mut flags = 0;
    if is_static {
        flags |= EVAL_REFLECTION_MEMBER_FLAG_STATIC;
    }
    match visibility {
        EvalVisibility::Public => flags |= EVAL_REFLECTION_MEMBER_FLAG_PUBLIC,
        EvalVisibility::Protected => flags |= EVAL_REFLECTION_MEMBER_FLAG_PROTECTED,
        EvalVisibility::Private => flags |= EVAL_REFLECTION_MEMBER_FLAG_PRIVATE,
    }
    if is_final {
        flags |= EVAL_REFLECTION_MEMBER_FLAG_FINAL;
    }
    if is_abstract {
        flags |= EVAL_REFLECTION_MEMBER_FLAG_ABSTRACT;
    }
    flags
}

/// Packs ReflectionParameter predicate flags for the runtime parameter factory.
fn eval_reflection_parameter_flags(parameter: &EvalReflectionParameterMetadata) -> u64 {
    let mut flags = 0;
    if parameter.is_optional {
        flags |= EVAL_REFLECTION_PARAMETER_FLAG_OPTIONAL;
    }
    if parameter.is_variadic {
        flags |= EVAL_REFLECTION_PARAMETER_FLAG_VARIADIC;
    }
    if parameter.is_passed_by_reference {
        flags |= EVAL_REFLECTION_PARAMETER_FLAG_BY_REF;
    }
    if parameter.has_type {
        flags |= EVAL_REFLECTION_PARAMETER_FLAG_HAS_TYPE;
    }
    if parameter.default_value.is_some() {
        flags |= EVAL_REFLECTION_PARAMETER_FLAG_HAS_DEFAULT_VALUE;
    }
    flags
}

/// Packs ReflectionNamedType predicate flags for the runtime type factory.
fn eval_reflection_named_type_flags(type_metadata: &EvalReflectionNamedTypeMetadata) -> u64 {
    let mut flags = 0;
    if type_metadata.allows_null {
        flags |= EVAL_REFLECTION_NAMED_TYPE_FLAG_ALLOWS_NULL;
    }
    if type_metadata.is_builtin {
        flags |= EVAL_REFLECTION_NAMED_TYPE_FLAG_BUILTIN;
    }
    flags
}

/// Packs ReflectionUnionType predicate flags for the runtime type factory.
fn eval_reflection_union_type_flags(type_metadata: &EvalReflectionUnionTypeMetadata) -> u64 {
    if type_metadata.allows_null {
        EVAL_REFLECTION_NAMED_TYPE_FLAG_ALLOWS_NULL
    } else {
        0
    }
}

/// Converts one reflection constructor argument to a Rust UTF-8 string.
fn eval_reflection_string_arg(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Maps a PHP reflection owner class name to the helper owner kind.
fn reflection_owner_kind(class_name: &str) -> Option<u64> {
    match class_name
        .trim_start_matches('\\')
        .to_ascii_lowercase()
        .as_str()
    {
        "reflectionclass" => Some(EVAL_REFLECTION_OWNER_CLASS),
        "reflectionmethod" => Some(EVAL_REFLECTION_OWNER_METHOD),
        "reflectionproperty" => Some(EVAL_REFLECTION_OWNER_PROPERTY),
        "reflectionclassconstant" => Some(EVAL_REFLECTION_OWNER_CLASS_CONSTANT),
        "reflectionenumunitcase" => Some(EVAL_REFLECTION_OWNER_ENUM_UNIT_CASE),
        "reflectionenumbackedcase" => Some(EVAL_REFLECTION_OWNER_ENUM_BACKED_CASE),
        _ => None,
    }
}
