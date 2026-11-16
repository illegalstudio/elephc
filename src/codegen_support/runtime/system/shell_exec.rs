//! Purpose:
//! Emits the `__rt_shell_exec`, `__rt_cstr` runtime helper assembly for shell exec.
//! Keeps PHP builtin semantics, libc/syscall boundaries, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::system`.
//!
//! Key details:
//! - The helper crosses libc process I/O and must copy command output into runtime-owned string storage.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the `__rt_shell_exec` runtime helper.
///
/// Executes a shell command via `popen()` and reads its entire stdout
/// into the runtime's `concat_buf`, returning a pointer/length pair.
/// Uses `fgetc()` in a loop until EOF, then closes the pipe with `pclose()`.
/// Falls back to empty string (ptr=0, len=0) if `popen()` fails.
///
/// # Input (ARM64)
/// - `x1`: command string pointer
/// - `x2`: command string length
///
/// # Output (ARM64)
/// - `x1`: output string pointer in `concat_buf`
/// - `x2`: output string length
pub fn emit_shell_exec(emitter: &mut Emitter, web: bool) {
    if emitter.target.arch == Arch::X86_64 {
        emit_shell_exec_linux_x86_64(emitter, web);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: shell_exec ---");
    emitter.label_global("__rt_shell_exec");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #80");                                     // allocate 80 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set new frame pointer

    // -- null-terminate the command string --
    emitter.instruction("bl __rt_cstr");                                        // convert to C string → x0=null-terminated cmd

    // -- build "r\0" mode string on the stack --
    emitter.instruction("mov w9, #0x72");                                       // w9 = 'r' (0x72)
    emitter.instruction("strb w9, [sp, #32]");                                  // store 'r' at sp+32
    emitter.instruction("strb wzr, [sp, #33]");                                 // store null terminator at sp+33

    // -- open pipe with popen("cmd", "r") --
    emitter.instruction("add x1, sp, #32");                                     // x1 = pointer to "r\0" on stack
    emitter.bl_c(if web { "elephc_web_application_popen" } else { "popen" }); // popen(cmd, "r") → x0=FILE*
    emitter.instruction("str x0, [sp, #0]");                                    // save FILE* on stack

    // -- reserve a growable output buffer before reading the pipe --
    emitter.instruction("mov x0, #4096");                                       // start with a small scratch-backed capacity
    emitter.instruction("bl __rt_concat_reserve");                              // reserve concat scratch or owned heap storage
    emitter.instruction("str x0, [sp, #8]");                                    // save buffer start ptr
    emitter.instruction("str xzr, [sp, #16]");                                  // bytes written counter = 0
    emitter.instruction("mov x9, #4096");                                       // initial capacity
    emitter.instruction("str x9, [sp, #24]");                                   // save capacity on stack

    // -- read loop: fgetc until EOF --
    emitter.label("__rt_shell_exec_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload FILE* from stack
    emitter.bl_c("fgetc");                                                      // read one byte → w0=char or -1 (EOF)
    emitter.instruction("cmn w0, #1");                                          // compare 32-bit return with -1 (EOF)
    emitter.instruction("b.eq __rt_shell_exec_close");                          // if EOF, stop reading

    // -- grow the result buffer when the next byte would fill it --
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload buffer start
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload bytes written count
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload the current capacity
    emitter.instruction("cmp x10, x11");                                        // is the buffer already full?
    emitter.instruction("b.lo __rt_shell_exec_store");                          // the next byte still fits
    emitter.instruction("str x0, [sp, #40]");                                   // preserve the byte across the grow call
    emitter.instruction("mov x0, x9");                                          // current buffer pointer
    emitter.instruction("mov x1, x10");                                         // bytes to preserve
    emitter.instruction("lsl x2, x11, #1");                                     // double the capacity
    emitter.instruction("bl __rt_concat_grow");                                 // move the output into a larger owned block
    emitter.instruction("mov x9, #1");                                          // shell output is an owned call result, not a concat operand
    emitter.instruction("str x9, [x0, #-8]");                                   // retag the grown block so persistence copies instead of adopting it
    emitter.instruction("str x0, [sp, #8]");                                    // publish the grown buffer pointer
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the old capacity
    emitter.instruction("lsl x9, x9, #1");                                      // double it in the tracking slot
    emitter.instruction("str x9, [sp, #24]");                                   // save the new capacity
    emitter.instruction("ldr x0, [sp, #40]");                                   // restore the pending output byte
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the grown buffer pointer
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the byte count

    // -- store byte in the result buffer --
    emitter.label("__rt_shell_exec_store");
    emitter.instruction("strb w0, [x9, x10]");                                  // store byte at buffer[count]
    emitter.instruction("add x10, x10, #1");                                    // increment byte count
    emitter.instruction("str x10, [sp, #16]");                                  // save updated count
    emitter.instruction("b __rt_shell_exec_loop");                              // continue reading

    // -- close the pipe --
    emitter.label("__rt_shell_exec_close");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload FILE* from stack
    emitter.bl_c("pclose");                                                     // close the pipe

    // -- publish the output string --
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = output buffer start
    emitter.instruction("ldr x2, [sp, #16]");                                   // x2 = output length
    emitter.instruction("bl __rt_concat_publish");                              // advance the scratch offset only for scratch-backed results

    // -- tear down stack frame --
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the x86_64 Linux variant of the `__rt_shell_exec` runtime helper.
/// Uses the System V ABI: command pointer in `rdi`, length in `rsi`.
/// Returns output pointer in `rax`, length in `rdx`.
/// On `popen()` failure, returns null pointer and zero length.
/// Fallback label `__rt_shell_exec_empty` handles the NULL return path.
fn emit_shell_exec_linux_x86_64(emitter: &mut Emitter, web: bool) {
    emitter.blank();
    emitter.comment("--- runtime: shell_exec ---");
    emitter.label_global("__rt_shell_exec");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while the shell helper performs nested libc calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the x86_64 shell helper
    emitter.instruction("sub rsp, 48");                                         // reserve aligned local space for FILE*, counters, buffer, byte scratch, and the popen mode string

    abi::emit_call_label(emitter, "__rt_cstr");                                 // convert the command string result regs into a null-terminated C string in the scratch buffer
    emitter.instruction("mov rdi, rax");                                        // pass the null-terminated command pointer in the SysV first-argument register
    emitter.instruction("mov BYTE PTR [rbp - 32], 0x72");                       // store 'r' as the popen mode string's first byte
    emitter.instruction("mov BYTE PTR [rbp - 31], 0");                          // store the popen mode string's trailing C null terminator
    emitter.instruction("lea rsi, [rbp - 32]");                                 // pass the address of the local \"r\\0\" mode string in the SysV second-argument register
    emitter.bl_c(if web { "elephc_web_application_popen" } else { "popen" }); // popen(cmd, \"r\") → rax=FILE* or NULL on failure
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the FILE* so the read loop and close path can reload it later
    emitter.instruction("test rax, rax");                                       // did popen succeed and return a readable pipe?
    emitter.instruction("je __rt_shell_exec_empty");                            // failed pipes map to the empty PHP string result

    emitter.instruction("mov rax, 4096");                                       // start with a small scratch-backed capacity
    emitter.instruction("call __rt_concat_reserve");                            // reserve concat scratch or owned heap storage
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the growable output buffer pointer for the read loop
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // initialize the shell output length counter at zero bytes
    emitter.instruction("mov QWORD PTR [rbp - 40], 4096");                      // initialize the current capacity

    emitter.label("__rt_shell_exec_loop");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the FILE* for the next fgetc() probe
    emitter.bl_c("fgetc");                                                      // read one byte from the pipe → eax=byte or -1 (EOF)
    emitter.instruction("cmp eax, -1");                                         // did the pipe reach EOF?
    emitter.instruction("je __rt_shell_exec_close");                            // stop reading once the full shell output has been consumed

    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the current shell output length
    emitter.instruction("cmp r9, QWORD PTR [rbp - 40]");                        // is the buffer already full?
    emitter.instruction("jb __rt_shell_exec_store");                            // the next byte still fits
    emitter.instruction("mov BYTE PTR [rbp - 48], al");                         // preserve the byte across the grow call
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // current buffer pointer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // bytes to preserve
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // current capacity
    emitter.instruction("shl rsi, 1");                                          // double the capacity
    emitter.instruction("call __rt_concat_grow");                               // move the output into a larger owned block
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(1))); // materialize the owned-string kind word
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // retag the grown block so persistence copies instead of adopting it
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // publish the grown buffer pointer
    emitter.instruction("shl QWORD PTR [rbp - 40], 1");                         // record the doubled capacity
    emitter.instruction("mov al, BYTE PTR [rbp - 48]");                         // restore the pending output byte

    emitter.label("__rt_shell_exec_store");
    emitter.instruction("mov r8, QWORD PTR [rbp - 24]");                        // reload the output buffer pointer
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the current shell output length
    emitter.instruction("mov BYTE PTR [r8 + r9], al");                          // append the freshly read shell-output byte
    emitter.instruction("add r9, 1");                                           // advance the shell output length after appending one more byte
    emitter.instruction("mov QWORD PTR [rbp - 16], r9");                        // save the updated shell output length for the next read iteration
    emitter.instruction("jmp __rt_shell_exec_loop");                            // continue reading until the pipe reports EOF

    emitter.label("__rt_shell_exec_close");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the FILE* before closing the popen pipe
    emitter.bl_c("pclose");                                                     // close the pipe after the full command output has been read
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the output buffer pointer as the PHP string pointer result
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // return the measured command output length as the PHP string length result
    emitter.instruction("call __rt_concat_publish");                            // advance the scratch offset only for scratch-backed results
    emitter.instruction("add rsp, 48");                                         // release the x86_64 shell helper's aligned local frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the captured shell output
    emitter.instruction("ret");                                                 // return to the caller with the captured shell output ptr/len

    emitter.label("__rt_shell_exec_empty");
    emitter.instruction("mov rax, 0");                                          // return empty string ptr (null) when popen fails
    emitter.instruction("mov rdx, 0");                                          // return empty string len = 0 when popen fails
    emitter.instruction("add rsp, 48");                                         // release the x86_64 shell helper's aligned local frame before returning the empty result
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the empty shell result
    emitter.instruction("ret");                                                 // return to the caller with the empty PHP string result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Web process helpers use the mask-aware bridge on every ABI, while CLI keeps libc popen.
    #[test]
    fn web_process_creation_bridge_is_target_aware() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = crate::codegen_support::platform::Target::parse(name).unwrap();
            for web in [false, true] {
                let mut emitter = Emitter::new(target);
                emit_shell_exec(&mut emitter, web);
                crate::codegen_support::runtime::io::emit_popen(&mut emitter, web);
                let assembly = emitter.output();
                let callee = target.extern_symbol(if web { "elephc_web_application_popen" } else { "popen" });
                let instruction = if target.arch == Arch::AArch64 { "bl" } else { "call" };
                assert_eq!(assembly.matches(&format!("{instruction} {callee}")).count(), 2, "{name}: web={web}");
                assert_eq!(assembly.contains("elephc_web_application_popen"), web, "{name}");
            }
        }
    }
}
