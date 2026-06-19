//! Purpose:
//! Lowers metadata-aware allocation for builtin Reflection owner objects in the
//! EIR backend.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::objects::lower_object_new()`.
//!
//! Key details:
//! - `ReflectionClass`, `ReflectionFunction`, `ReflectionMethod`, `ReflectionProperty`,
//!   `ReflectionClassConstant`, and `ReflectionEnum*`
//!   constructors are compile-time metadata lookups that populate private
//!   `__name`/`__attrs` slots instead of running their public empty bodies.

use crate::codegen::platform::Arch;
use crate::codegen::{abi, emit_box_current_value_as_mixed, runtime_value_tag};
use crate::codegen_ir::literal_defaults::{
    emit_boxed_bool_literal_to_result, emit_boxed_float_literal_to_result,
    emit_boxed_int_literal_to_result, emit_boxed_null_literal_to_result,
    emit_boxed_string_literal_default_to_result, emit_empty_assoc_array_literal_to_result,
};
use crate::codegen_ir::{CodegenIrError, Result};
use crate::ir::{Immediate, Instruction, Op, TraitMethodInfo, ValueDef, ValueId};
use crate::names::{
    enum_case_symbol, php_symbol_key, property_hook_get_method, property_hook_set_method,
};
use crate::parser::ast::{BinOp, Expr, ExprKind, StaticReceiver, TypeExpr, Visibility};
use crate::types::{
    AttrArgValue, EnumCaseInfo, EnumCaseValue, FunctionSig, InterfaceInfo, PhpType,
};

use super::super::super::context::FunctionContext;

/// Compile-time metadata used to populate one Reflection owner object.
struct ReflectionOwnerMetadata {
    reflected_name: Option<String>,
    attr_names: Vec<String>,
    attr_args: Vec<Option<Vec<AttrArgValue>>>,
    interface_names: Vec<String>,
    trait_names: Vec<String>,
    parent_names: Vec<String>,
    method_names: Vec<String>,
    property_names: Vec<String>,
    constant_names: Vec<String>,
    constant_members: Vec<ReflectionConstantMember>,
    default_property_members: Vec<ReflectionDefaultPropertyMember>,
    constant_reflection_members: Vec<ReflectionListedMember>,
    method_members: Vec<ReflectionListedMember>,
    property_members: Vec<ReflectionListedMember>,
    constructor_member: Option<ReflectionListedMember>,
    parent_class_name: Option<String>,
    constant_value: Option<ReflectionConstantValue>,
    backing_value: Option<ReflectionConstantValue>,
    is_enum_case: bool,
    parameter_members: Vec<ReflectionParameterMember>,
    type_metadata: Option<ReflectionParameterTypeMetadata>,
    property_default_value: Option<ReflectionParameterDefaultValue>,
    required_parameter_count: i64,
    is_final: bool,
    is_abstract: bool,
    is_interface: bool,
    is_trait: bool,
    is_enum: bool,
    is_readonly: bool,
    is_anonymous: bool,
    is_instantiable: bool,
    is_cloneable: bool,
    modifiers: i64,
    member_flags: ReflectionMemberFlags,
}

/// Compile-time metadata for one class/interface/trait/enum constant reflector.
struct ReflectionClassConstantMetadata {
    declaring_class_name: String,
    attr_names: Vec<String>,
    attr_args: Vec<Option<Vec<AttrArgValue>>>,
    value: ReflectionConstantValue,
    visibility: Visibility,
    is_final: bool,
}

/// Metadata for one member object returned by `ReflectionClass::getMethods()` or `getProperties()`.
#[derive(Clone)]
struct ReflectionListedMember {
    name: String,
    declaring_class_name: Option<String>,
    attr_names: Vec<String>,
    attr_args: Vec<Option<Vec<AttrArgValue>>>,
    constant_value: Option<ReflectionConstantValue>,
    is_enum_case: bool,
    flags: ReflectionMemberFlags,
    modifiers: i64,
    type_metadata: Option<ReflectionParameterTypeMetadata>,
    default_value: Option<ReflectionParameterDefaultValue>,
    required_parameter_count: i64,
    parameters: Vec<ReflectionParameterMember>,
}

/// Metadata for one object returned by `ReflectionMethod::getParameters()`.
#[derive(Clone)]
struct ReflectionParameterMember {
    name: String,
    declaring_class_name: Option<String>,
    declaring_function: Option<ReflectionDeclaringFunctionMember>,
    attr_names: Vec<String>,
    attr_args: Vec<Option<Vec<AttrArgValue>>>,
    position: i64,
    is_optional: bool,
    is_variadic: bool,
    is_passed_by_reference: bool,
    has_type: bool,
    type_metadata: Option<ReflectionParameterTypeMetadata>,
    default_value: Option<ReflectionParameterDefaultValue>,
}

/// Metadata needed for `ReflectionParameter::getDeclaringFunction()`.
#[derive(Clone)]
enum ReflectionDeclaringFunctionMember {
    Function {
        name: String,
        attr_names: Vec<String>,
        attr_args: Vec<Option<Vec<AttrArgValue>>>,
        required_parameter_count: i64,
    },
    Method {
        name: String,
        declaring_class_name: Option<String>,
        attr_names: Vec<String>,
        attr_args: Vec<Option<Vec<AttrArgValue>>>,
        flags: ReflectionMemberFlags,
        required_parameter_count: i64,
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
}

/// Returns true for reflection owner classes that need metadata-aware construction.
pub(super) fn is_reflection_owner_class(class_name: &str) -> bool {
    matches!(
        class_name,
        "ReflectionClass"
            | "ReflectionFunction"
            | "ReflectionMethod"
            | "ReflectionProperty"
            | "ReflectionParameter"
            | "ReflectionClassConstant"
            | "ReflectionEnumUnitCase"
            | "ReflectionEnumBackedCase"
    )
}

/// Lowers builtin Reflection owner allocation by populating compile-time metadata slots.
pub(super) fn lower_reflection_owner_new(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    class_name: &str,
) -> Result<()> {
    let metadata = reflection_owner_metadata(ctx, class_name, inst)?;
    emit_reflection_owner_object(ctx, class_name, &metadata)?;
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("reflection object_new missing result"))?;
    ctx.store_result_value(result)
}

/// Allocates and populates one builtin Reflection owner object from metadata.
fn emit_reflection_owner_object(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    metadata: &ReflectionOwnerMetadata,
) -> Result<()> {
    let (class_id, property_count, uninitialized_marker_offsets) = {
        let class_info =
            ctx.module.class_infos.get(class_name).ok_or_else(|| {
                CodegenIrError::unsupported(format!("unknown class {}", class_name))
            })?;
        (
            class_info.class_id,
            class_info.properties.len(),
            super::uninitialized_property_marker_offsets(class_info),
        )
    };
    super::emit_object_allocation(
        ctx,
        class_id,
        property_count,
        false,
        &uninitialized_marker_offsets,
    )?;
    if let Some(reflected_name) = metadata.reflected_name.as_deref() {
        emit_reflection_string_property(ctx, reflected_name, 8, 16);
        if class_name == "ReflectionClass" {
            emit_reflection_class_name_parts(ctx, reflected_name)?;
            emit_reflection_string_array_property_by_name(
                ctx,
                "__interface_names",
                &metadata.interface_names,
            )?;
            emit_reflection_string_array_property_by_name(
                ctx,
                "__trait_names",
                &metadata.trait_names,
            )?;
            emit_reflection_string_array_property_by_name(
                ctx,
                "__parent_names",
                &metadata.parent_names,
            )?;
            emit_reflection_string_array_property_by_name(
                ctx,
                "__method_names",
                &metadata.method_names,
            )?;
            emit_reflection_string_array_property_by_name(
                ctx,
                "__property_names",
                &metadata.property_names,
            )?;
            emit_reflection_string_array_property_by_name(
                ctx,
                "__constant_names",
                &metadata.constant_names,
            )?;
            emit_reflection_constant_array_property_by_name(
                ctx,
                "__constants",
                &metadata.constant_members,
            )?;
            emit_reflection_default_property_array_property_by_name(
                ctx,
                "__default_properties",
                &metadata.default_property_members,
            )?;
            emit_reflection_member_array_property_by_name(
                ctx,
                "__reflection_constants",
                "ReflectionClassConstant",
                &metadata.constant_reflection_members,
            )?;
            emit_reflection_member_array_property_by_name(
                ctx,
                "__methods",
                "ReflectionMethod",
                &metadata.method_members,
            )?;
            emit_reflection_constructor_property(ctx, metadata.constructor_member.as_ref())?;
            emit_reflection_parent_class_property(ctx, metadata.parent_class_name.as_deref())?;
            emit_reflection_member_array_property_by_name(
                ctx,
                "__properties",
                "ReflectionProperty",
                &metadata.property_members,
            )?;
        }
    }
    emit_reflection_attrs_property(ctx, class_name, &metadata.attr_names, &metadata.attr_args)?;
    if class_name == "ReflectionClass" {
        emit_reflection_bool_property(ctx, "__is_final", metadata.is_final)?;
        emit_reflection_bool_property(ctx, "__is_abstract", metadata.is_abstract)?;
        emit_reflection_bool_property(ctx, "__is_interface", metadata.is_interface)?;
        emit_reflection_bool_property(ctx, "__is_trait", metadata.is_trait)?;
        emit_reflection_bool_property(ctx, "__is_enum", metadata.is_enum)?;
        emit_reflection_bool_property(ctx, "__is_readonly", metadata.is_readonly)?;
        emit_reflection_bool_property(ctx, "__is_anonymous", metadata.is_anonymous)?;
        emit_reflection_bool_property(ctx, "__is_instantiable", metadata.is_instantiable)?;
        emit_reflection_bool_property(ctx, "__is_cloneable", metadata.is_cloneable)?;
        emit_reflection_int_property(ctx, "__modifiers", metadata.modifiers)?;
    }
    if matches!(
        class_name,
        "ReflectionMethod"
            | "ReflectionProperty"
            | "ReflectionClassConstant"
            | "ReflectionEnumUnitCase"
            | "ReflectionEnumBackedCase"
    ) {
        emit_reflection_declaring_class_property(
            ctx,
            class_name,
            metadata.parent_class_name.as_deref(),
        )?;
    }
    if matches!(class_name, "ReflectionFunction" | "ReflectionMethod") {
        emit_reflection_parameter_array_property_by_name(
            ctx,
            class_name,
            "__parameters",
            &metadata.parameter_members,
        )?;
        emit_reflection_owner_int_property(
            ctx,
            class_name,
            "__required_parameter_count",
            metadata.required_parameter_count,
        )?;
    }
    if matches!(
        class_name,
        "ReflectionClassConstant" | "ReflectionEnumUnitCase" | "ReflectionEnumBackedCase"
    ) {
        if let Some(value) = &metadata.constant_value {
            abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
            emit_reflection_constant_value_as_mixed(ctx, value);
            emit_reflection_owner_mixed_property_from_result(ctx, class_name, "__value")?;
        }
    }
    if class_name == "ReflectionEnumBackedCase" {
        if let Some(value) = &metadata.backing_value {
            abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
            emit_reflection_constant_value_as_mixed(ctx, value);
            emit_reflection_owner_mixed_property_from_result(ctx, class_name, "__backing_value")?;
        }
    }
    if class_name == "ReflectionClassConstant" {
        emit_reflection_owner_bool_property(
            ctx,
            class_name,
            "__is_enum_case",
            metadata.is_enum_case,
        )?;
        emit_reflection_owner_int_property(ctx, class_name, "__modifiers", metadata.modifiers)?;
    }
    if class_name == "ReflectionMethod" {
        emit_reflection_owner_int_property(ctx, class_name, "__modifiers", metadata.modifiers)?;
    }
    if class_name == "ReflectionProperty" {
        emit_reflection_owner_int_property(ctx, class_name, "__modifiers", metadata.modifiers)?;
        emit_reflection_owner_type_property(ctx, class_name, metadata.type_metadata.as_ref())?;
        emit_reflection_owner_bool_property(
            ctx,
            class_name,
            "__has_default_value",
            metadata.property_default_value.is_some(),
        )?;
        emit_reflection_owner_default_value_property(
            ctx,
            class_name,
            metadata.property_default_value.as_ref(),
        )?;
    }
    if class_name == "ReflectionParameter" {
        if let Some(parameter) = metadata.parameter_members.first() {
            emit_reflection_parameter_properties(ctx, parameter)?;
        }
    }
    emit_reflection_member_flag_properties(ctx, class_name, metadata.member_flags)?;
    Ok(())
}

/// Stores namespace-aware name parts for a statically materialized ReflectionClass.
fn emit_reflection_class_name_parts(
    ctx: &mut FunctionContext<'_>,
    reflected_name: &str,
) -> Result<()> {
    let (namespace_name, short_name) = reflection_name_parts(reflected_name);
    emit_reflection_string_property_by_name(ctx, "__short_name", short_name)?;
    emit_reflection_string_property_by_name(ctx, "__namespace_name", namespace_name)?;
    emit_reflection_bool_property(ctx, "__in_namespace", !namespace_name.is_empty())?;
    Ok(())
}

/// Splits a canonical PHP class-like name into namespace and short-name parts.
fn reflection_name_parts(reflected_name: &str) -> (&str, &str) {
    match reflected_name.rfind('\\') {
        Some(separator) => (
            &reflected_name[..separator],
            &reflected_name[separator + 1..],
        ),
        None => ("", reflected_name),
    }
}

