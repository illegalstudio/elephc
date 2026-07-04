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

/// Local label for the shared `exit()`/`die()` bailout landing inside the unique
/// `_elephc_web_handler`. The boundary `setjmp` (in the prologue) branches here
/// on a non-zero return; the landing itself is emitted once at the handler tail.
/// A fixed name is safe because exactly one `_elephc_web_handler` exists per
/// program, so there is never more than one such label to collide.
const WEB_EXIT_BAILOUT_LABEL: &str = "__elephc_web_exit_bailout";

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
    /// Base offset of this frame's 24-byte exception-cleanup activation record
    /// (`{prev@base, cleanup_cb@base-8, saved_fp@base-16}`), or `None` when the
    /// function owns no refcounted locals and therefore needs no record. The
    /// record address is `x29 - base`, matching the `[+0]/[+8]/[+16]` layout that
    /// `__rt_exception_cleanup_frames` walks. Only set for functions that push a
    /// record; `main`/`_elephc_web_handler` are always the unwind root and never do.
    pub(super) activation_record_offset: Option<usize>,
}

/// Size in bytes of an exception-cleanup activation record: three consecutive
/// 8-byte fields (`prev`, `cleanup_cb`, `saved_fp`) matching the `[+0]/[+8]/[+16]`
/// layout that `__rt_exception_cleanup_frames` reads.
const ACTIVATION_RECORD_BYTES: usize = 24;

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
    // Reserve the 24-byte exception-cleanup activation record when this function
    // owns refcounted locals: on `throw`/`exit`/`die` unwinding THROUGH this frame,
    // `__rt_exception_cleanup_frames` runs its per-frame cleanup callback to release
    // those locals. Functions that own nothing refcounted stay out of the chain and
    // pay no push/pop cost. Reserved like the try-handler slots: `offset += size`
    // makes the record address `x29 - offset` with fields at `+0/+8/+16`.
    let activation_record_offset = if function_needs_cleanup_registry(function) {
        offset += ACTIVATION_RECORD_BYTES;
        Some(offset)
    } else {
        None
    };
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
        activation_record_offset,
    }
}

/// Returns whether a function owns refcounted locals and therefore needs an
/// exception-cleanup activation record pushed onto `_exc_call_frame_top`.
///
/// This is the frame-only mirror of the union of `function_cleanup_locals` (with
/// `include_returned = true`, since an unwound frame is abandoned and even a
/// would-be-returned local leaks) and `ref_cell_owner_locals`, computed without a
/// `FunctionContext` because the layout decision must precede context creation. A
/// function with an empty set stays out of the cleanup chain entirely.
fn function_needs_cleanup_registry(function: &Function) -> bool {
    let param_names = function
        .params
        .iter()
        .map(|param| param.name.as_str())
        .collect::<HashSet<_>>();
    let promoted = promoted_ref_cell_local_slots(function);
    let owns_refcounted_local = function.locals.iter().any(|local| {
        local_kind_needs_epilogue_cleanup(local.kind)
            && !promoted.contains(&local.id)
            && local
                .name
                .as_deref()
                .is_none_or(|name| !param_names.contains(name))
            && local_slot_is_written(function, local.id)
            && {
                let ty = local.php_type.codegen_repr();
                matches!(ty, PhpType::Str | PhpType::Callable) || ty.is_refcounted()
            }
    });
    let owns_ref_cell = function
        .locals
        .iter()
        .any(|local| local.kind == LocalKind::RefCell);
    owns_refcounted_local || owns_ref_cell
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
    store_argc_local_if_present(ctx);
    store_argv_local_if_present(ctx);
    zero_initialize_main_cleanup_locals(ctx);
    zero_initialize_ref_cell_owner_locals(ctx);
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
    // Register this frame in the exception-cleanup chain LAST — after params are
    // stored and cleanup locals zeroed, before any body code that could throw — so a
    // non-local unwind through this frame finds a well-formed record whose callback
    // can safely release its owned locals. Only functions that reserved a record push
    // one; the label is created here and reused by the epilogue's callback emission.
    if ctx.activation_record_offset.is_some() {
        let cleanup_label = ctx.next_label("frame_cleanup");
        emit_activation_record_push(ctx, &cleanup_label);
        ctx.frame_cleanup_label = Some(cleanup_label);
    }
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
    // Release function-static locals' owned refcounted values at process exit.
    // Statics persist across calls and are never freed by any function epilogue,
    // so without this the final static value leaks at exit. Runs BEFORE the
    // callee-saved restores / frame teardown so the runtime release helpers still
    // have a valid frame to call into, and before gc_stats/heap_debug so the freed
    // statics are counted in the allocator summary. CLI-only: the `--web` path
    // uses `emit_web_handler_epilogue` and never reaches this epilogue.
    {
        let emitter = &mut *ctx.emitter;
        let data = &*ctx.data;
        super::web::emit_function_static_locals_release_at_exit(emitter, data);
    }
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
    //
    // In `--web-worker` and `--web-worker=script` mode the boot runs once and
    // statics persist for the worker lifetime, so the boot does NOT call the
    // full reset. The per-request trampoline calls
    // `__rt_web_worker_request_reset`, which only resets request superglobals
    // and the concat offset. This skip is permanent and by design in both
    // worker modes: the boot must NOT reset persistent statics.
    if !ctx.web_worker && !ctx.web_worker_script {
        ctx.emitter.comment("reset per-request persistent state");
        abi::emit_call_label(ctx.emitter, "__rt_web_reset");
    }
    capture_concat_base(ctx);
    emit_callee_saved_saves(ctx);
    zero_initialize_main_cleanup_locals(ctx);
    zero_initialize_ref_cell_owner_locals(ctx);
    // Install the exit()/die() request boundary LAST, once the frame, callee-saved
    // saves, and cleanup-local zero-inits are all in place, so the bailout landing's
    // epilogue is valid when reached via longjmp. Only the top-level-re-run modes
    // (`--web`, `--web-worker=script`) install it; in `--web-worker` handler mode
    // this handler is the one-shot boot, not a per-request boundary.
    if !ctx.web_worker {
        emit_web_exit_boundary(ctx);
    }
}

