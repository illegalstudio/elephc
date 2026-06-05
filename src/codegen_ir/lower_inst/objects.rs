//! Purpose:
//! Lowers object metadata opcodes for the Phase 04 EIR backend.
//! Supports simple object allocation, declared property access, and named or dynamic `instanceof` checks.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - Object payload layout must match the legacy backend and runtime helpers:
//!   heap kind word before payload, class id at payload offset 0, then 16 bytes
//!   per declared property slot.
//! - This slice intentionally rejects dynamic properties, references, interface
//!   method metadata, and non-literal default property expressions until their
//!   runtime paths land.

use std::collections::HashSet;

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::UNINITIALIZED_TYPED_PROPERTY_SENTINEL;
use crate::ir::{Immediate, Instruction, Op, ValueDef, ValueId};
use crate::names::{method_symbol, php_symbol_key};
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{direct_call_stack_pad_bytes, expect_data, expect_operand, materialize_direct_call_args, store_if_result};
use crate::codegen_ir::literal_defaults::{literal_default_value, LiteralDefaultValue};
use crate::codegen_ir::{CodegenIrError, Result};

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// Resolved declared-property storage metadata for a known object receiver.
struct PropertySlot {
    class_name: String,
    property: String,
    php_type: PhpType,
    offset: usize,
    is_declared: bool,
}

/// Resolved object property default metadata for fixed-offset initialization.
struct PropertyDefault {
    offset: usize,
    value: LiteralDefaultValue,
}

/// Lowers fixed-class object allocation and optional constructor invocation.
pub(super) fn lower_object_new(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let class_name = class_name_immediate(ctx, inst)?.to_string();
    let constructor_key = php_symbol_key("__construct");
    let (
        class_id,
        property_count,
        uninitialized_marker_offsets,
        property_defaults,
        constructor_impl,
    ) = {
        let class_info = ctx
            .module
            .class_infos
            .get(&class_name)
            .ok_or_else(|| CodegenIrError::unsupported(format!("unknown class {}", class_name)))?;
        if class_info.allow_dynamic_properties
            || class_interfaces_require_method_metadata(ctx, class_info)
        {
            return Err(CodegenIrError::unsupported(format!(
                "object allocation requiring dynamic or interface method metadata for {}",
                class_name
            )));
        }
        let property_defaults = collect_property_defaults(class_info, inst)?;
        let constructor_impl = if let Some(constructor) = class_info.methods.get(&constructor_key) {
            if constructor.params.len() != inst.operands.len() {
                return Err(CodegenIrError::unsupported(format!(
                    "constructor call to {}::__construct with {} args for {} params",
                    class_name,
                    inst.operands.len(),
                    constructor.params.len()
                )));
            }
            let impl_class = class_info
                .method_impl_classes
                .get(&constructor_key)
                .cloned()
                .unwrap_or_else(|| class_name.clone());
            Some(impl_class)
        } else if !inst.operands.is_empty() {
            return Err(CodegenIrError::unsupported(format!(
                "constructor arguments for class {} without __construct",
                class_name
            )));
        } else {
            None
        };
        let marker_offsets = uninitialized_property_marker_offsets(class_info);
        (
            class_info.class_id,
            class_info.properties.len(),
            marker_offsets,
            property_defaults,
            constructor_impl,
        )
    };
    emit_object_allocation(
        ctx,
        class_id,
        property_count,
        &uninitialized_marker_offsets,
    )?;
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("object_new missing result value"))?;
    ctx.store_result_value(result)?;
    emit_property_defaults(ctx, result, &property_defaults)?;
    if let Some(impl_class) = constructor_impl {
        emit_constructor_call(ctx, result, &inst.operands, &impl_class, &constructor_key)?;
    }
    Ok(())
}

/// Collects literal defaults that can be copied directly into object property slots.
fn collect_property_defaults(
    class_info: &crate::types::ClassInfo,
    inst: &Instruction,
) -> Result<Vec<PropertyDefault>> {
    let mut defaults = Vec::new();
    for (index, (property, php_type)) in class_info.properties.iter().enumerate() {
        let Some(default_expr) = class_info.defaults.get(index).and_then(Option::as_ref) else {
            continue;
        };
        let offset = class_info
            .property_offsets
            .get(property)
            .copied()
            .unwrap_or(8 + index * 16);
        defaults.push(PropertyDefault {
            offset,
            value: literal_default_value(
                &format!("property ${}", property),
                php_type,
                &default_expr.kind,
                inst.op.name(),
            )?,
        });
    }
    Ok(defaults)
}

