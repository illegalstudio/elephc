//! Purpose:
//! Handles eval-aware construction of builtin reflection owner objects.
//! These objects need private metadata slots populated from eval-declared class
//! metadata, which ordinary public property writes cannot express.
//!
//! Called from:
//! - `crate::interpreter::expressions::eval_expr()` for `new Reflection*`.
//!
//! Key details:
//! - Eval-declared classes/interfaces/traits/enums are materialized from dynamic metadata.
//! - Generated/AOT targets use focused runtime hooks for supported point lookups.

use super::*;

const EVAL_REFLECTION_CLASS_FLAG_FINAL: u64 = 1;
const EVAL_REFLECTION_CLASS_FLAG_ABSTRACT: u64 = 2;
const EVAL_REFLECTION_CLASS_FLAG_INTERFACE: u64 = 4;
const EVAL_REFLECTION_CLASS_FLAG_TRAIT: u64 = 8;
const EVAL_REFLECTION_CLASS_FLAG_ENUM: u64 = 16;
const EVAL_REFLECTION_CLASS_FLAG_READONLY: u64 = 32;
const EVAL_REFLECTION_CLASS_FLAG_INSTANTIABLE: u64 = 64;
const EVAL_REFLECTION_CLASS_FLAG_CLONEABLE: u64 = 128;
const EVAL_REFLECTION_CLASS_FLAG_INTERNAL: u64 = 256;
const EVAL_REFLECTION_CLASS_FLAG_USER_DEFINED: u64 = 512;
const EVAL_REFLECTION_CLASS_FLAG_ITERABLE: u64 = 1024;
const EVAL_REFLECTION_CLASS_FLAG_ANONYMOUS: u64 = 2048;
const EVAL_REFLECTION_MEMBER_FLAG_STATIC: u64 = 1;
const EVAL_REFLECTION_MEMBER_FLAG_PUBLIC: u64 = 2;
const EVAL_REFLECTION_MEMBER_FLAG_PROTECTED: u64 = 4;
const EVAL_REFLECTION_MEMBER_FLAG_PRIVATE: u64 = 8;
const EVAL_REFLECTION_MEMBER_FLAG_FINAL: u64 = 16;
const EVAL_REFLECTION_MEMBER_FLAG_ABSTRACT: u64 = 32;
const EVAL_REFLECTION_MEMBER_FLAG_READONLY: u64 = 64;
const EVAL_REFLECTION_MEMBER_FLAG_ENUM_CASE: u64 = 128;
const EVAL_REFLECTION_MEMBER_FLAG_HAS_DEFAULT_VALUE: u64 = 256;
const EVAL_REFLECTION_MEMBER_FLAG_PROMOTED: u64 = 512;
const EVAL_REFLECTION_PARAMETER_FLAG_OPTIONAL: u64 = 1;
const EVAL_REFLECTION_PARAMETER_FLAG_VARIADIC: u64 = 2;
const EVAL_REFLECTION_PARAMETER_FLAG_BY_REF: u64 = 4;
const EVAL_REFLECTION_PARAMETER_FLAG_HAS_TYPE: u64 = 8;
const EVAL_REFLECTION_PARAMETER_FLAG_HAS_DEFAULT_VALUE: u64 = 16;
const EVAL_REFLECTION_PARAMETER_FLAG_PROMOTED: u64 = 32;
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
    is_readonly: bool,
    is_promoted: bool,
    modifiers: u64,
    type_metadata: Option<EvalReflectionParameterTypeMetadata>,
    return_type_metadata: Option<EvalReflectionParameterTypeMetadata>,
    default_value: Option<EvalExpr>,
    required_parameter_count: usize,
    parameters: Vec<EvalReflectionParameterMetadata>,
}

/// Eval metadata needed to materialize one `ReflectionParameter` object.
struct EvalReflectionParameterMetadata {
    name: String,
    declaring_class_name: Option<String>,
    declaring_function: Option<EvalReflectionDeclaringFunctionMetadata>,
    attributes: Vec<EvalAttribute>,
    position: usize,
    is_optional: bool,
    is_variadic: bool,
    is_passed_by_reference: bool,
    is_promoted: bool,
    has_type: bool,
    type_metadata: Option<EvalReflectionParameterTypeMetadata>,
    default_value: Option<EvalExpr>,
}

/// Eval metadata needed for `ReflectionParameter::getDeclaringFunction()`.
#[derive(Clone)]
struct EvalReflectionDeclaringFunctionMetadata {
    name: String,
    declaring_class_name: Option<String>,
    attributes: Vec<EvalAttribute>,
    flags: u64,
    required_parameter_count: usize,
}

/// Eval metadata needed to materialize one parameter `ReflectionType` object.
#[derive(Clone)]
struct EvalReflectionParameterTypeMetadata {
    kind: EvalReflectionParameterTypeKind,
}

/// Eval reflection parameter type object variants.
#[derive(Clone)]
enum EvalReflectionParameterTypeKind {
    Named(EvalReflectionNamedTypeMetadata),
    Union(EvalReflectionUnionTypeMetadata),
    Intersection(EvalReflectionIntersectionTypeMetadata),
}

/// Eval metadata needed to materialize one `ReflectionNamedType` object.
#[derive(Clone)]
struct EvalReflectionNamedTypeMetadata {
    name: String,
    allows_null: bool,
    is_builtin: bool,
}

/// Registered ReflectionFunctionAbstract target metadata for simple method dispatch.
enum EvalReflectionFunctionMethodTarget {
    Function {
        name: String,
        is_variadic: bool,
        return_type_metadata: Option<EvalReflectionParameterTypeMetadata>,
    },
    Method {
        name: String,
        is_variadic: bool,
        return_type_metadata: Option<EvalReflectionParameterTypeMetadata>,
    },
}

/// Eval metadata needed to materialize one `ReflectionUnionType` object.
#[derive(Clone)]
struct EvalReflectionUnionTypeMetadata {
    types: Vec<EvalReflectionNamedTypeMetadata>,
    allows_null: bool,
}

/// Eval metadata needed to materialize one `ReflectionIntersectionType` object.
#[derive(Clone)]
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
        Some(EVAL_REFLECTION_OWNER_FUNCTION) => {
            eval_reflection_function_new(evaluated_args, context, values)
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

/// Handles eval-backed `ReflectionClass::isSubclassOf()` calls.
pub(in crate::interpreter) fn eval_reflection_class_is_subclass_of_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("isSubclassOf") {
        return Ok(None);
    }
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let args = bind_evaluated_function_args(&[String::from("class")], evaluated_args)?;
    let target_name = eval_reflection_string_arg(args[0], values)?;
    if !eval_reflection_class_like_exists(&target_name, context)
        && !values.class_exists(&target_name)?
        && !values.interface_exists(&target_name)?
        && !values.trait_exists(&target_name)?
        && !values.enum_exists(&target_name)?
    {
        return eval_throw_reflection_exception(
            &format!("Class \"{}\" does not exist", target_name),
            context,
            values,
        );
    }
    let result = if eval_reflection_class_like_exists(&reflected_name, context) {
        eval_reflection_class_is_subclass_of_name(&reflected_name, &target_name, context)
    } else {
        let reflected_class = values.string(&reflected_name)?;
        let result = values.object_is_a(reflected_class, &target_name, true)?;
        values.release(reflected_class)?;
        result
    };
    values.bool_value(result).map(Some)
}

/// Handles eval-backed `ReflectionClass::isInstance()` calls.
pub(in crate::interpreter) fn eval_reflection_class_is_instance_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("isInstance") {
        return Ok(None);
    }
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let args = bind_evaluated_function_args(&[String::from("object")], evaluated_args)?;
    let object = args[0];
    if values.type_tag(object)? != EVAL_TAG_OBJECT {
        return Err(EvalStatus::RuntimeFatal);
    }
    let result = dynamic_object_is_a(object, &reflected_name, false, context, values)?
        .map_or_else(|| values.object_is_a(object, &reflected_name, false), Ok)?;
    values.bool_value(result).map(Some)
}

/// Handles eval-backed `ReflectionClass::hasMethod()` calls.
pub(in crate::interpreter) fn eval_reflection_class_has_method_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("hasMethod") {
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
    let exists =
        if let Some(metadata) = eval_reflection_class_like_attributes(&reflected_name, context) {
            metadata
                .method_names
                .iter()
                .any(|name| name.eq_ignore_ascii_case(&requested_name))
        } else {
            eval_reflection_aot_method_metadata_if_exists(&reflected_name, &requested_name, values)?
                .is_some()
        };
    values.bool_value(exists).map(Some)
}

/// Handles eval-backed `ReflectionClass::hasProperty()` calls.
pub(in crate::interpreter) fn eval_reflection_class_has_property_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("hasProperty") {
        return Ok(None);
    }
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let args = bind_evaluated_function_args(&[String::from("name")], evaluated_args)?;
    let property_name = eval_reflection_string_arg(args[0], values)?;
    let exists = if let Some(metadata) =
        eval_reflection_class_like_attributes(&reflected_name, context)
    {
        metadata
            .property_names
            .iter()
            .any(|name| name == &property_name)
    } else {
        eval_reflection_aot_property_metadata_if_exists(&reflected_name, &property_name, values)?
            .is_some()
    };
    values.bool_value(exists).map(Some)
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

/// Handles eval-backed `ReflectionClass::getDefaultProperties()` calls.
pub(in crate::interpreter) fn eval_reflection_class_get_default_properties_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("getDefaultProperties") {
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
    let property_names = eval_reflection_default_property_names(&reflected_name, context);
    let mut result = values.assoc_new(property_names.len())?;
    for name in property_names {
        let Some(member) = eval_reflection_property_metadata(&reflected_name, &name, context)
        else {
            continue;
        };
        let Some(default) = member.default_value.as_ref() else {
            continue;
        };
        let key = values.string(&name)?;
        let value = eval_method_parameter_default(default, context, values)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(Some(result))
}

/// Handles eval-backed `ReflectionClass::getStaticProperties()` calls.
pub(in crate::interpreter) fn eval_reflection_class_get_static_properties_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("getStaticProperties") {
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
    let property_names = eval_reflection_static_property_names(&reflected_name, context);
    let mut result = values.assoc_new(property_names.len())?;
    for name in property_names {
        let Some(value) =
            eval_reflection_static_property_value(&reflected_name, &name, context, values)?
        else {
            continue;
        };
        let key = values.string(&name)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(Some(result))
}

/// Handles eval-backed `ReflectionClass::getStaticPropertyValue()` calls.
pub(in crate::interpreter) fn eval_reflection_class_get_static_property_value_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("getStaticPropertyValue") {
        return Ok(None);
    }
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let (property_name, default_value) =
        eval_reflection_static_property_value_args(evaluated_args)?;
    let property_name = eval_reflection_string_arg(property_name, values)?;
    if let Some(value) =
        eval_reflection_static_property_value(&reflected_name, &property_name, context, values)?
    {
        return Ok(Some(value));
    }
    if let Some(default_value) = default_value {
        return Ok(Some(default_value));
    }
    eval_throw_reflection_exception(
        &format!(
            "Property {}::${} does not exist",
            reflected_name, property_name
        ),
        context,
        values,
    )
}

/// Handles eval-backed `ReflectionClass::setStaticPropertyValue()` calls.
pub(in crate::interpreter) fn eval_reflection_class_set_static_property_value_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("setStaticPropertyValue") {
        return Ok(None);
    }
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let args = bind_evaluated_function_args(
        &[String::from("name"), String::from("value")],
        evaluated_args,
    )?;
    let property_name = eval_reflection_string_arg(args[0], values)?;
    let Some(member) = eval_reflection_property_metadata(&reflected_name, &property_name, context)
    else {
        return eval_reflection_static_property_missing_for_set(
            &reflected_name,
            &property_name,
            context,
            values,
        );
    };
    if !member.is_static {
        return eval_reflection_static_property_missing_for_set(
            &reflected_name,
            &property_name,
            context,
            values,
        );
    }
    let declaring_class = member
        .declaring_class_name
        .as_deref()
        .ok_or(EvalStatus::RuntimeFatal)?;
    if let Some(replaced) = context.set_static_property(declaring_class, &property_name, args[1]) {
        values.release(replaced)?;
    }
    values.null().map(Some)
}