/// Installs the `exit()`/`die()` request boundary at the end of the
/// `_elephc_web_handler` prologue (top-level-re-run web modes only).
///
/// Emits `setjmp(_exit_jmp_buf)` — a channel SEPARATE from the exception handler
/// chain, so `exit()` is uncatchable by user `catch (\Throwable)` and skips
/// `finally`, matching PHP — and marks `_exit_boundary_active`. A later
/// `exit()`/`die()` at any call depth routes through `__rt_exit`, which longjmps
/// back here with a non-zero `setjmp` result; that branches to the shared bailout
/// landing (`emit_web_exit_bailout_landing`). On the install path (result 0) it
/// falls through into the handler body.
fn emit_web_exit_boundary(ctx: &mut FunctionContext<'_>) {
    let target = ctx.emitter.target;
    let arg0 = abi::int_arg_reg_name(target, 0);
    ctx.emitter
        .comment("-- install exit()/die() request boundary (setjmp into _exit_jmp_buf) --");
    abi::emit_symbol_address(ctx.emitter, arg0, "_exit_jmp_buf");                // arg0 = &_exit_jmp_buf (this request's exit setjmp buffer)
    ctx.emitter.bl_c("setjmp");                                                  // returns 0 on install, 1 when exit()/die() longjmps back here
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, WEB_EXIT_BAILOUT_LABEL); // non-zero result → arrived via exit()/die() → run the bailout landing
    // Install path: mark the boundary active so __rt_exit longjmps here instead of
    // terminating the process. temp_int_reg is x10/r10, distinct from the x9/r-scratch
    // emit_store_reg_to_symbol uses internally, so the value survives the store.
    let flag = abi::temp_int_reg(target);
    abi::emit_load_int_immediate(ctx.emitter, flag, 1);
    abi::emit_store_reg_to_symbol(ctx.emitter, flag, "_exit_boundary_active", 0);
    // Capture the request-entry cleanup-chain head as the exit survivor frame. On a
    // later exit()/die() at any call depth, __rt_exit passes this to
    // __rt_exception_cleanup_frames so every PHP activation frame pushed during the
    // request is released before the longjmp back here. At install time no user
    // function has run yet, so this is the clean request baseline (0 on a fresh
    // request; the per-request reset zeroes `_exc_call_frame_top`).
    abi::emit_load_symbol_to_reg(ctx.emitter, flag, "_exc_call_frame_top", 0);
    abi::emit_store_reg_to_symbol(ctx.emitter, flag, "_exit_survivor_frame", 0);
}