/// Writes all supported property defaults into the newly allocated object.
fn emit_property_defaults(
    ctx: &mut FunctionContext<'_>,
    object: crate::ir::ValueId,
    defaults: &[PropertyDefault],
) -> Result<()> {
    for default in defaults {
        let object_reg = abi::secondary_scratch_reg(ctx.emitter);
        ctx.load_value_to_reg(object, object_reg)?;
        emit_property_default(ctx, object_reg, default)?;
    }
    Ok(())
}

/// Writes one literal property default into its object slot.
fn emit_property_default(
    ctx: &mut FunctionContext<'_>,
    object_reg: &str,
    default: &PropertyDefault,
) -> Result<()> {
    match &default.value {
        LiteralDefaultValue::Int(value) => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_load_int_immediate(ctx.emitter, int_reg, *value);
            abi::emit_store_to_address(ctx.emitter, int_reg, object_reg, default.offset);
            abi::emit_store_zero_to_address(ctx.emitter, object_reg, default.offset + 8);
        }
        LiteralDefaultValue::Bool(value) => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_load_int_immediate(ctx.emitter, int_reg, i64::from(*value));
            abi::emit_store_to_address(ctx.emitter, int_reg, object_reg, default.offset);
            abi::emit_store_zero_to_address(ctx.emitter, object_reg, default.offset + 8);
        }
        LiteralDefaultValue::Float(value) => {
            let label = ctx.data.add_float(*value);
            let scratch = abi::symbol_scratch_reg(ctx.emitter);
            let float_reg = abi::float_result_reg(ctx.emitter);
            abi::emit_symbol_address(ctx.emitter, scratch, &label);
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction(&format!("ldr {}, [{}]", float_reg, scratch)); // load the property default float literal through the symbol scratch register
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction(&format!("movsd {}, QWORD PTR [{}]", float_reg, scratch)); // load the property default float literal through the symbol scratch register
                }
            }
            abi::emit_store_to_address(ctx.emitter, float_reg, object_reg, default.offset);
            abi::emit_store_zero_to_address(ctx.emitter, object_reg, default.offset + 8);
        }
        LiteralDefaultValue::Str(value) => {
            let (label, len) = ctx.data.add_string(value.as_bytes());
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
            abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
            abi::emit_store_to_address(ctx.emitter, ptr_reg, object_reg, default.offset);
            abi::emit_store_to_address(ctx.emitter, len_reg, object_reg, default.offset + 8);
        }
    }
    Ok(())
}

/// Calls the resolved `__construct` method with the newly allocated object as `$this`.
fn emit_constructor_call(
    ctx: &mut FunctionContext<'_>,
    object: crate::ir::ValueId,
    constructor_args: &[crate::ir::ValueId],
    impl_class: &str,
    constructor_key: &str,
) -> Result<()> {
    let mut args = Vec::with_capacity(constructor_args.len() + 1);
    args.push(object);
    args.extend(constructor_args.iter().copied());
    let overflow_bytes = materialize_direct_call_args(ctx, &args)?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_call_label(ctx.emitter, &method_symbol(impl_class, constructor_key));
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, overflow_bytes);
    Ok(())
}

/// Lowers a declared object property read for statically known object receivers.
pub(super) fn lower_prop_get(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let property = property_name_immediate(ctx, inst)?.to_string();
    let slot = resolve_property_slot(ctx, object, &property, inst)?;
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(object, base_reg)?;
    if slot.is_declared {
        emit_uninitialized_typed_property_guard(ctx, &slot, base_reg);
    }
    emit_property_load(ctx, &slot, base_reg)?;
    store_if_result(ctx, inst)
}

/// Lowers a declared object property write for statically known object receivers.
pub(super) fn lower_prop_set(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let value = expect_operand(inst, 1)?;
    let property = property_name_immediate(ctx, inst)?.to_string();
    let slot = resolve_property_slot(ctx, object, &property, inst)?;
    let value_ty = ctx.value_php_type(value)?;
    ensure_property_value_supported(ctx, &slot, value, &value_ty, inst)?;
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(object, base_reg)?;
    emit_property_store(ctx, value, &slot, base_reg)
}

