//! Purpose:
//! Computes and emits stack-frame setup/teardown for the EIR backend.
//! Reuses the target-aware ABI frame helpers from the legacy backend.
//!
//! Called from:
//! - `crate::codegen_ir::block_emit`.
//!
//! Key details:
//! - Frame size is value-placement bytes plus the target frame footer, rounded to 16 bytes.
//! - Main currently exits through the process syscall, matching the legacy entry path.
//! - Each frame stores the inherited concat-buffer offset so statement resets do not clobber
//!   `_concat_buf` slices that were passed in by the caller.

use std::collections::{HashMap, HashSet};

use crate::codegen::abi;
use crate::codegen::{
    emit_box_current_value_as_mixed, emit_write_current_string_stderr,
    emit_write_literal_stderr,
};
use crate::codegen::context::TRY_HANDLER_SLOT_SIZE;
use crate::codegen::platform::Arch;
use crate::ir::{Function, Immediate, LocalKind, LocalSlotId, Op, Terminator, ValueDef};
use crate::names::ir_global_symbol;
use crate::types::PhpType;

use super::context::FunctionContext;
use super::value_placement::{self, ValuePlacement};

const FRAME_FOOTER_BYTES: usize = 16;

/// Complete fixed frame layout for Phase 04 spill slots and addressable locals.
pub(super) struct FrameLayout {
    pub(super) value_placement: ValuePlacement,
    pub(super) local_offsets: HashMap<LocalSlotId, usize>,
    pub(super) try_handler_offsets: HashMap<i64, usize>,
    pub(super) concat_base_offset: usize,
    pub(super) frame_size: usize,
}

/// Computes fixed stack slots for SSA values and EIR locals.
pub(super) fn layout_for_function(function: &Function) -> FrameLayout {
    let value_placement = value_placement::allocate(function);
    let mut local_offsets = HashMap::new();
    let mut offset = value_placement.total_slot_bytes;
    for local in &function.locals {
        let bytes = value_placement::bytes_for(local.ir_type)
            .max(local.php_type.codegen_repr().stack_size());
        if bytes == 0 {
            continue;
        }
        offset += bytes;
        local_offsets.insert(local.id, offset);
    }
    let mut try_handler_offsets = HashMap::new();
    for token in try_handler_tokens(function) {
        offset += TRY_HANDLER_SLOT_SIZE;
        try_handler_offsets.insert(token, offset);
    }
    offset += 8;
    let concat_base_offset = offset;
    let frame_size = align_to_16(offset + FRAME_FOOTER_BYTES);
    FrameLayout {
        value_placement,
        local_offsets,
        try_handler_offsets,
        concat_base_offset,
        frame_size,
    }
}

/// Returns the unique try-handler tokens used by EIR `try_push_handler` opcodes.
fn try_handler_tokens(function: &Function) -> Vec<i64> {
    let mut tokens = Vec::new();
    for inst in &function.instructions {
        if inst.op != Op::TryPushHandler {
            continue;
        }
        let Some(Immediate::I64(token)) = inst.immediate else {
            continue;
        };
        if !tokens.contains(&token) {
            tokens.push(token);
        }
    }
    tokens
}

/// Emits the process-entry prologue for the EIR main function.
pub(super) fn emit_main_prologue(ctx: &mut FunctionContext<'_>) {
    if ctx.emitter.target.arch == Arch::AArch64 {
        ctx.emitter.raw(".align 2");
    }
    ctx.emitter.blank();
    ctx.emitter.entry_label();
    abi::emit_frame_prologue(ctx.emitter, ctx.frame_size);
    capture_concat_base(ctx);
    ctx.emitter.comment("save argc/argv to globals");
    abi::emit_store_process_args_to_globals(ctx.emitter);
    if ctx.heap_debug {
        ctx.emitter.comment("enable heap debug flag");
        abi::emit_enable_heap_debug_flag(ctx.emitter);
    }
    zero_initialize_main_cleanup_locals(ctx);
    zero_initialize_ref_cell_owner_locals(ctx);
    zero_initialize_eval_context_locals(ctx);
    zero_initialize_eval_scope_locals(ctx);
    store_argc_global_if_needed(ctx);
    store_argv_global_if_needed(ctx);
    store_argc_local_if_present(ctx);
    store_argv_local_if_present(ctx);
}

