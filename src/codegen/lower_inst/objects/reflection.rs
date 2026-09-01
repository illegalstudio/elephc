//! Purpose:
//! Lowers metadata-aware allocation for builtin Reflection owner objects in the
//! EIR backend.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::lower_object_new()`.
//!
//! Key details:
//! - `ReflectionClass`, `ReflectionObject`, `ReflectionFunction`, `ReflectionMethod`,
//!   `ReflectionProperty`, `ReflectionClassConstant`, and `ReflectionEnum*`
//!   constructors are compile-time metadata lookups that populate private
//!   metadata slots instead of running their public empty bodies.

use crate::codegen::platform::Arch;
use crate::codegen::literal_defaults::{
    emit_boxed_bool_literal_to_result, emit_boxed_float_literal_to_result,
    emit_boxed_int_literal_to_result, emit_boxed_null_literal_to_result,
    emit_boxed_string_literal_default_to_result, emit_empty_assoc_array_literal_to_result,
    emit_string_literal_default_to_result,
};
use crate::codegen::{
    abi, emit_array_value_type_stamp, emit_box_current_owned_value_as_mixed,
    emit_box_current_value_as_mixed, emit_release_pushed_refcounted_temp_after_array_push,
    runtime_value_tag, CodegenIrError, Result, UNINITIALIZED_TYPED_PROPERTY_SENTINEL,
};
use crate::ir::{
    Immediate, Instruction, LocalSlotId, Op, Terminator, TraitMethodInfo, ValueDef, ValueId,
};
use crate::names::{
    php_symbol_key, property_hook_get_method, property_hook_set_method,
    static_property_symbol,
};
use crate::parser::ast::{BinOp, Expr, ExprKind, StaticReceiver, TypeExpr, Visibility};
use crate::types::{
    is_php_integer_array_key, AttrArgEntry, AttrArgValue, AttrKey, EnumCaseInfo, EnumCaseValue,
    FunctionSig, InterfaceInfo, PhpType,
};

use super::super::super::context::FunctionContext;

mod owner_dispatch;
mod constructorless_usage;
mod owner_emission;
mod class_metadata;
mod callable_metadata;
mod property_metadata;
mod constant_metadata;
mod class_traits;
mod class_members;
mod member_resolution;
mod method_members;
mod property_members;
mod default_members;
mod parameter_defaults;
mod names_constants;
mod operand_extract;
mod string_attrs_emit;
mod member_properties_emit;
mod collection_emit;
mod default_emit;
mod member_object_emit;
mod parameter_property_emit;
mod type_object_emit;
mod flags_offsets;

use owner_emission::*;
use constructorless_usage::*;
use class_metadata::*;
use callable_metadata::*;
use property_metadata::*;
use constant_metadata::*;
use class_traits::*;
use class_members::*;
use member_resolution::*;
use method_members::*;
use property_members::*;
use default_members::*;
use parameter_defaults::*;
use names_constants::*;
use operand_extract::*;
use string_attrs_emit::*;
use member_properties_emit::*;
use collection_emit::*;
use default_emit::*;
use member_object_emit::*;
use parameter_property_emit::*;
use type_object_emit::*;
use flags_offsets::*;

pub(super) use owner_dispatch::{is_reflection_owner_class, lower_reflection_owner_new};