/// Resolves Reflection constructor operands to captured class/member metadata.
fn reflection_owner_metadata(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    inst: &Instruction,
) -> Result<ReflectionOwnerMetadata> {
    match class_name {
        "ReflectionClass" => reflection_class_metadata(ctx, inst),
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

/// Resolves `ReflectionClass(class)` metadata.
fn reflection_class_metadata(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<ReflectionOwnerMetadata> {
    let Some(class_operand) = inst.operands.first().copied() else {
        return Ok(empty_reflection_metadata());
    };
    let reflected_class = const_string_or_class_operand(ctx, class_operand, "ReflectionClass")?;
    reflection_class_metadata_for_name(ctx, &reflected_class)
}

/// Resolves `ReflectionClass(name)` metadata for a known class-like name.
fn reflection_class_metadata_for_name(
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
        let constant_reflection_members =
            reflection_class_constant_reflection_members(ctx, class_name, info)?;
        let method_members = reflection_class_method_members(info, &method_names);
        let property_members =
            reflection_class_property_members(ctx, class_name, info, &property_names);
        let constructor_member = reflection_constructor_member(&method_members);
        let is_instantiable =
            reflection_class_is_instantiable(info, is_enum, constructor_member.as_ref());
        let is_cloneable = reflection_class_is_cloneable(class_name, info, is_enum);
        return Ok(ReflectionOwnerMetadata {
            reflected_name: Some(class_name.to_string()),
            attr_names: info.attribute_names.clone(),
            attr_args: info.attribute_args.clone(),
            interface_names: info.interfaces.clone(),
            trait_names: info.used_traits.clone(),
            parent_names: reflection_parent_class_names(ctx, info),
            method_names,
            property_names,
            constant_names,
            constant_members,
            default_property_members,
            constant_reflection_members,
            method_members,
            property_members,
            constructor_member,
            parent_class_name: reflection_parent_class_name(ctx, info),
            constant_value: None,
            backing_value: None,
            is_enum_case: false,
            parameter_members: Vec::new(),
            type_metadata: None,
            property_default_value: None,
            required_parameter_count: 0,
            is_final: info.is_final,
            is_abstract: info.is_abstract,
            is_interface: false,
            is_trait: false,
            is_enum,
            is_readonly: info.is_readonly_class && !is_enum,
            is_anonymous: is_reflection_anonymous_class_name(class_name),
            is_instantiable,
            is_cloneable,
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
            .map(|info| reflection_interface_method_members(info, interface_name, &method_names))
            .unwrap_or_else(|| default_method_members(&method_names, true, interface_name));
        let property_members = default_property_members(&property_names, true, interface_name);
        let constructor_member = reflection_constructor_member(&method_members);
        return Ok(ReflectionOwnerMetadata {
            reflected_name: Some(interface_name.to_string()),
            attr_names: Vec::new(),
            attr_args: Vec::new(),
            interface_names: reflection_interface_parent_names(ctx, interface_name),
            trait_names: Vec::new(),
            parent_names: Vec::new(),
            method_names,
            property_names,
            constant_names,
            constant_members,
            default_property_members: Vec::new(),
            constant_reflection_members,
            method_members,
            property_members,
            constructor_member,
            parent_class_name: None,
            constant_value: None,
            backing_value: None,
            is_enum_case: false,
            parameter_members: Vec::new(),
            type_metadata: None,
            property_default_value: None,
            required_parameter_count: 0,
            is_final: false,
            is_abstract: false,
            is_interface: true,
            is_trait: false,
            is_enum: false,
            is_readonly: false,
            is_anonymous: false,
            is_instantiable: false,
            is_cloneable: false,
            modifiers: 0,
            member_flags: reflection_member_flags(false, &Visibility::Public, false, false, false),
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
            .map(|methods| reflection_trait_method_members(methods, trait_name, &method_names))
            .unwrap_or_else(|| default_method_members(&method_names, false, trait_name));
        let property_members = default_property_members(&property_names, false, trait_name);
        let constructor_member = reflection_constructor_member(&method_members);
        return Ok(ReflectionOwnerMetadata {
            reflected_name: Some(trait_name.to_string()),
            attr_names: Vec::new(),
            attr_args: Vec::new(),
            interface_names: Vec::new(),
            trait_names,
            parent_names: Vec::new(),
            method_names,
            property_names,
            constant_names,
            constant_members,
            default_property_members: Vec::new(),
            constant_reflection_members,
            method_members,
            property_members,
            constructor_member,
            parent_class_name: None,
            constant_value: None,
            backing_value: None,
            is_enum_case: false,
            parameter_members: Vec::new(),
            type_metadata: None,
            property_default_value: None,
            required_parameter_count: 0,
            is_final: false,
            is_abstract: false,
            is_interface: false,
            is_trait: true,
            is_enum: false,
            is_readonly: false,
            is_anonymous: false,
            is_instantiable: false,
            is_cloneable: false,
            modifiers: 0,
            member_flags: reflection_member_flags(false, &Visibility::Public, false, false, false),
        });
    }
    Ok(empty_reflection_metadata())
}

/// Resolves class metadata for nested declaring-class slots without recursive member objects.
fn reflection_shallow_class_metadata_for_name(
    ctx: &FunctionContext<'_>,
    reflected_class: &str,
) -> Result<ReflectionOwnerMetadata> {
    let mut metadata = reflection_class_metadata_for_name(ctx, reflected_class)?;
    metadata.method_names.clear();
    metadata.property_names.clear();
    metadata.constant_names.clear();
    metadata.constant_members.clear();
    metadata.constant_reflection_members.clear();
    metadata.method_members.clear();
    metadata.property_members.clear();
    metadata.constructor_member = None;
    metadata.parent_class_name = None;
    Ok(metadata)
}

/// Resolves `ReflectionFunction(function)` metadata.
fn reflection_function_metadata(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<ReflectionOwnerMetadata> {
    let Some(function_operand) = inst.operands.first().copied() else {
        return Ok(empty_reflection_metadata());
    };
    let function_name = const_required_string_operand(ctx, function_operand, "ReflectionFunction")?;
    let Some(function) = ctx.function_by_name(&function_name) else {
        return Ok(empty_reflection_metadata());
    };
    let Some(signature) = function.signature.as_ref() else {
        return Ok(empty_reflection_metadata());
    };
    let reflected_name = function.name.trim_start_matches('\\').to_string();
    let required_parameter_count = reflection_required_parameter_count(signature);
    let declaring_function = ReflectionDeclaringFunctionMember::Function {
        name: reflected_name.clone(),
        attr_names: function.attribute_names.clone(),
        attr_args: function.attribute_args.clone(),
        required_parameter_count,
    };
    let mut metadata = empty_reflection_metadata();
    metadata.reflected_name = Some(reflected_name);
    metadata.attr_names = function.attribute_names.clone();
    metadata.attr_args = function.attribute_args.clone();
    metadata.parameter_members = reflection_parameter_members_with_declaring_function(
        signature,
        None,
        Some(declaring_function),
    );
    metadata.required_parameter_count = required_parameter_count;
    Ok(metadata)
}

/// Resolves `ReflectionMethod(class, method)` metadata.
fn reflection_method_metadata(
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
        if let Some(member) = reflection_class_method_member(info, &method_key) {
            return Ok(reflection_method_owner_metadata(&method_name, member));
        }
    }
    if let Some(interface_name) = resolve_reflection_interface(ctx, &reflected_class) {
        if let Some(info) = ctx.module.interface_infos.get(interface_name) {
            if let Some(member) =
                reflection_interface_method_member(info, interface_name, &method_key)
            {
                return Ok(reflection_method_owner_metadata(&method_name, member));
            }
        }
    }
    if let Some(trait_name) = resolve_reflection_trait(ctx, &reflected_class) {
        if let Some(methods) = ctx.module.declared_trait_methods.get(trait_name) {
            if let Some(member) = reflection_trait_method_member(methods, trait_name, &method_key) {
                return Ok(reflection_method_owner_metadata(&method_name, member));
            }
        }
    }
    Ok(empty_reflection_metadata())
}

/// Builds direct ReflectionMethod constructor metadata from one reflected method member.
fn reflection_method_owner_metadata(
    method_name: &str,
    member: ReflectionListedMember,
) -> ReflectionOwnerMetadata {
    ReflectionOwnerMetadata {
        reflected_name: Some(method_name.to_string()),
        attr_names: member.attr_names,
        attr_args: member.attr_args,
        interface_names: Vec::new(),
        trait_names: Vec::new(),
        parent_names: Vec::new(),
        method_names: Vec::new(),
        property_names: Vec::new(),
        constant_names: Vec::new(),
        constant_members: Vec::new(),
        default_property_members: Vec::new(),
        constant_reflection_members: Vec::new(),
        method_members: Vec::new(),
        property_members: Vec::new(),
        constructor_member: None,
        parent_class_name: member.declaring_class_name,
        constant_value: member.constant_value,
        backing_value: None,
        is_enum_case: member.is_enum_case,
        parameter_members: member.parameters,
        type_metadata: None,
        property_default_value: None,
        required_parameter_count: member.required_parameter_count,
        is_final: false,
        is_abstract: false,
        is_interface: false,
        is_trait: false,
        is_enum: false,
        is_readonly: false,
        is_anonymous: false,
        is_instantiable: false,
        is_cloneable: false,
        modifiers: reflection_method_modifiers_from_flags(member.flags),
        member_flags: member.flags,
    }
}

/// Resolves `ReflectionProperty(class, property)` metadata.
fn reflection_property_metadata(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<ReflectionOwnerMetadata> {
    let Some(class_operand) = inst.operands.first().copied() else {
        return Ok(empty_reflection_metadata());
    };
    let Some(property_operand) = inst.operands.get(1).copied() else {
        return Ok(empty_reflection_metadata());
    };
    let reflected_class = const_string_or_class_operand(ctx, class_operand, "ReflectionProperty")?;
    let property_name = const_required_string_operand(ctx, property_operand, "ReflectionProperty")?;
    Ok(resolve_reflection_class(ctx, &reflected_class)
        .and_then(|(_, info)| {
            let declaring_class_name =
                reflection_property_declaring_class_name(info, &property_name);
            Some(ReflectionOwnerMetadata {
                reflected_name: Some(property_name.clone()),
                attr_names: info.property_attribute_names.get(&property_name)?.clone(),
                attr_args: info.property_attribute_args.get(&property_name)?.clone(),
                interface_names: Vec::new(),
                trait_names: Vec::new(),
                parent_names: Vec::new(),
                method_names: Vec::new(),
                property_names: Vec::new(),
                constant_names: Vec::new(),
                constant_members: Vec::new(),
                default_property_members: Vec::new(),
                constant_reflection_members: Vec::new(),
                method_members: Vec::new(),
                property_members: Vec::new(),
                constructor_member: None,
                parent_class_name: declaring_class_name,
                constant_value: None,
                backing_value: None,
                is_enum_case: false,
                parameter_members: Vec::new(),
                type_metadata: reflection_property_type_metadata(info, &property_name),
                property_default_value: reflection_property_default_value(info, &property_name),
                required_parameter_count: 0,
                is_final: false,
                is_abstract: false,
                is_interface: false,
                is_trait: false,
                is_enum: false,
                is_readonly: false,
                is_anonymous: false,
                is_instantiable: false,
                is_cloneable: false,
                modifiers: reflection_property_modifiers_for_info(info, &property_name)?,
                member_flags: reflection_property_member_flags(info, &property_name)?,
            })
        })
        .unwrap_or_else(empty_reflection_metadata))
}

/// Resolves `ReflectionParameter(target, parameter)` metadata.
fn reflection_parameter_metadata(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<ReflectionOwnerMetadata> {
    if inst.operands.len() == 2 {
        return reflection_function_parameter_metadata(ctx, inst);
    }
    let Some(class_operand) = inst.operands.first().copied() else {
        return Ok(empty_reflection_metadata());
    };
    let Some(method_operand) = inst.operands.get(1).copied() else {
        return Ok(empty_reflection_metadata());
    };
    let Some(parameter_operand) = inst.operands.get(2).copied() else {
        return Ok(empty_reflection_metadata());
    };
    let reflected_class = const_string_or_class_operand(ctx, class_operand, "ReflectionParameter")?;
    let method_name = const_required_string_operand(ctx, method_operand, "ReflectionParameter")?;
    let selector = const_parameter_selector_operand(ctx, parameter_operand)?;
    let method_key = php_symbol_key(&method_name);
    let method = reflection_method_member_for_class_like(ctx, &reflected_class, &method_key);
    let Some(parameter) = method
        .as_ref()
        .and_then(|method| reflection_parameter_member_for_selector(&method.parameters, selector))
    else {
        return Ok(empty_reflection_metadata());
    };
    Ok(reflection_parameter_owner_metadata(parameter))
}

/// Resolves `ReflectionParameter(function, parameter)` metadata.
fn reflection_function_parameter_metadata(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<ReflectionOwnerMetadata> {
    let Some(function_operand) = inst.operands.first().copied() else {
        return Ok(empty_reflection_metadata());
    };
    let Some(parameter_operand) = inst.operands.get(1).copied() else {
        return Ok(empty_reflection_metadata());
    };
    let function_name =
        const_required_string_operand(ctx, function_operand, "ReflectionParameter")?;
    let selector = const_parameter_selector_operand(ctx, parameter_operand)?;
    let Some(function) = ctx.function_by_name(&function_name) else {
        return Ok(empty_reflection_metadata());
    };
    let Some(signature) = function.signature.as_ref() else {
        return Ok(empty_reflection_metadata());
    };
    let reflected_name = function.name.trim_start_matches('\\').to_string();
    let declaring_function = ReflectionDeclaringFunctionMember::Function {
        name: reflected_name,
        attr_names: function.attribute_names.clone(),
        attr_args: function.attribute_args.clone(),
        required_parameter_count: reflection_required_parameter_count(signature),
    };
    let parameters = reflection_parameter_members_with_declaring_function(
        signature,
        None,
        Some(declaring_function),
    );
    let Some(parameter) = reflection_parameter_member_for_selector(&parameters, selector) else {
        return Ok(empty_reflection_metadata());
    };
    Ok(reflection_parameter_owner_metadata(parameter))
}

/// Builds direct ReflectionParameter constructor metadata from one parameter member.
fn reflection_parameter_owner_metadata(
    parameter: ReflectionParameterMember,
) -> ReflectionOwnerMetadata {
    let mut metadata = empty_reflection_metadata();
    metadata.reflected_name = Some(parameter.name.clone());
    metadata.parameter_members.push(parameter);
    metadata
}

/// Resolves a reflected method member on a class, interface, or trait.
fn reflection_method_member_for_class_like(
    ctx: &FunctionContext<'_>,
    reflected_class: &str,
    method_key: &str,
) -> Option<ReflectionListedMember> {
    if let Some((_, info)) = resolve_reflection_class(ctx, reflected_class) {
        return reflection_class_method_member(info, method_key);
    }
    if let Some(interface_name) = resolve_reflection_interface(ctx, reflected_class) {
        return ctx
            .module
            .interface_infos
            .get(interface_name)
            .and_then(|info| reflection_interface_method_member(info, interface_name, method_key));
    }
    resolve_reflection_trait(ctx, reflected_class).and_then(|trait_name| {
        ctx.module
            .declared_trait_methods
            .get(trait_name)
            .and_then(|methods| reflection_trait_method_member(methods, trait_name, method_key))
    })
}

/// Returns the selected parameter member by PHP name or zero-based position.
fn reflection_parameter_member_for_selector(
    parameters: &[ReflectionParameterMember],
    selector: ReflectionParameterSelector,
) -> Option<ReflectionParameterMember> {
    match selector {
        ReflectionParameterSelector::Name(name) => parameters
            .iter()
            .find(|parameter| parameter.name == name)
            .cloned(),
        ReflectionParameterSelector::Position(position) if position >= 0 => {
            parameters.get(position as usize).cloned()
        }
        ReflectionParameterSelector::Position(_) => None,
    }
}

/// Resolves `ReflectionClassConstant(class, constant)` metadata.
fn reflection_class_constant_metadata(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<ReflectionOwnerMetadata> {
    let Some(class_operand) = inst.operands.first().copied() else {
        return Ok(empty_reflection_metadata());
    };
    let Some(constant_operand) = inst.operands.get(1).copied() else {
        return Ok(empty_reflection_metadata());
    };
    let reflected_class =
        const_string_or_class_operand(ctx, class_operand, "ReflectionClassConstant")?;
    let constant_name =
        const_required_string_operand(ctx, constant_operand, "ReflectionClassConstant")?;
    if let Some((enum_name, case)) =
        resolve_reflection_enum_case(ctx, &reflected_class, &constant_name)
    {
        return Ok(ReflectionOwnerMetadata {
            reflected_name: Some(constant_name.clone()),
            attr_names: case.attribute_names.clone(),
            attr_args: case.attribute_args.clone(),
            interface_names: Vec::new(),
            trait_names: Vec::new(),
            parent_names: Vec::new(),
            method_names: Vec::new(),
            property_names: Vec::new(),
            constant_names: Vec::new(),
            constant_members: Vec::new(),
            default_property_members: Vec::new(),
            constant_reflection_members: Vec::new(),
            method_members: Vec::new(),
            property_members: Vec::new(),
            constructor_member: None,
            parent_class_name: Some(enum_name.to_string()),
            constant_value: Some(ReflectionConstantValue::EnumCase {
                enum_name: enum_name.to_string(),
                case_name: constant_name.clone(),
            }),
            backing_value: None,
            is_enum_case: true,
            parameter_members: Vec::new(),
            type_metadata: None,
            property_default_value: None,
            required_parameter_count: 0,
            is_final: false,
            is_abstract: false,
            is_interface: false,
            is_trait: false,
            is_enum: false,
            is_readonly: false,
            is_anonymous: false,
            is_instantiable: false,
            is_cloneable: false,
            modifiers: reflection_class_constant_modifiers(&Visibility::Public, false),
            member_flags: reflection_member_flags(false, &Visibility::Public, false, false, false),
        });
    }
    Ok(
        reflection_class_constant_lookup(ctx, &reflected_class, &constant_name)?
            .map(|metadata| reflection_class_constant_owner_metadata(constant_name, metadata))
            .unwrap_or_else(empty_reflection_metadata),
    )
}

/// Resolves `ReflectionEnumUnitCase/BackedCase(enum, case)` metadata.
fn reflection_enum_case_metadata(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    inst: &Instruction,
) -> Result<ReflectionOwnerMetadata> {
    let Some(enum_operand) = inst.operands.first().copied() else {
        return Ok(empty_reflection_metadata());
    };
    let Some(case_operand) = inst.operands.get(1).copied() else {
        return Ok(empty_reflection_metadata());
    };
    let reflected_enum = const_string_or_class_operand(ctx, enum_operand, class_name)?;
    let case_name = const_required_string_operand(ctx, case_operand, class_name)?;
    Ok(
        resolve_reflection_enum_case(ctx, &reflected_enum, &case_name)
            .map(|(enum_name, case)| ReflectionOwnerMetadata {
                reflected_name: Some(case_name.clone()),
                attr_names: case.attribute_names.clone(),
                attr_args: case.attribute_args.clone(),
                interface_names: Vec::new(),
                trait_names: Vec::new(),
                parent_names: Vec::new(),
                method_names: Vec::new(),
                property_names: Vec::new(),
                constant_names: Vec::new(),
                constant_members: Vec::new(),
                default_property_members: Vec::new(),
                constant_reflection_members: Vec::new(),
                method_members: Vec::new(),
                property_members: Vec::new(),
                constructor_member: None,
                parent_class_name: Some(enum_name.to_string()),
                constant_value: Some(ReflectionConstantValue::EnumCase {
                    enum_name: enum_name.to_string(),
                    case_name: case_name.clone(),
                }),
                backing_value: reflection_enum_case_backing_value(case),
                is_enum_case: true,
                parameter_members: Vec::new(),
                type_metadata: None,
                property_default_value: None,
                required_parameter_count: 0,
                is_final: false,
                is_abstract: false,
                is_interface: false,
                is_trait: false,
                is_enum: false,
                is_readonly: false,
                is_anonymous: false,
                is_instantiable: false,
                is_cloneable: false,
                modifiers: 0,
                member_flags: ReflectionMemberFlags::default(),
            })
            .unwrap_or_else(empty_reflection_metadata),
    )
}

/// Builds owner metadata for one resolved class/interface/trait/enum constant reflector.
fn reflection_class_constant_owner_metadata(
    reflected_name: String,
    metadata: ReflectionClassConstantMetadata,
) -> ReflectionOwnerMetadata {
    let is_final = metadata.is_final;
    let modifiers = reflection_class_constant_modifiers(&metadata.visibility, is_final);
    let member_flags = reflection_member_flags(false, &metadata.visibility, is_final, false, false);
    ReflectionOwnerMetadata {
        reflected_name: Some(reflected_name),
        attr_names: metadata.attr_names,
        attr_args: metadata.attr_args,
        interface_names: Vec::new(),
        trait_names: Vec::new(),
        parent_names: Vec::new(),
        method_names: Vec::new(),
        property_names: Vec::new(),
        constant_names: Vec::new(),
        constant_members: Vec::new(),
        default_property_members: Vec::new(),
        constant_reflection_members: Vec::new(),
        method_members: Vec::new(),
        property_members: Vec::new(),
        constructor_member: None,
        parent_class_name: Some(metadata.declaring_class_name),
        constant_value: Some(metadata.value),
        backing_value: None,
        is_enum_case: false,
        parameter_members: Vec::new(),
        type_metadata: None,
        property_default_value: None,
        required_parameter_count: 0,
        is_final,
        is_abstract: false,
        is_interface: false,
        is_trait: false,
        is_enum: false,
        is_readonly: false,
        is_anonymous: false,
        is_instantiable: false,
        is_cloneable: false,
        modifiers,
        member_flags,
    }
}

/// Resolves static metadata for a direct `ReflectionClassConstant` constructor call.
fn reflection_class_constant_lookup(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    constant_name: &str,
) -> Result<Option<ReflectionClassConstantMetadata>> {
    if let Some((declaring_class_name, info)) =
        resolve_reflection_class_constant(ctx, class_name, constant_name)
    {
        let Some(value_expr) = info.constants.get(constant_name) else {
            return Ok(None);
        };
        let value =
            reflection_constant_value(ctx, declaring_class_name, Some(info), value_expr, 0)?;
        return Ok(Some(ReflectionClassConstantMetadata {
            declaring_class_name: declaring_class_name.to_string(),
            attr_names: info
                .constant_attribute_names
                .get(constant_name)
                .cloned()
                .unwrap_or_default(),
            attr_args: info
                .constant_attribute_args
                .get(constant_name)
                .cloned()
                .unwrap_or_default(),
            value,
            visibility: info
                .constant_visibilities
                .get(constant_name)
                .cloned()
                .unwrap_or(Visibility::Public),
            is_final: info.final_constants.contains(constant_name),
        }));
    }
    if let Some((_, class_info)) = resolve_reflection_class(ctx, class_name) {
        for interface_name in &class_info.interfaces {
            if let Some(metadata) =
                reflection_interface_class_constant_lookup(ctx, interface_name, constant_name)?
            {
                return Ok(Some(metadata));
            }
        }
    }
    if let Some(interface_name) = resolve_reflection_interface(ctx, class_name) {
        if let Some(metadata) =
            reflection_interface_class_constant_lookup(ctx, interface_name, constant_name)?
        {
            return Ok(Some(metadata));
        }
    }
    if let Some(trait_name) = resolve_reflection_trait(ctx, class_name) {
        if let Some(value_expr) = ctx
            .module
            .declared_trait_constants
            .get(trait_name)
            .and_then(|constants| constants.get(constant_name))
        {
            let is_final = ctx
                .module
                .declared_trait_final_constants
                .get(trait_name)
                .is_some_and(|constants| constants.contains(constant_name));
            let value = reflection_constant_value(ctx, trait_name, None, value_expr, 0)?;
            return Ok(Some(ReflectionClassConstantMetadata {
                declaring_class_name: trait_name.to_string(),
                attr_names: Vec::new(),
                attr_args: Vec::new(),
                value,
                visibility: ctx
                    .module
                    .declared_trait_constant_visibilities
                    .get(trait_name)
                    .and_then(|constants| constants.get(constant_name))
                    .cloned()
                    .unwrap_or(Visibility::Public),
                is_final,
            }));
        }
    }
    Ok(None)
}

/// Resolves interface constant metadata with the original declaring interface preserved.
fn reflection_interface_class_constant_lookup(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
    constant_name: &str,
) -> Result<Option<ReflectionClassConstantMetadata>> {
    let Some(interface_name) = resolve_reflection_interface(ctx, interface_name) else {
        return Ok(None);
    };
    let Some(info) = ctx.module.interface_infos.get(interface_name) else {
        return Ok(None);
    };
    let Some(value_expr) = info.constants.get(constant_name) else {
        return Ok(None);
    };
    let declaring_interface =
        interface_constant_declaring_interface(info, interface_name, constant_name);
    let is_final = ctx
        .module
        .interface_infos
        .get(declaring_interface)
        .is_some_and(|info| info.final_constants.contains(constant_name));
    let value = reflection_constant_value(ctx, declaring_interface, None, value_expr, 0)?;
    Ok(Some(ReflectionClassConstantMetadata {
        declaring_class_name: declaring_interface.to_string(),
        attr_names: Vec::new(),
        attr_args: Vec::new(),
        value,
        visibility: Visibility::Public,
        is_final,
    }))
}

/// Returns the interface that originally declared a visible interface constant.
fn interface_constant_declaring_interface<'a>(
    info: &'a InterfaceInfo,
    fallback_interface: &'a str,
    constant_name: &str,
) -> &'a str {
    info.constant_declaring_interfaces
        .get(constant_name)
        .map(String::as_str)
        .unwrap_or(fallback_interface)
}

/// Looks up class metadata by PHP-style case-insensitive name.
fn resolve_reflection_class<'a>(
    ctx: &'a FunctionContext<'_>,
    class_name: &str,
) -> Option<(&'a str, &'a crate::types::ClassInfo)> {
    let class_key = php_symbol_key(class_name.trim_start_matches('\\'));
    ctx.module
        .class_infos
        .iter()
        .find(|(candidate, _)| php_symbol_key(candidate.trim_start_matches('\\')) == class_key)
        .map(|(name, info)| (name.as_str(), info))
}

/// Returns true when a class name uses the parser's anonymous-class synthetic prefix.
fn is_reflection_anonymous_class_name(class_name: &str) -> bool {
    class_name
        .trim_start_matches('\\')
        .starts_with("class@anonymous#")
}

/// Looks up interface metadata by PHP-style case-insensitive name.
fn resolve_reflection_interface<'a>(
    ctx: &'a FunctionContext<'_>,
    interface_name: &str,
) -> Option<&'a str> {
    let interface_key = php_symbol_key(interface_name.trim_start_matches('\\'));
    ctx.module
        .interface_infos
        .keys()
        .find(|candidate| php_symbol_key(candidate.trim_start_matches('\\')) == interface_key)
        .map(String::as_str)
}

/// Looks up a declared trait by PHP-style case-insensitive name.
fn resolve_reflection_trait<'a>(ctx: &'a FunctionContext<'_>, trait_name: &str) -> Option<&'a str> {
    let trait_key = php_symbol_key(trait_name.trim_start_matches('\\'));
    ctx.module
        .trait_table
        .names
        .iter()
        .find(|candidate| php_symbol_key(candidate.trim_start_matches('\\')) == trait_key)
        .map(String::as_str)
}

/// Looks up enum metadata by PHP-style case-insensitive name.
fn is_reflection_enum(ctx: &FunctionContext<'_>, enum_name: &str) -> bool {
    let enum_key = php_symbol_key(enum_name.trim_start_matches('\\'));
    ctx.module
        .enum_infos
        .keys()
        .any(|candidate| php_symbol_key(candidate.trim_start_matches('\\')) == enum_key)
}

/// Returns the canonical parent class name for a reflected class, if any.
fn reflection_parent_class_name(
    ctx: &FunctionContext<'_>,
    info: &crate::types::ClassInfo,
) -> Option<String> {
    let parent = info.parent.as_ref()?;
    resolve_reflection_class(ctx, parent)
        .map(|(parent_name, _)| parent_name.to_string())
        .or_else(|| Some(parent.trim_start_matches('\\').to_string()))
}

/// Returns direct and inherited parent class names for `ReflectionClass::isSubclassOf()`.
fn reflection_parent_class_names(
    ctx: &FunctionContext<'_>,
    info: &crate::types::ClassInfo,
) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut current = reflection_parent_class_name(ctx, info);
    while let Some(parent_name) = current {
        if names
            .iter()
            .any(|name| php_symbol_key(name) == php_symbol_key(&parent_name))
        {
            break;
        }
        current = resolve_reflection_class(ctx, &parent_name)
            .and_then(|(_, parent_info)| reflection_parent_class_name(ctx, parent_info));
        names.push(parent_name);
    }
    names
}

/// Returns PHP's `ReflectionClass::isInstantiable()` value for static class metadata.
fn reflection_class_is_instantiable(
    info: &crate::types::ClassInfo,
    is_enum: bool,
    constructor_member: Option<&ReflectionListedMember>,
) -> bool {
    if info.is_abstract || is_enum {
        return false;
    }
    constructor_member
        .map(|member| member.flags.is_public)
        .unwrap_or(true)
}

/// Returns PHP/elephc cloneability for a reflected class.
fn reflection_class_is_cloneable(
    class_name: &str,
    info: &crate::types::ClassInfo,
    is_enum: bool,
) -> bool {
    if info.is_abstract || is_enum || reflection_class_has_runtime_managed_storage(class_name) {
        return false;
    }
    let clone_key = php_symbol_key("__clone");
    info.method_visibilities
        .get(&clone_key)
        .is_none_or(|visibility| matches!(visibility, Visibility::Public))
}

/// Returns whether a builtin's object layout is outside ordinary declared slots.
fn reflection_class_has_runtime_managed_storage(class_name: &str) -> bool {
    let key = php_symbol_key(class_name);
    matches!(
        key.as_str(),
        "throwable"
            | "error"
            | "exception"
            | "valueerror"
            | "runtimeexception"
            | "reflectionexception"
            | "jsonexception"
            | "fiber"
            | "fibererror"
            | "generator"
            | "reflectionattribute"
            | "reflectionclass"
            | "reflectionfunction"
            | "reflectionmethod"
            | "reflectionproperty"
            | "reflectionparameter"
            | "reflectionnamedtype"
            | "reflectionuniontype"
            | "reflectionintersectiontype"
            | "reflectionclassconstant"
            | "reflectionenumunitcase"
            | "reflectionenumbackedcase"
            | "splfixedarray"
            | "spldoublylinkedlist"
            | "splstack"
            | "splqueue"
            | "iteratoriterator"
            | "filteriterator"
            | "callbackfilteriterator"
            | "recursivefilteriterator"
            | "recursivecallbackfilteriterator"
            | "recursiveiteratoriterator"
    )
}

/// Collects direct and inherited parent interfaces for a reflected interface.
fn reflection_interface_parent_names(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
) -> Vec<String> {
    let mut names = Vec::new();
    collect_reflection_interface_parent_names(ctx, interface_name, &mut names);
    names
}

/// Recursively collects interface parents without duplicating case-insensitive names.
fn collect_reflection_interface_parent_names(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
    names: &mut Vec<String>,
) {
    let Some(interface) = ctx.module.interface_infos.get(interface_name) else {
        return;
    };
    for parent in &interface.parents {
        let parent_name = resolve_reflection_interface(ctx, parent)
            .map(str::to_string)
            .unwrap_or_else(|| parent.clone());
        if !names
            .iter()
            .any(|name| php_symbol_key(name) == php_symbol_key(&parent_name))
        {
            names.push(parent_name.clone());
            collect_reflection_interface_parent_names(ctx, &parent_name, names);
        }
    }
}

/// Returns PHP case-insensitive method names visible to `ReflectionClass::hasMethod()`.
fn reflection_class_method_names(ctx: &FunctionContext<'_>, class_name: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut current = Some(class_name.to_string());
    while let Some(current_name) = current {
        let Some((resolved_name, info)) = resolve_reflection_class(ctx, &current_name) else {
            break;
        };
        push_unique_method_names(info.methods.keys(), &mut names, &mut seen);
        push_unique_method_names(info.static_methods.keys(), &mut names, &mut seen);
        current = info.parent.clone();
        if current.as_deref() == Some(resolved_name) {
            break;
        }
    }
    names
}

/// Returns PHP case-sensitive property names visible to `ReflectionClass::hasProperty()`.
fn reflection_class_property_names(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    info: &crate::types::ClassInfo,
) -> Vec<String> {
    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if is_reflection_enum(ctx, class_name) {
        push_unique_property_name("name", &mut names, &mut seen);
    }
    for (name, _) in &info.properties {
        if reflection_property_visible_from_class(info, class_name, name, false) {
            push_unique_property_name(name, &mut names, &mut seen);
        }
    }
    for (name, _) in &info.static_properties {
        if reflection_property_visible_from_class(info, class_name, name, true) {
            push_unique_property_name(name, &mut names, &mut seen);
        }
    }
    names
}

/// Returns PHP case-sensitive class constant names visible to `ReflectionClass::hasConstant()`.
fn reflection_class_constant_names(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    _info: &crate::types::ClassInfo,
) -> Vec<String> {
    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(enum_info) = ctx.module.enum_infos.get(class_name) {
        for case in &enum_info.cases {
            push_unique_constant_name(&case.name, &mut names, &mut seen);
        }
    }
    let mut current = Some(class_name.to_string());
    while let Some(current_name) = current {
        let Some((resolved_name, current_info)) = resolve_reflection_class(ctx, &current_name)
        else {
            break;
        };
        for constant in current_info.constants.keys() {
            push_unique_constant_name(constant, &mut names, &mut seen);
        }
        for interface_name in &current_info.interfaces {
            for constant in reflection_interface_constant_names(ctx, interface_name) {
                push_unique_constant_name(&constant, &mut names, &mut seen);
            }
        }
        current = current_info.parent.clone();
        if current.as_deref() == Some(resolved_name) {
            break;
        }
    }
    names
}

/// Returns materializable class constant values for `ReflectionClass::getConstants()`.
fn reflection_class_constant_members(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    _info: &crate::types::ClassInfo,
) -> Result<Vec<ReflectionConstantMember>> {
    let mut members = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(enum_info) = ctx.module.enum_infos.get(class_name) {
        for case in &enum_info.cases {
            push_unique_constant_member(
                &case.name,
                ReflectionConstantValue::EnumCase {
                    enum_name: class_name.to_string(),
                    case_name: case.name.clone(),
                },
                &mut members,
                &mut seen,
            );
        }
    }
    let mut current = Some(class_name.to_string());
    while let Some(current_name) = current {
        let Some((resolved_name, current_info)) = resolve_reflection_class(ctx, &current_name)
        else {
            break;
        };
        for (constant_name, value_expr) in &current_info.constants {
            if seen.contains(constant_name) {
                continue;
            }
            let value =
                reflection_constant_value(ctx, resolved_name, Some(current_info), value_expr, 0)?;
            push_unique_constant_member(constant_name, value, &mut members, &mut seen);
        }
        for interface_name in &current_info.interfaces {
            for member in reflection_interface_constant_members(ctx, interface_name)? {
                push_unique_constant_member(&member.name, member.value, &mut members, &mut seen);
            }
        }
        current = current_info.parent.clone();
        if current.as_deref() == Some(resolved_name) {
            break;
        }
    }
    Ok(members)
}

/// Returns materializable property defaults for `ReflectionClass::getDefaultProperties()`.
fn reflection_class_default_property_members(
    info: &crate::types::ClassInfo,
    property_names: &[String],
) -> Vec<ReflectionDefaultPropertyMember> {
    property_names
        .iter()
        .filter_map(|property_name| {
            reflection_property_default_value(info, property_name).map(|value| {
                ReflectionDefaultPropertyMember {
                    name: property_name.clone(),
                    value,
                }
            })
        })
        .collect()
}

/// Returns materializable interface constant values for ReflectionClass metadata.
fn reflection_interface_constant_members(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
) -> Result<Vec<ReflectionConstantMember>> {
    let mut members = Vec::new();
    let mut seen = std::collections::HashSet::new();
    collect_interface_constant_members(ctx, interface_name, &mut members, &mut seen)?;
    Ok(members)
}

/// Appends flattened interface constants while preserving their declaring interface.
fn collect_interface_constant_members(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
    members: &mut Vec<ReflectionConstantMember>,
    seen: &mut std::collections::HashSet<String>,
) -> Result<()> {
    let Some(interface_info) = ctx.module.interface_infos.get(interface_name) else {
        return Ok(());
    };
    for (constant_name, value_expr) in &interface_info.constants {
        if seen.contains(constant_name) {
            continue;
        }
        let declaring_interface =
            interface_constant_declaring_interface(interface_info, interface_name, constant_name);
        let value = reflection_constant_value(ctx, declaring_interface, None, value_expr, 0)?;
        push_unique_constant_member(constant_name, value, members, seen);
    }
    Ok(())
}

/// Returns materializable direct trait constant values for ReflectionClass metadata.
fn reflection_trait_constant_members(
    ctx: &FunctionContext<'_>,
    trait_name: &str,
) -> Result<Vec<ReflectionConstantMember>> {
    let mut members = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(constants) = ctx.module.declared_trait_constants.get(trait_name) {
        for (constant_name, value_expr) in constants {
            if seen.contains(constant_name) {
                continue;
            }
            let value = reflection_constant_value(ctx, trait_name, None, value_expr, 0)?;
            push_unique_constant_member(constant_name, value, &mut members, &mut seen);
        }
    }
    Ok(members)
}

/// Returns materializable constant-reflector objects for `ReflectionClass::getReflectionConstants()`.
fn reflection_class_constant_reflection_members(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    _info: &crate::types::ClassInfo,
) -> Result<Vec<ReflectionListedMember>> {
    let mut members = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(enum_info) = ctx.module.enum_infos.get(class_name) {
        for case in &enum_info.cases {
            push_unique_constant_reflection_member(
                &case.name,
                class_name,
                case.attribute_names.clone(),
                case.attribute_args.clone(),
                ReflectionConstantValue::EnumCase {
                    enum_name: class_name.to_string(),
                    case_name: case.name.clone(),
                },
                Visibility::Public,
                false,
                true,
                &mut members,
                &mut seen,
            );
        }
    }
    let mut current = Some(class_name.to_string());
    while let Some(current_name) = current {
        let Some((resolved_name, current_info)) = resolve_reflection_class(ctx, &current_name)
        else {
            break;
        };
        for (constant_name, value_expr) in &current_info.constants {
            if seen.contains(constant_name) {
                continue;
            }
            let value =
                reflection_constant_value(ctx, resolved_name, Some(current_info), value_expr, 0)?;
            push_unique_constant_reflection_member(
                constant_name,
                resolved_name,
                current_info
                    .constant_attribute_names
                    .get(constant_name)
                    .cloned()
                    .unwrap_or_default(),
                current_info
                    .constant_attribute_args
                    .get(constant_name)
                    .cloned()
                    .unwrap_or_default(),
                value,
                current_info
                    .constant_visibilities
                    .get(constant_name)
                    .cloned()
                    .unwrap_or(Visibility::Public),
                current_info.final_constants.contains(constant_name),
                false,
                &mut members,
                &mut seen,
            );
        }
        for interface_name in &current_info.interfaces {
            for member in reflection_interface_constant_reflection_members(ctx, interface_name)? {
                push_unique_listed_constant_member(member, &mut members, &mut seen);
            }
        }
        current = current_info.parent.clone();
        if current.as_deref() == Some(resolved_name) {
            break;
        }
    }
    Ok(members)
}

/// Returns constant-reflector objects for interface constants.
fn reflection_interface_constant_reflection_members(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
) -> Result<Vec<ReflectionListedMember>> {
    let mut members = Vec::new();
    let mut seen = std::collections::HashSet::new();
    collect_interface_constant_reflection_members(ctx, interface_name, &mut members, &mut seen)?;
    Ok(members)
}

/// Appends flattened interface constant-reflector objects with declaring-interface metadata.
fn collect_interface_constant_reflection_members(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
    members: &mut Vec<ReflectionListedMember>,
    seen: &mut std::collections::HashSet<String>,
) -> Result<()> {
    let Some(interface_info) = ctx.module.interface_infos.get(interface_name) else {
        return Ok(());
    };
    for (constant_name, value_expr) in &interface_info.constants {
        let declaring_interface =
            interface_constant_declaring_interface(interface_info, interface_name, constant_name);
        let is_final = ctx
            .module
            .interface_infos
            .get(declaring_interface)
            .is_some_and(|info| info.final_constants.contains(constant_name));
        let value = reflection_constant_value(ctx, declaring_interface, None, value_expr, 0)?;
        push_unique_constant_reflection_member(
            constant_name,
            declaring_interface,
            Vec::new(),
            Vec::new(),
            value,
            Visibility::Public,
            is_final,
            false,
            members,
            seen,
        );
    }
    Ok(())
}

/// Returns constant-reflector objects for direct trait constants.
fn reflection_trait_constant_reflection_members(
    ctx: &FunctionContext<'_>,
    trait_name: &str,
) -> Result<Vec<ReflectionListedMember>> {
    let mut members = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let Some(constants) = ctx.module.declared_trait_constants.get(trait_name) else {
        return Ok(members);
    };
    let final_constants = ctx.module.declared_trait_final_constants.get(trait_name);
    for (constant_name, value_expr) in constants {
        let value = reflection_constant_value(ctx, trait_name, None, value_expr, 0)?;
        push_unique_constant_reflection_member(
            constant_name,
            trait_name,
            Vec::new(),
            Vec::new(),
            value,
            ctx.module
                .declared_trait_constant_visibilities
                .get(trait_name)
                .and_then(|constants| constants.get(constant_name))
                .cloned()
                .unwrap_or(Visibility::Public),
            final_constants.is_some_and(|constants| constants.contains(constant_name)),
            false,
            &mut members,
            &mut seen,
        );
    }
    Ok(members)
}

/// Appends one constant-reflector member if a constant with this name was not already visible.
fn push_unique_constant_reflection_member(
    name: &str,
    declaring_class_name: &str,
    attr_names: Vec<String>,
    attr_args: Vec<Option<Vec<AttrArgValue>>>,
    value: ReflectionConstantValue,
    visibility: Visibility,
    is_final: bool,
    is_enum_case: bool,
    members: &mut Vec<ReflectionListedMember>,
    seen: &mut std::collections::HashSet<String>,
) {
    if !seen.insert(name.to_string()) {
        return;
    }
    members.push(ReflectionListedMember {
        name: name.to_string(),
        declaring_class_name: Some(declaring_class_name.to_string()),
        attr_names,
        attr_args,
        constant_value: Some(value),
        is_enum_case,
        flags: reflection_member_flags(false, &visibility, is_final, false, false),
        modifiers: reflection_class_constant_modifiers(&visibility, is_final),
        type_metadata: None,
        default_value: None,
        required_parameter_count: 0,
        parameters: Vec::new(),
    });
}

/// Appends a prebuilt constant-reflector member if its name was not already visible.
fn push_unique_listed_constant_member(
    member: ReflectionListedMember,
    members: &mut Vec<ReflectionListedMember>,
    seen: &mut std::collections::HashSet<String>,
) {
    if seen.insert(member.name.clone()) {
        members.push(member);
    }
}

/// Returns true when a property should be visible for `ReflectionClass::hasProperty()`.
fn reflection_property_visible_from_class(
    info: &crate::types::ClassInfo,
    reflected_class: &str,
    property_name: &str,
    is_static: bool,
) -> bool {
    let visibility = if is_static {
        info.static_property_visibilities.get(property_name)
    } else {
        info.property_visibilities.get(property_name)
    };
    if visibility != Some(&Visibility::Private) {
        return true;
    }
    let declaring_class = if is_static {
        info.static_property_declaring_classes.get(property_name)
    } else {
        info.property_declaring_classes.get(property_name)
    };
    declaring_class
        .map(|declaring_class| php_symbol_key(declaring_class) == php_symbol_key(reflected_class))
        .unwrap_or(false)
}

/// Returns the class that declares one reflected instance or static method.
fn reflection_method_declaring_class_name(
    info: &crate::types::ClassInfo,
    method_key: &str,
) -> Option<String> {
    info.method_impl_classes
        .get(method_key)
        .or_else(|| info.static_method_impl_classes.get(method_key))
        .cloned()
}

/// Returns the class that declares one reflected instance or static property.
fn reflection_property_declaring_class_name(
    info: &crate::types::ClassInfo,
    property_name: &str,
) -> Option<String> {
    info.property_declaring_classes
        .get(property_name)
        .or_else(|| info.static_property_declaring_classes.get(property_name))
        .cloned()
}

/// Returns ReflectionMethod predicate flags for a method visible on one class.
fn reflection_method_member_flags(
    info: &crate::types::ClassInfo,
    method_key: &str,
) -> Option<ReflectionMemberFlags> {
    if info.methods.contains_key(method_key) {
        let visibility = info
            .method_visibilities
            .get(method_key)
            .unwrap_or(&Visibility::Public);
        return Some(reflection_member_flags(
            false,
            visibility,
            info.final_methods.contains(method_key),
            !info.method_impl_classes.contains_key(method_key),
            false,
        ));
    }
    if info.static_methods.contains_key(method_key) {
        let visibility = info
            .static_method_visibilities
            .get(method_key)
            .unwrap_or(&Visibility::Public);
        return Some(reflection_member_flags(
            true,
            visibility,
            info.final_static_methods.contains(method_key),
            !info.static_method_impl_classes.contains_key(method_key),
            false,
        ));
    }
    None
}

/// Returns ReflectionProperty predicate flags for a property visible on one class.
fn reflection_property_member_flags(
    info: &crate::types::ClassInfo,
    property_name: &str,
) -> Option<ReflectionMemberFlags> {
    if info
        .properties
        .iter()
        .any(|(name, _)| name == property_name)
    {
        let visibility = info
            .property_visibilities
            .get(property_name)
            .unwrap_or(&Visibility::Public);
        return Some(reflection_member_flags(
            false,
            visibility,
            info.final_properties.contains(property_name),
            info.abstract_properties.contains(property_name),
            info.readonly_properties.contains(property_name),
        ));
    }
    if info
        .static_properties
        .iter()
        .any(|(name, _)| name == property_name)
    {
        let visibility = info
            .static_property_visibilities
            .get(property_name)
            .unwrap_or(&Visibility::Public);
        return Some(reflection_member_flags(
            true,
            visibility,
            info.final_static_properties.contains(property_name),
            false,
            false,
        ));
    }
    None
}

/// Builds ReflectionMethod array entries for the methods visible on one class.
fn reflection_class_method_members(
    info: &crate::types::ClassInfo,
    method_names: &[String],
) -> Vec<ReflectionListedMember> {
    method_names
        .iter()
        .filter_map(|method_name| reflection_class_method_member(info, method_name))
        .collect()
}

/// Builds one ReflectionMethod array entry from class metadata.
fn reflection_class_method_member(
    info: &crate::types::ClassInfo,
    method_name: &str,
) -> Option<ReflectionListedMember> {
    let method_key = php_symbol_key(method_name);
    let sig = info
        .methods
        .get(&method_key)
        .or_else(|| info.static_methods.get(&method_key))?;
    let declaring_class_name = reflection_method_declaring_class_name(info, &method_key);
    let attr_names = info
        .method_attribute_names
        .get(&method_key)
        .cloned()
        .unwrap_or_default();
    let attr_args = info
        .method_attribute_args
        .get(&method_key)
        .cloned()
        .unwrap_or_default();
    let flags = reflection_method_member_flags(info, &method_key)?;
    let required_parameter_count = reflection_required_parameter_count(sig);
    let declaring_function = ReflectionDeclaringFunctionMember::Method {
        name: method_key.clone(),
        declaring_class_name: declaring_class_name.clone(),
        attr_names: attr_names.clone(),
        attr_args: attr_args.clone(),
        flags,
        required_parameter_count,
    };
    let parameters = reflection_parameter_members_with_declaring_class(
        sig,
        declaring_class_name.as_deref(),
        Some(declaring_function),
    );
    Some(ReflectionListedMember {
        name: method_key.clone(),
        declaring_class_name,
        attr_names,
        attr_args,
        constant_value: None,
        is_enum_case: false,
        flags,
        modifiers: reflection_method_modifiers_from_flags(flags),
        type_metadata: None,
        default_value: None,
        required_parameter_count,
        parameters,
    })
}

/// Builds ReflectionMethod array entries for methods declared by an interface.
fn reflection_interface_method_members(
    info: &InterfaceInfo,
    interface_name: &str,
    method_names: &[String],
) -> Vec<ReflectionListedMember> {
    method_names
        .iter()
        .filter_map(|method_name| {
            reflection_interface_method_member(info, interface_name, method_name)
        })
        .collect()
}

/// Builds one ReflectionMethod array entry from interface metadata.
fn reflection_interface_method_member(
    info: &InterfaceInfo,
    interface_name: &str,
    method_name: &str,
) -> Option<ReflectionListedMember> {
    let method_key = php_symbol_key(method_name);
    let (sig, is_static) = info
        .methods
        .get(&method_key)
        .map(|sig| (sig, false))
        .or_else(|| info.static_methods.get(&method_key).map(|sig| (sig, true)))?;
    let declaring_class_name = info
        .method_declaring_interfaces
        .get(&method_key)
        .or_else(|| info.static_method_declaring_interfaces.get(&method_key))
        .cloned()
        .unwrap_or_else(|| interface_name.to_string());
    let required_parameter_count = reflection_required_parameter_count(sig);
    let flags = reflection_member_flags(is_static, &Visibility::Public, false, true, false);
    let declaring_function = ReflectionDeclaringFunctionMember::Method {
        name: method_key.clone(),
        declaring_class_name: Some(declaring_class_name.clone()),
        attr_names: Vec::new(),
        attr_args: Vec::new(),
        flags,
        required_parameter_count,
    };
    let parameters = reflection_parameter_members_with_declaring_class(
        sig,
        Some(declaring_class_name.as_str()),
        Some(declaring_function),
    );
    Some(ReflectionListedMember {
        name: method_key,
        declaring_class_name: Some(declaring_class_name),
        attr_names: Vec::new(),
        attr_args: Vec::new(),
        constant_value: None,
        is_enum_case: false,
        flags,
        modifiers: reflection_method_modifiers_from_flags(flags),
        type_metadata: None,
        default_value: None,
        required_parameter_count,
        parameters,
    })
}

/// Builds ReflectionMethod array entries for methods declared by a trait.
fn reflection_trait_method_members(
    methods: &std::collections::HashMap<String, TraitMethodInfo>,
    trait_name: &str,
    method_names: &[String],
) -> Vec<ReflectionListedMember> {
    method_names
        .iter()
        .filter_map(|method_name| reflection_trait_method_member(methods, trait_name, method_name))
        .collect()
}

/// Builds one ReflectionMethod array entry from retained trait metadata.
fn reflection_trait_method_member(
    methods: &std::collections::HashMap<String, TraitMethodInfo>,
    trait_name: &str,
    method_name: &str,
) -> Option<ReflectionListedMember> {
    let method_key = php_symbol_key(method_name);
    let info = methods.get(&method_key)?;
    let flags = reflection_member_flags(
        info.is_static,
        &info.visibility,
        info.is_final,
        info.is_abstract,
        false,
    );
    let required_parameter_count = reflection_required_parameter_count(&info.signature);
    let declaring_function = ReflectionDeclaringFunctionMember::Method {
        name: method_key.clone(),
        declaring_class_name: Some(trait_name.to_string()),
        attr_names: Vec::new(),
        attr_args: Vec::new(),
        flags,
        required_parameter_count,
    };
    Some(ReflectionListedMember {
        name: method_key,
        declaring_class_name: Some(trait_name.to_string()),
        attr_names: Vec::new(),
        attr_args: Vec::new(),
        constant_value: None,
        is_enum_case: false,
        flags,
        modifiers: reflection_method_modifiers_from_flags(flags),
        type_metadata: None,
        default_value: None,
        required_parameter_count,
        parameters: reflection_parameter_members_with_declaring_class(
            &info.signature,
            Some(trait_name),
            Some(declaring_function),
        ),
    })
}

/// Builds ReflectionProperty array entries for the properties visible on one class.
fn reflection_class_property_members(
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
fn reflection_class_property_member(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    info: &crate::types::ClassInfo,
    property_name: &str,
) -> Option<ReflectionListedMember> {
    let flags = reflection_property_member_flags(info, property_name).or_else(|| {
        (is_reflection_enum(ctx, class_name) && property_name == "name").then_some(
            reflection_member_flags(false, &Visibility::Public, false, false, true),
        )
    })?;
    let type_metadata = reflection_property_type_metadata(info, property_name);
    let default_value = reflection_property_default_value(info, property_name);
    Some(ReflectionListedMember {
        name: property_name.to_string(),
        declaring_class_name: reflection_property_declaring_class_name(info, property_name)
            .or_else(|| {
                (is_reflection_enum(ctx, class_name) && property_name == "name")
                    .then(|| class_name.to_string())
            }),
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
        is_enum_case: false,
        flags,
        modifiers: reflection_property_modifiers_for_info(info, property_name)
            .unwrap_or_else(|| reflection_property_modifiers_from_flags(flags)),
        type_metadata,
        default_value,
        required_parameter_count: 0,
        parameters: Vec::new(),
    })
}

/// Returns reflection type metadata for one typed property visible on a class.
fn reflection_property_type_metadata(
    info: &crate::types::ClassInfo,
    property_name: &str,
) -> Option<ReflectionParameterTypeMetadata> {
    if !info.visible_property_is_declared(property_name) {
        return None;
    }
    let (_, (_, property_type)) = info.visible_property(property_name)?;
    reflection_parameter_type_metadata(None, property_type)
}

/// Returns supported default metadata for one reflected property.
fn reflection_property_default_value(
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
fn reflection_property_slot_default_value(
    is_declared: bool,
    default: Option<&Expr>,
) -> Option<ReflectionParameterDefaultValue> {
    match default {
        Some(default) => reflection_parameter_default_value(default),
        None if !is_declared => Some(ReflectionParameterDefaultValue::Null),
        None => None,
    }
}

/// Builds placeholder ReflectionMethod entries for class-like metadata without full method schemas.
fn default_method_members(
    method_names: &[String],
    is_interface: bool,
    declaring_class_name: &str,
) -> Vec<ReflectionListedMember> {
    method_names
        .iter()
        .map(|name| ReflectionListedMember {
            name: name.clone(),
            declaring_class_name: Some(declaring_class_name.to_string()),
            attr_names: Vec::new(),
            attr_args: Vec::new(),
            constant_value: None,
            is_enum_case: false,
            flags: reflection_member_flags(false, &Visibility::Public, false, is_interface, false),
            modifiers: reflection_method_modifiers_from_flags(reflection_member_flags(
                false,
                &Visibility::Public,
                false,
                is_interface,
                false,
            )),
            type_metadata: None,
            default_value: None,
            required_parameter_count: 0,
            parameters: Vec::new(),
        })
        .collect()
}

/// Builds placeholder ReflectionProperty entries for class-like metadata without full property schemas.
fn default_property_members(
    property_names: &[String],
    is_interface: bool,
    declaring_class_name: &str,
) -> Vec<ReflectionListedMember> {
    property_names
        .iter()
        .map(|name| ReflectionListedMember {
            name: name.clone(),
            declaring_class_name: Some(declaring_class_name.to_string()),
            attr_names: Vec::new(),
            attr_args: Vec::new(),
            constant_value: None,
            is_enum_case: false,
            flags: reflection_member_flags(false, &Visibility::Public, false, is_interface, false),
            modifiers: reflection_property_modifiers(
                &Visibility::Public,
                false,
                false,
                is_interface,
                false,
                is_interface,
                None,
            ),
            type_metadata: None,
            default_value: None,
            required_parameter_count: 0,
            parameters: Vec::new(),
        })
        .collect()
}

/// Returns PHP's required parameter count for a reflected native signature.
fn reflection_required_parameter_count(sig: &FunctionSig) -> i64 {
    let fixed_count = sig
        .variadic
        .as_deref()
        .and_then(|variadic| {
            sig.params
                .iter()
                .position(|(name, _)| name.as_str() == variadic)
        })
        .unwrap_or(sig.params.len());
    (0..fixed_count)
        .rfind(|index| !sig.defaults.get(*index).is_some_and(Option::is_some))
        .map_or(0, |index| index as i64 + 1)
}

/// Builds reflected parameter metadata and attaches declaring class metadata when present.
fn reflection_parameter_members_with_declaring_class(
    sig: &FunctionSig,
    declaring_class_name: Option<&str>,
    declaring_function: Option<ReflectionDeclaringFunctionMember>,
) -> Vec<ReflectionParameterMember> {
    reflection_parameter_members_with_declaring_function(
        sig,
        declaring_class_name,
        declaring_function,
    )
}

/// Builds reflected parameter metadata with optional declaring owner metadata.
fn reflection_parameter_members_with_declaring_function(
    sig: &FunctionSig,
    declaring_class_name: Option<&str>,
    declaring_function: Option<ReflectionDeclaringFunctionMember>,
) -> Vec<ReflectionParameterMember> {
    sig.params
        .iter()
        .enumerate()
        .map(|(index, (name, ty))| {
            let is_variadic = sig.variadic.as_deref() == Some(name.as_str());
            let has_type = sig.declared_params.get(index).copied().unwrap_or(false);
            ReflectionParameterMember {
                name: name.clone(),
                declaring_class_name: declaring_class_name.map(str::to_string),
                declaring_function: declaring_function.clone(),
                attr_names: sig
                    .param_attributes
                    .get(index)
                    .map(|groups| crate::types::collect_attribute_names(groups))
                    .unwrap_or_default(),
                attr_args: sig
                    .param_attributes
                    .get(index)
                    .map(|groups| crate::types::collect_attribute_args(groups))
                    .unwrap_or_default(),
                position: index as i64,
                is_optional: is_variadic
                    || sig
                        .defaults
                        .get(index)
                        .map(|default| default.is_some())
                        .unwrap_or(false),
                is_variadic,
                is_passed_by_reference: sig.ref_params.get(index).copied().unwrap_or(false),
                has_type,
                type_metadata: reflection_parameter_type_metadata(
                    sig.param_type_exprs.get(index).and_then(Option::as_ref),
                    ty,
                )
                .filter(|_| has_type),
                default_value: sig
                    .defaults
                    .get(index)
                    .and_then(Option::as_ref)
                    .and_then(reflection_parameter_default_value),
            }
        })
        .collect()
}

/// Converts a supported parameter default expression into Reflection metadata.
fn reflection_parameter_default_value(default: &Expr) -> Option<ReflectionParameterDefaultValue> {
    match &default.kind {
        ExprKind::IntLiteral(value) => Some(ReflectionParameterDefaultValue::Int(*value)),
        ExprKind::BoolLiteral(value) => Some(ReflectionParameterDefaultValue::Bool(*value)),
        ExprKind::FloatLiteral(value) => Some(ReflectionParameterDefaultValue::Float(*value)),
        ExprKind::StringLiteral(value) => Some(ReflectionParameterDefaultValue::Str(value.clone())),
        ExprKind::Null => Some(ReflectionParameterDefaultValue::Null),
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(value) => value
                .checked_neg()
                .map(ReflectionParameterDefaultValue::Int),
            ExprKind::FloatLiteral(value) => Some(ReflectionParameterDefaultValue::Float(-value)),
            _ => None,
        },
        _ => None,
    }
}

/// Converts a normalized parameter type into a supported `ReflectionType` subset.
fn reflection_parameter_type_metadata(
    type_expr: Option<&TypeExpr>,
    ty: &PhpType,
) -> Option<ReflectionParameterTypeMetadata> {
    if let Some(TypeExpr::Intersection(members)) = type_expr {
        return reflection_intersection_type_metadata(members);
    }
    match ty {
        PhpType::Union(members) => reflection_union_or_nullable_type_metadata(members),
        _ => reflection_named_type_metadata(ty).map(ReflectionParameterTypeMetadata::Named),
    }
}

/// Converts a normalized non-union parameter type into a simple `ReflectionNamedType`.
fn reflection_named_type_metadata(ty: &PhpType) -> Option<ReflectionNamedTypeMetadata> {
    match ty {
        PhpType::Int => Some(reflection_builtin_named_type("int", false)),
        PhpType::Float => Some(reflection_builtin_named_type("float", false)),
        PhpType::Str => Some(reflection_builtin_named_type("string", false)),
        PhpType::Bool => Some(reflection_builtin_named_type("bool", false)),
        PhpType::Iterable => Some(reflection_builtin_named_type("iterable", false)),
        PhpType::Mixed => Some(reflection_builtin_named_type("mixed", true)),
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            Some(reflection_builtin_named_type("array", false))
        }
        PhpType::Callable => Some(reflection_builtin_named_type("callable", false)),
        PhpType::Object(name) => Some(ReflectionNamedTypeMetadata {
            name: name.clone(),
            allows_null: false,
            is_builtin: false,
        }),
        _ => None,
    }
}

/// Builds metadata for one builtin named type.
fn reflection_builtin_named_type(name: &str, allows_null: bool) -> ReflectionNamedTypeMetadata {
    ReflectionNamedTypeMetadata {
        name: name.to_string(),
        allows_null,
        is_builtin: true,
    }
}

/// Handles `T|null` as a nullable named type and wider unions as `ReflectionUnionType`.
fn reflection_union_or_nullable_type_metadata(
    members: &[PhpType],
) -> Option<ReflectionParameterTypeMetadata> {
    let allows_null = members.iter().any(|member| matches!(member, PhpType::Void));
    let non_null_members = members
        .iter()
        .filter(|member| !matches!(member, PhpType::Void))
        .collect::<Vec<_>>();
    if non_null_members.len() == 1 {
        let mut metadata = reflection_named_type_metadata(non_null_members[0])?;
        metadata.allows_null = allows_null;
        return Some(ReflectionParameterTypeMetadata::Named(metadata));
    }
    let types = non_null_members
        .into_iter()
        .map(reflection_named_type_metadata)
        .collect::<Option<Vec<_>>>()?;
    (!types.is_empty()).then_some(ReflectionParameterTypeMetadata::Union(
        ReflectionUnionTypeMetadata { types, allows_null },
    ))
}

/// Converts a declared `A&B` type into `ReflectionIntersectionType` metadata.
fn reflection_intersection_type_metadata(
    members: &[TypeExpr],
) -> Option<ReflectionParameterTypeMetadata> {
    let types = members
        .iter()
        .map(reflection_named_type_metadata_from_type_expr)
        .collect::<Option<Vec<_>>>()?;
    (!types.is_empty()).then_some(ReflectionParameterTypeMetadata::Intersection(
        ReflectionIntersectionTypeMetadata { types },
    ))
}

/// Converts one declared type atom into `ReflectionNamedType` metadata.
fn reflection_named_type_metadata_from_type_expr(
    type_expr: &TypeExpr,
) -> Option<ReflectionNamedTypeMetadata> {
    match type_expr {
        TypeExpr::Int => Some(reflection_builtin_named_type("int", false)),
        TypeExpr::Float => Some(reflection_builtin_named_type("float", false)),
        TypeExpr::Bool => Some(reflection_builtin_named_type("bool", false)),
        TypeExpr::Str => Some(reflection_builtin_named_type("string", false)),
        TypeExpr::Iterable => Some(reflection_builtin_named_type("iterable", false)),
        TypeExpr::Array(_) => Some(reflection_builtin_named_type("array", false)),
        TypeExpr::Named(name) => {
            let raw_name = name.as_str().trim_start_matches('\\');
            match raw_name.to_ascii_lowercase().as_str() {
                "array" | "callable" | "mixed" | "object" => {
                    Some(reflection_builtin_named_type(raw_name, false))
                }
                _ => Some(ReflectionNamedTypeMetadata {
                    name: raw_name.to_string(),
                    allows_null: false,
                    is_builtin: false,
                }),
            }
        }
        _ => None,
    }
}

/// Returns the `__construct` member object metadata when the reflected class-like symbol has one.
fn reflection_constructor_member(
    method_members: &[ReflectionListedMember],
) -> Option<ReflectionListedMember> {
    method_members
        .iter()
        .find(|member| php_symbol_key(&member.name) == "__construct")
        .cloned()
}

/// Builds common ReflectionMethod/ReflectionProperty predicate flags.
fn reflection_member_flags(
    is_static: bool,
    visibility: &Visibility,
    is_final: bool,
    is_abstract: bool,
    is_readonly: bool,
) -> ReflectionMemberFlags {
    ReflectionMemberFlags {
        is_static,
        is_public: visibility == &Visibility::Public,
        is_protected: visibility == &Visibility::Protected,
        is_private: visibility == &Visibility::Private,
        is_final,
        is_abstract,
        is_readonly,
    }
}

/// Returns PHP case-insensitive method names declared by an interface and its parents.
fn reflection_interface_method_names(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
) -> Vec<String> {
    let Some(interface_name) = resolve_reflection_interface(ctx, interface_name) else {
        return Vec::new();
    };
    let Some(info) = ctx.module.interface_infos.get(interface_name) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    push_unique_method_names(info.methods.keys(), &mut names, &mut seen);
    push_unique_method_names(info.static_methods.keys(), &mut names, &mut seen);
    names
}

/// Returns PHP case-sensitive property names declared by an interface and its parents.
fn reflection_interface_property_names(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
) -> Vec<String> {
    let Some(interface_name) = resolve_reflection_interface(ctx, interface_name) else {
        return Vec::new();
    };
    let Some(info) = ctx.module.interface_infos.get(interface_name) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for property in info.properties.keys() {
        push_unique_property_name(property, &mut names, &mut seen);
    }
    names
}

/// Returns PHP case-sensitive constant names declared by an interface and its parents.
fn reflection_interface_constant_names(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
) -> Vec<String> {
    let Some(interface_name) = resolve_reflection_interface(ctx, interface_name) else {
        return Vec::new();
    };
    let Some(info) = ctx.module.interface_infos.get(interface_name) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for constant in info.constants.keys() {
        push_unique_constant_name(constant, &mut names, &mut seen);
    }
    names
}

/// Returns PHP case-insensitive direct method names declared by a trait.
fn reflection_trait_method_names(ctx: &FunctionContext<'_>, trait_name: &str) -> Vec<String> {
    ctx.module
        .declared_trait_method_names
        .get(trait_name)
        .cloned()
        .unwrap_or_default()
}

/// Returns PHP case-sensitive direct property names declared by a trait.
fn reflection_trait_property_names(ctx: &FunctionContext<'_>, trait_name: &str) -> Vec<String> {
    ctx.module
        .declared_trait_property_names
        .get(trait_name)
        .cloned()
        .unwrap_or_default()
}

/// Returns PHP case-sensitive direct constant names declared by a trait.
fn reflection_trait_constant_names(ctx: &FunctionContext<'_>, trait_name: &str) -> Vec<String> {
    ctx.module
        .declared_trait_constant_names
        .get(trait_name)
        .cloned()
        .unwrap_or_default()
}

/// Appends lower-case method names while preserving first-seen order.
fn push_unique_method_names<'a>(
    method_names: impl Iterator<Item = &'a String>,
    names: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
) {
    for method_name in method_names {
        let key = php_symbol_key(method_name);
        if seen.insert(key.clone()) {
            names.push(key);
        }
    }
}

/// Appends one case-sensitive property name while preserving first-seen order.
fn push_unique_property_name(
    property_name: &str,
    names: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
) {
    if seen.insert(property_name.to_string()) {
        names.push(property_name.to_string());
    }
}

/// Appends one case-sensitive class constant name while preserving first-seen order.
fn push_unique_constant_name(
    constant_name: &str,
    names: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
) {
    if seen.insert(constant_name.to_string()) {
        names.push(constant_name.to_string());
    }
}

/// Appends one constant metadata member while preserving first-seen order.
fn push_unique_constant_member(
    constant_name: &str,
    value: ReflectionConstantValue,
    members: &mut Vec<ReflectionConstantMember>,
    seen: &mut std::collections::HashSet<String>,
) {
    if seen.insert(constant_name.to_string()) {
        members.push(ReflectionConstantMember {
            name: constant_name.to_string(),
            value,
        });
    }
}

/// Evaluates one class/interface/trait constant expression for static Reflection metadata.
fn reflection_constant_value(
    ctx: &FunctionContext<'_>,
    current_class: &str,
    current_info: Option<&crate::types::ClassInfo>,
    expr: &Expr,
    depth: usize,
) -> Result<ReflectionConstantValue> {
    if depth > 16 {
        return Err(CodegenIrError::unsupported(
            "deep recursive ReflectionClass constant metadata",
        ));
    }
    match &expr.kind {
        ExprKind::IntLiteral(value) => Ok(ReflectionConstantValue::Int(*value)),
        ExprKind::BoolLiteral(value) => Ok(ReflectionConstantValue::Bool(*value)),
        ExprKind::FloatLiteral(value) => Ok(ReflectionConstantValue::Float(*value)),
        ExprKind::StringLiteral(value) => Ok(ReflectionConstantValue::Str(value.clone())),
        ExprKind::Null => Ok(ReflectionConstantValue::Null),
        ExprKind::Negate(inner) => {
            match reflection_constant_value(ctx, current_class, current_info, inner, depth + 1)? {
                ReflectionConstantValue::Int(value) => Ok(ReflectionConstantValue::Int(-value)),
                ReflectionConstantValue::Float(value) => Ok(ReflectionConstantValue::Float(-value)),
                other => Err(unsupported_reflection_constant_value(other)),
            }
        }
        ExprKind::BinaryOp { left, op, right } => reflection_binary_constant_value(
            ctx,
            current_class,
            current_info,
            left,
            op,
            right,
            depth + 1,
        ),
        ExprKind::ClassConstant { receiver } => {
            let class_name =
                reflection_static_receiver_name(current_class, current_info, receiver)?;
            Ok(ReflectionConstantValue::Str(class_name))
        }
        ExprKind::ScopedConstantAccess { receiver, name } => reflection_scoped_constant_value(
            ctx,
            current_class,
            current_info,
            receiver,
            name,
            depth + 1,
        ),
        other => Err(CodegenIrError::unsupported(format!(
            "ReflectionClass constant metadata expression {:?}",
            other
        ))),
    }
}

/// Evaluates one supported binary operator in a static Reflection constant expression.
fn reflection_binary_constant_value(
    ctx: &FunctionContext<'_>,
    current_class: &str,
    current_info: Option<&crate::types::ClassInfo>,
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    depth: usize,
) -> Result<ReflectionConstantValue> {
    let left = reflection_constant_value(ctx, current_class, current_info, left, depth)?;
    let right = reflection_constant_value(ctx, current_class, current_info, right, depth)?;
    match (left, op, right) {
        (ReflectionConstantValue::Int(left), BinOp::Add, ReflectionConstantValue::Int(right)) => {
            Ok(ReflectionConstantValue::Int(left + right))
        }
        (ReflectionConstantValue::Int(left), BinOp::Sub, ReflectionConstantValue::Int(right)) => {
            Ok(ReflectionConstantValue::Int(left - right))
        }
        (ReflectionConstantValue::Int(left), BinOp::Mul, ReflectionConstantValue::Int(right)) => {
            Ok(ReflectionConstantValue::Int(left * right))
        }
        (ReflectionConstantValue::Int(left), BinOp::Mod, ReflectionConstantValue::Int(right)) => {
            Ok(ReflectionConstantValue::Int(left % right))
        }
        (ReflectionConstantValue::Int(left), BinOp::Pow, ReflectionConstantValue::Int(right))
            if right >= 0 =>
        {
            Ok(ReflectionConstantValue::Int(left.pow(right as u32)))
        }
        (
            ReflectionConstantValue::Str(left),
            BinOp::Concat,
            ReflectionConstantValue::Str(right),
        ) => Ok(ReflectionConstantValue::Str(format!("{}{}", left, right))),
        (left, _, right) => Err(CodegenIrError::unsupported(format!(
            "ReflectionClass constant metadata binary value {:?} {:?}",
            reflection_constant_value_kind(&left),
            reflection_constant_value_kind(&right)
        ))),
    }
}

/// Resolves and evaluates one scoped class/interface/trait constant value.
fn reflection_scoped_constant_value(
    ctx: &FunctionContext<'_>,
    current_class: &str,
    current_info: Option<&crate::types::ClassInfo>,
    receiver: &StaticReceiver,
    constant_name: &str,
    depth: usize,
) -> Result<ReflectionConstantValue> {
    let class_name = reflection_static_receiver_name(current_class, current_info, receiver)?;
    if let Some((resolved_name, info)) = resolve_reflection_class(ctx, &class_name) {
        if let Some(value_expr) = info.constants.get(constant_name) {
            return reflection_constant_value(ctx, resolved_name, Some(info), value_expr, depth);
        }
        for interface_name in &info.interfaces {
            if let Some(value_expr) =
                reflection_interface_constant_expr(ctx, interface_name, constant_name)
            {
                return reflection_constant_value(ctx, interface_name, None, &value_expr, depth);
            }
        }
    }
    if let Some(interface_name) = resolve_reflection_interface(ctx, &class_name) {
        if let Some(value_expr) =
            reflection_interface_constant_expr(ctx, interface_name, constant_name)
        {
            return reflection_constant_value(ctx, interface_name, None, &value_expr, depth);
        }
    }
    if let Some(trait_name) = resolve_reflection_trait(ctx, &class_name) {
        if let Some(value_expr) = ctx
            .module
            .declared_trait_constants
            .get(trait_name)
            .and_then(|constants| constants.get(constant_name))
        {
            return reflection_constant_value(ctx, trait_name, None, value_expr, depth);
        }
    }
    if ctx
        .module
        .enum_infos
        .get(&class_name)
        .is_some_and(|info| info.cases.iter().any(|case| case.name == constant_name))
    {
        return Ok(ReflectionConstantValue::EnumCase {
            enum_name: class_name,
            case_name: constant_name.to_string(),
        });
    }
    Err(CodegenIrError::unsupported(format!(
        "ReflectionClass constant metadata for {}::{}",
        current_class, constant_name
    )))
}

/// Returns an interface constant expression, including inherited parent interfaces.
fn reflection_interface_constant_expr(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
    constant_name: &str,
) -> Option<Expr> {
    let mut visited = std::collections::HashSet::new();
    let mut queue = vec![interface_name.to_string()];
    while let Some(name) = queue.pop() {
        if !visited.insert(name.clone()) {
            continue;
        }
        if let Some(info) = ctx.module.interface_infos.get(&name) {
            if let Some(value) = info.constants.get(constant_name) {
                return Some(value.clone());
            }
            queue.extend(info.parents.iter().cloned());
        }
    }
    None
}

/// Resolves a static receiver against the current reflected declaration.
fn reflection_static_receiver_name(
    current_class: &str,
    current_info: Option<&crate::types::ClassInfo>,
    receiver: &StaticReceiver,
) -> Result<String> {
    match receiver {
        StaticReceiver::Named(name) => Ok(name.as_str().trim_start_matches('\\').to_string()),
        StaticReceiver::Self_ | StaticReceiver::Static => Ok(current_class.to_string()),
        StaticReceiver::Parent => current_info
            .and_then(|info| info.parent.clone())
            .ok_or_else(|| {
                CodegenIrError::unsupported(format!(
                    "ReflectionClass constant metadata parent receiver in {}",
                    current_class
                ))
            }),
    }
}

/// Returns a small label for unsupported constant-value diagnostics.
fn reflection_constant_value_kind(value: &ReflectionConstantValue) -> &'static str {
    match value {
        ReflectionConstantValue::Int(_) => "int",
        ReflectionConstantValue::Bool(_) => "bool",
        ReflectionConstantValue::Float(_) => "float",
        ReflectionConstantValue::Str(_) => "string",
        ReflectionConstantValue::Null => "null",
        ReflectionConstantValue::EnumCase { .. } => "enum-case",
    }
}

/// Reports an unsupported unary constant value while avoiding large debug output.
fn unsupported_reflection_constant_value(value: ReflectionConstantValue) -> CodegenIrError {
    CodegenIrError::unsupported(format!(
        "ReflectionClass constant metadata unary value {}",
        reflection_constant_value_kind(&value)
    ))
}

/// Looks up class-constant metadata by PHP-style class name and case-sensitive constant name.
fn resolve_reflection_class_constant<'a>(
    ctx: &'a FunctionContext<'_>,
    class_name: &str,
    constant_name: &str,
) -> Option<(&'a str, &'a crate::types::ClassInfo)> {
    let (resolved_name, info) = resolve_reflection_class(ctx, class_name)?;
    if info.constants.contains_key(constant_name) {
        return Some((resolved_name, info));
    }
    let parent = info.parent.as_deref()?;
    resolve_reflection_class_constant(ctx, parent, constant_name)
}

/// Looks up enum-case metadata by PHP-style enum name and case-sensitive case name.
fn resolve_reflection_enum_case<'a>(
    ctx: &'a FunctionContext<'_>,
    enum_name: &str,
    case_name: &str,
) -> Option<(&'a str, &'a crate::types::EnumCaseInfo)> {
    let enum_key = php_symbol_key(enum_name.trim_start_matches('\\'));
    ctx.module
        .enum_infos
        .iter()
        .find(|(candidate, _)| php_symbol_key(candidate.trim_start_matches('\\')) == enum_key)
        .and_then(|(name, info)| {
            info.cases
                .iter()
                .find(|case| case.name == case_name)
                .map(|case| (name.as_str(), case))
        })
}

