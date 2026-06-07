//! Purpose:
//! Lowers simple static property loads and stores for the Phase 04 EIR backend.
//! Handles direct named receivers backed by runtime user-data symbols.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - This slice supports public scalar/string/array/object static properties with
//!   named, lexical `self`, and lexical `parent` receivers, but not late static
//!   binding, references, or non-indexed array mutation.
//! - Typed static properties use the same high-word uninitialized sentinel as
//!   the legacy backend before reads.

use crate::codegen::{abi, emit_box_current_value_as_mixed};
use crate::codegen::platform::Arch;
use crate::codegen::UNINITIALIZED_TYPED_PROPERTY_SENTINEL;
use crate::ir::{Instruction, ValueDef, ValueId};
use crate::names::static_property_symbol;
use crate::parser::ast::Visibility;
use crate::types::{ClassInfo, PhpType};

use super::super::context::FunctionContext;
use super::{expect_data, expect_operand, store_if_result};
use crate::codegen_ir::{CodegenIrError, Result};

const CALLED_CLASS_ID_PARAM: &str = "__elephc_called_class_id";

/// Resolved direct static property metadata for symbol-backed storage.
struct StaticPropertySlot {
    declaring_class: String,
    property: String,
    php_type: PhpType,
    symbol: String,
    is_declared: bool,
    late_bound: bool,
    branches: Vec<StaticPropertyBranch>,
}

/// One runtime class-id branch for a late-bound static property slot.
struct StaticPropertyBranch {
    class_id: u64,
    declaring_class: String,
    private_inaccessible: bool,
}

/// Lowers a direct static property read into the current result register(s).
pub(super) fn lower_load_static_property(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let slot = resolve_static_property_slot(ctx, inst)?;
    ensure_static_property_type_supported(&slot.php_type, inst)?;
    if slot.late_bound && !slot.branches.is_empty() {
        let class_id_reg = class_id_work_reg(ctx.emitter);
        if emit_called_class_id_to_reg(ctx, class_id_reg)? {
            emit_dynamic_load_static_property_result(ctx, &slot, class_id_reg)?;
            return store_if_result(ctx, inst);
        }
    }
    emit_direct_load_static_property_result(ctx, &slot);
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
    box_static_property_value_if_needed(ctx, &slot.php_type, &value_ty);
    let release_previous = !value_is_same_static_property_load(ctx, value, &slot)?;
    if slot.late_bound && !slot.branches.is_empty() {
        let class_id_reg = class_id_work_reg(ctx.emitter);
        if emit_called_class_id_to_reg(ctx, class_id_reg)? {
            emit_dynamic_store_static_property_result(ctx, &slot, class_id_reg, release_previous);
            return Ok(());
        }
    }
    emit_direct_store_static_property_result(ctx, &slot, release_previous);
    Ok(())
}

/// Returns true when a store writes back the same static slot it just loaded.
fn value_is_same_static_property_load(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    slot: &StaticPropertySlot,
) -> Result<bool> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(false);
    };
    let Some(inst_ref) = ctx.function.instruction(inst) else {
        return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
    };
    if inst_ref.op != crate::ir::Op::LoadStaticProperty {
        return Ok(false);
    }
    Ok(resolve_static_property_slot(ctx, inst_ref)?.symbol == slot.symbol)
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
    ensure_static_property_visibility(ctx, declaring_class, property, declaring_info, inst)?;
    let (raw_receiver, _) = parse_static_property_label(label)?;
    let late_bound = raw_receiver.trim_start_matches('\\') == "static";
    let branches = dynamic_static_property_branches(ctx, late_bound, property, declaring_class);
    Ok(StaticPropertySlot {
        declaring_class: declaring_class.to_string(),
        property: property.to_string(),
        php_type: php_type.clone(),
        symbol: static_property_symbol(declaring_class, property),
        is_declared: declaring_info.declared_static_properties.contains(property),
        late_bound,
        branches,
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
        "static" => super::current_method_class(ctx).map(str::to_string),
        _ => Ok(receiver.to_string()),
    }
}

/// Emits a direct static property read from the fallback declaring-class symbol.
fn emit_direct_load_static_property_result(
    ctx: &mut FunctionContext<'_>,
    slot: &StaticPropertySlot,
) {
    if slot.is_declared {
        emit_uninitialized_static_property_guard(ctx, slot);
    }
    abi::emit_load_symbol_to_result(ctx.emitter, &slot.symbol, &slot.php_type);
}

/// Emits a direct static property store into the fallback declaring-class symbol.
fn emit_direct_store_static_property_result(
    ctx: &mut FunctionContext<'_>,
    slot: &StaticPropertySlot,
    release_previous: bool,
) {
    abi::emit_store_result_to_symbol(ctx.emitter, &slot.symbol, &slot.php_type, release_previous);
    clear_uninitialized_marker_after_static_store(ctx, &slot.symbol, &slot.php_type);
}