/// Emits the shared `exit()`/`die()` bailout landing at the tail of the unique
/// `_elephc_web_handler` body.
///
/// Reached only via `setjmp` returning non-zero after `__rt_exit` longjmps into
/// `_exit_jmp_buf`. The longjmp skipped every pending try-pop, so this restores
/// the exception state to its request-entry baseline — `_exc_handler_top` and
/// `_rt_diag_suppression` are both 0 at `_elephc_web_handler` entry (nothing
/// outside the handler pushes a handler record or an `@` suppression), so it
/// zeroes them — clears `_exit_boundary_active`, then runs the normal handler
/// epilogue (owned-local cleanup, callee-saved restores, frame teardown, `ret`).
/// The `ret` returns to the Rust worker loop, which flushes the response body
/// already buffered by the pre-exit `echo`s and serves the next request.
///
/// Emitted once per program (the handler is unique), only for `--web` and
/// `--web-worker=script`. Gating matches `emit_web_exit_boundary`.
pub(super) fn emit_web_exit_bailout_landing(ctx: &mut FunctionContext<'_>) {
    ctx.emitter.blank();
    ctx.emitter
        .comment("-- exit()/die() bailout landing: end the request, keep the worker alive --");
    ctx.emitter.label(WEB_EXIT_BAILOUT_LABEL);
    abi::emit_store_zero_to_symbol(ctx.emitter, "_exc_handler_top", 0);
    abi::emit_store_zero_to_symbol(ctx.emitter, "_rt_diag_suppression", 0);
    abi::emit_store_zero_to_symbol(ctx.emitter, "_exit_boundary_active", 0);
    emit_web_handler_epilogue(ctx);
}

