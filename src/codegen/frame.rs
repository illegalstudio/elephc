//! Purpose:
//! Computes and emits stack-frame setup/teardown for the EIR backend.
//! Reuses the target-aware ABI frame helpers shared by the assembly emitter.
//!
//! Called from:
//! - `crate::codegen::block_emit`.
//!
//! Key details:
//! - Frame size is value-placement bytes plus the target frame footer, rounded to 16 bytes.
//! - Main currently exits through the process syscall used by normal executable output.
//! - Each frame stores the inherited concat-buffer offset so statement resets do not clobber
//!   `_concat_buf` slices that were passed in by the caller.

use std::collections::{HashMap, HashSet};

use crate::codegen::abi;
use crate::codegen::platform::{Arch, Target};
use crate::codegen::{
    emit_box_current_value_as_mixed, emit_write_current_string_stderr, emit_write_literal_stderr,
};
use crate::codegen_support::try_handlers::TRY_HANDLER_SLOT_SIZE;
use crate::ir::{Function, Immediate, LocalKind, LocalSlotId, Op, ValueDef, ValueId};
use crate::ir_passes::{allocate_registers, Allocation};
use crate::names::ir_global_symbol;
use crate::types::PhpType;

use super::context::FunctionContext;
use super::value_placement::{self, ValuePlacement};

const FRAME_FOOTER_BYTES: usize = 16;

/// Symbol name for the C-callable `--web` top-level handler.
///
/// Emitted as a global label on the handler body and referenced by the
/// process-entry stub when it materializes the handler address for
/// `elephc_web_run`. Keeping it as one constant guarantees the label and the
/// reference never drift.
pub(super) const WEB_HANDLER_SYMBOL: &str = "_elephc_web_handler";