/// Loads the forwarded called-class id into `dest_reg` when the current frame has it.
fn emit_called_class_id_to_reg(
    ctx: &mut FunctionContext<'_>,
    dest_reg: &str,
) -> Result<bool> {
    let Some(slot) = ctx.local_slot_by_name(CALLED_CLASS_ID_PARAM) else {
        return Ok(false);
    };
    let offset = ctx.local_offset(slot)?;
    abi::load_at_offset(ctx.emitter, dest_reg, offset);
    Ok(true)
}

/// Emits a late-bound static property read selected by the runtime called-class id.
fn emit_dynamic_load_static_property_result(
    ctx: &mut FunctionContext<'_>,
    slot: &StaticPropertySlot,
    class_id_reg: &str,
) -> Result<()> {
    let done = ctx.next_label("static_prop_load_done");
    let mut labels = Vec::new();
    for branch in &slot.branches {
        let label = ctx.next_label("static_prop_load_branch");
        emit_branch_if_class_id_matches(ctx, class_id_reg, branch.class_id, &label);
        labels.push((label, branch));
    }
    emit_direct_load_static_property_result(ctx, slot);
    abi::emit_jump(ctx.emitter, &done);
    for (label, branch) in labels {
        ctx.emitter.label(&label);
        if branch.private_inaccessible {
            emit_private_static_property_access_fatal(ctx);
            continue;
        }
        let branch_slot = branch_static_property_slot(ctx, slot, branch);
        emit_direct_load_static_property_result(ctx, &branch_slot);
        abi::emit_jump(ctx.emitter, &done);
    }
    ctx.emitter.label(&done);
    Ok(())
}

/// Emits a late-bound static property store selected by the runtime called-class id.
fn emit_dynamic_store_static_property_result(
    ctx: &mut FunctionContext<'_>,
    slot: &StaticPropertySlot,
    class_id_reg: &str,
    release_previous: bool,
) {
    let done = ctx.next_label("static_prop_store_done");
    let mut labels = Vec::new();
    for branch in &slot.branches {
        let label = ctx.next_label("static_prop_store_branch");
        emit_branch_if_class_id_matches(ctx, class_id_reg, branch.class_id, &label);
        labels.push((label, branch));
    }
    emit_direct_store_static_property_result(ctx, slot, release_previous);
    abi::emit_jump(ctx.emitter, &done);
    for (label, branch) in labels {
        ctx.emitter.label(&label);
        if branch.private_inaccessible {
            emit_private_static_property_access_fatal(ctx);
            continue;
        }
        let branch_slot = branch_static_property_slot(ctx, slot, branch);
        emit_direct_store_static_property_result(ctx, &branch_slot, release_previous);
        abi::emit_jump(ctx.emitter, &done);
    }
    ctx.emitter.label(&done);
}

/// Builds direct symbol metadata for one redeclared late-bound static property branch.
fn branch_static_property_slot(
    ctx: &FunctionContext<'_>,
    fallback: &StaticPropertySlot,
    branch: &StaticPropertyBranch,
) -> StaticPropertySlot {
    let is_declared = ctx
        .module
        .class_infos
        .get(&branch.declaring_class)
        .is_some_and(|class_info| class_info.declared_static_properties.contains(&fallback.property));
    StaticPropertySlot {
        declaring_class: branch.declaring_class.clone(),
        property: fallback.property.clone(),
        php_type: fallback.php_type.clone(),
        symbol: static_property_symbol(&branch.declaring_class, &fallback.property),
        is_declared,
        late_bound: false,
        branches: Vec::new(),
    }
}

/// Clears the typed-property high word after a successful static property store.
fn clear_uninitialized_marker_after_static_store(
    ctx: &mut FunctionContext<'_>,
    symbol: &str,
    ty: &PhpType,
) {
    if !matches!(ty.codegen_repr(), PhpType::Str) {
        abi::emit_store_zero_to_symbol(ctx.emitter, symbol, 8);
    }
}

/// Emits a conditional branch when the runtime called-class id matches `class_id`.
fn emit_branch_if_class_id_matches(
    ctx: &mut FunctionContext<'_>,
    class_id_reg: &str,
    class_id: u64,
    label: &str,
) {
    let compare_reg = class_id_compare_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, compare_reg, class_id as i64);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", class_id_reg, compare_reg)); // compare the runtime called class id to a redeclared static property owner
            ctx.emitter.instruction(&format!("b.eq {}", label));                // use this static property slot when the called class id matches
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", class_id_reg, compare_reg)); // compare the runtime called class id to a redeclared static property owner
            ctx.emitter.instruction(&format!("je {}", label));                  // use this static property slot when the called class id matches
        }
    }
}

/// Returns the scratch register that carries the runtime called-class id.
fn class_id_work_reg(emitter: &crate::codegen::emit::Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x13",
        Arch::X86_64 => "r13",
    }
}

/// Returns the scratch register used for class-id branch comparisons.
fn class_id_compare_reg(emitter: &crate::codegen::emit::Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x14",
        Arch::X86_64 => "r14",
    }
}

