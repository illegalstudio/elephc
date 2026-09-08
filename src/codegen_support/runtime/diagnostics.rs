//! Purpose:
//! Emits runtime diagnostic suppression and warning-output helpers.
//! The helpers implement PHP-style `error_reporting`, `@` suppression, and stderr writes.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` before PHP-visible helper emission.
//!
//! Key details:
//! - Generic warnings test E_WARNING in `_rt_error_reporting`; callers that already
//!   checked another E_* level use the raw suppression-aware writer.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::abi;

/// Emits runtime diagnostic helpers for suppression depth and warning output.
///
/// Dispatches to `emit_diagnostics_linux_x86_64` when targeting x86_64; otherwise
/// emits architecture-agnostic ARM64 diagnostic helpers inline. Each helper set
/// includes `__rt_diag_push_suppression`, `__rt_diag_pop_suppression`, and
/// `__rt_diag_warning`.
///
/// # Arguments
/// * `emitter` - The code emitter used to append instructions and labels.
///
/// # ABI behavior
/// - `__rt_diag_push_suppression`: increments the global `_rt_diag_suppression` counter and returns.
/// - `__rt_diag_pop_suppression`: decrements the counter (guarded against underflow) and returns.
/// - `__rt_diag_warning`: applies E_WARNING and delegates to `__rt_diag_write`.
/// - `__rt_diag_write`: writes to stderr when suppression depth is zero.
pub(crate) fn emit_diagnostics(emitter: &mut Emitter) {
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

    emitter.label_global("__rt_diag_warning");
    abi::emit_load_symbol_to_reg(emitter, "x10", "_rt_error_reporting", 0);
    emitter.instruction("tst x10, #2");                                        // is E_WARNING enabled by the active runtime mask?
    emitter.instruction("b.eq __rt_diag_warning_masked");                     // suppress generic warnings excluded by error_reporting()
    emitter.instruction("b __rt_diag_write");                                 // reuse the raw writer after warning-level validation
    emitter.label("__rt_diag_warning_masked");
    emitter.instruction("ret");                                               // return without touching the supplied message slice

    emitter.label_global("__rt_diag_write");
    abi::emit_symbol_address(emitter, "x9", "_rt_diag_suppression");
    emitter.instruction("ldr x10, [x9]");                                       // load suppression depth before deciding whether to emit the warning
    emitter.instruction("cbnz x10, __rt_diag_write_done");                      // suppress the diagnostic while inside an active @ scope
    emitter.instruction("mov x0, #2");                                          // fd = stderr for runtime warning diagnostics
    emitter.syscall(4);
    emitter.label("__rt_diag_write_done");
    emitter.instruction("ret");                                                 // return after either writing or suppressing the warning
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
/// - `__rt_diag_warning`: applies E_WARNING before reaching the raw writer.
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

    emitter.label_global("__rt_diag_warning");
    abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_error_reporting", 0);    // load the active PHP diagnostic mask
    emitter.instruction("test r10, 2");                                        // is E_WARNING enabled by the active runtime mask?
    emitter.instruction("jz __rt_diag_warning_masked_linux_x86_64");           // suppress generic warnings excluded by error_reporting()
    emitter.instruction("jmp __rt_diag_write");                               // reuse the raw writer after warning-level validation
    emitter.label("__rt_diag_warning_masked_linux_x86_64");
    emitter.instruction("ret");                                               // return without touching the supplied message slice

    emitter.label_global("__rt_diag_write");
    abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_diag_suppression", 0);    // load suppression depth before deciding whether to emit the warning
    emitter.instruction("test r10, r10");                                       // is runtime warning output currently suppressed?
    emitter.instruction("jnz __rt_diag_write_done_linux_x86_64");               // suppress the diagnostic while inside an active @ scope
    emitter.instruction("mov rdx, rsi");                                        // move warning length into the Linux write length register
    emitter.instruction("mov rsi, rdi");                                        // move warning pointer into the Linux write buffer register
    emitter.instruction("mov edi, 2");                                          // fd = stderr for runtime warning diagnostics
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // emit the runtime warning diagnostic to stderr
    emitter.label("__rt_diag_write_done_linux_x86_64");
    emitter.instruction("ret");                                                 // return after either writing or suppressing the warning
}