/// Complete fixed frame layout for spill slots, addressable locals, and the
/// callee-saved registers the register allocator decided to use.
pub(super) struct FrameLayout {
    pub(super) value_placement: ValuePlacement,
    pub(super) local_offsets: HashMap<LocalSlotId, usize>,
    pub(super) try_handler_offsets: HashMap<i64, usize>,
    pub(super) concat_base_offset: usize,
    pub(super) frame_size: usize,
    pub(super) allocation: Allocation,
    pub(super) callee_saved_offsets: Vec<(&'static str, usize)>,
}

/// Computes the register allocation and fixed stack slots for a function.
///
/// Every SSA value keeps a spill slot (register-allocated values simply leave
/// theirs unused), and each callee-saved register the allocator uses gets a
/// dedicated save slot so the prologue/epilogue can preserve it. When
/// `regalloc_linear` is false the allocation is all-spilled, reproducing the
/// original stack-only behavior.
pub(super) fn layout_for_function(
    function: &Function,
    target: Target,
    regalloc_linear: bool,
) -> FrameLayout {
    let allocation = if regalloc_linear {
        allocate_registers(function, target)
    } else {
        Allocation::all_spilled()
    };

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
    let mut callee_saved_offsets = Vec::new();
    for reg in allocation.used_callee_saved() {
        offset += 8;
        callee_saved_offsets.push((*reg, offset));
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
        allocation,
        callee_saved_offsets,
    }
}

/// Saves the callee-saved registers the allocator used into their reserved
/// frame slots, preserving the caller's values for the function's lifetime.
fn emit_callee_saved_saves(ctx: &mut FunctionContext<'_>) {
    if ctx.callee_saved_offsets.is_empty() {
        return;
    }
    ctx.emitter
        .comment("save callee-saved registers used by the register allocator");
    for (reg, offset) in ctx.callee_saved_offsets.clone() {
        abi::store_at_offset(ctx.emitter, reg, offset);
    }
}

/// Restores the callee-saved registers saved by `emit_callee_saved_saves`,
/// returning the caller's values before frame teardown.
fn emit_callee_saved_restores(ctx: &mut FunctionContext<'_>) {
    if ctx.callee_saved_offsets.is_empty() {
        return;
    }
    ctx.emitter
        .comment("restore callee-saved registers used by the register allocator");
    for (reg, offset) in ctx.callee_saved_offsets.clone() {
        abi::load_at_offset(ctx.emitter, reg, offset);
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
    emit_callee_saved_saves(ctx);
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
) -> crate::codegen::Result<()> {
    if ctx.emitter.target.arch == Arch::AArch64 {
        ctx.emitter.raw(".align 2");
    }
    ctx.emitter.blank();
    ctx.emitter.label_global(entry_label);
    abi::emit_frame_prologue(ctx.emitter, ctx.frame_size);
    capture_concat_base(ctx);
    emit_callee_saved_saves(ctx);
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
///
/// The top-level body emits this epilogue INLINE at every `return` terminator
/// (it has no shared epilogue label to jump to, unlike user functions). It must
/// therefore emit a full self-contained epilogue on EVERY call — a one-shot guard
/// would leave all but the first `return` falling through into later blocks. The
/// trailing caller in `block_emit` is already gated on `!epilogue_emitted`, so the
/// final epilogue is still emitted at most once when the body has no `return`.
pub(super) fn emit_main_epilogue(ctx: &mut FunctionContext<'_>) {
    ctx.emitter.blank();
    ctx.emitter.comment("epilogue + exit(0)");
    emit_main_local_epilogue_cleanup(ctx);
    emit_main_static_local_cleanup(ctx);
    emit_main_global_epilogue_cleanup(ctx);
    emit_callee_saved_restores(ctx);
    abi::emit_frame_restore(ctx.emitter, ctx.frame_size);
    if ctx.gc_stats {
        emit_gc_stats(ctx);
    }
    if ctx.heap_debug {
        ctx.emitter
            .comment("heap-debug: print allocator summary and leak report to stderr");
        abi::emit_call_label(ctx.emitter, "__rt_heap_debug_report");
    }
    abi::emit_exit(ctx.emitter, 0);
    ctx.epilogue_emitted = true;
}

/// Releases initialized function static locals before process-exit diagnostics.
fn emit_main_static_local_cleanup(ctx: &mut FunctionContext<'_>) {
    let static_locals = ctx.data.static_locals().to_vec();
    for record in static_locals {
        let ty = record.php_type.codegen_repr();
        if !(matches!(ty, PhpType::Str | PhpType::Callable) || ty.is_refcounted()) {
            continue;
        }
        let done = ctx.next_label("static_local_cleanup_done");
        ctx.emitter
            .comment(&format!("epilogue cleanup static local {}", record.symbol));
        abi::emit_load_symbol_to_reg(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            &record.init_symbol,
            0,
        );
        abi::emit_branch_if_int_result_zero(ctx.emitter, &done);
        emit_static_symbol_value_cleanup(ctx, &record.symbol, &ty);
        abi::emit_store_zero_to_symbol(ctx.emitter, &record.symbol, 0);
        abi::emit_store_zero_to_symbol(ctx.emitter, &record.symbol, 8);
        abi::emit_store_zero_to_symbol(ctx.emitter, &record.init_symbol, 0);
        ctx.emitter.label(&done);
    }
}

/// Releases global symbol storage owned by the top-level EIR body before diagnostics.
fn emit_main_global_epilogue_cleanup(ctx: &mut FunctionContext<'_>) {
    let globals = ctx.module.data.global_names.clone();
    for name in globals {
        if ctx.module.extern_globals.contains_key(&name) {
            continue;
        }
        let ty = if crate::superglobals::is_superglobal(&name) {
            crate::superglobals::superglobal_type().codegen_repr()
        } else {
            PhpType::Mixed
        };
        if !cleanup_tracked_codegen_type(&ty) {
            continue;
        }
        let symbol = ir_global_symbol(&name);
        if !ctx.data.has_comm(&symbol) {
            continue;
        }
        ctx.emitter.comment(&format!("epilogue cleanup global ${}", name));
        emit_static_symbol_value_cleanup(ctx, &symbol, &ty);
        abi::emit_store_zero_to_symbol(ctx.emitter, &symbol, 0);
        if ty == PhpType::Str {
            abi::emit_store_zero_to_symbol(ctx.emitter, &symbol, 8);
        }
    }
}

/// Releases the refcounted value stored in a static-local symbol.
fn emit_static_symbol_value_cleanup(ctx: &mut FunctionContext<'_>, symbol: &str, ty: &PhpType) {
    match ty {
        PhpType::Str => {
            abi::emit_load_symbol_to_reg(ctx.emitter, abi::int_result_reg(ctx.emitter), symbol, 0);
            abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
        }
        PhpType::Callable => {
            abi::emit_load_symbol_to_result(ctx.emitter, symbol, ty);
            abi::emit_decref_if_refcounted(ctx.emitter, ty);
        }
        other if other.is_refcounted() => {
            abi::emit_load_symbol_to_result(ctx.emitter, symbol, other);
            abi::emit_decref_if_refcounted(ctx.emitter, other);
        }
        _ => {}
    }
}

/// Emits the C-callable `--web` top-level handler prologue.
///
/// Mirrors `emit_main_prologue` but labels the body `_elephc_web_handler` (a
/// C-ABI `extern "C" fn()`) and never stores argc/argv. At `handler()` entry
/// those registers are not the OS-provided values — the process-entry stub
/// stores them to `_global_argc`/`_global_argv` once before calling the bridge,
/// so the handler must not overwrite them. Consequently `$argc`/`$argv` are not
/// populated inside a `--web` top-level body in Phase 1 (acceptable for echo).
pub(super) fn emit_web_handler_prologue(ctx: &mut FunctionContext<'_>) {
    if ctx.emitter.target.arch == Arch::AArch64 {
        ctx.emitter.raw(".align 2");
    }
    ctx.emitter.blank();
    ctx.emitter.label_global(WEB_HANDLER_SYMBOL);
    abi::emit_frame_prologue(ctx.emitter, ctx.frame_size);
    // Reset all process-persistent state (function static locals, refcounted
    // static property values, and `_concat_off`) BEFORE this frame captures the
    // concat base and BEFORE the body's re-run static/enum initializers, so each
    // request sees clean state. `__rt_web_reset` is generated per program after
    // every function is emitted; the call here forward-references its label.
    ctx.emitter.comment("reset per-request persistent state");
    abi::emit_call_label(ctx.emitter, "__rt_web_reset");
    capture_concat_base(ctx);
    emit_callee_saved_saves(ctx);
    zero_initialize_main_cleanup_locals(ctx);
    zero_initialize_ref_cell_owner_locals(ctx);
    zero_initialize_eval_context_locals(ctx);
    zero_initialize_eval_scope_locals(ctx);
}

/// Emits the `--web` top-level handler epilogue and returns to the bridge.
///
/// Like `emit_main_epilogue` it runs the per-request main local cleanup (so
/// owned refcounted top-level locals are released each request) and restores the
/// frame, but it `ret`s instead of exiting. Requested gc-stats are emitted after
/// cleanup for every request; process-end heap-debug diagnostics remain skipped.
pub(super) fn emit_web_handler_epilogue(ctx: &mut FunctionContext<'_>) {
    ctx.emitter.blank();
    ctx.emitter.comment("web handler epilogue + ret");
    emit_main_local_epilogue_cleanup(ctx);
    // Under `--web` the handler returns to the bridge server loop instead of
    // exiting, so the exit-based main epilogue (where `--gc-stats` normally
    // prints) is never reached. Emitting the counters here, once per request,
    // is the only way to observe them in server mode.
    if ctx.gc_stats {
        emit_gc_stats(ctx);
    }
    emit_callee_saved_restores(ctx);
    abi::emit_frame_restore(ctx.emitter, ctx.frame_size);
    abi::emit_return(ctx.emitter);
    ctx.epilogue_emitted = true;
}

/// Emits the `--web` process-entry stub that drives the bridge server entry.
///
/// The stub is the real process entry (`_main`/`main`). It stores the OS argc/argv
/// to globals once, loads them plus the handler address into the first three
/// C-ABI integer argument registers, calls `elephc_web_run(argc, argv, &handler)`,
/// and exits the process with the bridge's integer return value. The handler
/// address (arg 2) is materialized last so a destination-register page load on
/// AArch64 cannot clobber the already-loaded argc/argv argument registers.
pub(super) fn emit_web_entry_stub(ctx: &mut FunctionContext<'_>) {
    let target = ctx.emitter.target;
    if target.arch == Arch::AArch64 {
        ctx.emitter.raw(".align 2");
    }
    ctx.emitter.blank();
    ctx.emitter
        .comment("--web process entry: call elephc_web_run(argc, argv, &handler)");
    ctx.emitter.entry_label();
    abi::emit_frame_prologue(ctx.emitter, ctx.frame_size);
    ctx.emitter
        .comment("save argc/argv to globals for the bridge and handler");
    abi::emit_store_process_args_to_globals(ctx.emitter);
    let argc_reg = abi::int_arg_reg_name(target, 0);
    let argv_reg = abi::int_arg_reg_name(target, 1);
    let handler_reg = abi::int_arg_reg_name(target, 2);
    abi::emit_load_symbol_to_reg(ctx.emitter, argc_reg, "_global_argc", 0);
    abi::emit_load_symbol_to_reg(ctx.emitter, argv_reg, "_global_argv", 0);
    abi::emit_symbol_address(ctx.emitter, handler_reg, WEB_HANDLER_SYMBOL);
    // `elephc_web_run` is a `#[no_mangle] extern "C"` Rust symbol in the bridge
    // staticlib, so it carries the platform's C-ABI underscore: resolve it through
    // `extern_symbol` (`_elephc_web_run` on macOS, `elephc_web_run` on Linux).
    let bridge_entry = target.extern_symbol("elephc_web_run");
    abi::emit_call_label(ctx.emitter, &bridge_entry);
    abi::emit_exit_with_result_reg(ctx.emitter);
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
        .filter(|local| local_kind_needs_epilogue_cleanup(local.kind))
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
        ctx.emitter
            .comment(&format!("epilogue cleanup ref-cell owner ${}", name));
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
        ctx.emitter
            .instruction(&format!("mov {}, {}", arg_reg, result_reg)); // pass the persistent eval scope handle to the free helper
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
        inst.op == Op::StoreLocal
            && matches!(inst.immediate, Some(Immediate::LocalSlot(candidate)) if candidate == slot)
    })
}

/// Returns PHP-visible locals whose slot is rewritten to a ref-cell pointer.
fn promoted_ref_cell_local_slots(function: &Function) -> HashSet<LocalSlotId> {
    let mut slots = function
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
        .collect::<HashSet<_>>();
    slots.extend(closure_ref_capture_local_slots(function));
    slots
}

/// Returns local slots whose value is captured by reference into a closure descriptor.
fn closure_ref_capture_local_slots(function: &Function) -> HashSet<LocalSlotId> {
    function
        .instructions
        .iter()
        .filter(|inst| inst.op == Op::ClosureCapture)
        .filter(|inst| inst.immediate == Some(Immediate::I64(1)))
        .filter_map(|inst| inst.operands.first().copied())
        .filter_map(|value| loaded_local_slot(function, value))
        .collect()
}

/// Resolves a lowered local read value back to its source slot.
fn loaded_local_slot(function: &Function, value: ValueId) -> Option<LocalSlotId> {
    let value = function.value(value)?;
    let ValueDef::Instruction { inst, .. } = value.def else {
        return None;
    };
    let inst = function.instruction(inst)?;
    match (inst.op, inst.immediate.as_ref()) {
        (Op::LoadLocal | Op::LoadRefCell, Some(Immediate::LocalSlot(slot))) => Some(*slot),
        _ => None,
    }
}

/// Releases a string local through the validating heap-free helper.
///
/// `__rt_heap_free_safe` skips non-heap pointers (null for uninitialized locals,
/// .rodata, out-of-range) and frees plausible live heap blocks, so it safely handles
/// the zero-length owned strings that `__rt_str_persist` now allocates. The previous
/// `cbz len` guard skipped them and leaked every owned empty string at scope exit.
fn emit_main_string_cleanup(ctx: &mut FunctionContext<'_>, offset: usize) {
    let (ptr_reg, _) = abi::string_result_regs(ctx.emitter);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, ptr_reg, offset);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("mov {}, {}", result_reg, ptr_reg)); // pass the local string pointer to the validating heap-free helper
            abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
        }
        Arch::X86_64 => {
            if ptr_reg != result_reg {
                ctx.emitter
                    .instruction(&format!("mov {}, {}", result_reg, ptr_reg)); // pass the local string pointer to the validating heap-free helper
            }
            abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
        }
    }
}

