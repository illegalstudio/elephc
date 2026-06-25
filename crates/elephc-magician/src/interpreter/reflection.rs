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
use crate::eval_ir::EvalSourceLocation;

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
pub(in crate::interpreter) const EVAL_REFLECTION_ATTRIBUTE_TARGET_CLASS: u64 = 1;
pub(in crate::interpreter) const EVAL_REFLECTION_ATTRIBUTE_TARGET_FUNCTION: u64 = 2;
pub(in crate::interpreter) const EVAL_REFLECTION_ATTRIBUTE_TARGET_METHOD: u64 = 4;
pub(in crate::interpreter) const EVAL_REFLECTION_ATTRIBUTE_TARGET_PROPERTY: u64 = 8;
pub(in crate::interpreter) const EVAL_REFLECTION_ATTRIBUTE_TARGET_CLASS_CONSTANT: u64 = 16;
pub(in crate::interpreter) const EVAL_REFLECTION_ATTRIBUTE_TARGET_PARAMETER: u64 = 32;
const EVAL_REFLECTION_MEMBER_FLAG_PROMOTED: u64 = 512;
const EVAL_REFLECTION_MEMBER_FLAG_VIRTUAL: u64 = 1024;
const EVAL_REFLECTION_MEMBER_FLAG_PROTECTED_SET: u64 = 2048;
const EVAL_REFLECTION_MEMBER_FLAG_PRIVATE_SET: u64 = 4096;
const EVAL_REFLECTION_MEMBER_FLAG_DYNAMIC: u64 = 8192;
const EVAL_REFLECTION_PARAMETER_FLAG_OPTIONAL: u64 = 1;
const EVAL_REFLECTION_PARAMETER_FLAG_VARIADIC: u64 = 2;
const EVAL_REFLECTION_PARAMETER_FLAG_BY_REF: u64 = 4;
const EVAL_REFLECTION_PARAMETER_FLAG_HAS_TYPE: u64 = 8;
const EVAL_REFLECTION_PARAMETER_FLAG_HAS_DEFAULT_VALUE: u64 = 16;
const EVAL_REFLECTION_PARAMETER_FLAG_PROMOTED: u64 = 32;
const EVAL_REFLECTION_PARAMETER_FLAG_ALLOWS_NULL: u64 = 64;
const EVAL_REFLECTION_PARAMETER_FLAG_DEFAULT_VALUE_CONSTANT: u64 = 128;
const EVAL_REFLECTION_PARAMETER_FLAG_ARRAY_TYPE: u64 = 256;
const EVAL_REFLECTION_PARAMETER_FLAG_CALLABLE_TYPE: u64 = 512;
const EVAL_REFLECTION_NAMED_TYPE_FLAG_ALLOWS_NULL: u64 = 1;
const EVAL_REFLECTION_NAMED_TYPE_FLAG_BUILTIN: u64 = 2;

/// Eval metadata needed to materialize one `ReflectionClass` owner object.
struct EvalReflectionClassMetadata {
    resolved_name: String,
    source_location: Option<EvalSourceLocation>,
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
    source_location: Option<EvalSourceLocation>,
    attributes: Vec<EvalAttribute>,
    visibility: EvalVisibility,
    is_static: bool,
    is_final: bool,
    is_abstract: bool,
    is_readonly: bool,
    is_promoted: bool,
    is_dynamic: bool,
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
    allows_null: bool,
    is_array_type: bool,
    is_callable_type: bool,
    type_metadata: Option<EvalReflectionParameterTypeMetadata>,
    default_value: Option<EvalExpr>,
    default_value_constant_name: Option<String>,
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

/// Property hook kind accepted by `ReflectionProperty` hook APIs.
#[derive(Clone, Copy)]
enum EvalReflectionPropertyHook {
    Get,
    Set,
}

/// Constructor selector accepted by `ReflectionParameter`.
enum EvalReflectionParameterSelector {
    Name(String),
    Position(i64),
}

impl EvalReflectionPropertyHook {
    /// Returns the associative-array key PHP uses for this hook kind.
    const fn key(self) -> &'static str {
        match self {
            Self::Get => "get",
            Self::Set => "set",
        }
    }

    /// Returns the PHP-visible synthetic hook method name.
    fn reflected_method_name(self, property_name: &str) -> String {
        format!("${}::{}", property_name, self.key())
    }