/// Handles eval-backed `ReflectionMethod::invoke()` and `invokeArgs()` calls.
pub(in crate::interpreter) fn eval_reflection_method_invoke_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let is_invoke = method_name.eq_ignore_ascii_case("invoke");
    let is_invoke_args = method_name.eq_ignore_ascii_case("invokeArgs");
    if !is_invoke && !is_invoke_args {
        return Ok(None);
    }
    let Some((declaring_class, reflected_method)) = context
        .eval_reflection_method(identity)
        .map(|(declaring_class, method)| (declaring_class.to_string(), method.to_string()))
    else {
        return Ok(None);
    };
    let (object, method_args) = if is_invoke {
        eval_reflection_method_invoke_args(evaluated_args)?
    } else {
        eval_reflection_method_invoke_args_array(evaluated_args, values)?
    };
    eval_reflection_method_invoke_dispatch(
        &declaring_class,
        &reflected_method,
        object,
        method_args,
        context,
        values,
    )
    .map(Some)
}

/// Handles eval-backed `ReflectionFunction::invoke()` and `invokeArgs()` calls.
pub(in crate::interpreter) fn eval_reflection_function_invoke_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let is_invoke = method_name.eq_ignore_ascii_case("invoke");
    let is_invoke_args = method_name.eq_ignore_ascii_case("invokeArgs");
    if !is_invoke && !is_invoke_args {
        return Ok(None);
    }
    let Some(function_name) = context
        .eval_reflection_function_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let function_args = if is_invoke {
        evaluated_args
            .into_iter()
            .map(eval_reflection_method_forwarded_value_arg)
            .collect()
    } else {
        eval_reflection_function_invoke_args_array(evaluated_args, values)?
    };
    eval_reflection_function_invoke_dispatch(&function_name, function_args, context, values)
        .map(Some)
}

/// Handles eval-backed ReflectionFunctionAbstract name/origin metadata calls.
pub(in crate::interpreter) fn eval_reflection_function_method_metadata_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(target) = eval_reflection_function_method_target(identity, context) else {
        return Ok(None);
    };
    let method_key = method_name.to_ascii_lowercase();
    match method_key.as_str() {
        "getshortname" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            values
                .string(&eval_reflection_function_method_short_name(&target))
                .map(Some)
        }
        "getnamespacename" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            values
                .string(&eval_reflection_function_method_namespace_name(&target))
                .map(Some)
        }
        "innamespace" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            values
                .bool_value(!eval_reflection_function_method_namespace_name(&target).is_empty())
                .map(Some)
        }
        "isinternal"
        | "isclosure"
        | "isdeprecated"
        | "returnsreference"
        | "isgenerator"
        | "hastentativereturntype" => eval_reflection_false_metadata_result(evaluated_args, values),
        "hasreturntype" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            values
                .bool_value(eval_reflection_function_method_return_type(&target).is_some())
                .map(Some)
        }
        "isuserdefined" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            values.bool_value(true).map(Some)
        }
        "isvariadic" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            values
                .bool_value(eval_reflection_function_method_is_variadic(&target))
                .map(Some)
        }
        "isdisabled" => match target {
            EvalReflectionFunctionMethodTarget::Function { .. } => {
                eval_reflection_false_metadata_result(evaluated_args, values)
            }
            EvalReflectionFunctionMethodTarget::Method { .. } => Ok(None),
        },
        "getreturntype" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            match eval_reflection_function_method_return_type(&target) {
                Some(type_metadata) => {
                    eval_reflection_type_object_result(type_metadata, values).map(Some)
                }
                None => values.null().map(Some),
            }
        }
        "gettentativereturntype" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            values.null().map(Some)
        }
        _ => Ok(None),
    }
}

/// Handles eval-backed `ReflectionMethod::hasPrototype()` and `getPrototype()` calls.
pub(in crate::interpreter) fn eval_reflection_method_prototype_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let is_has_prototype = method_name.eq_ignore_ascii_case("hasPrototype");
    let is_get_prototype = method_name.eq_ignore_ascii_case("getPrototype");
    if !is_has_prototype && !is_get_prototype {
        return Ok(None);
    }
    let Some((declaring_class, reflected_method)) =
        context
            .eval_reflection_method(identity)
            .map(|(declaring_class, method_name)| {
                (declaring_class.to_string(), method_name.to_string())
            })
    else {
        return Ok(None);
    };
    eval_reflection_bind_no_args(evaluated_args)?;
    let Some((prototype_class, prototype_method)) =
        eval_reflection_method_prototype_target(&declaring_class, &reflected_method, context)
    else {
        if is_has_prototype {
            return values.bool_value(false).map(Some);
        }
        return eval_throw_reflection_exception(
            &format!(
                "Method {}::{} does not have a prototype",
                declaring_class, reflected_method
            ),
            context,
            values,
        );
    };
    if is_has_prototype {
        return values.bool_value(true).map(Some);
    }
    let Some(metadata) =
        eval_reflection_method_metadata(&prototype_class, &prototype_method, context)
    else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_reflection_member_object_result(
        EVAL_REFLECTION_OWNER_METHOD,
        &prototype_method,
        &metadata,
        context,
        values,
    )
    .map(Some)
}

/// Handles PHP's no-op `ReflectionMethod/Property::setAccessible()` calls.
pub(in crate::interpreter) fn eval_reflection_set_accessible_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("setAccessible") {
        return Ok(None);
    }
    if context.eval_reflection_method(identity).is_none()
        && context.eval_reflection_property(identity).is_none()
    {
        return Ok(None);
    }
    let _ = bind_evaluated_function_args(&[String::from("accessible")], evaluated_args)?;
    values.null().map(Some)
}

/// Handles eval-backed `ReflectionProperty::getValue()` calls.
pub(in crate::interpreter) fn eval_reflection_property_get_value_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("getValue") {
        return Ok(None);
    }
    let Some((declaring_class, property_name)) =
        context
            .eval_reflection_property(identity)
            .map(|(declaring_class, property_name)| {
                (declaring_class.to_string(), property_name.to_string())
            })
    else {
        return Ok(None);
    };
    let object = eval_reflection_property_get_value_arg(evaluated_args)?;
    let Some(member) = eval_reflection_property_metadata(&declaring_class, &property_name, context)
    else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if member.is_static {
        return eval_reflection_static_property_value(
            &declaring_class,
            &property_name,
            context,
            values,
        )?
        .map(Some)
        .ok_or(EvalStatus::RuntimeFatal);
    }
    let object = object.ok_or(EvalStatus::RuntimeFatal)?;
    eval_reflection_instance_property_get_value(
        &declaring_class,
        &property_name,
        object,
        context,
        values,
    )
    .map(Some)
}

/// Handles eval-backed `ReflectionProperty::setValue()` calls.
pub(in crate::interpreter) fn eval_reflection_property_set_value_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("setValue") {
        return Ok(None);
    }
    let Some((declaring_class, property_name)) =
        context
            .eval_reflection_property(identity)
            .map(|(declaring_class, property_name)| {
                (declaring_class.to_string(), property_name.to_string())
            })
    else {
        return Ok(None);
    };
    let Some(member) = eval_reflection_property_metadata(&declaring_class, &property_name, context)
    else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let (object_or_value, value) = eval_reflection_property_set_value_args(evaluated_args)?;
    if member.is_static {
        let value = value.unwrap_or(object_or_value);
        let declaring_class = member
            .declaring_class_name
            .as_deref()
            .ok_or(EvalStatus::RuntimeFatal)?;
        if let Some(replaced) = context.set_static_property(declaring_class, &property_name, value)
        {
            values.release(replaced)?;
        }
        return values.null().map(Some);
    }
    let value = value.ok_or(EvalStatus::RuntimeFatal)?;
    eval_reflection_instance_property_set_value(
        &declaring_class,
        &property_name,
        object_or_value,
        value,
        context,
        values,
    )?;
    values.null().map(Some)
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