/// Compile-time metadata used to populate one Reflection owner object.
struct ReflectionOwnerMetadata {
    reflected_name: Option<String>,
    attr_names: Vec<String>,
    attr_args: Vec<Option<Vec<AttrArgEntry>>>,
    interface_names: Vec<String>,
    trait_names: Vec<String>,
    trait_aliases: Vec<(String, String)>,
    parent_names: Vec<String>,
    method_names: Vec<String>,
    property_names: Vec<String>,
    constant_names: Vec<String>,
    constant_members: Vec<ReflectionConstantMember>,
    default_property_members: Vec<ReflectionDefaultPropertyMember>,
    static_property_members: Vec<ReflectionStaticPropertyMember>,
    constant_reflection_members: Vec<ReflectionListedMember>,
    enum_case_members: Vec<ReflectionListedMember>,
    method_members: Vec<ReflectionListedMember>,
    property_members: Vec<ReflectionListedMember>,
    property_hook_members: Vec<(String, ReflectionListedMember)>,
    constructor_member: Option<ReflectionListedMember>,
    parent_class_name: Option<String>,
    constant_value: Option<ReflectionConstantValue>,
    backing_value: Option<ReflectionConstantValue>,
    is_enum_case: bool,
    parameter_members: Vec<ReflectionParameterMember>,
    type_metadata: Option<ReflectionParameterTypeMetadata>,
    property_default_value: Option<ReflectionParameterDefaultValue>,
    required_parameter_count: i64,
    is_deprecated: bool,
    is_generator: bool,
    prototype_member: Option<Box<ReflectionListedMember>>,
    is_final: bool,
    is_abstract: bool,
    is_interface: bool,
    is_trait: bool,
    is_enum: bool,
    is_readonly: bool,
    is_anonymous: bool,
    is_instantiable: bool,
    is_cloneable: bool,
    is_iterable: bool,
    modifiers: i64,
    member_flags: ReflectionMemberFlags,
}

/// Compile-time metadata for one class/interface/trait/enum constant reflector.
struct ReflectionClassConstantMetadata {
    declaring_class_name: String,
    attr_names: Vec<String>,
    attr_args: Vec<Option<Vec<AttrArgEntry>>>,
    value: ReflectionConstantValue,
    type_metadata: Option<ReflectionParameterTypeMetadata>,
    visibility: Visibility,
    is_final: bool,
}

/// Metadata for one member object returned by `ReflectionClass::getMethods()` or `getProperties()`.
#[derive(Clone)]
struct ReflectionListedMember {
    name: String,
    declaring_class_name: Option<String>,
    attr_names: Vec<String>,
    attr_args: Vec<Option<Vec<AttrArgEntry>>>,
    constant_value: Option<ReflectionConstantValue>,
    backing_value: Option<ReflectionConstantValue>,
    is_enum_case: bool,
    flags: ReflectionMemberFlags,
    modifiers: i64,
    type_metadata: Option<ReflectionParameterTypeMetadata>,
    default_value: Option<ReflectionParameterDefaultValue>,
    property_hook_members: Vec<(String, ReflectionListedMember)>,
    required_parameter_count: i64,
    is_deprecated: bool,
    is_generator: bool,
    prototype_member: Option<Box<ReflectionListedMember>>,
    parameters: Vec<ReflectionParameterMember>,
}

/// Metadata for one object returned by `ReflectionMethod::getParameters()`.
#[derive(Clone)]
struct ReflectionParameterMember {
    name: String,
    declaring_class_name: Option<String>,
    declaring_function: Option<ReflectionDeclaringFunctionMember>,
    attr_names: Vec<String>,
    attr_args: Vec<Option<Vec<AttrArgEntry>>>,
    position: i64,
    is_optional: bool,
    is_variadic: bool,
    is_passed_by_reference: bool,
    is_promoted: bool,
    has_type: bool,
    allows_null: bool,
    is_array_type: bool,
    is_callable_type: bool,
    type_metadata: Option<ReflectionParameterTypeMetadata>,
    default_value: Option<ReflectionParameterDefaultValue>,
    default_value_constant_name: Option<String>,
}

/// Metadata needed for `ReflectionParameter::getDeclaringFunction()`.
#[derive(Clone)]
enum ReflectionDeclaringFunctionMember {
    Function {
        name: String,
        attr_names: Vec<String>,
        attr_args: Vec<Option<Vec<AttrArgEntry>>>,
        required_parameter_count: i64,
        type_metadata: Option<ReflectionParameterTypeMetadata>,
        is_deprecated: bool,
        is_generator: bool,
    },
    Method {
        name: String,
        declaring_class_name: Option<String>,
        attr_names: Vec<String>,
        attr_args: Vec<Option<Vec<AttrArgEntry>>>,
        flags: ReflectionMemberFlags,
        required_parameter_count: i64,
        type_metadata: Option<ReflectionParameterTypeMetadata>,
        is_deprecated: bool,
        is_generator: bool,
    },
}