    /// Returns the internal eval method name that stores the hook body.
    fn synthetic_method_name(self, property_name: &str) -> String {
        match self {
            Self::Get => property_hook_get_method(property_name),
            Self::Set => property_hook_set_method(property_name),
        }
    }
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
        static_key: Option<String>,
        static_variables: Vec<EvalStaticVarInitializer>,
        source_location: Option<EvalSourceLocation>,
        is_variadic: bool,
        return_type_metadata: Option<EvalReflectionParameterTypeMetadata>,
    },
    Method {
        declaring_class: Option<String>,
        name: String,
        static_key: Option<String>,
        static_variables: Vec<EvalStaticVarInitializer>,
        source_location: Option<EvalSourceLocation>,
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
        Some(EVAL_REFLECTION_OWNER_PARAMETER) => {
            eval_reflection_parameter_new(evaluated_args, context, values)
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
    let result = if eval_reflection_class_like_exists(&reflected_name, context) {
        eval_reflection_class_implements_interface_name(&reflected_name, &interface_name, context)
    } else if values.interface_exists(&reflected_name)? {
        eval_reflection_same_class_like_name(&reflected_name, &interface_name)
    } else {
        let reflected_class = values.string(&reflected_name)?;
        let result = values.object_is_a(reflected_class, &interface_name, false);
        values.release(reflected_class)?;
        result?
    };
    values.bool_value(result).map(Some)
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

/// Handles eval-backed `ReflectionClass` source-location metadata calls.
pub(in crate::interpreter) fn eval_reflection_class_source_location_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let method_key = method_name.to_ascii_lowercase();
    if !matches!(
        method_key.as_str(),
        "getfilename" | "getstartline" | "getendline"
    ) {
        return Ok(None);
    }
    let Some(reflected_name) = context.eval_reflection_class_name(identity) else {
        return Ok(None);
    };
    let source_location = eval_reflection_class_like_attributes(reflected_name, context)
        .and_then(|metadata| metadata.source_location);
    eval_reflection_source_location_result(
        method_key.as_str(),
        source_location,
        evaluated_args,
        context,
        values,
    )
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
    let exists =
        if let Some(metadata) = eval_reflection_class_like_attributes(&reflected_name, context) {
            metadata
                .property_names
                .iter()
                .any(|name| name == &property_name)
        } else {
            eval_reflection_aot_property_metadata_if_exists(
                &reflected_name,
                &property_name,
                context,
                values,
            )?
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
    let constant_names = eval_reflection_constant_names(&reflected_name, context, values)?;
    values
        .bool_value(constant_names.iter().any(|name| name == &constant_name))
        .map(Some)
}

/// Handles eval-backed `ReflectionClass::getInterfaces()` and `getTraits()` calls.
pub(in crate::interpreter) fn eval_reflection_class_get_relation_objects_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let relation_kind = if method_name.eq_ignore_ascii_case("getInterfaces") {
        "interfaces"
    } else if method_name.eq_ignore_ascii_case("getTraits") {
        "traits"
    } else {
        return Ok(None);
    };
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let names =
        if let Some(metadata) = eval_reflection_class_like_attributes(&reflected_name, context) {
            if relation_kind == "interfaces" {
                metadata.interface_names
            } else {
                metadata.trait_names
            }
        } else if relation_kind == "interfaces" {
            eval_reflection_aot_class_interface_names(&reflected_name, values)?
        } else {
            Vec::new()
        };
    eval_reflection_class_object_map_result(&names, context, values).map(Some)
}

/// Handles eval-backed `ReflectionClass::getTraitAliases()` calls.
pub(in crate::interpreter) fn eval_reflection_class_get_trait_aliases_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("getTraitAliases") {
        return Ok(None);
    }
    eval_reflection_bind_no_args(evaluated_args)?;
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    eval_reflection_string_assoc_result(context.class_trait_aliases(&reflected_name), values)
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
    if let Some(value) =
        eval_reflection_constant_value(&reflected_name, &constant_name, context, values)?
    {
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
    let filter = eval_reflection_member_filter(evaluated_args, values)?;
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let names = eval_reflection_constant_names(&reflected_name, context, values)?;
    let mut result = values.assoc_new(names.len())?;
    for name in names {
        if !eval_reflection_constant_matches_filter(&reflected_name, &name, filter, context, values)?
        {
            continue;
        }
        let Some(value) = eval_reflection_constant_value(&reflected_name, &name, context, values)?
        else {
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
    let property_names = eval_reflection_default_property_names(&reflected_name, context, values)?;
    let mut result = values.assoc_new(property_names.len())?;
    for name in property_names {
        let Some(member) =
            eval_reflection_default_property_metadata(&reflected_name, &name, context, values)?
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
    let property_names = eval_reflection_static_property_names(&reflected_name, context, values)?;
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
    let Some(member) =
        eval_reflection_static_property_metadata(&reflected_name, &property_name, context, values)?
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
    if eval_reflection_class_like_exists(&reflected_name, context) {
        let declaring_class = member
            .declaring_class_name
            .as_deref()
            .ok_or(EvalStatus::RuntimeFatal)?;
        if let Some(replaced) =
            context.set_static_property(declaring_class, &property_name, args[1])
        {
            values.release(replaced)?;
        }
    } else {
        let declaring_class = member
            .declaring_class_name
            .as_deref()
            .unwrap_or(reflected_name.as_str());
        let updated = eval_reflection_with_declaring_class_scope(declaring_class, context, || {
            values.static_property_set(&reflected_name, &property_name, args[1])
        })?;
        if updated {
            return values.null().map(Some);
        }
        return eval_reflection_static_property_missing_for_set(
            &reflected_name,
            &property_name,
            context,
            values,
        );
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
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(target) = eval_reflection_function_method_target(identity, context, values)? else {
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
        "getfilename" | "getstartline" | "getendline" => {
            eval_reflection_source_location_result(
                method_key.as_str(),
                eval_reflection_function_method_source_location(&target),
                evaluated_args,
                context,
                values,
            )
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
        "getstaticvariables" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            eval_reflection_function_method_static_variables_result(&target, context, values)
                .map(Some)
        }
        "getclosureusedvariables" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            values.array_new(0).map(Some)
        }
        _ => Ok(None),
    }
}

/// Handles eval-backed `ReflectionParameter::isArray()` and `isCallable()` calls.
pub(in crate::interpreter) fn eval_reflection_parameter_legacy_type_predicate_result(
    object: RuntimeCellHandle,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(expected_type) = eval_reflection_parameter_legacy_type_name(method_name) else {
        return Ok(None);
    };
    if !eval_reflection_object_has_class(object, "ReflectionParameter", values)? {
        return Ok(None);
    }
    eval_reflection_bind_no_args(evaluated_args)?;
    if let Some(flag_property) = eval_reflection_parameter_legacy_type_flag_property(method_name) {
        let flag = values.property_get(object, flag_property)?;
        if values.type_tag(flag)? == EVAL_TAG_BOOL {
            return Ok(Some(flag));
        }
    }
    let type_value = values.method_call(object, "getType", Vec::new())?;
    if values.is_null(type_value)? {
        return values.bool_value(false).map(Some);
    }
    if !eval_reflection_object_has_class(type_value, "ReflectionNamedType", values)? {
        return values.bool_value(false).map(Some);
    }
    let name = values.method_call(type_value, "getName", Vec::new())?;
    let bytes = values.string_bytes(name)?;
    let name = String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
    values
        .bool_value(name.eq_ignore_ascii_case(expected_type))
        .map(Some)
}

/// Handles eval-backed `ReflectionType::__toString()` calls.
pub(in crate::interpreter) fn eval_reflection_type_to_string_result(
    object: RuntimeCellHandle,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("__toString") {
        return Ok(None);
    }
    let type_kind = if eval_reflection_object_has_class(object, "ReflectionNamedType", values)? {
        Some(("ReflectionNamedType", ""))
    } else if eval_reflection_object_has_class(object, "ReflectionUnionType", values)? {
        Some(("ReflectionUnionType", "|"))
    } else if eval_reflection_object_has_class(object, "ReflectionIntersectionType", values)? {
        Some(("ReflectionIntersectionType", "&"))
    } else {
        None
    };
    let Some((class_name, separator)) = type_kind else {
        return Ok(None);
    };
    eval_reflection_bind_no_args(evaluated_args)?;
    let rendered = if class_name == "ReflectionNamedType" {
        eval_reflection_named_type_to_string(object, values)?
    } else {
        eval_reflection_composite_type_to_string(object, separator, values)?
    };
    values.string(&rendered).map(Some)
}

/// Formats one eval-visible ReflectionNamedType object from its public methods.
fn eval_reflection_named_type_to_string(
    object: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let name = eval_reflection_type_method_string(object, "getName", values)?;
    let allows_null = eval_reflection_type_method_bool(object, "allowsNull", values)?;
    if allows_null && name != "mixed" {
        Ok(format!("?{name}"))
    } else {
        Ok(name)
    }
}

/// Formats one eval-visible ReflectionUnionType or ReflectionIntersectionType object.
fn eval_reflection_composite_type_to_string(
    object: RuntimeCellHandle,
    separator: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let types = values.method_call(object, "getTypes", Vec::new())?;
    let mut names = Vec::new();
    for position in 0..values.array_len(types)? {
        let key = values.array_iter_key(types, position)?;
        let member = values.array_get(types, key)?;
        names.push(eval_reflection_type_method_string(member, "getName", values)?);
    }
    if separator == "|" && eval_reflection_type_method_bool(object, "allowsNull", values)? {
        names.push(String::from("null"));
    }
    Ok(names.join(separator))
}

/// Calls one no-arg ReflectionType method and returns its string result.
fn eval_reflection_type_method_string(
    object: RuntimeCellHandle,
    method: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let value = values.method_call(object, method, Vec::new())?;
    let bytes = values.string_bytes(value)?;
    String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Calls one no-arg ReflectionType method and returns its bool result.
fn eval_reflection_type_method_bool(
    object: RuntimeCellHandle,
    method: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let value = values.method_call(object, method, Vec::new())?;
    if values.type_tag(value)? != EVAL_TAG_BOOL {
        return Err(EvalStatus::RuntimeFatal);
    }
    values.truthy(value)
}

/// Maps a legacy ReflectionParameter predicate method to its target named type.
fn eval_reflection_parameter_legacy_type_name(method_name: &str) -> Option<&'static str> {
    if method_name.eq_ignore_ascii_case("isArray") {
        Some("array")
    } else if method_name.eq_ignore_ascii_case("isCallable") {
        Some("callable")
    } else {
        None
    }
}

/// Maps a legacy ReflectionParameter predicate method to its precomputed flag slot.
fn eval_reflection_parameter_legacy_type_flag_property(method_name: &str) -> Option<&'static str> {
    if method_name.eq_ignore_ascii_case("isArray") {
        Some("__is_array_type")
    } else if method_name.eq_ignore_ascii_case("isCallable") {
        Some("__is_callable_type")
    } else {
        None
    }
}

/// Returns whether one runtime object cell has the requested PHP class name.
fn eval_reflection_object_has_class(
    object: RuntimeCellHandle,
    class_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if values.is_null(object)? || values.type_tag(object)? != EVAL_TAG_OBJECT {
        return Ok(false);
    }
    let actual = values.object_class_name(object)?;
    let bytes = values.string_bytes(actual);
    values.release(actual)?;
    let actual = String::from_utf8(bytes?).map_err(|_| EvalStatus::RuntimeFatal)?;
    Ok(actual
        .trim_start_matches('\\')
        .eq_ignore_ascii_case(class_name))
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

/// Handles eval-backed `ReflectionProperty` hook-inspection calls.
pub(in crate::interpreter) fn eval_reflection_property_hooks_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some((declaring_class, property_name)) =
        context
            .eval_reflection_property(identity)
            .map(|(declaring_class, property_name)| {
                (declaring_class.to_string(), property_name.to_string())
            })
    else {
        return Ok(None);
    };
    let Some((property_class, property)) =
        eval_reflection_property_for_hooks(&declaring_class, &property_name, context)
    else {
        return Ok(None);
    };
    match method_name.to_ascii_lowercase().as_str() {
        "hashooks" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            let has_hooks = !eval_reflection_property_hook_kinds(&property).is_empty();
            values.bool_value(has_hooks).map(Some)
        }
        "hashook" => {
            let hook = eval_reflection_property_hook_arg(evaluated_args, context, values)?;
            values
                .bool_value(eval_reflection_property_has_hook(&property, hook))
                .map(Some)
        }
        "gethook" => {
            let hook = eval_reflection_property_hook_arg(evaluated_args, context, values)?;
            if !eval_reflection_property_has_hook(&property, hook) {
                return values.null().map(Some);
            }
            eval_reflection_property_hook_method_object(
                &property_class,
                &property,
                hook,
                context,
                values,
            )
            .map(Some)
        }
        "gethooks" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            eval_reflection_property_hook_method_array(&property_class, &property, context, values)
                .map(Some)
        }
        _ => Ok(None),
    }
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
    if context.eval_reflection_property_is_dynamic(identity) {
        let object = object.ok_or(EvalStatus::RuntimeFatal)?;
        return eval_reflection_dynamic_property_get_value(
            &declaring_class,
            &property_name,
            object,
            context,
            values,
        )
        .map(Some);
    }
    let Some(member) =
        eval_reflection_reflected_property_metadata(&declaring_class, &property_name, context, values)?
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
    if eval_reflection_class_like_exists(&declaring_class, context) {
        eval_reflection_instance_property_get_value(
            &declaring_class,
            &property_name,
            object,
            context,
            values,
        )
    } else {
        eval_reflection_aot_instance_property_get_value(
            &declaring_class,
            &property_name,
            object,
            context,
            values,
        )
    }
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
    let (object_or_value, value) = eval_reflection_property_set_value_args(evaluated_args)?;
    if context.eval_reflection_property_is_dynamic(identity) {
        let value = value.ok_or(EvalStatus::RuntimeFatal)?;
        eval_reflection_dynamic_property_set_value(
            &declaring_class,
            &property_name,
            object_or_value,
            value,
            context,
            values,
        )?;
        return values.null().map(Some);
    }
    let Some(member) =
        eval_reflection_reflected_property_metadata(&declaring_class, &property_name, context, values)?
    else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if member.is_static {
        let value = value.unwrap_or(object_or_value);
        if eval_reflection_class_like_exists(&declaring_class, context) {
            let declaring_class = member
                .declaring_class_name
                .as_deref()
                .ok_or(EvalStatus::RuntimeFatal)?;
            if let Some(replaced) =
                context.set_static_property(declaring_class, &property_name, value)
            {
                values.release(replaced)?;
            }
        } else {
            let declaring_class = member
                .declaring_class_name
                .as_deref()
                .unwrap_or(declaring_class.as_str());
            let updated = eval_reflection_with_declaring_class_scope(declaring_class, context, || {
                values.static_property_set(declaring_class, &property_name, value)
            })?;
            if !updated {
                return Err(EvalStatus::RuntimeFatal);
            }
        }
        return values.null().map(Some);
    }
    let value = value.ok_or(EvalStatus::RuntimeFatal)?;
    if eval_reflection_class_like_exists(&declaring_class, context) {
        eval_reflection_instance_property_set_value(
            &declaring_class,
            &property_name,
            object_or_value,
            value,
            context,
            values,
        )?;
    } else {
        eval_reflection_aot_instance_property_set_value(
            &declaring_class,
            &property_name,
            object_or_value,
            value,
            context,
            values,
        )?;
    }
    values.null().map(Some)
}

/// Handles `ReflectionProperty::isInitialized()` calls for eval and generated/AOT properties.
pub(in crate::interpreter) fn eval_reflection_property_is_initialized_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("isInitialized") {
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
    if context.eval_reflection_property_is_dynamic(identity) {
        let object = object.ok_or(EvalStatus::RuntimeFatal)?;
        return eval_reflection_dynamic_property_is_initialized(
            &declaring_class,
            &property_name,
            object,
            context,
            values,
        )
        .and_then(|initialized| values.bool_value(initialized))
        .map(Some);
    }
    let Some(member) =
        eval_reflection_reflected_property_metadata(&declaring_class, &property_name, context, values)?
    else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if member.is_static {
        let declaring_class = member
            .declaring_class_name
            .as_deref()
            .ok_or(EvalStatus::RuntimeFatal)?;
        let initialized = if eval_reflection_class_like_exists(declaring_class, context) {
            context
                .static_property(declaring_class, &property_name)
                .is_some()
        } else {
            eval_reflection_aot_static_property_is_initialized(
                declaring_class,
                &property_name,
                context,
                values,
            )?
        };
        return values.bool_value(initialized).map(Some);
    }
    let object = object.ok_or(EvalStatus::RuntimeFatal)?;
    if eval_reflection_class_like_exists(&declaring_class, context) {
        eval_reflection_instance_property_is_initialized(
            &declaring_class,
            &property_name,
            object,
            context,
            values,
        )
    } else {
        eval_reflection_aot_instance_property_is_initialized(
            &declaring_class,
            &property_name,
            object,
            context,
            values,
        )
    }
    .and_then(|initialized| values.bool_value(initialized))
    .map(Some)
}

/// Handles `ReflectionProperty::isLazy()` and `skipLazyInitialization()` calls.
pub(in crate::interpreter) fn eval_reflection_property_lazy_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some((declaring_class, property_name)) =
        context
            .eval_reflection_property(identity)
            .map(|(declaring_class, property_name)| {
                (declaring_class.to_string(), property_name.to_string())
            })
    else {
        return Ok(None);
    };
    if method_name.eq_ignore_ascii_case("isLazy") {
        let object = eval_reflection_property_raw_value_arg(evaluated_args)?;
        if context.eval_reflection_property_is_dynamic(identity) {
            eval_reflection_dynamic_property_validate_object(
                &declaring_class,
                object,
                context,
                values,
            )?;
            return values.bool_value(false).map(Some);
        }
        if eval_reflection_class_like_exists(&declaring_class, context) {
            eval_reflection_property_validate_object(&declaring_class, object, context, values)?;
        } else {
            eval_reflection_aot_instance_property_validate_object(
                &declaring_class,
                object,
                context,
                values,
            )?;
        }
        return values.bool_value(false).map(Some);
    }
    if method_name.eq_ignore_ascii_case("skipLazyInitialization") {
        let object = eval_reflection_property_raw_value_arg(evaluated_args)?;
        if context.eval_reflection_property_is_dynamic(identity) {
            eval_reflection_dynamic_property_validate_object(
                &declaring_class,
                object,
                context,
                values,
            )?;
            return Err(EvalStatus::RuntimeFatal);
        }
        if eval_reflection_class_like_exists(&declaring_class, context) {
            let (_, property) = eval_reflection_instance_property_target(
                &declaring_class,
                &property_name,
                object,
                context,
                values,
            )?;
            if property.is_virtual() {
                return Err(EvalStatus::RuntimeFatal);
            }
        } else {
            eval_reflection_aot_instance_property_validate_object(
                &declaring_class,
                object,
                context,
                values,
            )?;
        }
        return values.null().map(Some);
    }
    Ok(None)
}