/// Lowers named `instanceof` using runtime class/interface metadata.
pub(super) fn lower_instanceof(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let value_ty = ctx.value_php_type(value)?;
    if !matches!(value_ty, PhpType::Object(_) | PhpType::Mixed | PhpType::Union(_)) {
        emit_false(ctx);
        return store_if_result(ctx, inst);
    }
    let class_name = class_name_immediate(ctx, inst)?;
    let Some((target_id, target_kind)) = classify_named_target(ctx, class_name) else {
        emit_false(ctx);
        return store_if_result(ctx, inst);
    };
    reject_interface_method_metadata_target(ctx, class_name)?;
    match value_ty {
        PhpType::Object(_) => {
            ctx.load_value_to_reg(value, abi::int_arg_reg_name(ctx.emitter.target, 0))?;
            emit_match_call(ctx, target_id, target_kind, "__rt_exception_matches");
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_reg(value, abi::int_arg_reg_name(ctx.emitter.target, 0))?;
            emit_match_call(ctx, target_id, target_kind, "__rt_mixed_instanceof");
        }
        _ => emit_false(ctx),
    }
    store_if_result(ctx, inst)
}

/// Lowers dynamic `instanceof` where the target is resolved from a runtime string or object.
pub(super) fn lower_instanceof_dynamic(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let target = expect_operand(inst, 1)?;
    let value_ty = ctx.value_php_type(value)?;
    let target_ty = ctx.value_php_type(target)?;
    let target_false = ctx.next_label("instanceof_dynamic_target_false");
    let done = ctx.next_label("instanceof_dynamic_done");
    emit_normalized_dynamic_instanceof_value(ctx, value, &value_ty)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    emit_dynamic_target_metadata(ctx, target, &target_ty, &target_false)?;
    emit_dynamic_match_call(ctx);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&target_false);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    emit_false(ctx);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Emits allocation, class-id stamping, and declared-property slot initialization.
fn emit_object_allocation(
    ctx: &mut FunctionContext<'_>,
    class_id: u64,
    property_count: usize,
    uninitialized_marker_offsets: &[usize],
) -> Result<()> {
    let payload_size = 8 + property_count * 16;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov x0, #{}", payload_size));     // request object payload storage for the class id and property slots
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction("mov x9, #4");                              // heap kind 4 marks object instances for ownership helpers
            ctx.emitter.instruction("str x9, [x0, #-8]");                       // stamp the heap header before the object payload
            ctx.emitter.instruction(&format!("mov x10, #{}", class_id));        // materialize the compile-time class id
            ctx.emitter.instruction("str x10, [x0]");                           // store the class id at object payload offset zero
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov rax, {}", payload_size));     // request object payload storage for the class id and property slots
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 4)); // materialize the x86_64 object heap kind word
            ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");            // stamp the heap header before the object payload
            ctx.emitter.instruction(&format!("mov r10, {}", class_id));         // materialize the compile-time class id
            ctx.emitter.instruction("mov QWORD PTR [rax], r10");                // store the class id at object payload offset zero
        }
    }
    let object_reg = abi::int_result_reg(ctx.emitter);
    for index in 0..property_count {
        let offset = 8 + index * 16;
        abi::emit_store_zero_to_address(ctx.emitter, object_reg, offset);
        abi::emit_store_zero_to_address(ctx.emitter, object_reg, offset + 8);
    }
    if !uninitialized_marker_offsets.is_empty() {
        let marker_reg = abi::secondary_scratch_reg(ctx.emitter);
        abi::emit_load_int_immediate(ctx.emitter, marker_reg, UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
        for offset in uninitialized_marker_offsets {
            abi::emit_store_to_address(ctx.emitter, marker_reg, object_reg, *offset);
        }
    }
    Ok(())
}

/// Returns true when implemented interfaces require method-wrapper metadata not emitted here.
fn class_interfaces_require_method_metadata(
    ctx: &FunctionContext<'_>,
    class_info: &crate::types::ClassInfo,
) -> bool {
    let mut seen = HashSet::new();
    let mut stack = class_info.interfaces.iter().map(String::as_str).collect::<Vec<_>>();
    while let Some(interface_name) = stack.pop() {
        if !seen.insert(interface_name.to_string()) {
            continue;
        }
        let Some(interface_info) = ctx.module.interface_infos.get(interface_name) else {
            return true;
        };
        if !interface_info.method_order.is_empty() {
            return true;
        }
        stack.extend(interface_info.parents.iter().map(String::as_str));
    }
    false
}

