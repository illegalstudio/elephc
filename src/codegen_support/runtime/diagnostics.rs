//! Purpose:
//! Emits runtime diagnostic suppression and the raw stderr writer.
//! Ordinary warnings are dispatched by the shared warning-line/handler adapter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` before PHP-visible helper emission.
//!
//! Key details:
//! - Suppression depth lives in _rt_diag_suppression and warning output must follow each target syscall ABI.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::abi;
use crate::codegen_support::RuntimeFeatures;

/// Emits runtime diagnostic helpers for suppression depth and warning output.
///
/// Dispatches to `emit_diagnostics_linux_x86_64` when targeting x86_64; otherwise
/// emits architecture-agnostic ARM64 diagnostic helpers inline. Each helper set
/// includes `__rt_diag_push_suppression`, `__rt_diag_pop_suppression`, and
/// `__rt_diag_write`, plus the shared `__rt_diag_warning` dispatcher.
///
/// # Arguments
/// * `emitter` - The code emitter used to append instructions and labels.
///
/// # ABI behavior
/// - `__rt_diag_push_suppression`: increments the global `_rt_diag_suppression` counter and returns.
/// - `__rt_diag_pop_suppression`: decrements the counter (guarded against underflow) and returns.
/// - `__rt_diag_write`: writes already-filtered diagnostics when suppression depth is zero.
/// - `__rt_diag_warning`: dispatches full warning lines through handlers and reporting masks.
pub(crate) fn emit_diagnostics(emitter: &mut Emitter, features: RuntimeFeatures) {
    if features.handler_state || features.eval_bridge || features.web {
        super::handler_state::emit_handler_state(emitter);
    }
    super::error_handlers::emit_error_handler_invoke(emitter);
    super::warning_dispatch::emit_warning_dispatch(emitter);
    if emitter.target.arch == Arch::X86_64 {
        emit_diagnostics_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: diagnostics ---");

    emitter.label_global("__rt_diag_push_suppression");
    abi::emit_symbol_address(emitter, "x9", "_rt_diag_suppression");
    emitter.instruction("ldr x10, [x9]");                                       // load the current nested diagnostic-suppression depth
    emitter.instruction("add x10, x10, #1");                                    // enter one additional diagnostic-suppression scope
    emitter.instruction("str x10, [x9]");                                       // publish the incremented diagnostic-suppression depth
    emitter.instruction("ret");                                                 // return to the suppressed expression wrapper

    emitter.label_global("__rt_diag_pop_suppression");
    abi::emit_symbol_address(emitter, "x9", "_rt_diag_suppression");
    emitter.instruction("ldr x10, [x9]");                                       // load the current nested diagnostic-suppression depth
    emitter.instruction("cbz x10, __rt_diag_pop_done");                         // avoid underflow if suppression scopes are already balanced
    emitter.instruction("sub x10, x10, #1");                                    // leave one diagnostic-suppression scope
    emitter.instruction("str x10, [x9]");                                       // publish the decremented diagnostic-suppression depth
    emitter.label("__rt_diag_pop_done");
    emitter.instruction("ret");                                                 // return to the expression wrapper after restoring suppression state

    // DIAGNOSTICS ROUTE THROUGH THE OUTPUT LAYER. php (display_errors=1 on the CLI) renders
    // diagnostics into the ordinary output surface, which output buffering captures in order
    // with echo/var_dump output — the curl_copy_handle_basic_006 EXPECTF order proves it.
    // `__rt_diag_write` therefore tail-paths through `__rt_stdout_write` instead of a raw
    // stderr syscall. The 7 producers share the leaf convention (ptr in x1, len in x2 on
    // AArch64; ptr in rdi, len in rsi on x86_64) and keep registers live across this call,
    // so this helper is non-leaf and preserves every volatile register around it.
    //
    // __rt_stdout_write takes the bytes as (ptr in x0, len in x1) on AArch64; the two
    // producer words shift over inside the preserved frame.
    emitter.label_global("__rt_diag_write");
    abi::emit_symbol_address(emitter, "x9", "_rt_diag_suppression");
    emitter.instruction("ldr x10, [x9]");                                       // load suppression depth before deciding whether to emit the warning
    emitter.instruction("cbnz x10, 1f");                                        // suppress the warning while inside an active @ scope
    abi::emit_symbol_address(emitter, "x9", "_rt_display_errors");              // read the compiled display_errors mode
    emitter.instruction("ldr x10, [x9]");                                       // load the display copy mode: 0 off, 1 output layer, 2 stderr
    emitter.instruction("cbz x10, 1f");                                         // display_errors=0 renders no display copy
    emitter.instruction("cmp x10, #2");                                         // does php send the display copy to stderr?
    emitter.instruction("b.ne 2f");                                             // otherwise route it through the output layer
    emitter.instruction("mov x0, #2");                                          // fd = stderr for display_errors=stderr
    emitter.syscall(4);                                                         // write the producer's x1/x2 bytes to stderr
    emitter.instruction("ret");                                                 // return after the stderr display copy
    emitter.label("2");
    emitter.instruction("sub sp, sp, #1024");                                   // reserve the diagnostic write frame (register preservation without a scratch borrow)
    emitter.instruction("str x29, [sp, #1008]");                                // preserve caller linkage without borrowing a volatile register
    emitter.instruction("str x30, [sp, #1016]");                                // preserve the return address at the fixed frame footer
    emitter.instruction("str x1, [sp, #984]");                                  // park the producer's byte pointer across the volatile save
    emitter.instruction("str x2, [sp, #992]");                                  // park the producer's byte length across the volatile save
    super::warning_dispatch::volatile_registers(emitter, false);                // preserve the volatile general/SIMD registers the producers hold across this non-leaf call
    emitter.instruction("ldr x0, [sp, #984]");                                  // byte pointer → __rt_stdout_write's first argument
    emitter.instruction("ldr x1, [sp, #992]");                                  // byte length → __rt_stdout_write's second argument
    abi::emit_call_label(emitter, "__rt_stdout_write");                          // emit through the output layer (ob-aware, stdout-surface)
    super::warning_dispatch::volatile_registers(emitter, true);                 // restore every preserved register so the producers see their values intact
    emitter.instruction("ldr x30, [sp, #1016]");                                // reload the preserved return address from the fixed frame footer
    emitter.instruction("ldr x29, [sp, #1008]");                                // reload the preserved caller frame pointer
    emitter.instruction("add sp, sp, #1024");                                   // release the diagnostic write frame while preserving every volatile register
    emitter.instruction("ret");                                                 // return after either writing or suppressing the warning
    emitter.label("1");
    emitter.instruction("ret");                                                 // the suppressed path returns without writing
}

/// Emits x86_64 Linux-specific diagnostic helpers for suppression depth and warning output.
///
/// Uses the System V AMD64 ABI: `rdi` holds the warning message pointer, `rsi` holds the
/// length, `edi` holds the file descriptor (set to 2 for stderr), and `eax`/`syscall`
/// invoke Linux `write`. The suppression counter `_rt_diag_suppression` is accessed via
/// RIP-relative addressing.
///
/// # Arguments
/// * `emitter` - The code emitter used to append instructions and labels.
///
/// # ABI constraints
/// - `__rt_diag_push_suppression`: reads/writes `_rt_diag_suppression` via RIP-relative load/store.
/// - `__rt_diag_pop_suppression`: guards decrement against zero to prevent underflow.
/// - `__rt_diag_write`: uses Linux `write` syscall (number 1) with arguments in rdi, rsi, rdx.
fn emit_diagnostics_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: diagnostics ---");

    emitter.label_global("__rt_diag_push_suppression");
    abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_diag_suppression", 0);    // load the current nested diagnostic-suppression depth
    emitter.instruction("add r10, 1");                                          // enter one additional diagnostic-suppression scope
    abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_suppression", 0);   // publish the incremented diagnostic-suppression depth
    emitter.instruction("ret");                                                 // return to the suppressed expression wrapper

    emitter.label_global("__rt_diag_pop_suppression");
    abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_diag_suppression", 0);    // load the current nested diagnostic-suppression depth
    emitter.instruction("test r10, r10");                                       // check whether a suppression scope is active before decrementing
    emitter.instruction("jz __rt_diag_pop_done_linux_x86_64");                  // avoid underflow if suppression scopes are already balanced
    emitter.instruction("sub r10, 1");                                          // leave one diagnostic-suppression scope
    abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_suppression", 0);   // publish the decremented diagnostic-suppression depth
    emitter.label("__rt_diag_pop_done_linux_x86_64");
    emitter.instruction("ret");                                                 // return to the expression wrapper after restoring suppression state

    //
    // __rt_diag_write routes through __rt_stdout_write (the output layer) with the same
    // argument spellings (ptr in rdi, len in rsi); it is non-leaf here so the producers'
    // live volatile registers survive the call.
    emitter.label_global("__rt_diag_write");
    abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_diag_suppression", 0);    // load suppression depth before deciding whether to emit the warning
    emitter.instruction("test r10, r10");                                       // is runtime warning output currently suppressed?
    emitter.instruction("jnz 1f");                                              // suppress the warning while inside an active @ scope
    abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_display_errors", 0);      // read the compiled display_errors mode
    emitter.instruction("test r10, r10");                                       // display_errors=0 renders no display copy
    emitter.instruction("jz 1f");                                               // skip the write when the display copy is off
    emitter.instruction("cmp r10, 2");                                          // does php send the display copy to stderr?
    emitter.instruction("jne 2f");                                              // otherwise route it through the output layer
    emitter.instruction("mov rdx, rsi");                                        // move warning length into the Linux write length register
    emitter.instruction("mov rsi, rdi");                                        // move warning pointer into the Linux write buffer register
    emitter.instruction("mov edi, 2");                                          // fd = stderr for display_errors=stderr
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // emit the stderr display copy
    emitter.instruction("ret");                                                 // return after the stderr display copy
    emitter.label("2");
    abi::emit_frame_prologue(emitter, 1024);                                    // reserve the diagnostic write frame with SysV alignment
    super::warning_dispatch::volatile_registers(emitter, false);                // preserve the volatile general/SIMD registers the producers hold across this non-leaf call
    abi::emit_call_label(emitter, "__rt_stdout_write");                          // emit through the output layer (ob-aware, stdout-surface)
    super::warning_dispatch::volatile_registers(emitter, true);                 // restore every preserved register so the producers see their values intact
    abi::emit_frame_restore(emitter, 1024);                                     // release the diagnostic write frame while preserving every volatile register
    emitter.instruction("ret");                                                 // return after either writing or suppressing the warning
    emitter.label("1");
    emitter.instruction("ret");                                                 // the suppressed path returns without writing
}
