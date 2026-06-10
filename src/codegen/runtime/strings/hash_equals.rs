//! Purpose:
//! Emits the `__rt_hash_equals` runtime helper: a timing-safe string equality
//! check backing PHP's `hash_equals()`. Pure computation — no crypto library.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::strings`.
//!
//! Key details:
//! - Two-string ABI (matches `__rt_strpos`): AArch64 `x1`=known ptr, `x2`=known len,
//!   `x3`=user ptr, `x4`=user len → bool in `x0`; x86_64 `rdi`/`rsi`/`rdx`/`rcx` → `rax`.
//! - Returns false immediately on a length mismatch (lengths are public, so this leaks
//!   no secret-dependent timing). For equal lengths the comparison is constant-time:
//!   it XOR-accumulates every byte pair and only checks the accumulator at the end.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_hash_equals` timing-safe comparison helper for both targets.
///
/// Input: known (ptr,len) + user (ptr,len) in the two-string ABI registers.
/// Output: `1` (true) when the strings are byte-equal, `0` (false) otherwise.
pub fn emit_hash_equals(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_equals_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_equals (timing-safe string compare) ---");
    emitter.label_global("__rt_hash_equals");
    // -- length mismatch is an immediate, non-secret false --
    emitter.instruction("cmp x2, x4");                                          // compare known length against user length
    emitter.instruction("b.ne __rt_hash_equals_false");                         // different lengths → not equal (lengths are public)
    // -- constant-time accumulate over every byte pair --
    emitter.instruction("mov x5, #0");                                          // difference accumulator starts at zero
    emitter.instruction("mov x6, #0");                                          // byte index starts at zero
    emitter.label("__rt_hash_equals_loop");
    emitter.instruction("cmp x6, x2");                                          // have all bytes been compared?
    emitter.instruction("b.ge __rt_hash_equals_done");                          // yes → evaluate the accumulator
    emitter.instruction("ldrb w7, [x1, x6]");                                   // load the next known byte
    emitter.instruction("ldrb w8, [x3, x6]");                                   // load the next user byte
    emitter.instruction("eor w7, w7, w8");                                      // XOR the byte pair (zero only when equal)
    emitter.instruction("orr w5, w5, w7");                                      // fold the difference into the accumulator
    emitter.instruction("add x6, x6, #1");                                      // advance to the next byte
    emitter.instruction("b __rt_hash_equals_loop");                             // keep scanning all bytes regardless of mismatches
    emitter.label("__rt_hash_equals_done");
    emitter.instruction("cmp w5, #0");                                          // any accumulated difference?
    emitter.instruction("cset x0, eq");                                         // x0 = 1 when no byte differed, else 0
    emitter.instruction("ret");                                                 // return the boolean result
    emitter.label("__rt_hash_equals_false");
    emitter.instruction("mov x0, #0");                                          // length mismatch → false
    emitter.instruction("ret");                                                 // return false
}

/// Emits the x86_64 Linux variant of the `__rt_hash_equals` timing-safe comparison.
fn emit_hash_equals_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_equals (timing-safe string compare) ---");
    emitter.label_global("__rt_hash_equals");
    // -- length mismatch is an immediate, non-secret false --
    emitter.instruction("cmp rsi, rcx");                                        // compare known length against user length
    emitter.instruction("jne __rt_hash_equals_false_x86");                      // different lengths → not equal (lengths are public)
    // -- constant-time accumulate over every byte pair --
    emitter.instruction("xor r10d, r10d");                                      // difference accumulator starts at zero
    emitter.instruction("xor r11, r11");                                        // byte index starts at zero
    emitter.label("__rt_hash_equals_loop_x86");
    emitter.instruction("cmp r11, rsi");                                        // have all bytes been compared?
    emitter.instruction("jge __rt_hash_equals_done_x86");                       // yes → evaluate the accumulator
    emitter.instruction("movzx r8d, BYTE PTR [rdi + r11]");                     // load the next known byte
    emitter.instruction("movzx r9d, BYTE PTR [rdx + r11]");                     // load the next user byte
    emitter.instruction("xor r8d, r9d");                                        // XOR the byte pair (zero only when equal)
    emitter.instruction("or r10d, r8d");                                        // fold the difference into the accumulator
    emitter.instruction("add r11, 1");                                          // advance to the next byte
    emitter.instruction("jmp __rt_hash_equals_loop_x86");                       // keep scanning all bytes regardless of mismatches
    emitter.label("__rt_hash_equals_done_x86");
    emitter.instruction("test r10d, r10d");                                     // any accumulated difference?
    emitter.instruction("setz al");                                             // al = 1 when no byte differed, else 0
    emitter.instruction("movzx eax, al");                                       // widen the boolean byte into the result register
    emitter.instruction("ret");                                                 // return the boolean result
    emitter.label("__rt_hash_equals_false_x86");
    emitter.instruction("xor eax, eax");                                        // length mismatch → false
    emitter.instruction("ret");                                                 // return false
}