/// Collects property high-word offsets that should start with the typed-property sentinel.
fn uninitialized_property_marker_offsets(class_info: &crate::types::ClassInfo) -> Vec<usize> {
    class_info
        .properties
        .iter()
        .enumerate()
        .filter_map(|(index, (property, _))| {
            let starts_uninitialized = class_info.declared_properties.contains(property)
                && class_info.defaults.get(index).is_some_and(|default| default.is_none());
            if starts_uninitialized {
                Some(
                    class_info
                        .property_offsets
                        .get(property)
                        .copied()
                        .unwrap_or(8 + index * 16)
                        + 8,
                )
            } else {
                None
            }
        })
        .collect()
}

/// Resolves the property slot for a concrete object receiver and declared property name.
fn resolve_property_slot(
    ctx: &FunctionContext<'_>,
    object: crate::ir::ValueId,
    property: &str,
    inst: &Instruction,
) -> Result<PropertySlot> {
    let object_ty = ctx.value_php_type(object)?;
    let PhpType::Object(class_name) = object_ty else {
        if let PhpType::Packed(class_name) = object_ty {
            return resolve_packed_field_slot(ctx, &class_name, property, inst);
        }
        return Err(CodegenIrError::unsupported(format!(
            "{} for receiver PHP type {:?}",
            inst.op.name(),
            object_ty
        )));
    };
    let normalized = class_name.trim_start_matches('\\');
    let class_info = ctx
        .module
        .class_infos
        .get(normalized)
        .ok_or_else(|| CodegenIrError::unsupported(format!("unknown class {}", normalized)))?;
    if class_info.reference_properties.contains(property) {
        return Err(CodegenIrError::unsupported(format!(
            "{} for reference property {}::${}",
            inst.op.name(),
            normalized,
            property
        )));
    }
    let Some((index, (_, php_type))) = class_info
        .properties
        .iter()
        .enumerate()
        .find(|(_, (name, _))| name == property)
    else {
        return Err(CodegenIrError::unsupported(format!(
            "{} for dynamic or missing property {}::${}",
            inst.op.name(),
            normalized,
            property
        )));
    };
    ensure_property_type_supported(php_type, inst)?;
    let offset = class_info
        .property_offsets
        .get(property)
        .copied()
        .unwrap_or(8 + index * 16);
    Ok(PropertySlot {
        class_name: normalized.to_string(),
        property: property.to_string(),
        php_type: php_type.clone(),
        offset,
        is_declared: class_info.declared_properties.contains(property),
    })
}

/// Resolves a field slot on an embedded packed-class receiver.
fn resolve_packed_field_slot(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    property: &str,
    inst: &Instruction,
) -> Result<PropertySlot> {
    let normalized = class_name.trim_start_matches('\\');
    let class_info = ctx
        .module
        .packed_class_infos
        .get(normalized)
        .ok_or_else(|| CodegenIrError::unsupported(format!("unknown packed class {}", normalized)))?;
    let Some(field) = class_info.fields.iter().find(|field| field.name == property) else {
        return Err(CodegenIrError::unsupported(format!(
            "{} for missing packed field {}::${}",
            inst.op.name(),
            normalized,
            property
        )));
    };
    ensure_property_type_supported(&field.php_type, inst)?;
    Ok(PropertySlot {
        class_name: normalized.to_string(),
        property: property.to_string(),
        php_type: field.php_type.clone(),
        offset: field.offset,
        is_declared: false,
    })
}

/// Verifies that this slice knows how to represent the property type in an object slot.
fn ensure_property_type_supported(php_type: &PhpType, inst: &Instruction) -> Result<()> {
    match php_type {
        PhpType::Bool | PhpType::Int | PhpType::Float | PhpType::Str => Ok(()),
        ty if is_pointer_sized_property_type(ty) => Ok(()),
        _ => Err(CodegenIrError::unsupported(format!(
            "{} for property PHP type {:?}",
            inst.op.name(),
            php_type
        ))),
    }
}