/// Returns a static Reflection value for a backed enum case, when present.
fn reflection_enum_case_backing_value(case: &EnumCaseInfo) -> Option<ReflectionConstantValue> {
    match case.value.as_ref()? {
        EnumCaseValue::Int(value) => Some(ReflectionConstantValue::Int(*value)),
        EnumCaseValue::Str(value) => Some(ReflectionConstantValue::Str(value.clone())),
    }
}

/// Returns empty Reflection metadata for unsupported dynamic constructor operands.
fn empty_reflection_metadata() -> ReflectionOwnerMetadata {
    ReflectionOwnerMetadata {
        reflected_name: None,
        attr_names: Vec::new(),
        attr_args: Vec::new(),
        interface_names: Vec::new(),
        trait_names: Vec::new(),
        parent_names: Vec::new(),
        method_names: Vec::new(),
        property_names: Vec::new(),
        constant_names: Vec::new(),
        constant_members: Vec::new(),
        default_property_members: Vec::new(),
        constant_reflection_members: Vec::new(),
        method_members: Vec::new(),
        property_members: Vec::new(),
        constructor_member: None,
        parent_class_name: None,
        constant_value: None,
        backing_value: None,
        is_enum_case: false,
        parameter_members: Vec::new(),
        type_metadata: None,
        property_default_value: None,
        required_parameter_count: 0,
        is_final: false,
        is_abstract: false,
        is_interface: false,
        is_trait: false,
        is_enum: false,
        is_readonly: false,
        is_anonymous: false,
        is_instantiable: false,
        is_cloneable: false,
        modifiers: 0,
        member_flags: ReflectionMemberFlags::default(),
    }
}

