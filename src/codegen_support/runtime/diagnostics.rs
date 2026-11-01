//! Purpose:
//! Emits runtime diagnostic suppression and the raw stderr writer.
//! Ordinary warnings are dispatched by the shared warning-line/handler adapter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` before PHP-visible helper emission.
//!
//! Key details:
//! - Suppression depth lives in _rt_diag_suppression and warning output must follow each target syscall ABI.
//! - When `RuntimeFeatures.diag_user_handler` is set, `__rt_diag_warning` ACCUMULATES the
//!   diagnostic instead of writing each fragment, and offers the whole message to the compiled
//!   PHP function `__elephc_diag_render` when the terminating newline arrives. That function
//!   owns PHP's dispatch rule (reporting mask, then the installed handler, then the default
//!   display); the `write(2)` here is the fallback for a diagnostic nobody took.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::abi;
use crate::codegen_support::runtime::data::fixed::RT_DIAG_BUF_BYTES;
use crate::codegen_support::RuntimeFeatures;

/// AArch64 frame bytes for the buffered diagnostic path. Holds the caller-saved registers the
/// PHP call would otherwise destroy, the flushed length, and the concat cursor. 16-aligned so
/// the `bl` into compiled PHP sees an ABI-conformant stack.
const DIAG_FRAME_BYTES: usize = 176;
/// Frame slot holding the length handed to `__elephc_diag_render`, reused by the fallback write.
const DIAG_SLOT_LENGTH: usize = 120;
/// Frame slot holding `_concat_off` across the PHP call.
const DIAG_SLOT_CONCAT: usize = 128;
/// Frame slot holding whatever `__elephc_diag_render` answered.
const DIAG_SLOT_RESULT: usize = 136;
/// Frame slot pair holding the saved frame pointer and return address.
const DIAG_SLOT_LINKAGE: usize = 160;

/// x86_64 frame bytes for the buffered diagnostic path; see [`DIAG_FRAME_BYTES`].
const DIAG_FRAME_BYTES_X86: usize = 112;

/// Emits runtime diagnostic helpers for suppression depth and warning output.
///
/// Dispatches to `emit_diagnostics_linux_x86_64` when targeting x86_64; otherwise
/// emits architecture-agnostic ARM64 diagnostic helpers inline. Each helper set
/// includes `__rt_diag_push_suppression`, `__rt_diag_pop_suppression`, and
/// `__rt_diag_write`, plus the shared `__rt_diag_warning` dispatcher.
///
/// # Arguments
/// * `emitter` - The code emitter used to append instructions and labels.
/// * `features` - Whose `diag_user_handler` bit decides whether `__rt_diag_warning` may call
///   the compiled PHP dispatch function. The call is a hard reference to a symbol only the
///   program object defines, so a build without that prelude must not emit it.
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
    super::last_error::emit_last_error(emitter);
    super::warning_dispatch::emit_warning_dispatch(emitter);
    if emitter.target.arch == Arch::X86_64 {
        emit_diagnostics_linux_x86_64(emitter, features);
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

    emitter.label_global("__rt_diag_write");
    if emitter.phpext_output { emit_phpext_diagnostic(emitter); return; }
    abi::emit_symbol_address(emitter, "x9", "_rt_diag_suppression");
    emitter.instruction("ldr x10, [x9]");                                       // load suppression depth before deciding whether to emit the warning
    if features.diag_user_handler {
        emitter.instruction("cbz x10, __rt_diag_warning_live");                 // not inside an @ scope: consider buffering
        emitter.instruction("mov x10, xzr");
        abi::emit_store_reg_to_symbol(emitter, "x10", "_rt_diag_complete", 0);  // a dropped message must not mark the next one complete
        emitter.instruction("ret");                                             // suppress the warning while inside an active @ scope
        emitter.label("__rt_diag_warning_live");
        abi::emit_load_symbol_to_reg(emitter, "x10", "_rt_diag_dispatching", 0); // is a diagnostic already being handed to PHP?
        emitter.instruction("cbz x10, __rt_diag_warning_buffer");               // ordinary diagnostic: accumulate it for the handler
        emitter.instruction("mov x10, xzr");
        abi::emit_store_reg_to_symbol(emitter, "x10", "_rt_diag_complete", 0);  // written straight through, so nothing is pending
    } else {
        emitter.instruction("cbnz x10, __rt_diag_warning_done");                // suppress the warning while inside an active @ scope
    }
    emitter.instruction("mov x0, #2");                                          // fd = stderr for runtime warning diagnostics
    emitter.syscall(4);
    emitter.label("__rt_diag_warning_done");
    emitter.instruction("ret");                                                 // return after either writing or suppressing the warning

    if features.diag_user_handler {
        emit_diag_buffered_path_aarch64(emitter);
    }
}

