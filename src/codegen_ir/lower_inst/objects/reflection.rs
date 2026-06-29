//! Purpose:
//! Lowers metadata-aware allocation for builtin Reflection owner objects in the
//! EIR backend.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::objects::lower_object_new()`.
//!
//! Key details:
//! - `ReflectionClass`, `ReflectionMethod`, and `ReflectionProperty`
//!   constructors are compile-time metadata lookups that populate private
//!   `__name`/`__attrs` slots instead of running their public empty bodies.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen_ir::{CodegenIrError, Result};
use crate::ir::{Immediate, Instruction, Op, ValueDef, ValueId};
use crate::names::php_symbol_key;
use crate::types::AttrArgEntry;

use super::super::super::context::FunctionContext;

/// Compile-time metadata used to populate one Reflection owner object.
struct ReflectionOwnerMetadata {
    reflected_name: Option<String>,
    attr_names: Vec<String>,
    attr_args: Vec<Option<Vec<AttrArgEntry>>>,
}

/// Returns true for reflection owner classes that need metadata-aware construction.
pub(super) fn is_reflection_owner_class(class_name: &str) -> bool {
    matches!(
        class_name,
        "ReflectionClass" | "ReflectionMethod" | "ReflectionProperty"
    )
}

/// Lowers builtin Reflection owner allocation by populating compile-time metadata slots.
pub(super) fn lower_reflection_owner_new(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    class_name: &str,
) -> Result<()> {
    let metadata = reflection_owner_metadata(ctx, class_name, inst)?;
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
    }
    emit_reflection_attrs_property(
        ctx,
        class_name,
        &metadata.attr_names,
        &metadata.attr_args,
    )?;
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("reflection object_new missing result"))?;
    ctx.store_result_value(result)
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

/// Resolves Reflection constructor operands to captured class/member metadata.
fn reflection_owner_metadata(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    inst: &Instruction,
) -> Result<ReflectionOwnerMetadata> {
    match class_name {
        "ReflectionClass" => reflection_class_metadata(ctx, inst),
        "ReflectionMethod" => reflection_method_metadata(ctx, inst),
        "ReflectionProperty" => reflection_property_metadata(ctx, inst),
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
    Ok(resolve_reflection_class(ctx, &reflected_class)
        .map(|(class_name, info)| ReflectionOwnerMetadata {
            reflected_name: Some(class_name.to_string()),
            attr_names: info.attribute_names.clone(),
            attr_args: info.attribute_args.clone(),
        })
        .unwrap_or_else(empty_reflection_metadata))
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
    Ok(resolve_reflection_class(ctx, &reflected_class)
        .and_then(|(_, info)| {
            Some(ReflectionOwnerMetadata {
                reflected_name: None,
                attr_names: info.method_attribute_names.get(&method_key)?.clone(),
                attr_args: info.method_attribute_args.get(&method_key)?.clone(),
            })
        })
        .unwrap_or_else(empty_reflection_metadata))
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
                reflected_name: None,
                attr_names: info.property_attribute_names.get(&property_name)?.clone(),
                attr_args: info.property_attribute_args.get(&property_name)?.clone(),
            })
        })
        .unwrap_or_else(empty_reflection_metadata))
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

/// Returns empty Reflection metadata for unsupported dynamic constructor operands.
fn empty_reflection_metadata() -> ReflectionOwnerMetadata {
    ReflectionOwnerMetadata {
        reflected_name: None,
        attr_names: Vec::new(),
        attr_args: Vec::new(),
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

/// Replaces the Reflection object's default `__attrs` array with populated metadata.
fn emit_reflection_attrs_property(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    attr_names: &[String],
    attr_args: &[Option<Vec<AttrArgEntry>>],
) -> Result<()> {
    let (attrs_low_offset, attrs_high_offset) = reflection_attrs_offsets(class_name);
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

/// Returns the low/high object offsets for the private `__attrs` slot.
fn reflection_attrs_offsets(class_name: &str) -> (usize, usize) {
    if class_name == "ReflectionClass" {
        (24, 32)
    } else {
        (8, 16)
    }
}
