//! Purpose:
//! Emits the scalar truthiness lookup shared by TLS stream-context consumers.
//! It composes the existing integer and string option helpers.
//!
//! Called from:
//! - HTTPS and `stream_socket_enable_crypto` policy lowering.
//!
//! Key details:
//! - Missing options leave the caller-provided default untouched.
//! - Integer and bool values use their numeric truthiness; strings are false
//!   only when empty or exactly `"0"`, matching PHP scalar truthiness.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits `__rt_get_bool_context_option`.
///
/// Input: AArch64 x0..x3 = wrapper/option pointer-length pairs, x4 = output address.
///        x86_64 rdi/rsi/rdx/rcx = pairs, r8 = output address.
/// Output: 1 when a supported scalar option was found, otherwise 0.
pub fn emit_get_bool_context_option(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_get_bool_context_option_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: get_bool_context_option ---");
    emitter.label_global("__rt_get_bool_context_option");

    // Frame (96 bytes): saved input args at 0..32, string scratch at 40..48,
    // saved frame pointer and return address at 80..88.
    emitter.instruction("sub sp, sp, #96");                                     // allocate the scalar lookup frame
    emitter.instruction("stp x29, x30, [sp, #80]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #80");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the wrapper pointer
    emitter.instruction("str x1, [sp, #8]");                                    // preserve the wrapper length
    emitter.instruction("str x2, [sp, #16]");                                   // preserve the option pointer
    emitter.instruction("str x3, [sp, #24]");                                   // preserve the option length
    emitter.instruction("str x4, [sp, #32]");                                   // preserve the caller output address

    // -- prefer native integer/bool context values --
    emitter.instruction("bl __rt_get_int_context_option");                      // resolve integer and bool values directly
    emitter.instruction("cbz x0, __rt_gbco_string");                            // fall back when no integer or bool value exists
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the caller output address
    emitter.instruction("ldr x10, [x9]");                                       // load the integer or bool payload
    emitter.instruction("cmp x10, #0");                                         // PHP treats only integer zero as false
    emitter.instruction("cset x10, ne");                                        // normalize every nonzero integer to true
    emitter.instruction("str x10, [x9]");                                       // publish the normalized boolean
    emitter.instruction("b __rt_gbco_done");                                    // return the integer helper hit
    emitter.label("__rt_gbco_string");

    // -- fall back to PHP string truthiness --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the wrapper pointer
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the wrapper length
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the option pointer
    emitter.instruction("ldr x3, [sp, #24]");                                   // reload the option length
    emitter.instruction("add x4, sp, #40");                                     // pass the string-pointer scratch address
    emitter.instruction("add x5, sp, #48");                                     // pass the string-length scratch address
    emitter.instruction("bl __rt_get_string_context_option");                   // resolve string values from the same context entry
    emitter.instruction("cbz x0, __rt_gbco_done");                              // preserve the caller default when the option is missing
    emitter.instruction("ldr x9, [sp, #48]");                                   // load the resolved string length
    emitter.instruction("cbz x9, __rt_gbco_false");                             // the empty string is false
    emitter.instruction("cmp x9, #1");                                          // only a one-byte string can be the special false value
    emitter.instruction("b.ne __rt_gbco_true");                                 // every longer non-empty string is true
    emitter.instruction("ldr x10, [sp, #40]");                                  // load the resolved string pointer
    emitter.instruction("ldrb w11, [x10]");                                     // inspect the sole string byte
    emitter.instruction("cmp w11, #48");                                        // compare with ASCII `0`
    emitter.instruction("b.eq __rt_gbco_false");                                // exactly `"0"` is false

    emitter.label("__rt_gbco_true");
    emitter.instruction("mov x9, #1");                                          // materialize PHP true
    emitter.instruction("b __rt_gbco_store");                                   // store the normalized boolean
    emitter.label("__rt_gbco_false");
    emitter.instruction("mov x9, #0");                                          // materialize PHP false
    emitter.label("__rt_gbco_store");
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the caller output address
    emitter.instruction("str x9, [x10]");                                       // publish the normalized boolean value
    emitter.instruction("mov x0, #1");                                          // report a supported scalar hit

    emitter.label("__rt_gbco_done");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #96");                                     // release the scalar lookup frame
    emitter.instruction("ret");                                                 // return the hit or miss status
}

