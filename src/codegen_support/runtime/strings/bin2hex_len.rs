//! Purpose:
//! Emits `__rt_bin2hex_len`, the allocation-free length computation for
//! `strlen(bin2hex(...))`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - The hexadecimal result is always twice the source byte count. Overflow and
//!   sizes above the configured heap bound branch to PHP's allocation-overflow fatal.
//! - ABI: AArch64 x1/x2 = source pointer/length; x86_64 rax/rdx = source. Result in
//!   x0/rax; the source pointer is not read.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the `__rt_bin2hex_len` helper for the active target.
pub fn emit_bin2hex_len(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_bin2hex_len_x86_64(emitter);
        return;
    }
    emit_bin2hex_len_aarch64(emitter);
}

/// Emits the AArch64 length helper.
fn emit_bin2hex_len_aarch64(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: bin2hex_len ---");
    e.label_global("__rt_bin2hex_len");
    e.instruction("adds x0, x2, x2");                                            // result length = 2 * source length
    e.instruction("b.cs __rt_bin2hex_len_overflow");                            // reject a wrapped size
    abi::emit_symbol_address(e, "x9", "_heap_max");
    e.instruction("ldr x9, [x9]");                                               // load the configured heap bound
    e.instruction("cmp x0, x9");                                                 // would bin2hex() exceed the heap?
    e.instruction("b.hi __rt_bin2hex_len_overflow");                             // report the same allocation fatal
    e.instruction("ret");                                                        // return the exact result length
    e.label("__rt_bin2hex_len_overflow");
    e.instruction("b __rt_alloc_overflow");                                      // keep the shared fatal trampoline cross-atom safe
}

/// Emits the x86_64 SysV length helper.
fn emit_bin2hex_len_x86_64(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: bin2hex_len ---");
    e.label_global("__rt_bin2hex_len");
    e.instruction("mov rax, rdx");                                               // start from the source byte count
    e.instruction("add rax, rax");                                               // result length = 2 * source length
    e.instruction("jc __rt_bin2hex_len_overflow_x86_64");                        // reject a wrapped size
    abi::emit_symbol_address(e, "r9", "_heap_max");
    e.instruction("mov r9, QWORD PTR [r9]");                                     // load the configured heap bound
    e.instruction("cmp rax, r9");                                                // would bin2hex() exceed the heap?
    e.instruction("ja __rt_bin2hex_len_overflow_x86_64");                        // report the same allocation fatal
    e.instruction("ret");                                                        // return the exact result length
    e.label("__rt_bin2hex_len_overflow_x86_64");
    e.instruction("jmp __rt_alloc_overflow");                                    // keep the shared fatal trampoline reachable
}
