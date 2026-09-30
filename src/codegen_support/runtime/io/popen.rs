//! Purpose:
//! Emits the `__rt_popen` runtime helper, which opens a process pipe through
//! the libc `popen` call and returns both its descriptor and owning `FILE*`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - libc `popen` yields a `FILE*`; `fileno` recovers the raw descriptor so
//!   elephc's fd-based `fread`/`fwrite` work on the pipe.
//! - The caller adopts the returned `FILE*` into `StreamState.backend_aux`;
//!   process ownership is never indexed by the reusable OS descriptor.
//! - php does NOT hand the mode to libc as written. `PHP_FUNCTION(popen)`
//!   (php-src ext/standard/file.c:794) strips the FIRST `b` from a copy, then refuses anything
//!   that is not `"r"`, `"rb"`, `"w"` or `"wb"` with a ValueError, and calls libc with the
//!   STRIPPED mode. MEASURED on `php -n` 8.5.6 against elephc:
//!
//!   ```text
//!       mode   php                                      elephc (before)
//!       r      handle                                   handle
//!       rb     handle                                   FALSE
//!       w      handle                                   handle
//!       wb     handle                                   FALSE
//!       r+     ValueError: popen(): Argument #2 …       handle
//!   ```
//!
//!   macOS libc refuses `"rb"` outright, which is where the two `false`s came from, and `"r+"`
//!   it accepts, which is where the missing refusal came from.
//! - An invalid mode is reported to the lowering as -2, which it turns into php's ValueError.
//!   The runtime cannot throw, and the mode is not always a literal, so the check cannot live in
//!   the lowering either.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// popen: open a process pipe and return its descriptor.
/// Input:  AArch64 x1/x2 = command string, x3/x4 = mode string
///         x86_64  rdi/rsi = command string, rdx/rcx = mode string
/// Output: descriptor in x0/rax and owning FILE* in x1/rdx, -1/null on failure, or -2 when the
///         mode is not one php accepts (the lowering turns that into a ValueError).
pub fn emit_popen(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_popen_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: popen ---");
    emitter.label_global("__rt_popen");

    // Frame: [0..16) saved regs, [16) cmd cstr, [24) FILE*, [32..40) mode cstr,
    //        [40) mode ptr, [48) mode len.
    emitter.instruction("sub sp, sp, #64");                                     // frame for the popen state
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x3, [sp, #40]");                                   // save the mode string pointer
    emitter.instruction("str x4, [sp, #48]");                                   // save the mode string length
    emitter.instruction("bl __rt_cstr");                                        // null-terminate the command, x0 = C string
    emitter.instruction("str x0, [sp, #16]");                                   // save the command C-string pointer

    // -- build the mode C-string on the stack (clamped to 7 bytes) --
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the mode string pointer
    emitter.instruction("ldr x10, [sp, #48]");                                  // reload the mode string length
    emitter.instruction("cmp x10, #7");                                         // clamp the mode length to the 7-byte buffer
    emitter.instruction("b.ls __rt_popen_mode_ok");                             // keep short mode strings as-is
    emitter.instruction("mov x10, #7");                                         // truncate an over-long mode string
    emitter.label("__rt_popen_mode_ok");
    emitter.instruction("add x12, sp, #32");                                    // mode C-string buffer base
    emitter.instruction("mov x11, #0");                                         // mode copy index
    emitter.label("__rt_popen_mode_copy");
    emitter.instruction("cmp x11, x10");                                        // copied every mode byte?
    emitter.instruction("b.hs __rt_popen_mode_done");                           // mode copy complete
    emitter.instruction("ldrb w13, [x9, x11]");                                 // load a mode byte
    emitter.instruction("strb w13, [x12, x11]");                                // store it into the buffer
    emitter.instruction("add x11, x11, #1");                                    // advance the copy index
    emitter.instruction("b __rt_popen_mode_copy");                              // keep copying the mode
    emitter.label("__rt_popen_mode_done");
    emitter.instruction("strb wzr, [x12, x11]");                                // NUL-terminate the mode string

    // -- php strips the FIRST 'b' before libc ever sees the mode --
    emitter.instruction("mov x13, #0");                                         // scan index
    emitter.label("__rt_popen_mode_scan");
    emitter.instruction("cmp x13, x11");                                        // scanned the whole mode?
    emitter.instruction("b.hs __rt_popen_mode_checked");                        // no 'b' to strip
    emitter.instruction("ldrb w14, [x12, x13]");
    emitter.instruction("cmp w14, #0x62");                                      // 'b'
    emitter.instruction("b.eq __rt_popen_mode_strip");
    emitter.instruction("add x13, x13, #1");
    emitter.instruction("b __rt_popen_mode_scan");
    emitter.label("__rt_popen_mode_strip");
    emitter.instruction("mov x15, x13");                                        // shift the tail left over the 'b'
    emitter.label("__rt_popen_mode_shift");
    emitter.instruction("cmp x15, x11");                                        // the NUL at index x11 moves too
    emitter.instruction("b.hs __rt_popen_mode_shifted");
    emitter.instruction("add x16, x15, #1");
    emitter.instruction("ldrb w14, [x12, x16]");
    emitter.instruction("strb w14, [x12, x15]");
    emitter.instruction("add x15, x15, #1");
    emitter.instruction("b __rt_popen_mode_shift");
    emitter.label("__rt_popen_mode_shifted");
    emitter.instruction("sub x11, x11, #1");                                    // one byte shorter

    // -- and refuses anything that is not "r", "rb", "w" or "wb" --
    //
    // Read on the STRIPPED mode, exactly as php reads it: `"rb"` has become `"r"` by now, so the
    // two-byte arm only ever sees a mode with a second `b` of its own. An EMPTY mode passes this
    // check and fails in libc instead, which is also what php does.
    emitter.label("__rt_popen_mode_checked");
    emitter.instruction("cmp x11, #2");
    emitter.instruction("b.hi __rt_popen_mode_invalid");                        // longer than two bytes
    emitter.instruction("cmp x11, #1");
    emitter.instruction("b.ne __rt_popen_mode_pair");
    emitter.instruction("ldrb w14, [x12]");
    emitter.instruction("cmp w14, #0x72");                                      // 'r'
    emitter.instruction("b.eq __rt_popen_mode_valid");
    emitter.instruction("cmp w14, #0x77");                                      // 'w'
    emitter.instruction("b.eq __rt_popen_mode_valid");
    emitter.instruction("b __rt_popen_mode_invalid");
    emitter.label("__rt_popen_mode_pair");
    emitter.instruction("cmp x11, #2");
    emitter.instruction("b.ne __rt_popen_mode_valid");                          // empty: php lets libc refuse it
    emitter.instruction("ldrb w14, [x12]");
    emitter.instruction("ldrb w15, [x12, #1]");
    emitter.instruction("cmp w15, #0x62");                                      // "?b"
    emitter.instruction("b.ne __rt_popen_mode_invalid");
    emitter.instruction("cmp w14, #0x72");                                      // "rb"
    emitter.instruction("b.eq __rt_popen_mode_valid");
    emitter.instruction("cmp w14, #0x77");                                      // "wb"
    emitter.instruction("b.eq __rt_popen_mode_valid");
    emitter.instruction("b __rt_popen_mode_invalid");
    emitter.label("__rt_popen_mode_valid");

    // -- popen(command, mode) --
    emitter.instruction("ldr x0, [sp, #16]");                                   // command C-string argument
    emitter.instruction("add x1, sp, #32");                                     // mode C-string argument
    emitter.bl_c("popen");
    emitter.instruction("cbz x0, __rt_popen_fail");                             // a NULL FILE* means popen failed
    emitter.instruction("str x0, [sp, #24]");                                   // save the FILE* across the fileno call

    // -- fileno(FILE*) recovers the raw descriptor --
    emitter.bl_c("fileno");
    emitter.instruction("mov w9, w0");                                          // x9 = the pipe descriptor

    emitter.instruction("ldr x1, [sp, #24]");                                   // return the owning FILE* as backend auxiliary state
    emitter.instruction("mov x0, x9");                                          // return the pipe descriptor
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the frame
    emitter.instruction("ret");                                                 // return the descriptor

    emitter.label("__rt_popen_fail");
    emitter.instruction("mov x0, #-1");                                         // -1 reports a popen failure
    emitter.instruction("mov x1, #0");                                          // failed opens have no backend auxiliary owner
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the frame
    emitter.instruction("ret");                                                 // return the failure result

    emitter.label("__rt_popen_mode_invalid");
    emitter.instruction("mov x0, #-2");                                         // the lowering turns -2 into php's ValueError
    emitter.instruction("mov x1, #0");                                          // nothing was opened, so nothing is owned
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the frame
    emitter.instruction("ret");                                                 // return the refusal cue
}

/// Emits the Linux x86_64 stream runtime helper for popen.
fn emit_popen_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: popen ---");
    emitter.label_global("__rt_popen");

    // Frame: [rbp-8) mode ptr, [rbp-16) mode len, [rbp-24) cmd cstr,
    //        [rbp-32) FILE*, [rbp-40..rbp-32) mode cstr buffer.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // frame for the popen state
    emitter.instruction("mov QWORD PTR [rbp - 8], rdx");                        // save the mode string pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rcx");                       // save the mode string length
    emitter.instruction("mov rax, rdi");                                        // command pointer into the cstr input register
    emitter.instruction("mov rdx, rsi");                                        // command length into the cstr input register
    emitter.instruction("call __rt_cstr");                                      // null-terminate the command, rax = C string
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the command C-string pointer

    // -- build the mode C-string on the stack (clamped to 7 bytes) --
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the mode string pointer
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the mode string length
    emitter.instruction("cmp r9, 7");                                           // clamp the mode length to the 7-byte buffer
    emitter.instruction("jbe __rt_popen_mode_ok_x86");                          // keep short mode strings as-is
    emitter.instruction("mov r9, 7");                                           // truncate an over-long mode string
    emitter.label("__rt_popen_mode_ok_x86");
    emitter.instruction("lea r10, [rbp - 40]");                                 // mode C-string buffer base
    emitter.instruction("xor rcx, rcx");                                        // mode copy index
    emitter.label("__rt_popen_mode_copy_x86");
    emitter.instruction("cmp rcx, r9");                                         // copied every mode byte?
    emitter.instruction("jae __rt_popen_mode_done_x86");                        // mode copy complete
    emitter.instruction("movzx eax, BYTE PTR [r8 + rcx]");                      // load a mode byte
    emitter.instruction("mov BYTE PTR [r10 + rcx], al");                        // store it into the buffer
    emitter.instruction("inc rcx");                                             // advance the copy index
    emitter.instruction("jmp __rt_popen_mode_copy_x86");                        // keep copying the mode
    emitter.label("__rt_popen_mode_done_x86");
    emitter.instruction("mov BYTE PTR [r10 + rcx], 0");                         // NUL-terminate the mode string
    emitter.instruction("mov r9, rcx");                                         // r9 = the mode length that was copied

    // See the AArch64 arm for php's rule and the measurements behind it.
    emitter.instruction("xor r11, r11");                                        // scan index
    emitter.label("__rt_popen_mode_scan_x86");
    emitter.instruction("cmp r11, r9");                                         // scanned the whole mode?
    emitter.instruction("jae __rt_popen_mode_checked_x86");                     // no 'b' to strip
    emitter.instruction("movzx eax, BYTE PTR [r10 + r11]");
    emitter.instruction("cmp al, 0x62");                                        // 'b'
    emitter.instruction("je __rt_popen_mode_strip_x86");
    emitter.instruction("inc r11");
    emitter.instruction("jmp __rt_popen_mode_scan_x86");
    emitter.label("__rt_popen_mode_strip_x86");
    emitter.instruction("mov rcx, r11");                                        // shift the tail left over the 'b'
    emitter.label("__rt_popen_mode_shift_x86");
    emitter.instruction("cmp rcx, r9");                                         // the NUL at index r9 moves too
    emitter.instruction("jae __rt_popen_mode_shifted_x86");
    emitter.instruction("movzx eax, BYTE PTR [r10 + rcx + 1]");
    emitter.instruction("mov BYTE PTR [r10 + rcx], al");
    emitter.instruction("inc rcx");
    emitter.instruction("jmp __rt_popen_mode_shift_x86");
    emitter.label("__rt_popen_mode_shifted_x86");
    emitter.instruction("dec r9");                                              // one byte shorter
    emitter.label("__rt_popen_mode_checked_x86");
    emitter.instruction("cmp r9, 2");
    emitter.instruction("ja __rt_popen_mode_invalid_x86");                      // longer than two bytes
    emitter.instruction("cmp r9, 1");
    emitter.instruction("jne __rt_popen_mode_pair_x86");
    emitter.instruction("movzx eax, BYTE PTR [r10]");
    emitter.instruction("cmp al, 0x72");                                        // 'r'
    emitter.instruction("je __rt_popen_mode_valid_x86");
    emitter.instruction("cmp al, 0x77");                                        // 'w'
    emitter.instruction("je __rt_popen_mode_valid_x86");
    emitter.instruction("jmp __rt_popen_mode_invalid_x86");
    emitter.label("__rt_popen_mode_pair_x86");
    emitter.instruction("cmp r9, 2");
    emitter.instruction("jne __rt_popen_mode_valid_x86");                       // empty: php lets libc refuse it
    emitter.instruction("movzx eax, BYTE PTR [r10 + 1]");
    emitter.instruction("cmp al, 0x62");                                        // "?b"
    emitter.instruction("jne __rt_popen_mode_invalid_x86");
    emitter.instruction("movzx eax, BYTE PTR [r10]");
    emitter.instruction("cmp al, 0x72");                                        // "rb"
    emitter.instruction("je __rt_popen_mode_valid_x86");
    emitter.instruction("cmp al, 0x77");                                        // "wb"
    emitter.instruction("je __rt_popen_mode_valid_x86");
    emitter.instruction("jmp __rt_popen_mode_invalid_x86");
    emitter.label("__rt_popen_mode_valid_x86");

    // -- popen(command, mode) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // command C-string argument
    emitter.instruction("lea rsi, [rbp - 40]");                                 // mode C-string argument
    emitter.bl_c("popen");
    emitter.instruction("test rax, rax");                                       // a NULL FILE* means popen failed
    emitter.instruction("jz __rt_popen_fail_x86");                              // bail out on a popen failure
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the FILE* across the fileno call

    // -- fileno(FILE*) recovers the raw descriptor --
    emitter.instruction("mov rdi, rax");                                        // FILE* argument for fileno
    emitter.bl_c("fileno");
    emitter.instruction("mov r9d, eax");                                        // r9 = the pipe descriptor

    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // return the owning FILE* as backend auxiliary state
    emitter.instruction("mov rax, r9");                                         // return the pipe descriptor
    emitter.instruction("add rsp, 48");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the descriptor

    emitter.label("__rt_popen_fail_x86");
    emitter.instruction("mov rax, -1");                                         // -1 reports a popen failure
    emitter.instruction("xor edx, edx");                                        // failed opens have no backend auxiliary owner
    emitter.instruction("add rsp, 48");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure result

    emitter.label("__rt_popen_mode_invalid_x86");
    emitter.instruction("mov rax, -2");                                         // the lowering turns -2 into php's ValueError
    emitter.instruction("xor edx, edx");                                        // nothing was opened, so nothing is owned
    emitter.instruction("add rsp, 48");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the refusal cue
}
