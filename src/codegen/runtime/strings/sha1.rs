//! Purpose:
//! Emits the `__rt_sha1` runtime helper assembly for the PHP `sha1()` builtin.
//! Sets up the shared `__rt_hash` register contract with a fixed "sha1" algorithm
//! name and tail-calls into it, so sha1 routes through the elephc-crypto staticlib
//! exactly like `hash("sha1", ...)`.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::strings`.
//!
//! Key details:
//! - Input data string arrives in the string ABI pair (AArch64 x1/x2, x86_64
//!   rax/rdx) and the optional `$binary` flag in AArch64 x5 / x86_64 r10.
//! - The helper loads `_sha1_algo_name` (".asciz \"sha1\"", length 4) into the
//!   algorithm-name register pair, moves the data string into the data register
//!   pair, leaves the binary flag in place, and tail-branches to `__rt_hash`,
//!   which owns the frame, the elephc-crypto call, and `__rt_digest_to_string`
//!   formatting (hex when the flag is 0, raw 20 bytes when it is non-zero).

use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_sha1` runtime helper for the PHP `sha1()` builtin.
///
/// Dispatches to the x86_64 variant on Linux x86_64; otherwise emits the AArch64
/// variant. Both set up the `__rt_hash` register contract for the fixed "sha1"
/// algorithm and tail-branch to `__rt_hash`.
///
/// Input registers:
///   AArch64: x1/x2 = data string ptr/len, x5 = binary flag (0 = hex, else raw).
///   x86_64:  rax/rdx = data string ptr/len, r10 = binary flag.
///
/// Output registers (PHP string ptr/len pair), produced by `__rt_hash`:
///   AArch64: x1 = ptr, x2 = len.
///   x86_64:  rax = ptr, rdx = len.
pub fn emit_sha1(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_sha1_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: sha1 ---");
    emitter.label_global("__rt_sha1");

    // -- set up the __rt_hash contract: algo = "sha1", data = input string, flag in x5 --
    emitter.instruction("mov x3, x1");                                          // move the data string pointer into __rt_hash's data register
    emitter.instruction("mov x4, x2");                                          // move the data string length into __rt_hash's data register
    abi::emit_symbol_address(emitter, "x1", "_sha1_algo_name");
    emitter.instruction("mov x2, #4");                                          // algorithm-name length = 4 ("sha1")
    emitter.instruction("b __rt_hash");                                         // tail-call the shared hash routine; it returns the PHP string to our caller
}

/// Emits the `__rt_sha1` runtime helper for Linux x86_64 using the SysV ABI.
///
/// Sets up the `__rt_hash` register contract for the fixed "sha1" algorithm and
/// tail-branches into it. See [`emit_sha1`] for the register contract.
fn emit_sha1_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: sha1 ---");
    emitter.label_global("__rt_sha1");

    // -- set up the __rt_hash contract: algo = "sha1", data = input string, flag in r10 --
    emitter.instruction("mov rdi, rax");                                        // move the data string pointer into __rt_hash's data register
    emitter.instruction("mov rsi, rdx");                                        // move the data string length into __rt_hash's data register
    abi::emit_symbol_address(emitter, "rax", "_sha1_algo_name");
    emitter.instruction("mov rdx, 4");                                          // algorithm-name length = 4 ("sha1")
    emitter.instruction("jmp __rt_hash");                                       // tail-call the shared hash routine; it returns the PHP string to our caller
}