/// Emits a callable function prologue using an already-resolved entry label.
pub(super) fn emit_function_prologue_with_label(
    ctx: &mut FunctionContext<'_>,
    entry_label: &str,
) -> crate::codegen_ir::Result<()> {
    if ctx.emitter.target.arch == Arch::AArch64 {
        ctx.emitter.raw(".align 2");
    }
    ctx.emitter.blank();
    ctx.emitter.label_global(entry_label);
    abi::emit_frame_prologue(ctx.emitter, ctx.frame_size);
    capture_concat_base(ctx);
    let mut incoming_args = abi::IncomingArgCursor::for_target(ctx.emitter.target, 0);
    for (index, param) in ctx.function.params.iter().enumerate() {
        let slot = LocalSlotId::from_raw(index as u32);
        let offset = ctx.local_offset(slot)?;
        abi::emit_store_incoming_param(
            ctx.emitter,
            &param.name,
            &param.php_type,
            offset,
            param.by_ref,
            &mut incoming_args,
        );
        let local_ty = ctx.local_php_type(slot)?;
        if !param.by_ref
            && local_ty.codegen_repr() == PhpType::Mixed
            && param.php_type.codegen_repr() != PhpType::Mixed
        {
            abi::emit_load(ctx.emitter, &param.php_type.codegen_repr(), offset);
            emit_box_current_value_as_mixed(ctx.emitter, &param.php_type.codegen_repr());
            abi::emit_store(ctx.emitter, &PhpType::Mixed, offset);
        }
    }
    zero_initialize_function_cleanup_locals(ctx);
    zero_initialize_ref_cell_owner_locals(ctx);
    zero_initialize_eval_context_locals(ctx);
    zero_initialize_eval_scope_locals(ctx);
    Ok(())
}

/// Captures the caller-visible concat-buffer offset as this frame's reset base.
fn capture_concat_base(ctx: &mut FunctionContext<'_>) {
    let scratch = abi::temp_int_reg(ctx.emitter.target);
    abi::emit_load_symbol_to_reg(ctx.emitter, scratch, "_concat_off", 0);
    abi::store_at_offset(ctx.emitter, scratch, ctx.concat_base_offset);
}

/// Emits frame teardown and exits the process with status 0.
pub(super) fn emit_main_epilogue(ctx: &mut FunctionContext<'_>) {
    if ctx.epilogue_emitted {
        return;
    }
    ctx.emitter.blank();
    ctx.emitter.comment("epilogue + exit(0)");
    emit_main_local_epilogue_cleanup(ctx);
    abi::emit_frame_restore(ctx.emitter, ctx.frame_size);
    if ctx.gc_stats {
        emit_gc_stats(ctx);
    }
    if ctx.heap_debug {
        ctx.emitter.comment("heap-debug: print allocator summary and leak report to stderr");
        abi::emit_call_label(ctx.emitter, "__rt_heap_debug_report");
    }
    abi::emit_exit(ctx.emitter, 0);
    ctx.epilogue_emitted = true;
}

/// Zero-initializes cleanup-tracked locals so skipped assignments stay safe at epilogue.
fn zero_initialize_main_cleanup_locals(ctx: &mut FunctionContext<'_>) {
    for (_, _, ty, offset) in main_cleanup_locals(ctx) {
        match ty {
            PhpType::Str => {
                abi::emit_store_zero_to_local_slot(ctx.emitter, offset);
                abi::emit_store_zero_to_local_slot(ctx.emitter, offset - 8);
            }
            _ => {
                abi::emit_store_zero_to_local_slot(ctx.emitter, offset);
            }
        }
    }
}

