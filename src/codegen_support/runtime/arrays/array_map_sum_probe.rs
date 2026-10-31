//! Purpose:
//! Emits `__rt_array_map_sum_probe_int`, the one-time checked probe used by the
//! loop-invariant fused map+sum cache.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::managed::emit_managed_runtime`.
//!
//! Key details:
//! - The probe walks a packed int array once, applies the literal map offset with PHP's
//!   checked integer addition, and accumulates with the same checked addition.
//! - It returns the exact integer result when every step stays in range, or `PHP_INT_MIN`
//!   when a map or sum step would promote to float. The caller then keeps the exact
//!   per-iteration raw kernel so warned casts and promotion are preserved.
//! - The probe never allocates or warns.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the fused map+sum probe for the active target.
pub fn emit_array_map_sum_probe_int(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
        return;
    }
    emit_aarch64(emitter);
}

/// Emits the AArch64 probe helper.
fn emit_aarch64(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: array_map_sum_probe_int ---");
    e.label_global("__rt_array_map_sum_probe_int");
    e.instruction("ldr x9, [x0]");                                              // load the packed element count
    e.instruction("cbz x9, __rt_array_map_sum_probe_zero");                     // an empty map+sum is integer zero
    e.instruction("add x10, x0, #24");                                          // skip the indexed-array header to the payload base
    e.instruction("mov x11, xzr");                                              // start the integer accumulator at zero
    e.label("__rt_array_map_sum_probe_loop");
    e.instruction("ldr x12, [x10], #8");                                        // load the next source element
    e.instruction("adds x13, x12, x1");                                         // map with PHP's checked integer addition
    e.instruction("b.vs __rt_array_map_sum_probe_no");                          // an overflowed map result must warn per call
    e.instruction("adds x11, x11, x13");                                        // sum the mapped value with the checked accumulator
    e.instruction("b.vs __rt_array_map_sum_probe_no");                          // an overflowed sum must promote per call
    e.instruction("subs x9, x9, #1");                                           // one fewer element remains
    e.instruction("b.ne __rt_array_map_sum_probe_loop");                        // continue through the packed payload
    abi::emit_load_int_immediate(e, "x14", i64::MIN);                           // the sentinel is the only unrepresentable result
    e.instruction("cmp x11, x14");                                              // does the exact sum collide with the sentinel?
    e.instruction("b.eq __rt_array_map_sum_probe_no");                          // fall back rather than cache an ambiguous value
    e.instruction("mov x0, x11");                                               // return the exact integer map+sum
    e.instruction("ret");                                                       // transfer the cached result to the caller
    e.label("__rt_array_map_sum_probe_zero");
    e.instruction("mov x0, xzr");                                               // an empty map+sum is integer zero
    e.instruction("ret");                                                       // return the cached result
    e.label("__rt_array_map_sum_probe_no");
    abi::emit_load_int_immediate(e, "x0", i64::MIN);                            // sentinel: keep the exact per-iteration kernel
    e.instruction("ret");                                                       // return the sentinel
}

/// Emits the x86_64 probe helper.
fn emit_x86_64(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: array_map_sum_probe_int ---");
    e.label_global("__rt_array_map_sum_probe_int");
    e.instruction("mov r8, QWORD PTR [rdi]");                                   // load the packed element count
    e.instruction("test r8, r8");                                               // is the range empty?
    e.instruction("jz __rt_array_map_sum_probe_zero_x86");                      // an empty map+sum is integer zero
    e.instruction("lea r9, [rdi + 24]");                                        // skip the indexed-array header to the payload base
    e.instruction("xor ecx, ecx");                                              // start the integer accumulator at zero
    e.label("__rt_array_map_sum_probe_loop_x86");
    e.instruction("mov r10, QWORD PTR [r9]");                                   // load the next source element
    e.instruction("add r9, 8");                                                 // advance the element cursor
    e.instruction("mov rax, r10");                                              // copy the source element before mapping it
    e.instruction("add rax, rsi");                                              // map with PHP's checked integer addition
    e.instruction("jo __rt_array_map_sum_probe_no_x86");                        // an overflowed map result must warn per call
    e.instruction("add rcx, rax");                                              // sum the mapped value with the checked accumulator
    e.instruction("jo __rt_array_map_sum_probe_no_x86");                        // an overflowed sum must promote per call
    e.instruction("dec r8");                                                    // one fewer element remains
    e.instruction("jnz __rt_array_map_sum_probe_loop_x86");                     // continue through the packed payload
    e.instruction("mov rax, 0x8000000000000000");                               // the sentinel is the only unrepresentable result
    e.instruction("cmp rcx, rax");                                              // does the exact sum collide with the sentinel?
    e.instruction("je __rt_array_map_sum_probe_no_x86");                        // fall back rather than cache an ambiguous value
    e.instruction("mov rax, rcx");                                              // return the exact integer map+sum
    e.instruction("ret");                                                       // transfer the cached result to the caller
    e.label("__rt_array_map_sum_probe_zero_x86");
    e.instruction("xor eax, eax");                                              // an empty map+sum is integer zero
    e.instruction("ret");                                                       // return the cached result
    e.label("__rt_array_map_sum_probe_no_x86");
    e.instruction("mov rax, 0x8000000000000000");                               // sentinel: keep the exact per-iteration kernel
    e.instruction("ret");                                                       // return the sentinel
}