/// Metadata for one `ReflectionType` object returned by `ReflectionParameter::getType()`.
#[derive(Clone)]
enum ReflectionParameterTypeMetadata {
    Named(ReflectionNamedTypeMetadata),
    Union(ReflectionUnionTypeMetadata),
    Intersection(ReflectionIntersectionTypeMetadata),
}

/// Metadata for one `ReflectionNamedType` returned by `ReflectionParameter::getType()`.
#[derive(Clone)]
struct ReflectionNamedTypeMetadata {
    name: String,
    allows_null: bool,
    is_builtin: bool,
}

/// Metadata for one `ReflectionUnionType` returned by `ReflectionParameter::getType()`.
#[derive(Clone)]
struct ReflectionUnionTypeMetadata {
    types: Vec<ReflectionNamedTypeMetadata>,
    allows_null: bool,
}

/// Metadata for one `ReflectionIntersectionType` returned by `ReflectionParameter::getType()`.
#[derive(Clone)]
struct ReflectionIntersectionTypeMetadata {
    types: Vec<ReflectionNamedTypeMetadata>,
}

/// Compile-time default forms returned by `ReflectionParameter::getDefaultValue()`.
#[derive(Clone)]
enum ReflectionParameterDefaultValue {
    Int(i64),
    Bool(bool),
    Float(f64),
    Str(String),
    Null,
    Object {
        class_name: String,
        args: Vec<ReflectionParameterDefaultValue>,
    },
    Array(Vec<ReflectionParameterDefaultValue>),
    AssocArray(Vec<ReflectionDefaultAssocEntry>),
}

/// Metadata for one key/value pair in an associative Reflection default array.
#[derive(Clone)]
struct ReflectionDefaultAssocEntry {
    key: ReflectionDefaultArrayKey,
    value: ReflectionParameterDefaultValue,
}

/// Normalized PHP key forms for associative Reflection default arrays.
#[derive(Clone)]
enum ReflectionDefaultArrayKey {
    Int(i64),
    Str(String),
}

/// Metadata for one constant entry returned by `ReflectionClass::getConstants()`.
#[derive(Clone)]
struct ReflectionConstantMember {
    name: String,
    value: ReflectionConstantValue,
}

/// Metadata for one property entry returned by `ReflectionClass::getDefaultProperties()`.
#[derive(Clone)]
struct ReflectionDefaultPropertyMember {
    name: String,
    value: ReflectionParameterDefaultValue,
}

/// Metadata for one live static-property value exposed by ReflectionClass.
struct ReflectionStaticPropertyMember {
    name: String,
    declaring_class_name: String,
    php_type: PhpType,
    is_declared: bool,
}

/// Compile-time value forms supported by Reflection constant metadata emission.
#[derive(Clone)]
enum ReflectionConstantValue {
    Int(i64),
    Bool(bool),
    Float(f64),
    Str(String),
    Null,
    EnumCase {
        enum_name: String,
        case_name: String,
    },
}

/// Compile-time parameter selector from `ReflectionParameter::__construct()`.
enum ReflectionParameterSelector {
    Name(String),
    Position(i64),
}

/// Boolean metadata exposed by ReflectionMethod and ReflectionProperty predicates.
#[derive(Clone, Copy, Default)]
struct ReflectionMemberFlags {
    is_static: bool,
    is_public: bool,
    is_protected: bool,
    is_private: bool,
    is_final: bool,
    is_abstract: bool,
    is_readonly: bool,
    is_promoted: bool,
    is_virtual: bool,
    is_dynamic: bool,
}

/// Runtime class candidate used when object reflection must dispatch by object class id.
struct ReflectionRuntimeClassCandidate {
    class_name: String,
    class_id: u64,
}