/// Releases owned main locals that still hold refcounted storage at process exit.
fn emit_main_local_epilogue_cleanup(ctx: &mut FunctionContext<'_>) {
    emit_ref_cell_owner_epilogue_cleanup(ctx);
    for (name, _, ty, offset) in main_cleanup_locals(ctx) {
        ctx.emitter.comment(&format!("epilogue cleanup ${}", name));
        match ty {
            PhpType::Str => emit_main_string_cleanup(ctx, offset),
            PhpType::Callable => emit_main_refcounted_cleanup(ctx, offset, &ty),
            other if other.is_refcounted() => emit_main_refcounted_cleanup(ctx, offset, &other),
            _ => {}
        }
    }
    emit_eval_scope_epilogue_cleanup(ctx);
    emit_eval_context_epilogue_cleanup(ctx);
}

/// Returns main local slots that receive owned refcounted values through `StoreLocal`.
fn main_cleanup_locals(ctx: &FunctionContext<'_>) -> Vec<(String, LocalSlotId, PhpType, usize)> {
    let param_names = ctx
        .function
        .params
        .iter()
        .map(|param| param.name.as_str())
        .collect::<std::collections::HashSet<_>>();
    let mut locals = ctx
        .function
        .locals
        .iter()
        .filter(|local| local.kind == LocalKind::PhpLocal)
        .filter(|local| !promoted_ref_cell_local_slots(ctx.function).contains(&local.id))
        .filter(|local| {
            local
                .name
                .as_deref()
                .is_none_or(|name| !param_names.contains(name))
        })
        .filter(|local| {
            local_slot_has_store(ctx.function, local.id) || function_has_eval_scope(ctx.function)
        })
        .filter_map(|local| {
            let ty = local.php_type.codegen_repr();
            if !(matches!(ty, PhpType::Str | PhpType::Callable) || ty.is_refcounted()) {
                return None;
            }
            let offset = ctx.local_offset(local.id).ok()?;
            let name = local
                .name
                .clone()
                .unwrap_or_else(|| format!("slot{}", local.id.as_raw()));
            Some((name, local.id, ty, offset))
        })
        .collect::<Vec<_>>();
    locals.sort_by_key(|(_, _, _, offset)| *offset);
    locals
}

/// Zero-initializes hidden ref-cell owner slots before any fallback promotion can run.
fn zero_initialize_ref_cell_owner_locals(ctx: &mut FunctionContext<'_>) {
    for (_, _, _, offset) in ref_cell_owner_locals(ctx) {
        abi::emit_store_zero_to_local_slot(ctx.emitter, offset);
    }
}

/// Releases hidden ref-cell owner slots that still hold fallback cells at exit.
fn emit_ref_cell_owner_epilogue_cleanup(ctx: &mut FunctionContext<'_>) {
    let owners = ref_cell_owner_locals(ctx);
    emit_ref_cell_owner_epilogue_cleanup_for(ctx, owners);
}

/// Releases a precomputed set of hidden ref-cell owner slots.
fn emit_ref_cell_owner_epilogue_cleanup_for(
    ctx: &mut FunctionContext<'_>,
    owners: Vec<(String, LocalSlotId, PhpType, usize)>,
) {
    for (name, _, ty, offset) in owners {
        ctx.emitter.comment(&format!("epilogue cleanup ref-cell owner ${}", name));
        emit_ref_cell_owner_cleanup(ctx, offset, &ty);
    }
}

