//! Purpose:
//! Lowers simple static property loads and stores for the Phase 04 EIR backend.
//! Handles direct named receivers backed by runtime user-data symbols.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - This slice supports public scalar/string/array static properties with named,
//!   lexical `self`, and lexical `parent` receivers, but not late static binding,
//!   references, or non-indexed array mutation.
//! - Typed static properties use the same high-word uninitialized sentinel as
//!   the legacy backend before reads.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::UNINITIALIZED_TYPED_PROPERTY_SENTINEL;
use crate::ir::Instruction;
use crate::names::static_property_symbol;
use crate::parser::ast::Visibility;
use crate::types::{ClassInfo, PhpType};

use super::super::context::FunctionContext;
use super::{expect_data, expect_operand, store_if_result};
use crate::codegen_ir::{CodegenIrError, Result};

/// Resolved direct static property metadata for symbol-backed storage.
struct StaticPropertySlot {
    declaring_class: String,
    property: String,
    php_type: PhpType,
    symbol: String,
    is_declared: bool,
}

/// Lowers a direct static property read into the current result register(s).
pub(super) fn lower_load_static_property(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let slot = resolve_static_property_slot(ctx, inst)?;
    ensure_static_property_type_supported(&slot.php_type, inst)?;
    if slot.is_declared {
        emit_uninitialized_static_property_guard(ctx, &slot);
    }
    abi::emit_load_symbol_to_result(ctx.emitter, &slot.symbol, &slot.php_type);
    store_if_result(ctx, inst)
}

/// Lowers a direct static property write from one SSA operand into symbol-backed storage.
pub(super) fn lower_store_static_property(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let slot = resolve_static_property_slot(ctx, inst)?;
    ensure_static_property_type_supported(&slot.php_type, inst)?;
    let value_ty = ctx.value_php_type(value)?;
    ensure_static_property_value_supported(&slot, &value_ty, inst)?;
    ctx.load_value_to_result(value)?;
    abi::emit_store_result_to_symbol(ctx.emitter, &slot.symbol, &slot.php_type, true);
    if !matches!(slot.php_type.codegen_repr(), PhpType::Str) {
        abi::emit_store_zero_to_symbol(ctx.emitter, &slot.symbol, 8);
    }
    Ok(())
}

/// Resolves a static property immediate into declaring-class symbol metadata.
fn resolve_static_property_slot(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<StaticPropertySlot> {
    let label = static_property_label(ctx, inst)?;
    let (receiver, property) = parse_static_property_label(label)?;
    let receiver = resolve_static_property_receiver(ctx, receiver, inst)?;
    let class_info = ctx
        .module
        .class_infos
        .get(receiver.as_str())
        .ok_or_else(|| CodegenIrError::unsupported(format!("unknown static property class {}", receiver)))?;
    let Some((_, php_type)) = class_info
        .static_properties
        .iter()
        .find(|(name, _)| name == property)
    else {
        return Err(CodegenIrError::unsupported(format!(
            "{} for missing static property {}::${}",
            inst.op.name(),
            receiver,
            property
        )));
    };
    let declaring_class = class_info
        .static_property_declaring_classes
        .get(property)
        .map(String::as_str)
        .unwrap_or(receiver.as_str());
    let declaring_info = ctx
        .module
        .class_infos
        .get(declaring_class)
        .ok_or_else(|| CodegenIrError::unsupported(format!("unknown static property declaring class {}", declaring_class)))?;
    reject_non_public_static_property(declaring_class, property, declaring_info, inst)?;
    Ok(StaticPropertySlot {
        declaring_class: declaring_class.to_string(),
        property: property.to_string(),
        php_type: php_type.clone(),
        symbol: static_property_symbol(declaring_class, property),
        is_declared: declaring_info.declared_static_properties.contains(property),
    })
}

/// Resolves named, `self`, and `parent` receivers for direct static property access.
fn resolve_static_property_receiver(
    ctx: &FunctionContext<'_>,
    receiver: &str,
    inst: &Instruction,
) -> Result<String> {
    let receiver = receiver.trim_start_matches('\\');
    match receiver {
        "self" => super::current_method_class(ctx).map(str::to_string),
        "parent" => {
            let class_name = super::current_method_class(ctx)?;
            ctx.module
                .class_infos
                .get(class_name)
                .and_then(|class| class.parent.clone())
                .ok_or_else(|| CodegenIrError::unsupported(format!(
                    "{} for parent static receiver outside class with parent for {}",
                    inst.op.name(),
                    ctx.function.name
                )))
        }
        "static" => Err(CodegenIrError::unsupported(format!(
            "{} for late-bound static receiver static",
            inst.op.name()
        ))),
        _ => Ok(receiver.to_string()),
    }
}

/// Resolves the instruction string immediate that encodes `Class::property`.
fn static_property_label<'a>(
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

/// Splits a static property immediate into receiver and property names.
fn parse_static_property_label(label: &str) -> Result<(&str, &str)> {
    label.rsplit_once("::").ok_or_else(|| {
        CodegenIrError::invalid_module(format!("invalid static property label '{}'", label))
    })
}

/// Rejects non-public static properties until the EIR backend has class-context visibility checks.
fn reject_non_public_static_property(
    declaring_class: &str,
    property: &str,
    declaring_info: &ClassInfo,
    inst: &Instruction,
) -> Result<()> {
    if matches!(
        declaring_info.static_property_visibilities.get(property),
        None | Some(Visibility::Public)
    ) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} for non-public static property {}::${}",
        inst.op.name(),
        declaring_class,
        property
    )))
}