/// Handles eval-backed `ReflectionClass::getMethods()` and `getProperties()` calls.
pub(in crate::interpreter) fn eval_reflection_class_get_members_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let owner_kind = if method_name.eq_ignore_ascii_case("getMethods") {
        EVAL_REFLECTION_OWNER_METHOD
    } else if method_name.eq_ignore_ascii_case("getProperties") {
        EVAL_REFLECTION_OWNER_PROPERTY
    } else {
        return Ok(None);
    };
    let filter = eval_reflection_member_filter(evaluated_args, values)?;
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    if let Some(metadata) = eval_reflection_class_like_attributes(&reflected_name, context) {
        let names = if owner_kind == EVAL_REFLECTION_OWNER_METHOD {
            metadata.method_names
        } else {
            metadata.property_names
        };
        return eval_reflection_member_object_array_result(
            owner_kind,
            &reflected_name,
            &names,
            filter,
            context,
            values,
        )
        .map(Some);
    }
    let names = eval_reflection_aot_member_names(owner_kind, &reflected_name, values)?;
    eval_reflection_aot_member_object_array_result(
        owner_kind,
        &reflected_name,
        &names,
        filter,
        context,
        values,
    )
    .map(Some)
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
        if owner_kind == EVAL_REFLECTION_OWNER_METHOD
            && !eval_reflection_class_like_exists(&reflected_name, context)
        {
            if let Some(member) = eval_reflection_aot_method_metadata_if_exists(
                &reflected_name,
                &requested_name,
                values,
            )? {
                let member_name = requested_name.to_ascii_lowercase();
                return eval_reflection_member_object_result(
                    EVAL_REFLECTION_OWNER_METHOD,
                    &member_name,
                    &member,
                    context,
                    values,
                )
                .map(Some);
            }
        }
        if owner_kind == EVAL_REFLECTION_OWNER_PROPERTY
            && !eval_reflection_class_like_exists(&reflected_name, context)
        {
            if let Some(member) = eval_reflection_aot_property_metadata_if_exists(
                &reflected_name,
                &requested_name,
                values,
            )? {
                return eval_reflection_member_object_result(
                    EVAL_REFLECTION_OWNER_PROPERTY,
                    &requested_name,
                    &member,
                    context,
                    values,
                )
                .map(Some);
            }
        }
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
    eval_reflection_member_object_result(owner_kind, &member_name, &member, context, values)
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
    let (declaring_class_name, attributes, visibility, is_final, is_enum_case) =
        eval_reflection_class_constant_metadata(reflected_name, constant_name, context)
            .ok_or(EvalStatus::RuntimeFatal)?;
    let constant_value = eval_reflection_constant_value(reflected_name, constant_name, context)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let mut flags = eval_reflection_member_flags(visibility, false, is_final, false, false);
    if is_enum_case {
        flags |= EVAL_REFLECTION_MEMBER_FLAG_ENUM_CASE;
    }
    let modifiers = eval_reflection_class_constant_modifiers(visibility, is_final);
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
        None,
        None,
        flags,
        modifiers,
        0,
        Some(constant_value),
        None,
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
    let reflected_name = context
        .resolve_class_like_name(&class_name)
        .unwrap_or_else(|| class_name.trim_start_matches('\\').to_string());
    let Some(metadata) = eval_reflection_class_like_attributes(&reflected_name, context) else {
        let Some((flags, modifiers)) = eval_reflection_aot_class_flags(&reflected_name, values)?
        else {
            return Ok(None);
        };
        return eval_reflection_owner_object(
            EVAL_REFLECTION_OWNER_CLASS,
            &reflected_name,
            &[],
            &[],
            &[],
            &[],
            &[],
            None,
            &[],
            None,
            None,
            flags,
            modifiers,
            0,
            None,
            None,
            context,
            values,
        )
        .map(Some);
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
        None,
        None,
        metadata.flags,
        metadata.modifiers,
        0,
        None,
        None,
        context,
        values,
    )
    .map(Some)
}

/// Returns generated/AOT class flags for synthetic ReflectionClass fallback objects.
fn eval_reflection_aot_class_flags(
    class_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(u64, u64)>, EvalStatus> {
    let runtime_class_name = class_name.trim_start_matches('\\');
    let is_class = values.class_exists(runtime_class_name)?;
    let is_interface = values.interface_exists(runtime_class_name)?;
    let is_trait = values.trait_exists(runtime_class_name)?;
    let is_enum = values.enum_exists(runtime_class_name)?;
    if !(is_class || is_interface || is_trait || is_enum) {
        return Ok(None);
    }
    let mut flags = 0;
    if eval_reflection_class_like_is_internal(runtime_class_name) {
        flags |= EVAL_REFLECTION_CLASS_FLAG_INTERNAL;
    } else {
        flags |= EVAL_REFLECTION_CLASS_FLAG_USER_DEFINED;
    }
    if is_interface {
        flags |= EVAL_REFLECTION_CLASS_FLAG_INTERFACE;
    }
    if is_trait {
        flags |= EVAL_REFLECTION_CLASS_FLAG_TRAIT;
    }
    if is_enum {
        flags |= EVAL_REFLECTION_CLASS_FLAG_FINAL | EVAL_REFLECTION_CLASS_FLAG_ENUM;
    }
    if eval_reflection_builtin_class_is_iterable(runtime_class_name) {
        flags |= EVAL_REFLECTION_CLASS_FLAG_ITERABLE;
    }
    let modifiers = if is_enum { 32 } else { 0 };
    Ok(Some((flags, modifiers)))
}

/// Builds an eval-backed `ReflectionFunction` object for eval or registered native functions.
fn eval_reflection_function_new(
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let args = bind_evaluated_function_args(&[String::from("function")], evaluated_args)?;
    let requested_name = eval_reflection_string_arg(args[0], values)?;
    let lookup_name = requested_name.trim_start_matches('\\').to_ascii_lowercase();
    if let Some(function) = context.function(&lookup_name).cloned() {
        let required_parameter_count = eval_reflection_required_parameter_count(
            function.parameter_defaults(),
            function.parameter_is_variadic(),
        );
        let parameters = eval_reflection_function_parameters(
            function.name(),
            function.params(),
            function.attributes().to_vec(),
            function.parameter_attributes(),
            function.parameter_types(),
            function.parameter_defaults(),
            function.parameter_is_by_ref(),
            function.parameter_is_variadic(),
        );
        return eval_reflection_function_object_result(
            function.name(),
            function.attributes(),
            &parameters,
            required_parameter_count,
            context,
            values,
        )
        .map(Some);
    }
    if let Some(function) = context.native_function(&lookup_name) {
        let reflected_name = requested_name.trim_start_matches('\\');
        let parameter_names = eval_reflection_native_function_parameter_names(&function);
        let parameter_attributes = vec![Vec::new(); parameter_names.len()];
        let parameter_types: Vec<Option<EvalParameterType>> = vec![None; parameter_names.len()];
        let parameter_defaults = vec![None; parameter_names.len()];
        let parameter_is_by_ref = vec![false; parameter_names.len()];
        let parameter_is_variadic = vec![false; parameter_names.len()];
        let required_parameter_count =
            eval_reflection_required_parameter_count(&parameter_defaults, &parameter_is_variadic);
        let parameters = eval_reflection_function_parameters(
            reflected_name,
            &parameter_names,
            Vec::new(),
            &parameter_attributes,
            &parameter_types,
            &parameter_defaults,
            &parameter_is_by_ref,
            &parameter_is_variadic,
        );
        return eval_reflection_function_object_result(
            reflected_name,
            &[],
            &parameters,
            required_parameter_count,
            context,
            values,
        )
        .map(Some);
    }
    Ok(None)
}

/// Returns parameter names for a registered native function, filling missing bridge names.
fn eval_reflection_native_function_parameter_names(function: &NativeFunction) -> Vec<String> {
    (0..function.param_count())
        .map(|index| {
            function
                .param_names()
                .get(index)
                .filter(|name| !name.is_empty())
                .cloned()
                .unwrap_or_else(|| format!("arg{}", index))
        })
        .collect()
}

/// Builds one `ReflectionFunction` object from retained eval function metadata.
fn eval_reflection_function_object_result(
    function_name: &str,
    attributes: &[EvalAttribute],
    parameters: &[EvalReflectionParameterMetadata],
    required_parameter_count: usize,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_reflection_owner_object(
        EVAL_REFLECTION_OWNER_FUNCTION,
        function_name,
        attributes,
        &[],
        &[],
        &[],
        &[],
        None,
        parameters,
        None,
        None,
        0,
        required_parameter_count as u64,
        0,
        None,
        None,
        context,
        values,
    )
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
        let method_name = eval_reflection_string_arg(args[1], values)?;
        if let Some(method) =
            eval_reflection_aot_method_metadata_if_exists(&class_name, &method_name, values)?
        {
            let method_name = method_name.to_ascii_lowercase();
            return eval_reflection_member_object_result(
                EVAL_REFLECTION_OWNER_METHOD,
                &method_name,
                &method,
                context,
                values,
            )
            .map(Some);
        }
        return Ok(None);
    }
    let requested_method_name = eval_reflection_string_arg(args[1], values)?;
    let method_name = eval_reflection_member_name(
        EVAL_REFLECTION_OWNER_METHOD,
        &class_name,
        &requested_method_name,
        context,
    )
    .ok_or(EvalStatus::RuntimeFatal)?;
    let method = eval_reflection_method_metadata(&class_name, &method_name, context)
        .ok_or(EvalStatus::RuntimeFatal)?;
    eval_reflection_member_object_result(
        EVAL_REFLECTION_OWNER_METHOD,
        &method_name,
        &method,
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
        let property_name = eval_reflection_string_arg(args[1], values)?;
        if let Some(property) =
            eval_reflection_aot_property_metadata_if_exists(&class_name, &property_name, values)?
        {
            return eval_reflection_member_object_result(
                EVAL_REFLECTION_OWNER_PROPERTY,
                &property_name,
                &property,
                context,
                values,
            )
            .map(Some);
        }
        return Ok(None);
    }
    let property_name = eval_reflection_string_arg(args[1], values)?;
    let property = eval_reflection_property_metadata(&class_name, &property_name, context)
        .ok_or(EvalStatus::RuntimeFatal)?;
    eval_reflection_member_object_result(
        EVAL_REFLECTION_OWNER_PROPERTY,
        &property_name,
        &property,
        context,
        values,
    )
    .map(Some)
}