/// Releases the owner slot's ref-cell pointer when it is non-null, then clears the owner.
fn emit_ref_cell_owner_cleanup(ctx: &mut FunctionContext<'_>, offset: usize, ty: &PhpType) {
    let done = ctx.next_label("ref_cell_owner_cleanup_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::load_at_offset_scratch(ctx.emitter, "x9", offset, "x11");
            ctx.emitter.instruction(&format!("cbz x9, {}", done));              // skip released or never-created fallback ref-cells
            abi::emit_release_local_ref_cell(ctx.emitter, "x9", ty);
            abi::emit_store_zero_to_local_slot(ctx.emitter, offset);
        }
        Arch::X86_64 => {
            abi::load_at_offset_scratch(ctx.emitter, "r11", offset, "r10");
            ctx.emitter.instruction("test r11, r11");                           // check whether this owner still holds a fallback ref-cell
            ctx.emitter.instruction(&format!("je {}", done));                   // skip released or never-created fallback ref-cells
            abi::emit_release_local_ref_cell(ctx.emitter, "r11", ty);
            abi::emit_store_zero_to_local_slot(ctx.emitter, offset);
        }
    }
    ctx.emitter.label(&done);
}

/// Returns hidden owner locals that track promoted fallback ref-cells.
fn ref_cell_owner_locals(ctx: &FunctionContext<'_>) -> Vec<(String, LocalSlotId, PhpType, usize)> {
    let mut locals = ctx
        .function
        .locals
        .iter()
        .filter(|local| local.kind == LocalKind::RefCell)
        .filter_map(|local| {
            let offset = ctx.local_offset(local.id).ok()?;
            let name = local
                .name
                .clone()
                .unwrap_or_else(|| format!("slot{}", local.id.as_raw()));
            Some((name, local.id, local.php_type.codegen_repr(), offset))
        })
        .collect::<Vec<_>>();
    locals.sort_by_key(|(_, _, _, offset)| *offset);
    locals
}

/// Zero-initializes persistent eval scope handles before the first eval call can allocate one.
fn zero_initialize_eval_scope_locals(ctx: &mut FunctionContext<'_>) {
    for (_, offset) in eval_scope_locals(ctx) {
        abi::emit_store_zero_to_local_slot(ctx.emitter, offset);
    }
}

/// Zero-initializes persistent eval context handles before the first eval call can allocate one.
fn zero_initialize_eval_context_locals(ctx: &mut FunctionContext<'_>) {
    for (_, offset) in eval_context_locals(ctx) {
        abi::emit_store_zero_to_local_slot(ctx.emitter, offset);
    }
}

/// Releases persistent eval scopes allocated for this frame.
fn emit_eval_scope_epilogue_cleanup(ctx: &mut FunctionContext<'_>) {
    for (name, offset) in eval_scope_locals(ctx) {
        ctx.emitter.comment(&format!("epilogue cleanup {}", name));
        emit_eval_scope_cleanup(ctx, offset);
    }
}

/// Releases persistent eval contexts allocated for this frame.
fn emit_eval_context_epilogue_cleanup(ctx: &mut FunctionContext<'_>) {
    for (name, offset) in eval_context_locals(ctx) {
        ctx.emitter.comment(&format!("epilogue cleanup {}", name));
        emit_eval_context_cleanup(ctx, offset);
    }
}

/// Frees one persistent eval scope handle when it was allocated.
fn emit_eval_scope_cleanup(ctx: &mut FunctionContext<'_>, offset: usize) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let done = ctx.next_label("eval_scope_cleanup_done");
    abi::load_at_offset(ctx.emitter, result_reg, offset);
    abi::emit_branch_if_int_result_zero(ctx.emitter, &done);
    let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    if arg_reg != result_reg {
        ctx.emitter.instruction(&format!("mov {}, {}", arg_reg, result_reg));   // pass the persistent eval scope handle to the free helper
    }
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_scope_free");
    abi::emit_call_label(ctx.emitter, &symbol);
    abi::emit_store_zero_to_local_slot(ctx.emitter, offset);
    ctx.emitter.label(&done);
}

/// Frees one persistent eval context handle when it was allocated.
fn emit_eval_context_cleanup(ctx: &mut FunctionContext<'_>, offset: usize) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let done = ctx.next_label("eval_context_cleanup_done");
    abi::load_at_offset(ctx.emitter, result_reg, offset);
    abi::emit_branch_if_int_result_zero(ctx.emitter, &done);
    let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    if arg_reg != result_reg {
        ctx.emitter.instruction(&format!("mov {}, {}", arg_reg, result_reg));   // pass the persistent eval context handle to the free helper
    }
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_context_free");
    abi::emit_call_label(ctx.emitter, &symbol);
    abi::emit_store_zero_to_local_slot(ctx.emitter, offset);
    ctx.emitter.label(&done);
}

