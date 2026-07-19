//! Purpose:
//! Lowers simple static property loads and stores for the Phase 04 EIR backend.
//! Handles direct named receivers backed by runtime user-data symbols.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - This slice supports public scalar/string/array/object static properties with
//!   named, lexical `self`, and lexical `parent` receivers, but not late static
//!   references or non-indexed array mutation.
//! - `static::` receivers use native class-id branches for generated classes and
//!   the eval native-frame override when late static scope points at an eval class.
//! - Typed static properties use the same high-word uninitialized sentinel as
//!   the emitted code before reads.

use crate::codegen::{abi, emit_box_current_value_as_mixed};
use crate::codegen::platform::Arch;
use crate::codegen::UNINITIALIZED_TYPED_PROPERTY_SENTINEL;
use crate::ir::{Instruction, ValueDef, ValueId};
use crate::names::static_property_symbol;
use crate::parser::ast::Visibility;
use crate::types::{ClassInfo, PhpType};

use super::super::context::FunctionContext;
use super::{
    builtins, expect_data, expect_operand, load_value_to_first_int_arg, property_values,
    store_if_result,
};
use crate::codegen::{CodegenIrError, Result};

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
pub(super) fn lower_load_static_property(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if let Some((class_name, property)) = eval_dynamic_static_property_target(ctx, inst)? {
        return builtins::lower_eval_static_property_get(ctx, inst, &class_name, &property);
    }
    let slot = resolve_static_property_slot(ctx, inst, true)?;
    ensure_static_property_type_supported(&slot.php_type, inst)?;
    let eval_done_label = emit_eval_native_frame_static_property_get_if_needed(ctx, inst, &slot)?;
    if slot.late_bound && !slot.branches.is_empty() {
        let class_id_reg = class_id_work_reg(ctx.emitter);
        if emit_called_class_id_to_reg(ctx, class_id_reg)? {
            emit_dynamic_load_static_property_result(ctx, &slot, class_id_reg)?;
            store_if_result(ctx, inst)?;
            if let Some(done_label) = eval_done_label {
                ctx.emitter.label(&done_label);
            }
            return Ok(());
        }
    }
    emit_direct_load_static_property_result(ctx, &slot);
    store_if_result(ctx, inst)?;
    if let Some(done_label) = eval_done_label {
        ctx.emitter.label(&done_label);
    }
    Ok(())
}

