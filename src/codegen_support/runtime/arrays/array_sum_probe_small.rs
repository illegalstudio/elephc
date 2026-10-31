//! Purpose:
//! Emits `__rt_array_sum_probe_small`, the one-time sum probe used by the loop-invariant
//! array-sum cache.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::managed::emit_managed_runtime`.
//!
//! Key details:
//! - The probe scans a packed int array once and returns its exact sum when every element
//!   is non-negative and below 2^32 and the count fits the 64-bit total bound.
//! - Any wide, negative or oversized input returns `PHP_INT_MIN`, which cannot be a valid
//!   non-negative probe sum; the caller then keeps the exact per-iteration reducer.
//! - The probe never allocates, warns or enters PHP code.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the small-array sum probe for the active target.
pub fn emit_array_sum_probe_small(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
        return;
    }
    emit_aarch64(emitter);
}

/// Emits the AArch64 probe helper.
fn emit_aarch64(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: array_sum_probe_small ---");
    e.label_global("__rt_array_sum_probe_small");
    e.instruction("ldr x9, [x0]");                                              // load the packed element count
    e.instruction("mov x10, #0x7fffffff");                                      // largest count whose total fits below 2^63
    e.instruction("cmp x9, x10");                                               // does the count fit the bound?
    e.instruction("b.hi __rt_array_sum_probe_small_no");                        // larger arrays keep the exact reducer
    e.instruction("add x10, x0, #24");                                          // skip the indexed-array header to the payload base
    e.instruction("movi v0.2d, #0");                                            // zero the two-lane 64-bit sum
    e.instruction("movi v3.2d, #0");                                            // zero the high-dword guard
    e.instruction("mov x13, xzr");                                              // the odd tail contributes zero by default
    e.label("__rt_array_sum_probe_small_loop");
    e.instruction("cmp x9, #2");                                                // can a full 64-bit pair be consumed?
    e.instruction("b.lo __rt_array_sum_probe_small_tail");                      // an odd tail element stays scalar
    e.instruction("ldr q2, [x10], #16");                                        // load the next two int64 elements
    e.instruction("ushr v4.2d, v2.2d, #32");                                    // isolate both high dwords
    e.instruction("orr v3.16b, v3.16b, v4.16b");                                // record any wide or negative element
    e.instruction("add v0.2d, v0.2d, v2.2d");                                   // accumulate the pair in 64-bit lanes
    e.instruction("sub x9, x9, #2");                                            // account for the two consumed elements
    e.instruction("b __rt_array_sum_probe_small_loop");                         // continue through the packed payload
    e.label("__rt_array_sum_probe_small_tail");
    e.instruction("cbz x9, __rt_array_sum_probe_small_check");                  // an even count needs no tail check
    e.instruction("ldr x13, [x10]");                                            // load the single remaining element
    e.instruction("lsr x11, x13, #32");                                         // isolate its high dword
    e.instruction("cbnz x11, __rt_array_sum_probe_small_no");                   // a wide or negative element keeps the exact reducer
    e.label("__rt_array_sum_probe_small_check");
    e.instruction("fmov x11, d3");                                              // move the guard's low lane to an integer register
    e.instruction("mov x12, v3.d[1]");                                          // move the guard's high lane to an integer register
    e.instruction("orr x11, x11, x12");                                         // combine both guard lanes
    e.instruction("cbnz x11, __rt_array_sum_probe_small_no");                   // any wide or negative element keeps the exact reducer
    e.instruction("fmov x11, d0");                                              // move the sum's low lane to an integer register
    e.instruction("mov x12, v0.d[1]");                                          // move the sum's high lane to an integer register
    e.instruction("add x0, x11, x12");                                          // fold both accumulated lanes into the sum
    e.instruction("add x0, x0, x13");                                           // add the odd tail element
    e.instruction("ret");                                                       // return the exact small sum
    e.label("__rt_array_sum_probe_small_no");
    abi::emit_load_int_immediate(e, "x0", i64::MIN);                            // sentinel: keep the exact per-iteration reducer
    e.instruction("ret");                                                       // return the sentinel
}