/// Extracts a constant string or class-name operand from an EIR value.
fn const_string_or_class_operand(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    owner: &str,
) -> Result<String> {
    const_data_operand(ctx, value, owner, true)
}

/// Extracts a constant string operand from an EIR value.
fn const_required_string_operand(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    owner: &str,
) -> Result<String> {
    const_data_operand(ctx, value, owner, false)
}

/// Extracts a constant ReflectionParameter name or offset selector from EIR.
fn const_parameter_selector_operand(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<ReflectionParameterSelector> {
    let value_ref = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Err(CodegenIrError::unsupported(
            "ReflectionParameter constructor with non-literal parameter selector",
        ));
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    match inst_ref.op {
        Op::ConstI64 => match inst_ref.immediate {
            Some(Immediate::I64(value)) => Ok(ReflectionParameterSelector::Position(value)),
            _ => Err(CodegenIrError::invalid_module(
                "ReflectionParameter position selector missing i64 immediate",
            )),
        },
        Op::ConstStr => {
            let Some(Immediate::Data(data)) = inst_ref.immediate else {
                return Err(CodegenIrError::invalid_module(
                    "ReflectionParameter name selector missing data id",
                ));
            };
            ctx.module
                .data
                .strings
                .get(data.as_raw() as usize)
                .cloned()
                .map(ReflectionParameterSelector::Name)
                .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))
        }
        _ => Err(CodegenIrError::unsupported(
            "ReflectionParameter constructor with non-literal parameter selector",
        )),
    }
}