/// Returns generated AOT ReflectionMethod metadata when the runtime table has a matching row.
fn eval_reflection_aot_method_metadata_if_exists(
    class_name: &str,
    method_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalReflectionMemberMetadata>, EvalStatus> {
    let runtime_class_name = class_name.trim_start_matches('\\');
    let Some(flags) = values.reflection_method_flags(runtime_class_name, method_name)? else {
        return Ok(None);
    };
    Ok(Some(eval_reflection_aot_method_metadata(
        runtime_class_name,
        flags,
    )))
}

/// Returns generated/AOT method dispatch metadata for interpreter-only runtime decisions.
pub(in crate::interpreter) fn eval_aot_method_dispatch_metadata(
    class_name: &str,
    method_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(EvalVisibility, bool, bool)>, EvalStatus> {
    Ok(
        eval_reflection_aot_method_metadata_if_exists(class_name, method_name, values)?
            .map(|member| (member.visibility, member.is_static, member.is_abstract)),
    )
}

/// Converts AOT method flag metadata into the eval ReflectionMethod shape.
fn eval_reflection_aot_method_metadata(
    class_name: &str,
    flags: u64,
) -> EvalReflectionMemberMetadata {
    let visibility = if flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE != 0 {
        EvalVisibility::Private
    } else if flags & EVAL_REFLECTION_MEMBER_FLAG_PROTECTED != 0 {
        EvalVisibility::Protected
    } else {
        EvalVisibility::Public
    };
    EvalReflectionMemberMetadata {
        declaring_class_name: Some(class_name.trim_start_matches('\\').to_string()),
        attributes: Vec::new(),
        visibility,
        is_static: flags & EVAL_REFLECTION_MEMBER_FLAG_STATIC != 0,
        is_final: flags & EVAL_REFLECTION_MEMBER_FLAG_FINAL != 0,
        is_abstract: flags & EVAL_REFLECTION_MEMBER_FLAG_ABSTRACT != 0,
        is_readonly: false,
        is_promoted: false,
        modifiers: eval_reflection_method_modifiers_from_flags(flags),
        type_metadata: None,
        return_type_metadata: None,
        default_value: None,
        required_parameter_count: 0,
        parameters: Vec::new(),
    }
}

/// Returns generated AOT ReflectionProperty metadata when the runtime table has a matching row.
fn eval_reflection_aot_property_metadata_if_exists(
    class_name: &str,
    property_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalReflectionMemberMetadata>, EvalStatus> {
    let runtime_class_name = class_name.trim_start_matches('\\');
    let Some(flags) = values.reflection_property_flags(runtime_class_name, property_name)? else {
        return Ok(None);
    };
    Ok(Some(eval_reflection_aot_property_metadata(
        runtime_class_name,
        flags,
    )))
}

/// Converts AOT property flag metadata into the eval ReflectionProperty shape.
fn eval_reflection_aot_property_metadata(
    class_name: &str,
    flags: u64,
) -> EvalReflectionMemberMetadata {
    let visibility = if flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE != 0 {
        EvalVisibility::Private
    } else if flags & EVAL_REFLECTION_MEMBER_FLAG_PROTECTED != 0 {
        EvalVisibility::Protected
    } else {
        EvalVisibility::Public
    };
    let is_static = flags & EVAL_REFLECTION_MEMBER_FLAG_STATIC != 0;
    let is_final = flags & EVAL_REFLECTION_MEMBER_FLAG_FINAL != 0;
    let is_abstract = flags & EVAL_REFLECTION_MEMBER_FLAG_ABSTRACT != 0;
    let is_readonly = flags & EVAL_REFLECTION_MEMBER_FLAG_READONLY != 0;
    EvalReflectionMemberMetadata {
        declaring_class_name: Some(class_name.trim_start_matches('\\').to_string()),
        attributes: Vec::new(),
        visibility,
        is_static,
        is_final,
        is_abstract,
        is_readonly,
        is_promoted: flags & EVAL_REFLECTION_MEMBER_FLAG_PROMOTED != 0,
        modifiers: eval_reflection_property_modifiers(
            visibility,
            is_static,
            is_final,
            is_abstract,
            is_readonly,
            false,
        ),
        type_metadata: None,
        return_type_metadata: None,
        default_value: None,
        required_parameter_count: 0,
        parameters: Vec::new(),
    }
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
    let (declaring_class_name, attributes, visibility, is_final, is_enum_case) =
        eval_reflection_class_constant_metadata(&class_name, &constant_name, context)
            .ok_or(EvalStatus::RuntimeFatal)?;
    let constant_value = eval_reflection_constant_value(&class_name, &constant_name, context)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let mut flags = eval_reflection_member_flags(visibility, false, is_final, false, false);
    if is_enum_case {
        flags |= EVAL_REFLECTION_MEMBER_FLAG_ENUM_CASE;
    }
    let modifiers = eval_reflection_class_constant_modifiers(visibility, is_final);
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
        None,
        None,
        flags,
        modifiers,
        0,
        Some(constant_value),
        None,
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
    let case_value = context
        .enum_case(&declaring_class_name, &case_name)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let backing_value = if owner_kind == EVAL_REFLECTION_OWNER_ENUM_BACKED_CASE {
        Some(
            context
                .enum_case_value(&declaring_class_name, &case_name)
                .ok_or(EvalStatus::RuntimeFatal)?,
        )
    } else {
        None
    };
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
        None,
        None,
        0,
        0,
        0,
        Some(case_value),
        backing_value,
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
    type_metadata: Option<&EvalReflectionParameterTypeMetadata>,
    default_value: Option<&EvalExpr>,
    flags: u64,
    modifiers: u64,
    method_modifiers: u64,
    constant_value: Option<RuntimeCellHandle>,
    backing_value: Option<RuntimeCellHandle>,
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
        type_metadata,
        default_value,
        flags,
        modifiers,
        method_modifiers,
        constant_value,
        backing_value,
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
    type_metadata: Option<&EvalReflectionParameterTypeMetadata>,
    default_value: Option<&EvalExpr>,
    flags: u64,
    modifiers: u64,
    method_modifiers: u64,
    constant_value: Option<RuntimeCellHandle>,
    backing_value: Option<RuntimeCellHandle>,
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
            None,
            context,
            values,
        )?
    } else if matches!(
        owner_kind,
        EVAL_REFLECTION_OWNER_METHOD | EVAL_REFLECTION_OWNER_FUNCTION
    ) {
        eval_reflection_parameter_object_array_result(parameter_metadata, context, values)?
    } else if owner_kind == EVAL_REFLECTION_OWNER_PROPERTY {
        match type_metadata {
            Some(type_metadata) => eval_reflection_type_object_result(type_metadata, values)?,
            None => values.null()?,
        }
    } else {
        values.array_new(0)?
    };
    let property_objects = if owner_kind == EVAL_REFLECTION_OWNER_CLASS && include_class_members {
        eval_reflection_member_object_array_result(
            EVAL_REFLECTION_OWNER_PROPERTY,
            reflected_name,
            &property_names,
            None,
            context,
            values,
        )?
    } else if owner_kind == EVAL_REFLECTION_OWNER_PROPERTY {
        match default_value {
            Some(default) => eval_method_parameter_default(default, context, values)?,
            None => values.null()?,
        }
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
    let constructor = eval_reflection_constructor_object_result(
        owner_kind,
        reflected_name,
        include_class_members,
        context,
        values,
    )?;
    let (constant_value_cell, release_constant_value) = match constant_value {
        Some(value) => (value, false),
        None => (values.null()?, true),
    };
    let (backing_value_cell, release_backing_value) = match backing_value {
        Some(value) => (value, false),
        None => (values.null()?, true),
    };
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
        method_modifiers,
        constant_value_cell,
        backing_value_cell,
        constructor,
    )?;
    if owner_kind == EVAL_REFLECTION_OWNER_CLASS {
        let identity = values.object_identity(object)?;
        context.register_eval_reflection_class(identity, reflected_name);
    } else if owner_kind == EVAL_REFLECTION_OWNER_METHOD {
        if let Some(declaring_class) = parent_class_name {
            let identity = values.object_identity(object)?;
            context.register_eval_reflection_method(identity, declaring_class, reflected_name);
        }
    } else if owner_kind == EVAL_REFLECTION_OWNER_FUNCTION {
        let identity = values.object_identity(object)?;
        context.register_eval_reflection_function(identity, reflected_name);
    } else if owner_kind == EVAL_REFLECTION_OWNER_PROPERTY {
        if let Some(declaring_class) = parent_class_name {
            if context.has_class(declaring_class) {
                let identity = values.object_identity(object)?;
                context.register_eval_reflection_property(
                    identity,
                    declaring_class,
                    reflected_name,
                );
            }
        }
    }
    values.release(attrs)?;
    values.release(interface_names_array)?;
    values.release(trait_names_array)?;
    values.release(method_names_array)?;
    values.release(property_names_array)?;
    values.release(method_objects)?;
    values.release(property_objects)?;
    values.release(parent_class)?;
    values.release(constructor)?;
    if release_constant_value {
        values.release(constant_value_cell)?;
    }
    if release_backing_value {
        values.release(backing_value_cell)?;
    }
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
        None,
        None,
        metadata.flags,
        metadata.modifiers,
        0,
        None,
        None,
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
        let Some((flags, modifiers)) = eval_reflection_aot_class_flags(class_name, values)? else {
            return values.bool_value(false);
        };
        return eval_reflection_owner_object_with_members(
            EVAL_REFLECTION_OWNER_CLASS,
            class_name.trim_start_matches('\\'),
            &[],
            &[],
            &[],
            &[],
            &[],
            None,
            &[],
            None,
            None,
            flags,
            modifiers,
            0,
            None,
            None,
            false,
            context,
            values,
        );
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
        None,
        None,
        metadata.flags,
        metadata.modifiers,
        0,
        None,
        None,
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
    let declaring_function = match parameter.declaring_function.as_ref() {
        Some(metadata) => {
            eval_reflection_declaring_function_object_result(metadata, context, values)?
        }
        None => values.null()?,
    };
    let trait_names = values.array_new(0)?;
    let method_names = values.array_new(0)?;
    let property_names = values.array_new(0)?;
    let method_objects = values.array_new(0)?;
    let parent_class = match parameter.declaring_class_name.as_deref() {
        Some(declaring_class_name) => {
            eval_reflection_shallow_class_object_result(declaring_class_name, context, values)?
        }
        None => values.null()?,
    };
    let type_value = match parameter.type_metadata.as_ref() {
        Some(type_metadata) => eval_reflection_type_object_result(type_metadata, values)?,
        None => values.null()?,
    };
    let default_value = match parameter.default_value.as_ref() {
        Some(default) => eval_method_parameter_default(default, context, values)?,
        None => values.null()?,
    };
    let constant_value = values.null()?;
    let backing_value = values.null()?;
    let flags = eval_reflection_parameter_flags(parameter);
    let object = values.reflection_owner_new(
        EVAL_REFLECTION_OWNER_PARAMETER,
        &parameter.name,
        attrs,
        declaring_function,
        trait_names,
        method_names,
        property_names,
        type_value,
        default_value,
        parent_class,
        flags,
        parameter.position as u64,
        0,
        constant_value,
        backing_value,
        constant_value,
    )?;
    values.release(attrs)?;
    values.release(declaring_function)?;
    values.release(trait_names)?;
    values.release(method_names)?;
    values.release(property_names)?;
    values.release(method_objects)?;
    values.release(type_value)?;
    values.release(default_value)?;
    values.release(parent_class)?;
    values.release(constant_value)?;
    values.release(backing_value)?;
    Ok(object)
}

/// Builds a shallow ReflectionMethod object for a parameter's declaring function metadata.
fn eval_reflection_declaring_function_object_result(
    metadata: &EvalReflectionDeclaringFunctionMetadata,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let owner_kind = if metadata.declaring_class_name.is_some() {
        EVAL_REFLECTION_OWNER_METHOD
    } else {
        EVAL_REFLECTION_OWNER_FUNCTION
    };
    let method_modifiers = if metadata.declaring_class_name.is_some() {
        eval_reflection_method_modifiers_from_flags(metadata.flags)
    } else {
        0
    };
    eval_reflection_owner_object(
        owner_kind,
        &metadata.name,
        &metadata.attributes,
        &[],
        &[],
        &[],
        &[],
        metadata.declaring_class_name.as_deref(),
        &[],
        None,
        None,
        metadata.flags,
        metadata.required_parameter_count as u64,
        method_modifiers,
        None,
        None,
        context,
        values,
    )
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
    let constant_value = values.null()?;
    let backing_value = values.null()?;
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
        0,
        constant_value,
        backing_value,
        constant_value,
    )?;
    values.release(attrs)?;
    values.release(interface_names)?;
    values.release(trait_names)?;
    values.release(method_names)?;
    values.release(property_names)?;
    values.release(method_objects)?;
    values.release(property_objects)?;
    values.release(parent_class)?;
    values.release(constant_value)?;
    values.release(backing_value)?;
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
    let constant_value = values.null()?;
    let backing_value = values.null()?;
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
        0,
        constant_value,
        backing_value,
        constant_value,
    )?;
    values.release(attrs)?;
    values.release(interface_names)?;
    values.release(trait_names)?;
    values.release(method_names)?;
    values.release(property_names)?;
    values.release(types)?;
    values.release(property_objects)?;
    values.release(parent_class)?;
    values.release(constant_value)?;
    values.release(backing_value)?;
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
    let constant_value = values.null()?;
    let backing_value = values.null()?;
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
        0,
        constant_value,
        backing_value,
        constant_value,
    )?;
    values.release(attrs)?;
    values.release(interface_names)?;
    values.release(trait_names)?;
    values.release(method_names)?;
    values.release(property_names)?;
    values.release(types)?;
    values.release(property_objects)?;
    values.release(parent_class)?;
    values.release(constant_value)?;
    values.release(backing_value)?;
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