/// Verifies that this slice knows how to represent the static property type.
fn ensure_static_property_type_supported(php_type: &PhpType, inst: &Instruction) -> Result<()> {
    match php_type {
        PhpType::Bool | PhpType::Int | PhpType::Float | PhpType::Str | PhpType::Array(_) => Ok(()),
        _ => Err(CodegenIrError::unsupported(format!(
            "{} for static property PHP type {:?}",
            inst.op.name(),
            php_type
        ))),
    }
}

/// Verifies the assigned value already has the static property storage representation.
fn ensure_static_property_value_supported(
    slot: &StaticPropertySlot,
    value_ty: &PhpType,
    inst: &Instruction,
) -> Result<()> {
    if value_ty == &slot.php_type {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} assigning PHP type {:?} to {}::${} with PHP type {:?}",
        inst.op.name(),
        value_ty,
        slot.declaring_class,
        slot.property,
        slot.php_type
    )))
}

/// Emits a fatal guard for reads from uninitialized typed static properties.
fn emit_uninitialized_static_property_guard(
    ctx: &mut FunctionContext<'_>,
    slot: &StaticPropertySlot,
) {
    let initialized_label = ctx.next_label("static_prop_initialized");
    let marker_reg = abi::secondary_scratch_reg(ctx.emitter);
    let sentinel_reg = abi::tertiary_scratch_reg(ctx.emitter);
    abi::emit_load_symbol_to_reg(ctx.emitter, marker_reg, &slot.symbol, 8);
    abi::emit_load_int_immediate(ctx.emitter, sentinel_reg, UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", marker_reg, sentinel_reg)); // compare the static property marker against the uninitialized sentinel
            ctx.emitter.instruction(&format!("b.ne {}", initialized_label));    // continue the static property read once the slot has been initialized
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", marker_reg, sentinel_reg)); // compare the static property marker against the uninitialized sentinel
            ctx.emitter.instruction(&format!("jne {}", initialized_label));     // continue the static property read once the slot has been initialized
        }
    }
    emit_uninitialized_static_property_fatal(ctx, slot);
    ctx.emitter.label(&initialized_label);
}

/// Emits the runtime fatal diagnostic for an uninitialized typed static-property read.
fn emit_uninitialized_static_property_fatal(
    ctx: &mut FunctionContext<'_>,
    slot: &StaticPropertySlot,
) {
    let message = format!(
        "Fatal error: Typed static property {}::${} must not be accessed before initialization\n",
        slot.declaring_class, slot.property
    );
    let (message_label, message_len) = ctx.data.add_string(message.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #2");                              // select stderr for the uninitialized static-property fatal
            abi::emit_symbol_address(ctx.emitter, "x1", &message_label);
            ctx.emitter.instruction(&format!("mov x2, #{}", message_len));      // pass the fatal diagnostic byte length to write()
            ctx.emitter.syscall(4);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rsi", &message_label);
            ctx.emitter.instruction(&format!("mov edx, {}", message_len));      // pass the fatal diagnostic byte length to write()
            ctx.emitter.instruction("mov edi, 2");                              // select stderr for the uninitialized static-property fatal
            ctx.emitter.instruction("mov eax, 1");                              // select Linux write syscall
            ctx.emitter.instruction("syscall");                                 // write the uninitialized static-property fatal diagnostic
        }
    }
    abi::emit_exit(ctx.emitter, 1);
}
