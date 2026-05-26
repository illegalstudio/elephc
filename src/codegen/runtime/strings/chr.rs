//! Purpose:
//! Emits the `__rt_chr` runtime helper assembly for chr.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::strings`.
//!
//! Key details:
//! - String helpers use PHP pointer/length pairs and target ABI return registers; heap-backed results must remain refcount-compatible.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_chr` runtime helper assembly for chr.
///
/// Converts an integer code point to a single-byte string by writing the byte
/// into the shared concat buffer and returning a pointer/length pair.
///
/// # Inputs
/// - `x0`: The character code (only the low byte is used)
///
/// # Outputs
/// - `x1`: Pointer to the byte in the concat buffer
/// - `x2`: Length = 1
///
/// # Side effects
/// - Advances `_concat_off` by 1 byte; result is stored in the concat buffer
///   which is heap-backed and refcount-compatible.
pub fn emit_chr(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_chr_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: chr ---");
    emitter.label_global("__rt_chr");

    // -- get concat_buf write position --
    crate::codegen::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current write offset
    crate::codegen::abi::emit_symbol_address(emitter, "x7", "_concat_buf");

    // -- store single character --
    emitter.instruction("add x1, x7, x8");                                      // compute write position, set as return ptr
    emitter.instruction("strb w0, [x1]");                                       // store the character byte at that position
    emitter.instruction("add x8, x8, #1");                                      // advance offset by 1 byte
    emitter.instruction("str x8, [x6]");                                        // store updated offset to _concat_off
    emitter.instruction("mov x2, #1");                                          // return length = 1 (single character)
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the x86_64 Linux variant of `__rt_chr`.
///
/// Same behavior as the ARM64 variant but uses x86_64 System V ABI registers:
/// - `dil` (low byte of `di`/`rdi`): input character code
/// - `rax`: returned string pointer (concat buffer address)
/// - `rdx`: returned string length = 1
///
/// # Side effects
/// - Reads and updates `_concat_off` and `_concat_buf` globals.
fn emit_chr_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: chr ---");
    emitter.label_global("__rt_chr");

    // -- get concat_buf write position --
    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the current concat-buffer write offset before materializing the chr() result byte
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_concat_buf");

    // -- store single character --
    emitter.instruction("lea rax, [r10 + r9]");                                 // compute the concat-buffer write address and return it as the one-byte string pointer
    emitter.instruction("mov BYTE PTR [rax], dil");                             // store the low byte of the requested character code into concat storage
    emitter.instruction("add r9, 1");                                           // advance the concat-buffer write offset by the single byte that chr() produced
    emitter.instruction("mov QWORD PTR [r8], r9");                              // persist the updated concat-buffer write offset after materializing the chr() result
    emitter.instruction("mov rdx, 1");                                          // return a one-byte string length for the concat-backed chr() result
    emitter.instruction("ret");                                                 // return the concat-backed one-byte string in the standard x86_64 string result registers
}