/// Emits the x86_64 probe helper.
fn emit_x86_64(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: array_sum_probe_small ---");
    e.label_global("__rt_array_sum_probe_small");
    e.instruction("mov r8, QWORD PTR [rdi]");                                   // load the packed element count
    e.instruction("mov r9, 0x7fffffff");                                        // largest count whose total fits below 2^63
    e.instruction("cmp r8, r9");                                                // does the count fit the bound?
    e.instruction("ja __rt_array_sum_probe_small_no_x86");                      // larger arrays keep the exact reducer
    e.instruction("lea r9, [rdi + 24]");                                        // skip the indexed-array header to the payload base
    e.instruction("pxor xmm0, xmm0");                                           // zero the two-lane 64-bit sum
    e.instruction("pxor xmm3, xmm3");                                           // zero the high-dword guard
    e.instruction("xor edx, edx");                                              // the odd tail contributes zero by default
    e.label("__rt_array_sum_probe_small_loop_x86");
    e.instruction("cmp r8, 2");                                                 // can a full 64-bit pair be consumed?
    e.instruction("jb __rt_array_sum_probe_small_tail_x86");                    // an odd tail element stays scalar
    e.instruction("movdqu xmm1, XMMWORD PTR [r9]");                             // load the next two int64 elements
    e.instruction("add r9, 16");                                                // advance past the two consumed elements
    e.instruction("movdqa xmm2, xmm1");                                         // copy the pair before isolating high dwords
    e.instruction("psrlq xmm2, 32");                                            // isolate both high dwords
    e.instruction("por xmm3, xmm2");                                            // record any wide or negative element
    e.instruction("paddq xmm0, xmm1");                                          // accumulate the pair in 64-bit lanes
    e.instruction("sub r8, 2");                                                 // account for the two consumed elements
    e.instruction("jmp __rt_array_sum_probe_small_loop_x86");                   // continue through the packed payload
    e.label("__rt_array_sum_probe_small_tail_x86");
    e.instruction("test r8, r8");                                               // is there a single remaining element?
    e.instruction("jz __rt_array_sum_probe_small_check_x86");                   // an even count needs no tail check
    e.instruction("mov rdx, QWORD PTR [r9]");                                   // load the single remaining element
    e.instruction("mov rax, rdx");                                              // copy it before isolating the high dword
    e.instruction("shr rax, 32");                                               // isolate its high dword
    e.instruction("jnz __rt_array_sum_probe_small_no_x86");                     // a wide or negative element keeps the exact reducer
    e.label("__rt_array_sum_probe_small_check_x86");
    e.instruction("movq rax, xmm3");                                            // move the guard's low lane to an integer register
    e.instruction("movdqa xmm1, xmm3");                                         // copy the guard before extracting its high lane
    e.instruction("psrldq xmm1, 8");                                            // move the guard's high lane into the low lane
    e.instruction("movq rcx, xmm1");                                            // copy the high guard lane into an integer register
    e.instruction("or rax, rcx");                                               // combine both guard lanes
    e.instruction("jnz __rt_array_sum_probe_small_no_x86");                     // any wide or negative element keeps the exact reducer
    e.instruction("movq rax, xmm0");                                            // move the sum's low lane to an integer register
    e.instruction("movdqa xmm1, xmm0");                                         // copy the sum before extracting its high lane
    e.instruction("psrldq xmm1, 8");                                            // move the sum's high lane into the low lane
    e.instruction("movq rcx, xmm1");                                            // copy the high sum lane into an integer register
    e.instruction("add rax, rcx");                                              // fold both accumulated lanes into the sum
    e.instruction("add rax, rdx");                                              // add the odd tail element
    e.instruction("ret");                                                       // return the exact small sum
    e.label("__rt_array_sum_probe_small_no_x86");
    e.instruction("mov rax, 0x8000000000000000");                               // sentinel: keep the exact per-iteration reducer
    e.instruction("ret");                                                       // return the sentinel
}