/// Reads a `ConstStr` or optional `ConstClassName` value from the module data pool.
fn const_data_operand(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    owner: &str,
    allow_class_name: bool,
) -> Result<String> {
    let value_ref = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Err(CodegenIrError::unsupported(format!(
            "{} constructor with non-literal reflection argument",
            owner
        )));
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    let Some(Immediate::Data(data)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(format!(
            "{} reflection literal missing data id",
            owner
        )));
    };
    match inst_ref.op {
        Op::ConstStr => ctx
            .module
            .data
            .strings
            .get(data.as_raw() as usize)
            .cloned()
            .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw())),
        Op::ConstClassName if allow_class_name => ctx
            .module
            .data
            .class_names
            .get(data.as_raw() as usize)
            .cloned()
            .ok_or_else(|| CodegenIrError::missing_entry("class data", data.as_raw())),
        _ => Err(CodegenIrError::unsupported(format!(
            "{} constructor with non-literal reflection argument",
            owner
        ))),
    }
}

/// Writes a heap-persisted string into the current Reflection object result slot.
fn emit_reflection_string_property(
    ctx: &mut FunctionContext<'_>,
    value: &str,
    low_offset: usize,
    high_offset: usize,
) {
    let (label, len) = ctx.data.add_string(value.as_bytes());
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            abi::emit_pop_reg(ctx.emitter, object_reg);
            abi::emit_store_to_address(ctx.emitter, "x1", object_reg, low_offset);
            abi::emit_store_to_address(ctx.emitter, "x2", object_reg, high_offset);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rax", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            abi::emit_pop_reg(ctx.emitter, object_reg);
            abi::emit_store_to_address(ctx.emitter, "rax", object_reg, low_offset);
            abi::emit_store_to_address(ctx.emitter, "rdx", object_reg, high_offset);
        }
    }
    abi::emit_push_reg(ctx.emitter, object_reg);
    abi::emit_pop_reg(ctx.emitter, result_reg);
}