/// Emits the `--web` top-level handler epilogue and returns to the bridge.
///
/// Like `emit_main_epilogue` it runs the per-request main local cleanup (so
/// owned refcounted top-level locals are released each request) and restores the
/// frame, but it `ret`s instead of exiting and skips the process-end gc-stats and
/// heap-debug diagnostics, which are wrong to report per request.
pub(super) fn emit_web_handler_epilogue(ctx: &mut FunctionContext<'_>) {
    ctx.emitter.blank();
    ctx.emitter.comment("web handler epilogue + ret");
    emit_main_local_epilogue_cleanup(ctx);
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

/// Emits the `--web-worker` process-entry stub that drives the worker bridge.
///
/// The stub is the real process entry (`_main`/`main`). It stores the OS
/// argc/argv to globals once, loads them plus the boot function address into the
/// first three C-ABI integer argument registers, calls
/// `elephc_web_run_worker(argc, argv, &boot)`, and exits with the bridge's
/// integer return value. The boot address (arg 2) is the `_elephc_web_handler`
/// symbol — the top-level PHP body that runs once per worker, initializes the
/// app, and calls `elephc_worker_register` to hand the trampoline to Rust.
pub(super) fn emit_web_worker_entry_stub(ctx: &mut FunctionContext<'_>) {
    let target = ctx.emitter.target;
    if target.arch == Arch::AArch64 {
        ctx.emitter.raw(".align 2");
    }
    ctx.emitter.blank();
    ctx.emitter.comment("--web-worker process entry: call elephc_web_run_worker(argc, argv, &boot)");
    ctx.emitter.entry_label();
    abi::emit_frame_prologue(ctx.emitter, ctx.frame_size);
    ctx.emitter.comment("save argc/argv to globals for the bridge and boot");
    abi::emit_store_process_args_to_globals(ctx.emitter);
    let argc_reg = abi::int_arg_reg_name(target, 0);
    let argv_reg = abi::int_arg_reg_name(target, 1);
    let boot_reg = abi::int_arg_reg_name(target, 2);
    abi::emit_load_symbol_to_reg(ctx.emitter, argc_reg, "_global_argc", 0);
    abi::emit_load_symbol_to_reg(ctx.emitter, argv_reg, "_global_argv", 0);
    // The boot function is the top-level PHP body, emitted as _elephc_web_handler.
    abi::emit_symbol_address(ctx.emitter, boot_reg, WEB_HANDLER_SYMBOL);
    let bridge_entry = target.extern_symbol("elephc_web_run_worker");
    abi::emit_call_label(ctx.emitter, &bridge_entry);
    abi::emit_exit_with_result_reg(ctx.emitter);
}

/// Emits the process entry stub for `--web-worker=script`: stores argc/argv to
/// globals, then tail-calls the Rust bridge `elephc_web_run_script(argc, argv,
/// &handler)` where `handler` is the compiled top-level `_elephc_web_handler`.
/// Identical to `emit_web_worker_entry_stub` except the bridge symbol; the
/// handler is registered directly per forked child (no PHP boot/register phase).
pub(super) fn emit_web_worker_script_entry_stub(ctx: &mut FunctionContext<'_>) {
    let target = ctx.emitter.target;
    if target.arch == Arch::AArch64 {
        ctx.emitter.raw(".align 2");
    }
    ctx.emitter.blank();
    ctx.emitter.comment("--web-worker=script process entry: call elephc_web_run_script(argc, argv, &handler)");
    ctx.emitter.entry_label();
    abi::emit_frame_prologue(ctx.emitter, ctx.frame_size);
    ctx.emitter.comment("save argc/argv to globals for the bridge and handler");
    abi::emit_store_process_args_to_globals(ctx.emitter);
    let argc_reg = abi::int_arg_reg_name(target, 0);
    let argv_reg = abi::int_arg_reg_name(target, 1);
    let handler_reg = abi::int_arg_reg_name(target, 2);
    abi::emit_load_symbol_to_reg(ctx.emitter, argc_reg, "_global_argc", 0);
    abi::emit_load_symbol_to_reg(ctx.emitter, argv_reg, "_global_argv", 0);
    // The handler is the top-level PHP body, emitted as _elephc_web_handler.
    abi::emit_symbol_address(ctx.emitter, handler_reg, WEB_HANDLER_SYMBOL);
    let bridge_entry = target.extern_symbol("elephc_web_run_script");
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
        .filter(|local| local_slot_is_written(ctx.function, local.id))
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
            ctx.emitter.instruction(&format!("cbz x9, {}", done)); // skip released or never-created fallback ref-cells
            abi::emit_release_local_ref_cell(ctx.emitter, "x9", ty);
            abi::emit_store_zero_to_local_slot(ctx.emitter, offset);
        }
        Arch::X86_64 => {
            abi::load_at_offset_scratch(ctx.emitter, "r11", offset, "r10");
            ctx.emitter.instruction("test r11, r11"); // check whether this owner still holds a fallback ref-cell
            ctx.emitter.instruction(&format!("je {}", done)); // skip released or never-created fallback ref-cells
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

/// Returns true when a local slot receives an owned value through an explicit EIR
/// `StoreLocal` or a `CatchBind`.
///
/// `CatchBind` counts because a caught exception variable (`$e`) owns the exception
/// object moved out of the runtime exception slot: it is written by `Op::CatchBind`
/// (never `Op::StoreLocal`), yet it must be zero-initialized, released at scope end,
/// and released by the unwind cleanup callback exactly like any other owned local.
fn local_slot_is_written(function: &Function, slot: LocalSlotId) -> bool {
    function.instructions.iter().any(|inst| {
        matches!(inst.op, Op::StoreLocal | Op::CatchBind)
            && matches!(inst.immediate, Some(Immediate::LocalSlot(candidate)) if candidate == slot)
    })
}

/// Returns whether `slot` names a local whose owned refcounted value the frame's
/// cleanup paths (epilogue release and the unwind cleanup callback) will free.
///
/// Per-slot form of the membership test shared by `function_needs_cleanup_registry`,
/// `main_cleanup_locals`, and `function_cleanup_locals`: the slot must be a
/// cleanup-eligible kind, not a promoted ref-cell, not a parameter, written by an
/// owning `StoreLocal`/`CatchBind`, and hold a refcounted representation. Used by
/// `lower_throw_value` to decide whether a rethrown local (`throw $e`) needs an
/// incref so the in-flight exception survives this frame releasing its slot.
pub(super) fn slot_is_cleanup_tracked(function: &Function, slot: LocalSlotId) -> bool {
    let Some(local) = function.locals.iter().find(|local| local.id == slot) else {
        return false;
    };
    if !local_kind_needs_epilogue_cleanup(local.kind) {
        return false;
    }
    if local
        .name
        .as_deref()
        .is_some_and(|name| function.params.iter().any(|param| param.name == name))
    {
        return false;
    }
    if !local_slot_is_written(function, slot) {
        return false;
    }
    let ty = local.php_type.codegen_repr();
    if !(matches!(ty, PhpType::Str | PhpType::Callable) || ty.is_refcounted()) {
        return false;
    }
    !promoted_ref_cell_local_slots(function).contains(&slot)
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
    abi::load_at_offset(ctx.emitter, result_reg, offset);
    emit_refcounted_release_in_result(ctx, ty);
}

/// Decrefs the refcounted value already loaded in the int result register, skipping
/// a null pointer.
///
/// Shared by the epilogue local cleanup (via `emit_main_refcounted_cleanup`) and the
/// catch-bind release-old / release-discarded paths in `lower_inst`. The caller is
/// responsible for loading the candidate pointer into the int result register first.
pub(super) fn emit_refcounted_release_in_result(ctx: &mut FunctionContext<'_>, ty: &PhpType) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let done = ctx.next_label("refcounted_release_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz {}, {}", result_reg, done));  // skip uninitialized/null refcounted values
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {}, {}", result_reg, result_reg)); // check whether the refcounted value is non-null
            ctx.emitter.instruction(&format!("je {}", done));                   // skip uninitialized/null refcounted values
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
        .filter(|local| local_slot_is_written(ctx.function, local.id))
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

/// Pushes this frame's exception-cleanup activation record onto
/// `_exc_call_frame_top` and records `cleanup_label` as its per-frame callback.
///
/// Emitted at the end of the prologue (once params are stored and cleanup locals
/// zero-initialized, before any body code that could throw). Writes the record's
/// three fields — `prev` = the previous chain head, `cleanup_cb` = `&cleanup_label`,
/// `saved_fp` = this frame's `x29`/`rbp` — then publishes the record address
/// (`x29 - base`) as the new chain head. On a `throw`/`exit`/`die` that unwinds
/// through this frame, `__rt_exception_cleanup_frames` invokes the callback with
/// `saved_fp` to release the frame's owned locals. No-op unless the layout reserved
/// a record (`ctx.activation_record_offset`).
fn emit_activation_record_push(ctx: &mut FunctionContext<'_>, cleanup_label: &str) {
    let Some(base) = ctx.activation_record_offset else {
        return;
    };
    let target = ctx.emitter.target;
    let scratch = abi::temp_int_reg(target);
    ctx.emitter.comment("register exception cleanup frame");
    abi::emit_load_symbol_to_reg(ctx.emitter, scratch, "_exc_call_frame_top", 0);
    abi::store_at_offset(ctx.emitter, scratch, base);                           // record[+0] = previous cleanup-chain head
    abi::emit_symbol_address(ctx.emitter, scratch, cleanup_label);
    abi::store_at_offset(ctx.emitter, scratch, base - 8);                       // record[+8] = per-frame cleanup callback address
    abi::emit_copy_frame_pointer(ctx.emitter, scratch);
    abi::store_at_offset(ctx.emitter, scratch, base - 16);                      // record[+16] = this frame's saved frame pointer
    abi::emit_frame_slot_address(ctx.emitter, scratch, base);
    abi::emit_store_reg_to_symbol(ctx.emitter, scratch, "_exc_call_frame_top", 0);
}

/// Pops this frame's exception-cleanup activation record on the normal-return path,
/// restoring `_exc_call_frame_top` to the `prev` value the push saved.
///
/// Emitted at the top of the epilogue, before the return-value-preserving local
/// cleanup. It only touches scratch registers (`x10`/`r10` and `x9`), never the
/// return-value registers, so the already-loaded return value survives. Removing the
/// record before normal cleanup makes the normal-return epilogue and the unwind
/// callback mutually exclusive for this activation: once popped, no later `throw`
/// can invoke this frame's callback. No-op unless a record was pushed.
fn emit_activation_record_pop(ctx: &mut FunctionContext<'_>) {
    let Some(base) = ctx.activation_record_offset else {
        return;
    };
    let target = ctx.emitter.target;
    let scratch = abi::temp_int_reg(target);
    ctx.emitter.comment("unregister exception cleanup frame");
    abi::load_at_offset(ctx.emitter, scratch, base);                           // reload record[+0] = previous cleanup-chain head
    abi::emit_store_reg_to_symbol(ctx.emitter, scratch, "_exc_call_frame_top", 0);
}

/// Emits the per-frame cleanup callback invoked by `__rt_exception_cleanup_frames`
/// while unwinding a `throw`/`exit`/`die` through this frame.
///
/// The unwinder calls the callback with the unwound frame's saved frame pointer in
/// the first integer argument register. `emit_cleanup_callback_prologue` rebases
/// `x29`/`rbp` onto that pointer so the shared cleanup helpers address the unwound
/// frame's local slots. It releases the ref-cell owners and the FULL owned-local set
/// (`function_cleanup_locals(ctx, None)` — `include_returned = true`, because an
/// unwound frame never hands its would-be return value to a caller, so that local
/// leaks unless freed here). Emitted once, after the function's `ret`; control never
/// falls into it.
fn emit_frame_cleanup_callback(ctx: &mut FunctionContext<'_>, cleanup_label: &str) {
    ctx.emitter.blank();
    ctx.emitter
        .comment("-- exception/exit frame-cleanup callback: release this frame's owned locals --");
    ctx.emitter.label(cleanup_label);
    abi::emit_cleanup_callback_prologue(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0));
    let ref_cell_owners = ref_cell_owner_locals(ctx);
    emit_ref_cell_owner_epilogue_cleanup_for(ctx, ref_cell_owners);
    for (name, _, ty, offset) in function_cleanup_locals(ctx, None) {
        ctx.emitter
            .comment(&format!("frame-cleanup release ${}", name));
        match ty {
            PhpType::Str => emit_main_string_cleanup(ctx, offset),
            PhpType::Callable => emit_main_refcounted_cleanup(ctx, offset, &ty),
            other if other.is_refcounted() => emit_main_refcounted_cleanup(ctx, offset, &other),
            _ => {}
        }
    }
    abi::emit_cleanup_callback_epilogue(ctx.emitter);
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
    emit_activation_record_pop(ctx);
    emit_function_local_epilogue_cleanup(ctx, None);
    emit_callee_saved_restores(ctx);
    abi::emit_frame_restore(ctx.emitter, ctx.frame_size);
    abi::emit_return(ctx.emitter);
    ctx.epilogue_emitted = true;
    if let Some(cleanup_label) = ctx.frame_cleanup_label.clone() {
        emit_frame_cleanup_callback(ctx, &cleanup_label);
    }
}

/// Rounds a byte count up to a 16-byte stack alignment boundary.
fn align_to_16(bytes: usize) -> usize {
    (bytes + 15) & !15
}

/// Stores the OS argument count into `$argc` when the EIR main function has that local.
fn store_argc_local_if_present(ctx: &mut FunctionContext<'_>) {
    let Some(argc_slot) = ctx
        .function
        .locals
        .iter()
        .find(|local| local.name.as_deref() == Some("argc"))
        .map(|local| local.id)
    else {
        return;
    };
    let Ok(offset) = ctx.local_offset(argc_slot) else {
        return;
    };
    abi::store_at_offset(
        ctx.emitter,
        abi::process_argc_reg(ctx.emitter.target),
        offset,
    );
}

/// Builds and stores the PHP `$argv` array when the EIR main function has that local.
fn store_argv_local_if_present(ctx: &mut FunctionContext<'_>) {
    let Some(argv_slot) = ctx
        .function
        .locals
        .iter()
        .find(|local| local.name.as_deref() == Some("argv"))
        .map(|local| local.id)
    else {
        return;
    };
    let Ok(offset) = ctx.local_offset(argv_slot) else {
        return;
    };
    ctx.emitter.comment("build $argv array from OS argv");
    abi::emit_call_label(ctx.emitter, "__rt_build_argv");
    abi::emit_store(ctx.emitter, &PhpType::Array(Box::new(PhpType::Str)), offset);
}
