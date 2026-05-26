//! Purpose:
//! Emits the `__rt_strcmp`, `__rt_strcmp_loop` runtime helper assembly for strcmp.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::strings`.
//!
//! Key details:
//! - String helpers use PHP pointer/length pairs and target ABI return registers; heap-backed results must remain refcount-compatible.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_strcmp` runtime helper for lexicographic string comparison.
///
/// Dispatches to the x86_64 variant when `target.arch == Arch::X86_64`; otherwise
/// emits ARM64 assembly inline. The callee owns no heap allocations and the result
/// is determined entirely from the byte sequences and their lengths.
///
/// Register contract (ARM64):
/// - Input: x1 = ptr_a, x2 = len_a, x3 = ptr_b, x4 = len_b
/// - Output: x0 = result (< 0 if a < b, 0 if equal, > 0 if a > b)
///
/// Register contract (x86_64 System V):
/// - Input: rdi = ptr_a, rsi = len_a, rdx = ptr_b, rcx = len_b
/// - Output: rax = result (< 0 if a < b, 0 if equal, > 0 if a > b)
pub fn emit_strcmp(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_strcmp_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: strcmp ---");
    emitter.label_global("__rt_strcmp");

    // -- determine minimum length for comparison --
    emitter.instruction("cmp x2, x4");                                          // compare lengths of both strings
    emitter.instruction("csel x5, x2, x4, lt");                                 // x5 = min(len_a, len_b)
    emitter.instruction("mov x6, #0");                                          // initialize byte index to 0

    // -- compare bytes up to minimum length --
    emitter.label("__rt_strcmp_loop");
    emitter.instruction("cmp x6, x5");                                          // check if we've compared all min-length bytes
    emitter.instruction("b.ge __rt_strcmp_len");                                // if done, compare by string lengths
    emitter.instruction("ldrb w7, [x1, x6]");                                   // load byte from string A at index
    emitter.instruction("ldrb w8, [x3, x6]");                                   // load byte from string B at index
    emitter.instruction("cmp w7, w8");                                          // compare the two bytes
    emitter.instruction("b.ne __rt_strcmp_diff");                               // if different, return their difference
    emitter.instruction("add x6, x6, #1");                                      // advance to next byte
    emitter.instruction("b __rt_strcmp_loop");                                  // continue comparing

    // -- bytes differ: return difference --
    emitter.label("__rt_strcmp_diff");
    emitter.instruction("sub x0, x7, x8");                                      // return char_a - char_b (negative, 0, or positive)
    emitter.instruction("ret");                                                 // return to caller

    // -- all shared bytes equal: compare by length --
    emitter.label("__rt_strcmp_len");
    emitter.instruction("sub x0, x2, x4");                                      // return len_a - len_b as tiebreaker
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the x86_64 Linux implementation of `__rt_strcmp`.
///
/// Uses the System V AMD64 ABI register convention:
/// - Input: rdi = ptr_a, rsi = len_a, rdx = ptr_b, rcx = len_b
/// - Output: rax = result (< 0 if a < b, 0 if equal, > 0 if a > b)
///
/// Compares bytes in the shared prefix first; if all match, returns the length difference.
/// Uses `cmovg` to clamp the comparison bound to the shorter string length.
fn emit_strcmp_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strcmp ---");
    emitter.label_global("__rt_strcmp");

    emitter.instruction("mov r8, rsi");                                         // seed the shared-length comparison bound from the first string length
    emitter.instruction("cmp rsi, rcx");                                        // compare both string lengths to determine the shorter shared prefix
    emitter.instruction("cmovg r8, rcx");                                       // clamp the shared-length comparison bound to the shorter string length
    emitter.instruction("xor r9d, r9d");                                        // start comparing bytes from offset zero

    emitter.label("__rt_strcmp_loop_linux_x86_64");
    emitter.instruction("cmp r9, r8");                                          // have we compared every byte in the shared prefix?
    emitter.instruction("jge __rt_strcmp_len_linux_x86_64");                    // fall back to comparing string lengths once the shared prefix is exhausted
    emitter.instruction("movzx rax, BYTE PTR [rdi + r9]");                      // load the current byte from the first string
    emitter.instruction("movzx r10, BYTE PTR [rdx + r9]");                      // load the current byte from the second string
    emitter.instruction("cmp rax, r10");                                        // compare the current bytes from both strings
    emitter.instruction("jne __rt_strcmp_diff_linux_x86_64");                   // return the byte difference on the first mismatch
    emitter.instruction("add r9, 1");                                           // advance to the next shared-prefix byte after an equal comparison
    emitter.instruction("jmp __rt_strcmp_loop_linux_x86_64");                   // continue comparing the remaining shared-prefix bytes

    emitter.label("__rt_strcmp_diff_linux_x86_64");
    emitter.instruction("sub rax, r10");                                        // return the signed byte difference for the first mismatching character pair
    emitter.instruction("ret");                                                 // return the byte-difference result to the caller

    emitter.label("__rt_strcmp_len_linux_x86_64");
    emitter.instruction("mov rax, rsi");                                        // seed the length-tiebreak result from the first string length
    emitter.instruction("sub rax, rcx");                                        // return the signed length difference once the shared prefix is equal
    emitter.instruction("ret");                                                 // return the length-difference result to the caller
}