/// Writes a heap-persisted string into a named ReflectionClass property slot.
fn emit_reflection_string_property_by_name(
    ctx: &mut FunctionContext<'_>,
    property_name: &str,
    value: &str,
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get("ReflectionClass")
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let low_offset = reflection_property_offset(class_info, property_name)?;
    emit_reflection_string_property(ctx, value, low_offset, low_offset + 8);
    Ok(())
}

/// Replaces the Reflection object's default `__attrs` array with populated metadata.
fn emit_reflection_attrs_property(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    attr_names: &[String],
    attr_args: &[Option<Vec<AttrArgValue>>],
) -> Result<()> {
    let (attrs_low_offset, attrs_high_offset) = reflection_attrs_offsets(ctx, class_name)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_reg, 0);
    abi::emit_load_from_address(ctx.emitter, result_reg, object_reg, attrs_low_offset);
    abi::emit_call_label(ctx.emitter, "__rt_decref_array");
    super::super::builtins::attributes::emit_reflection_attribute_array(
        ctx, attr_names, attr_args,
    )?;
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, object_reg, attrs_low_offset);
    abi::emit_load_int_immediate(ctx.emitter, abi::secondary_scratch_reg(ctx.emitter), 4);
    abi::emit_store_to_address(
        ctx.emitter,
        abi::secondary_scratch_reg(ctx.emitter),
        object_reg,
        attrs_high_offset,
    );
    abi::emit_push_reg(ctx.emitter, object_reg);
    abi::emit_pop_reg(ctx.emitter, result_reg);
    Ok(())
}

/// Replaces a ReflectionClass private array slot with an indexed string array.
fn emit_reflection_string_array_property_by_name(
    ctx: &mut FunctionContext<'_>,
    property_name: &str,
    names: &[String],
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get("ReflectionClass")
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let low_offset = reflection_property_offset(class_info, property_name)?;
    let high_offset = low_offset + 8;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_reg, 0);
    abi::emit_load_from_address(ctx.emitter, result_reg, object_reg, low_offset);
    abi::emit_call_label(ctx.emitter, "__rt_decref_array");
    emit_reflection_string_array(ctx, names)?;
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, object_reg, low_offset);
    abi::emit_load_int_immediate(ctx.emitter, abi::secondary_scratch_reg(ctx.emitter), 4);
    abi::emit_store_to_address(
        ctx.emitter,
        abi::secondary_scratch_reg(ctx.emitter),
        object_reg,
        high_offset,
    );
    abi::emit_push_reg(ctx.emitter, object_reg);
    abi::emit_pop_reg(ctx.emitter, result_reg);
    Ok(())
}

/// Replaces a ReflectionClass private slot with an associative constant-value array.
fn emit_reflection_constant_array_property_by_name(
    ctx: &mut FunctionContext<'_>,
    property_name: &str,
    members: &[ReflectionConstantMember],
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get("ReflectionClass")
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let low_offset = reflection_property_offset(class_info, property_name)?;
    let high_offset = low_offset + 8;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_reg, 0);
    abi::emit_load_from_address(ctx.emitter, result_reg, object_reg, low_offset);
    abi::emit_call_label(ctx.emitter, "__rt_decref_array");
    emit_reflection_constant_array(ctx, members)?;
    let assoc_type = PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    };
    emit_box_current_value_as_mixed(ctx.emitter, &assoc_type);
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, object_reg, low_offset);
    abi::emit_store_zero_to_address(ctx.emitter, object_reg, high_offset);
    abi::emit_push_reg(ctx.emitter, object_reg);
    abi::emit_pop_reg(ctx.emitter, result_reg);
    Ok(())
}

/// Replaces a ReflectionClass private slot with an associative default-property array.
fn emit_reflection_default_property_array_property_by_name(
    ctx: &mut FunctionContext<'_>,
    property_name: &str,
    members: &[ReflectionDefaultPropertyMember],
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get("ReflectionClass")
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let low_offset = reflection_property_offset(class_info, property_name)?;
    let high_offset = low_offset + 8;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_reg, 0);
    abi::emit_load_from_address(ctx.emitter, result_reg, object_reg, low_offset);
    abi::emit_call_label(ctx.emitter, "__rt_decref_array");
    emit_reflection_default_property_array(ctx, members);
    let assoc_type = PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    };
    emit_box_current_value_as_mixed(ctx.emitter, &assoc_type);
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, object_reg, low_offset);
    abi::emit_store_zero_to_address(ctx.emitter, object_reg, high_offset);
    abi::emit_push_reg(ctx.emitter, object_reg);
    abi::emit_pop_reg(ctx.emitter, result_reg);
    Ok(())
}

/// Replaces a ReflectionClass private array slot with ReflectionMethod/Property objects.
fn emit_reflection_member_array_property_by_name(
    ctx: &mut FunctionContext<'_>,
    property_name: &str,
    member_class_name: &str,
    members: &[ReflectionListedMember],
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get("ReflectionClass")
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let low_offset = reflection_property_offset(class_info, property_name)?;
    let high_offset = low_offset + 8;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_reg, 0);
    abi::emit_load_from_address(ctx.emitter, result_reg, object_reg, low_offset);
    abi::emit_call_label(ctx.emitter, "__rt_decref_array");
    emit_reflection_member_array(ctx, member_class_name, members)?;
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, object_reg, low_offset);
    abi::emit_load_int_immediate(ctx.emitter, abi::secondary_scratch_reg(ctx.emitter), 4);
    abi::emit_store_to_address(
        ctx.emitter,
        abi::secondary_scratch_reg(ctx.emitter),
        object_reg,
        high_offset,
    );
    abi::emit_push_reg(ctx.emitter, object_reg);
    abi::emit_pop_reg(ctx.emitter, result_reg);
    Ok(())
}

/// Replaces the ReflectionClass private constructor slot with `ReflectionMethod|null`.
fn emit_reflection_constructor_property(
    ctx: &mut FunctionContext<'_>,
    member: Option<&ReflectionListedMember>,
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get("ReflectionClass")
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let low_offset = reflection_property_offset(class_info, "__constructor")?;
    let high_offset = low_offset + 8;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    if let Some(member) = member {
        emit_reflection_member_object(ctx, "ReflectionMethod", member)?;
        emit_box_current_value_as_mixed(
            ctx.emitter,
            &PhpType::Object("ReflectionMethod".to_string()),
        );
    } else {
        super::emit_boxed_null(ctx);
    }
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, object_reg, low_offset);
    abi::emit_store_zero_to_address(ctx.emitter, object_reg, high_offset);
    abi::emit_reg_move(ctx.emitter, result_reg, object_reg);
    Ok(())
}

/// Replaces the ReflectionClass private parent slot with `ReflectionClass|false`.
fn emit_reflection_parent_class_property(
    ctx: &mut FunctionContext<'_>,
    parent_class_name: Option<&str>,
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get("ReflectionClass")
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let low_offset = reflection_property_offset(class_info, "__parent_class")?;
    let high_offset = low_offset + 8;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    if let Some(parent_class_name) = parent_class_name {
        let parent_metadata = reflection_class_metadata_for_name(ctx, parent_class_name)?;
        emit_reflection_owner_object(ctx, "ReflectionClass", &parent_metadata)?;
        emit_box_current_value_as_mixed(
            ctx.emitter,
            &PhpType::Object("ReflectionClass".to_string()),
        );
    } else {
        abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
        emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    }
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, object_reg, low_offset);
    abi::emit_store_zero_to_address(ctx.emitter, object_reg, high_offset);
    abi::emit_reg_move(ctx.emitter, result_reg, object_reg);
    Ok(())
}

/// Replaces a member reflector's private declaring-class slot with `ReflectionClass|false`.
fn emit_reflection_declaring_class_property(
    ctx: &mut FunctionContext<'_>,
    member_class_name: &str,
    declaring_class_name: Option<&str>,
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get(member_class_name)
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let Some(low_offset) = class_info
        .property_offsets
        .get("__declaring_class")
        .copied()
    else {
        return Ok(());
    };
    let high_offset = low_offset + 8;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    if let Some(declaring_class_name) = declaring_class_name {
        let declaring_metadata =
            reflection_shallow_class_metadata_for_name(ctx, declaring_class_name)?;
        emit_reflection_owner_object(ctx, "ReflectionClass", &declaring_metadata)?;
        emit_box_current_value_as_mixed(
            ctx.emitter,
            &PhpType::Object("ReflectionClass".to_string()),
        );
    } else {
        abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
        emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    }
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, object_reg, low_offset);
    abi::emit_store_zero_to_address(ctx.emitter, object_reg, high_offset);
    abi::emit_reg_move(ctx.emitter, result_reg, object_reg);
    Ok(())
}

/// Replaces a ReflectionMethod private array slot with ReflectionParameter objects.
fn emit_reflection_parameter_array_property_by_name(
    ctx: &mut FunctionContext<'_>,
    owner_class_name: &str,
    property_name: &str,
    parameters: &[ReflectionParameterMember],
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get(owner_class_name)
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let low_offset = reflection_property_offset(class_info, property_name)?;
    let high_offset = low_offset + 8;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_reg, 0);
    abi::emit_load_from_address(ctx.emitter, result_reg, object_reg, low_offset);
    abi::emit_call_label(ctx.emitter, "__rt_decref_array");
    emit_reflection_parameter_array(ctx, parameters)?;
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, object_reg, low_offset);
    abi::emit_load_int_immediate(ctx.emitter, abi::secondary_scratch_reg(ctx.emitter), 4);
    abi::emit_store_to_address(
        ctx.emitter,
        abi::secondary_scratch_reg(ctx.emitter),
        object_reg,
        high_offset,
    );
    abi::emit_push_reg(ctx.emitter, object_reg);
    abi::emit_pop_reg(ctx.emitter, result_reg);
    Ok(())
}