/// Returns hidden eval scope slots and their frame offsets.
fn eval_scope_locals(ctx: &FunctionContext<'_>) -> Vec<(String, usize)> {
    let mut locals = ctx
        .function
        .locals
        .iter()
        .filter(|local| matches!(local.kind, LocalKind::EvalScope | LocalKind::EvalGlobalScope))
        .filter_map(|local| {
            let offset = ctx.local_offset(local.id).ok()?;
            let name = local
                .name
                .clone()
                .unwrap_or_else(|| format!("slot{}", local.id.as_raw()));
            Some((name, offset))
        })
        .collect::<Vec<_>>();
    locals.sort_by_key(|(_, offset)| *offset);
    locals
}

/// Returns hidden eval context slots and their frame offsets.
fn eval_context_locals(ctx: &FunctionContext<'_>) -> Vec<(String, usize)> {
    let mut locals = ctx
        .function
        .locals
        .iter()
        .filter(|local| local.kind == LocalKind::EvalContext)
        .filter_map(|local| {
            let offset = ctx.local_offset(local.id).ok()?;
            let name = local
                .name
                .clone()
                .unwrap_or_else(|| format!("slot{}", local.id.as_raw()));
            Some((name, offset))
        })
        .collect::<Vec<_>>();
    locals.sort_by_key(|(_, offset)| *offset);
    locals
}

/// Returns true when the function owns a persistent eval scope local.
fn function_has_eval_scope(function: &Function) -> bool {
    function
        .locals
        .iter()
        .any(|local| matches!(local.kind, LocalKind::EvalScope | LocalKind::EvalGlobalScope))
}

/// Returns true when a local slot is written by an explicit EIR `StoreLocal`.
fn local_slot_has_store(function: &Function, slot: LocalSlotId) -> bool {
    function.instructions.iter().any(|inst| {
        inst.op == Op::StoreLocal && matches!(inst.immediate, Some(Immediate::LocalSlot(candidate)) if candidate == slot)
    })
}

/// Returns PHP-visible locals whose slot is rewritten to a ref-cell pointer.
fn promoted_ref_cell_local_slots(function: &Function) -> HashSet<LocalSlotId> {
    function
        .instructions
        .iter()
        .filter_map(|inst| match inst.immediate {
            Some(Immediate::LocalSlotPair { first, .. }) if inst.op == Op::PromoteLocalRefCell => {
                Some(first)
            }
            Some(Immediate::LocalSlotPair { first, .. }) if inst.op == Op::AliasLocalRefCell => {
                Some(first)
            }
            _ => None,
        })
        .collect()
}

/// Releases a string local when its length proves it owns heap-backed storage.
fn emit_main_string_cleanup(ctx: &mut FunctionContext<'_>, offset: usize) {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    let result_reg = abi::int_result_reg(ctx.emitter);
    let done = ctx.next_label("main_string_cleanup_done");
    abi::load_at_offset(ctx.emitter, ptr_reg, offset);
    abi::load_at_offset(ctx.emitter, len_reg, offset - 8);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz {}, {}", len_reg, done));     // skip empty or uninitialized string locals
            ctx.emitter.instruction(&format!("mov {}, {}", result_reg, ptr_reg)); // pass the owned string pointer to the heap-free helper
            abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {}, {}", len_reg, len_reg)); // check whether this string local has owned bytes
            ctx.emitter.instruction(&format!("je {}", done));                   // skip empty or uninitialized string locals
            if ptr_reg != result_reg {
                ctx.emitter.instruction(&format!("mov {}, {}", result_reg, ptr_reg)); // pass the owned string pointer to the heap-free helper
            }
            abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
        }
    }
    ctx.emitter.label(&done);
}