/// Verifies the assigned value already has the property storage representation.
fn ensure_property_value_supported(
    ctx: &FunctionContext<'_>,
    slot: &PropertySlot,
    value: ValueId,
    value_ty: &PhpType,
    inst: &Instruction,
) -> Result<()> {
    if value_ty == &slot.php_type {
        return Ok(());
    }
    if is_pointer_sized_property_type(&slot.php_type)
        && is_pointer_slot_null_sentinel(ctx, value, value_ty)?
    {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} assigning PHP type {:?} to {}::${} with PHP type {:?}",
        inst.op.name(),
        value_ty,
        slot.class_name,
        slot.property,
        slot.php_type
    )))
}

/// Returns true when a value can initialize a pointer-sized slot as null.
fn is_pointer_slot_null_sentinel(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    value_ty: &PhpType,
) -> Result<bool> {
    if matches!(value_ty, PhpType::Void) {
        return Ok(true);
    }
    if !matches!(value_ty, PhpType::Int) {
        return Ok(false);
    }
    let metadata = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = metadata.def else {
        return Ok(false);
    };
    let instruction = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    Ok(instruction.op == Op::ConstI64 && instruction.immediate == Some(Immediate::I64(0)))
}

/// Emits the declared-property load into the canonical result register(s).
fn emit_property_load(
    ctx: &mut FunctionContext<'_>,
    slot: &PropertySlot,
    base_reg: &str,
) -> Result<()> {
    match &slot.php_type {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_load_from_address(ctx.emitter, ptr_reg, base_reg, slot.offset);
            abi::emit_load_from_address(ctx.emitter, len_reg, base_reg, slot.offset + 8);
        }
        PhpType::Float => {
            let float_reg = abi::float_result_reg(ctx.emitter);
            abi::emit_load_from_address(ctx.emitter, float_reg, base_reg, slot.offset);
        }
        PhpType::Bool | PhpType::Int => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_load_from_address(ctx.emitter, int_reg, base_reg, slot.offset);
        }
        ty if is_pointer_sized_property_type(ty) => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_load_from_address(ctx.emitter, int_reg, base_reg, slot.offset);
        }
        _ => return Err(CodegenIrError::unsupported(format!(
            "property load for PHP type {:?}",
            slot.php_type
        ))),
    }
    Ok(())
}

/// Emits a declared-property store from an SSA value into the object slot.
fn emit_property_store(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    slot: &PropertySlot,
    base_reg: &str,
) -> Result<()> {
    match &slot.php_type {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            ctx.load_string_value_to_regs(value, ptr_reg, len_reg)?;
            abi::emit_store_to_address(ctx.emitter, ptr_reg, base_reg, slot.offset);
            abi::emit_store_to_address(ctx.emitter, len_reg, base_reg, slot.offset + 8);
        }
        PhpType::Float => {
            let float_reg = abi::float_result_reg(ctx.emitter);
            ctx.load_value_to_reg(value, float_reg)?;
            abi::emit_store_to_address(ctx.emitter, float_reg, base_reg, slot.offset);
            abi::emit_store_zero_to_address(ctx.emitter, base_reg, slot.offset + 8);
        }
        PhpType::Bool | PhpType::Int => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            ctx.load_value_to_reg(value, int_reg)?;
            abi::emit_store_to_address(ctx.emitter, int_reg, base_reg, slot.offset);
            abi::emit_store_zero_to_address(ctx.emitter, base_reg, slot.offset + 8);
        }
        ty if is_pointer_sized_property_type(ty) => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            ctx.load_value_to_reg(value, int_reg)?;
            abi::emit_store_to_address(ctx.emitter, int_reg, base_reg, slot.offset);
            abi::emit_store_zero_to_address(ctx.emitter, base_reg, slot.offset + 8);
        }
        _ => return Err(CodegenIrError::unsupported(format!(
            "property store for PHP type {:?}",
            slot.php_type
        ))),
    }
    Ok(())
}

/// Returns true for property values represented as a single pointer-sized word.
fn is_pointer_sized_property_type(php_type: &PhpType) -> bool {
    matches!(
        php_type.codegen_repr(),
        PhpType::Iterable
            | PhpType::Mixed
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Buffer(_)
            | PhpType::Callable
            | PhpType::Object(_)
            | PhpType::Packed(_)
            | PhpType::Pointer(_)
            | PhpType::Resource(_)
    )
}