/// Allocates an indexed array of populated ReflectionMethod/ReflectionProperty objects.
fn emit_reflection_member_array(
    ctx: &mut FunctionContext<'_>,
    member_class_name: &str,
    members: &[ReflectionListedMember],
) -> Result<()> {
    emit_reflection_indexed_array(ctx, members.len().max(1), 8);
    crate::codegen::emit_array_value_type_stamp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        &PhpType::Object(member_class_name.to_string()),
    );

    for member in members {
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        emit_reflection_member_object(ctx, member_class_name, member)?;
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        emit_append_reflection_member_object(ctx);
    }

    Ok(())
}

/// Allocates an indexed array of populated ReflectionParameter objects.
fn emit_reflection_parameter_array(
    ctx: &mut FunctionContext<'_>,
    parameters: &[ReflectionParameterMember],
) -> Result<()> {
    emit_reflection_indexed_array(ctx, parameters.len().max(1), 8);
    crate::codegen::emit_array_value_type_stamp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        &PhpType::Object("ReflectionParameter".to_string()),
    );

    for parameter in parameters {
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        emit_reflection_parameter_object(ctx, parameter)?;
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        emit_append_reflection_member_object(ctx);
    }

    Ok(())
}

/// Allocates and populates the associative ReflectionClass constant map.
fn emit_reflection_constant_array(
    ctx: &mut FunctionContext<'_>,
    members: &[ReflectionConstantMember],
) -> Result<()> {
    emit_empty_assoc_array_literal_to_result(ctx, &PhpType::Mixed);
    for member in members {
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        emit_reflection_constant_value_as_mixed(ctx, &member.value);
        emit_reflection_constant_hash_insert(ctx, &member.name);
    }
    Ok(())
}

/// Allocates and populates the associative ReflectionClass default-property map.
fn emit_reflection_default_property_array(
    ctx: &mut FunctionContext<'_>,
    members: &[ReflectionDefaultPropertyMember],
) {
    emit_empty_assoc_array_literal_to_result(ctx, &PhpType::Mixed);
    for member in members {
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        emit_reflection_default_value_as_mixed(ctx, &member.value);
        emit_reflection_constant_hash_insert(ctx, &member.name);
    }
}

/// Materializes one Reflection constant value as a boxed Mixed cell.
fn emit_reflection_constant_value_as_mixed(
    ctx: &mut FunctionContext<'_>,
    value: &ReflectionConstantValue,
) {
    match value {
        ReflectionConstantValue::Int(value) => emit_boxed_int_literal_to_result(ctx, *value),
        ReflectionConstantValue::Bool(value) => emit_boxed_bool_literal_to_result(ctx, *value),
        ReflectionConstantValue::Float(value) => emit_boxed_float_literal_to_result(ctx, *value),
        ReflectionConstantValue::Str(value) => {
            emit_boxed_string_literal_default_to_result(ctx, value)
        }
        ReflectionConstantValue::Null => emit_boxed_null_literal_to_result(ctx),
        ReflectionConstantValue::EnumCase {
            enum_name,
            case_name,
        } => {
            let case_label = enum_case_symbol(enum_name, case_name);
            abi::emit_load_symbol_to_reg(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                &case_label,
                0,
            );
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Object(enum_name.clone()));
        }
    }
}

/// Materializes one Reflection default-property value as a boxed Mixed cell.
fn emit_reflection_default_value_as_mixed(
    ctx: &mut FunctionContext<'_>,
    value: &ReflectionParameterDefaultValue,
) {
    match value {
        ReflectionParameterDefaultValue::Int(value) => {
            emit_boxed_int_literal_to_result(ctx, *value)
        }
        ReflectionParameterDefaultValue::Bool(value) => {
            emit_boxed_bool_literal_to_result(ctx, *value)
        }
        ReflectionParameterDefaultValue::Float(value) => {
            emit_boxed_float_literal_to_result(ctx, *value)
        }
        ReflectionParameterDefaultValue::Str(value) => {
            emit_boxed_string_literal_default_to_result(ctx, value)
        }
        ReflectionParameterDefaultValue::Null => emit_boxed_null_literal_to_result(ctx),
    }
}

/// Inserts the current boxed Mixed constant value into the stacked associative array.
fn emit_reflection_constant_hash_insert(ctx: &mut FunctionContext<'_>, key: &str) {
    let (key_label, key_len) = ctx.data.add_string(key.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x3, x0"); // pass the boxed Reflection constant value as the hash payload
            ctx.emitter.instruction("mov x4, xzr"); // boxed Mixed hash payloads do not use the high word
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_symbol_address(ctx.emitter, "x1", &key_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", key_len as i64);
            abi::emit_load_int_immediate(
                ctx.emitter,
                "x5",
                runtime_value_tag(&PhpType::Mixed) as i64,
            );
            abi::emit_call_label(ctx.emitter, "__rt_hash_set");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rcx, rax"); // pass the boxed Reflection constant value as the hash payload
            ctx.emitter.instruction("xor r8, r8"); // boxed Mixed hash payloads do not use the high word
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_symbol_address(ctx.emitter, "rsi", &key_label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", key_len as i64);
            abi::emit_load_int_immediate(
                ctx.emitter,
                "r9",
                runtime_value_tag(&PhpType::Mixed) as i64,
            );
            abi::emit_call_label(ctx.emitter, "__rt_hash_set");
        }
    }
}

/// Allocates and populates one ReflectionMethod/ReflectionProperty object.
fn emit_reflection_member_object(
    ctx: &mut FunctionContext<'_>,
    member_class_name: &str,
    member: &ReflectionListedMember,
) -> Result<()> {
    let (class_id, property_count, uninitialized_marker_offsets) = {
        let class_info = ctx
            .module
            .class_infos
            .get(member_class_name)
            .ok_or_else(|| {
                CodegenIrError::unsupported(format!("unknown class {}", member_class_name))
            })?;
        (
            class_info.class_id,
            class_info.properties.len(),
            super::uninitialized_property_marker_offsets(class_info),
        )
    };
    super::emit_object_allocation(
        ctx,
        class_id,
        property_count,
        false,
        &uninitialized_marker_offsets,
    )?;
    let class_info = ctx
        .module
        .class_infos
        .get(member_class_name)
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let name_offset = reflection_property_offset(class_info, "__name")?;
    emit_reflection_string_property(ctx, &member.name, name_offset, name_offset + 8);
    emit_reflection_attrs_property(
        ctx,
        member_class_name,
        &member.attr_names,
        &member.attr_args,
    )?;
    emit_reflection_declaring_class_property(
        ctx,
        member_class_name,
        member.declaring_class_name.as_deref(),
    )?;
    if member_class_name == "ReflectionMethod" {
        emit_reflection_parameter_array_property_by_name(
            ctx,
            member_class_name,
            "__parameters",
            &member.parameters,
        )?;
        emit_reflection_owner_int_property(
            ctx,
            member_class_name,
            "__required_parameter_count",
            member.required_parameter_count,
        )?;
        emit_reflection_owner_int_property(
            ctx,
            member_class_name,
            "__modifiers",
            member.modifiers,
        )?;
    }
    if member_class_name == "ReflectionProperty" {
        emit_reflection_owner_int_property(
            ctx,
            member_class_name,
            "__modifiers",
            member.modifiers,
        )?;
        emit_reflection_owner_type_property(ctx, member_class_name, member.type_metadata.as_ref())?;
        emit_reflection_owner_bool_property(
            ctx,
            member_class_name,
            "__has_default_value",
            member.default_value.is_some(),
        )?;
        emit_reflection_owner_default_value_property(
            ctx,
            member_class_name,
            member.default_value.as_ref(),
        )?;
    }
    if member_class_name == "ReflectionClassConstant" {
        if let Some(value) = &member.constant_value {
            abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
            emit_reflection_constant_value_as_mixed(ctx, value);
            emit_reflection_owner_mixed_property_from_result(ctx, member_class_name, "__value")?;
        }
        emit_reflection_owner_bool_property(
            ctx,
            member_class_name,
            "__is_enum_case",
            member.is_enum_case,
        )?;
        emit_reflection_owner_int_property(
            ctx,
            member_class_name,
            "__modifiers",
            member.modifiers,
        )?;
    }
    emit_reflection_member_flag_properties(ctx, member_class_name, member.flags)?;
    Ok(())
}

/// Allocates and populates one ReflectionParameter object.
fn emit_reflection_parameter_object(
    ctx: &mut FunctionContext<'_>,
    parameter: &ReflectionParameterMember,
) -> Result<()> {
    let (class_id, property_count, uninitialized_marker_offsets) = {
        let class_info = ctx
            .module
            .class_infos
            .get("ReflectionParameter")
            .ok_or_else(|| CodegenIrError::unsupported("unknown class ReflectionParameter"))?;
        (
            class_info.class_id,
            class_info.properties.len(),
            super::uninitialized_property_marker_offsets(class_info),
        )
    };
    super::emit_object_allocation(
        ctx,
        class_id,
        property_count,
        false,
        &uninitialized_marker_offsets,
    )?;
    emit_reflection_parameter_properties(ctx, parameter)
}

/// Writes one ReflectionParameter object's private metadata properties.
fn emit_reflection_parameter_properties(
    ctx: &mut FunctionContext<'_>,
    parameter: &ReflectionParameterMember,
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get("ReflectionParameter")
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let name_offset = reflection_property_offset(class_info, "__name")?;
    emit_reflection_string_property(ctx, &parameter.name, name_offset, name_offset + 8);
    emit_reflection_attrs_property(
        ctx,
        "ReflectionParameter",
        &parameter.attr_names,
        &parameter.attr_args,
    )?;
    emit_reflection_owner_int_property(
        ctx,
        "ReflectionParameter",
        "__position",
        parameter.position,
    )?;
    emit_reflection_owner_bool_property(
        ctx,
        "ReflectionParameter",
        "__is_optional",
        parameter.is_optional,
    )?;
    emit_reflection_owner_bool_property(
        ctx,
        "ReflectionParameter",
        "__is_variadic",
        parameter.is_variadic,
    )?;
    emit_reflection_owner_bool_property(
        ctx,
        "ReflectionParameter",
        "__is_passed_by_reference",
        parameter.is_passed_by_reference,
    )?;
    emit_reflection_owner_bool_property(
        ctx,
        "ReflectionParameter",
        "__has_type",
        parameter.has_type,
    )?;
    emit_reflection_parameter_type_property(ctx, parameter)?;
    emit_reflection_owner_bool_property(
        ctx,
        "ReflectionParameter",
        "__has_default_value",
        parameter.default_value.is_some(),
    )?;
    emit_reflection_parameter_default_property(ctx, parameter)?;
    emit_reflection_parameter_declaring_class_property(ctx, parameter)?;
    emit_reflection_parameter_declaring_function_property(ctx, parameter)?;
    Ok(())
}

/// Writes one ReflectionParameter object's declaring-function slot.
fn emit_reflection_parameter_declaring_function_property(
    ctx: &mut FunctionContext<'_>,
    parameter: &ReflectionParameterMember,
) -> Result<()> {
    let declaring_function_offset = {
        let class_info = ctx
            .module
            .class_infos
            .get("ReflectionParameter")
            .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
        reflection_property_offset(class_info, "__declaring_function")?
    };
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    match parameter.declaring_function.as_ref() {
        Some(ReflectionDeclaringFunctionMember::Function {
            name,
            attr_names,
            attr_args,
            required_parameter_count,
        }) => {
            let mut metadata = empty_reflection_metadata();
            metadata.reflected_name = Some(name.clone());
            metadata.attr_names = attr_names.clone();
            metadata.attr_args = attr_args.clone();
            metadata.required_parameter_count = *required_parameter_count;
            emit_reflection_owner_object(ctx, "ReflectionFunction", &metadata)?;
            emit_box_current_value_as_mixed(
                ctx.emitter,
                &PhpType::Object("ReflectionFunction".to_string()),
            );
        }
        Some(ReflectionDeclaringFunctionMember::Method {
            name,
            declaring_class_name,
            attr_names,
            attr_args,
            flags,
            required_parameter_count,
        }) => {
            let mut metadata = empty_reflection_metadata();
            metadata.reflected_name = Some(name.clone());
            metadata.parent_class_name = declaring_class_name.clone();
            metadata.attr_names = attr_names.clone();
            metadata.attr_args = attr_args.clone();
            metadata.member_flags = *flags;
            metadata.required_parameter_count = *required_parameter_count;
            emit_reflection_owner_object(ctx, "ReflectionMethod", &metadata)?;
            emit_box_current_value_as_mixed(
                ctx.emitter,
                &PhpType::Object("ReflectionMethod".to_string()),
            );
        }
        None => emit_boxed_null_literal_to_result(ctx),
    }
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(
        ctx.emitter,
        result_reg,
        object_reg,
        declaring_function_offset,
    );
    abi::emit_store_zero_to_address(ctx.emitter, object_reg, declaring_function_offset + 8);
    abi::emit_reg_move(ctx.emitter, result_reg, object_reg);
    Ok(())
}

/// Writes one ReflectionParameter object's nullable declaring-class slot.
fn emit_reflection_parameter_declaring_class_property(
    ctx: &mut FunctionContext<'_>,
    parameter: &ReflectionParameterMember,
) -> Result<()> {
    let declaring_class_offset = {
        let class_info = ctx
            .module
            .class_infos
            .get("ReflectionParameter")
            .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
        reflection_property_offset(class_info, "__declaring_class")?
    };
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    if let Some(declaring_class_name) = parameter.declaring_class_name.as_deref() {
        let declaring_metadata =
            reflection_shallow_class_metadata_for_name(ctx, declaring_class_name)?;
        emit_reflection_owner_object(ctx, "ReflectionClass", &declaring_metadata)?;
        emit_box_current_value_as_mixed(
            ctx.emitter,
            &PhpType::Object("ReflectionClass".to_string()),
        );
    } else {
        emit_boxed_null_literal_to_result(ctx);
    }
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, object_reg, declaring_class_offset);
    abi::emit_store_zero_to_address(ctx.emitter, object_reg, declaring_class_offset + 8);
    abi::emit_reg_move(ctx.emitter, result_reg, object_reg);
    Ok(())
}

/// Writes one ReflectionParameter object's nullable `ReflectionNamedType` slot.
fn emit_reflection_parameter_type_property(
    ctx: &mut FunctionContext<'_>,
    parameter: &ReflectionParameterMember,
) -> Result<()> {
    emit_reflection_owner_type_property(
        ctx,
        "ReflectionParameter",
        parameter.type_metadata.as_ref(),
    )
}

/// Writes one reflection owner's nullable type slot.
fn emit_reflection_owner_type_property(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    type_metadata: Option<&ReflectionParameterTypeMetadata>,
) -> Result<()> {
    let type_offset = {
        let class_info = ctx
            .module
            .class_infos
            .get(class_name)
            .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
        reflection_property_offset(class_info, "__type")?
    };
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    match type_metadata {
        Some(ReflectionParameterTypeMetadata::Named(type_metadata)) => {
            emit_reflection_named_type_object(ctx, type_metadata)?;
            emit_box_current_value_as_mixed(
                ctx.emitter,
                &PhpType::Object("ReflectionNamedType".to_string()),
            );
        }
        Some(ReflectionParameterTypeMetadata::Union(type_metadata)) => {
            emit_reflection_union_type_object(ctx, type_metadata)?;
            emit_box_current_value_as_mixed(
                ctx.emitter,
                &PhpType::Object("ReflectionUnionType".to_string()),
            );
        }
        Some(ReflectionParameterTypeMetadata::Intersection(type_metadata)) => {
            emit_reflection_intersection_type_object(ctx, type_metadata)?;
            emit_box_current_value_as_mixed(
                ctx.emitter,
                &PhpType::Object("ReflectionIntersectionType".to_string()),
            );
        }
        None => emit_boxed_null_literal_to_result(ctx),
    }
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, object_reg, type_offset);
    abi::emit_store_zero_to_address(ctx.emitter, object_reg, type_offset + 8);
    abi::emit_reg_move(ctx.emitter, result_reg, object_reg);
    Ok(())
}

/// Writes one ReflectionParameter object's boxed default-value slot.
fn emit_reflection_parameter_default_property(
    ctx: &mut FunctionContext<'_>,
    parameter: &ReflectionParameterMember,
) -> Result<()> {
    emit_reflection_owner_default_value_property(
        ctx,
        "ReflectionParameter",
        parameter.default_value.as_ref(),
    )
}

/// Writes one reflection owner's boxed default-value slot.
fn emit_reflection_owner_default_value_property(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    default_value: Option<&ReflectionParameterDefaultValue>,
) -> Result<()> {
    let default_offset = {
        let class_info = ctx
            .module
            .class_infos
            .get(class_name)
            .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
        reflection_property_offset(class_info, "__default_value")?
    };
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    match default_value {
        Some(ReflectionParameterDefaultValue::Int(value)) => {
            emit_boxed_int_literal_to_result(ctx, *value)
        }
        Some(ReflectionParameterDefaultValue::Bool(value)) => {
            emit_boxed_bool_literal_to_result(ctx, *value)
        }
        Some(ReflectionParameterDefaultValue::Float(value)) => {
            emit_boxed_float_literal_to_result(ctx, *value)
        }
        Some(ReflectionParameterDefaultValue::Str(value)) => {
            emit_boxed_string_literal_default_to_result(ctx, value)
        }
        Some(ReflectionParameterDefaultValue::Null) | None => {
            emit_boxed_null_literal_to_result(ctx)
        }
    }
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, object_reg, default_offset);
    abi::emit_store_zero_to_address(ctx.emitter, object_reg, default_offset + 8);
    abi::emit_reg_move(ctx.emitter, result_reg, object_reg);
    Ok(())
}