/// Releases a refcounted local when the slot contains a non-null heap pointer.
fn emit_main_refcounted_cleanup(ctx: &mut FunctionContext<'_>, offset: usize, ty: &PhpType) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let done = ctx.next_label("main_refcounted_cleanup_done");
    abi::load_at_offset(ctx.emitter, result_reg, offset);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz {}, {}", result_reg, done));  // skip uninitialized refcounted locals
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {}, {}", result_reg, result_reg)); // check whether the refcounted local is initialized
            ctx.emitter.instruction(&format!("je {}", done));                   // skip uninitialized refcounted locals
        }
    }
    abi::emit_decref_if_refcounted(ctx.emitter, ty);
    ctx.emitter.label(&done);
}

/// Zero-initializes function locals that may be released by the shared epilogue.
fn zero_initialize_function_cleanup_locals(ctx: &mut FunctionContext<'_>) {
    for (_, _, ty, offset) in function_cleanup_locals(ctx) {
        match ty {
            PhpType::Str => {
                abi::emit_store_zero_to_local_slot(ctx.emitter, offset);
                abi::emit_store_zero_to_local_slot(ctx.emitter, offset - 8);
            }
            _ => {
                abi::emit_store_zero_to_local_slot(ctx.emitter, offset);
            }
        }
    }
}

/// Releases owned function locals that do not directly provide the return value.
fn emit_function_local_epilogue_cleanup(ctx: &mut FunctionContext<'_>) {
    let cleanup_locals = function_cleanup_locals(ctx);
    let ref_cell_owners = ref_cell_owner_locals(ctx);
    if cleanup_locals.is_empty() && ref_cell_owners.is_empty() {
        return;
    }
    let return_ty = ctx.function.return_php_type.codegen_repr();
    let preserves_return = !matches!(return_ty, PhpType::Void | PhpType::Never);
    if preserves_return {
        push_return_value(ctx, &return_ty);
    }
    emit_ref_cell_owner_epilogue_cleanup_for(ctx, ref_cell_owners);
    for (name, _, ty, offset) in cleanup_locals {
        ctx.emitter.comment(&format!("epilogue cleanup ${}", name));
        match ty {
            PhpType::Str => emit_main_string_cleanup(ctx, offset),
            PhpType::Callable => emit_main_refcounted_cleanup(ctx, offset, &ty),
            other if other.is_refcounted() => emit_main_refcounted_cleanup(ctx, offset, &other),
            _ => {}
        }
    }
    emit_eval_scope_epilogue_cleanup(ctx);
    emit_eval_context_epilogue_cleanup(ctx);
    if preserves_return {
        pop_return_value(ctx, &return_ty);
    }
}

/// Returns function local slots that receive owned refcounted values through `StoreLocal`.
fn function_cleanup_locals(ctx: &FunctionContext<'_>) -> Vec<(String, LocalSlotId, PhpType, usize)> {
    let param_names = ctx
        .function
        .params
        .iter()
        .map(|param| param.name.as_str())
        .collect::<HashSet<_>>();
    let returned_slots = direct_return_local_slots(ctx.function);
    let mut locals = ctx
        .function
        .locals
        .iter()
        .filter(|local| local.kind == LocalKind::PhpLocal)
        .filter(|local| !promoted_ref_cell_local_slots(ctx.function).contains(&local.id))
        .filter(|local| {
            local
                .name
                .as_deref()
                .is_none_or(|name| !param_names.contains(name))
        })
        .filter(|local| !returned_slots.contains(&local.id))
        .filter(|local| {
            local_slot_has_store(ctx.function, local.id) || function_has_eval_scope(ctx.function)
        })
        .filter_map(|local| {
            let ty = local.php_type.codegen_repr();
            if !(matches!(ty, PhpType::Str | PhpType::Callable) || ty.is_refcounted()) {
                return None;
            }
            let offset = ctx.local_offset(local.id).ok()?;
            let name = local
                .name
                .clone()
                .unwrap_or_else(|| format!("slot{}", local.id.as_raw()));
            Some((name, local.id, ty, offset))
        })
        .collect::<Vec<_>>();
    locals.sort_by_key(|(_, _, _, offset)| *offset);
    locals
}