/// Emits a fatal guard for reads from uninitialized typed properties.
fn emit_uninitialized_typed_property_guard(
    ctx: &mut FunctionContext<'_>,
    slot: &PropertySlot,
    object_reg: &str,
) {
    let initialized_label = ctx.next_label("typed_prop_initialized");
    let marker_reg = abi::secondary_scratch_reg(ctx.emitter);
    let sentinel_reg = abi::tertiary_scratch_reg(ctx.emitter);
    abi::emit_load_from_address(ctx.emitter, marker_reg, object_reg, slot.offset + 8);
    abi::emit_load_int_immediate(ctx.emitter, sentinel_reg, UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", marker_reg, sentinel_reg)); // compare the property marker against the uninitialized sentinel
            ctx.emitter.instruction(&format!("b.ne {}", initialized_label));    // continue the property read once the slot has been initialized
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", marker_reg, sentinel_reg)); // compare the property marker against the uninitialized sentinel
            ctx.emitter.instruction(&format!("jne {}", initialized_label));     // continue the property read once the slot has been initialized
        }
    }
    emit_uninitialized_typed_property_fatal(ctx, slot);
    ctx.emitter.label(&initialized_label);
}

/// Emits the runtime fatal diagnostic for an uninitialized typed-property read.
fn emit_uninitialized_typed_property_fatal(
    ctx: &mut FunctionContext<'_>,
    slot: &PropertySlot,
) {
    let message = format!(
        "Fatal error: Typed property {}::${} must not be accessed before initialization\n",
        slot.class_name, slot.property
    );
    let (message_label, message_len) = ctx.data.add_string(message.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #2");                              // select stderr for the uninitialized typed-property fatal
            ctx.emitter.adrp("x1", &message_label);
            ctx.emitter.add_lo12("x1", "x1", &message_label);
            ctx.emitter.instruction(&format!("mov x2, #{}", message_len));      // pass the fatal diagnostic byte length to write()
            ctx.emitter.syscall(4);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rsi", &message_label);
            ctx.emitter.instruction(&format!("mov edx, {}", message_len));      // pass the fatal diagnostic byte length to write()
            ctx.emitter.instruction("mov edi, 2");                              // select stderr for the uninitialized typed-property fatal
            ctx.emitter.instruction("mov eax, 1");                              // select Linux write syscall
            ctx.emitter.instruction("syscall");                                 // write the uninitialized typed-property fatal diagnostic
        }
    }
    abi::emit_exit(ctx.emitter, 1);
}

/// Normalizes the tested value into an object pointer or null for dynamic `instanceof`.
fn emit_normalized_dynamic_instanceof_value(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    match value_ty {
        PhpType::Object(_) => {
            ctx.load_value_to_reg(value, abi::int_result_reg(ctx.emitter))?;
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_reg(value, abi::int_result_reg(ctx.emitter))?;
            emit_mixed_instanceof_value_normalization(ctx);
        }
        _ => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
    }
    Ok(())
}

/// Unboxes a Mixed/Union tested value and leaves only object payloads as matchable.
fn emit_mixed_instanceof_value_normalization(ctx: &mut FunctionContext<'_>) {
    let object_label = ctx.next_label("instanceof_dynamic_value_object");
    let done = ctx.next_label("instanceof_dynamic_value_done");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #6");                              // runtime tag 6 means the tested mixed payload is an object
            ctx.emitter.instruction(&format!("b.eq {}", object_label));         // object payloads can be matched after dynamic target resolution
            ctx.emitter.instruction("mov x0, #0");                              // scalar mixed payloads become null so the matcher returns false
            ctx.emitter.instruction(&format!("b {}", done));                    // skip object-payload promotion for scalar payloads
            ctx.emitter.label(&object_label);
            ctx.emitter.instruction("mov x0, x1");                              // promote the unboxed object pointer into the normal result register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 6");                              // runtime tag 6 means the tested mixed payload is an object
            ctx.emitter.instruction(&format!("je {}", object_label));           // object payloads can be matched after dynamic target resolution
            ctx.emitter.instruction("xor eax, eax");                            // scalar mixed payloads become null so the matcher returns false
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip object-payload promotion for scalar payloads
            ctx.emitter.label(&object_label);
            ctx.emitter.instruction("mov rax, rdi");                            // promote the unboxed object pointer into the normal result register
        }
    }
    ctx.emitter.label(&done);
}

