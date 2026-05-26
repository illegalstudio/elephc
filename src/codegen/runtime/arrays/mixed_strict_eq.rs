//! Purpose:
//! Emits the `__rt_mixed_strict_eq`, `__rt_mixed_unbox` runtime helper assembly for mixed strict eq.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Mixed helpers use boxed tag/payload cells; tag constants and ownership rules are shared with type checking and codegen.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// Compares two boxed mixed values for strict equality using runtime tag and payload dispatch.
///
/// dispatches to `emit_mixed_strict_eq_linux_x86_64` on x86_64, otherwise uses ARM64 SysV ABI.
/// Saves both operand pointers and calls `__rt_mixed_unbox` on each to extract runtime tags.
/// If tags match, dispatches on the shared tag: scalar/pointer payloads compare word-for-word;
/// string payloads delegate to `__rt_str_eq` for byte-by-byte comparison.
/// Returns 1 in `x0` (ARM64) or `rax` (x86_64) if strictly equal, 0 otherwise.
/// Clobbers: x0–x12, lr. Preserves: x29 (frame pointer).
pub fn emit_mixed_strict_eq(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_strict_eq_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: mixed_strict_eq ---");
    emitter.label_global("__rt_mixed_strict_eq");

    // -- save both mixed operands across helper calls --
    emitter.instruction("sub sp, sp, #64");                                     // allocate stack space for both operands, payloads, and saved frame state
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper stack frame
    emitter.instruction("stp x0, x1, [sp, #0]");                                // save the incoming left/right mixed pointers

    // -- unbox the left payload --
    emitter.instruction("bl __rt_mixed_unbox");                                 // left mixed pointer -> x0=tag, x1=value_lo, x2=value_hi
    emitter.instruction("str x0, [sp, #16]");                                   // save the left runtime tag
    emitter.instruction("stp x1, x2, [sp, #24]");                               // save the left payload words

    // -- unbox the right payload --
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the right mixed pointer into the helper argument register
    emitter.instruction("bl __rt_mixed_unbox");                                 // right mixed pointer -> x0=tag, x1=value_lo, x2=value_hi
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the saved left runtime tag
    emitter.instruction("cmp x9, x0");                                          // strict equality first requires matching runtime tags
    emitter.instruction("b.ne __rt_mixed_strict_eq_false");                     // different payload tags are never strictly equal

    // -- dispatch on the shared concrete runtime tag --
    emitter.instruction("cmp x0, #1");                                          // do both payloads hold strings?
    emitter.instruction("b.eq __rt_mixed_strict_eq_string");                    // strings need byte-by-byte comparison
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the left payload low word
    emitter.instruction("cmp x10, x1");                                         // compare low payload words for scalar/pointer tags
    emitter.instruction("b.ne __rt_mixed_strict_eq_false");                     // mismatched payload low words are not equal
    emitter.instruction("ldr x11, [sp, #32]");                                  // reload the left payload high word
    emitter.instruction("cmp x11, x2");                                         // compare high payload words for string/null padding
    emitter.instruction("b.ne __rt_mixed_strict_eq_false");                     // mismatched payload high words are not equal
    emitter.instruction("mov x0, #1");                                          // matching tag + payload words means strict equality
    emitter.instruction("b __rt_mixed_strict_eq_done");                         // return true after the scalar/pointer comparison

    // -- strings compare by bytes, not by pointer identity --
    emitter.label("__rt_mixed_strict_eq_string");
    emitter.instruction("mov x3, x1");                                          // move right string pointer into the third string-equality argument slot
    emitter.instruction("mov x4, x2");                                          // move right string length into the fourth string-equality argument slot
    emitter.instruction("ldp x1, x2, [sp, #24]");                               // reload the left string pointer/length into the first two argument slots
    emitter.instruction("bl __rt_str_eq");                                      // compare the two string payloads byte-for-byte
    emitter.instruction("b __rt_mixed_strict_eq_done");                         // return the string comparison result

    emitter.label("__rt_mixed_strict_eq_false");
    emitter.instruction("mov x0, #0");                                          // report that the mixed payloads are not strictly equal

    emitter.label("__rt_mixed_strict_eq_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper stack frame
    emitter.instruction("ret");                                                 // return the strict-equality boolean in x0
}

/// x86_64 Linux implementation of mixed strict equality comparison.
///
/// Uses System V AMD64 ABI: left mixed pointer in `rdi`, right in `rsi`.
/// Saves both operands on the stack, calls `__rt_mixed_unbox` on each, then compares
/// tags and payloads. String payloads delegate to `__rt_str_eq`. Returns boolean in `rax`.
/// Clobbers: rax, rcx, rdx, rdi, rsi, r10, r11. Preserves: rbx, rbp, r12–r15.
fn emit_mixed_strict_eq_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_strict_eq ---");
    emitter.label_global("__rt_mixed_strict_eq");

    emitter.instruction("sub rsp, 64");                                         // allocate stack space for both operands, payloads, and the saved comparison state
    emitter.instruction("mov QWORD PTR [rsp], rdi");                            // save the incoming left mixed pointer for the later comparison and cleanup path
    emitter.instruction("mov QWORD PTR [rsp + 8], rsi");                        // save the incoming right mixed pointer for the later comparison and cleanup path

    emitter.instruction("mov rax, rdi");                                        // move the left mixed pointer into the x86_64 mixed-unbox input register
    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // left mixed pointer -> rax=tag, rdi=value_lo, rdx=value_hi
    emitter.instruction("mov QWORD PTR [rsp + 16], rax");                       // save the left runtime tag
    emitter.instruction("mov QWORD PTR [rsp + 24], rdi");                       // save the left payload low word
    emitter.instruction("mov QWORD PTR [rsp + 32], rdx");                       // save the left payload high word

    emitter.instruction("mov rax, QWORD PTR [rsp + 8]");                        // reload the right mixed pointer into the x86_64 mixed-unbox input register
    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // right mixed pointer -> rax=tag, rdi=value_lo, rdx=value_hi
    emitter.instruction("mov r10, QWORD PTR [rsp + 16]");                       // reload the saved left runtime tag
    emitter.instruction("cmp r10, rax");                                        // strict equality first requires matching runtime tags
    emitter.instruction("jne __rt_mixed_strict_eq_false");                      // different payload tags are never strictly equal

    emitter.instruction("cmp rax, 1");                                          // do both payloads hold strings?
    emitter.instruction("je __rt_mixed_strict_eq_string");                      // strings need byte-by-byte comparison
    emitter.instruction("cmp QWORD PTR [rsp + 24], rdi");                       // compare low payload words for scalar or pointer tags
    emitter.instruction("jne __rt_mixed_strict_eq_false");                      // mismatched payload low words are not equal
    emitter.instruction("cmp QWORD PTR [rsp + 32], rdx");                       // compare high payload words for string/null padding
    emitter.instruction("jne __rt_mixed_strict_eq_false");                      // mismatched payload high words are not equal
    emitter.instruction("mov rax, 1");                                          // matching tag plus payload words means strict equality
    emitter.instruction("jmp __rt_mixed_strict_eq_done");                       // return true after the scalar or pointer comparison path

    emitter.label("__rt_mixed_strict_eq_string");
    emitter.instruction("mov rcx, rdx");                                        // move the right string length into the fourth SysV integer argument register
    emitter.instruction("mov rdx, rdi");                                        // move the right string pointer into the third SysV integer argument register
    emitter.instruction("mov rdi, QWORD PTR [rsp + 24]");                       // reload the left string pointer into the first SysV integer argument register
    emitter.instruction("mov rsi, QWORD PTR [rsp + 32]");                       // reload the left string length into the second SysV integer argument register
    abi::emit_call_label(emitter, "__rt_str_eq");                               // compare the two string payloads byte-by-byte
    emitter.instruction("jmp __rt_mixed_strict_eq_done");                       // return the string comparison result

    emitter.label("__rt_mixed_strict_eq_false");
    emitter.instruction("xor rax, rax");                                        // report that the mixed payloads are not strictly equal

    emitter.label("__rt_mixed_strict_eq_done");
    emitter.instruction("add rsp, 64");                                         // release the helper stack frame
    emitter.instruction("ret");                                                 // return the strict-equality boolean in rax
}