/// Emits the x86_64 implementation of `__rt_get_bool_context_option`.
fn emit_get_bool_context_option_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: get_bool_context_option ---");
    emitter.label_global("__rt_get_bool_context_option");

    // rbp-relative frame: saved inputs at -8..-40 and string scratch at -48..-56.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 64");                                         // reserve aligned input and string scratch storage
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the wrapper pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the wrapper length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the option pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // preserve the option length
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // preserve the caller output address

    // -- prefer native integer/bool context values --
    emitter.instruction("call __rt_get_int_context_option");                    // resolve integer and bool values directly
    emitter.instruction("test rax, rax");                                       // did the integer helper find a supported value?
    emitter.instruction("jz __rt_gbco_string_x86");                             // fall back when no integer or bool value exists
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the caller output address
    emitter.instruction("cmp QWORD PTR [r10], 0");                              // PHP treats only integer zero as false
    emitter.instruction("xor r11d, r11d");                                      // clear the full normalization register
    emitter.instruction("setne r11b");                                          // normalize every nonzero integer to true
    emitter.instruction("mov QWORD PTR [r10], r11");                            // publish the normalized boolean
    emitter.instruction("mov eax, 1");                                          // preserve the integer helper hit result
    emitter.instruction("jmp __rt_gbco_done_x86");                              // return the normalized integer hit
    emitter.label("__rt_gbco_string_x86");

    // -- fall back to PHP string truthiness --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the wrapper pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the wrapper length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload the option pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the option length
    emitter.instruction("lea r8, [rbp - 48]");                                  // pass the string-pointer scratch address
    emitter.instruction("lea r9, [rbp - 56]");                                  // pass the string-length scratch address
    emitter.instruction("call __rt_get_string_context_option");                 // resolve string values from the same context entry
    emitter.instruction("test rax, rax");                                       // did the string helper find a supported value?
    emitter.instruction("jz __rt_gbco_done_x86");                               // preserve the caller default when the option is missing
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // load the resolved string length
    emitter.instruction("test r10, r10");                                       // is the resolved string empty?
    emitter.instruction("jz __rt_gbco_false_x86");                              // the empty string is false
    emitter.instruction("cmp r10, 1");                                          // only a one-byte string can be the special false value
    emitter.instruction("jne __rt_gbco_true_x86");                              // every longer non-empty string is true
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // load the resolved string pointer
    emitter.instruction("cmp BYTE PTR [r11], 48");                              // compare the sole byte with ASCII `0`
    emitter.instruction("je __rt_gbco_false_x86");                              // exactly `"0"` is false

    emitter.label("__rt_gbco_true_x86");
    emitter.instruction("mov r10, 1");                                          // materialize PHP true
    emitter.instruction("jmp __rt_gbco_store_x86");                             // store the normalized boolean
    emitter.label("__rt_gbco_false_x86");
    emitter.instruction("xor r10d, r10d");                                      // materialize PHP false
    emitter.label("__rt_gbco_store_x86");
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the caller output address
    emitter.instruction("mov QWORD PTR [r11], r10");                            // publish the normalized boolean value
    emitter.instruction("mov eax, 1");                                          // report a supported scalar hit

    emitter.label("__rt_gbco_done_x86");
    emitter.instruction("add rsp, 64");                                         // release input and string scratch storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the hit or miss status
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::platform::{Platform, Target};

    use super::*;

    /// Verifies both backends compose typed lookup with PHP string truthiness.
    #[test]
    fn bool_context_lookup_preserves_php_scalar_truthiness() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_get_bool_context_option(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_get_int_context_option"));
            assert!(asm.contains("__rt_get_string_context_option"));
            match target.arch {
                Arch::AArch64 => {
                    assert!(asm.contains("cset x10, ne"));
                    assert!(asm.contains("cbz x9, __rt_gbco_false"));
                    assert!(asm.contains("cmp w11, #48"));
                }
                Arch::X86_64 => {
                    assert!(asm.contains("setne r11b"));
                    assert!(asm.contains("jz __rt_gbco_false_x86"));
                    assert!(asm.contains("cmp BYTE PTR [r11], 48"));
                }
            }
        }
    }
}