/// Returns local slots whose loaded value is returned directly by any terminator.
fn direct_return_local_slots(function: &Function) -> HashSet<LocalSlotId> {
    function
        .blocks
        .iter()
        .filter_map(|block| match &block.terminator {
            Some(Terminator::Return { value: Some(value) }) => {
                direct_return_local_slot(function, *value)
            }
            _ => None,
        })
        .collect()
}

/// Returns the local slot behind a returned local or in-place converted local.
fn direct_return_local_slot(function: &Function, value: crate::ir::ValueId) -> Option<LocalSlotId> {
    let value = function.value(value)?;
    let ValueDef::Instruction { inst, .. } = value.def else {
        return None;
    };
    let inst = function.instruction(inst)?;
    match inst.op {
        Op::LoadLocal => match inst.immediate {
            Some(Immediate::LocalSlot(slot)) => Some(slot),
            _ => None,
        },
        Op::ArrayToMixed | Op::HashToMixed => {
            let source = *inst.operands.first()?;
            direct_return_local_slot(function, source)
        }
        _ => None,
    }
}

/// Preserves the current typed return value on the temporary stack.
fn push_return_value(ctx: &mut FunctionContext<'_>, ty: &PhpType) {
    match ty.codegen_repr() {
        PhpType::Float => {
            abi::emit_push_float_reg(ctx.emitter, abi::float_result_reg(ctx.emitter));
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);
        }
        PhpType::TaggedScalar => {
            abi::emit_push_reg_pair(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                crate::codegen::sentinels::tagged_scalar_tag_reg(ctx.emitter),
            );
        }
        _ => {
            abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        }
    }
}

/// Restores a typed return value preserved by `push_return_value`.
fn pop_return_value(ctx: &mut FunctionContext<'_>, ty: &PhpType) {
    match ty.codegen_repr() {
        PhpType::Float => {
            abi::emit_pop_float_reg(ctx.emitter, abi::float_result_reg(ctx.emitter));
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_pop_reg_pair(ctx.emitter, ptr_reg, len_reg);
        }
        PhpType::TaggedScalar => {
            abi::emit_pop_reg_pair(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                crate::codegen::sentinels::tagged_scalar_tag_reg(ctx.emitter),
            );
        }
        _ => {
            abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        }
    }
}

/// Emits allocation/free totals to stderr using the shared runtime counters.
fn emit_gc_stats(ctx: &mut FunctionContext<'_>) {
    ctx.emitter.comment("gc-stats: print allocation statistics to stderr");
    let (allocs_label, allocs_len) = ctx.data.add_string(b"GC: allocs=");
    emit_write_literal_stderr(ctx.emitter, &allocs_label, allocs_len);
    let int_result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_symbol_to_reg(ctx.emitter, int_result_reg, "_gc_allocs", 0);
    abi::emit_call_label(ctx.emitter, "__rt_itoa");
    emit_write_current_string_stderr(ctx.emitter);
    let (frees_label, frees_len) = ctx.data.add_string(b" frees=");
    emit_write_literal_stderr(ctx.emitter, &frees_label, frees_len);
    abi::emit_load_symbol_to_reg(ctx.emitter, int_result_reg, "_gc_frees", 0);
    abi::emit_call_label(ctx.emitter, "__rt_itoa");
    emit_write_current_string_stderr(ctx.emitter);
    let (newline_label, _) = ctx.data.add_string(b"\n");
    emit_write_literal_stderr(ctx.emitter, &newline_label, 1);
}

