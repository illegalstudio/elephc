//! Purpose:
//! Emits runtime diagnostic suppression and warning-output helpers.
//! The helpers implement PHP-style @ suppression depth and route warnings to php's diagnostic
//! stream for each target ABI.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` before PHP-visible helper emission.
//!
//! Key details:
//! - Suppression depth lives in _rt_diag_suppression.
//! - php's CLI writes diagnostics to STDOUT, not stderr, and through the OUTPUT BUFFER — MEASURED:
//!   `ob_start(fn($s) => "[[$s]]")` wraps a warning the same way it wraps an echo, and `2>&1
//!   1>/dev/null` shows an empty stderr. So `__rt_stdout_write` is the funnel, not a `write(2)`
//!   on fd 2, and a program that captures its own output sees its warnings like php does.
//! - One diagnostic is composed from SEVERAL calls here (head, name, tail). They accumulate in
//!   `_rt_diag_buf` and are written together when the piece carrying the newline arrives, so
//!   php's ` in FILE on line N` suffix is appended once per LINE rather than once per piece.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::abi;
use crate::codegen_support::runtime::data::RT_DIAG_BUF_BYTES;

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
/// - `__rt_diag_warning`: buffers the piece when suppression depth is zero, and writes the whole
///   line through `__rt_stdout_write` once it ends with a newline; silently returns when suppressed.
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

    emitter.label_global("__rt_diag_push_filter_suppression");
    abi::emit_symbol_address(emitter, "x9", "_php_filter_suppression");
    emitter.instruction("ldr x10, [x9]");                                       // load the current filter-scope suppression depth
    emitter.instruction("add x10, x10, #1");                                    // enter one additional filter suppression scope
    emitter.instruction("str x10, [x9]");                                       // publish the incremented filter suppression depth
    emitter.instruction("ret");                                                 // return to the filtered open that silenced its inner opener

    emitter.label_global("__rt_diag_pop_filter_suppression");
    abi::emit_symbol_address(emitter, "x9", "_php_filter_suppression");
    emitter.instruction("ldr x10, [x9]");                                       // load the current filter-scope suppression depth
    emitter.instruction("cbz x10, __rt_diag_pop_filter_done");                  // avoid underflow if filter scopes are already balanced
    emitter.instruction("sub x10, x10, #1");                                    // leave one filter suppression scope
    emitter.instruction("str x10, [x9]");                                       // publish the decremented filter suppression depth
    emitter.label("__rt_diag_pop_filter_done");
    emitter.instruction("ret");                                                 // return to the open that is done with its inner opener

    emitter.label_global("__rt_diag_warning");
    abi::emit_symbol_address(emitter, "x9", "_rt_diag_suppression");
    emitter.instruction("ldr x10, [x9]");                                       // load suppression depth before deciding whether to emit the warning
    emitter.instruction("cbnz x10, __rt_diag_warning_done");                    // suppress the warning while inside an active @ scope
    // The filter machinery's own scope silences the same warnings, through a counter `@` does
    // not touch — so a filtered open can stand its scope DOWN for the wrapper PHP it calls
    // without ever handing back a depth `@` is still holding.
    abi::emit_symbol_address(emitter, "x9", "_php_filter_suppression");
    emitter.instruction("ldr x10, [x9]");                                       // load the filter-scope suppression depth
    emitter.instruction("cbnz x10, __rt_diag_warning_done");                    // suppress while a filtered open silences its inner opener

    // -- accumulate this piece, and write the LINE once the piece ending it arrives --
    //
    // php prints one diagnostic as one line — a blank line, the message, ` in FILE on line N` —
    // and routes it through the output buffer, so `ob_start()` captures it like any echo. elephc
    // composes a message from several calls here (head, name, tail), so appending the location
    // per CALL would stamp it three times. The pieces land in `_rt_diag_buf` and go out together.
    emitter.instruction("stp x29, x30, [sp, #-32]!");                           // this helper now CALLS, so it needs a frame
    emitter.instruction("add x29, sp, #0");
    emitter.instruction("str x1, [sp, #16]");                                   // the piece pointer
    emitter.instruction("str x2, [sp, #24]");                                   // and its length
    abi::emit_symbol_address(emitter, "x9", "_rt_diag_buf_len");
    emitter.instruction("ldr x10, [x9]");                                       // bytes already buffered
    abi::emit_symbol_address(emitter, "x11", "_rt_diag_buf");
    emitter.instruction("mov x12, #0");                                         // index into the incoming piece
    emitter.label("__rt_diag_append_loop");
    emitter.instruction("cmp x12, x2");
    emitter.instruction("b.ge __rt_diag_append_done");
    emitter.instruction("add x13, x10, x12");                                   // where this byte would land
    emitter.instruction(&format!("mov x14, #{}", RT_DIAG_BUF_BYTES - 1));
    emitter.instruction("cmp x13, x14");
    emitter.instruction("b.ge __rt_diag_append_done");                          // truncate rather than run past the buffer
    emitter.instruction("ldrb w15, [x1, x12]");
    emitter.instruction("strb w15, [x11, x13]");
    emitter.instruction("add x12, x12, #1");
    emitter.instruction("b __rt_diag_append_loop");
    emitter.label("__rt_diag_append_done");
    emitter.instruction("add x10, x10, x12");                                   // the new buffered length
    emitter.instruction("str x10, [x9]");

    // -- a buffered line ending in a newline is a COMPLETE diagnostic --
    emitter.instruction("cbz x10, __rt_diag_warning_pop");
    emitter.instruction("sub x13, x10, #1");
    emitter.instruction("ldrb w15, [x11, x13]");
    emitter.instruction("cmp w15, #0x0a");
    emitter.instruction("b.ne __rt_diag_warning_pop");                          // still mid-message: wait for the rest

    abi::emit_symbol_address(emitter, "x0", "_rt_diag_nl");
    emitter.instruction("mov x1, #1");
    emitter.instruction("bl __rt_stdout_write");                                // php opens a diagnostic with a blank line
    abi::emit_symbol_address(emitter, "x9", "_rt_diag_buf_len");
    emitter.instruction("ldr x1, [x9]");
    emitter.instruction("sub x1, x1, #1");                                      // everything but the newline it ends with
    abi::emit_symbol_address(emitter, "x0", "_rt_diag_buf");
    emitter.instruction("bl __rt_stdout_write");
    abi::emit_symbol_address(emitter, "x9", "_rt_diag_loc_len");
    emitter.instruction("ldr x1, [x9]");
    emitter.instruction("cbz x1, __rt_diag_bare_newline");                      // no site published one: end the line plainly
    abi::emit_load_symbol_to_reg(emitter, "x9", "_script_source_file_len", 0);
    emitter.instruction("cbz x9, __rt_diag_bare_newline");                      // a module with no source path omits the location
    abi::emit_symbol_address(emitter, "x0", "_rt_diag_in");
    emitter.instruction("mov x1, #4");                                          // " in "
    emitter.instruction("bl __rt_stdout_write");
    abi::emit_symbol_address(emitter, "x0", "_script_source_file");
    abi::emit_load_symbol_to_reg(emitter, "x1", "_script_source_file_len", 0);
    emitter.instruction("bl __rt_stdout_write");                                // the script php names in every diagnostic
    abi::emit_symbol_address(emitter, "x9", "_rt_diag_loc_ptr");
    emitter.instruction("ldr x0, [x9]");
    abi::emit_symbol_address(emitter, "x9", "_rt_diag_loc_len");
    emitter.instruction("ldr x1, [x9]");
    emitter.instruction("bl __rt_stdout_write");                                // ` on line N` and its newline
    emitter.instruction("b __rt_diag_flush_reset");
    emitter.label("__rt_diag_bare_newline");
    abi::emit_symbol_address(emitter, "x0", "_rt_diag_nl");
    emitter.instruction("mov x1, #1");
    emitter.instruction("bl __rt_stdout_write");
    emitter.label("__rt_diag_flush_reset");
    abi::emit_symbol_address(emitter, "x9", "_rt_diag_buf_len");
    emitter.instruction("str xzr, [x9]");                                       // the line is out; start the next one empty

    emitter.label("__rt_diag_warning_pop");
    emitter.instruction("ldp x29, x30, [sp], #32");                             // release the frame
    emitter.label("__rt_diag_warning_done");
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
/// - `__rt_diag_warning`: uses Linux `write` syscall (number 1) with arguments in rdi, rsi, rdx.
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

    emitter.label_global("__rt_diag_push_filter_suppression");
    abi::emit_load_symbol_to_reg(emitter, "r10", "_php_filter_suppression", 0); // load the current filter-scope suppression depth
    emitter.instruction("add r10, 1");                                          // enter one additional filter suppression scope
    abi::emit_store_reg_to_symbol(emitter, "r10", "_php_filter_suppression", 0); // publish the incremented filter suppression depth
    emitter.instruction("ret");                                                 // return to the filtered open that silenced its inner opener

    emitter.label_global("__rt_diag_pop_filter_suppression");
    abi::emit_load_symbol_to_reg(emitter, "r10", "_php_filter_suppression", 0); // load the current filter-scope suppression depth
    emitter.instruction("test r10, r10");                                       // check whether a filter scope is active before decrementing
    emitter.instruction("jz __rt_diag_pop_filter_done_linux_x86_64");           // avoid underflow if filter scopes are already balanced
    emitter.instruction("sub r10, 1");                                          // leave one filter suppression scope
    abi::emit_store_reg_to_symbol(emitter, "r10", "_php_filter_suppression", 0); // publish the decremented filter suppression depth
    emitter.label("__rt_diag_pop_filter_done_linux_x86_64");
    emitter.instruction("ret");                                                 // return to the open that is done with its inner opener

    emitter.label_global("__rt_diag_warning");
    abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_diag_suppression", 0);    // load suppression depth before deciding whether to emit the warning
    emitter.instruction("test r10, r10");                                       // is runtime warning output currently suppressed?
    emitter.instruction("jnz __rt_diag_warning_done_linux_x86_64");             // suppress the warning while inside an active @ scope
    // See the AArch64 counterpart: the filter machinery's scope is a SEPARATE counter, so it can
    // stand down for a user wrapper's PHP without giving back a depth `@` is holding.
    abi::emit_load_symbol_to_reg(emitter, "r10", "_php_filter_suppression", 0); // load the filter-scope suppression depth
    emitter.instruction("test r10, r10");                                       // is a filtered open silencing its inner opener?
    emitter.instruction("jnz __rt_diag_warning_done_linux_x86_64");             // suppress the warning for the length of that scope

    // See the AArch64 counterpart: the pieces of one diagnostic are accumulated and the line is
    // written once the piece carrying its newline arrives, through the output-buffer funnel.
    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 32");
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // the piece pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // and its length
    abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_diag_buf_len", 0);        // bytes already buffered
    abi::emit_symbol_address(emitter, "r11", "_rt_diag_buf");
    emitter.instruction("xor r8, r8");                                          // index into the incoming piece
    emitter.label("__rt_diag_append_loop_x");
    emitter.instruction("cmp r8, rsi");
    emitter.instruction("jge __rt_diag_append_done_x");
    emitter.instruction("mov r9, r10");
    emitter.instruction("add r9, r8");                                          // where this byte would land
    emitter.instruction(&format!("cmp r9, {}", RT_DIAG_BUF_BYTES - 1));
    emitter.instruction("jge __rt_diag_append_done_x");                         // truncate rather than run past the buffer
    emitter.instruction("movzx eax, BYTE PTR [rdi + r8]");
    emitter.instruction("mov BYTE PTR [r11 + r9], al");
    emitter.instruction("add r8, 1");
    emitter.instruction("jmp __rt_diag_append_loop_x");
    emitter.label("__rt_diag_append_done_x");
    emitter.instruction("add r10, r8");                                         // the new buffered length
    abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_buf_len", 0);

    emitter.instruction("test r10, r10");
    emitter.instruction("jz __rt_diag_warning_pop_x");
    emitter.instruction("mov r9, r10");
    emitter.instruction("sub r9, 1");
    emitter.instruction("movzx eax, BYTE PTR [r11 + r9]");
    emitter.instruction("cmp eax, 0x0a");
    emitter.instruction("jne __rt_diag_warning_pop_x");                         // still mid-message: wait for the rest

    abi::emit_symbol_address(emitter, "rdi", "_rt_diag_nl");
    emitter.instruction("mov rsi, 1");
    emitter.instruction("call __rt_stdout_write");                              // php opens a diagnostic with a blank line
    abi::emit_load_symbol_to_reg(emitter, "rsi", "_rt_diag_buf_len", 0);
    emitter.instruction("sub rsi, 1");                                          // everything but the newline it ends with
    abi::emit_symbol_address(emitter, "rdi", "_rt_diag_buf");
    emitter.instruction("call __rt_stdout_write");
    abi::emit_load_symbol_to_reg(emitter, "rsi", "_rt_diag_loc_len", 0);
    emitter.instruction("test rsi, rsi");
    emitter.instruction("jz __rt_diag_bare_newline_x");                         // no site published one: end the line plainly
    abi::emit_load_symbol_to_reg(emitter, "r10", "_script_source_file_len", 0);
    emitter.instruction("test r10, r10");
    emitter.instruction("jz __rt_diag_bare_newline_x");                         // a module with no source path omits the location
    abi::emit_symbol_address(emitter, "rdi", "_rt_diag_in");
    emitter.instruction("mov rsi, 4");                                          // " in "
    emitter.instruction("call __rt_stdout_write");
    abi::emit_symbol_address(emitter, "rdi", "_script_source_file");
    abi::emit_load_symbol_to_reg(emitter, "rsi", "_script_source_file_len", 0);
    emitter.instruction("call __rt_stdout_write");                              // the script php names in every diagnostic
    abi::emit_load_symbol_to_reg(emitter, "rdi", "_rt_diag_loc_ptr", 0);
    abi::emit_load_symbol_to_reg(emitter, "rsi", "_rt_diag_loc_len", 0);
    emitter.instruction("call __rt_stdout_write");                              // ` on line N` and its newline
    emitter.instruction("jmp __rt_diag_flush_reset_x");
    emitter.label("__rt_diag_bare_newline_x");
    abi::emit_symbol_address(emitter, "rdi", "_rt_diag_nl");
    emitter.instruction("mov rsi, 1");
    emitter.instruction("call __rt_stdout_write");
    emitter.label("__rt_diag_flush_reset_x");
    emitter.instruction("xor r10d, r10d");
    abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_buf_len", 0);       // the line is out; start the next one empty

    emitter.label("__rt_diag_warning_pop_x");
    emitter.instruction("mov rsp, rbp");
    emitter.instruction("pop rbp");
    emitter.label("__rt_diag_warning_done_linux_x86_64");
    emitter.instruction("ret");                                                 // return after either writing or suppressing the warning
}
