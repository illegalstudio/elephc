//! Purpose:
//! Emits `__rt_defined`, the runtime helper backing a non-literal `defined($name)`
//! call by searching the closed-world constant registry.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` (gated by the
//!   `const_introspection` feature) and the EIR `defined` lowering.
//!
//! Key details:
//! - Input:  x0/rdi=name_ptr, x1/rsi=name_len.
//! - Output: x0/rax = 1 when the constant is registered, 0 otherwise. No throw.
//! - Delegates the lookup to `__rt_sorted_name_search` over the 40-byte
//!   `_const_table` entries and reports presence as a boolean.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// Emits the target-specific `__rt_defined` constant-existence helper.
pub fn emit_rt_defined(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the AArch64 implementation of `__rt_defined`.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: defined ---");
    emitter.label_global("__rt_defined");

    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the new frame pointer
    abi::emit_symbol_address(emitter, "x2", "_const_table");
    abi::emit_load_symbol_to_reg(emitter, "x3", "_const_table_count", 0);
    emitter.instruction("mov x4, #40");                                         // constant entries are 40 bytes wide
    emitter.instruction("bl __rt_sorted_name_search");                          // search for the constant name
    emitter.instruction("cmp x0, #0");                                          // did the search return a matching entry?
    emitter.instruction("cset x0, ne");                                         // map a non-null entry pointer to true
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return the boolean in x0
}

/// Emits the x86_64 implementation of `__rt_defined`.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: defined ---");
    emitter.label_global("__rt_defined");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // keep the nested call 16-byte aligned
    abi::emit_symbol_address(emitter, "rdx", "_const_table");
    abi::emit_load_symbol_to_reg(emitter, "rcx", "_const_table_count", 0);
    emitter.instruction("mov r8, 40");                                          // constant entries are 40 bytes wide
    emitter.instruction("call __rt_sorted_name_search");                        // search for the constant name
    emitter.instruction("test rax, rax");                                       // did the search return a matching entry?
    emitter.instruction("setne al");                                            // map a non-null entry pointer to true
    emitter.instruction("movzx eax, al");                                       // widen the boolean byte into the result register
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the boolean in rax
}

