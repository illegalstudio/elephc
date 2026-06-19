//! Purpose:
//! Lowers metadata-aware allocation for builtin Reflection owner objects in the
//! EIR backend.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::lower_object_new()`.
//!
//! Key details:
//! - `ReflectionClass`, `ReflectionFunction`, `ReflectionMethod`,
//!   `ReflectionProperty`, `ReflectionClassConstant`, and `ReflectionEnum*`
//!   constructors are compile-time metadata lookups that populate private
//!   metadata slots instead of running their public empty bodies.

use crate::codegen::platform::Arch;
use crate::codegen::literal_defaults::{
    emit_boxed_bool_literal_to_result, emit_boxed_float_literal_to_result,
    emit_boxed_int_literal_to_result, emit_boxed_null_literal_to_result,
    emit_boxed_string_literal_default_to_result, emit_empty_assoc_array_literal_to_result,
};
use crate::codegen::{
    abi, emit_box_current_value_as_mixed, runtime_value_tag, CodegenIrError, Result,
};
use crate::ir::{Immediate, Instruction, Op, TraitMethodInfo, ValueDef, ValueId};
use crate::names::{enum_case_symbol, php_symbol_key};
use crate::parser::ast::{BinOp, Expr, ExprKind, StaticReceiver, Visibility};
use crate::types::{AttrArgEntry, FunctionSig, InterfaceInfo, PhpType};

use super::super::super::context::FunctionContext;

/// Compile-time metadata used to populate one Reflection owner object.
struct ReflectionOwnerMetadata {
    reflected_name: Option<String>,
    attr_names: Vec<String>,
    attr_args: Vec<Option<Vec<AttrArgEntry>>>,
    interface_names: Vec<String>,
    trait_names: Vec<String>,
    method_names: Vec<String>,
    property_names: Vec<String>,
    constant_names: Vec<String>,
    constant_members: Vec<ReflectionConstantMember>,
    constant_reflection_members: Vec<ReflectionListedMember>,
    method_members: Vec<ReflectionListedMember>,
    property_members: Vec<ReflectionListedMember>,
    constructor_member: Option<ReflectionListedMember>,
    parent_class_name: Option<String>,
    parameter_members: Vec<ReflectionParameterMember>,
    required_parameter_count: i64,
    is_final: bool,
    is_abstract: bool,
    is_interface: bool,
    is_trait: bool,
    is_enum: bool,
    is_readonly: bool,
    is_instantiable: bool,
    modifiers: i64,
    member_flags: ReflectionMemberFlags,
}

/// Metadata for one member object returned by `ReflectionClass::getMethods()` or `getProperties()`.
#[derive(Clone)]
struct ReflectionListedMember {
    name: String,
    attr_names: Vec<String>,
    attr_args: Vec<Option<Vec<AttrArgEntry>>>,
    flags: ReflectionMemberFlags,
    required_parameter_count: i64,
    parameters: Vec<ReflectionParameterMember>,
}

/// Metadata for one object returned by `ReflectionMethod::getParameters()`.
#[derive(Clone)]
struct ReflectionParameterMember {
    name: String,
    position: i64,
    is_optional: bool,
    is_variadic: bool,
    is_passed_by_reference: bool,
    has_type: bool,
}

/// Metadata for one constant entry returned by `ReflectionClass::getConstants()`.
#[derive(Clone)]
struct ReflectionConstantMember {
    name: String,
    value: ReflectionConstantValue,
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
        let class_info = ctx
            .module
            .class_infos
            .get(class_name)
            .ok_or_else(|| CodegenIrError::unsupported(format!("unknown class {}", class_name)))?;
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
        &[],
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
        emit_reflection_bool_property(ctx, "__is_instantiable", metadata.is_instantiable)?;
        emit_reflection_int_property_by_name(ctx, "__modifiers", metadata.modifiers)?;
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
    if class_name == "ReflectionParameter" {
        if let Some(parameter) = metadata.parameter_members.first() {
            emit_reflection_parameter_properties(ctx, parameter)?;
        }
    }
    emit_reflection_member_flag_properties(ctx, class_name, metadata.member_flags)?;
    Ok(())
}