/// Returns an eval dynamic static-property target when no AOT class owns the receiver.
fn eval_dynamic_static_property_target(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<Option<(String, String)>> {
    if !builtins::has_eval_context(ctx) {
        return Ok(None);
    }
    let label = static_property_label(ctx, inst)?;
    let (receiver, property) = parse_static_property_label(label)?;
    let receiver = resolve_static_property_receiver(ctx, receiver, inst)?;
    if ctx.module.class_infos.contains_key(receiver.as_str()) {
        return Ok(None);
    }
    Ok(Some((receiver, property.to_string())))
}

/// Lowers a direct static property write from one SSA operand into symbol-backed storage.
pub(super) fn lower_store_static_property(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    if let Some((class_name, property)) = eval_dynamic_static_property_target(ctx, inst)? {
        return builtins::lower_eval_static_property_set(
            ctx,
            inst,
            value,
            &class_name,
            &property,
        );
    }
    let slot = resolve_static_property_slot(ctx, inst, true)?;
    ensure_static_property_type_supported(&slot.php_type, inst)?;
    let value_ty = ctx.value_php_type(value)?;
    ensure_static_property_value_supported(&slot, &value_ty, inst)?;
    let release_previous = !value_is_same_static_property_load(ctx, value, &slot)?;
    let eval_done_label =
        emit_eval_native_frame_static_property_set_if_needed(ctx, inst, value, &slot)?;
    load_static_property_store_value_to_result(ctx, value, &slot.php_type)?;
    if slot.late_bound && !slot.branches.is_empty() {
        let class_id_reg = class_id_work_reg(ctx.emitter);
        if emit_called_class_id_to_reg(ctx, class_id_reg)? {
            emit_dynamic_store_static_property_result(ctx, &slot, class_id_reg, release_previous);
            if let Some(done_label) = eval_done_label {
                ctx.emitter.label(&done_label);
            }
            return Ok(());
        }
    }
    emit_direct_store_static_property_result(ctx, &slot, release_previous);
    if let Some(done_label) = eval_done_label {
        ctx.emitter.label(&done_label);
    }
    Ok(())
}

/// Lowers a Reflection static property read, bypassing PHP member visibility.
pub(super) fn lower_load_reflection_static_property(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let slot = resolve_static_property_slot(ctx, inst, false)?;
    ensure_static_property_type_supported(&slot.php_type, inst)?;
    emit_direct_load_static_property_result(ctx, &slot);
    store_if_result(ctx, inst)
}

/// Lowers a Reflection static-property initialization probe.
pub(super) fn lower_reflection_static_property_initialized(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let slot = resolve_static_property_slot(ctx, inst, false)?;
    emit_direct_static_property_initialized_result(ctx, &slot);
    store_if_result(ctx, inst)
}

/// Lowers a Reflection static property write, bypassing PHP member visibility.
pub(super) fn lower_store_reflection_static_property(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let slot = resolve_static_property_slot(ctx, inst, false)?;
    ensure_static_property_type_supported(&slot.php_type, inst)?;
    let value_ty = ctx.value_php_type(value)?;
    ensure_static_property_value_supported(&slot, &value_ty, inst)?;
    load_static_property_store_value_to_result(ctx, value, &slot.php_type)?;
    let release_previous = !value_is_same_static_property_load(ctx, value, &slot)?;
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
    if !matches!(
        inst_ref.op,
        crate::ir::Op::LoadStaticProperty | crate::ir::Op::LoadReflectionStaticProperty
    ) {
        return Ok(false);
    }
    let enforce_visibility = inst_ref.op == crate::ir::Op::LoadStaticProperty;
    Ok(resolve_static_property_slot(ctx, inst_ref, enforce_visibility)?.symbol == slot.symbol)
}

/// Resolves a static property immediate into declaring-class symbol metadata.
fn resolve_static_property_slot(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
    enforce_visibility: bool,
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
    if enforce_visibility {
        ensure_static_property_visibility(ctx, declaring_class, property, declaring_info, inst)?;
    }
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

/// Emits `true` when the static-property slot is initialized.
fn emit_direct_static_property_initialized_result(
    ctx: &mut FunctionContext<'_>,
    slot: &StaticPropertySlot,
) {
    if !slot.is_declared {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
        return;
    }
    emit_static_property_initialized_bool(ctx, slot);
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

/// Compares a typed static-property marker with the uninitialized sentinel.
fn emit_static_property_initialized_bool(
    ctx: &mut FunctionContext<'_>,
    slot: &StaticPropertySlot,
) {
    let marker_reg = abi::secondary_scratch_reg(ctx.emitter);
    let sentinel_reg = abi::tertiary_scratch_reg(ctx.emitter);
    abi::emit_load_symbol_to_reg(ctx.emitter, marker_reg, &slot.symbol, 8);
    abi::emit_load_int_immediate(ctx.emitter, sentinel_reg, UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", marker_reg, sentinel_reg)); // compare the static property marker against the uninitialized sentinel
            ctx.emitter.instruction("cset x0, ne");                             // materialize true when the static property is initialized
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", marker_reg, sentinel_reg)); // compare the static property marker against the uninitialized sentinel
            ctx.emitter.instruction("setne al");                                // materialize true when the static property is initialized
            ctx.emitter.instruction("movzx rax, al");                           // widen the initialization flag into the integer result register
        }
    }
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

/// Emits an eval late-static override read before falling back to native slots.
fn emit_eval_native_frame_static_property_get_if_needed(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    slot: &StaticPropertySlot,
) -> Result<Option<String>> {
    if !slot.late_bound || !ctx.module.required_runtime_features.eval_bridge {
        return Ok(None);
    }
    let frame_class = super::current_method_class(ctx)?.to_string();
    let no_override_label = ctx.next_label("eval_late_static_prop_get_no_override");
    let done_label = ctx.next_label("eval_late_static_prop_get_done");
    builtins::lower_eval_native_frame_static_property_get(
        ctx,
        inst,
        &frame_class,
        &slot.property,
        &no_override_label,
        &done_label,
    )?;
    ctx.emitter.label(&no_override_label);
    Ok(Some(done_label))
}

/// Emits an eval late-static override write before falling back to native slots.
fn emit_eval_native_frame_static_property_set_if_needed(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    value: ValueId,
    slot: &StaticPropertySlot,
) -> Result<Option<String>> {
    if !slot.late_bound || !ctx.module.required_runtime_features.eval_bridge {
        return Ok(None);
    }
    let frame_class = super::current_method_class(ctx)?.to_string();
    let no_override_label = ctx.next_label("eval_late_static_prop_set_no_override");
    let done_label = ctx.next_label("eval_late_static_prop_set_done");
    builtins::lower_eval_native_frame_static_property_set(
        ctx,
        inst,
        value,
        &frame_class,
        &slot.property,
        &no_override_label,
        &done_label,
    )?;
    ctx.emitter.label(&no_override_label);
    Ok(Some(done_label))
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
    if !matches!(ty.codegen_repr(), PhpType::Str | PhpType::TaggedScalar) {
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
        | PhpType::Void
        | PhpType::Never
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
    if can_store_value_as_tagged_scalar_static_property(value_ty, &slot.php_type) {
        return Ok(());
    }
    if matches!(slot.php_type.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        return Ok(());
    }
    if matches!(slot.php_type.codegen_repr(), PhpType::Int)
        && matches!(value_ty.codegen_repr(), PhpType::Mixed)
    {
        return Ok(());
    }
    if is_empty_array_for_array_static_property(value_ty, &slot.php_type) {
        return Ok(());
    }
    if can_coerce_mixed_to_scalar_static_property(value_ty, &slot.php_type) {
        return Ok(());
    }
    if property_values::can_unbox_mixed_to_object_property(value_ty, &slot.php_type) {
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

/// Returns true when a boxed Mixed value can be coerced before a scalar static-property store.
fn can_coerce_mixed_to_scalar_static_property(value_ty: &PhpType, slot_ty: &PhpType) -> bool {
    matches!(value_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
        && matches!(
            slot_ty.codegen_repr(),
            PhpType::Int | PhpType::Bool | PhpType::Float | PhpType::Str
        )
}

/// Returns true when a value can materialize the inline nullable-int static-property shape.
fn can_store_value_as_tagged_scalar_static_property(
    value_ty: &PhpType,
    slot_ty: &PhpType,
) -> bool {
    if slot_ty.codegen_repr() != PhpType::TaggedScalar {
        return false;
    }
    matches!(
        value_ty.codegen_repr(),
        PhpType::Int
            | PhpType::Bool
            | PhpType::Callable
            | PhpType::Void
            | PhpType::Never
            | PhpType::TaggedScalar
            | PhpType::Mixed
            | PhpType::Union(_)
    )
}

/// Loads a value in the register shape required by the target static-property slot.
fn load_static_property_store_value_to_result(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    slot_ty: &PhpType,
) -> Result<()> {
    let value_ty = ctx.value_php_type(value)?;
    if slot_ty.codegen_repr() == PhpType::TaggedScalar {
        match value_ty.codegen_repr() {
            PhpType::TaggedScalar => {
                ctx.load_value_to_result(value)?;
            }
            PhpType::Int | PhpType::Bool | PhpType::Callable => {
                ctx.load_value_to_result(value)?;
                crate::codegen::sentinels::emit_tagged_scalar_from_int_result(ctx.emitter);
            }
            PhpType::Void | PhpType::Never => {
                crate::codegen::sentinels::emit_tagged_scalar_null(ctx.emitter);
            }
            PhpType::Mixed | PhpType::Union(_) => {
                ctx.load_value_to_result(value)?;
                emit_mixed_result_as_tagged_scalar(ctx);
            }
            other => {
                return Err(CodegenIrError::unsupported(format!(
                    "static property tagged-scalar store from PHP type {:?}",
                    other
                )))
            }
        }
        return Ok(());
    }
    if matches!(value_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        load_value_to_first_int_arg(ctx, value)?;
        match slot_ty.codegen_repr() {
            PhpType::Str => {
                abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
                abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            }
            PhpType::Int => abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int"),
            PhpType::Bool => abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool"),
            PhpType::Float => abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_float"),
            PhpType::Object(_) => {
                property_values::emit_mixed_object_for_property_store(ctx)
            }
            _ => {}
        }
        return Ok(());
    }
    ctx.load_value_to_result(value)?;
    if matches!(slot_ty.codegen_repr(), PhpType::Int)
        && matches!(value_ty.codegen_repr(), PhpType::Mixed)
    {
        emit_mixed_result_as_int(ctx, value)?;
        return Ok(());
    }
    box_static_property_value_if_needed(ctx, slot_ty, &value_ty);
    Ok(())
}

/// Narrows a loaded Mixed result to int for coercive typed static-property stores.
fn emit_mixed_result_as_int(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // move the Mixed pointer into the first SysV argument register
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
        }
    }
    if value_is_owned_mixed_store_temporary(ctx, value)? {
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        ctx.load_value_to_result(value)?;
        abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
        abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    }
    Ok(())
}

/// Returns true when a Mixed store source is a temporary that must be released after narrowing.
fn value_is_owned_mixed_store_temporary(ctx: &FunctionContext<'_>, value: ValueId) -> Result<bool> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(false);
    };
    let Some(inst_ref) = ctx.function.instruction(inst) else {
        return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
    };
    Ok(matches!(
        inst_ref.op,
        crate::ir::Op::ICheckedAdd
            | crate::ir::Op::ICheckedSub
            | crate::ir::Op::ICheckedMul
            | crate::ir::Op::MixedNumericBinop
            | crate::ir::Op::MixedBox
    ))
}

/// Reorders `__rt_mixed_unbox` output into the tagged-scalar result register pair.
fn emit_mixed_result_as_tagged_scalar(ctx: &mut FunctionContext<'_>) {
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x9, x0");                              // preserve the unboxed Mixed tag before moving the payload
            ctx.emitter.instruction("mov x0, x1");                              // place the unboxed payload into the tagged-scalar payload register
            ctx.emitter.instruction("mov x1, x9");                              // place the unboxed Mixed tag into the tagged-scalar tag register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r10, rax");                            // preserve the unboxed Mixed tag before moving the payload
            ctx.emitter.instruction("mov rax, rdi");                            // place the unboxed payload into the tagged-scalar payload register
            ctx.emitter.instruction("mov rdx, r10");                            // place the unboxed Mixed tag into the tagged-scalar tag register
        }
    }
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

/// Emits the runtime throw for an uninitialized typed static-property read.
///
/// Constructs an `Error` object with the diagnostic message, publishes it to
/// `_exc_value`, and branches to `__rt_throw_current` so surrounding try/catch
/// blocks can observe and catch it. When no handler is registered, the uncaught
/// fast path prints the specific fatal diagnostic and exits, preserving the old behavior.
fn emit_uninitialized_static_property_fatal(
    ctx: &mut FunctionContext<'_>,
    slot: &StaticPropertySlot,
) {
    let message = format!(
        "Typed static property {}::${} must not be accessed before initialization",
        slot.declaring_class, slot.property
    );
    let fatal_message = format!("Fatal error: {}\n", message);
    let (fatal_label, fatal_len) = ctx.data.add_string(fatal_message.as_bytes());
    emit_uninitialized_static_property_uncaught_fatal_if_no_handler(
        ctx,
        &fatal_label,
        fatal_len,
    );
    let (message_label, message_len) = ctx.data.add_string(message.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #32");                             // request Throwable payload storage
            ctx.emitter.instruction("bl __rt_heap_alloc");                      // allocate the Error object payload
            ctx.emitter.instruction("mov x9, #6");                              // heap kind 6 = object instance
            ctx.emitter.instruction("str x9, [x0, #-8]");                       // stamp allocation as a runtime object
            abi::emit_symbol_address(ctx.emitter, "x9", "_spl_error_class_id");   // load Error's runtime class id symbol
            ctx.emitter.instruction("ldr x9, [x9]");                            // load Error's runtime class id for this program
            ctx.emitter.instruction("str x9, [x0]");                            // store class id at the object header
            abi::emit_symbol_address(ctx.emitter, "x9", &message_label);          // materialize static Error message pointer
            ctx.emitter.instruction("str x9, [x0, #8]");                        // store static Error message pointer
            ctx.emitter.instruction(&format!("mov x9, #{}", message_len));      // load Error message length
            ctx.emitter.instruction("str x9, [x0, #16]");                       // store exception message length
            ctx.emitter.instruction("str xzr, [x0, #24]");                      // exception code defaults to zero
            abi::emit_symbol_address(ctx.emitter, "x9", "_exc_value");             // materialize the active exception cell
            ctx.emitter.instruction("str x0, [x9]");                            // publish the active exception object
            ctx.emitter.instruction("b __rt_throw_current");                    // enter the standard exception unwinder
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("push rbp");                                // preserve caller frame pointer for exception allocation
            ctx.emitter.instruction("mov rbp, rsp");                            // establish aligned helper frame
            ctx.emitter.instruction("sub rsp, 16");                             // keep the nested heap allocation call 16-byte aligned
            ctx.emitter.instruction("mov rax, 32");                             // request Throwable payload storage
            ctx.emitter.instruction("call __rt_heap_alloc");                    // allocate the Error object payload
            ctx.emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(6))); // stamp the canonical x86_64 heap-kind word (magic + kind 6 throwable)
            ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");            // stamp allocation as a runtime object
            abi::emit_load_symbol_to_reg(ctx.emitter, "r10", "_spl_error_class_id", 0); // load Error's runtime class id for this program
            ctx.emitter.instruction("mov QWORD PTR [rax], r10");                // store class id at the object header
            abi::emit_symbol_address(ctx.emitter, "r10", &message_label);          // materialize static Error message pointer
            ctx.emitter.instruction("mov QWORD PTR [rax + 8], r10");            // store static Error message pointer
            ctx.emitter.instruction(&format!("mov QWORD PTR [rax + 16], {}", message_len)); // store Error message length
            ctx.emitter.instruction("mov QWORD PTR [rax + 24], 0");             // exception code defaults to zero
            abi::emit_store_reg_to_symbol(ctx.emitter, "rax", "_exc_value", 0);   // publish the active exception object
            ctx.emitter.instruction("mov rsp, rbp");                            // release helper frame before throwing
            ctx.emitter.instruction("pop rbp");                                 // restore caller frame pointer before throwing
            ctx.emitter.instruction("jmp __rt_throw_current");                  // enter the standard exception unwinder
        }
    }
}

