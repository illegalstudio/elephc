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
use crate::codegen::emit::Emitter;
use crate::codegen::platform::{Arch, Target};
use crate::codegen::{
    emit_box_current_value_as_mixed, emit_write_current_string_stderr, emit_write_literal_stderr,
};
use crate::codegen_support::data_section::DataWord;
use crate::codegen_support::try_handlers::TRY_HANDLER_SLOT_SIZE;
use crate::ir::{Function, Immediate, LocalKind, LocalSlotId, Op, ValueDef, ValueId};
use crate::ir_passes::{allocate_registers, Allocation};
use crate::names::ir_global_symbol;
use crate::types::PhpType;

use super::context::FunctionContext;
use super::local_analysis::LocalSlotAnalysis;
use super::stack_guard;
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
    pub(super) ref_cell_state_offsets: HashMap<LocalSlotId, usize>,
    pub(super) try_handler_offsets: HashMap<i64, usize>,
    pub(super) concat_base_offset: usize,
    pub(super) frame_size: usize,
    pub(super) allocation: Allocation,
    pub(super) callee_saved_offsets: Vec<(&'static str, usize)>,
    pub(super) local_analysis: LocalSlotAnalysis,
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
    let local_analysis = LocalSlotAnalysis::new(function);
    let mut local_offsets = HashMap::new();
    let mut offset = value_placement.total_slot_bytes;
    for local in &function.locals {
        // A parameter may be widened to Mixed storage after its signature has been fixed (for
        // example, bindColumn() promotes a string parameter to a durable ref-cell). Reserve the
        // larger of the local representation and the incoming ABI representation so saving a
        // two-word string cannot overlap the preceding widened one-word local.
        let incoming_param_bytes = function
            .params
            .get(local.id.as_raw() as usize)
            .map(|param| param.php_type.codegen_repr().stack_size())
            .unwrap_or(0);
        let bytes = value_placement::bytes_for(local.ir_type)
            .max(local.php_type.codegen_repr().stack_size())
            .max(incoming_param_bytes);
        if bytes == 0 {
            continue;
        }
        offset += bytes;
        local_offsets.insert(local.id, offset);
    }
    let mut ref_cell_state_offsets = HashMap::new();
    let mut dynamic_ref_cell_slots = local_analysis.dynamic_ref_cell_slots().collect::<Vec<_>>();
    dynamic_ref_cell_slots.sort_by_key(|slot| slot.as_raw());
    for slot in dynamic_ref_cell_slots {
        offset += 8;
        ref_cell_state_offsets.insert(slot, offset);
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
    // Method/callable dispatch hand-uses the reserved nested-call register
    // (x19/r12) to hold a receiver across argument-lowering calls; it is
    // callee-saved but outside the allocator's tracking, so reserve a save slot
    // when the function needs it (issue #511).
    if function_uses_nested_call_reg(function) {
        let reg = nested_call_reg_name(target.arch);
        if !callee_saved_offsets.iter().any(|(saved, _)| *saved == reg) {
            offset += 8;
            callee_saved_offsets.push((reg, offset));
        }
    }
    offset += 8;
    let concat_base_offset = offset;
    let frame_size = align_to_16(offset + FRAME_FOOTER_BYTES);
    FrameLayout {
        value_placement,
        local_offsets,
        ref_cell_state_offsets,
        try_handler_offsets,
        concat_base_offset,
        frame_size,
        allocation,
        callee_saved_offsets,
        local_analysis,
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

/// Returns the reserved "nested call" scratch register for `arch` — the
/// callee-saved register (`x19` / `r12`) that method- and callable-dispatch
/// lowering hand-uses to hold a receiver or descriptor across argument-lowering
/// calls. It lives outside the register allocator's pools, so a function that
/// uses it must reserve its own save slot (see `layout_for_function`). Mirrors
/// `abi::nested_call_reg`, which resolves the same register from an `Emitter`.
fn nested_call_reg_name(arch: Arch) -> &'static str {
    match arch {
        Arch::AArch64 => "x19",
        Arch::X86_64 => "r12",
    }
}

/// Returns true when the function contains a method call whose receiver is
/// dispatched through the reserved nested-call register: a union receiver or a
/// receiver whose codegen representation is `Mixed`. `lower_mixed_method_call`
/// and `lower_nullable_receiver_method_call` hand-use `nested_call_reg` to hold
/// the unboxed object payload across argument-lowering calls, but that register
/// is callee-saved and outside the allocator's tracking — without a reserved
/// save slot the function silently clobbers the caller's value (issue #511: a
/// `--web` handler calling a method on a `PDOStatement|bool` receiver corrupted
/// the hyper worker's `x19`, freeing a garbage pointer during response flush).
/// A plain single non-nullable object receiver uses direct dispatch and is
/// excluded. Over-detection is harmless — an unused save/restore pair costs one
/// store and one load — so the receiver test errs toward inclusion.
fn function_uses_nested_call_reg(function: &Function) -> bool {
    function.instructions.iter().any(|inst| {
        if !matches!(inst.op, Op::MethodCall | Op::NullsafeMethodCall) {
            return false;
        }
        let Some(receiver) = inst.operands.first() else {
            return false;
        };
        let Some(value) = function.value(*receiver) else {
            return false;
        };
        matches!(value.php_type, PhpType::Union(_))
            || matches!(value.php_type.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
    })
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
    // Measure the stack only after argc/argv are safe in globals: the initializer is an
    // ordinary call and clobbers the C-ABI argument registers they arrive in. `main` itself
    // is never guarded — it is the root of every call chain and runs before the floor exists.
    stack_guard::emit_stack_limit_init_call(ctx.emitter);
    emit_probe_init(ctx);
    emit_instr_init(ctx);
    if ctx.heap_debug {
        ctx.emitter.comment("enable heap debug flag");
        abi::emit_enable_heap_debug_flag(ctx.emitter);
    }
    zero_initialize_main_cleanup_locals(ctx);
    zero_initialize_ref_cell_state_slots(ctx);
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
    // The depth check runs before anything is written to the new frame and before the
    // incoming arguments are spilled, so it only needs x9 (AArch64) / no register at all
    // (x86_64) and cannot disturb the ABI. Placing it after the frame has been reserved
    // means the compare already accounts for this function's own frame size.
    let stack_ok_label = ctx.next_label("stack_ok");
    stack_guard::emit_stack_limit_check(ctx.emitter, &stack_ok_label);
    emit_call_counter_increment(ctx, entry_label);
    capture_concat_base(ctx);
    emit_callee_saved_saves(ctx);

    // Save every incoming argument before any parameter post-processing can call
    // a runtime helper and clobber caller-saved argument registers.
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
    }

    // Once all ABI inputs are safe in their frame slots, materialize any widened
    // Mixed boxes and retain the parameters whose slots become callee-owned.
    for (index, param) in ctx.function.params.iter().enumerate() {
        let slot = LocalSlotId::from_raw(index as u32);
        let offset = ctx.local_offset(slot)?;
        let local_ty = ctx.local_php_type(slot)?;
        let converted_to_owned_mixed = !param.by_ref
            && local_ty.codegen_repr() == PhpType::Mixed
            && param.php_type.codegen_repr() != PhpType::Mixed;
        if converted_to_owned_mixed {
            abi::emit_load(ctx.emitter, &param.php_type.codegen_repr(), offset);
            emit_box_current_value_as_mixed(ctx.emitter, &param.php_type.codegen_repr());
            abi::emit_store(ctx.emitter, &PhpType::Mixed, offset);
        }
        if ctx.owns_parameter_slot(slot) && !converted_to_owned_mixed {
            retain_owned_parameter_local(ctx.emitter, offset, &local_ty);
        }
    }
    zero_initialize_function_cleanup_locals(ctx);
    zero_initialize_ref_cell_state_slots(ctx);
    zero_initialize_ref_cell_owner_locals(ctx);
    zero_initialize_eval_context_locals(ctx);
    zero_initialize_eval_scope_locals(ctx);
    // Instrument entry runs LAST in the prologue: the incoming arguments are
    // already spilled to their slots, so a call clobbering the argument/scratch
    // registers is safe. Callee-saved registers are already preserved above.
    emit_instr_enter(ctx);
    Ok(())
}

/// Retains a mutable by-value parameter so its frame slot has one callee-owned reference.
fn retain_owned_parameter_local(emitter: &mut Emitter, offset: usize, ty: &PhpType) {
    abi::emit_load(emitter, ty, offset);
    if !matches!(ty, PhpType::Str) {
        abi::emit_incref_if_refcounted(emitter, ty);
    }
    // `emit_store` performs the string persist itself; other refcounted values
    // were retained explicitly above and are stored without another incref.
    abi::emit_store(emitter, ty, offset);
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
    // Drain still-active output buffers before any teardown so user output
    // handlers (including eval-registered ones) run while locals, statics, and
    // the eval context are still alive. The exit-path flush in abi::emit_exit
    // stays as the guard for exit()/die() and fatal terminations.
    abi::emit_call_label(ctx.emitter, "__rt_ob_flush_all");
    emit_main_local_epilogue_cleanup(ctx);
    emit_main_static_local_cleanup(ctx);
    emit_main_global_epilogue_cleanup(ctx);
    emit_callee_saved_restores(ctx);
    abi::emit_frame_restore(ctx.emitter, ctx.frame_size);
    emit_probe_dump(ctx);
    if ctx.gc_stats {
        emit_gc_stats(ctx);
    }
    if ctx.shared.counters {
        emit_counters_dump(ctx);
    }
    if ctx.shared.instrument.is_on() {
        emit_instr_dump(ctx);
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
        let ty = if ctx.module.web && crate::superglobals::is_superglobal(&name) {
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
    zero_initialize_ref_cell_state_slots(ctx);
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
    emit_probe_dump(ctx);
    if ctx.gc_stats {
        emit_gc_stats(ctx);
    }
    if ctx.shared.counters {
        emit_counters_dump(ctx);
    }
    if ctx.shared.instrument.is_on() {
        emit_instr_dump(ctx);
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
pub(super) fn emit_web_entry_stub(
    ctx: &mut FunctionContext<'_>,
    isolation: super::WebIsolation,
) {
    let target = ctx.emitter.target;
    if target.arch == Arch::AArch64 {
        ctx.emitter.raw(".align 2");
    }
    ctx.emitter.blank();
    ctx.emitter.comment(&format!(
        "--web process entry: call {}(argc, argv, &handler)",
        isolation.bridge_symbol()
    ));
    ctx.emitter.entry_label();
    abi::emit_frame_prologue(ctx.emitter, ctx.frame_size);
    ctx.emitter
        .comment("save argc/argv to globals for the bridge and handler");
    abi::emit_store_process_args_to_globals(ctx.emitter);
    // `--web` forks its workers from this process and each worker serves requests on its own
    // main stack, so the floor measured here stays valid in every child. Measuring before the
    // bridge call also keeps the clobbered argument registers away from `elephc_web_run`.
    stack_guard::emit_stack_limit_init_call(ctx.emitter);
    // Install the probe here — in the MASTER, before elephc_web_run forks — so the SIGPROF
    // sampler and the shared-memory ring are inherited by every worker. Placed in the handler
    // prologue instead, it would re-init per request in each worker and never share.
    emit_probe_init(ctx);
    // The instrument name table is set once in the master before the fork, so
    // workers inherit it; each worker's thread-local counters stay its own.
    emit_instr_init(ctx);
    // Enable the small-bin double-free guard for every --web worker process: a detected
    // double free `_exit(1)`s the worker (the prefork master respawns it), containing
    // corruption to one request. Cheap — a short bin-chain scan on free, with no
    // per-allocation free-list validation. Set once here at worker startup, not per
    // request. (A `--no-web-heap-guard` opt-out for benchmarking is a follow-up.)
    ctx.emitter.comment("enable web heap-guard flag");
    abi::emit_enable_web_heap_guard_flag(ctx.emitter);
    let argc_reg = abi::int_arg_reg_name(target, 0);
    let argv_reg = abi::int_arg_reg_name(target, 1);
    let handler_reg = abi::int_arg_reg_name(target, 2);
    abi::emit_load_symbol_to_reg(ctx.emitter, argc_reg, "_global_argc", 0);
    abi::emit_load_symbol_to_reg(ctx.emitter, argv_reg, "_global_argv", 0);
    abi::emit_symbol_address(ctx.emitter, handler_reg, WEB_HANDLER_SYMBOL);
    // The selected entry is a `#[no_mangle] extern "C"` Rust symbol in the bridge
    // staticlib, so it carries the platform's C-ABI underscore: resolve it through
    // `extern_symbol` (`_elephc_web_run` on macOS, `elephc_web_run` on Linux).
    let bridge_entry = target.extern_symbol(isolation.bridge_symbol());
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
    for (name, slot, ty, offset) in main_cleanup_locals(ctx) {
        ctx.emitter.comment(&format!("epilogue cleanup ${}", name));
        emit_owned_local_cleanup(ctx, slot, offset, &ty);
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
        // Slots this frame only BORROWS (inliner-transplanted callee params, and slots whose
        // ownership moved into a return value) must never be released here: the frame never
        // acquired them. Releasing a borrow is a use-after-free, and it is what made a
        // read-only `array` param in a loop die with `heap debug detected bad refcount`.
        .filter(|local| !ctx.function.no_epilogue_cleanup_slots.contains(&local.id))
        .filter(|local| {
            !ctx.local_slot_ever_stores_ref_cell_pointer(local.id)
                || ctx.has_dynamic_ref_cell_state(local.id)
        })
        .filter(|local| {
            local
                .name
                .as_deref()
                .is_none_or(|name| !param_names.contains(name))
        })
        .filter(|local| {
            ctx.local_slot_has_store(local.id) || function_has_eval_scope(ctx.function)
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

/// Zero-initializes runtime flags for slots that may later store ref-cell pointers.
fn zero_initialize_ref_cell_state_slots(ctx: &mut FunctionContext<'_>) {
    let mut offsets = ctx
        .function
        .locals
        .iter()
        .filter_map(|local| ctx.ref_cell_state_offset(local.id))
        .collect::<Vec<_>>();
    offsets.sort_unstable();
    offsets.dedup();
    for offset in offsets {
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

/// Releases a string local through the validating heap-free helper.
///
/// `__rt_heap_free_safe` skips non-heap pointers (null for uninitialized locals,
/// .rodata, out-of-range) and frees plausible live heap blocks, so it safely handles
/// the zero-length owned strings that `__rt_str_persist` now allocates. The previous
/// `cbz len` guard skipped them and leaked every owned empty string at scope exit.
pub(super) fn emit_main_string_cleanup(ctx: &mut FunctionContext<'_>, offset: usize) {
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
pub(super) fn emit_main_refcounted_cleanup(ctx: &mut FunctionContext<'_>, offset: usize, ty: &PhpType) {
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
    for (_, slot, ty, offset) in function_cleanup_locals(ctx, None) {
        if local_slot_is_parameter(ctx.function, slot) {
            continue;
        }
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
    // Instrument exit runs FIRST — before the early return for cleanup-free
    // functions — so every return path records the exit. It preserves the
    // return value across its own call.
    emit_instr_exit(ctx);
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
    for (name, slot, ty, offset) in cleanup_locals {
        ctx.emitter.comment(&format!("epilogue cleanup ${}", name));
        emit_owned_local_cleanup(ctx, slot, offset, &ty);
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
    let mut locals = ctx
        .function
        .locals
        .iter()
        .filter(|local| local_kind_needs_epilogue_cleanup(local.kind))
        // Slots this frame only BORROWS (inliner-transplanted callee params, and slots whose
        // ownership moved into a return value) must never be released here: the frame never
        // acquired them. Releasing a borrow is a use-after-free, and it is what made a
        // read-only `array` param in a loop die with `heap debug detected bad refcount`.
        .filter(|local| !ctx.function.no_epilogue_cleanup_slots.contains(&local.id))
        .filter(|local| {
            !ctx.local_slot_ever_stores_ref_cell_pointer(local.id)
                || ctx.has_dynamic_ref_cell_state(local.id)
        })
        .filter(|local| {
            !local_slot_is_parameter(ctx.function, local.id)
                || ctx.owns_parameter_slot(local.id)
        })
        .filter(|local| Some(local.id) != skip_return_slot)
        .filter(|local| {
            ctx.local_slot_has_store(local.id)
                || ctx.owns_parameter_slot(local.id)
                || function_has_eval_scope(ctx.function)
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

/// Returns whether a local slot belongs to a function parameter.
fn local_slot_is_parameter(function: &Function, slot: LocalSlotId) -> bool {
    function.params.get(slot.as_raw() as usize).is_some()
}

/// Releases one owned raw local unless its runtime slot currently holds a ref-cell pointer.
pub(super) fn emit_owned_local_cleanup(
    ctx: &mut FunctionContext<'_>,
    slot: LocalSlotId,
    offset: usize,
    ty: &PhpType,
) {
    let done = ctx
        .ref_cell_state_offset(slot)
        .map(|_| ctx.next_label("raw_local_cleanup_done"));
    if let (Some(state_offset), Some(done)) = (ctx.ref_cell_state_offset(slot), done.as_ref()) {
        let state_reg = abi::int_result_reg(ctx.emitter);
        abi::load_at_offset(ctx.emitter, state_reg, state_offset);
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(
                    &format!("cbnz {}, {}", state_reg, done)
                );                                                              // skip raw cleanup while this slot stores a ref-cell pointer
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(
                    &format!("test {}, {}", state_reg, state_reg)
                );                                                              // test whether this slot currently stores a ref-cell pointer
                ctx.emitter
                    .instruction(&format!("jne {}", done));                      // skip raw cleanup for the ref-cell representation
            }
        }
    }
    match ty {
        PhpType::Str => emit_main_string_cleanup(ctx, offset),
        PhpType::Callable => emit_main_refcounted_cleanup(ctx, offset, ty),
        other if other.is_refcounted() => emit_main_refcounted_cleanup(ctx, offset, other),
        _ => {}
    }
    if let Some(done) = done {
        ctx.emitter.label(&done);
    }
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
/// Emits the `--probe` initialization call in main's prologue: hands the
/// embedded symbol table's address and entry count to `elephc_probe_init`,
/// which installs the SIGPROF handler and arms the profiling timer. Inert
/// unless the probe symbol table was built (i.e. `--probe`).
fn emit_probe_init(ctx: &mut FunctionContext<'_>) {
    let Some((table_label, count)) = ctx.shared.probe_table.clone() else {
        return;
    };
    ctx.emitter.comment("probe: install SIGPROF sampler (--probe)");
    let target = ctx.emitter.target;
    let table_arg = abi::int_arg_reg_name(target, 0);
    let count_arg = abi::int_arg_reg_name(target, 1);
    let key_arg = abi::int_arg_reg_name(target, 2);
    abi::emit_symbol_address(ctx.emitter, table_arg, &table_label);
    abi::emit_load_int_immediate(ctx.emitter, count_arg, count as i64);
    let key_symbol = target.extern_symbol("elephc_probe_build_key");
    abi::emit_symbol_address(ctx.emitter, key_arg, &key_symbol);
    // `elephc_probe_init` is a `#[no_mangle] extern "C"` Rust symbol in the probe
    // staticlib; resolve its platform C-ABI mangling like the web bridge entry.
    let entry = target.extern_symbol("elephc_probe_init");
    abi::emit_call_label(ctx.emitter, &entry);
    // Publish the route-tagging entry into the core runtime's `_elephc_probe_route_fn`
    // slot so the --web bridge can call it without a dlsym or a compile-time coupling.
    // This reference also keeps `elephc_probe_set_route` from being dead-stripped (it is
    // otherwise reached only through this slot). Both symbols exist only under --probe.
    let set_route = target.extern_symbol("elephc_probe_set_route");
    let scratch = abi::int_arg_reg_name(target, 0);
    abi::emit_symbol_address(ctx.emitter, scratch, &set_route);
    abi::emit_store_reg_to_symbol(ctx.emitter, scratch, &target.extern_symbol("elephc_probe_route_fn"), 0);
    // Publish the worker re-arm entry the same way: the --web bridge calls it to
    // restore the profiling timer the post-fork disarm turned off.
    let rearm = target.extern_symbol("elephc_probe_rearm");
    abi::emit_symbol_address(ctx.emitter, scratch, &rearm);
    abi::emit_store_reg_to_symbol(ctx.emitter, scratch, &target.extern_symbol("elephc_probe_rearm_fn"), 0);
    let verify = target.extern_symbol("elephc_probe_verify_query");
    abi::emit_symbol_address(ctx.emitter, scratch, &verify);
    abi::emit_store_reg_to_symbol(
        ctx.emitter,
        scratch,
        &target.extern_symbol("elephc_probe_verify_fn"),
        0,
    );
    // Claim the I/O event slots the PDO bridge calls through, so a `--probe`
    // binary reports EXACT query counts and I/O wait per route. Those events are
    // not sampled — a driver call fires exactly one — and they cost an atomic
    // increment paid only when a query happens, so unlike per-call
    // instrumentation this is affordable in production.
    //
    // `--instrument` wins when both are linked: it attributes the same events per
    // FUNCTION through its shadow stack, which strictly refines per-route. Two
    // writers to one slot would otherwise leave the winner to emission order.
    // A marker `monitor` can find by reading the file, before it runs anything.
    // Detection has to work from OUTSIDE the process — the whole point is to tell
    // a user "this binary cannot be monitored, rebuild it" instead of launching
    // it and reporting an empty profile, which reads as "your program is fast".
    // A named global rather than an anonymous string, so nothing strips it and
    // `strings` shows a human why it is there.
    ctx.data.add_named_symbol(
        target.extern_symbol("elephc_monitoring_marker"),
        b"elephc-monitoring-v1 (built with --with-monitoring)\0",
    );
    // Hand the sampler the address of the allocation counter, so it can attribute
    // allocation deltas to the stack it samples — the same shape as Go's heap
    // profile. A pointer rather than the symbol, so `_gc_allocs` stays a name only
    // the emitted assembly resolves.
    abi::emit_symbol_address(ctx.emitter, scratch, "_gc_allocs");
    abi::emit_store_reg_to_symbol(
        ctx.emitter,
        scratch,
        &target.extern_symbol("elephc_probe_allocs_ptr"),
        0,
    );
    if !ctx.shared.instrument.is_on() {
        let note_io = target.extern_symbol("elephc_probe_note_io");
        abi::emit_symbol_address(ctx.emitter, scratch, &note_io);
        abi::emit_store_reg_to_symbol(
            ctx.emitter,
            scratch,
            &target.extern_symbol("elephc_instr_io_fn"),
            0,
        );
        let note_wait = target.extern_symbol("elephc_probe_note_wait");
        abi::emit_symbol_address(ctx.emitter, scratch, &note_wait);
        abi::emit_store_reg_to_symbol(
            ctx.emitter,
            scratch,
            &target.extern_symbol("elephc_instr_wait_fn"),
            0,
        );
    }
}

/// Emits the `--probe` exit dump call at main's epilogue: disarms the timer and
/// writes the folded profile to stderr. Placed like the gc-stats dump.
fn emit_probe_dump(ctx: &mut FunctionContext<'_>) {
    if ctx.shared.probe_table.is_none() {
        return;
    }
    ctx.emitter.comment("probe: dump folded profile to stderr (--probe)");
    let entry = ctx.emitter.target.extern_symbol("elephc_probe_dump");
    abi::emit_call_label(ctx.emitter, &entry);
}

/// Emits the `--instrument` name table and the `elephc_instr_init` call in main's
/// prologue (and the `--web` entry stub). The table is `(name_ptr, name_len)`
/// pairs in function-id order — the registry the per-function prologues built as
/// they were emitted. Main is emitted last, so the registry is complete here.
/// Inert unless `--instrument`.
fn emit_instr_init(ctx: &mut FunctionContext<'_>) {
    if !ctx.shared.instrument.is_on() {
        return;
    }
    let names = ctx.shared.instr_registry().to_vec();
    if names.is_empty() {
        return;
    }
    let mut words = Vec::with_capacity(names.len() * 2);
    for name in &names {
        let (name_label, name_len) = ctx.data.add_string(name.as_bytes());
        words.push(DataWord::Symbol(name_label));
        words.push(DataWord::U64(name_len as u64));
    }
    let table_label = ctx.data.add_words(words);
    ctx.emitter
        .comment("instrument: register function name table (--instrument)");
    let target = ctx.emitter.target;
    let table_arg = abi::int_arg_reg_name(target, 0);
    let count_arg = abi::int_arg_reg_name(target, 1);
    abi::emit_symbol_address(ctx.emitter, table_arg, &table_label);
    abi::emit_load_int_immediate(ctx.emitter, count_arg, names.len() as i64);
    let entry = target.extern_symbol("elephc_instr_init");
    abi::emit_call_label(ctx.emitter, &entry);
    // Publish elephc_instr_io into the runtime slot so I/O builtins (PDO
    // queries) can count operations per function without a compile-time coupling
    // to the instrument crate. The slot stays zero in non-instrument binaries.
    let io_fn = target.extern_symbol("elephc_instr_io");
    let scratch = abi::int_arg_reg_name(target, 0);
    abi::emit_symbol_address(ctx.emitter, scratch, &io_fn);
    abi::emit_store_reg_to_symbol(ctx.emitter, scratch, &target.extern_symbol("elephc_instr_io_fn"), 0);
    // Companion slot: elephc_instr_query, so the PDO bridge can report each
    // query's SQL text (the N+1 view) the same pay-for-use way as the counter.
    let query_fn = target.extern_symbol("elephc_instr_query");
    abi::emit_symbol_address(ctx.emitter, scratch, &query_fn);
    abi::emit_store_reg_to_symbol(ctx.emitter, scratch, &target.extern_symbol("elephc_instr_query_fn"), 0);
    // Third slot: elephc_instr_wait, so the bridge can report how long a driver
    // call actually blocked — the CPU-vs-wait split of each function's time.
    let wait_fn = target.extern_symbol("elephc_instr_wait");
    abi::emit_symbol_address(ctx.emitter, scratch, &wait_fn);
    abi::emit_store_reg_to_symbol(ctx.emitter, scratch, &target.extern_symbol("elephc_instr_wait_fn"), 0);
    // Fourth slot: elephc_instr_trace_begin, so the web bridge can open each
    // request's W3C trace context (distributed profiling).
    let trace_fn = target.extern_symbol("elephc_instr_trace_begin");
    abi::emit_symbol_address(ctx.emitter, scratch, &trace_fn);
    abi::emit_store_reg_to_symbol(ctx.emitter, scratch, &target.extern_symbol("elephc_instr_trace_fn"), 0);
    let request = target.extern_symbol("elephc_instr_request");
    abi::emit_symbol_address(ctx.emitter, scratch, &request);
    abi::emit_store_reg_to_symbol(
        ctx.emitter,
        scratch,
        &target.extern_symbol("elephc_instr_request_fn"),
        0,
    );
    // Let the environment decide whether the hooks are live for this run.
    abi::emit_call_label(ctx.emitter, &target.extern_symbol("elephc_instr_boot"));
    // Tell the runtime its numbers describe a subset, so the report can say that
    // self time now absorbs uninstrumented callees rather than presenting the same
    // columns as the full mode.
    if ctx.shared.instrument.is_partial() {
        abi::emit_call_label(ctx.emitter, &target.extern_symbol("elephc_instr_partial"));
    }

}

/// Emits the `--instrument` entry hook at the end of a user function's prologue:
/// registers the function (assigning its id), records the id on the context for
/// the epilogue, and calls `elephc_instr_enter(id)`. Synthetic bodies are not
/// user code and stay uninstrumented; `main` uses its own prologue and is not
/// a timed frame in this mode.
fn emit_instr_enter(ctx: &mut FunctionContext<'_>) {
    // Synthetic bodies are not user code. Beyond that, a selective set hooks
    // only the functions it names — the rest run at full speed.
    if ctx.function.flags.is_synthetic || !ctx.shared.instrument.covers(&ctx.function.name) {
        return;
    }
    let id = ctx.shared.register_instr(ctx.function.name.clone());
    ctx.instr_id = Some(id);
    ctx.emitter.comment("instrument: enter (--instrument)");
    emit_instr_hook_call(ctx, "elephc_instr_enter", id);
}

/// Emits the `--instrument` exit hook for one return path. Preserves the return
/// value across the call (the hook clobbers the ABI result register).
fn emit_instr_exit(ctx: &mut FunctionContext<'_>) {
    if !ctx.shared.instrument.is_on() {
        return;
    }
    let Some(id) = ctx.instr_id else {
        return;
    };
    ctx.emitter.comment("instrument: exit (--instrument)");
    let return_ty = ctx.function.return_php_type.codegen_repr();
    let preserves_return = !matches!(return_ty, PhpType::Void | PhpType::Never);
    if preserves_return {
        push_return_value(ctx, &return_ty);
    }
    emit_instr_hook_call(ctx, "elephc_instr_exit", id);
    if preserves_return {
        pop_return_value(ctx, &return_ty);
    }
}

/// Loads `id` into the first integer argument register, the program's live
/// allocation counter (`_gc_allocs`) into the second, and the free counter
/// (`_gc_frees`) into the third, then calls a `elephc_instr_*` hook. Reading
/// both counters at the call site lets the runtime attribute allocations — and
/// net retained objects (allocs minus frees) — per function exactly the way it
/// attributes time.
fn emit_instr_hook_call(ctx: &mut FunctionContext<'_>, hook: &str, id: usize) {
    let target = ctx.emitter.target;
    let id_arg = abi::int_arg_reg_name(target, 0);
    abi::emit_load_int_immediate(ctx.emitter, id_arg, id as i64);
    let allocs_arg = abi::int_arg_reg_name(target, 1);
    abi::emit_load_symbol_to_reg(ctx.emitter, allocs_arg, "_gc_allocs", 0);
    let frees_arg = abi::int_arg_reg_name(target, 2);
    abi::emit_load_symbol_to_reg(ctx.emitter, frees_arg, "_gc_frees", 0);
    let entry = target.extern_symbol(hook);
    abi::emit_call_label(ctx.emitter, &entry);
}

/// Emits the `--instrument` exit dump call at main's epilogue (and per `--web`
/// request): writes the exact per-function table and edges to stderr.
fn emit_instr_dump(ctx: &mut FunctionContext<'_>) {
    if !ctx.shared.instrument.is_on() {
        return;
    }
    ctx.emitter
        .comment("instrument: dump exact per-function profile to stderr (--instrument)");
    let entry = ctx.emitter.target.extern_symbol("elephc_instr_dump");
    abi::emit_call_label(ctx.emitter, &entry);
}

/// Emits the `--counters` prologue increment for one compiled PHP function and
/// registers it for the exit dump.
///
/// Placed with the stack guard, before the parameter spill: only scratch
/// registers are touched (AArch64 x9/x10, x86_64 r10), so the incoming ABI
/// arguments are undisturbed. The count is a plain load/add/store — a lost
/// update under threads costs a tick of precision, never correctness. Synthetic
/// bodies are not user code and stay uncounted; `main` never reaches this
/// prologue (it has its own), which keeps the dump to functions the user wrote.
fn emit_call_counter_increment(ctx: &mut FunctionContext<'_>, entry_label: &str) {
    if !ctx.shared.counters || ctx.function.flags.is_synthetic {
        return;
    }
    let slot = ctx.data.add_comm(format!("_elephc_cnt{}", entry_label), 8);
    ctx.shared
        .register_counter(ctx.function.name.clone(), slot.clone());
    ctx.emitter.comment("call counter (--counters)");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", &slot);
            ctx.emitter.instruction("ldr x10, [x9]");
            ctx.emitter.instruction("add x10, x10, #1");
            ctx.emitter.instruction("str x10, [x9]");
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r10", &slot);
            ctx.emitter.instruction("inc qword ptr [r10]");
        }
    }
}

/// Emits the `--counters` exit dump: one `elephc-counters: <name> <count>` line
/// per counted function, written to stderr like the gc-stats report.
///
/// Rendered from the shared registry, which is complete because main — the only
/// emitter of this dump — is generated after every other function.
fn emit_counters_dump(ctx: &mut FunctionContext<'_>) {
    ctx.emitter
        .comment("counters: print exact per-function call counts to stderr");
    let registry: Vec<(String, String)> = ctx.shared.counter_registry().to_vec();
    let int_result_reg = abi::int_result_reg(ctx.emitter);
    for (name, symbol) in registry {
        let line = format!("elephc-counters: {} ", name);
        let (label, len) = ctx.data.add_string(line.as_bytes());
        emit_write_literal_stderr(ctx.emitter, &label, len);
        abi::emit_load_symbol_to_reg(ctx.emitter, int_result_reg, &symbol, 0);
        abi::emit_call_label(ctx.emitter, "__rt_itoa");
        emit_write_current_string_stderr(ctx.emitter);
        let (newline_label, _) = ctx.data.add_string(b"\n");
        emit_write_literal_stderr(ctx.emitter, &newline_label, 1);
    }
}

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

#[cfg(test)]
mod tests;