/// Resolves the dynamic `instanceof` target into matcher id/kind registers.
fn emit_dynamic_target_metadata(
    ctx: &mut FunctionContext<'_>,
    target: crate::ir::ValueId,
    target_ty: &PhpType,
    false_label: &str,
) -> Result<()> {
    match target_ty {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            ctx.load_string_value_to_regs(target, ptr_reg, len_reg)?;
            emit_lookup_string_target(ctx, false_label);
        }
        PhpType::Object(_) => {
            ctx.load_value_to_reg(target, abi::int_result_reg(ctx.emitter))?;
            emit_object_target_metadata(ctx);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_reg(target, abi::int_result_reg(ctx.emitter))?;
            emit_mixed_target_metadata(ctx, false_label);
        }
        _ => emit_invalid_dynamic_target_fatal(ctx),
    }
    Ok(())
}

/// Looks up a string dynamic target in the runtime class/interface name table.
fn emit_lookup_string_target(ctx: &mut FunctionContext<'_>, false_label: &str) {
    abi::emit_call_label(ctx.emitter, "__rt_instanceof_lookup");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // did the dynamic string resolve to a known class or interface?
            ctx.emitter.instruction(&format!("b.eq {}", false_label));          // unresolved class-string targets make instanceof false
            ctx.emitter.instruction("mov x0, x1");                              // move the resolved target id into the matcher target-id register
            ctx.emitter.instruction("mov x1, x2");                              // move the resolved target kind into the matcher target-kind register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // did the dynamic string resolve to a known class or interface?
            ctx.emitter.instruction(&format!("je {}", false_label));            // unresolved class-string targets make instanceof false
            ctx.emitter.instruction("mov rax, rdi");                            // move the resolved target id into the matcher target-id register
        }
    }
}

/// Extracts matcher metadata from an object-typed dynamic target.
fn emit_object_target_metadata(ctx: &mut FunctionContext<'_>) {
    let ok_label = ctx.next_label("instanceof_dynamic_object_target_ok");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz x0, {}", ok_label));         // non-null object targets can provide runtime class metadata
            emit_invalid_dynamic_target_fatal(ctx);
            ctx.emitter.label(&ok_label);
            ctx.emitter.instruction("ldr x0, [x0]");                            // load the runtime class id from the target object header
            ctx.emitter.instruction("mov x1, #0");                              // object targets always resolve to class target kind
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // null object targets are not valid dynamic instanceof targets
            ctx.emitter.instruction(&format!("jne {}", ok_label));              // non-null object targets can provide runtime class metadata
            emit_invalid_dynamic_target_fatal(ctx);
            ctx.emitter.label(&ok_label);
            ctx.emitter.instruction("mov rax, QWORD PTR [rax]");                // load the runtime class id from the target object header
            ctx.emitter.instruction("xor edx, edx");                            // object targets always resolve to class target kind
        }
    }
}

/// Unboxes a Mixed/Union target and routes strings or objects to the matching target resolver.
fn emit_mixed_target_metadata(ctx: &mut FunctionContext<'_>, false_label: &str) {
    let string_label = ctx.next_label("instanceof_dynamic_target_string");
    let object_label = ctx.next_label("instanceof_dynamic_target_object");
    let done = ctx.next_label("instanceof_dynamic_target_done");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #1");                              // runtime tag 1 means the dynamic target is a string
            ctx.emitter.instruction(&format!("b.eq {}", string_label));         // resolve boxed string targets through class-string lookup
            ctx.emitter.instruction("cmp x0, #6");                              // runtime tag 6 means the dynamic target is an object
            ctx.emitter.instruction(&format!("b.eq {}", object_label));         // resolve boxed object targets through their runtime class id
            emit_invalid_dynamic_target_fatal(ctx);
            ctx.emitter.label(&string_label);
            emit_lookup_string_target(ctx, false_label);
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&object_label);
            ctx.emitter.instruction("mov x0, x1");                              // move the unboxed target object pointer into the result register
            emit_object_target_metadata(ctx);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 1");                              // runtime tag 1 means the dynamic target is a string
            ctx.emitter.instruction(&format!("je {}", string_label));           // resolve boxed string targets through class-string lookup
            ctx.emitter.instruction("cmp rax, 6");                              // runtime tag 6 means the dynamic target is an object
            ctx.emitter.instruction(&format!("je {}", object_label));           // resolve boxed object targets through their runtime class id
            emit_invalid_dynamic_target_fatal(ctx);
            ctx.emitter.label(&string_label);
            ctx.emitter.instruction("mov rax, rdi");                            // move the unboxed target string pointer into the lookup input register
            emit_lookup_string_target(ctx, false_label);
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&object_label);
            ctx.emitter.instruction("mov rax, rdi");                            // move the unboxed target object pointer into the result register
            emit_object_target_metadata(ctx);
        }
    }
    ctx.emitter.label(&done);
}