/// Emits a no-handler fast path that preserves the specific typed static-property fatal text.
fn emit_uninitialized_static_property_uncaught_fatal_if_no_handler(
    ctx: &mut FunctionContext<'_>,
    fatal_label: &str,
    fatal_len: usize,
) {
    let throw_label = ctx.next_label("typed_static_property_throw");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(ctx.emitter, "x9", "_exc_handler_top", 0);
            ctx.emitter.instruction(&format!("cbnz x9, {}", throw_label));      // keep typed static-property errors catchable when a handler is active
            abi::emit_symbol_address(ctx.emitter, "x1", fatal_label);          // load the specific uninitialized static-property fatal text
            ctx.emitter.instruction(&format!("mov x2, #{}", fatal_len));        // pass the fatal diagnostic byte length to write()
            ctx.emitter.instruction("mov x0, #2");                              // select stderr for the uninitialized static-property fatal
            ctx.emitter.syscall(4);
            abi::emit_exit(ctx.emitter, 1);
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(ctx.emitter, "r10", "_exc_handler_top", 0);
            ctx.emitter.instruction("test r10, r10");                           // is there an active handler that can catch the Error?
            ctx.emitter.instruction(&format!("jne {}", throw_label));           // keep typed static-property errors catchable when a handler is active
            abi::emit_symbol_address(ctx.emitter, "rsi", fatal_label);          // load the specific uninitialized static-property fatal text
            ctx.emitter.instruction(&format!("mov edx, {}", fatal_len));        // pass the fatal diagnostic byte length to write()
            ctx.emitter.instruction("mov edi, 2");                              // select stderr for the uninitialized static-property fatal
            ctx.emitter.instruction("mov eax, 1");                              // select Linux write syscall
            ctx.emitter.instruction("syscall");                                 // write the specific uninitialized static-property fatal diagnostic
            abi::emit_exit(ctx.emitter, 1);
        }
    }
    ctx.emitter.label(&throw_label);
}