/// Emits the AArch64 accumulate-and-dispatch path of `__rt_diag_warning`.
///
/// Raise sites hand this helper FRAGMENTS — `"Warning: Undefined array key "`, the formatted
/// key, `"\n"` are three separate calls — because that is how the rendered line is assembled.
/// A `set_error_handler()` callback is owed the whole message, so the fragments are appended to
/// `_rt_diag_buf` and the diagnostic is considered complete at its terminating newline, which
/// every message the runtime renders ends with.
///
/// The frame saves every caller-saved register the ORIGINAL helper left intact: its contract was
/// x0/x9/x10 plus the syscall's scratch, and the hand-written runtime helpers that call it three
/// times in a row rely on that. Calling compiled PHP clobbers x0-x18, so the difference is
/// restored here rather than audited at ~40 call sites.
fn emit_diag_buffered_path_aarch64(emitter: &mut Emitter) {
    emitter.label("__rt_diag_warning_buffer");
    emitter.instruction(&format!("sub sp, sp, #{}", DIAG_FRAME_BYTES));         // reserve the diagnostic accumulation frame
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", DIAG_SLOT_LINKAGE)); // save frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{}", DIAG_SLOT_LINKAGE));       // establish a stable diagnostic frame
    emitter.instruction("stp x1, x2, [sp, #0]");                                // preserve the fragment pointer and length for the caller
    emitter.instruction("stp x3, x4, [sp, #16]");                               // preserve caller-saved registers the PHP call would destroy
    emitter.instruction("stp x5, x6, [sp, #32]");                               // preserve caller-saved registers the PHP call would destroy
    emitter.instruction("stp x7, x8, [sp, #48]");                               // preserve caller-saved registers the PHP call would destroy
    emitter.instruction("stp x11, x12, [sp, #64]");                             // preserve caller-saved registers the PHP call would destroy
    emitter.instruction("stp x13, x14, [sp, #80]");                             // preserve caller-saved registers the PHP call would destroy
    emitter.instruction("stp x15, x17, [sp, #96]");                             // preserve caller-saved registers the PHP call would destroy
    emitter.instruction("str x18, [sp, #112]");                                 // preserve the platform register across the PHP call

    // -- append the fragment --
    abi::emit_load_symbol_to_reg(emitter, "x10", "_rt_diag_buf_len", 0);        // x10 = bytes already accumulated for this diagnostic
    abi::emit_symbol_address(emitter, "x9", "_rt_diag_buf");
    emitter.label("__rt_diag_warning_append");
    emitter.instruction("cbz x2, __rt_diag_warning_appended");                  // the whole fragment has been copied
    emitter.instruction(&format!("mov x12, #{}", RT_DIAG_BUF_BYTES));           // x12 = accumulator capacity
    emitter.instruction("cmp x10, x12");                                        // is the accumulator full?
    emitter.instruction("b.hs __rt_diag_warning_appended");                     // truncate rather than overrun the accumulator
    emitter.instruction("ldrb w11, [x1], #1");                                  // read one fragment byte and advance the source cursor
    emitter.instruction("strb w11, [x9, x10]");                                 // append the byte to the accumulated diagnostic
    emitter.instruction("add x10, x10, #1");                                    // advance the accumulator cursor
    emitter.instruction("sub x2, x2, #1");                                      // one fewer fragment byte to copy
    emitter.instruction("b __rt_diag_warning_append");                          // continue copying the fragment
    emitter.label("__rt_diag_warning_appended");
    abi::emit_store_reg_to_symbol(emitter, "x10", "_rt_diag_buf_len", 0);       // publish the accumulated length

    // -- is the diagnostic complete? --
    emitter.instruction("cbz x10, __rt_diag_warning_buffer_ret");               // nothing accumulated yet
    abi::emit_load_symbol_to_reg(emitter, "x11", "_rt_diag_complete", 0);       // did the caller say this was the last fragment?
    emitter.instruction("mov x12, xzr");
    abi::emit_store_reg_to_symbol(emitter, "x12", "_rt_diag_complete", 0);      // the marker is consumed by exactly one diagnostic
    emitter.instruction("cbnz x11, __rt_diag_warning_flush");                   // a whole-message caller flushes without a newline
    abi::emit_symbol_address(emitter, "x9", "_rt_diag_buf");
    emitter.instruction("sub x11, x10, #1");                                    // index of the last accumulated byte
    emitter.instruction("ldrb w12, [x9, x11]");                                 // load the last accumulated byte
    emitter.instruction("cmp w12, #10");                                        // every rendered diagnostic ends with a newline
    emitter.instruction("b.ne __rt_diag_warning_buffer_ret");                   // still mid-message: wait for the rest
    emitter.label("__rt_diag_warning_flush");

    // -- hand the whole diagnostic to PHP --
    emitter.instruction(&format!("str x10, [sp, #{}]", DIAG_SLOT_LENGTH));      // keep the length for the fallback write
    emitter.instruction("mov x1, x10");                                         // arg 2 = accumulated length
    emitter.instruction("mov x11, xzr");
    // Cleared BEFORE the call, not after: a handler that throws longjmps straight past this
    // frame, and a stale length would prepend the abandoned message to the next diagnostic.
    abi::emit_store_reg_to_symbol(emitter, "x11", "_rt_diag_buf_len", 0);       // the accumulator is now owned by this flush
    abi::emit_load_symbol_to_reg(emitter, "x2", "_rt_diag_file_ptr", 0);        // arg 3 = raise-site file pointer (0 when unknown)
    abi::emit_load_symbol_to_reg(emitter, "x3", "_rt_diag_file_len", 0);        // arg 4 = raise-site file length
    abi::emit_load_symbol_to_reg(emitter, "x4", "_rt_diag_line", 0);            // arg 5 = raise-site line (0 when unknown)
    emitter.instruction("mov x11, xzr");
    abi::emit_store_reg_to_symbol(emitter, "x11", "_rt_diag_file_ptr", 0);      // a location is consumed by exactly one diagnostic
    abi::emit_store_reg_to_symbol(emitter, "x11", "_rt_diag_file_len", 0);      // so the next one cannot inherit this one's
    abi::emit_store_reg_to_symbol(emitter, "x11", "_rt_diag_line", 0);          // and report a line it was never raised on
    abi::emit_load_symbol_to_reg(emitter, "x11", "_exc_handler_top", 0);        // the handler that would catch a throw escaping this dispatch
    abi::emit_store_reg_to_symbol(emitter, "x11", "_rt_diag_dispatch_handler", 0); // `__rt_throw_current` ends the dispatch when it unwinds to it
    emitter.instruction("mov x11, #1");
    abi::emit_store_reg_to_symbol(emitter, "x11", "_rt_diag_dispatching", 0);   // a diagnostic raised inside the handler must not re-enter
    abi::emit_load_symbol_to_reg(emitter, "x11", "_concat_off", 0);             // snapshot the shared concat cursor
    emitter.instruction(&format!("str x11, [sp, #{}]", DIAG_SLOT_CONCAT));      // the PHP call builds strings in the same scratch buffer
    abi::emit_symbol_address(emitter, "x0", "_rt_diag_buf");                    // arg 1 = the accumulated diagnostic bytes
    abi::emit_call_label(emitter, &crate::names::function_symbol(crate::names::DIAG_RENDER_FUNCTION));
    emitter.instruction(&format!("str x0, [sp, #{}]", DIAG_SLOT_RESULT));       // non-zero: the display is already taken care of
    emitter.instruction(&format!("ldr x11, [sp, #{}]", DIAG_SLOT_CONCAT));
    abi::emit_store_reg_to_symbol(emitter, "x11", "_concat_off", 0);            // restore the caller's concat cursor
    emitter.instruction("mov x11, xzr");
    abi::emit_store_reg_to_symbol(emitter, "x11", "_rt_diag_dispatching", 0);   // dispatch finished; diagnostics may buffer again
    emitter.instruction(&format!("ldr x11, [sp, #{}]", DIAG_SLOT_RESULT));
    emitter.instruction("cbnz x11, __rt_diag_warning_buffer_ret");              // handled (or already rendered by PHP): write nothing

    // -- nobody took it: the original stderr write, byte for byte --
    emitter.instruction("mov x0, #2");                                          // fd = stderr for runtime warning diagnostics
    abi::emit_symbol_address(emitter, "x1", "_rt_diag_buf");
    emitter.instruction(&format!("ldr x2, [sp, #{}]", DIAG_SLOT_LENGTH));
    emitter.syscall(4);

    emitter.label("__rt_diag_warning_buffer_ret");
    emitter.instruction("ldp x1, x2, [sp, #0]");                                // restore the fragment pointer and length
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // restore the caller-saved registers
    emitter.instruction("ldp x5, x6, [sp, #32]");                               // restore the caller-saved registers
    emitter.instruction("ldp x7, x8, [sp, #48]");                               // restore the caller-saved registers
    emitter.instruction("ldp x11, x12, [sp, #64]");                             // restore the caller-saved registers
    emitter.instruction("ldp x13, x14, [sp, #80]");                             // restore the caller-saved registers
    emitter.instruction("ldp x15, x17, [sp, #96]");                             // restore the caller-saved registers
    emitter.instruction("ldr x18, [sp, #112]");                                 // restore the platform register
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", DIAG_SLOT_LINKAGE)); // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", DIAG_FRAME_BYTES));         // release the diagnostic accumulation frame
    emitter.instruction("ret");                                                 // return to the raise site
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
/// * `features` - See [`emit_diagnostics`].
///
/// # ABI constraints
/// - `__rt_diag_push_suppression`: reads/writes `_rt_diag_suppression` via RIP-relative load/store.
/// - `__rt_diag_pop_suppression`: guards decrement against zero to prevent underflow.
/// - `__rt_diag_write`: uses Linux `write` syscall (number 1) with arguments in rdi, rsi, rdx.
fn emit_diagnostics_linux_x86_64(emitter: &mut Emitter, features: RuntimeFeatures) {
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

    emitter.label_global("__rt_diag_write");
    if emitter.phpext_output { emit_phpext_diagnostic(emitter); return; }
    abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_diag_suppression", 0);    // load suppression depth before deciding whether to emit the warning
    emitter.instruction("test r10, r10");                                       // is runtime warning output currently suppressed?
    if features.diag_user_handler {
        emitter.instruction("jz __rt_diag_warning_live_linux_x86_64");          // not inside an @ scope: consider buffering
        emitter.instruction("xor r10d, r10d");
        abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_complete", 0);  // a dropped message must not mark the next one complete
        emitter.instruction("ret");                                             // suppress the warning while inside an active @ scope
        emitter.label("__rt_diag_warning_live_linux_x86_64");
        abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_diag_dispatching", 0); // is a diagnostic already being handed to PHP?
        emitter.instruction("test r10, r10");                                   // a diagnostic raised inside the handler never re-enters
        emitter.instruction("jz __rt_diag_warning_buffer_linux_x86_64");        // ordinary diagnostic: accumulate it for the handler
        emitter.instruction("xor r10d, r10d");
        abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_complete", 0);  // written straight through, so nothing is pending
    } else {
        emitter.instruction("jnz __rt_diag_warning_done_linux_x86_64");         // suppress the warning while inside an active @ scope
    }
    emitter.instruction("mov rdx, rsi");                                        // move warning length into the Linux write length register
    emitter.instruction("mov rsi, rdi");                                        // move warning pointer into the Linux write buffer register
    emitter.instruction("mov edi, 2");                                          // fd = stderr for runtime warning diagnostics
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // emit the runtime warning diagnostic to stderr
    emitter.label("__rt_diag_warning_done_linux_x86_64");
    emitter.instruction("ret");                                                 // return after either writing or suppressing the warning

    if features.diag_user_handler {
        emit_diag_buffered_path_x86_64(emitter);
    }
}