/// Collects redeclared static property slots reachable from a late-bound receiver.
fn dynamic_static_property_branches(
    ctx: &FunctionContext<'_>,
    late_bound: bool,
    property: &str,
    fallback_declaring_class: &str,
) -> Vec<StaticPropertyBranch> {
    if !late_bound {
        return Vec::new();
    }
    let Ok(base_class) = super::current_method_class(ctx) else {
        return Vec::new();
    };
    let mut branches = Vec::new();
    for (class_name, class_info) in &ctx.module.class_infos {
        if !is_same_or_descendant(ctx, class_name, base_class) {
            continue;
        }
        let Some(declaring_class) = class_info.static_property_declaring_classes.get(property) else {
            continue;
        };
        if declaring_class == fallback_declaring_class {
            continue;
        }
        let visibility = class_info
            .static_property_visibilities
            .get(property)
            .unwrap_or(&Visibility::Public);
        branches.push(StaticPropertyBranch {
            class_id: class_info.class_id,
            declaring_class: declaring_class.clone(),
            private_inaccessible: matches!(visibility, Visibility::Private)
                && declaring_class.as_str() != base_class,
        });
    }
    branches.sort_by_key(|branch| branch.class_id);
    branches.dedup_by_key(|branch| branch.class_id);
    branches
}

/// Returns true when `class_name` is the base class or one of its descendants.
fn is_same_or_descendant(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    ancestor: &str,
) -> bool {
    let mut cursor = Some(class_name);
    while let Some(name) = cursor {
        if name == ancestor {
            return true;
        }
        cursor = ctx
            .module
            .class_infos
            .get(name)
            .and_then(|class_info| class_info.parent.as_deref());
    }
    false
}

/// Emits a PHP fatal for late-bound private static property access.
fn emit_private_static_property_access_fatal(ctx: &mut FunctionContext<'_>) {
    let message = "Fatal error: Cannot access private static property\n";
    let (message_label, message_len) = ctx.data.add_string(message.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #2");                              // select stderr for the private static-property fatal
            abi::emit_symbol_address(ctx.emitter, "x1", &message_label);
            ctx.emitter.instruction(&format!("mov x2, #{}", message_len));      // pass the fatal diagnostic byte length to write()
            ctx.emitter.syscall(4);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rsi", &message_label);
            ctx.emitter.instruction(&format!("mov edx, {}", message_len));      // pass the fatal diagnostic byte length to write()
            ctx.emitter.instruction("mov edi, 2");                              // select stderr for the private static-property fatal
            ctx.emitter.instruction("mov eax, 1");                              // select Linux write syscall
            ctx.emitter.instruction("syscall");                                 // write the private static-property fatal diagnostic
        }
    }
    abi::emit_exit(ctx.emitter, 1);
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

/// Verifies that the current class context may access a static property.
fn ensure_static_property_visibility(
    ctx: &FunctionContext<'_>,
    declaring_class: &str,
    property: &str,
    declaring_info: &ClassInfo,
    inst: &Instruction,
) -> Result<()> {
    let visibility = declaring_info
        .static_property_visibilities
        .get(property)
        .unwrap_or(&Visibility::Public);
    if static_property_is_visible(ctx, declaring_class, visibility) {
        Ok(())
    } else {
        Err(CodegenIrError::unsupported(format!(
            "{} for non-public static property {}::${}",
            inst.op.name(),
            declaring_class,
            property
        )))
    }
}

/// Returns true when the current EIR function has access to the member visibility.
fn static_property_is_visible(
    ctx: &FunctionContext<'_>,
    declaring_class: &str,
    visibility: &Visibility,
) -> bool {
    match visibility {
        Visibility::Public => true,
        Visibility::Private => super::current_method_class(ctx)
            .is_ok_and(|current| current == declaring_class),
        Visibility::Protected => super::current_method_class(ctx).is_ok_and(|current| {
            current == declaring_class || is_same_or_descendant(ctx, current, declaring_class)
        }),
    }
}

/// Verifies that this slice knows how to represent the static property type.
fn ensure_static_property_type_supported(php_type: &PhpType, inst: &Instruction) -> Result<()> {
    match php_type {
        PhpType::Bool
        | PhpType::Int
        | PhpType::Float
        | PhpType::Str
        | PhpType::Mixed
        | PhpType::Union(_)
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Object(_) => Ok(()),
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
    if matches!(slot.php_type.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        return Ok(());
    }
    if is_empty_array_for_array_static_property(value_ty, &slot.php_type) {
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

/// Returns true when an empty array literal initializes a typed static array property.
fn is_empty_array_for_array_static_property(value_ty: &PhpType, slot_ty: &PhpType) -> bool {
    let PhpType::Array(value_elem) = value_ty.codegen_repr() else {
        return false;
    };
    if !matches!(slot_ty.codegen_repr(), PhpType::Array(_)) {
        return false;
    }
    matches!(value_elem.codegen_repr(), PhpType::Never | PhpType::Void)
}

/// Boxes concrete values when the static property storage is Mixed/Union.
fn box_static_property_value_if_needed(
    ctx: &mut FunctionContext<'_>,
    slot_ty: &PhpType,
    value_ty: &PhpType,
) {
    if matches!(slot_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
        && !matches!(value_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
    {
        emit_box_current_value_as_mixed(ctx.emitter, &value_ty.codegen_repr());
    }
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