/// Lowers `new ReflectionFunction("name")` by populating its name and
/// parameter-count slots from the reflected function's signature. The slot
/// layout is `__name` (8/16), `__short` (24/32), `__num_params` (40/48),
/// `__num_required` (56/64).
pub(super) fn lower_reflection_function_new(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let (full_name, short_name, num_params, num_required) = reflection_function_metadata(ctx, inst)?;
    let (class_id, property_count, uninitialized_marker_offsets, name_off, short_off, np_off, nr_off) = {
        let class_info = ctx
            .module
            .class_infos
            .get("ReflectionFunction")
            .ok_or_else(|| CodegenIrError::unsupported("unknown class ReflectionFunction"))?;
        let slot = |name: &str| -> Result<usize> {
            class_info
                .property_offsets
                .get(name)
                .copied()
                .ok_or_else(|| CodegenIrError::missing_entry("property offset", 0))
        };
        (
            class_info.class_id,
            class_info.properties.len(),
            super::uninitialized_property_marker_offsets(class_info),
            slot("__name")?,
            slot("__short")?,
            slot("__num_params")?,
            slot("__num_required")?,
        )
    };
    super::emit_object_allocation(
        ctx,
        class_id,
        property_count,
        false,
        &uninitialized_marker_offsets,
        &[],
    )?;
    emit_reflection_string_property(ctx, &full_name, name_off, name_off + 8);
    emit_reflection_string_property(ctx, &short_name, short_off, short_off + 8);
    emit_reflection_int_property(ctx, num_params, np_off, np_off + 8);
    emit_reflection_int_property(ctx, num_required, nr_off, nr_off + 8);

    // Build the `ReflectionParameter[]` array and store it into `__params`.
    let params_off = ctx
        .module
        .class_infos
        .get("ReflectionFunction")
        .and_then(|ci| ci.property_offsets.get("__params").copied())
        .ok_or_else(|| CodegenIrError::missing_entry("property offset", 0))?;
    let param_infos = reflection_function_param_infos(ctx, &full_name);
    let (rp_class_id, rp_prop_count, rp_markers, rp_name, rp_pos, rp_opt, rp_var, rp_type, rp_has_type) = {
        let ci = ctx
            .module
            .class_infos
            .get("ReflectionParameter")
            .ok_or_else(|| CodegenIrError::unsupported("unknown class ReflectionParameter"))?;
        let slot = |n: &str| -> Result<usize> {
            ci.property_offsets
                .get(n)
                .copied()
                .ok_or_else(|| CodegenIrError::missing_entry("property offset", 0))
        };
        (
            ci.class_id,
            ci.properties.len(),
            super::uninitialized_property_marker_offsets(ci),
            slot("__name")?,
            slot("__position")?,
            slot("__optional")?,
            slot("__variadic")?,
            slot("__type")?,
            slot("__has_type")?,
        )
    };
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_reg, 0);
    abi::emit_load_from_address(ctx.emitter, result_reg, object_reg, params_off);
    abi::emit_call_label(ctx.emitter, "__rt_decref_array");
    emit_reflection_parameter_array(
        ctx,
        &param_infos,
        rp_class_id,
        rp_prop_count,
        &rp_markers,
        rp_name,
        rp_pos,
        rp_opt,
        rp_var,
        rp_type,
        rp_has_type,
    )?;
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, object_reg, params_off);
    abi::emit_load_int_immediate(ctx.emitter, abi::secondary_scratch_reg(ctx.emitter), 4);
    abi::emit_store_to_address(
        ctx.emitter,
        abi::secondary_scratch_reg(ctx.emitter),
        object_reg,
        params_off + 8,
    );
    abi::emit_push_reg(ctx.emitter, object_reg);
    abi::emit_pop_reg(ctx.emitter, result_reg);

    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("reflection object_new missing result"))?;
    ctx.store_result_value(result)
}

/// Resolves `ReflectionFunction(name)` to its full name, short name, and
/// parameter counts from the reflected function's lowered signature.
fn reflection_function_metadata(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<(String, String, i64, i64)> {
    let Some(name_operand) = inst.operands.first().copied() else {
        return Ok((String::new(), String::new(), 0, 0));
    };
    let function_name = const_required_string_operand(ctx, name_operand, "ReflectionFunction")?;
    let key = php_symbol_key(function_name.trim_start_matches('\\'));
    let signature = ctx
        .module
        .functions
        .iter()
        .find(|function| php_symbol_key(function.name.trim_start_matches('\\')) == key)
        .and_then(|function| function.signature.as_ref());
    let (num_params, num_required) = signature
        .map(|sig| {
            let total = sig.params.len() as i64;
            let required = sig
                .params
                .iter()
                .zip(sig.defaults.iter().chain(std::iter::repeat(&None)))
                .filter(|((name, _), default)| {
                    default.is_none() && sig.variadic.as_deref() != Some(name.as_str())
                })
                .count() as i64;
            (total, required)
        })
        .unwrap_or((0, 0));
    let short_name = function_name
        .trim_start_matches('\\')
        .rsplit('\\')
        .next()
        .unwrap_or(&function_name)
        .to_string();
    Ok((function_name.clone(), short_name, num_params, num_required))
}

/// Stores an integer immediate into a Reflection object's property slot.
fn emit_reflection_int_property(
    ctx: &mut FunctionContext<'_>,
    value: i64,
    low_offset: usize,
    high_offset: usize,
) {
    let object_reg = abi::int_result_reg(ctx.emitter);
    let scratch = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, scratch, value);
    abi::emit_store_to_address(ctx.emitter, scratch, object_reg, low_offset);
    abi::emit_load_int_immediate(ctx.emitter, scratch, 0);
    abi::emit_store_to_address(ctx.emitter, scratch, object_reg, high_offset);
}

