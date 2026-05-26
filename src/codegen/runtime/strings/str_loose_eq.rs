//! Purpose:
//! Emits PHP loose equality for two runtime strings.
//! Numeric strings compare by numeric value; non-numeric strings compare byte-for-byte.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::strings`.
//!
//! Key details:
//! - Both operands must be parsed before falling back to byte equality so numeric-looking strings follow PHP 8 rules.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// Emits the `__rt_str_loose_eq` runtime routine.
/// Compares two PHP strings using loose equality (== semantics).
///
/// Input registers:
///   - ARM64: x1/x2 = left (ptr, len), x3/x4 = right (ptr, len)
///   - x86_64: rdi/rsi = left (ptr, len), rdx/rcx = right (ptr, len)
///
/// Both operands are first parsed as PHP numeric strings via `__rt_str_to_number`.
/// If both parse as numeric, their parsed float values are compared for equality.
/// If either operand is non-numeric, falls back to byte-for-byte comparison via `__rt_str_eq`.
///
/// Output:
///   - ARM64: x0 = 1 if loosely equal, 0 otherwise
///   - x86_64: rax = 1 if loosely equal, 0 otherwise
///
/// Calls: `__rt_str_to_number` (twice) and `__rt_str_eq` (on fallback path).
pub fn emit_str_loose_eq(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_str_loose_eq_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: str_loose_eq ---");
    emitter.label_global("__rt_str_loose_eq");

    emitter.instruction("sub sp, sp, #80");                                     // allocate helper slots for both strings and parsed numeric state
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish a stable helper frame pointer
    emitter.instruction("stp x1, x2, [sp, #0]");                                // save the left string pointer and length
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save the right string pointer and length

    emitter.instruction("bl __rt_str_to_number");                               // parse the left string as a PHP numeric string
    emitter.instruction("str x0, [sp, #32]");                                   // save whether the left string parsed as numeric
    emitter.instruction("str d0, [sp, #40]");                                   // save the parsed left numeric value
    emitter.instruction("ldp x1, x2, [sp, #16]");                               // reload the right string into the parser input registers
    emitter.instruction("bl __rt_str_to_number");                               // parse the right string as a PHP numeric string
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the left numeric-string flag
    emitter.instruction("cbz x9, __rt_str_loose_eq_bytes");                     // non-numeric left strings compare by bytes
    emitter.instruction("cbz x0, __rt_str_loose_eq_bytes");                     // non-numeric right strings compare by bytes
    emitter.instruction("ldr d1, [sp, #40]");                                   // reload the parsed left numeric value
    emitter.instruction("fcmp d1, d0");                                         // compare the numeric values for equality
    emitter.instruction("cset x0, eq");                                         // produce true only when the parsed numeric values match
    emitter.instruction("b __rt_str_loose_eq_done");                            // skip the byte-comparison fallback

    emitter.label("__rt_str_loose_eq_bytes");
    emitter.instruction("ldp x1, x2, [sp, #0]");                                // reload the left string pointer and length
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // reload the right string pointer and length
    emitter.instruction("bl __rt_str_eq");                                      // compare non-numeric strings byte-for-byte

    emitter.label("__rt_str_loose_eq_done");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the helper stack frame
    emitter.instruction("ret");                                                 // return the loose string equality result in x0
}

/// x86_64 Linux implementation of the `__rt_str_loose_eq` runtime routine.
/// Identical logic to the ARM64 path but uses the System V AMD64 ABI:
///   - rdi/rsi = left (ptr, len), rdx/rcx = right (ptr, len)
///   - rax = result (1 if loosely equal, 0 otherwise)
///
/// Saves the left string in `[rbp - 8..16]` and right string in `[rbp - 24..32]`
/// to preserve them across the two `__rt_str_to_number` calls.
/// Uses `[rbp - 40]` for the left numeric-flag and `[rbp - 48]` for the left numeric value.
///
/// Falls back to `__rt_str_eq` when either operand is non-numeric.
fn emit_str_loose_eq_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_loose_eq ---");
    emitter.label_global("__rt_str_loose_eq");

    emitter.instruction("push rbp");                                            // save the caller frame pointer before nested runtime calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 80");                                         // allocate aligned helper slots for both strings and parsed numeric state
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the left string pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the left string length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the right string pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the right string length

    emitter.instruction("mov rax, rdi");                                        // move the left string pointer into the parser input register
    emitter.instruction("mov rdx, rsi");                                        // move the left string length into the parser input register
    abi::emit_call_label(emitter, "__rt_str_to_number");                        // parse the left string as a PHP numeric string
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save whether the left string parsed as numeric
    emitter.instruction("movsd QWORD PTR [rbp - 48], xmm0");                    // save the parsed left numeric value
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the right string pointer into the parser input register
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // reload the right string length into the parser input register
    abi::emit_call_label(emitter, "__rt_str_to_number");                        // parse the right string as a PHP numeric string
    emitter.instruction("cmp QWORD PTR [rbp - 40], 0");                         // did the left string parse as numeric?
    emitter.instruction("je __rt_str_loose_eq_bytes_linux_x86_64");             // non-numeric left strings compare by bytes
    emitter.instruction("test rax, rax");                                       // did the right string parse as numeric?
    emitter.instruction("je __rt_str_loose_eq_bytes_linux_x86_64");             // non-numeric right strings compare by bytes
    emitter.instruction("movsd xmm1, QWORD PTR [rbp - 48]");                    // reload the parsed left numeric value
    emitter.instruction("ucomisd xmm1, xmm0");                                  // compare the parsed numeric values
    emitter.instruction("sete al");                                             // produce true only when the numeric values match
    emitter.instruction("movzx rax, al");                                       // widen the boolean byte into the full result register
    emitter.instruction("jmp __rt_str_loose_eq_done_linux_x86_64");             // skip the byte-comparison fallback

    emitter.label("__rt_str_loose_eq_bytes_linux_x86_64");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the left string pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the left string length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload the right string pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the right string length
    abi::emit_call_label(emitter, "__rt_str_eq");                               // compare non-numeric strings byte-for-byte

    emitter.label("__rt_str_loose_eq_done_linux_x86_64");
    emitter.instruction("add rsp, 80");                                         // release the helper stack frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the loose string equality result in rax
}
