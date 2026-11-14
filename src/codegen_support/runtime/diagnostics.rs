//! Purpose:
//! Emits runtime diagnostic suppression and the raw stderr writer.
//! Ordinary warnings are dispatched by the shared warning-line/handler adapter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` before PHP-visible helper emission.
//!
//! Key details:
//! - `@` saves the active `error_reporting()` mask on a depth-indexed stack, installs PHP's
//!   4437 suppression mask, and restores the saved mask on exit. Warning output must follow
//!   each target syscall ABI; the writer itself only receives diagnostics that were already
//!   filtered by the reporting mask and user handler.

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
/// - `__rt_diag_push_suppression`: saves the active reporting mask at the current depth, installs
///   PHP's `@` mask, and increments `_rt_diag_suppression`.
/// - `__rt_diag_pop_suppression`: decrements the counter (guarded against underflow), restores the
///   saved reporting mask when the expression did not change it itself, and returns.
/// - `__rt_diag_write`: writes already-filtered diagnostics; callers own reporting-mask filtering.
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
    abi::emit_symbol_address(emitter, "x11", "_rt_diag_mask_stack");
    abi::emit_symbol_address(emitter, "x12", "_php_error_reporting");
    emitter.instruction("ldr x13, [x12]");                                      // active reporting mask before the suppressed expression
    emitter.instruction("cmp x10, #256");                                       // is the fixed mask stack exhausted?
    emitter.instruction("b.hs __rt_diag_push_mask_saved");                      // an impossible depth keeps the mask without a saved slot
    emitter.instruction("str x13, [x11, x10, lsl #3]");                         // save the previous mask at this depth
    emitter.label("__rt_diag_push_mask_saved");
    abi::emit_load_int_immediate(emitter, "x14", 4437);                         // PHP's @ mask: E_ERROR | E_PARSE | E_CORE_ERROR | E_COMPILE_ERROR | E_USER_ERROR | E_RECOVERABLE_ERROR
    emitter.instruction("str x14, [x12]");                                      // expose the suppression mask to error_reporting() and handlers
    emitter.instruction("add x10, x10, #1");                                    // enter one additional diagnostic-suppression scope
    emitter.instruction("str x10, [x9]");                                       // publish the incremented diagnostic-suppression depth
    emitter.instruction("ret");                                                 // return to the suppressed expression wrapper

    emitter.label_global("__rt_diag_pop_suppression");
    abi::emit_symbol_address(emitter, "x9", "_rt_diag_suppression");
    emitter.instruction("ldr x10, [x9]");                                       // load the current nested diagnostic-suppression depth
    emitter.instruction("cbz x10, __rt_diag_pop_done");                         // avoid underflow if suppression scopes are already balanced
    emitter.instruction("sub x10, x10, #1");                                    // leave one diagnostic-suppression scope
    emitter.instruction("cmp x10, #256");                                       // was this scope's mask saved?
    emitter.instruction("b.hs __rt_diag_pop_mask_restored");                    // only an impossible overflow can skip restoration
    abi::emit_symbol_address(emitter, "x12", "_php_error_reporting");
    emitter.instruction("ldr x13, [x12]");                                      // current reporting mask on leaving the @ expression
    abi::emit_load_int_immediate(emitter, "x14", 4437);                         // the suppression mask installed by push
    emitter.instruction("cmp x13, x14");                                        // did the expression change error_reporting() itself?
    emitter.instruction("b.ne __rt_diag_pop_mask_restored");                    // PHP keeps a level the expression changed explicitly
    abi::emit_symbol_address(emitter, "x11", "_rt_diag_mask_stack");
    emitter.instruction("ldr x13, [x11, x10, lsl #3]");                         // reload the mask active before this @ expression
    emitter.instruction("str x13, [x12]");                                      // restore the caller-visible reporting level
    emitter.label("__rt_diag_pop_mask_restored");
    emitter.instruction("str x10, [x9]");                                       // publish the decremented diagnostic-suppression depth
    emitter.label("__rt_diag_pop_done");
    emitter.instruction("ret");                                                 // return to the expression wrapper after restoring suppression state

    emitter.label_global("__rt_diag_write");
    if emitter.phpext_output { emit_phpext_diagnostic(emitter); return; }
    emitter.instruction("mov x0, #2");                                          // fd = stderr for runtime warning diagnostics
    emitter.syscall(4);
    emitter.label("__rt_diag_warning_done");
    emitter.instruction("ret");                                                 // return after writing the already-filtered diagnostic
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
    abi::emit_symbol_address(emitter, "r11", "_rt_diag_mask_stack");
    abi::emit_symbol_address(emitter, "r8", "_php_error_reporting");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // active reporting mask before the suppressed expression
    emitter.instruction("cmp r10, 256");                                        // is the fixed mask stack exhausted?
    emitter.instruction("jae __rt_diag_push_mask_saved_linux_x86_64");          // an impossible depth keeps the mask without a saved slot
    emitter.instruction("mov QWORD PTR [r11 + r10 * 8], r9");                   // save the previous mask at this depth
    emitter.label("__rt_diag_push_mask_saved_linux_x86_64");
    abi::emit_load_int_immediate(emitter, "r9", 4437);                          // PHP's @ mask: E_ERROR | E_PARSE | E_CORE_ERROR | E_COMPILE_ERROR | E_USER_ERROR | E_RECOVERABLE_ERROR
    emitter.instruction("mov QWORD PTR [r8], r9");                              // expose the suppression mask to error_reporting() and handlers
    emitter.instruction("add r10, 1");                                          // enter one additional diagnostic-suppression scope
    abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_suppression", 0);   // publish the incremented diagnostic-suppression depth
    emitter.instruction("ret");                                                 // return to the suppressed expression wrapper

    emitter.label_global("__rt_diag_pop_suppression");
    abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_diag_suppression", 0);    // load the current nested diagnostic-suppression depth
    emitter.instruction("test r10, r10");                                       // check whether a suppression scope is active before decrementing
    emitter.instruction("jz __rt_diag_pop_done_linux_x86_64");                  // avoid underflow if suppression scopes are already balanced
    emitter.instruction("sub r10, 1");                                          // leave one diagnostic-suppression scope
    emitter.instruction("cmp r10, 256");                                        // was this scope's mask saved?
    emitter.instruction("jae __rt_diag_pop_mask_restored_linux_x86_64");        // only an impossible overflow can skip restoration
    abi::emit_symbol_address(emitter, "r8", "_php_error_reporting");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // current reporting mask on leaving the @ expression
    abi::emit_load_int_immediate(emitter, "rax", 4437);                        // the suppression mask installed by push
    emitter.instruction("cmp r9, rax");                                         // did the expression change error_reporting() itself?
    emitter.instruction("jne __rt_diag_pop_mask_restored_linux_x86_64");        // PHP keeps a level the expression changed explicitly
    abi::emit_symbol_address(emitter, "r11", "_rt_diag_mask_stack");
    emitter.instruction("mov r9, QWORD PTR [r11 + r10 * 8]");                   // reload the mask active before this @ expression
    emitter.instruction("mov QWORD PTR [r8], r9");                              // restore the caller-visible reporting level
    emitter.label("__rt_diag_pop_mask_restored_linux_x86_64");
    abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_suppression", 0);   // publish the decremented diagnostic-suppression depth
    emitter.label("__rt_diag_pop_done_linux_x86_64");
    emitter.instruction("ret");                                                 // return to the expression wrapper after restoring suppression state

    emitter.label_global("__rt_diag_write");
    if emitter.phpext_output { emit_phpext_diagnostic(emitter); return; }
    emitter.instruction("mov rdx, rsi");                                        // move warning length into the Linux write length register
    emitter.instruction("mov rsi, rdi");                                        // move warning pointer into the Linux write buffer register
    emitter.instruction("mov edi, 2");                                          // fd = stderr for runtime warning diagnostics
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // emit the runtime warning diagnostic to stderr
    emitter.label("__rt_diag_warning_done_linux_x86_64");
    emitter.instruction("ret");                                                 // return after writing the already-filtered diagnostic
}

/// Calls the host warning API with native state parked behind a C bailout guard.
fn emit_phpext_diagnostic(emitter: &mut Emitter) {
    let done = "L_phpext_diagnostic_done";
    abi::emit_frame_prologue(emitter, 16);
    let scratch = abi::secondary_scratch_reg(emitter);
    abi::emit_load_symbol_to_reg(emitter, scratch, "_rt_diag_suppression", 0);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbnz {scratch}, {done}"));            // preserve native suppression before entering the host warning API
            emitter.instruction("mov x0, x1");                                  // adapt the raw warning pointer to the C callback ABI
            emitter.instruction("mov x1, x2");                                  // adapt the raw warning length to the C callback ABI
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("test {scratch}, {scratch}"));         // inspect native diagnostic suppression without changing the message arguments
            emitter.instruction(&format!("jnz {done}"));                        // keep suppressed warnings inside native code
        }
    }
    emitter.bl_c("elephc_php_warning");                                         // let real PHP dispatch warnings while the native image is parked
    crate::codegen_support::cdylib::emit_host_callback_check(emitter, done);
    emitter.label(done);
    abi::emit_frame_restore(emitter, 16);
    abi::emit_return(emitter);
}