/// Releases a refcounted local when the slot contains a non-null heap pointer.
fn emit_main_refcounted_cleanup(ctx: &mut FunctionContext<'_>, offset: usize, ty: &PhpType) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let done = ctx.next_label("main_refcounted_cleanup_done");
    abi::load_at_offset(ctx.emitter, result_reg, offset);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cbz {}, {}", result_reg, done)); // skip uninitialized refcounted locals
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("test {}, {}", result_reg, result_reg)); // check whether the refcounted local is initialized
            ctx.emitter.instruction(&format!("je {}", done));                   // skip uninitialized refcounted locals
        }
    }
    abi::emit_decref_if_refcounted(ctx.emitter, ty);
    ctx.emitter.label(&done);
}

/// Zero-initializes function locals that may be released by the shared epilogue.
fn zero_initialize_function_cleanup_locals(ctx: &mut FunctionContext<'_>) {
    for (_, _, ty, offset) in function_cleanup_locals(ctx, None) {
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

/// Releases owned function locals that do not transfer ownership to this return path.
fn emit_function_local_epilogue_cleanup(
    ctx: &mut FunctionContext<'_>,
    skip_return_slot: Option<LocalSlotId>,
) {
    let cleanup_locals = function_cleanup_locals(ctx, skip_return_slot);
    let ref_cell_owners = ref_cell_owner_locals(ctx);
    let eval_scopes = eval_scope_locals(ctx);
    let eval_contexts = eval_context_locals(ctx);
    if cleanup_locals.is_empty()
        && ref_cell_owners.is_empty()
        && eval_scopes.is_empty()
        && eval_contexts.is_empty()
    {
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
    for (name, offset) in eval_scopes {
        ctx.emitter.comment(&format!("epilogue cleanup {}", name));
        emit_eval_scope_cleanup(ctx, offset);
    }
    for (name, offset) in eval_contexts {
        ctx.emitter.comment(&format!("epilogue cleanup {}", name));
        emit_eval_context_cleanup(ctx, offset);
    }
    if preserves_return {
        pop_return_value(ctx, &return_ty);
    }
}

/// Returns function local slots that receive owned refcounted values through `StoreLocal`.
///
/// `skip_return_slot` excludes the one local whose refcounted owner is transferred by the
/// current return terminator. It is deliberately path-local: another `return` in the same
/// function may return a scalar or a different value and must still release this slot.
fn function_cleanup_locals(
    ctx: &FunctionContext<'_>,
    skip_return_slot: Option<LocalSlotId>,
) -> Vec<(String, LocalSlotId, PhpType, usize)> {
    let param_names = ctx
        .function
        .params
        .iter()
        .map(|param| param.name.as_str())
        .collect::<HashSet<_>>();
    let mut locals = ctx
        .function
        .locals
        .iter()
        .filter(|local| local_kind_needs_epilogue_cleanup(local.kind))
        .filter(|local| !promoted_ref_cell_local_slots(ctx.function).contains(&local.id))
        .filter(|local| {
            local
                .name
                .as_deref()
                .is_none_or(|name| !param_names.contains(name))
        })
        .filter(|local| Some(local.id) != skip_return_slot)
        .filter(|local| {
            local_slot_has_store(ctx.function, local.id) || function_has_eval_scope(ctx.function)
        })
        .filter_map(|local| {
            let ty = local.php_type.codegen_repr();
            if !cleanup_tracked_codegen_type(&ty) {
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

/// Returns whether a local kind can own values through ordinary `StoreLocal`.
fn local_kind_needs_epilogue_cleanup(kind: LocalKind) -> bool {
    matches!(
        kind,
        LocalKind::PhpLocal
            | LocalKind::HiddenTemp
            | LocalKind::OwnedTemp
            | LocalKind::NamedArgTemp
    )
}

/// Returns the local slot whose cleanup this return path must skip, if ownership is transferred.
pub(super) fn return_cleanup_skip_slot(function: &Function, value: ValueId) -> Option<LocalSlotId> {
    let result_ty = function.value(value)?.php_type.codegen_repr();
    let return_ty = function.return_php_type.codegen_repr();
    let mut visited = HashSet::new();
    return_cleanup_skip_slot_inner(function, value, &result_ty, &return_ty, &mut visited)
}

/// Recursively traces forwarding return values back to the owned local they transfer.
fn return_cleanup_skip_slot_inner(
    function: &Function,
    value: ValueId,
    result_ty: &PhpType,
    return_ty: &PhpType,
    visited: &mut HashSet<ValueId>,
) -> Option<LocalSlotId> {
    if !visited.insert(value) {
        return None;
    }
    let value_ref = function.value(value)?;
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return None;
    };
    let inst = function.instruction(inst)?;
    match inst.op {
        Op::LoadLocal => {
            let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
                return None;
            };
            let local_ty = local_codegen_type(function, slot)?;
            if local_load_transfers_stored_owner(&local_ty, result_ty)
                && return_preserves_result_owner(result_ty, return_ty)
            {
                Some(slot)
            } else {
                None
            }
        }
        Op::ArrayToMixed | Op::HashToMixed => {
            let source = *inst.operands.first()?;
            let slot = direct_return_local_slot_inner(function, source, visited)?;
            let local_ty = local_codegen_type(function, slot)?;
            if cleanup_tracked_codegen_type(&local_ty)
                && return_preserves_result_owner(result_ty, return_ty)
            {
                Some(slot)
            } else {
                None
            }
        }
        Op::Move | Op::Borrow => {
            let source = *inst.operands.first()?;
            return_cleanup_skip_slot_inner(function, source, result_ty, return_ty, visited)
        }
        _ => None,
    }
}

/// Recursively traces forwarding values to the local slot that backs them.
fn direct_return_local_slot_inner(
    function: &Function,
    value: crate::ir::ValueId,
    visited: &mut HashSet<ValueId>,
) -> Option<LocalSlotId> {
    if !visited.insert(value) {
        return None;
    }
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
            direct_return_local_slot_inner(function, source, visited)
        }
        Op::Move | Op::Borrow => {
            let source = *inst.operands.first()?;
            direct_return_local_slot_inner(function, source, visited)
        }
        _ => None,
    }
}

/// Returns a local slot's codegen PHP type.
fn local_codegen_type(function: &Function, slot: LocalSlotId) -> Option<PhpType> {
    function
        .locals
        .get(slot.as_raw() as usize)
        .filter(|local| local.id == slot)
        .map(|local| local.php_type.codegen_repr())
}

/// Returns true when a codegen type carries refcounted ownership to release or transfer.
fn cleanup_tracked_codegen_type(ty: &PhpType) -> bool {
    matches!(ty, PhpType::Str | PhpType::Callable) || ty.is_refcounted()
}

/// Returns true when loading a local into an SSA result leaves the same owner in the result.
fn local_load_transfers_stored_owner(local_ty: &PhpType, result_ty: &PhpType) -> bool {
    if !cleanup_tracked_codegen_type(local_ty) {
        return false;
    }
    if local_ty == result_ty {
        return true;
    }
    matches!(
        (local_ty, result_ty),
        (PhpType::Array(_), PhpType::Array(_))
            | (PhpType::AssocArray { .. }, PhpType::AssocArray { .. })
    )
}

/// Returns true when final return lowering preserves the loaded refcounted result owner.
fn return_preserves_result_owner(result_ty: &PhpType, return_ty: &PhpType) -> bool {
    if !cleanup_tracked_codegen_type(result_ty) || !cleanup_tracked_codegen_type(return_ty) {
        return false;
    }
    if result_ty == return_ty {
        return true;
    }
    matches!(
        (result_ty, return_ty),
        (PhpType::Array(_), PhpType::Array(_))
            | (PhpType::AssocArray { .. }, PhpType::AssocArray { .. })
    )
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
    ctx.emitter
        .comment("gc-stats: print allocation statistics to stderr");
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

/// Emits a path-specific epilogue for one user-function return terminator.
pub(super) fn emit_function_return_epilogue(
    ctx: &mut FunctionContext<'_>,
    skip_return_slot: Option<LocalSlotId>,
) {
    emit_function_local_epilogue_cleanup(ctx, skip_return_slot);
    emit_callee_saved_restores(ctx);
    abi::emit_frame_restore(ctx.emitter, ctx.frame_size);
    abi::emit_return(ctx.emitter);
}

/// Emits the shared epilogue for a direct-callable user function.
pub(super) fn emit_function_epilogue(ctx: &mut FunctionContext<'_>) {
    if ctx.epilogue_emitted {
        return;
    }
    let label = ctx
        .epilogue_label
        .clone()
        .expect("codegen bug: user function has no epilogue label");
    ctx.emitter.label(&label);
    emit_function_local_epilogue_cleanup(ctx, None);
    emit_callee_saved_restores(ctx);
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
    ctx.module.required_runtime_features.eval_bridge
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