/// Handles eval-backed `ReflectionProperty::__toString()` calls.
pub(in crate::interpreter) fn eval_reflection_property_to_string_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("__toString") {
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
    eval_reflection_bind_no_args(evaluated_args)?;
    if context.eval_reflection_property_is_dynamic(identity) {
        let member = eval_reflection_dynamic_property_metadata(&declaring_class);
        let text = eval_reflection_property_to_string(&property_name, &member);
        return values.string(&text).map(Some);
    }
    let member = if let Some(member) =
        eval_reflection_property_metadata(&declaring_class, &property_name, context)
    {
        member
    } else {
        eval_reflection_aot_property_metadata_if_exists(
            &declaring_class,
            &property_name,
            context,
            values,
        )?
        .ok_or(EvalStatus::RuntimeFatal)?
    };
    let text = eval_reflection_property_to_string(&property_name, &member);
    values.string(&text).map(Some)
}

/// Handles `ReflectionProperty::getRawValue()` and raw write calls.
pub(in crate::interpreter) fn eval_reflection_property_raw_value_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some((declaring_class, property_name)) =
        context
            .eval_reflection_property(identity)
            .map(|(declaring_class, property_name)| {
                (declaring_class.to_string(), property_name.to_string())
            })
    else {
        return Ok(None);
    };
    if method_name.eq_ignore_ascii_case("getRawValue") {
        let object = eval_reflection_property_raw_value_arg(evaluated_args)?;
        if context.eval_reflection_property_is_dynamic(identity) {
            return eval_reflection_dynamic_property_get_value(
                &declaring_class,
                &property_name,
                object,
                context,
                values,
            )
            .map(Some);
        }
        return if eval_reflection_class_like_exists(&declaring_class, context) {
            eval_reflection_instance_property_get_raw_value(
                &declaring_class,
                &property_name,
                object,
                context,
                values,
            )
        } else {
            eval_reflection_aot_instance_property_get_value(
                &declaring_class,
                &property_name,
                object,
                context,
                values,
            )
        }
        .map(Some);
    }
    if method_name.eq_ignore_ascii_case("setRawValue")
        || method_name.eq_ignore_ascii_case("setRawValueWithoutLazyInitialization")
    {
        let (object, value) = eval_reflection_property_set_raw_value_args(evaluated_args)?;
        if context.eval_reflection_property_is_dynamic(identity) {
            eval_reflection_dynamic_property_set_value(
                &declaring_class,
                &property_name,
                object,
                value,
                context,
                values,
            )?;
            return values.null().map(Some);
        }
        if eval_reflection_class_like_exists(&declaring_class, context) {
            eval_reflection_instance_property_set_raw_value(
                &declaring_class,
                &property_name,
                object,
                value,
                context,
                values,
            )?;
        } else {
            eval_reflection_aot_instance_property_set_value(
                &declaring_class,
                &property_name,
                object,
                value,
                context,
                values,
            )?;
        }
        return values.null().map(Some);
    }
    Ok(None)
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
    if !eval_reflection_constant_names(&reflected_name, context, values)?
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
    let filter = eval_reflection_member_filter(evaluated_args, values)?;
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let names = eval_reflection_constant_names(&reflected_name, context, values)?;
    let mut result = values.array_new(names.len())?;
    let mut index = 0;
    for name in &names {
        if !eval_reflection_constant_matches_filter(reflected_name.as_str(), name, filter, context, values)?
        {
            continue;
        }
        let object =
            eval_reflection_class_constant_object_result(&reflected_name, name, context, values)?;
        let key = values.int(index)?;
        result = values.array_set(result, key, object)?;
        index += 1;
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
            if let Some(member) = eval_reflection_aot_method_metadata_with_signature_if_exists(
                &reflected_name,
                &requested_name,
                context,
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
                context,
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

/// Returns generated/AOT constant names visible through eval ReflectionClass.
fn eval_reflection_aot_constant_names(
    reflected_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    let runtime_class_name = reflected_name.trim_start_matches('\\');
    let names_array = values.reflection_constant_names(runtime_class_name)?;
    let names = eval_reflection_string_array_to_vec(names_array, values)?;
    values.release(names_array)?;
    Ok(names)
}

/// Returns constant names from eval metadata or generated/AOT runtime metadata.
fn eval_reflection_constant_names(
    reflected_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    if context.has_interface(reflected_name) {
        Ok(context.interface_constant_names(reflected_name))
    } else if context.has_trait(reflected_name) {
        Ok(context.trait_constant_names(reflected_name))
    } else if context.has_class(reflected_name) || context.has_enum(reflected_name) {
        Ok(context.class_constant_names(reflected_name))
    } else {
        eval_reflection_aot_constant_names(reflected_name, values)
    }
}

/// Returns a materialized eval constant value for Reflection without visibility checks.
fn eval_reflection_eval_constant_value(
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

/// Returns a materialized eval or AOT constant value for Reflection without visibility checks.
fn eval_reflection_constant_value(
    reflected_name: &str,
    constant_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if eval_reflection_class_like_exists(reflected_name, context) {
        return Ok(eval_reflection_eval_constant_value(
            reflected_name,
            constant_name,
            context,
        ));
    }
    let runtime_class_name = reflected_name.trim_start_matches('\\');
    values.reflection_constant_value(runtime_class_name, constant_name)
}

/// Builds one eval-backed `ReflectionClassConstant` object for a visible constant name.
fn eval_reflection_class_constant_object_result(
    reflected_name: &str,
    constant_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (declaring_class_name, attributes, visibility, is_final, is_enum_case) =
        eval_reflection_class_constant_metadata(reflected_name, constant_name, context, values)?
            .ok_or(EvalStatus::RuntimeFatal)?;
    let constant_value =
        eval_reflection_constant_value(reflected_name, constant_name, context, values)?
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

/// Returns whether one class constant passes an optional `ReflectionClassConstant` filter.
fn eval_reflection_constant_matches_filter(
    reflected_name: &str,
    constant_name: &str,
    filter: Option<u64>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Some(filter) = filter else {
        return Ok(true);
    };
    Ok(eval_reflection_class_constant_metadata(reflected_name, constant_name, context, values)?
        .is_some_and(|(_, _, visibility, is_final, _)| {
            eval_reflection_class_constant_modifiers(visibility, is_final) & filter != 0
        }))
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
    let class_name = eval_reflection_class_target_name(args[0], context, values)?;
    let reflected_name = context
        .resolve_class_like_name(&class_name)
        .unwrap_or_else(|| class_name.trim_start_matches('\\').to_string());
    let Some(metadata) = eval_reflection_class_like_attributes(&reflected_name, context) else {
        let Some((flags, modifiers)) = eval_reflection_aot_class_flags(&reflected_name, values)?
        else {
            return Ok(None);
        };
        let method_names = eval_reflection_aot_member_names(
            EVAL_REFLECTION_OWNER_METHOD,
            &reflected_name,
            values,
        )?;
        let property_names = eval_reflection_aot_member_names(
            EVAL_REFLECTION_OWNER_PROPERTY,
            &reflected_name,
            values,
        )?;
        let interface_names = eval_reflection_aot_class_interface_names(&reflected_name, values)?;
        let parent_class_name = eval_reflection_aot_parent_class_name(&reflected_name, values)?;
        let attributes = context.native_class_attributes(&reflected_name);
        return eval_reflection_owner_object(
            EVAL_REFLECTION_OWNER_CLASS,
            &reflected_name,
            &attributes,
            &interface_names,
            &[],
            &method_names,
            &property_names,
            parent_class_name.as_deref(),
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

/// Resolves a ReflectionClass constructor target from a class-name string or object.
fn eval_reflection_class_target_name(
    target: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    if values.type_tag(target)? == EVAL_TAG_OBJECT {
        return eval_reflection_object_class_name(target, context, values);
    }
    eval_reflection_string_arg(target, values)
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
    let mut class_flags = values.reflection_class_flags(runtime_class_name)?.unwrap_or(0);
    if is_enum {
        class_flags &= !EVAL_REFLECTION_CLASS_FLAG_READONLY;
    }
    flags |= class_flags
        & (EVAL_REFLECTION_CLASS_FLAG_FINAL
            | EVAL_REFLECTION_CLASS_FLAG_ABSTRACT
            | EVAL_REFLECTION_CLASS_FLAG_READONLY);
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
    if is_class && !is_enum && flags & EVAL_REFLECTION_CLASS_FLAG_ABSTRACT == 0 {
        if eval_reflection_aot_lifecycle_method_allows_public_reflection(
            runtime_class_name,
            "__construct",
            values,
        )? {
            flags |= EVAL_REFLECTION_CLASS_FLAG_INSTANTIABLE;
        }
        if eval_reflection_aot_lifecycle_method_allows_public_reflection(
            runtime_class_name,
            "__clone",
            values,
        )? {
            flags |= EVAL_REFLECTION_CLASS_FLAG_CLONEABLE;
        }
    }
    let modifiers = eval_reflection_class_modifiers(
        flags & EVAL_REFLECTION_CLASS_FLAG_FINAL != 0,
        flags & EVAL_REFLECTION_CLASS_FLAG_ABSTRACT != 0,
        flags & EVAL_REFLECTION_CLASS_FLAG_READONLY != 0,
        is_enum,
    );
    Ok(Some((flags, modifiers)))
}

/// Returns whether a generated/AOT reflected class can be allocated without its constructor.
pub(in crate::interpreter) fn eval_reflection_aot_class_allows_without_constructor_allocation(
    class_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<bool>, EvalStatus> {
    let Some((flags, _)) = eval_reflection_aot_class_flags(class_name, values)? else {
        return Ok(None);
    };
    let rejected_flags = EVAL_REFLECTION_CLASS_FLAG_ABSTRACT
        | EVAL_REFLECTION_CLASS_FLAG_INTERFACE
        | EVAL_REFLECTION_CLASS_FLAG_TRAIT
        | EVAL_REFLECTION_CLASS_FLAG_ENUM;
    Ok(Some(flags & rejected_flags == 0))
}

/// Returns whether an absent or public AOT lifecycle method allows public reflection.
fn eval_reflection_aot_lifecycle_method_allows_public_reflection(
    class_name: &str,
    method_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Some(flags) = values.reflection_method_flags(class_name, method_name)? else {
        return Ok(true);
    };
    Ok(flags & EVAL_REFLECTION_MEMBER_FLAG_PUBLIC != 0
        && flags & EVAL_REFLECTION_MEMBER_FLAG_ABSTRACT == 0)
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

/// Builds an eval-backed `ReflectionParameter` object for a function or method parameter.
fn eval_reflection_parameter_new(
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let args = bind_evaluated_function_args(
        &[String::from("function"), String::from("param")],
        evaluated_args,
    )?;
    let selector = eval_reflection_parameter_selector(args[1], values)?;
    let Some(parameter) =
        eval_reflection_parameter_constructor_metadata(args[0], selector, context, values)?
    else {
        return Ok(None);
    };
    eval_reflection_parameter_object_result(&parameter, context, values).map(Some)
}

/// Resolves `ReflectionParameter` constructor target metadata.
fn eval_reflection_parameter_constructor_metadata(
    target: RuntimeCellHandle,
    selector: EvalReflectionParameterSelector,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalReflectionParameterMetadata>, EvalStatus> {
    if values.is_array_like(target)? {
        return eval_reflection_method_parameter_metadata(target, selector, context, values);
    }
    if values.type_tag(target)? == EVAL_TAG_STRING {
        return eval_reflection_function_parameter_metadata(target, selector, context, values);
    }
    Err(EvalStatus::RuntimeFatal)
}

/// Builds selected parameter metadata for an eval or native free function.
fn eval_reflection_function_parameter_metadata(
    target: RuntimeCellHandle,
    selector: EvalReflectionParameterSelector,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalReflectionParameterMetadata>, EvalStatus> {
    let requested_name = eval_reflection_string_arg(target, values)?;
    let lookup_name = requested_name.trim_start_matches('\\').to_ascii_lowercase();
    if let Some(function) = context.function(&lookup_name).cloned() {
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
        return Ok(eval_reflection_parameter_for_selector(parameters, selector));
    }
    if let Some(function) = context.native_function(&lookup_name) {
        let reflected_name = requested_name.trim_start_matches('\\');
        let parameter_names = eval_reflection_native_function_parameter_names(&function);
        let parameter_attributes = vec![Vec::new(); parameter_names.len()];
        let parameter_types: Vec<Option<EvalParameterType>> = vec![None; parameter_names.len()];
        let parameter_defaults = vec![None; parameter_names.len()];
        let parameter_is_by_ref = vec![false; parameter_names.len()];
        let parameter_is_variadic = vec![false; parameter_names.len()];
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
        return Ok(eval_reflection_parameter_for_selector(parameters, selector));
    }
    Ok(None)
}

/// Builds selected parameter metadata for an eval or generated/AOT method target.
fn eval_reflection_method_parameter_metadata(
    target: RuntimeCellHandle,
    selector: EvalReflectionParameterSelector,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalReflectionParameterMetadata>, EvalStatus> {
    if values.array_len(target)? != 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let zero = values.int(0)?;
    let one = values.int(1)?;
    let receiver = values.array_get(target, zero)?;
    let method = values.array_get(target, one)?;
    let method_name = eval_reflection_string_arg(method, values)?;
    let class_name = match values.type_tag(receiver)? {
        EVAL_TAG_OBJECT => eval_reflection_object_class_name(receiver, context, values)?,
        EVAL_TAG_STRING => eval_reflection_string_arg(receiver, values)?,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let member = if eval_reflection_class_like_exists(&class_name, context) {
        let reflected_method_name = eval_reflection_member_name(
            EVAL_REFLECTION_OWNER_METHOD,
            &class_name,
            &method_name,
            context,
        )
        .ok_or(EvalStatus::RuntimeFatal)?;
        eval_reflection_method_metadata(&class_name, &reflected_method_name, context)
            .ok_or(EvalStatus::RuntimeFatal)?
    } else {
        let Some(member) = eval_reflection_aot_method_metadata_with_signature_if_exists(
            &class_name,
            &method_name,
            context,
            values,
        )?
        else {
            return Ok(None);
        };
        member
    };
    Ok(eval_reflection_parameter_for_selector(
        member.parameters,
        selector,
    ))
}

/// Converts a `ReflectionParameter` selector runtime value to a supported selector.
fn eval_reflection_parameter_selector(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalReflectionParameterSelector, EvalStatus> {
    match values.type_tag(value)? {
        EVAL_TAG_STRING => {
            eval_reflection_string_arg(value, values).map(EvalReflectionParameterSelector::Name)
        }
        EVAL_TAG_INT => {
            eval_int_value(value, values).map(EvalReflectionParameterSelector::Position)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Selects a parameter by PHP name or zero-based position.
fn eval_reflection_parameter_for_selector(
    parameters: Vec<EvalReflectionParameterMetadata>,
    selector: EvalReflectionParameterSelector,
) -> Option<EvalReflectionParameterMetadata> {
    match selector {
        EvalReflectionParameterSelector::Name(name) => parameters
            .into_iter()
            .find(|parameter| parameter.name == name),
        EvalReflectionParameterSelector::Position(position) if position >= 0 => {
            parameters.into_iter().nth(position as usize)
        }
        EvalReflectionParameterSelector::Position(_) => None,
    }
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
    let class_name = eval_reflection_class_target_name(args[0], context, values)?;
    if !eval_reflection_class_like_exists(&class_name, context) {
        let method_name = eval_reflection_string_arg(args[1], values)?;
        if let Some(method) = eval_reflection_aot_method_metadata_with_signature_if_exists(
            &class_name,
            &method_name,
            context,
            values,
        )? {
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
    let property_name = eval_reflection_string_arg(args[1], values)?;
    if values.type_tag(args[0])? == EVAL_TAG_OBJECT {
        return eval_reflection_property_new_for_object(args[0], &property_name, context, values);
    }
    let class_name = eval_reflection_string_arg(args[0], values)?;
    if !eval_reflection_class_like_exists(&class_name, context) {
        if let Some(property) = eval_reflection_aot_property_metadata_if_exists(
            &class_name,
            &property_name,
            context,
            values,
        )? {
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

/// Builds a ReflectionProperty from an object argument, including dynamic properties.
fn eval_reflection_property_new_for_object(
    object: RuntimeCellHandle,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let class_name = eval_reflection_object_class_name(object, context, values)?;
    if let Some(property) = eval_reflection_property_metadata(&class_name, property_name, context) {
        return eval_reflection_member_object_result(
            EVAL_REFLECTION_OWNER_PROPERTY,
            property_name,
            &property,
            context,
            values,
        )
        .map(Some);
    }
    if !eval_reflection_object_dynamic_property_exists(object, property_name, values)? {
        return Ok(None);
    }
    let property = eval_reflection_dynamic_property_metadata(&class_name);
    eval_reflection_member_object_result(
        EVAL_REFLECTION_OWNER_PROPERTY,
        property_name,
        &property,
        context,
        values,
    )
    .map(Some)
}

/// Returns the class name for an object passed to a Reflection constructor.
fn eval_reflection_object_class_name(
    object: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let identity = values.object_identity(object)?;
    if let Some(class) = context.dynamic_object_class(identity) {
        return Ok(class.name().trim_start_matches('\\').to_string());
    }
    let class_name = values.object_class_name(object)?;
    let bytes = values.string_bytes(class_name);
    values.release(class_name)?;
    let class_name = String::from_utf8(bytes?).map_err(|_| EvalStatus::RuntimeFatal)?;
    Ok(class_name.trim_start_matches('\\').to_string())
}

/// Returns whether one object has a public dynamic property by exact PHP name.
fn eval_reflection_object_dynamic_property_exists(
    object: RuntimeCellHandle,
    property_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if property_name.contains('\0') {
        return Ok(false);
    }
    let property_count = values.object_property_len(object)?;
    for position in 0..property_count {
        let key = values.object_property_iter_key(object, position)?;
        let key_bytes = values.string_bytes(key);
        values.release(key)?;
        if key_bytes? == property_name.as_bytes() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Builds PHP reflection metadata for a public dynamic object property.
fn eval_reflection_dynamic_property_metadata(class_name: &str) -> EvalReflectionMemberMetadata {
    EvalReflectionMemberMetadata {
        declaring_class_name: Some(class_name.trim_start_matches('\\').to_string()),
        source_location: None,
        attributes: Vec::new(),
        visibility: EvalVisibility::Public,
        is_static: false,
        is_final: false,
        is_abstract: false,
        is_readonly: false,
        is_promoted: false,
        is_dynamic: true,
        modifiers: eval_reflection_property_modifiers(
            EvalVisibility::Public,
            None,
            false,
            false,
            false,
            false,
            false,
        ),
        type_metadata: None,
        return_type_metadata: None,
        default_value: None,
        required_parameter_count: 0,
        parameters: Vec::new(),
    }
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
    let declaring_class_name = values
        .reflection_method_declaring_class(runtime_class_name, method_name)?
        .unwrap_or_else(|| runtime_class_name.to_string());
    Ok(Some(eval_reflection_aot_method_metadata(
        &declaring_class_name,
        method_name,
        flags,
        Vec::new(),
        None,
    )))
}

/// Returns generated AOT ReflectionMethod metadata with registered signature details.
fn eval_reflection_aot_method_metadata_with_signature_if_exists(
    class_name: &str,
    method_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalReflectionMemberMetadata>, EvalStatus> {
    let runtime_class_name = class_name.trim_start_matches('\\');
    let Some(flags) = values.reflection_method_flags(runtime_class_name, method_name)? else {
        return Ok(None);
    };
    let declaring_class_name = values
        .reflection_method_declaring_class(runtime_class_name, method_name)?
        .unwrap_or_else(|| runtime_class_name.to_string());
    let mut signature =
        eval_reflection_aot_method_signature(&declaring_class_name, method_name, flags, context);
    if signature.is_none() && declaring_class_name != runtime_class_name {
        signature =
            eval_reflection_aot_method_signature(runtime_class_name, method_name, flags, context);
    }
    let attributes = eval_reflection_aot_method_attributes(
        runtime_class_name,
        &declaring_class_name,
        method_name,
        context,
    );
    Ok(Some(eval_reflection_aot_method_metadata(
        &declaring_class_name,
        method_name,
        flags,
        attributes,
        signature.as_ref(),
    )))
}

/// Returns generated/AOT method dispatch metadata for interpreter-only runtime decisions.
pub(in crate::interpreter) fn eval_aot_method_dispatch_metadata(
    class_name: &str,
    method_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(String, EvalVisibility, bool, bool)>, EvalStatus> {
    Ok(
        eval_reflection_aot_method_metadata_if_exists(class_name, method_name, values)?.map(
            |member| {
                (
                    member
                        .declaring_class_name
                        .unwrap_or_else(|| class_name.trim_start_matches('\\').to_string()),
                    member.visibility,
                    member.is_static,
                    member.is_abstract,
                )
            },
        ),
    )
}

/// Converts AOT method flag metadata into the eval ReflectionMethod shape.
fn eval_reflection_aot_method_metadata(
    class_name: &str,
    method_name: &str,
    flags: u64,
    attributes: Vec<EvalAttribute>,
    signature: Option<&NativeCallableSignature>,
) -> EvalReflectionMemberMetadata {
    let visibility = if flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE != 0 {
        EvalVisibility::Private
    } else if flags & EVAL_REFLECTION_MEMBER_FLAG_PROTECTED != 0 {
        EvalVisibility::Protected
    } else {
        EvalVisibility::Public
    };
    let required_parameter_count =
        signature.map_or(0, NativeCallableSignature::required_param_count);
    let parameters = signature.map_or_else(Vec::new, |signature| {
        eval_reflection_native_callable_parameters(class_name, method_name, flags, signature)
    });
    let return_type_metadata = signature
        .and_then(NativeCallableSignature::return_type)
        .and_then(eval_reflection_parameter_type_metadata);
    EvalReflectionMemberMetadata {
        declaring_class_name: Some(class_name.trim_start_matches('\\').to_string()),
        source_location: None,
        attributes,
        visibility,
        is_static: flags & EVAL_REFLECTION_MEMBER_FLAG_STATIC != 0,
        is_final: flags & EVAL_REFLECTION_MEMBER_FLAG_FINAL != 0,
        is_abstract: flags & EVAL_REFLECTION_MEMBER_FLAG_ABSTRACT != 0,
        is_readonly: false,
        is_promoted: false,
        is_dynamic: false,
        modifiers: eval_reflection_method_modifiers_from_flags(flags),
        type_metadata: None,
        return_type_metadata,
        default_value: None,
        required_parameter_count,
        parameters,
    }
}

/// Returns registered generated/AOT method attributes for one reflected method.
fn eval_reflection_aot_method_attributes(
    runtime_class_name: &str,
    declaring_class_name: &str,
    method_name: &str,
    context: &ElephcEvalContext,
) -> Vec<EvalAttribute> {
    let attributes = context.native_method_attributes(declaring_class_name, method_name);
    if !attributes.is_empty() || declaring_class_name == runtime_class_name {
        return attributes;
    }
    context.native_method_attributes(runtime_class_name, method_name)
}

/// Selects the registered native signature for an AOT method-like member.
fn eval_reflection_aot_method_signature(
    class_name: &str,
    method_name: &str,
    flags: u64,
    context: &ElephcEvalContext,
) -> Option<NativeCallableSignature> {
    if method_name.eq_ignore_ascii_case("__construct") {
        return context.native_constructor_signature(class_name);
    }
    if flags & EVAL_REFLECTION_MEMBER_FLAG_STATIC != 0 {
        context.native_static_method_signature(class_name, method_name)
    } else {
        context.native_method_signature(class_name, method_name)
    }
}

/// Builds ReflectionParameter metadata for one registered native AOT signature.
fn eval_reflection_native_callable_parameters(
    declaring_class_name: &str,
    method_name: &str,
    flags: u64,
    signature: &NativeCallableSignature,
) -> Vec<EvalReflectionParameterMetadata> {
    let names = eval_reflection_native_callable_parameter_names(signature);
    let parameter_count = names.len();
    let parameter_types = eval_reflection_native_callable_parameter_types(signature);
    let has_type_flags = parameter_types
        .iter()
        .map(Option::is_some)
        .collect::<Vec<_>>();
    let parameter_attributes = vec![Vec::new(); parameter_count];
    let defaults = eval_reflection_native_callable_parameter_defaults(signature);
    let by_ref_flags = (0..parameter_count)
        .map(|index| signature.param_by_ref(index))
        .collect::<Vec<_>>();
    let variadic_flags = (0..parameter_count)
        .map(|index| signature.param_variadic(index))
        .collect::<Vec<_>>();
    let declaring_function = EvalReflectionDeclaringFunctionMetadata {
        name: method_name.to_ascii_lowercase(),
        declaring_class_name: Some(declaring_class_name.trim_start_matches('\\').to_string()),
        attributes: Vec::new(),
        flags,
        required_parameter_count: signature.required_param_count(),
    };
    eval_reflection_parameters_from_names_and_type_flags(
        Some(declaring_class_name.trim_start_matches('\\')),
        Some(&declaring_function),
        &names,
        &has_type_flags,
        &parameter_types,
        &parameter_attributes,
        &defaults,
        &by_ref_flags,
        &variadic_flags,
        &[],
    )
}

/// Returns declared parameter type metadata for a registered native callable.
fn eval_reflection_native_callable_parameter_types(
    signature: &NativeCallableSignature,
) -> Vec<Option<EvalParameterType>> {
    (0..signature.param_count())
        .map(|index| signature.param_type(index).cloned())
        .collect()
}

/// Returns parameter names for a registered native callable, filling missing bridge names.
fn eval_reflection_native_callable_parameter_names(
    signature: &NativeCallableSignature,
) -> Vec<String> {
    (0..signature.param_count())
        .map(|index| {
            signature
                .param_names()
                .get(index)
                .filter(|name| !name.is_empty())
                .cloned()
                .unwrap_or_else(|| format!("arg{}", index))
        })
        .collect()
}

/// Converts registered scalar native defaults into eval constant expressions.
fn eval_reflection_native_callable_parameter_defaults(
    signature: &NativeCallableSignature,
) -> Vec<Option<EvalExpr>> {
    (0..signature.param_count())
        .map(|index| {
            signature
                .param_default(index)
                .map(eval_reflection_native_callable_default_expr)
        })
        .collect()
}

/// Converts one registered native default into an eval constant expression.
fn eval_reflection_native_callable_default_expr(default: &NativeCallableDefault) -> EvalExpr {
    match default {
        NativeCallableDefault::Null => EvalExpr::Const(EvalConst::Null),
        NativeCallableDefault::Bool(value) => EvalExpr::Const(EvalConst::Bool(*value)),
        NativeCallableDefault::Int(value) => EvalExpr::Const(EvalConst::Int(*value)),
        NativeCallableDefault::Float(value) => EvalExpr::Const(EvalConst::Float(*value)),
        NativeCallableDefault::String(value) => EvalExpr::Const(EvalConst::String(value.clone())),
        NativeCallableDefault::EmptyArray => EvalExpr::Array(Vec::new()),
        NativeCallableDefault::Object { class_name, args } => EvalExpr::NewObject {
            class_name: class_name.clone(),
            args: args
                .iter()
                .map(eval_reflection_native_callable_default_arg)
                .collect(),
        },
    }
}

/// Converts one native object-default constructor argument into a positional eval call arg.
fn eval_reflection_native_callable_default_arg(default: &NativeCallableDefault) -> EvalCallArg {
    EvalCallArg::positional(eval_reflection_native_callable_default_expr(default))
}

/// Returns generated AOT ReflectionProperty metadata when the runtime table has a matching row.
fn eval_reflection_aot_property_metadata_if_exists(
    class_name: &str,
    property_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalReflectionMemberMetadata>, EvalStatus> {
    let runtime_class_name = class_name.trim_start_matches('\\');
    let Some(flags) = values.reflection_property_flags(runtime_class_name, property_name)? else {
        return Ok(None);
    };
    let declaring_class_name = values
        .reflection_property_declaring_class(runtime_class_name, property_name)?
        .unwrap_or_else(|| runtime_class_name.to_string());
    let type_metadata = eval_reflection_aot_property_type_metadata(
        runtime_class_name,
        &declaring_class_name,
        property_name,
        context,
    );
    let default_value = eval_reflection_aot_property_default_value(
        runtime_class_name,
        &declaring_class_name,
        property_name,
        context,
    );
    let attributes = eval_reflection_aot_property_attributes(
        runtime_class_name,
        &declaring_class_name,
        property_name,
        context,
    );
    Ok(Some(eval_reflection_aot_property_metadata(
        &declaring_class_name,
        flags,
        attributes,
        type_metadata,
        default_value,
    )))
}

/// Returns registered generated/AOT property type metadata for one reflected property.
fn eval_reflection_aot_property_type_metadata(
    runtime_class_name: &str,
    declaring_class_name: &str,
    property_name: &str,
    context: &ElephcEvalContext,
) -> Option<EvalReflectionParameterTypeMetadata> {
    context
        .native_property_type(declaring_class_name, property_name)
        .or_else(|| context.native_property_type(runtime_class_name, property_name))
        .as_ref()
        .and_then(eval_reflection_parameter_type_metadata)
}

/// Returns registered generated/AOT property default metadata for one reflected property.
fn eval_reflection_aot_property_default_value(
    runtime_class_name: &str,
    declaring_class_name: &str,
    property_name: &str,
    context: &ElephcEvalContext,
) -> Option<EvalExpr> {
    context
        .native_property_default(declaring_class_name, property_name)
        .or_else(|| context.native_property_default(runtime_class_name, property_name))
        .as_ref()
        .map(eval_reflection_native_callable_default_expr)
}

/// Returns registered generated/AOT property attributes for one reflected property.
fn eval_reflection_aot_property_attributes(
    runtime_class_name: &str,
    declaring_class_name: &str,
    property_name: &str,
    context: &ElephcEvalContext,
) -> Vec<EvalAttribute> {
    let attributes = context.native_property_attributes(declaring_class_name, property_name);
    if !attributes.is_empty() || declaring_class_name == runtime_class_name {
        return attributes;
    }
    context.native_property_attributes(runtime_class_name, property_name)
}

/// Converts AOT property flag metadata into the eval ReflectionProperty shape.
fn eval_reflection_aot_property_metadata(
    class_name: &str,
    flags: u64,
    attributes: Vec<EvalAttribute>,
    type_metadata: Option<EvalReflectionParameterTypeMetadata>,
    default_value: Option<EvalExpr>,
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
    let is_virtual = flags & EVAL_REFLECTION_MEMBER_FLAG_VIRTUAL != 0;
    let mut modifiers = eval_reflection_property_modifiers(
        visibility,
        None,
        is_static,
        is_final,
        is_abstract,
        is_readonly,
        is_virtual,
    );
    if flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE_SET != 0 {
        modifiers |= 32 | 4096;
    } else if flags & EVAL_REFLECTION_MEMBER_FLAG_PROTECTED_SET != 0 {
        modifiers |= 2048;
    }
    EvalReflectionMemberMetadata {
        declaring_class_name: Some(class_name.trim_start_matches('\\').to_string()),
        source_location: None,
        attributes,
        visibility,
        is_static,
        is_final,
        is_abstract,
        is_readonly,
        is_promoted: flags & EVAL_REFLECTION_MEMBER_FLAG_PROMOTED != 0,
        is_dynamic: false,
        modifiers,
        type_metadata,
        return_type_metadata: None,
        default_value,
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
    let constant_name = eval_reflection_string_arg(args[1], values)?;
    let Some((declaring_class_name, attributes, visibility, is_final, is_enum_case)) =
        eval_reflection_class_constant_metadata(&class_name, &constant_name, context, values)?
    else {
        return if eval_reflection_class_like_exists(&class_name, context) {
            Err(EvalStatus::RuntimeFatal)
        } else {
            Ok(None)
        };
    };
    let constant_value = eval_reflection_constant_value(&class_name, &constant_name, context, values)?
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
    let attrs = eval_reflection_attribute_array_result(
        attributes,
        eval_reflection_attribute_target(owner_kind),
        context,
        values,
    )?;
    let interface_names_array = eval_reflection_string_array_result(interface_names, values)?;
    let trait_names_array = eval_reflection_string_array_result(trait_names, values)?;
    let method_names_array = eval_reflection_string_array_result(method_names, values)?;
    let property_names_array = eval_reflection_string_array_result(property_names, values)?;
    let is_eval_class = owner_kind == EVAL_REFLECTION_OWNER_CLASS
        && eval_reflection_class_like_exists(reflected_name, context);
    let method_objects = if owner_kind == EVAL_REFLECTION_OWNER_CLASS && include_class_members {
        if is_eval_class {
            eval_reflection_member_object_array_result(
                EVAL_REFLECTION_OWNER_METHOD,
                reflected_name,
                method_names,
                None,
                context,
                values,
            )?
        } else {
            eval_reflection_aot_member_object_array_result(
                EVAL_REFLECTION_OWNER_METHOD,
                reflected_name,
                method_names,
                None,
                context,
                values,
            )?
        }
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
        if is_eval_class {
            eval_reflection_member_object_array_result(
                EVAL_REFLECTION_OWNER_PROPERTY,
                reflected_name,
                property_names,
                None,
                context,
                values,
            )?
        } else {
            eval_reflection_aot_member_object_array_result(
                EVAL_REFLECTION_OWNER_PROPERTY,
                reflected_name,
                property_names,
                None,
                context,
                values,
            )?
        }
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
            if flags & EVAL_REFLECTION_MEMBER_FLAG_DYNAMIC != 0 {
                let identity = values.object_identity(object)?;
                context.register_eval_dynamic_reflection_property(
                    identity,
                    declaring_class,
                    reflected_name,
                );
            } else {
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
        let Some((flags, modifiers)) = eval_reflection_aot_class_flags(class_name, values)? else {
            return values.bool_value(false);
        };
        let runtime_class_name = class_name.trim_start_matches('\\');
        let method_names = eval_reflection_aot_member_names(
            EVAL_REFLECTION_OWNER_METHOD,
            runtime_class_name,
            values,
        )?;
        let property_names = eval_reflection_aot_member_names(
            EVAL_REFLECTION_OWNER_PROPERTY,
            runtime_class_name,
            values,
        )?;
        let interface_names =
            eval_reflection_aot_class_interface_names(runtime_class_name, values)?;
        let parent_class_name = eval_reflection_aot_parent_class_name(runtime_class_name, values)?;
        let attributes = context.native_class_attributes(runtime_class_name);
        return eval_reflection_owner_object(
            EVAL_REFLECTION_OWNER_CLASS,
            runtime_class_name,
            &attributes,
            &interface_names,
            &[],
            &method_names,
            &property_names,
            parent_class_name.as_deref(),
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
        );
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
        let interface_names = eval_reflection_aot_class_interface_names(class_name, values)?;
        let attributes = context.native_class_attributes(class_name);
        return eval_reflection_owner_object_with_members(
            EVAL_REFLECTION_OWNER_CLASS,
            class_name.trim_start_matches('\\'),
            &attributes,
            &interface_names,
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

/// Returns the generated/AOT parent class name for a reflected class, if any.
fn eval_reflection_aot_parent_class_name(
    class_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<String>, EvalStatus> {
    let runtime_class_name = class_name.trim_start_matches('\\');
    let class_cell = values.string(runtime_class_name)?;
    let parent_cell = match values.parent_class_name(class_cell) {
        Ok(parent_cell) => parent_cell,
        Err(err) => {
            values.release(class_cell)?;
            return Err(err);
        }
    };
    values.release(class_cell)?;
    let parent_bytes = match values.string_bytes(parent_cell) {
        Ok(parent_bytes) => parent_bytes,
        Err(err) => {
            values.release(parent_cell)?;
            return Err(err);
        }
    };
    values.release(parent_cell)?;
    let parent_name = String::from_utf8(parent_bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
    if parent_name.is_empty() {
        Ok(None)
    } else {
        Ok(Some(parent_name))
    }
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

/// Builds a string-keyed PHP associative array from owned string pairs.
fn eval_reflection_string_assoc_result(
    pairs: Vec<(String, String)>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.assoc_new(pairs.len())?;
    for (key, value) in pairs {
        let key = values.string(&key)?;
        let value = values.string(&value)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Builds a name-keyed PHP array of full ReflectionClass objects.
fn eval_reflection_class_object_map_result(
    names: &[String],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.assoc_new(names.len())?;
    for name in names {
        let key = values.string(name)?;
        let object = eval_reflection_full_class_object_result(name, context, values)?;
        result = values.array_set(result, key, object)?;
    }
    Ok(result)
}

/// Maps a synthetic reflection owner kind to PHP's `Attribute::TARGET_*` bitmask.
fn eval_reflection_attribute_target(owner_kind: u64) -> u64 {
    match owner_kind {
        EVAL_REFLECTION_OWNER_CLASS => EVAL_REFLECTION_ATTRIBUTE_TARGET_CLASS,
        EVAL_REFLECTION_OWNER_FUNCTION => EVAL_REFLECTION_ATTRIBUTE_TARGET_FUNCTION,
        EVAL_REFLECTION_OWNER_METHOD => EVAL_REFLECTION_ATTRIBUTE_TARGET_METHOD,
        EVAL_REFLECTION_OWNER_PROPERTY => EVAL_REFLECTION_ATTRIBUTE_TARGET_PROPERTY,
        EVAL_REFLECTION_OWNER_CLASS_CONSTANT
        | EVAL_REFLECTION_OWNER_ENUM_UNIT_CASE
        | EVAL_REFLECTION_OWNER_ENUM_BACKED_CASE => EVAL_REFLECTION_ATTRIBUTE_TARGET_CLASS_CONSTANT,
        _ => 0,
    }
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
    let attrs = eval_reflection_attribute_array_result(
        &parameter.attributes,
        EVAL_REFLECTION_ATTRIBUTE_TARGET_PARAMETER,
        context,
        values,
    )?;
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
    let class_value = eval_reflection_parameter_class_value(parameter, context, values)?;
    let default_value = eval_reflection_parameter_default_value(parameter, context, values)?;
    let default_value_constant_name = match parameter.default_value_constant_name.as_deref() {
        Some(name) => values.string(name)?,
        None => values.null()?,
    };
    let constructor = values.null()?;
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
        default_value_constant_name,
        class_value,
        constructor,
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
    values.release(default_value_constant_name)?;
    values.release(class_value)?;
    values.release(constructor)?;
    Ok(object)
}

/// Materializes the legacy ReflectionParameter::getClass() value for known named object types.
fn eval_reflection_parameter_class_value(
    parameter: &EvalReflectionParameterMetadata,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match eval_reflection_parameter_class_name(parameter) {
        Some(class_name) => eval_reflection_shallow_class_object_result(class_name, context, values),
        None => values.null(),
    }
}

/// Returns the retained object type name used by ReflectionParameter::getClass().
fn eval_reflection_parameter_class_name(
    parameter: &EvalReflectionParameterMetadata,
) -> Option<&str> {
    match &parameter.type_metadata.as_ref()?.kind {
        EvalReflectionParameterTypeKind::Named(named_type) if !named_type.is_builtin => {
            Some(named_type.name.as_str())
        }
        _ => None,
    }
}

/// Materializes one ReflectionParameter default using the declaring class scope when present.
fn eval_reflection_parameter_default_value(
    parameter: &EvalReflectionParameterMetadata,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(default) = parameter.default_value.as_ref() else {
        return values.null();
    };
    let Some(class_name) = parameter.declaring_class_name.as_deref() else {
        return eval_method_parameter_default(default, context, values);
    };
    context.push_class_scope(class_name.to_string());
    context.push_called_class_scope(class_name.to_string());
    let result = eval_method_parameter_default(default, context, values);
    context.pop_called_class_scope();
    context.pop_class_scope();
    result
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
        if !eval_reflection_class_like_exists(class_name, context) {
            if let Some(member) = eval_reflection_aot_method_metadata_with_signature_if_exists(
                class_name,
                "__construct",
                context,
                values,
            )? {
                return eval_reflection_member_object_result(
                    EVAL_REFLECTION_OWNER_METHOD,
                    "__construct",
                    &member,
                    context,
                    values,
                );
            }
        }
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
    if owner_kind == EVAL_REFLECTION_OWNER_PROPERTY && (member.modifiers & 512) != 0 {
        flags |= EVAL_REFLECTION_MEMBER_FLAG_VIRTUAL;
    }
    if owner_kind == EVAL_REFLECTION_OWNER_PROPERTY && (member.modifiers & 4096) != 0 {
        flags |= EVAL_REFLECTION_MEMBER_FLAG_PRIVATE_SET | EVAL_REFLECTION_MEMBER_FLAG_FINAL;
    } else if owner_kind == EVAL_REFLECTION_OWNER_PROPERTY
        && (member.modifiers & 2048) != 0
        && !member.is_readonly
    {
        flags |= EVAL_REFLECTION_MEMBER_FLAG_PROTECTED_SET;
    }
    if member.is_dynamic {
        flags |= EVAL_REFLECTION_MEMBER_FLAG_DYNAMIC;
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
            eval_reflection_aot_method_metadata_with_signature_if_exists(
                class_name, name, context, values,
            )?
        } else {
            eval_reflection_aot_property_metadata_if_exists(class_name, name, context, values)?
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

/// Returns generated AOT interface names for one reflected class-like symbol.
fn eval_reflection_aot_class_interface_names(
    class_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    let runtime_class_name = class_name.trim_start_matches('\\');
    let names_array = values.reflection_class_interface_names(runtime_class_name)?;
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
            source_location: class.source_location(),
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
            source_location: interface.source_location(),
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
            source_location: trait_decl.source_location(),
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
            source_location: enum_decl.source_location(),
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
    set_visibility: Option<EvalVisibility>,
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
    match set_visibility {
        Some(EvalVisibility::Private) => modifiers |= 32 | 4096,
        Some(EvalVisibility::Protected) => modifiers |= 2048,
        _ if is_readonly && visibility == EvalVisibility::Public => modifiers |= 2048,
        _ => {}
    }
    modifiers
}

/// Formats one reflected property similarly to PHP's `ReflectionProperty::__toString()`.
fn eval_reflection_property_to_string(
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

/// Returns PHP's lowercase label for one reflected visibility.
fn eval_reflection_visibility_label(visibility: EvalVisibility) -> &'static str {
    match visibility {
        EvalVisibility::Public => "public",
        EvalVisibility::Protected => "protected",
        EvalVisibility::Private => "private",
    }
}

/// Formats retained ReflectionType metadata for `ReflectionProperty::__toString()`.
fn eval_reflection_type_metadata_to_string(
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
fn eval_reflection_default_expr_to_string(default: &EvalExpr) -> Option<String> {
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
fn eval_reflection_property_is_virtual(property: &EvalClassProperty) -> bool {
    property.is_virtual()
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
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(String, Vec<EvalAttribute>, EvalVisibility, bool, bool)>, EvalStatus> {
    if let Some(enum_decl) = context.enum_decl(class_name) {
        if let Some(case) = enum_decl.case(constant_name) {
            return Ok(Some((
                enum_decl.name().to_string(),
                case.attributes().to_vec(),
                EvalVisibility::Public,
                false,
                true,
            )));
        }
    }
    if let Some(metadata) = context
        .class_constant(class_name, constant_name)
        .map(|(declaring_class, constant)| {
            (
                declaring_class,
                constant.attributes().to_vec(),
                constant.visibility(),
                constant.is_final(),
                false,
            )
        }) {
        return Ok(Some(metadata));
    }
    let runtime_class_name = class_name.trim_start_matches('\\');
    let Some(flags) = values.reflection_constant_flags(runtime_class_name, constant_name)? else {
        return Ok(None);
    };
    let declaring_class = values
        .reflection_constant_declaring_class(runtime_class_name, constant_name)?
        .unwrap_or_else(|| runtime_class_name.to_string());
    let attributes = eval_reflection_aot_constant_attributes(
        runtime_class_name,
        &declaring_class,
        constant_name,
        context,
    );
    let visibility = if flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE != 0 {
        EvalVisibility::Private
    } else if flags & EVAL_REFLECTION_MEMBER_FLAG_PROTECTED != 0 {
        EvalVisibility::Protected
    } else {
        EvalVisibility::Public
    };
    Ok(Some((
        declaring_class,
        attributes,
        visibility,
        flags & EVAL_REFLECTION_MEMBER_FLAG_FINAL != 0,
        flags & EVAL_REFLECTION_MEMBER_FLAG_ENUM_CASE != 0,
    )))
}

/// Returns registered generated/AOT class-constant attributes for one reflected constant.
fn eval_reflection_aot_constant_attributes(
    runtime_class_name: &str,
    declaring_class_name: &str,
    constant_name: &str,
    context: &ElephcEvalContext,
) -> Vec<EvalAttribute> {
    let attributes = context.native_constant_attributes(declaring_class_name, constant_name);
    if !attributes.is_empty() || declaring_class_name == runtime_class_name {
        return attributes;
    }
    context.native_constant_attributes(runtime_class_name, constant_name)
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
fn eval_reflection_default_property_metadata(
    reflected_name: &str,
    property_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalReflectionMemberMetadata>, EvalStatus> {
    if let Some(member) = eval_reflection_property_metadata(reflected_name, property_name, context) {
        return Ok(Some(member));
    }
    eval_reflection_aot_property_metadata_if_exists(reflected_name, property_name, context, values)
}

/// Returns eval or generated/AOT metadata for a materialized `ReflectionProperty`.
fn eval_reflection_reflected_property_metadata(
    declaring_class: &str,
    property_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalReflectionMemberMetadata>, EvalStatus> {
    if let Some(member) = eval_reflection_property_metadata(declaring_class, property_name, context) {
        return Ok(Some(member));
    }
    eval_reflection_aot_property_metadata_if_exists(declaring_class, property_name, context, values)
}

/// Returns eval-declared property names for reflection APIs that do not use AOT lists.
fn eval_reflection_eval_property_names(
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
fn eval_reflection_static_property_names(
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
fn eval_reflection_static_property_metadata(
    reflected_name: &str,
    property_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalReflectionMemberMetadata>, EvalStatus> {
    eval_reflection_reflected_property_metadata(reflected_name, property_name, context, values)
}

/// Returns the current eval or generated/AOT static property value.
fn eval_reflection_static_property_value(
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
            .map(|default| eval_method_parameter_default(default, context, values))
            .transpose();
    }
    let declaring_class = member
        .declaring_class_name
        .as_deref()
        .unwrap_or(reflected_name);
    eval_reflection_with_declaring_class_scope(declaring_class, context, || {
        values.static_property_get(reflected_name, property_name)
    })
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

/// Binds the required object argument for `ReflectionProperty::getRawValue()`.
fn eval_reflection_property_raw_value_arg(
    evaluated_args: Vec<EvaluatedCallArg>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let args = bind_evaluated_function_args(&[String::from("object")], evaluated_args)?;
    Ok(args[0])
}

/// Binds the object and value arguments for `ReflectionProperty::setRawValue()`.
fn eval_reflection_property_set_raw_value_args(
    evaluated_args: Vec<EvaluatedCallArg>,
) -> Result<(RuntimeCellHandle, RuntimeCellHandle), EvalStatus> {
    let args = bind_evaluated_function_args(
        &[String::from("object"), String::from("value")],
        evaluated_args,
    )?;
    Ok((args[0], args[1]))
}

/// Returns the eval property metadata eligible for ReflectionProperty hook APIs.
fn eval_reflection_property_for_hooks(
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
}

/// Binds the `PropertyHookType $type` argument used by ReflectionProperty hook APIs.
fn eval_reflection_property_hook_arg(
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalReflectionPropertyHook, EvalStatus> {
    let args = bind_evaluated_function_args(&[String::from("type")], evaluated_args)?;
    eval_reflection_property_hook_type(args[0], context, values)
}

/// Converts one synthetic `PropertyHookType` object into an eval reflection hook kind.
fn eval_reflection_property_hook_type(
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
fn eval_reflection_property_hook_kinds(
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
fn eval_reflection_property_has_hook(
    property: &EvalClassProperty,
    hook: EvalReflectionPropertyHook,
) -> bool {
    match hook {
        EvalReflectionPropertyHook::Get => property.has_get_hook() || property.requires_get_hook(),
        EvalReflectionPropertyHook::Set => property.has_set_hook() || property.requires_set_hook(),
    }
}

/// Builds PHP's string-keyed ReflectionMethod map returned by `getHooks()`.
fn eval_reflection_property_hook_method_array(
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
fn eval_reflection_property_hook_method_object(
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
fn eval_reflection_property_hook_method_metadata(
    declaring_class: &str,
    property: &EvalClassProperty,
    hook: EvalReflectionPropertyHook,
) -> EvalReflectionMemberMetadata {
    let parameters = eval_reflection_property_hook_parameters(declaring_class, property, hook);
    let required_parameter_count = parameters.len();
    EvalReflectionMemberMetadata {
        declaring_class_name: Some(declaring_class.to_string()),
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
        return_type_metadata: eval_reflection_property_hook_return_type(property, hook),
        default_value: None,
        required_parameter_count,
        parameters,
    }
}

/// Builds the synthetic setter parameter metadata exposed by PHP hook reflection.
fn eval_reflection_property_hook_parameters(
    declaring_class: &str,
    property: &EvalClassProperty,
    hook: EvalReflectionPropertyHook,
) -> Vec<EvalReflectionParameterMetadata> {
    if !matches!(hook, EvalReflectionPropertyHook::Set) {
        return Vec::new();
    }
    let type_metadata = property
        .property_type()
        .and_then(eval_reflection_parameter_type_metadata);
    let has_type = type_metadata.is_some();
    let is_array_type = eval_reflection_parameter_has_named_type(type_metadata.as_ref(), "array");
    let is_callable_type =
        eval_reflection_parameter_has_named_type(type_metadata.as_ref(), "callable");
    let declaring_function = EvalReflectionDeclaringFunctionMetadata {
        name: hook.reflected_method_name(property.name()),
        declaring_class_name: Some(declaring_class.to_string()),
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
fn eval_reflection_property_hook_return_type(
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
fn eval_reflection_property_hook_synthetic_method_name(method_name: &str) -> Option<String> {
    let body = method_name.strip_prefix('$')?;
    let (property_name, hook_name) = body.rsplit_once("::")?;
    match hook_name {
        "get" => Some(EvalReflectionPropertyHook::Get.synthetic_method_name(property_name)),
        "set" => Some(EvalReflectionPropertyHook::Set.synthetic_method_name(property_name)),
        _ => None,
    }
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

/// Dispatches one reflected method invocation through eval or AOT bridges.
fn eval_reflection_method_invoke_dispatch(
    declaring_class: &str,
    method_name: &str,
    object: RuntimeCellHandle,
    method_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let lookup_method_name = eval_reflection_property_hook_synthetic_method_name(method_name)
        .unwrap_or_else(|| method_name.to_string());
    if let Some((method_class, method)) = context.class_method(declaring_class, &lookup_method_name)
    {
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

/// Invokes one reflected generated/AOT method when it fits the bridge slice.
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
    if member.is_abstract {
        return Err(EvalStatus::RuntimeFatal);
    }
    if member.is_static {
        let args = bind_native_callable_args(
            context.native_static_method_signature(declaring_class, method_name),
            method_args,
            values,
        )?;
        return eval_reflection_with_declaring_class_scope(declaring_class, context, || {
            values.static_method_call(declaring_class, method_name, args)
        });
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
        values,
    )?;
    eval_reflection_with_declaring_class_scope(declaring_class, context, || {
        values.method_call(object, method_name, args)
    })
}

/// Runs a reflected AOT invocation with the declaring class as visibility scope.
fn eval_reflection_with_declaring_class_scope<T>(
    declaring_class: &str,
    context: &mut ElephcEvalContext,
    action: impl FnOnce() -> Result<T, EvalStatus>,
) -> Result<T, EvalStatus> {
    context.push_class_scope(declaring_class.to_string());
    context.push_called_class_scope(declaring_class.to_string());
    let result = action();
    context.pop_called_class_scope();
    context.pop_class_scope();
    result
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
    values.property_set(object, &storage_property_name, value)?;
    let identity = values.object_identity(object)?;
    context.mark_dynamic_property_initialized(identity, &storage_property_name);
    Ok(())
}

/// Reads one generated/AOT instance property through ReflectionProperty semantics.
fn eval_reflection_aot_instance_property_get_value(
    declaring_class: &str,
    property_name: &str,
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_reflection_aot_instance_property_validate_object(
        declaring_class,
        object,
        context,
        values,
    )?;
    eval_reflection_with_declaring_class_scope(declaring_class, context, || {
        values.property_get(object, property_name)
    })
}

/// Writes one generated/AOT instance property through ReflectionProperty semantics.
fn eval_reflection_aot_instance_property_set_value(
    declaring_class: &str,
    property_name: &str,
    object: RuntimeCellHandle,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    eval_reflection_aot_instance_property_validate_object(
        declaring_class,
        object,
        context,
        values,
    )?;
    eval_reflection_with_declaring_class_scope(declaring_class, context, || {
        values.property_set(object, property_name, value)
    })
}

/// Checks one generated/AOT instance property initialization marker through ReflectionProperty.
fn eval_reflection_aot_instance_property_is_initialized(
    declaring_class: &str,
    property_name: &str,
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    eval_reflection_aot_instance_property_validate_object(
        declaring_class,
        object,
        context,
        values,
    )?;
    eval_reflection_with_declaring_class_scope(declaring_class, context, || {
        values.property_is_initialized(object, property_name)
    })
}

/// Checks one generated/AOT static property initialization marker through ReflectionProperty.
fn eval_reflection_aot_static_property_is_initialized(
    declaring_class: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    eval_reflection_with_declaring_class_scope(declaring_class, context, || {
        values.static_property_is_initialized(declaring_class, property_name)
    })
}

/// Verifies a generated/AOT ReflectionProperty instance target is compatible.
fn eval_reflection_aot_instance_property_validate_object(
    declaring_class: &str,
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if values.is_null(object)? || values.type_tag(object)? != EVAL_TAG_OBJECT {
        return Err(EvalStatus::RuntimeFatal);
    }
    let is_instance = dynamic_object_is_a(object, declaring_class, false, context, values)?
        .map_or_else(|| values.object_is_a(object, declaring_class, false), Ok)?;
    if is_instance {
        Ok(())
    } else {
        Err(EvalStatus::RuntimeFatal)
    }
}

/// Returns whether one eval instance property is initialized for ReflectionProperty.
fn eval_reflection_instance_property_is_initialized(
    declaring_class: &str,
    property_name: &str,
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let (_, property) = eval_reflection_instance_property_target(
        declaring_class,
        property_name,
        object,
        context,
        values,
    )?;
    if property.is_virtual() {
        return Ok(true);
    }
    let identity = values.object_identity(object)?;
    let storage_property_name = eval_instance_property_storage_name(declaring_class, &property);
    Ok(context.dynamic_property_is_initialized(identity, &storage_property_name))
}

/// Reads one eval instance property through ReflectionProperty raw-storage semantics.
fn eval_reflection_instance_property_get_raw_value(
    declaring_class: &str,
    property_name: &str,
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (_, property) = eval_reflection_instance_property_target(
        declaring_class,
        property_name,
        object,
        context,
        values,
    )?;
    if property.is_virtual() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let storage_property_name = eval_instance_property_storage_name(declaring_class, &property);
    values.property_get(object, &storage_property_name)
}

/// Writes one eval instance property through ReflectionProperty raw-storage semantics.
fn eval_reflection_instance_property_set_raw_value(
    declaring_class: &str,
    property_name: &str,
    object: RuntimeCellHandle,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let (_, property) = eval_reflection_instance_property_target(
        declaring_class,
        property_name,
        object,
        context,
        values,
    )?;
    if property.is_virtual() {
        return Err(EvalStatus::RuntimeFatal);
    }
    validate_eval_reflection_property_write(declaring_class, &property, context)?;
    let storage_property_name = eval_instance_property_storage_name(declaring_class, &property);
    values.property_set(object, &storage_property_name, value)?;
    let identity = values.object_identity(object)?;
    context.mark_dynamic_property_initialized(identity, &storage_property_name);
    Ok(())
}

/// Reads a public dynamic property through ReflectionProperty semantics.
fn eval_reflection_dynamic_property_get_value(
    declaring_class: &str,
    property_name: &str,
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_reflection_dynamic_property_validate_object(declaring_class, object, context, values)?;
    values.property_get(object, property_name)
}

/// Writes a public dynamic property through ReflectionProperty semantics.
fn eval_reflection_dynamic_property_set_value(
    declaring_class: &str,
    property_name: &str,
    object: RuntimeCellHandle,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    eval_reflection_dynamic_property_validate_object(declaring_class, object, context, values)?;
    values.property_set(object, property_name, value)?;
    let identity = values.object_identity(object)?;
    context.mark_dynamic_property_initialized(identity, property_name);
    Ok(())
}

/// Returns whether a public dynamic property currently exists on the target object.
fn eval_reflection_dynamic_property_is_initialized(
    declaring_class: &str,
    property_name: &str,
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    eval_reflection_dynamic_property_validate_object(declaring_class, object, context, values)?;
    eval_reflection_object_dynamic_property_exists(object, property_name, values)
}

/// Validates the object argument used by dynamic ReflectionProperty operations.
fn eval_reflection_dynamic_property_validate_object(
    declaring_class: &str,
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let object_class_name = eval_reflection_object_class_name(object, context, values)?;
    if eval_reflection_class_like_exists(declaring_class, context) {
        if context.class_is_a(&object_class_name, declaring_class, false) {
            return Ok(());
        }
    } else if object_class_name
        .trim_start_matches('\\')
        .eq_ignore_ascii_case(declaring_class.trim_start_matches('\\'))
    {
        return Ok(());
    }
    Err(EvalStatus::RuntimeFatal)
}

/// Validates the object argument shared by non-mutating ReflectionProperty instance APIs.
fn eval_reflection_property_validate_object(
    declaring_class: &str,
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let identity = values.object_identity(object)?;
    let object_class_name = context
        .dynamic_object_class(identity)
        .map(|class| class.name().to_string())
        .ok_or(EvalStatus::RuntimeFatal)?;
    if !context.class_is_a(&object_class_name, declaring_class, false) {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(())
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
        .map(|(position, name)| {
            let has_type = has_type_flags.get(position).copied().unwrap_or(false);
            let default_value = defaults.get(position).and_then(Clone::clone);
            let default_value_constant_name = default_value
                .as_ref()
                .and_then(eval_reflection_default_constant_name);
            let type_metadata = parameter_types
                .get(position)
                .and_then(Option::as_ref)
                .and_then(eval_reflection_parameter_type_metadata)
                .filter(|_| has_type);
            let is_array_type =
                eval_reflection_parameter_has_named_type(type_metadata.as_ref(), "array");
            let is_callable_type =
                eval_reflection_parameter_has_named_type(type_metadata.as_ref(), "callable");
            EvalReflectionParameterMetadata {
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
                has_type,
                allows_null: eval_reflection_parameter_allows_null(
                    has_type,
                    type_metadata.as_ref(),
                    default_value.as_ref(),
                ),
                is_array_type,
                is_callable_type,
                type_metadata,
                default_value,
                default_value_constant_name,
            }
        })
        .collect()
}

/// Returns whether retained parameter metadata is one named type with the requested name.
fn eval_reflection_parameter_has_named_type(
    type_metadata: Option<&EvalReflectionParameterTypeMetadata>,
    expected_name: &str,
) -> bool {
    matches!(
        type_metadata,
        Some(EvalReflectionParameterTypeMetadata {
            kind: EvalReflectionParameterTypeKind::Named(named)
        }) if named.name.eq_ignore_ascii_case(expected_name)
    )
}

/// Returns PHP's ReflectionParameter default-constant name for retained eval defaults.
fn eval_reflection_default_constant_name(default: &EvalExpr) -> Option<String> {
    match default {
        EvalExpr::ConstFetch(name) => Some(name.clone()),
        EvalExpr::NamespacedConstFetch { name, .. } => Some(name.clone()),
        EvalExpr::ClassConstantFetch {
            class_name,
            constant,
        } => Some(format!("{}::{}", class_name, constant)),
        _ => None,
    }
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

/// Returns PHP's `ReflectionParameter::allowsNull()` value for retained metadata.
fn eval_reflection_parameter_allows_null(
    has_type: bool,
    type_metadata: Option<&EvalReflectionParameterTypeMetadata>,
    default_value: Option<&EvalExpr>,
) -> bool {
    !has_type
        || default_value.is_some_and(|default| matches!(default, EvalExpr::Const(EvalConst::Null)))
        || type_metadata.is_some_and(eval_reflection_type_allows_null)
}

/// Returns whether one retained ReflectionType metadata value accepts null.
fn eval_reflection_type_allows_null(type_metadata: &EvalReflectionParameterTypeMetadata) -> bool {
    match &type_metadata.kind {
        EvalReflectionParameterTypeKind::Named(named_type) => named_type.allows_null,
        EvalReflectionParameterTypeKind::Union(union_type) => union_type.allows_null,
        EvalReflectionParameterTypeKind::Intersection(_) => false,
    }
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
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalReflectionFunctionMethodTarget>, EvalStatus> {
    if let Some(name) = context.eval_reflection_function_name(identity) {
        let function = context.function(&name.to_ascii_lowercase());
        let is_variadic = function
            .is_some_and(|function| function.parameter_is_variadic().iter().any(|flag| *flag));
        let source_location = function.and_then(EvalFunction::source_location);
        let return_type_metadata = function
            .and_then(EvalFunction::return_type)
            .and_then(eval_reflection_parameter_type_metadata);
        let static_key = function.map(|function| function.name().to_string());
        let static_variables = function
            .map(|function| static_var_initializers(function.body()))
            .unwrap_or_default();
        return Ok(Some(EvalReflectionFunctionMethodTarget::Function {
            name: name.to_string(),
            static_key,
            static_variables,
            source_location,
            is_variadic,
            return_type_metadata,
        }));
    }
    let Some((declaring_class, method_name)) = context.eval_reflection_method(identity) else {
        return Ok(None);
    };
    let method_metadata = if let Some(method_metadata) =
        eval_reflection_method_metadata(declaring_class, method_name, context)
    {
        Some(method_metadata)
    } else {
        eval_reflection_aot_method_metadata_with_signature_if_exists(
            declaring_class,
            method_name,
            context,
            values,
        )?
    };
    let is_variadic = method_metadata.as_ref().is_some_and(|method| {
        method
            .parameters
            .iter()
            .any(|parameter| parameter.is_variadic)
    });
    let source_location = method_metadata.as_ref().and_then(|method| method.source_location);
    let return_type_metadata = method_metadata.and_then(|method| method.return_type_metadata);
    let static_method = eval_reflection_eval_method_static_target(declaring_class, method_name, context);
    let declaring_class = static_method
        .as_ref()
        .map(|(declaring_class, _)| declaring_class.clone());
    let static_key = static_method
        .as_ref()
        .map(|(declaring_class, method)| eval_method_static_local_key(declaring_class, method.name()));
    let static_variables = static_method
        .as_ref()
        .map(|(_, method)| static_var_initializers(method.body()))
        .unwrap_or_default();
    Ok(Some(EvalReflectionFunctionMethodTarget::Method {
        declaring_class,
        name: method_name.to_string(),
        static_key,
        static_variables,
        source_location,
        is_variadic,
        return_type_metadata,
    }))
}

/// Returns an eval method body that can contribute ReflectionMethod static locals.
fn eval_reflection_eval_method_static_target(
    declaring_class: &str,
    method_name: &str,
    context: &ElephcEvalContext,
) -> Option<(String, EvalClassMethod)> {
    if context.has_class(declaring_class) || context.has_enum(declaring_class) {
        return context.class_method(declaring_class, method_name);
    }
    let trait_decl = context.trait_decl(declaring_class)?;
    trait_decl
        .methods()
        .iter()
        .find(|method| method.name().eq_ignore_ascii_case(method_name))
        .map(|method| (trait_decl.name().to_string(), method.clone()))
}

/// Builds the static-local storage key shared by method execution and reflection.
fn eval_method_static_local_key(class_name: &str, method_name: &str) -> String {
    format!("{}::{}", class_name.trim_start_matches('\\'), method_name)
}

/// Builds the associative `getStaticVariables()` result for eval-backed reflection.
fn eval_reflection_function_method_static_variables_result(
    target: &EvalReflectionFunctionMethodTarget,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (Some(static_key), static_variables, declaring_class) =
        eval_reflection_function_method_static_target(target)
    else {
        return values.array_new(0);
    };
    let mut result = values.assoc_new(static_variables.len())?;
    for variable in static_variables {
        let key = values.string(&variable.name)?;
        let value = eval_reflection_static_local_value(
            static_key,
            variable,
            declaring_class,
            context,
            values,
        )?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Returns static-local storage details retained for a reflected eval function or method.
fn eval_reflection_function_method_static_target(
    target: &EvalReflectionFunctionMethodTarget,
) -> (
    Option<&str>,
    &[EvalStaticVarInitializer],
    Option<&str>,
) {
    match target {
        EvalReflectionFunctionMethodTarget::Function {
            static_key,
            static_variables,
            ..
        } => (static_key.as_deref(), static_variables, None),
        EvalReflectionFunctionMethodTarget::Method {
            declaring_class,
            static_key,
            static_variables,
            ..
        } => (
            static_key.as_deref(),
            static_variables,
            declaring_class.as_deref(),
        ),
    }
}

/// Returns the retained current static value or initializes it for reflection.
fn eval_reflection_static_local_value(
    static_key: &str,
    variable: &EvalStaticVarInitializer,
    declaring_class: Option<&str>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(value) = context.static_local(static_key, &variable.name) {
        return values.retain(value);
    }
    let value = eval_reflection_static_local_initializer_value(
        static_key,
        &variable.init,
        declaring_class,
        context,
        values,
    )?;
    if let Some(replaced) =
        context.set_static_local(static_key.to_string(), variable.name.clone(), value)
    {
        values.release(replaced)?;
    }
    values.retain(value)
}

/// Evaluates a static-local initializer with PHP magic class/function context.
fn eval_reflection_static_local_initializer_value(
    static_key: &str,
    init: &EvalExpr,
    declaring_class: Option<&str>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(declaring_class) = declaring_class {
        context.push_class_scope(declaring_class.to_string());
        context.push_called_class_scope(declaring_class.to_string());
    }
    context.push_function(static_key.to_string());
    let mut scope = ElephcEvalScope::new();
    let result = eval_expr(init, context, &mut scope, values);
    for cell in scope.drain_owned_cells() {
        values.release(cell)?;
    }
    context.pop_function();
    if declaring_class.is_some() {
        context.pop_called_class_scope();
        context.pop_class_scope();
    }
    result
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

/// Returns source file or line metadata for eval-backed reflection objects.
fn eval_reflection_source_location_result(
    method_key: &str,
    source_location: Option<EvalSourceLocation>,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    eval_reflection_bind_no_args(evaluated_args)?;
    let Some(source_location) = source_location else {
        return values.bool_value(false).map(Some);
    };
    match method_key {
        "getfilename" => values.string(&context.eval_file_magic()).map(Some),
        "getstartline" => values.int(source_location.start_line()).map(Some),
        "getendline" => values.int(source_location.end_line()).map(Some),
        _ => Ok(None),
    }
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

/// Returns eval-fragment source metadata for a ReflectionFunction or ReflectionMethod target.
fn eval_reflection_function_method_source_location(
    target: &EvalReflectionFunctionMethodTarget,
) -> Option<EvalSourceLocation> {
    match target {
        EvalReflectionFunctionMethodTarget::Function {
            source_location, ..
        }
        | EvalReflectionFunctionMethodTarget::Method {
            source_location, ..
        } => *source_location,
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
    if parameter.allows_null {
        flags |= EVAL_REFLECTION_PARAMETER_FLAG_ALLOWS_NULL;
    }
    if parameter.default_value_constant_name.is_some() {
        flags |= EVAL_REFLECTION_PARAMETER_FLAG_DEFAULT_VALUE_CONSTANT;
    }
    if parameter.is_array_type {
        flags |= EVAL_REFLECTION_PARAMETER_FLAG_ARRAY_TYPE;
    }
    if parameter.is_callable_type {
        flags |= EVAL_REFLECTION_PARAMETER_FLAG_CALLABLE_TYPE;
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
        "reflectionparameter" => Some(EVAL_REFLECTION_OWNER_PARAMETER),
        "reflectionclassconstant" => Some(EVAL_REFLECTION_OWNER_CLASS_CONSTANT),
        "reflectionenumunitcase" => Some(EVAL_REFLECTION_OWNER_ENUM_UNIT_CASE),
        "reflectionenumbackedcase" => Some(EVAL_REFLECTION_OWNER_ENUM_BACKED_CASE),
        _ => None,
    }
}