/// Emits the x86_64 accumulate-and-dispatch path of `__rt_diag_warning`.
///
/// Mirrors [`emit_diag_buffered_path_aarch64`]; see that function for why the message is
/// accumulated rather than written per fragment, and why the frame is this wide.
fn emit_diag_buffered_path_x86_64(emitter: &mut Emitter) {
    emitter.label("__rt_diag_warning_buffer_linux_x86_64");
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable diagnostic frame
    emitter.instruction(&format!("sub rsp, {}", DIAG_FRAME_BYTES_X86));         // reserve the diagnostic accumulation frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the fragment pointer for the caller
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the fragment length for the caller
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve caller-saved registers the PHP call would destroy
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // preserve caller-saved registers the PHP call would destroy
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // preserve caller-saved registers the PHP call would destroy
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // preserve caller-saved registers the PHP call would destroy
    emitter.instruction("mov QWORD PTR [rbp - 56], r10");                       // preserve caller-saved registers the PHP call would destroy
    emitter.instruction("mov QWORD PTR [rbp - 64], r11");                       // preserve caller-saved registers the PHP call would destroy
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // preserve caller-saved registers the PHP call would destroy

    // -- append the fragment --
    abi::emit_load_symbol_to_reg(emitter, "rax", "_rt_diag_buf_len", 0);        // rax = bytes already accumulated for this diagnostic
    abi::emit_symbol_address(emitter, "rcx", "_rt_diag_buf");
    emitter.label("__rt_diag_warning_append_linux_x86_64");
    emitter.instruction("test rsi, rsi");                                       // any fragment bytes left?
    emitter.instruction("jz __rt_diag_warning_appended_linux_x86_64");          // the whole fragment has been copied
    emitter.instruction(&format!("cmp rax, {}", RT_DIAG_BUF_BYTES));            // is the accumulator full?
    emitter.instruction("jae __rt_diag_warning_appended_linux_x86_64");         // truncate rather than overrun the accumulator
    emitter.instruction("mov dl, BYTE PTR [rdi]");                              // read one fragment byte
    emitter.instruction("mov BYTE PTR [rcx + rax], dl");                        // append the byte to the accumulated diagnostic
    emitter.instruction("inc rdi");                                             // advance the source cursor
    emitter.instruction("inc rax");                                             // advance the accumulator cursor
    emitter.instruction("dec rsi");                                             // one fewer fragment byte to copy
    emitter.instruction("jmp __rt_diag_warning_append_linux_x86_64");           // continue copying the fragment
    emitter.label("__rt_diag_warning_appended_linux_x86_64");
    abi::emit_store_reg_to_symbol(emitter, "rax", "_rt_diag_buf_len", 0);       // publish the accumulated length

    // -- is the diagnostic complete? --
    emitter.instruction("test rax, rax");                                       // anything accumulated at all?
    emitter.instruction("jz __rt_diag_warning_buffer_ret_linux_x86_64");        // nothing accumulated yet
    abi::emit_load_symbol_to_reg(emitter, "rcx", "_rt_diag_complete", 0);       // did the caller say this was the last fragment?
    emitter.instruction("xor edx, edx");
    abi::emit_store_reg_to_symbol(emitter, "rdx", "_rt_diag_complete", 0);      // the marker is consumed by exactly one diagnostic
    emitter.instruction("test rcx, rcx");
    emitter.instruction("jnz __rt_diag_warning_flush_linux_x86_64");            // a whole-message caller flushes without a newline
    abi::emit_symbol_address(emitter, "rcx", "_rt_diag_buf");
    emitter.instruction("mov dl, BYTE PTR [rcx + rax - 1]");                    // load the last accumulated byte
    emitter.instruction("cmp dl, 10");                                          // every rendered diagnostic ends with a newline
    emitter.instruction("jne __rt_diag_warning_buffer_ret_linux_x86_64");       // still mid-message: wait for the rest
    emitter.label("__rt_diag_warning_flush_linux_x86_64");

    // -- hand the whole diagnostic to PHP --
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // keep the length for the fallback write
    emitter.instruction("mov rsi, rax");                                        // arg 2 = accumulated length
    emitter.instruction("xor eax, eax");
    // Cleared BEFORE the call; see the AArch64 mirror for why.
    abi::emit_store_reg_to_symbol(emitter, "rax", "_rt_diag_buf_len", 0);       // the accumulator is now owned by this flush
    abi::emit_load_symbol_to_reg(emitter, "rdx", "_rt_diag_file_ptr", 0);       // arg 3 = raise-site file pointer (0 when unknown)
    abi::emit_load_symbol_to_reg(emitter, "rcx", "_rt_diag_file_len", 0);       // arg 4 = raise-site file length
    abi::emit_load_symbol_to_reg(emitter, "r8", "_rt_diag_line", 0);            // arg 5 = raise-site line (0 when unknown)
    emitter.instruction("xor eax, eax");
    abi::emit_store_reg_to_symbol(emitter, "rax", "_rt_diag_file_ptr", 0);      // a location is consumed by exactly one diagnostic
    abi::emit_store_reg_to_symbol(emitter, "rax", "_rt_diag_file_len", 0);      // so the next one cannot inherit this one's
    abi::emit_store_reg_to_symbol(emitter, "rax", "_rt_diag_line", 0);          // and report a line it was never raised on
    abi::emit_load_symbol_to_reg(emitter, "rax", "_exc_handler_top", 0);        // the handler that would catch a throw escaping this dispatch
    abi::emit_store_reg_to_symbol(emitter, "rax", "_rt_diag_dispatch_handler", 0); // `__rt_throw_current` ends the dispatch when it unwinds to it
    emitter.instruction("mov rax, 1");
    abi::emit_store_reg_to_symbol(emitter, "rax", "_rt_diag_dispatching", 0);   // a diagnostic raised inside the handler must not re-enter
    abi::emit_load_symbol_to_reg(emitter, "rax", "_concat_off", 0);             // snapshot the shared concat cursor
    emitter.instruction("mov QWORD PTR [rbp - 88], rax");                       // the PHP call builds strings in the same scratch buffer
    abi::emit_symbol_address(emitter, "rdi", "_rt_diag_buf");                   // arg 1 = the accumulated diagnostic bytes
    abi::emit_call_label(emitter, &crate::names::function_symbol(crate::names::DIAG_RENDER_FUNCTION));
    emitter.instruction("mov QWORD PTR [rbp - 96], rax");                       // non-zero: the display is already taken care of
    emitter.instruction("mov rax, QWORD PTR [rbp - 88]");
    abi::emit_store_reg_to_symbol(emitter, "rax", "_concat_off", 0);            // restore the caller's concat cursor
    emitter.instruction("xor eax, eax");
    abi::emit_store_reg_to_symbol(emitter, "rax", "_rt_diag_dispatching", 0);   // dispatch finished; diagnostics may buffer again
    emitter.instruction("mov rax, QWORD PTR [rbp - 96]");
    emitter.instruction("test rax, rax");                                       // did PHP take responsibility for the display?
    emitter.instruction("jnz __rt_diag_warning_buffer_ret_linux_x86_64");       // handled (or already rendered by PHP): write nothing

    // -- nobody took it: the original stderr write, byte for byte --
    abi::emit_symbol_address(emitter, "rsi", "_rt_diag_buf");                   // buffer = the accumulated diagnostic
    emitter.instruction("mov rdx, QWORD PTR [rbp - 80]");                       // length = what was accumulated
    emitter.instruction("mov edi, 2");                                          // fd = stderr for runtime warning diagnostics
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // emit the runtime warning diagnostic to stderr

    emitter.label("__rt_diag_warning_buffer_ret_linux_x86_64");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // restore the fragment pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // restore the fragment length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // restore the caller-saved registers
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // restore the caller-saved registers
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // restore the caller-saved registers
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // restore the caller-saved registers
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // restore the caller-saved registers
    emitter.instruction("mov r11, QWORD PTR [rbp - 64]");                       // restore the caller-saved registers
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // restore the caller-saved registers
    emitter.instruction("mov rsp, rbp");                                        // release the diagnostic accumulation frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the raise site
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
