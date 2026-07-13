//! Purpose:
//! Coordinates eval-aware Reflection dispatch and shared metadata shapes.
//! Owner-specific APIs, construction, lookup, formatting, and runtime access
//! live in focused child modules.
//!
//! Called from:
//! - `crate::interpreter::expressions::eval_expr()` for `new Reflection*`.
//! - `crate::interpreter::statements` for Reflection method dispatch.
//!
//! Key details:
//! - Shared metadata types stay here so every Reflection owner uses one contract.
//! - Generated/AOT targets use focused runtime hooks for supported point lookups.

mod callable_api;
mod class_api;
mod class_construction;
mod class_lookup;
mod class_member_api;
mod constant_construction;
mod flags;
mod formatting;
mod function_construction;
mod function_metadata;
mod invocation;
mod member_api;
mod member_construction;
mod member_metadata;
mod owner_materialization;
mod parameter_construction;
mod parameter_metadata;
mod property_access;
mod property_helpers;

use super::*;
use crate::context::{
    NativeCallableArrayDefaultElement, NativeCallableArrayDefaultKey,
    NativeCallableObjectDefaultArg,
};
use crate::eval_ir::EvalSourceLocation;

pub(in crate::interpreter) use callable_api::*;
pub(in crate::interpreter) use class_api::*;
pub(in crate::interpreter) use class_construction::*;
use class_lookup::*;
pub(in crate::interpreter) use class_member_api::*;
use constant_construction::*;
use flags::*;
use formatting::*;
use function_construction::*;
use function_metadata::*;
use invocation::*;
pub(in crate::interpreter) use member_api::*;
pub(in crate::interpreter) use member_construction::*;
use member_metadata::*;
use owner_materialization::*;
use parameter_construction::*;
use parameter_metadata::*;
use property_access::*;
use property_helpers::*;

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
const EVAL_REFLECTION_CLASS_SOURCE_LINE_MASK: u64 = 0x00ff_ffff;
const EVAL_REFLECTION_CLASS_SOURCE_START_SHIFT: u64 = 16;
const EVAL_REFLECTION_CLASS_SOURCE_END_SHIFT: u64 = 40;
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
const EVAL_REFLECTION_CALLABLE_FLAG_DEPRECATED: u64 = 16384;
const EVAL_REFLECTION_METHOD_SOURCE_LINE_MASK: u64 = 0x00ff_ffff;
const EVAL_REFLECTION_METHOD_SOURCE_START_SHIFT: u64 = 16;
const EVAL_REFLECTION_METHOD_SOURCE_END_SHIFT: u64 = 40;
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

/// Exception category and message for failed ReflectionClass instantiation.
pub(in crate::interpreter) enum EvalReflectionInstantiationError {
    ThrowableError(String),
    ReflectionException(String),
}

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
    source_file: Option<String>,
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
    settable_type_metadata: Option<EvalReflectionParameterTypeMetadata>,
    return_type_metadata: Option<EvalReflectionParameterTypeMetadata>,
    default_value: Option<EvalExpr>,
    default_value_trait_origin: Option<String>,
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

/// PHP-visible magic constant scope for one reflected parameter default.
#[derive(Clone)]
struct EvalReflectionParameterMagicScope {
    function_name: String,
    method_name: String,
    class_name: Option<String>,
    trait_name: Option<String>,
}

/// Eval metadata needed for `ReflectionParameter::getDeclaringFunction()`.
#[derive(Clone)]
struct EvalReflectionDeclaringFunctionMetadata {
    name: String,
    declaring_class_name: Option<String>,
    magic_scope: Option<EvalReflectionParameterMagicScope>,
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
#[derive(Clone)]
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
        closure_captures: Vec<EvalClosureCaptureBinding>,
        parameters: Vec<EvalReflectionParameterMetadata>,
        source_location: Option<EvalSourceLocation>,
        closure_target: Option<EvalClosureObjectTarget>,
        is_variadic: bool,
        is_static: bool,
        is_closure: bool,
        is_deprecated: bool,
        return_type_metadata: Option<EvalReflectionParameterTypeMetadata>,
    },
    Method {
        declaring_class: Option<String>,
        name: String,
        static_key: Option<String>,
        static_variables: Vec<EvalStaticVarInitializer>,
        source_file: Option<String>,
        parameters: Vec<EvalReflectionParameterMetadata>,
        source_location: Option<EvalSourceLocation>,
        visibility: Option<EvalVisibility>,
        is_variadic: bool,
        is_static: bool,
        is_final: bool,
        is_abstract: bool,
        is_deprecated: bool,
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
        Some(EVAL_REFLECTION_OWNER_OBJECT) => {
            eval_reflection_object_new(evaluated_args, context, values)
        }
        Some(EVAL_REFLECTION_OWNER_ENUM) => {
            eval_reflection_enum_new(evaluated_args, context, values)
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