/// Builds the `ReflectionMethod|null` value stored in ReflectionClass::__constructor.
fn eval_reflection_constructor_object_result(
    owner_kind: u64,
    class_name: &str,
    include_class_members: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if owner_kind != EVAL_REFLECTION_OWNER_CLASS || !include_class_members {
        return values.null();
    }
    let Some(member) = eval_reflection_method_metadata(class_name, "__construct", context) else {
        return values.null();
    };
    eval_reflection_member_object_result(
        EVAL_REFLECTION_OWNER_METHOD,
        "__construct",
        &member,
        context,
        values,
    )
}

/// Materializes one eval-backed ReflectionMethod or ReflectionProperty object.
fn eval_reflection_member_object_result(
    owner_kind: u64,
    reflected_name: &str,
    member: &EvalReflectionMemberMetadata,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut flags = eval_reflection_member_flags(
        member.visibility,
        member.is_static,
        member.is_final,
        member.is_abstract,
        member.is_readonly,
    );
    if member.default_value.is_some() {
        flags |= EVAL_REFLECTION_MEMBER_FLAG_HAS_DEFAULT_VALUE;
    }
    if member.is_promoted {
        flags |= EVAL_REFLECTION_MEMBER_FLAG_PROMOTED;
    }
    let owner_modifiers = if owner_kind == EVAL_REFLECTION_OWNER_METHOD {
        member.required_parameter_count as u64
    } else {
        member.modifiers
    };
    let method_modifiers = if owner_kind == EVAL_REFLECTION_OWNER_METHOD {
        member.modifiers
    } else {
        0
    };
    eval_reflection_owner_object(
        owner_kind,
        reflected_name,
        &member.attributes,
        &[],
        &[],
        &[],
        &[],
        member.declaring_class_name.as_deref(),
        &member.parameters,
        member.type_metadata.as_ref(),
        member.default_value.as_ref(),
        flags,
        owner_modifiers,
        method_modifiers,
        None,
        None,
        context,
        values,
    )
}

/// Builds an indexed array of ReflectionMethod or ReflectionProperty objects for a ReflectionClass.
fn eval_reflection_member_object_array_result(
    owner_kind: u64,
    class_name: &str,
    names: &[String],
    filter: Option<u64>,
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
        if !eval_reflection_member_matches_filter(&member, filter) {
            continue;
        }
        let member_object =
            eval_reflection_member_object_result(owner_kind, name, &member, context, values)?;
        let key = values.int(index)?;
        result = values.array_set(result, key, member_object)?;
        index += 1;
    }
    Ok(result)
}

/// Builds an indexed array of AOT ReflectionMethod or ReflectionProperty objects for a class.
fn eval_reflection_aot_member_object_array_result(
    owner_kind: u64,
    class_name: &str,
    names: &[String],
    filter: Option<u64>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(names.len())?;
    let mut index = 0;
    for name in names {
        let member = if owner_kind == EVAL_REFLECTION_OWNER_METHOD {
            eval_reflection_aot_method_metadata_if_exists(class_name, name, values)?
        } else {
            eval_reflection_aot_property_metadata_if_exists(class_name, name, values)?
        };
        let Some(member) = member else {
            continue;
        };
        if !eval_reflection_member_matches_filter(&member, filter) {
            continue;
        }
        let reflected_name = if owner_kind == EVAL_REFLECTION_OWNER_METHOD {
            name.to_ascii_lowercase()
        } else {
            name.clone()
        };
        let member_object = eval_reflection_member_object_result(
            owner_kind,
            &reflected_name,
            &member,
            context,
            values,
        )?;
        let key = values.int(index)?;
        result = values.array_set(result, key, member_object)?;
        index += 1;
    }
    Ok(result)
}

/// Returns true when a ReflectionClass member passes an optional modifier filter.
fn eval_reflection_member_matches_filter(
    member: &EvalReflectionMemberMetadata,
    filter: Option<u64>,
) -> bool {
    match filter {
        Some(filter) => member.modifiers & filter != 0,
        None => true,
    }
}