/// Emits a dynamic `instanceof` matcher call after target id/kind have been resolved.
fn emit_dynamic_match_call(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_push_reg(ctx.emitter, "x1");
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");
            abi::emit_push_reg(ctx.emitter, "rdx");
        }
    }
    abi::emit_pop_reg(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 2));
    abi::emit_pop_reg(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1));
    abi::emit_pop_reg(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0));
    abi::emit_call_label(ctx.emitter, "__rt_exception_matches");
}

/// Emits the runtime fatal for invalid dynamic `instanceof` targets.
fn emit_invalid_dynamic_target_fatal(ctx: &mut FunctionContext<'_>) {
    abi::emit_call_label(ctx.emitter, "__rt_instanceof_invalid_target");
}

/// Emits the metadata matcher call with object-or-mixed input already in argument 0.
fn emit_match_call(
    ctx: &mut FunctionContext<'_>,
    target_id: u64,
    target_kind: i64,
    helper: &str,
) {
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        target_id as i64,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        target_kind,
    );
    abi::emit_call_label(ctx.emitter, helper);
}

/// Classifies a named target as a class `(kind 0)` or interface `(kind 1)`.
fn classify_named_target(
    ctx: &FunctionContext<'_>,
    class_name: &str,
) -> Option<(u64, i64)> {
    let normalized = class_name.trim_start_matches('\\');
    if let Some(class_info) = ctx.module.class_infos.get(normalized) {
        return Some((class_info.class_id, 0));
    }
    ctx.module
        .interface_infos
        .get(normalized)
        .map(|interface_info| (interface_info.interface_id, 1))
}

/// Rejects targets whose runtime metadata would reference interface wrappers not emitted yet.
fn reject_interface_method_metadata_target(ctx: &FunctionContext<'_>, class_name: &str) -> Result<()> {
    let normalized = class_name.trim_start_matches('\\');
    if let Some(class_info) = ctx.module.class_infos.get(normalized) {
        if class_interfaces_require_method_metadata(ctx, class_info) {
            return Err(CodegenIrError::unsupported(format!(
                "instanceof target with interface method metadata {}",
                normalized
            )));
        }
    }
    if let Some(interface_info) = ctx.module.interface_infos.get(normalized) {
        if !interface_info.method_order.is_empty() {
            return Err(CodegenIrError::unsupported(format!(
                "instanceof interface target with method metadata {}",
                normalized
            )));
        }
    }
    Ok(())
}

/// Emits a boolean false result for non-object values or unresolved targets.
fn emit_false(ctx: &mut FunctionContext<'_>) {
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
}

/// Resolves an instruction property-name immediate into the module data pool.
fn property_name_immediate<'a>(
    ctx: &'a FunctionContext<'_>,
    inst: &Instruction,
) -> Result<&'a str> {
    let data = expect_data(inst)?;
    ctx.module
        .data
        .strings
        .get(data.as_raw() as usize)
        .map(String::as_str)
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))
}

/// Resolves an instruction class-name immediate into the module data pool.
fn class_name_immediate<'a>(
    ctx: &'a FunctionContext<'_>,
    inst: &Instruction,
) -> Result<&'a str> {
    let data = expect_data(inst)?;
    ctx.module
        .data
        .class_names
        .get(data.as_raw() as usize)
        .map(String::as_str)
        .ok_or_else(|| CodegenIrError::missing_entry("class data", data.as_raw()))
}