/// Allocates and populates one `ReflectionUnionType` object.
fn emit_reflection_union_type_object(
    ctx: &mut FunctionContext<'_>,
    type_metadata: &ReflectionUnionTypeMetadata,
) -> Result<()> {
    let (class_id, property_count, uninitialized_marker_offsets) = {
        let class_info = ctx
            .module
            .class_infos
            .get("ReflectionUnionType")
            .ok_or_else(|| CodegenIrError::unsupported("unknown class ReflectionUnionType"))?;
        (
            class_info.class_id,
            class_info.properties.len(),
            super::uninitialized_property_marker_offsets(class_info),
        )
    };
    super::emit_object_allocation(
        ctx,
        class_id,
        property_count,
        false,
        &uninitialized_marker_offsets,
    )?;
    emit_reflection_union_type_types_property(ctx, &type_metadata.types)?;
    emit_reflection_owner_bool_property(
        ctx,
        "ReflectionUnionType",
        "__allows_null",
        type_metadata.allows_null,
    )?;
    Ok(())
}

/// Writes the `ReflectionUnionType::__types` array of `ReflectionNamedType` objects.
fn emit_reflection_union_type_types_property(
    ctx: &mut FunctionContext<'_>,
    types: &[ReflectionNamedTypeMetadata],
) -> Result<()> {
    let types_offset = {
        let class_info = ctx
            .module
            .class_infos
            .get("ReflectionUnionType")
            .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
        reflection_property_offset(class_info, "__types")?
    };
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    emit_reflection_named_type_array(ctx, types)?;
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, object_reg, types_offset);
    abi::emit_load_int_immediate(ctx.emitter, abi::secondary_scratch_reg(ctx.emitter), 4);
    abi::emit_store_to_address(
        ctx.emitter,
        abi::secondary_scratch_reg(ctx.emitter),
        object_reg,
        types_offset + 8,
    );
    abi::emit_reg_move(ctx.emitter, result_reg, object_reg);
    Ok(())
}

/// Allocates and populates one `ReflectionIntersectionType` object.
fn emit_reflection_intersection_type_object(
    ctx: &mut FunctionContext<'_>,
    type_metadata: &ReflectionIntersectionTypeMetadata,
) -> Result<()> {
    let (class_id, property_count, uninitialized_marker_offsets) = {
        let class_info = ctx
            .module
            .class_infos
            .get("ReflectionIntersectionType")
            .ok_or_else(|| {
                CodegenIrError::unsupported("unknown class ReflectionIntersectionType")
            })?;
        (
            class_info.class_id,
            class_info.properties.len(),
            super::uninitialized_property_marker_offsets(class_info),
        )
    };
    super::emit_object_allocation(
        ctx,
        class_id,
        property_count,
        false,
        &uninitialized_marker_offsets,
    )?;
    emit_reflection_intersection_type_types_property(ctx, &type_metadata.types)?;
    emit_reflection_owner_bool_property(ctx, "ReflectionIntersectionType", "__allows_null", false)?;
    Ok(())
}

/// Writes the `ReflectionIntersectionType::__types` array of `ReflectionNamedType` objects.
fn emit_reflection_intersection_type_types_property(
    ctx: &mut FunctionContext<'_>,
    types: &[ReflectionNamedTypeMetadata],
) -> Result<()> {
    let types_offset = {
        let class_info = ctx
            .module
            .class_infos
            .get("ReflectionIntersectionType")
            .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
        reflection_property_offset(class_info, "__types")?
    };
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    emit_reflection_named_type_array(ctx, types)?;
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, object_reg, types_offset);
    abi::emit_load_int_immediate(ctx.emitter, abi::secondary_scratch_reg(ctx.emitter), 4);
    abi::emit_store_to_address(
        ctx.emitter,
        abi::secondary_scratch_reg(ctx.emitter),
        object_reg,
        types_offset + 8,
    );
    abi::emit_reg_move(ctx.emitter, result_reg, object_reg);
    Ok(())
}

/// Allocates an indexed array of populated `ReflectionNamedType` objects.
fn emit_reflection_named_type_array(
    ctx: &mut FunctionContext<'_>,
    types: &[ReflectionNamedTypeMetadata],
) -> Result<()> {
    emit_reflection_indexed_array(ctx, types.len().max(1), 8);
    crate::codegen::emit_array_value_type_stamp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        &PhpType::Object("ReflectionNamedType".to_string()),
    );
    for type_metadata in types {
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        emit_reflection_named_type_object(ctx, type_metadata)?;
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        emit_append_reflection_member_object(ctx);
    }
    Ok(())
}

/// Allocates and populates one `ReflectionNamedType` object.
fn emit_reflection_named_type_object(
    ctx: &mut FunctionContext<'_>,
    type_metadata: &ReflectionNamedTypeMetadata,
) -> Result<()> {
    let (class_id, property_count, uninitialized_marker_offsets, name_offset) = {
        let class_info = ctx
            .module
            .class_infos
            .get("ReflectionNamedType")
            .ok_or_else(|| CodegenIrError::unsupported("unknown class ReflectionNamedType"))?;
        (
            class_info.class_id,
            class_info.properties.len(),
            super::uninitialized_property_marker_offsets(class_info),
            reflection_property_offset(class_info, "__name")?,
        )
    };
    super::emit_object_allocation(
        ctx,
        class_id,
        property_count,
        false,
        &uninitialized_marker_offsets,
    )?;
    emit_reflection_string_property(ctx, &type_metadata.name, name_offset, name_offset + 8);
    emit_reflection_owner_bool_property(
        ctx,
        "ReflectionNamedType",
        "__allows_null",
        type_metadata.allows_null,
    )?;
    emit_reflection_owner_bool_property(
        ctx,
        "ReflectionNamedType",
        "__is_builtin",
        type_metadata.is_builtin,
    )?;
    Ok(())
}

/// Allocates an indexed array for static reflection metadata.
fn emit_reflection_indexed_array(ctx: &mut FunctionContext<'_>, capacity: usize, stride: i64) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "x1", stride);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", stride);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
}

/// Appends the stacked member object to the stacked member array and leaves the array in result.
fn emit_append_reflection_member_object(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(ctx.emitter, "x1");
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(ctx.emitter, "rsi");
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        }
    }
}

/// Allocates an indexed string array containing ReflectionClass metadata names.
fn emit_reflection_string_array(ctx: &mut FunctionContext<'_>, names: &[String]) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", names.len().max(1) as i64);
            abi::emit_load_int_immediate(ctx.emitter, "x1", 16);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", names.len().max(1) as i64);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", 16);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_reflection_string_array_fill_aarch64(ctx, names),
        Arch::X86_64 => emit_reflection_string_array_fill_x86_64(ctx, names),
    }
    Ok(())
}

/// Appends ReflectionClass metadata names to the current ARM64 result array.
fn emit_reflection_string_array_fill_aarch64(ctx: &mut FunctionContext<'_>, names: &[String]) {
    ctx.emitter.instruction("str x0, [sp, #-16]!"); // park the metadata-name array while appending strings
    for name in names {
        let (label, len) = ctx.data.add_string(name.as_bytes());
        ctx.emitter.instruction("ldr x0, [sp]"); // reload the metadata-name array for this append
        abi::emit_symbol_address(ctx.emitter, "x1", &label);
        abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
        abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
        ctx.emitter.instruction("str x0, [sp]"); // preserve the possibly-grown metadata-name array
    }
    ctx.emitter.instruction("ldr x0, [sp], #16"); // restore the final metadata-name array as the result
}

/// Appends ReflectionClass metadata names to the current x86_64 result array.
fn emit_reflection_string_array_fill_x86_64(ctx: &mut FunctionContext<'_>, names: &[String]) {
    ctx.emitter.instruction("push rax"); // park the metadata-name array while appending strings
    ctx.emitter.instruction("sub rsp, 8"); // keep stack alignment stable across append helper calls
    for name in names {
        let (label, len) = ctx.data.add_string(name.as_bytes());
        ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 8]"); // reload the metadata-name array for this append
        abi::emit_symbol_address(ctx.emitter, "rsi", &label);
        abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
        abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
        ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax"); // preserve the possibly-grown metadata-name array
    }
    ctx.emitter.instruction("add rsp, 8"); // drop the temporary alignment slot
    ctx.emitter.instruction("pop rax"); // restore the final metadata-name array as the result
}

/// Stores ReflectionMethod/ReflectionProperty boolean predicate slots when supported.
fn emit_reflection_member_flag_properties(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    flags: ReflectionMemberFlags,
) -> Result<()> {
    match class_name {
        "ReflectionMethod" => {
            emit_reflection_owner_bool_property(ctx, class_name, "__is_static", flags.is_static)?;
            emit_reflection_owner_bool_property(ctx, class_name, "__is_public", flags.is_public)?;
            emit_reflection_owner_bool_property(
                ctx,
                class_name,
                "__is_protected",
                flags.is_protected,
            )?;
            emit_reflection_owner_bool_property(ctx, class_name, "__is_private", flags.is_private)?;
            emit_reflection_owner_bool_property(ctx, class_name, "__is_final", flags.is_final)?;
            emit_reflection_owner_bool_property(
                ctx,
                class_name,
                "__is_abstract",
                flags.is_abstract,
            )?;
        }
        "ReflectionProperty" => {
            emit_reflection_owner_bool_property(ctx, class_name, "__is_static", flags.is_static)?;
            emit_reflection_owner_bool_property(ctx, class_name, "__is_public", flags.is_public)?;
            emit_reflection_owner_bool_property(
                ctx,
                class_name,
                "__is_protected",
                flags.is_protected,
            )?;
            emit_reflection_owner_bool_property(ctx, class_name, "__is_private", flags.is_private)?;
            emit_reflection_owner_bool_property(ctx, class_name, "__is_final", flags.is_final)?;
            emit_reflection_owner_bool_property(
                ctx,
                class_name,
                "__is_abstract",
                flags.is_abstract,
            )?;
            emit_reflection_owner_bool_property(
                ctx,
                class_name,
                "__is_readonly",
                flags.is_readonly,
            )?;
        }
        "ReflectionClassConstant" => {
            emit_reflection_owner_bool_property(ctx, class_name, "__is_public", flags.is_public)?;
            emit_reflection_owner_bool_property(
                ctx,
                class_name,
                "__is_protected",
                flags.is_protected,
            )?;
            emit_reflection_owner_bool_property(ctx, class_name, "__is_private", flags.is_private)?;
            emit_reflection_owner_bool_property(ctx, class_name, "__is_final", flags.is_final)?;
        }
        _ => {}
    }
    Ok(())
}

/// Stores one boolean property on the current Reflection owner object result.
fn emit_reflection_owner_bool_property(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    property_name: &str,
    value: bool,
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get(class_name)
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let low_offset = reflection_property_offset(class_info, property_name)?;
    let high_offset = low_offset + 8;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let value_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, value_reg, i64::from(value));
    abi::emit_store_to_address(ctx.emitter, value_reg, result_reg, low_offset);
    abi::emit_store_zero_to_address(ctx.emitter, result_reg, high_offset);
    Ok(())
}

/// Stores one boolean property on the current ReflectionClass object result.
fn emit_reflection_bool_property(
    ctx: &mut FunctionContext<'_>,
    property_name: &str,
    value: bool,
) -> Result<()> {
    emit_reflection_owner_bool_property(ctx, "ReflectionClass", property_name, value)
}

/// Stores one integer property on the current ReflectionClass object result.
fn emit_reflection_int_property(
    ctx: &mut FunctionContext<'_>,
    property_name: &str,
    value: i64,
) -> Result<()> {
    emit_reflection_owner_int_property(ctx, "ReflectionClass", property_name, value)
}

/// Stores one integer property on the current Reflection owner object result.
fn emit_reflection_owner_int_property(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    property_name: &str,
    value: i64,
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get(class_name)
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let low_offset = reflection_property_offset(class_info, property_name)?;
    let high_offset = low_offset + 8;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let value_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, value_reg, value);
    abi::emit_store_to_address(ctx.emitter, value_reg, result_reg, low_offset);
    abi::emit_store_zero_to_address(ctx.emitter, result_reg, high_offset);
    Ok(())
}

/// Stores the current boxed Mixed result into one Reflection owner property.
fn emit_reflection_owner_mixed_property_from_result(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    property_name: &str,
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get(class_name)
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let low_offset = reflection_property_offset(class_info, property_name)?;
    let high_offset = low_offset + 8;
    let value_reg = abi::int_result_reg(ctx.emitter);
    let owner_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_pop_reg(ctx.emitter, owner_reg);
    abi::emit_store_to_address(ctx.emitter, value_reg, owner_reg, low_offset);
    abi::emit_store_zero_to_address(ctx.emitter, owner_reg, high_offset);
    abi::emit_reg_move(ctx.emitter, value_reg, owner_reg);
    Ok(())
}

/// Computes PHP's `ReflectionClass::getModifiers()` bitmask for class metadata.
fn reflection_class_modifiers(
    is_final: bool,
    is_abstract: bool,
    is_readonly_class: bool,
    is_enum: bool,
) -> i64 {
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

/// Computes PHP's `ReflectionClassConstant::getModifiers()` bitmask.
fn reflection_class_constant_modifiers(visibility: &Visibility, is_final: bool) -> i64 {
    let mut modifiers = match visibility {
        Visibility::Public => 1,
        Visibility::Protected => 2,
        Visibility::Private => 4,
    };
    if is_final {
        modifiers |= 32;
    }
    modifiers
}

/// Computes PHP's `ReflectionProperty::getModifiers()` bitmask from class metadata.
fn reflection_property_modifiers_for_info(
    info: &crate::types::ClassInfo,
    property_name: &str,
) -> Option<i64> {
    if info
        .properties
        .iter()
        .any(|(name, _)| name == property_name)
    {
        let visibility = info
            .property_visibilities
            .get(property_name)
            .unwrap_or(&Visibility::Public);
        return Some(reflection_property_modifiers(
            visibility,
            false,
            info.final_properties.contains(property_name),
            info.abstract_properties.contains(property_name),
            info.readonly_properties.contains(property_name),
            reflection_property_is_virtual(info, property_name),
            info.property_set_visibilities.get(property_name),
        ));
    }
    if info
        .static_properties
        .iter()
        .any(|(name, _)| name == property_name)
    {
        let visibility = info
            .static_property_visibilities
            .get(property_name)
            .unwrap_or(&Visibility::Public);
        return Some(reflection_property_modifiers(
            visibility,
            true,
            info.final_static_properties.contains(property_name),
            false,
            false,
            false,
            None,
        ));
    }
    None
}

/// Returns whether a property is virtual because it has or requires hooks.
fn reflection_property_is_virtual(info: &crate::types::ClassInfo, property_name: &str) -> bool {
    let get_method = php_symbol_key(&property_hook_get_method(property_name));
    let set_method = php_symbol_key(&property_hook_set_method(property_name));
    info.abstract_property_hooks.contains_key(property_name)
        || info.methods.contains_key(&get_method)
        || info.methods.contains_key(&set_method)
}

/// Computes PHP's `ReflectionProperty::getModifiers()` bitmask.
fn reflection_property_modifiers(
    visibility: &Visibility,
    is_static: bool,
    is_final: bool,
    is_abstract: bool,
    is_readonly: bool,
    is_virtual: bool,
    set_visibility: Option<&Visibility>,
) -> i64 {
    let mut modifiers = match visibility {
        Visibility::Public => 1,
        Visibility::Protected => 2,
        Visibility::Private => 4,
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
        Some(Visibility::Private) => modifiers |= 32 | 4096,
        Some(Visibility::Protected) => modifiers |= 2048,
        Some(Visibility::Public) | None => {
            if is_readonly && visibility == &Visibility::Public {
                modifiers |= 2048;
            }
        }
    }
    modifiers
}

/// Computes PHP's `ReflectionProperty::getModifiers()` bitmask from predicate flags.
fn reflection_property_modifiers_from_flags(flags: ReflectionMemberFlags) -> i64 {
    let visibility = if flags.is_private {
        Visibility::Private
    } else if flags.is_protected {
        Visibility::Protected
    } else {
        Visibility::Public
    };
    reflection_property_modifiers(
        &visibility,
        flags.is_static,
        flags.is_final,
        flags.is_abstract,
        flags.is_readonly,
        false,
        None,
    )
}

/// Computes PHP's `ReflectionMethod::getModifiers()` bitmask from method flags.
fn reflection_method_modifiers_from_flags(flags: ReflectionMemberFlags) -> i64 {
    let mut modifiers = 0;
    if flags.is_public {
        modifiers |= 1;
    }
    if flags.is_protected {
        modifiers |= 2;
    }
    if flags.is_private {
        modifiers |= 4;
    }
    if flags.is_static {
        modifiers |= 16;
    }
    if flags.is_final {
        modifiers |= 32;
    }
    if flags.is_abstract {
        modifiers |= 64;
    }
    modifiers
}

/// Returns one declared property offset from a synthetic Reflection class layout.
fn reflection_property_offset(info: &crate::types::ClassInfo, property: &str) -> Result<usize> {
    info.property_offsets.get(property).copied().ok_or_else(|| {
        CodegenIrError::invalid_module(format!(
            "Reflection owner missing property offset for ${}",
            property
        ))
    })
}

/// Returns the low/high object offsets for the private `__attrs` slot.
fn reflection_attrs_offsets(ctx: &FunctionContext<'_>, class_name: &str) -> Result<(usize, usize)> {
    let class_info = ctx
        .module
        .class_infos
        .get(class_name)
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let attrs_low_offset = reflection_property_offset(class_info, "__attrs")?;
    Ok((attrs_low_offset, attrs_low_offset + 8))
}