/// Parses the optional ReflectionClass member filter argument.
fn eval_reflection_member_filter(
    evaluated_args: Vec<EvaluatedCallArg>,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<u64>, EvalStatus> {
    let mut filter = None;
    for arg in evaluated_args {
        if let Some(name) = arg.name.as_deref() {
            if name != "filter" {
                return Err(EvalStatus::RuntimeFatal);
            }
        }
        if filter.is_some() {
            return Err(EvalStatus::RuntimeFatal);
        }
        filter = Some(arg.value);
    }
    let Some(filter) = filter else {
        return Ok(None);
    };
    if values.is_null(filter)? {
        return Ok(None);
    }
    let cast_filter = values.cast_int(filter)?;
    let bytes = values.string_bytes(cast_filter)?;
    values.release(cast_filter)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
    text.parse::<i64>()
        .map(|value| Some(value as u64))
        .map_err(|_| EvalStatus::RuntimeFatal)
}

/// Returns generated AOT member names for one reflected class.
fn eval_reflection_aot_member_names(
    owner_kind: u64,
    class_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    let runtime_class_name = class_name.trim_start_matches('\\');
    let names_array = if owner_kind == EVAL_REFLECTION_OWNER_METHOD {
        values.reflection_method_names(runtime_class_name)?
    } else {
        values.reflection_property_names(runtime_class_name)?
    };
    let names = eval_reflection_string_array_to_vec(names_array, values)?;
    values.release(names_array)?;
    Ok(names)
}

/// Copies a runtime string array into Rust-owned strings for reflection metadata assembly.
fn eval_reflection_string_array_to_vec(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = Vec::with_capacity(len);
    for position in 0..len {
        let key = values.int(position as i64)?;
        let value = values.array_get(array, key)?;
        result.push(eval_reflection_string_arg(value, values)?);
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
        let mut flags = EVAL_REFLECTION_CLASS_FLAG_USER_DEFINED;
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
        if eval_reflection_class_is_cloneable(class, is_enum, context) {
            flags |= EVAL_REFLECTION_CLASS_FLAG_CLONEABLE;
        }
        if eval_reflection_class_is_iterable(class, is_enum, context) {
            flags |= EVAL_REFLECTION_CLASS_FLAG_ITERABLE;
        }
        if class.is_anonymous() {
            flags |= EVAL_REFLECTION_CLASS_FLAG_ANONYMOUS;
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
            flags: EVAL_REFLECTION_CLASS_FLAG_INTERFACE | EVAL_REFLECTION_CLASS_FLAG_USER_DEFINED,
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
            flags: EVAL_REFLECTION_CLASS_FLAG_TRAIT | EVAL_REFLECTION_CLASS_FLAG_USER_DEFINED,
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
            flags: EVAL_REFLECTION_CLASS_FLAG_FINAL
                | EVAL_REFLECTION_CLASS_FLAG_ENUM
                | EVAL_REFLECTION_CLASS_FLAG_USER_DEFINED,
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

/// Returns PHP's `ReflectionClass::isCloneable()` value for eval class metadata.
fn eval_reflection_class_is_cloneable(
    class: &EvalClass,
    is_enum: bool,
    context: &ElephcEvalContext,
) -> bool {
    if class.is_abstract() || is_enum {
        return false;
    }
    context
        .class_method(class.name(), "__clone")
        .map(|(_, method)| method.visibility() == EvalVisibility::Public)
        .unwrap_or(true)
}

/// Returns PHP's `ReflectionClass::isIterable()` value for eval class metadata.
fn eval_reflection_class_is_iterable(
    class: &EvalClass,
    is_enum: bool,
    context: &ElephcEvalContext,
) -> bool {
    if class.is_abstract() || is_enum {
        return false;
    }
    context
        .class_interface_names(class.name())
        .iter()
        .any(|name| {
            name.eq_ignore_ascii_case("Iterator") || name.eq_ignore_ascii_case("IteratorAggregate")
        })
}

/// Returns PHP's `ReflectionClass::isIterable()` value for compiler-injected class names.
fn eval_reflection_builtin_class_is_iterable(class_name: &str) -> bool {
    matches!(
        class_name
            .trim_start_matches('\\')
            .to_ascii_lowercase()
            .as_str(),
        "__elephcappenditeratorarrayiterator"
            | "appenditerator"
            | "arrayiterator"
            | "arrayobject"
            | "cachingiterator"
            | "callbackfilteriterator"
            | "directoryiterator"
            | "emptyiterator"
            | "filesystemiterator"
            | "generator"
            | "globiterator"
            | "infiniteiterator"
            | "internaliterator"
            | "iteratoriterator"
            | "limititerator"
            | "multipleiterator"
            | "norewinditerator"
            | "parentiterator"
            | "recursivearrayiterator"
            | "recursivecachingiterator"
            | "recursivecallbackfilteriterator"
            | "recursivedirectoryiterator"
            | "recursiveiteratoriterator"
            | "recursiveregexiterator"
            | "regexiterator"
            | "spldoublylinkedlist"
            | "splfixedarray"
            | "splfileobject"
            | "splmaxheap"
            | "splminheap"
            | "splobjectstorage"
            | "splpriorityqueue"
            | "splqueue"
            | "splstack"
            | "spltempfileobject"
    )
}

/// Returns whether one reflected class-like name belongs to compiler-injected metadata.
fn eval_reflection_class_like_is_internal(class_name: &str) -> bool {
    let trimmed = class_name.trim_start_matches('\\');
    if EVAL_SPL_CLASS_NAMES
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(trimmed))
    {
        return true;
    }
    matches!(
        trimmed.to_ascii_lowercase().as_str(),
        "__elephcappenditeratorarrayiterator"
            | "fiber"
            | "fibererror"
            | "generator"
            | "internaliterator"
            | "jsonexception"
            | "phar"
            | "phardata"
            | "pharfileinfo"
            | "php_user_filter"
            | "reflectionattribute"
            | "reflectionclass"
            | "reflectionclassconstant"
            | "reflectionenumbackedcase"
            | "reflectionenumunitcase"
            | "reflectionexception"
            | "reflectionfunction"
            | "reflectionintersectiontype"
            | "reflectionmethod"
            | "reflectionnamedtype"
            | "reflectionparameter"
            | "reflectionproperty"
            | "reflectionuniontype"
            | "sortdirection"
            | "splheap"
            | "splmaxheap"
            | "splminheap"
            | "splobjectstorage"
            | "splpriorityqueue"
            | "stdclass"
    )
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

/// Computes PHP's `ReflectionClassConstant::getModifiers()` bitmask for eval metadata.
fn eval_reflection_class_constant_modifiers(visibility: EvalVisibility, is_final: bool) -> u64 {
    let mut modifiers = match visibility {
        EvalVisibility::Public => 1,
        EvalVisibility::Protected => 2,
        EvalVisibility::Private => 4,
    };
    if is_final {
        modifiers |= 32;
    }
    modifiers
}

/// Computes PHP's `ReflectionMethod::getModifiers()` bitmask for eval metadata.
fn eval_reflection_method_modifiers(
    visibility: EvalVisibility,
    is_static: bool,
    is_final: bool,
    is_abstract: bool,
) -> u64 {
    let mut modifiers = match visibility {
        EvalVisibility::Public => 1,
        EvalVisibility::Protected => 2,
        EvalVisibility::Private => 4,
    };
    if is_static {
        modifiers |= 16;
    }
    if is_final {
        modifiers |= 32;
    }
    if is_abstract {
        modifiers |= 64;
    }
    modifiers
}

/// Computes PHP's `ReflectionProperty::getModifiers()` bitmask for eval metadata.
fn eval_reflection_property_modifiers(
    visibility: EvalVisibility,
    is_static: bool,
    is_final: bool,
    is_abstract: bool,
    is_readonly: bool,
    is_virtual: bool,
) -> u64 {
    let mut modifiers = match visibility {
        EvalVisibility::Public => 1,
        EvalVisibility::Protected => 2,
        EvalVisibility::Private => 4,
    };
    if is_static {
        modifiers |= 16;
    }
    if is_final {
        modifiers |= 32;
    }
    if is_abstract {
        modifiers |= 64;
    }
    if is_readonly {
        modifiers |= 128;
    }
    if is_virtual {
        modifiers |= 512;
    }
    if is_readonly && visibility == EvalVisibility::Public {
        modifiers |= 2048;
    }
    modifiers
}

/// Returns whether an eval property is virtual because it has or requires hooks.
fn eval_reflection_property_is_virtual(property: &EvalClassProperty) -> bool {
    property.has_get_hook()
        || property.has_set_hook()
        || property.requires_get_hook()
        || property.requires_set_hook()
}

/// Computes PHP's `ReflectionMethod::getModifiers()` bitmask from eval member flags.
fn eval_reflection_method_modifiers_from_flags(flags: u64) -> u64 {
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

/// Returns declaring class, attributes, visibility, finality, and enum-case kind.
fn eval_reflection_class_constant_metadata(
    class_name: &str,
    constant_name: &str,
    context: &ElephcEvalContext,
) -> Option<(String, Vec<EvalAttribute>, EvalVisibility, bool, bool)> {
    if let Some(enum_decl) = context.enum_decl(class_name) {
        if let Some(case) = enum_decl.case(constant_name) {
            return Some((
                enum_decl.name().to_string(),
                case.attributes().to_vec(),
                EvalVisibility::Public,
                false,
                true,
            ));
        }
    }
    context
        .class_constant(class_name, constant_name)
        .map(|(declaring_class, constant)| {
            (
                declaring_class,
                constant.attributes().to_vec(),
                constant.visibility(),
                constant.is_final(),
                false,
            )
        })
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

/// Returns true when reflected eval metadata is a subclass or subinterface of a target.
fn eval_reflection_class_is_subclass_of_name(
    reflected_name: &str,
    target_name: &str,
    context: &ElephcEvalContext,
) -> bool {
    if context.has_interface(reflected_name) {
        return context
            .interface_parent_names(reflected_name)
            .iter()
            .any(|parent| eval_reflection_same_class_like_name(parent, target_name));
    }
    if context.has_class(reflected_name) || context.has_enum(reflected_name) {
        return context.class_is_a(reflected_name, target_name, true);
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
            .map(|(declaring_class, method)| {
                let required_parameter_count = eval_reflection_required_parameter_count(
                    method.parameter_defaults(),
                    method.parameter_is_variadic(),
                );
                let flags = eval_reflection_member_flags(
                    method.visibility(),
                    method.is_static(),
                    method.is_final(),
                    method.is_abstract(),
                    false,
                );
                let return_type_metadata = method
                    .return_type()
                    .and_then(eval_reflection_parameter_type_metadata);
                let declaring_function = EvalReflectionDeclaringFunctionMetadata {
                    name: method.name().to_string(),
                    declaring_class_name: Some(declaring_class.clone()),
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
                EvalReflectionMemberMetadata {
                    declaring_class_name: Some(declaring_class),
                    attributes: method.attributes().to_vec(),
                    visibility: method.visibility(),
                    is_static: method.is_static(),
                    is_final: method.is_final(),
                    is_abstract: method.is_abstract(),
                    is_readonly: false,
                    is_promoted: false,
                    modifiers: eval_reflection_method_modifiers(
                        method.visibility(),
                        method.is_static(),
                        method.is_final(),
                        method.is_abstract(),
                    ),
                    type_metadata: None,
                    return_type_metadata,
                    default_value: None,
                    required_parameter_count,
                    parameters,
                }
            });
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
                let flags = eval_reflection_member_flags(
                    EvalVisibility::Public,
                    method.is_static(),
                    false,
                    true,
                    false,
                );
                let return_type_metadata = method
                    .return_type()
                    .and_then(eval_reflection_parameter_type_metadata);
                let declaring_function = EvalReflectionDeclaringFunctionMetadata {
                    name: method.name().to_string(),
                    declaring_class_name: Some(class_name.to_string()),
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
                    attributes: method.attributes().to_vec(),
                    visibility: EvalVisibility::Public,
                    is_static: method.is_static(),
                    is_final: false,
                    is_abstract: true,
                    is_readonly: false,
                    is_promoted: false,
                    modifiers: eval_reflection_method_modifiers(
                        EvalVisibility::Public,
                        method.is_static(),
                        false,
                        true,
                    ),
                    type_metadata: None,
                    return_type_metadata,
                    default_value: None,
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
                let flags = eval_reflection_member_flags(
                    method.visibility(),
                    method.is_static(),
                    method.is_final(),
                    method.is_abstract(),
                    false,
                );
                let return_type_metadata = method
                    .return_type()
                    .and_then(eval_reflection_parameter_type_metadata);
                let declaring_function = EvalReflectionDeclaringFunctionMetadata {
                    name: method.name().to_string(),
                    declaring_class_name: Some(trait_decl.name().to_string()),
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
                    attributes: method.attributes().to_vec(),
                    visibility: method.visibility(),
                    is_static: method.is_static(),
                    is_final: method.is_final(),
                    is_abstract: method.is_abstract(),
                    is_readonly: false,
                    is_promoted: false,
                    modifiers: eval_reflection_method_modifiers(
                        method.visibility(),
                        method.is_static(),
                        method.is_final(),
                        method.is_abstract(),
                    ),
                    type_metadata: None,
                    return_type_metadata,
                    default_value: None,
                    required_parameter_count,
                    parameters,
                }
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
        return context.class_property(class_name, property_name).map(
            |(declaring_class, property)| {
                let default_value = eval_reflection_property_default_value(&property);
                EvalReflectionMemberMetadata {
                    declaring_class_name: Some(declaring_class),
                    attributes: property.attributes().to_vec(),
                    visibility: property.visibility(),
                    is_static: property.is_static(),
                    is_final: property.is_final(),
                    is_abstract: property.is_abstract(),
                    is_readonly: property.is_readonly(),
                    is_promoted: property.is_promoted(),
                    modifiers: eval_reflection_property_modifiers(
                        property.visibility(),
                        property.is_static(),
                        property.is_final(),
                        property.is_abstract(),
                        property.is_readonly(),
                        eval_reflection_property_is_virtual(&property),
                    ),
                    type_metadata: property
                        .property_type()
                        .and_then(eval_reflection_parameter_type_metadata),
                    return_type_metadata: None,
                    default_value,
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
            .map(|property| EvalReflectionMemberMetadata {
                declaring_class_name: Some(class_name.to_string()),
                attributes: property.attributes().to_vec(),
                visibility: EvalVisibility::Public,
                is_static: false,
                is_final: false,
                is_abstract: true,
                is_readonly: false,
                is_promoted: false,
                modifiers: eval_reflection_property_modifiers(
                    EvalVisibility::Public,
                    false,
                    false,
                    true,
                    false,
                    true,
                ),
                type_metadata: property
                    .property_type()
                    .and_then(eval_reflection_parameter_type_metadata),
                return_type_metadata: None,
                default_value: None,
                required_parameter_count: 0,
                parameters: Vec::new(),
            });
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
                    attributes: property.attributes().to_vec(),
                    visibility: property.visibility(),
                    is_static: property.is_static(),
                    is_final: property.is_final(),
                    is_abstract: property.is_abstract(),
                    is_readonly: property.is_readonly(),
                    is_promoted: property.is_promoted(),
                    modifiers: eval_reflection_property_modifiers(
                        property.visibility(),
                        property.is_static(),
                        property.is_final(),
                        property.is_abstract(),
                        property.is_readonly(),
                        eval_reflection_property_is_virtual(property),
                    ),
                    type_metadata: property
                        .property_type()
                        .and_then(eval_reflection_parameter_type_metadata),
                    return_type_metadata: None,
                    default_value,
                    required_parameter_count: 0,
                    parameters: Vec::new(),
                }
            })
    })
}

/// Returns property names that can contribute to `ReflectionClass::getDefaultProperties()`.
fn eval_reflection_default_property_names(
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

/// Returns eval property names that can contribute to `ReflectionClass::getStaticProperties()`.
fn eval_reflection_static_property_names(
    reflected_name: &str,
    context: &ElephcEvalContext,
) -> Vec<String> {
    eval_reflection_default_property_names(reflected_name, context)
        .into_iter()
        .filter(|name| {
            eval_reflection_property_metadata(reflected_name, name, context)
                .is_some_and(|property| property.is_static)
        })
        .collect()
}

/// Returns the current eval static property value or the trait metadata default.
fn eval_reflection_static_property_value(
    reflected_name: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(member) = eval_reflection_property_metadata(reflected_name, property_name, context)
    else {
        return Ok(None);
    };
    if !member.is_static {
        return Ok(None);
    }
    let declaring_class = member
        .declaring_class_name
        .as_deref()
        .ok_or(EvalStatus::RuntimeFatal)?;
    if let Some(value) = context.static_property(declaring_class, property_name) {
        return Ok(Some(value));
    }
    member
        .default_value
        .as_ref()
        .map(|default| eval_method_parameter_default(default, context, values))
        .transpose()
}

/// Binds `getStaticPropertyValue()` arguments while preserving whether a default was supplied.
fn eval_reflection_static_property_value_args(
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
fn eval_reflection_property_get_value_arg(
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
fn eval_reflection_property_set_value_args(
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

/// Binds `ReflectionMethod::invoke()` arguments and preserves forwarded named args.
fn eval_reflection_method_invoke_args(
    evaluated_args: Vec<EvaluatedCallArg>,
) -> Result<(RuntimeCellHandle, Vec<EvaluatedCallArg>), EvalStatus> {
    let mut object = None;
    let mut method_args = Vec::new();
    for arg in evaluated_args {
        if matches!(arg.name.as_deref(), Some("object")) {
            if object.is_some() {
                return Err(EvalStatus::RuntimeFatal);
            }
            object = Some(arg.value);
        } else if object.is_none() && arg.name.is_none() {
            object = Some(arg.value);
        } else {
            method_args.push(eval_reflection_method_forwarded_value_arg(arg));
        }
    }
    object
        .map(|object| (object, method_args))
        .ok_or(EvalStatus::RuntimeFatal)
}

/// Converts a variadic `invoke()` argument into a by-value forwarded method argument.
fn eval_reflection_method_forwarded_value_arg(arg: EvaluatedCallArg) -> EvaluatedCallArg {
    EvaluatedCallArg {
        name: arg.name,
        value: arg.value,
        ref_target: None,
    }
}

/// Binds `ReflectionMethod::invokeArgs()` and expands its PHP argument array.
fn eval_reflection_method_invoke_args_array(
    evaluated_args: Vec<EvaluatedCallArg>,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Vec<EvaluatedCallArg>), EvalStatus> {
    let args = bind_evaluated_function_args(
        &[String::from("object"), String::from("args")],
        evaluated_args,
    )?;
    let method_args = eval_array_call_arg_values(args[1], values)?;
    Ok((args[0], method_args))
}

/// Binds `ReflectionFunction::invokeArgs()` and expands its PHP argument array.
fn eval_reflection_function_invoke_args_array(
    evaluated_args: Vec<EvaluatedCallArg>,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvaluatedCallArg>, EvalStatus> {
    let args = bind_evaluated_function_args(&[String::from("args")], evaluated_args)?;
    eval_array_call_arg_values(args[0], values)
}

/// Dispatches one reflected function invocation through eval or registered native functions.
fn eval_reflection_function_invoke_dispatch(
    function_name: &str,
    function_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let function_key = function_name.to_ascii_lowercase();
    if let Some(function) = context.function(&function_key).cloned() {
        let by_value_parameters = vec![false; function.params().len()];
        return eval_dynamic_function_with_evaluated_args_and_ref_flags(
            &function,
            &by_value_parameters,
            function_args,
            context,
            values,
        );
    }
    eval_callable_with_call_array_args(&function_key, function_args, context, values)
}

/// Dispatches one reflected method invocation through eval or public AOT bridges.
fn eval_reflection_method_invoke_dispatch(
    declaring_class: &str,
    method_name: &str,
    object: RuntimeCellHandle,
    method_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some((method_class, method)) = context.class_method(declaring_class, method_name) {
        if method.is_abstract() {
            return Err(EvalStatus::RuntimeFatal);
        }
        let by_value_parameters = vec![false; method.params().len()];
        if method.is_static() {
            return eval_dynamic_static_method_with_values_and_ref_flags(
                &method_class,
                &method_class,
                &method,
                &by_value_parameters,
                method_args,
                context,
                values,
            );
        }
        let called_class =
            eval_reflection_method_instance_called_class(declaring_class, object, context, values)?;
        return eval_dynamic_method_with_values_and_ref_flags(
            &method_class,
            &called_class,
            &method,
            object,
            &by_value_parameters,
            method_args,
            context,
            values,
        );
    }
    eval_reflection_aot_method_invoke_dispatch(
        declaring_class,
        method_name,
        object,
        method_args,
        context,
        values,
    )
}

/// Returns the runtime class name for an eval object used as a reflected receiver.
fn eval_reflection_method_instance_called_class(
    declaring_class: &str,
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    if values.is_null(object)? || values.type_tag(object)? != EVAL_TAG_OBJECT {
        return Err(EvalStatus::RuntimeFatal);
    }
    let identity = values.object_identity(object)?;
    let Some(object_class_name) = context
        .dynamic_object_class(identity)
        .map(|class| class.name().to_string())
    else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if !context.class_is_a(&object_class_name, declaring_class, false) {
        eval_throw_reflection_exception(
            "Given object is not an instance of the class this method was declared in",
            context,
            values,
        )?;
        return Err(EvalStatus::UncaughtThrowable);
    }
    Ok(object_class_name)
}

/// Invokes one reflected generated/AOT method when it fits the public bridge slice.
fn eval_reflection_aot_method_invoke_dispatch(
    declaring_class: &str,
    method_name: &str,
    object: RuntimeCellHandle,
    method_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let member =
        eval_reflection_aot_method_metadata_if_exists(declaring_class, method_name, values)?
            .ok_or(EvalStatus::RuntimeFatal)?;
    if member.visibility != EvalVisibility::Public || member.is_abstract {
        return Err(EvalStatus::RuntimeFatal);
    }
    if member.is_static {
        let args = bind_native_callable_args(
            context.native_static_method_signature(declaring_class, method_name),
            method_args,
        )?;
        return values.static_method_call(declaring_class, method_name, args);
    }
    if values.is_null(object)? || values.type_tag(object)? != EVAL_TAG_OBJECT {
        return Err(EvalStatus::RuntimeFatal);
    }
    let is_instance = dynamic_object_is_a(object, declaring_class, false, context, values)?
        .map_or_else(|| values.object_is_a(object, declaring_class, false), Ok)?;
    if !is_instance {
        eval_throw_reflection_exception(
            "Given object is not an instance of the class this method was declared in",
            context,
            values,
        )?;
        return Err(EvalStatus::UncaughtThrowable);
    }
    let args = bind_native_callable_args(
        context.native_method_signature(declaring_class, method_name),
        method_args,
    )?;
    values.method_call(object, method_name, args)
}

/// Reads one eval instance property through ReflectionProperty semantics.
fn eval_reflection_instance_property_get_value(
    declaring_class: &str,
    property_name: &str,
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (object_class_name, property) = eval_reflection_instance_property_target(
        declaring_class,
        property_name,
        object,
        context,
        values,
    )?;
    if property.has_get_hook()
        && !current_eval_property_hook_is(
            declaring_class,
            property.name(),
            &property_hook_get_method(property.name()),
            context,
        )
    {
        let (hook_class, hook_method) = context
            .class_method(
                &object_class_name,
                &property_hook_get_method(property.name()),
            )
            .ok_or(EvalStatus::RuntimeFatal)?;
        return eval_dynamic_method_with_values(
            &hook_class,
            &object_class_name,
            &hook_method,
            object,
            Vec::new(),
            context,
            values,
        );
    }
    let storage_property_name = eval_instance_property_storage_name(declaring_class, &property);
    values.property_get(object, &storage_property_name)
}

/// Writes one eval instance property through ReflectionProperty semantics.
fn eval_reflection_instance_property_set_value(
    declaring_class: &str,
    property_name: &str,
    object: RuntimeCellHandle,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let (object_class_name, property) = eval_reflection_instance_property_target(
        declaring_class,
        property_name,
        object,
        context,
        values,
    )?;
    validate_eval_reflection_property_write(declaring_class, &property, context)?;
    if property.has_set_hook() {
        if !current_eval_property_hook_is(
            declaring_class,
            property.name(),
            &property_hook_set_method(property.name()),
            context,
        ) {
            let (hook_class, hook_method) = context
                .class_method(
                    &object_class_name,
                    &property_hook_set_method(property.name()),
                )
                .ok_or(EvalStatus::RuntimeFatal)?;
            let hook_result = eval_dynamic_method_with_values(
                &hook_class,
                &object_class_name,
                &hook_method,
                object,
                vec![EvaluatedCallArg {
                    name: None,
                    value,
                    ref_target: None,
                }],
                context,
                values,
            )?;
            values.release(hook_result)?;
            return Ok(());
        }
    } else if property.has_get_hook() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let storage_property_name = eval_instance_property_storage_name(declaring_class, &property);
    values.property_set(object, &storage_property_name, value)
}

/// Resolves and validates the object/property pair targeted by ReflectionProperty.
fn eval_reflection_instance_property_target(
    declaring_class: &str,
    property_name: &str,
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(String, EvalClassProperty), EvalStatus> {
    let identity = values.object_identity(object)?;
    let object_class_name = context
        .dynamic_object_class(identity)
        .map(|class| class.name().to_string())
        .ok_or(EvalStatus::RuntimeFatal)?;
    if !context.class_is_a(&object_class_name, declaring_class, false) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let (_, property) = context
        .class_own_property(declaring_class, property_name)
        .ok_or(EvalStatus::RuntimeFatal)?;
    if property.is_static() || property.is_abstract() {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok((object_class_name, property))
}

/// Rejects writes to eval properties ReflectionProperty is not allowed to mutate.
fn validate_eval_reflection_property_write(
    declaring_class: &str,
    property: &EvalClassProperty,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    if !property.is_readonly() {
        return Ok(());
    }
    current_eval_property_hook_is(
        declaring_class,
        property.name(),
        &property_hook_set_method(property.name()),
        context,
    )
    .then_some(())
    .ok_or(EvalStatus::RuntimeFatal)
}

/// Throws PHP's `ReflectionException` for invalid static-property writes.
fn eval_reflection_static_property_missing_for_set(
    reflected_name: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    eval_throw_reflection_exception(
        &format!(
            "Class {} does not have a property named {}",
            reflected_name, property_name
        ),
        context,
        values,
    )
}

/// Returns ReflectionProperty default metadata for concrete eval properties.
fn eval_reflection_property_default_value(property: &EvalClassProperty) -> Option<EvalExpr> {
    if let Some(default) = property.default() {
        return Some(default.clone());
    }
    if property.is_abstract() || property.property_type().is_some() {
        return None;
    }
    Some(EvalExpr::Const(EvalConst::Null))
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
    declaring_class_name: Option<&str>,
    declaring_function: Option<&EvalReflectionDeclaringFunctionMetadata>,
    names: &[String],
    has_type_flags: &[bool],
    parameter_types: &[Option<EvalParameterType>],
    parameter_attributes: &[Vec<EvalAttribute>],
    defaults: &[Option<EvalExpr>],
    by_ref_flags: &[bool],
    variadic_flags: &[bool],
    promoted_parameter_names: &[String],
) -> Vec<EvalReflectionParameterMetadata> {
    names
        .iter()
        .enumerate()
        .map(|(position, name)| EvalReflectionParameterMetadata {
            name: name.clone(),
            declaring_class_name: declaring_class_name.map(str::to_string),
            declaring_function: declaring_function.cloned(),
            attributes: parameter_attributes
                .get(position)
                .cloned()
                .unwrap_or_default(),
            position,
            is_optional: defaults.get(position).is_some_and(Option::is_some)
                || variadic_flags.get(position).copied().unwrap_or(false),
            is_variadic: variadic_flags.get(position).copied().unwrap_or(false),
            is_passed_by_reference: by_ref_flags.get(position).copied().unwrap_or(false),
            is_promoted: promoted_parameter_names
                .iter()
                .any(|promoted_name| promoted_name == name),
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

/// Builds ReflectionParameter metadata for eval-declared or native free functions.
fn eval_reflection_function_parameters(
    function_name: &str,
    names: &[String],
    function_attributes: Vec<EvalAttribute>,
    parameter_attributes: &[Vec<EvalAttribute>],
    parameter_types: &[Option<EvalParameterType>],
    defaults: &[Option<EvalExpr>],
    by_ref_flags: &[bool],
    variadic_flags: &[bool],
) -> Vec<EvalReflectionParameterMetadata> {
    let has_type_flags = parameter_types
        .iter()
        .map(Option::is_some)
        .collect::<Vec<_>>();
    let declaring_function = EvalReflectionDeclaringFunctionMetadata {
        name: function_name.to_string(),
        declaring_class_name: None,
        attributes: function_attributes,
        flags: 0,
        required_parameter_count: eval_reflection_required_parameter_count(
            defaults,
            variadic_flags,
        ),
    };
    eval_reflection_parameters_from_names_and_type_flags(
        None,
        Some(&declaring_function),
        names,
        &has_type_flags,
        parameter_types,
        parameter_attributes,
        defaults,
        by_ref_flags,
        variadic_flags,
        &[],
    )
}

/// Returns promoted constructor parameter names for one eval class method.
fn eval_reflection_promoted_parameter_names(
    class_name: &str,
    method_name: &str,
    context: &ElephcEvalContext,
) -> Vec<String> {
    if !method_name.eq_ignore_ascii_case("__construct") {
        return Vec::new();
    }
    context
        .class(class_name)
        .map(eval_reflection_promoted_property_names)
        .unwrap_or_default()
}

/// Returns promoted constructor parameter names for one eval trait method.
fn eval_reflection_promoted_trait_parameter_names(
    trait_decl: &EvalTrait,
    method_name: &str,
) -> Vec<String> {
    if method_name.eq_ignore_ascii_case("__construct") {
        eval_reflection_promoted_property_names_from_slice(trait_decl.properties())
    } else {
        Vec::new()
    }
}

/// Returns property names marked as constructor-promoted in one eval class.
fn eval_reflection_promoted_property_names(class: &EvalClass) -> Vec<String> {
    eval_reflection_promoted_property_names_from_slice(class.properties())
}

/// Returns property names marked as constructor-promoted in one property list.
fn eval_reflection_promoted_property_names_from_slice(
    properties: &[EvalClassProperty],
) -> Vec<String> {
    properties
        .iter()
        .filter(|property| property.is_promoted())
        .map(|property| property.name().to_string())
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
        EvalParameterTypeVariant::Never => Some(eval_reflection_builtin_named_type("never", false)),
        EvalParameterTypeVariant::Object => {
            Some(eval_reflection_builtin_named_type("object", allows_null))
        }
        EvalParameterTypeVariant::String => {
            Some(eval_reflection_builtin_named_type("string", allows_null))
        }
        EvalParameterTypeVariant::Void => Some(eval_reflection_builtin_named_type("void", false)),
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

/// Returns function or method metadata registered for a synthetic reflection owner object.
fn eval_reflection_function_method_target(
    identity: u64,
    context: &ElephcEvalContext,
) -> Option<EvalReflectionFunctionMethodTarget> {
    if let Some(name) = context.eval_reflection_function_name(identity) {
        let function = context.function(&name.to_ascii_lowercase());
        let is_variadic = function
            .is_some_and(|function| function.parameter_is_variadic().iter().any(|flag| *flag));
        let return_type_metadata = function
            .and_then(EvalFunction::return_type)
            .and_then(eval_reflection_parameter_type_metadata);
        return Some(EvalReflectionFunctionMethodTarget::Function {
            name: name.to_string(),
            is_variadic,
            return_type_metadata,
        });
    }
    context
        .eval_reflection_method(identity)
        .map(|(declaring_class, method_name)| {
            let method_metadata =
                eval_reflection_method_metadata(declaring_class, method_name, context);
            let is_variadic = method_metadata.as_ref().is_some_and(|method| {
                method
                    .parameters
                    .iter()
                    .any(|parameter| parameter.is_variadic)
            });
            let return_type_metadata =
                method_metadata.and_then(|method| method.return_type_metadata);
            EvalReflectionFunctionMethodTarget::Method {
                name: method_name.to_string(),
                is_variadic,
                return_type_metadata,
            }
        })
}

/// Validates that a synthetic reflection metadata call received no arguments.
fn eval_reflection_bind_no_args(evaluated_args: Vec<EvaluatedCallArg>) -> Result<(), EvalStatus> {
    let _ = bind_evaluated_function_args(&[], evaluated_args)?;
    Ok(())
}

/// Returns a no-argument reflection metadata predicate result that is always false.
fn eval_reflection_false_metadata_result(
    evaluated_args: Vec<EvaluatedCallArg>,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    eval_reflection_bind_no_args(evaluated_args)?;
    values.bool_value(false).map(Some)
}

/// Returns PHP's short name for a ReflectionFunction or ReflectionMethod target.
fn eval_reflection_function_method_short_name(
    target: &EvalReflectionFunctionMethodTarget,
) -> String {
    match target {
        EvalReflectionFunctionMethodTarget::Function { name, .. } => {
            eval_reflection_short_name(name)
        }
        EvalReflectionFunctionMethodTarget::Method { name, .. } => name.clone(),
    }
}

/// Returns PHP's namespace name for a ReflectionFunction or ReflectionMethod target.
fn eval_reflection_function_method_namespace_name(
    target: &EvalReflectionFunctionMethodTarget,
) -> String {
    match target {
        EvalReflectionFunctionMethodTarget::Function { name, .. } => {
            eval_reflection_namespace_name(name)
        }
        EvalReflectionFunctionMethodTarget::Method { .. } => String::new(),
    }
}

/// Returns whether the reflected function or method has a variadic parameter.
fn eval_reflection_function_method_is_variadic(
    target: &EvalReflectionFunctionMethodTarget,
) -> bool {
    match target {
        EvalReflectionFunctionMethodTarget::Function { is_variadic, .. }
        | EvalReflectionFunctionMethodTarget::Method { is_variadic, .. } => *is_variadic,
    }
}

/// Returns the retained return type metadata for a reflected function or method.
fn eval_reflection_function_method_return_type(
    target: &EvalReflectionFunctionMethodTarget,
) -> Option<&EvalReflectionParameterTypeMetadata> {
    match target {
        EvalReflectionFunctionMethodTarget::Function {
            return_type_metadata,
            ..
        }
        | EvalReflectionFunctionMethodTarget::Method {
            return_type_metadata,
            ..
        } => return_type_metadata.as_ref(),
    }
}

/// Returns the final namespace segment-free name component from a PHP symbol name.
fn eval_reflection_short_name(name: &str) -> String {
    let name = name.trim_start_matches('\\');
    name.rsplit_once('\\').map_or_else(
        || name.to_string(),
        |(_, short_name)| short_name.to_string(),
    )
}

/// Returns the namespace prefix from a PHP function name, or an empty string.
fn eval_reflection_namespace_name(name: &str) -> String {
    name.trim_start_matches('\\')
        .rsplit_once('\\')
        .map_or_else(String::new, |(namespace_name, _)| {
            namespace_name.to_string()
        })
}

/// Finds the PHP ReflectionMethod prototype target for an eval-declared method.
fn eval_reflection_method_prototype_target(
    declaring_class: &str,
    method_name: &str,
    context: &ElephcEvalContext,
) -> Option<(String, String)> {
    if !(context.has_class(declaring_class) || context.has_enum(declaring_class)) {
        return None;
    }
    eval_reflection_parent_method_prototype_target(declaring_class, method_name, context).or_else(
        || eval_reflection_interface_method_prototype_target(declaring_class, method_name, context),
    )
}

/// Finds the nearest parent-class method prototype for an eval-declared override.
fn eval_reflection_parent_method_prototype_target(
    declaring_class: &str,
    method_name: &str,
    context: &ElephcEvalContext,
) -> Option<(String, String)> {
    for parent_class in context.class_parent_names(declaring_class) {
        if let Some((prototype_class, prototype_method)) =
            context.class_own_method(&parent_class, method_name)
        {
            return Some((prototype_class, prototype_method.name().to_string()));
        }
    }
    None
}

/// Finds the interface method prototype for an eval-declared class method.
fn eval_reflection_interface_method_prototype_target(
    declaring_class: &str,
    method_name: &str,
    context: &ElephcEvalContext,
) -> Option<(String, String)> {
    let mut seen = std::collections::HashSet::new();
    for interface_name in context.class_interface_names(declaring_class) {
        if let Some(prototype) = eval_reflection_interface_declared_method_target(
            &interface_name,
            method_name,
            context,
            &mut seen,
        ) {
            return Some(prototype);
        }
    }
    None
}

/// Finds the interface that actually declares a method in an interface hierarchy.
fn eval_reflection_interface_declared_method_target(
    interface_name: &str,
    method_name: &str,
    context: &ElephcEvalContext,
    seen: &mut std::collections::HashSet<String>,
) -> Option<(String, String)> {
    let interface = context.interface(interface_name)?;
    if !seen.insert(interface.name().to_ascii_lowercase()) {
        return None;
    }
    if let Some(method) = interface
        .methods()
        .iter()
        .find(|method| method.name().eq_ignore_ascii_case(method_name))
    {
        return Some((interface.name().to_string(), method.name().to_string()));
    }
    for parent in interface.parents() {
        if let Some(prototype) =
            eval_reflection_interface_declared_method_target(parent, method_name, context, seen)
        {
            return Some(prototype);
        }
    }
    None
}

/// Packs ReflectionMethod/ReflectionProperty predicate flags for the runtime owner factory.
fn eval_reflection_member_flags(
    visibility: EvalVisibility,
    is_static: bool,
    is_final: bool,
    is_abstract: bool,
    is_readonly: bool,
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
    if is_readonly {
        flags |= EVAL_REFLECTION_MEMBER_FLAG_READONLY;
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
    if parameter.is_promoted {
        flags |= EVAL_REFLECTION_PARAMETER_FLAG_PROMOTED;
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
        "reflectionfunction" => Some(EVAL_REFLECTION_OWNER_FUNCTION),
        "reflectionmethod" => Some(EVAL_REFLECTION_OWNER_METHOD),
        "reflectionproperty" => Some(EVAL_REFLECTION_OWNER_PROPERTY),
        "reflectionclassconstant" => Some(EVAL_REFLECTION_OWNER_CLASS_CONSTANT),
        "reflectionenumunitcase" => Some(EVAL_REFLECTION_OWNER_ENUM_UNIT_CASE),
        "reflectionenumbackedcase" => Some(EVAL_REFLECTION_OWNER_ENUM_BACKED_CASE),
        _ => None,
    }
}