/// Emits the shared epilogue for a direct-callable user function.
pub(super) fn emit_function_epilogue(ctx: &mut FunctionContext<'_>) {
    if ctx.epilogue_emitted {
        return;
    }
    let label = ctx
        .epilogue_label
        .clone()
        .expect("codegen_ir bug: user function has no epilogue label");
    ctx.emitter.label(&label);
    emit_function_local_epilogue_cleanup(ctx);
    abi::emit_frame_restore(ctx.emitter, ctx.frame_size);
    abi::emit_return(ctx.emitter);
    ctx.epilogue_emitted = true;
}

/// Rounds a byte count up to a 16-byte stack alignment boundary.
fn align_to_16(bytes: usize) -> usize {
    (bytes + 15) & !15
}

/// Stores the OS argument count into `$argc` when the EIR main function has that local.
fn store_argc_local_if_present(ctx: &mut FunctionContext<'_>) {
    let Some((argc_slot, argc_ty)) = ctx
        .function
        .locals
        .iter()
        .find(|local| local.name.as_deref() == Some("argc"))
        .map(|local| (local.id, local.php_type.codegen_repr()))
    else {
        return;
    };
    let Ok(offset) = ctx.local_offset(argc_slot) else {
        return;
    };
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_symbol_to_reg(ctx.emitter, result_reg, "_global_argc", 0);
    if matches!(argc_ty, PhpType::Mixed | PhpType::Union(_)) {
        emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
    }
    abi::store_at_offset(ctx.emitter, result_reg, offset);
}

/// Builds and stores the PHP `$argv` array when the EIR main function has that local.
fn store_argv_local_if_present(ctx: &mut FunctionContext<'_>) {
    let Some((argv_slot, argv_ty)) = ctx
        .function
        .locals
        .iter()
        .find(|local| local.name.as_deref() == Some("argv"))
        .map(|local| (local.id, local.php_type.codegen_repr()))
    else {
        return;
    };
    let Ok(offset) = ctx.local_offset(argv_slot) else {
        return;
    };
    let array_ty = argv_array_type();
    ctx.emitter.comment("build $argv array from OS argv");
    abi::emit_call_label(ctx.emitter, "__rt_build_argv");
    if matches!(argv_ty, PhpType::Mixed | PhpType::Union(_)) {
        emit_box_current_value_as_mixed(ctx.emitter, &array_ty);
    }
    abi::emit_store(ctx.emitter, &argv_ty, offset);
}

/// Initializes program-global `$argc` storage for eval or static `global $argc`.
fn store_argc_global_if_needed(ctx: &mut FunctionContext<'_>) {
    if !superglobal_storage_needed(ctx, "argc") {
        return;
    }
    let symbol = ir_global_symbol("argc");
    ctx.data.add_comm(symbol.clone(), PhpType::Int.stack_size().max(8));
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_symbol_to_reg(ctx.emitter, result_reg, "_global_argc", 0);
    abi::emit_store_result_to_symbol(ctx.emitter, &symbol, &PhpType::Int, false);
}

/// Initializes program-global `$argv` storage for eval or static `global $argv`.
fn store_argv_global_if_needed(ctx: &mut FunctionContext<'_>) {
    if !superglobal_storage_needed(ctx, "argv") {
        return;
    }
    let symbol = ir_global_symbol("argv");
    let array_ty = argv_array_type();
    ctx.data.add_comm(symbol.clone(), array_ty.stack_size().max(8));
    ctx.emitter.comment("build global $argv array from OS argv");
    abi::emit_call_label(ctx.emitter, "__rt_build_argv");
    abi::emit_store_result_to_symbol(ctx.emitter, &symbol, &array_ty, false);
}

/// Returns true when a process superglobal needs program-global storage.
fn superglobal_storage_needed(ctx: &FunctionContext<'_>, name: &str) -> bool {
    ctx.module.required_runtime_features.eval
        || ctx
            .module
            .data
            .global_names
            .iter()
            .any(|candidate| candidate == name)
}

/// Returns the PHP storage type for `$argv`.
fn argv_array_type() -> PhpType {
    PhpType::Array(Box::new(PhpType::Str))
}