/// Per-parameter reflection metadata for one function parameter.
struct ReflectionParamInfo {
    name: String,
    optional: bool,
    variadic: bool,
    /// `Some((type_name, is_builtin, allows_null))` when the parameter declares a
    /// single named type; `None` for an untyped parameter (`getType()` is null).
    type_info: Option<(String, bool, bool)>,
}

/// Maps a declared parameter type to `ReflectionNamedType` metadata
/// `(name, is_builtin, allows_null)`, or `None` for an unsupported/union shape.
fn reflection_named_type_info(ty: &crate::types::PhpType) -> Option<(String, bool, bool)> {
    use crate::types::PhpType;
    match ty {
        PhpType::Int => Some(("int".to_string(), true, false)),
        PhpType::Str => Some(("string".to_string(), true, false)),
        PhpType::Float => Some(("float".to_string(), true, false)),
        PhpType::Bool => Some(("bool".to_string(), true, false)),
        PhpType::Array(_) | PhpType::AssocArray { .. } => Some(("array".to_string(), true, false)),
        PhpType::Callable => Some(("callable".to_string(), true, false)),
        PhpType::Iterable => Some(("iterable".to_string(), true, false)),
        // Bare `Mixed` is how an *untyped* parameter is represented in the EIR
        // signature (and `declared_params` is unreliable here — it is also set
        // for boxed-ABI params). PHP reports untyped parameters as having no
        // type, so map `Mixed` to no named type. An explicit `mixed` hint is
        // the only case this under-reports, which is an accepted edge case.
        PhpType::Object(class) => Some((class.trim_start_matches('\\').to_string(), false, false)),
        PhpType::Union(members) => {
            let has_null = members.iter().any(|m| matches!(m, PhpType::Void));
            let mut non_null = members.iter().filter(|m| !matches!(m, PhpType::Void));
            let single = non_null.next();
            // Only `T|null` (a single non-null member) maps to a named type.
            match (single, non_null.next()) {
                (Some(member), None) => reflection_named_type_info(member)
                    .map(|(name, builtin, _)| (name, builtin, has_null)),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Extracts per-parameter reflection metadata from a function's lowered
/// signature. A parameter is optional once a default or the variadic is seen
/// (matching PHP's `isOptional`).
fn reflection_function_param_infos(
    ctx: &FunctionContext<'_>,
    function_name: &str,
) -> Vec<ReflectionParamInfo> {
    let key = php_symbol_key(function_name.trim_start_matches('\\'));
    let Some(signature) = ctx
        .module
        .functions
        .iter()
        .find(|function| php_symbol_key(function.name.trim_start_matches('\\')) == key)
        .and_then(|function| function.signature.as_ref())
    else {
        return Vec::new();
    };
    let mut seen_optional = false;
    signature
        .params
        .iter()
        .enumerate()
        .map(|(idx, (name, ty))| {
            let variadic = signature.variadic.as_deref() == Some(name.as_str());
            let has_default = signature.defaults.get(idx).map_or(false, Option::is_some);
            if has_default || variadic {
                seen_optional = true;
            }
            let declared = signature.declared_params.get(idx).copied().unwrap_or(false);
            let type_info = if declared {
                reflection_named_type_info(ty)
            } else {
                None
            };
            ReflectionParamInfo {
                name: name.clone(),
                optional: seen_optional,
                variadic,
                type_info,
            }
        })
        .collect()
}

/// Allocates a fresh indexed array sized for `count` object handles (8-byte stride).
fn emit_alloc_object_array(ctx: &mut FunctionContext<'_>, count: usize) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", count.max(1) as i64);
            abi::emit_load_int_immediate(ctx.emitter, "x1", 8);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", count.max(1) as i64);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", 8);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
}

/// Pops a freshly built object and the result array off the stack and appends
/// the object handle to the array (leaving the array pointer in the result reg).
fn emit_append_object_to_array(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(ctx.emitter, "x1");
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(ctx.emitter, "rsi");
            abi::emit_pop_reg(ctx.emitter, "rdi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
}

/// Builds an indexed array of `ReflectionParameter` objects (one per function
/// parameter), leaving the array pointer in the result register. Stack-balanced.
#[allow(clippy::too_many_arguments)]
fn emit_reflection_parameter_array(
    ctx: &mut FunctionContext<'_>,
    params: &[ReflectionParamInfo],
    class_id: u64,
    property_count: usize,
    markers: &[usize],
    name_off: usize,
    pos_off: usize,
    opt_off: usize,
    var_off: usize,
    type_off: usize,
    has_type_off: usize,
) -> Result<()> {
    // ReflectionNamedType layout for building per-parameter type objects.
    let named_type = ctx.module.class_infos.get("ReflectionNamedType").map(|ci| {
        let off = |n: &str| ci.property_offsets.get(n).copied().unwrap_or(0);
        (
            ci.class_id,
            ci.properties.len(),
            super::uninitialized_property_marker_offsets(ci),
            off("__name"),
            off("__allows_null"),
            off("__builtin"),
        )
    });
    emit_alloc_object_array(ctx, params.len());
    crate::codegen::emit_array_value_type_stamp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        &crate::types::PhpType::Object("ReflectionParameter".to_string()),
    );
    for (position, param) in params.iter().enumerate() {
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        super::emit_object_allocation(ctx, class_id, property_count, false, markers, &[])?;
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        emit_reflection_string_property(ctx, &param.name, name_off, name_off + 8);
        emit_reflection_int_property(ctx, position as i64, pos_off, pos_off + 8);
        emit_reflection_int_property(ctx, param.optional as i64, opt_off, opt_off + 8);
        emit_reflection_int_property(ctx, param.variadic as i64, var_off, var_off + 8);
        if let (Some((type_name, builtin, allows_null)), Some((nt_id, nt_count, nt_markers, nt_name, nt_anull, nt_builtin))) =
            (&param.type_info, &named_type)
        {
            // Build a ReflectionNamedType (result reg); the parameter object is
            // safe on the stack at slot 0 across this balanced construction.
            super::emit_object_allocation(ctx, *nt_id, *nt_count, false, nt_markers, &[])?;
            emit_reflection_string_property(ctx, type_name, *nt_name, *nt_name + 8);
            emit_reflection_int_property(ctx, *builtin as i64, *nt_builtin, *nt_builtin + 8);
            emit_reflection_int_property(ctx, *allows_null as i64, *nt_anull, *nt_anull + 8);
            // `__type` is a `mixed` property, so its value must be a *boxed*
            // Mixed cell (the receiver later dispatches `getType()->...` through
            // the Mixed unbox path). Box the freshly built object pointer (still
            // in the result reg) into a cell, then store it as a Mixed slot:
            // boxed-cell pointer in the low word, 0 in the high word. The slot
            // was zero-initialized at allocation, so no decref of an old value
            // is required.
            crate::codegen::emit_box_current_value_as_mixed(
                ctx.emitter,
                &crate::types::PhpType::Object("ReflectionNamedType".to_string()),
            );
            let cell_reg = abi::int_result_reg(ctx.emitter);
            let param_reg = abi::symbol_scratch_reg(ctx.emitter);
            let flag_reg = abi::secondary_scratch_reg(ctx.emitter);
            abi::emit_load_temporary_stack_slot(ctx.emitter, param_reg, 0);
            abi::emit_store_to_address(ctx.emitter, cell_reg, param_reg, type_off);
            abi::emit_store_zero_to_address(ctx.emitter, param_reg, type_off + 8);
            abi::emit_load_int_immediate(ctx.emitter, flag_reg, 1);
            abi::emit_store_to_address(ctx.emitter, flag_reg, param_reg, has_type_off);
            abi::emit_store_zero_to_address(ctx.emitter, param_reg, has_type_off + 8);
        }
        emit_append_object_to_array(ctx);
    }
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
        let constant_reflection_members =
            reflection_class_constant_reflection_members(ctx, class_name, info)?;
        let method_members = reflection_class_method_members(info, &method_names);
        let property_members =
            reflection_class_property_members(ctx, class_name, info, &property_names);
        let constructor_member = reflection_constructor_member(&method_members);
        let is_instantiable =
            reflection_class_is_instantiable(info, is_enum, constructor_member.as_ref());
        return Ok(ReflectionOwnerMetadata {
            reflected_name: Some(class_name.to_string()),
            attr_names: info.attribute_names.clone(),
            attr_args: info.attribute_args.clone(),
            interface_names: info.interfaces.clone(),
            trait_names: info.used_traits.clone(),
            method_names,
            property_names,
            constant_names,
            constant_members,
            constant_reflection_members,
            method_members,
            property_members,
            constructor_member,
            parent_class_name: reflection_parent_class_name(ctx, info),
            parameter_members: Vec::new(),
            required_parameter_count: 0,
            is_final: info.is_final,
            is_abstract: info.is_abstract,
            is_interface: false,
            is_trait: false,
            is_enum,
            is_readonly: info.is_readonly_class && !is_enum,
            is_instantiable,
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
            reflection_interface_constant_reflection_members(ctx, interface_name);
        let method_members = ctx
            .module
            .interface_infos
            .get(interface_name)
            .map(|info| reflection_interface_method_members(info, &method_names))
            .unwrap_or_else(|| default_method_members(&method_names, true, false));
        let property_members = default_property_members(&property_names, true);
        let constructor_member = reflection_constructor_member(&method_members);
        return Ok(ReflectionOwnerMetadata {
            reflected_name: Some(interface_name.to_string()),
            attr_names: Vec::new(),
            attr_args: Vec::new(),
            interface_names: reflection_interface_parent_names(ctx, interface_name),
            trait_names: Vec::new(),
            method_names,
            property_names,
            constant_names,
            constant_members,
            constant_reflection_members,
            method_members,
            property_members,
            constructor_member,
            parent_class_name: None,
            parameter_members: Vec::new(),
            required_parameter_count: 0,
            is_final: false,
            is_abstract: false,
            is_interface: true,
            is_trait: false,
            is_enum: false,
            is_readonly: false,
            is_instantiable: false,
            modifiers: 0,
            member_flags: ReflectionMemberFlags::default(),
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
            reflection_trait_constant_reflection_members(ctx, trait_name);
        let method_members = ctx
            .module
            .declared_trait_methods
            .get(trait_name)
            .map(|methods| reflection_trait_method_members(methods, &method_names))
            .unwrap_or_else(|| default_method_members(&method_names, false, true));
        let property_members = default_property_members(&property_names, false);
        let constructor_member = reflection_constructor_member(&method_members);
        return Ok(ReflectionOwnerMetadata {
            reflected_name: Some(trait_name.to_string()),
            attr_names: Vec::new(),
            attr_args: Vec::new(),
            interface_names: Vec::new(),
            trait_names,
            method_names,
            property_names,
            constant_names,
            constant_members,
            constant_reflection_members,
            method_members,
            property_members,
            constructor_member,
            parent_class_name: None,
            parameter_members: Vec::new(),
            required_parameter_count: 0,
            is_final: false,
            is_abstract: false,
            is_interface: false,
            is_trait: true,
            is_enum: false,
            is_readonly: false,
            is_instantiable: false,
            modifiers: 0,
            member_flags: ReflectionMemberFlags::default(),
        });
    }
    Ok(empty_reflection_metadata())
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
    let mut metadata = empty_reflection_metadata();
    metadata.reflected_name = Some(function.name.trim_start_matches('\\').to_string());
    metadata.attr_names = function.attribute_names.clone();
    metadata.attr_args = function.attribute_args.clone();
    metadata.parameter_members = reflection_parameter_members(signature);
    metadata.required_parameter_count = reflection_required_parameter_count(signature);
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
            if let Some(member) = reflection_interface_method_member(info, &method_key) {
                return Ok(reflection_method_owner_metadata(&method_name, member));
            }
        }
    }
    if let Some(trait_name) = resolve_reflection_trait(ctx, &reflected_class) {
        if let Some(methods) = ctx.module.declared_trait_methods.get(trait_name) {
            if let Some(member) = reflection_trait_method_member(methods, &method_key) {
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
        method_names: Vec::new(),
        property_names: Vec::new(),
        constant_names: Vec::new(),
        constant_members: Vec::new(),
        constant_reflection_members: Vec::new(),
        method_members: Vec::new(),
        property_members: Vec::new(),
        constructor_member: None,
        parent_class_name: None,
        parameter_members: member.parameters,
        required_parameter_count: member.required_parameter_count,
        is_final: false,
        is_abstract: false,
        is_interface: false,
        is_trait: false,
        is_enum: false,
        is_readonly: false,
        is_instantiable: false,
        modifiers: 0,
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
            Some(ReflectionOwnerMetadata {
                reflected_name: Some(property_name.clone()),
                attr_names: info.property_attribute_names.get(&property_name)?.clone(),
                attr_args: info.property_attribute_args.get(&property_name)?.clone(),
                interface_names: Vec::new(),
                trait_names: Vec::new(),
                method_names: Vec::new(),
                property_names: Vec::new(),
                constant_names: Vec::new(),
                constant_members: Vec::new(),
                constant_reflection_members: Vec::new(),
                method_members: Vec::new(),
                property_members: Vec::new(),
                constructor_member: None,
                parent_class_name: None,
                parameter_members: Vec::new(),
                required_parameter_count: 0,
                is_final: false,
                is_abstract: false,
                is_interface: false,
                is_trait: false,
                is_enum: false,
                is_readonly: false,
                is_instantiable: false,
                modifiers: 0,
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
    let parameters = reflection_parameter_members(signature);
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
            .and_then(|info| reflection_interface_method_member(info, method_key));
    }
    resolve_reflection_trait(ctx, reflected_class).and_then(|trait_name| {
        ctx.module
            .declared_trait_methods
            .get(trait_name)
            .and_then(|methods| reflection_trait_method_member(methods, method_key))
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
    if let Some(case) = resolve_reflection_enum_case(ctx, &reflected_class, &constant_name) {
        return Ok(ReflectionOwnerMetadata {
            reflected_name: Some(constant_name),
            attr_names: case.attribute_names.clone(),
            attr_args: case.attribute_args.clone(),
            interface_names: Vec::new(),
            trait_names: Vec::new(),
            method_names: Vec::new(),
            property_names: Vec::new(),
            constant_names: Vec::new(),
            constant_members: Vec::new(),
            constant_reflection_members: Vec::new(),
            method_members: Vec::new(),
            property_members: Vec::new(),
            constructor_member: None,
            parent_class_name: None,
            parameter_members: Vec::new(),
            required_parameter_count: 0,
            is_final: false,
            is_abstract: false,
            is_interface: false,
            is_trait: false,
            is_enum: false,
            is_readonly: false,
            is_instantiable: false,
            modifiers: 0,
            member_flags: ReflectionMemberFlags::default(),
        });
    }
    Ok(
        resolve_reflection_class_constant(ctx, &reflected_class, &constant_name)
            .map(|(_, info)| {
                let attr_names = info
                    .constant_attribute_names
                    .get(&constant_name)
                    .cloned()
                    .unwrap_or_default();
                let attr_args = info
                    .constant_attribute_args
                    .get(&constant_name)
                    .cloned()
                    .unwrap_or_default();
                ReflectionOwnerMetadata {
                    reflected_name: Some(constant_name),
                    attr_names,
                    attr_args,
                    interface_names: Vec::new(),
                    trait_names: Vec::new(),
                    method_names: Vec::new(),
                    property_names: Vec::new(),
                    constant_names: Vec::new(),
                    constant_members: Vec::new(),
                    constant_reflection_members: Vec::new(),
                    method_members: Vec::new(),
                    property_members: Vec::new(),
                    constructor_member: None,
                    parent_class_name: None,
                    parameter_members: Vec::new(),
                    required_parameter_count: 0,
                    is_final: false,
                    is_abstract: false,
                    is_interface: false,
                    is_trait: false,
                    is_enum: false,
                    is_readonly: false,
                    is_instantiable: false,
                    modifiers: 0,
                    member_flags: ReflectionMemberFlags::default(),
                }
            })
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
            .map(|case| ReflectionOwnerMetadata {
                reflected_name: Some(case_name.clone()),
                attr_names: case.attribute_names.clone(),
                attr_args: case.attribute_args.clone(),
                interface_names: Vec::new(),
                trait_names: Vec::new(),
                method_names: Vec::new(),
                property_names: Vec::new(),
                constant_names: Vec::new(),
                constant_members: Vec::new(),
                constant_reflection_members: Vec::new(),
                method_members: Vec::new(),
                property_members: Vec::new(),
                constructor_member: None,
                parent_class_name: None,
                parameter_members: Vec::new(),
                required_parameter_count: 0,
                is_final: false,
                is_abstract: false,
                is_interface: false,
                is_trait: false,
                is_enum: false,
                is_readonly: false,
                is_instantiable: false,
                modifiers: 0,
                member_flags: ReflectionMemberFlags::default(),
            })
            .unwrap_or_else(empty_reflection_metadata),
    )
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

/// Recursively appends interface constants, preserving inherited-interface precedence.
fn collect_interface_constant_members(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
    members: &mut Vec<ReflectionConstantMember>,
    seen: &mut std::collections::HashSet<String>,
) -> Result<()> {
    let Some(interface_info) = ctx.module.interface_infos.get(interface_name) else {
        return Ok(());
    };
    for parent in &interface_info.parents {
        collect_interface_constant_members(ctx, parent, members, seen)?;
    }
    for (constant_name, value_expr) in &interface_info.constants {
        if seen.contains(constant_name) {
            continue;
        }
        let value = reflection_constant_value(ctx, interface_name, None, value_expr, 0)?;
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
                case.attribute_names.clone(),
                case.attribute_args.clone(),
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
        for constant_name in current_info.constants.keys() {
            if seen.contains(constant_name) {
                continue;
            }
            push_unique_constant_reflection_member(
                constant_name,
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
                &mut members,
                &mut seen,
            );
        }
        for interface_name in &current_info.interfaces {
            for member in reflection_interface_constant_reflection_members(ctx, interface_name) {
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
) -> Vec<ReflectionListedMember> {
    let mut members = Vec::new();
    let mut seen = std::collections::HashSet::new();
    collect_interface_constant_reflection_members(ctx, interface_name, &mut members, &mut seen);
    members
}

/// Recursively appends interface constant-reflector objects.
fn collect_interface_constant_reflection_members(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
    members: &mut Vec<ReflectionListedMember>,
    seen: &mut std::collections::HashSet<String>,
) {
    let Some(interface_info) = ctx.module.interface_infos.get(interface_name) else {
        return;
    };
    for parent in &interface_info.parents {
        collect_interface_constant_reflection_members(ctx, parent, members, seen);
    }
    for constant_name in interface_info.constants.keys() {
        push_unique_constant_reflection_member(
            constant_name,
            Vec::new(),
            Vec::new(),
            members,
            seen,
        );
    }
}

/// Returns constant-reflector objects for direct trait constants.
fn reflection_trait_constant_reflection_members(
    ctx: &FunctionContext<'_>,
    trait_name: &str,
) -> Vec<ReflectionListedMember> {
    ctx.module
        .declared_trait_constants
        .get(trait_name)
        .map(|constants| {
            let mut members = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for constant_name in constants.keys() {
                push_unique_constant_reflection_member(
                    constant_name,
                    Vec::new(),
                    Vec::new(),
                    &mut members,
                    &mut seen,
                );
            }
            members
        })
        .unwrap_or_default()
}

/// Appends one constant-reflector member if a constant with this name was not already visible.
fn push_unique_constant_reflection_member(
    name: &str,
    attr_names: Vec<String>,
    attr_args: Vec<Option<Vec<AttrArgValue>>>,
    members: &mut Vec<ReflectionListedMember>,
    seen: &mut std::collections::HashSet<String>,
) {
    if !seen.insert(name.to_string()) {
        return;
    }
    members.push(ReflectionListedMember {
        name: name.to_string(),
        attr_names,
        attr_args,
        flags: ReflectionMemberFlags::default(),
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
        return Some(reflection_member_flags(false, visibility, false, false));
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
        return Some(reflection_member_flags(true, visibility, false, false));
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
    Some(ReflectionListedMember {
        name: method_key.clone(),
        attr_names: info
            .method_attribute_names
            .get(&method_key)
            .cloned()
            .unwrap_or_default(),
        attr_args: info
            .method_attribute_args
            .get(&method_key)
            .cloned()
            .unwrap_or_default(),
        flags: reflection_method_member_flags(info, &method_key)?,
        required_parameter_count: reflection_required_parameter_count(sig),
        parameters: reflection_parameter_members(sig),
    })
}

/// Builds ReflectionMethod array entries for methods declared by an interface.
fn reflection_interface_method_members(
    info: &InterfaceInfo,
    method_names: &[String],
) -> Vec<ReflectionListedMember> {
    method_names
        .iter()
        .filter_map(|method_name| reflection_interface_method_member(info, method_name))
        .collect()
}

/// Builds one ReflectionMethod array entry from interface metadata.
fn reflection_interface_method_member(
    info: &InterfaceInfo,
    method_name: &str,
) -> Option<ReflectionListedMember> {
    let method_key = php_symbol_key(method_name);
    let (sig, is_static) = info
        .methods
        .get(&method_key)
        .map(|sig| (sig, false))
        .or_else(|| info.static_methods.get(&method_key).map(|sig| (sig, true)))?;
    Some(ReflectionListedMember {
        name: method_key,
        attr_names: Vec::new(),
        attr_args: Vec::new(),
        flags: reflection_member_flags(is_static, &Visibility::Public, false, true),
        required_parameter_count: reflection_required_parameter_count(sig),
        parameters: reflection_parameter_members(sig),
    })
}

/// Builds ReflectionMethod array entries for methods declared by a trait.
fn reflection_trait_method_members(
    methods: &std::collections::HashMap<String, TraitMethodInfo>,
    method_names: &[String],
) -> Vec<ReflectionListedMember> {
    method_names
        .iter()
        .filter_map(|method_name| reflection_trait_method_member(methods, method_name))
        .collect()
}

/// Builds one ReflectionMethod array entry from retained trait metadata.
fn reflection_trait_method_member(
    methods: &std::collections::HashMap<String, TraitMethodInfo>,
    method_name: &str,
) -> Option<ReflectionListedMember> {
    let method_key = php_symbol_key(method_name);
    let info = methods.get(&method_key)?;
    Some(ReflectionListedMember {
        name: method_key,
        attr_names: Vec::new(),
        attr_args: Vec::new(),
        flags: reflection_member_flags(
            info.is_static,
            &info.visibility,
            info.is_final,
            info.is_abstract,
        ),
        required_parameter_count: reflection_required_parameter_count(&info.signature),
        parameters: reflection_parameter_members(&info.signature),
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
            reflection_member_flags(false, &Visibility::Public, false, false),
        )
    })?;
    Some(ReflectionListedMember {
        name: property_name.to_string(),
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
        flags,
        required_parameter_count: 0,
        parameters: Vec::new(),
    })
}

/// Builds placeholder ReflectionMethod entries for class-like metadata without full method schemas.
fn default_method_members(
    method_names: &[String],
    is_interface: bool,
    _is_trait: bool,
) -> Vec<ReflectionListedMember> {
    method_names
        .iter()
        .map(|name| ReflectionListedMember {
            name: name.clone(),
            attr_names: Vec::new(),
            attr_args: Vec::new(),
            flags: reflection_member_flags(false, &Visibility::Public, false, is_interface),
            required_parameter_count: 0,
            parameters: Vec::new(),
        })
        .collect()
}

/// Builds placeholder ReflectionProperty entries for class-like metadata without full property schemas.
fn default_property_members(
    property_names: &[String],
    is_interface: bool,
) -> Vec<ReflectionListedMember> {
    property_names
        .iter()
        .map(|name| ReflectionListedMember {
            name: name.clone(),
            attr_names: Vec::new(),
            attr_args: Vec::new(),
            flags: reflection_member_flags(false, &Visibility::Public, false, is_interface),
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

/// Builds reflected parameter metadata from a method/function signature.
fn reflection_parameter_members(sig: &FunctionSig) -> Vec<ReflectionParameterMember> {
    sig.params
        .iter()
        .enumerate()
        .map(|(index, (name, _))| {
            let is_variadic = sig.variadic.as_deref() == Some(name.as_str());
            ReflectionParameterMember {
                name: name.clone(),
                position: index as i64,
                is_optional: is_variadic
                    || sig
                        .defaults
                        .get(index)
                        .map(|default| default.is_some())
                        .unwrap_or(false),
                is_variadic,
                is_passed_by_reference: sig.ref_params.get(index).copied().unwrap_or(false),
                has_type: sig.declared_params.get(index).copied().unwrap_or(false),
            }
        })
        .collect()
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
) -> ReflectionMemberFlags {
    ReflectionMemberFlags {
        is_static,
        is_public: visibility == &Visibility::Public,
        is_protected: visibility == &Visibility::Protected,
        is_private: visibility == &Visibility::Private,
        is_final,
        is_abstract,
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
) -> Option<&'a crate::types::EnumCaseInfo> {
    let enum_key = php_symbol_key(enum_name.trim_start_matches('\\'));
    ctx.module
        .enum_infos
        .iter()
        .find(|(candidate, _)| php_symbol_key(candidate.trim_start_matches('\\')) == enum_key)
        .and_then(|(_, info)| info.cases.iter().find(|case| case.name == case_name))
}

/// Returns empty Reflection metadata for unsupported dynamic constructor operands.
fn empty_reflection_metadata() -> ReflectionOwnerMetadata {
    ReflectionOwnerMetadata {
        reflected_name: None,
        attr_names: Vec::new(),
        attr_args: Vec::new(),
        interface_names: Vec::new(),
        trait_names: Vec::new(),
        method_names: Vec::new(),
        property_names: Vec::new(),
        constant_names: Vec::new(),
        constant_members: Vec::new(),
        constant_reflection_members: Vec::new(),
        method_members: Vec::new(),
        property_members: Vec::new(),
        constructor_member: None,
        parent_class_name: None,
        parameter_members: Vec::new(),
        required_parameter_count: 0,
        is_final: false,
        is_abstract: false,
        is_interface: false,
        is_trait: false,
        is_enum: false,
        is_readonly: false,
        is_instantiable: false,
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
    attr_args: &[Option<Vec<AttrArgEntry>>],
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

/// Inserts the current boxed Mixed constant value into the stacked associative array.
fn emit_reflection_constant_hash_insert(ctx: &mut FunctionContext<'_>, key: &str) {
    let (key_label, key_len) = ctx.data.add_string(key.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x3, x0");                              // pass the boxed Reflection constant value as the hash payload
            ctx.emitter.instruction("mov x4, xzr");                             // boxed Mixed hash payloads do not use the high word
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
            ctx.emitter.instruction("mov rcx, rax");                            // pass the boxed Reflection constant value as the hash payload
            ctx.emitter.instruction("xor r8, r8");                              // boxed Mixed hash payloads do not use the high word
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
    emit_reflection_owner_int_property(
        ctx,
        "ReflectionParameter",
        "__position",
        parameter.position,
    )?;
    emit_reflection_owner_bool_property(
        ctx,
        "ReflectionParameter",
        "__optional",
        parameter.is_optional,
    )?;
    emit_reflection_owner_bool_property(
        ctx,
        "ReflectionParameter",
        "__variadic",
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
    ctx.emitter.instruction("str x0, [sp, #-16]!");                             // park the metadata-name array while appending strings
    for name in names {
        let (label, len) = ctx.data.add_string(name.as_bytes());
        ctx.emitter.instruction("ldr x0, [sp]");                                // reload the metadata-name array for this append
        abi::emit_symbol_address(ctx.emitter, "x1", &label);
        abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
        abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
        ctx.emitter.instruction("str x0, [sp]");                                // preserve the possibly-grown metadata-name array
    }
    ctx.emitter.instruction("ldr x0, [sp], #16");                               // restore the final metadata-name array as the result
}

/// Appends ReflectionClass metadata names to the current x86_64 result array.
fn emit_reflection_string_array_fill_x86_64(ctx: &mut FunctionContext<'_>, names: &[String]) {
    ctx.emitter.instruction("push rax");                                        // park the metadata-name array while appending strings
    ctx.emitter.instruction("sub rsp, 8");                                      // keep stack alignment stable across append helper calls
    for name in names {
        let (label, len) = ctx.data.add_string(name.as_bytes());
        ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 8]");                // reload the metadata-name array for this append
        abi::emit_symbol_address(ctx.emitter, "rsi", &label);
        abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
        abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
        ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");                // preserve the possibly-grown metadata-name array
    }
    ctx.emitter.instruction("add rsp, 8");                                      // drop the temporary alignment slot
    ctx.emitter.instruction("pop rax");                                         // restore the final metadata-name array as the result
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
    emit_reflection_int_property(ctx, i64::from(value), low_offset, low_offset + 8);
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
fn emit_reflection_int_property_by_name(
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
