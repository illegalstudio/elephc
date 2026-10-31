//! Purpose:
//! Emits the typed int-array reduction helpers used by the EIR sum-loop rewrite
//! and the `array_sum()` builtin fast path.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::managed::emit_managed_runtime`.
//!
//! Key details:
//! - `__rt_array_sum_int` takes a borrowed Mixed seed cell for the loop rewrite.
//! - `__rt_array_sum_int_i64` takes a raw int seed for `array_sum()` and boxes it
//!   for an empty range.
//! - Both entries reproduce PHP's left-to-right `+` sequence: the first integer
//!   overflow promotes the running sum and that element to doubles, later elements
//!   stay double, and the result is a fresh Mixed int/float cell owned by the caller.
//! - Non-negative ranges whose elements all fit in 32 unsigned bits and whose total
//!   is provably below `2^63` take a NEON/SSE2 sum. Any gate failure restarts the
//!   exact scalar sequence from element 0, so promotion behavior never changes.
//! - The two entries are self-contained so Mach-O dead stripping can remove either
//!   one without leaving a branch into removed code.

use crate::codegen_support::{abi, emit::Emitter};
use crate::codegen_support::platform::Arch;

/// Emits every `__rt_array_sum_int` reducer entry for the active target.
///
/// `__rt_array_sum_int_value` is the allocation-free entry the EIR int sink selects when the
/// boxed `array_sum()` result is observed only through an `(int)` cast: it returns the raw sum,
/// or PHP's warned float-to-int conversion when the running sum promoted to double.
pub fn emit_array_sum_int(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_sum_int_x86_64(emitter, false, false);
        emit_array_sum_int_x86_64(emitter, true, false);
        emit_array_sum_int_x86_64(emitter, true, true);
        emit_array_sum_int_cached_x86_64(emitter);
        return;
    }
    emit_array_sum_int_aarch64(emitter, false, false);
    emit_array_sum_int_aarch64(emitter, true, false);
    emit_array_sum_int_aarch64(emitter, true, true);
    emit_array_sum_int_cached_aarch64(emitter);
}

/// Emits one self-contained AArch64 reducer entry.
///
/// `raw_seed` selects the integer entry; `raw_result` selects the allocation-free value entry.
fn emit_array_sum_int_aarch64(emitter: &mut Emitter, raw_seed: bool, raw_result: bool) {
    let prefix = if raw_result {
        "__rt_array_sum_int_value"
    } else if raw_seed {
        "__rt_array_sum_int_i64"
    } else {
        "__rt_array_sum_int"
    };
    let loop_label = format!("{prefix}_int_loop");
    let empty_label = format!("{prefix}_empty");
    let promote_label = format!("{prefix}_promote");
    let float_loop = format!("{prefix}_float_loop");
    let box_float = format!("{prefix}_box_float");
    let float_init = format!("{prefix}_float_init");
    let scalar_loop = format!("{prefix}_scalar_loop");
    let vector_loop = format!("{prefix}_vector_loop");
    let vector_pair = format!("{prefix}_vector_pair");
    let vector_tail = format!("{prefix}_vector_tail");
    let vector_restart = format!("{prefix}_vector_restart");
    let vector_box = format!("{prefix}_vector_box");
    emitter.blank();
    emitter.comment("--- runtime: array_sum int reducer ---");
    emitter.label_global(prefix);
    // -- empty range: raw seeds are boxed; borrowed cells are retained --
    emitter.instruction("subs x9, x2, x1");                                     // count = end - start and flag an empty range
    emitter.instruction(&format!("b.le {empty_label}"));                        // empty or reversed ranges keep the seed value
    if raw_seed {
        if raw_result {
            emitter.instruction("mov x11, xzr");                                // the allocation-free value entry always starts from zero
        } else {
            emitter.instruction("mov x11, x3");                                 // seed the integer accumulator from the raw seed
        }
        emitter.instruction("mov x10, #0");                                     // the raw seed starts in integer mode
    } else {
        emitter.instruction("ldr x10, [x3]");                                   // load the seed Mixed tag
        emitter.instruction("ldr x11, [x3, #8]");                               // load the seed payload word
    }
    emitter.instruction("add x12, x0, #24");                                    // skip the indexed-array header to the payload base
    emitter.instruction("add x12, x12, x1, lsl #3");                            // address the first element in the requested range
    emitter.instruction("cbz x10, 1f");                                         // tag 0 starts the checked integer sequence
    emitter.instruction("cmp x10, #2");                                         // tag 2 is a float seed
    emitter.instruction(&format!("b.eq {float_init}"));                         // float seeds accumulate directly as doubles
    emitter.instruction("mov x11, xzr");                                        // the numeric proof excludes other tags; null reads as zero
    emitter.label("1");

    // -- SIMD gate: non-negative seed, at most 2^31-1 elements, and a provable 64-bit total --
    emitter.instruction("cmp x9, #8");                                          // tiny ranges keep the scalar loop and its small fixed cost
    emitter.instruction(&format!("b.lo {scalar_loop}"));                        // vector setup is not worth it below eight elements
    emitter.instruction("mov x13, #0x7fffffff");                                // largest range the 32-bit element bound can still total safely
    emitter.instruction("cmp x9, x13");                                         // does the element count fit the SIMD bound?
    emitter.instruction(&format!("b.hi {scalar_loop}"));                        // larger ranges restart with the exact scalar sequence
    emitter.instruction(&format!("tbnz x11, #63, {scalar_loop}"));              // negative seeds can promote through intermediate sums
    emitter.instruction("lsr x13, x11, #32");                                   // high dword of the non-negative seed contributes its own 2^32 units
    emitter.instruction("add x13, x13, x9");                                    // bound the total as (seed>>32 + count) * 2^32
    emitter.instruction("mov x14, #0x7fffffff");                                // the total must stay below 2^63
    emitter.instruction("cmp x13, x14");                                        // does the combined headroom keep every partial sum in range?
    emitter.instruction(&format!("b.hi {scalar_loop}"));                        // otherwise restart with the exact scalar sequence
    emitter.instruction("movi v0.2d, #0");                                      // zero the two-lane 64-bit accumulator
    emitter.instruction("movi v1.2d, #0");                                      // zero the OR-accumulated high-dword guard

    emitter.label(&vector_loop);
    emitter.instruction("cmp x9, #4");                                          // can two full vectors be consumed together?
    emitter.instruction(&format!("b.lo {vector_pair}"));                        // fall back to a single vector for two or three elements
    emitter.instruction("ldp q2, q3, [x12], #32");                              // load four consecutive int64 elements
    emitter.instruction("ushr v4.2d, v2.2d, #32");                              // move each high dword of the first pair into its low lane
    emitter.instruction("ushr v5.2d, v3.2d, #32");                              // move each high dword of the second pair into its low lane
    emitter.instruction("orr v1.16b, v1.16b, v4.16b");                          // record any element at or above 2^32 in the guard
    emitter.instruction("orr v1.16b, v1.16b, v5.16b");                          // record the second pair's high dwords too
    emitter.instruction("add v0.2d, v0.2d, v2.2d");                             // accumulate the first pair in 64-bit lanes
    emitter.instruction("add v0.2d, v0.2d, v3.2d");                             // accumulate the second pair in 64-bit lanes
    emitter.instruction("sub x9, x9, #4");                                      // account for the four consumed elements
    emitter.instruction(&format!("b {vector_loop}"));                           // continue consuming four elements at a time

    emitter.label(&vector_pair);
    emitter.instruction("cmp x9, #2");                                          // is one full 64-bit pair still available?
    emitter.instruction(&format!("b.lo {vector_tail}"));                        // an odd tail element stays scalar
    emitter.instruction("ldr q2, [x12], #16");                                  // load the final complete element pair
    emitter.instruction("ushr v4.2d, v2.2d, #32");                              // isolate the pair's high dwords
    emitter.instruction("orr v1.16b, v1.16b, v4.16b");                          // record elements at or above 2^32 in the guard
    emitter.instruction("add v0.2d, v0.2d, v2.2d");                             // accumulate the final complete pair
    emitter.instruction("sub x9, x9, #2");                                      // account for the two consumed elements

    emitter.label(&vector_tail);
    emitter.instruction("fmov x13, d1");                                        // move the guard's low 64-bit lane to an integer register
    emitter.instruction("mov x14, v1.d[1]");                                    // move the guard's high 64-bit lane to an integer register
    emitter.instruction("orr x13, x13, x14");                                   // combine both halves of the high-dword guard
    emitter.instruction(&format!("cbnz x13, {vector_restart}"));                // any large element restarts the exact scalar sequence
    emitter.instruction("fmov x13, d0");                                        // move the accumulator's low 64-bit lane to an integer register
    emitter.instruction("mov x14, v0.d[1]");                                    // move the accumulator's high 64-bit lane to an integer register
    emitter.instruction("add x11, x11, x13");                                   // fold the low accumulated lane into the seed
    emitter.instruction("add x11, x11, x14");                                   // fold the high accumulated lane into the sum
    emitter.instruction(&format!("cbz x9, {vector_box}"));                      // stop when the range had an even element count
    emitter.instruction("ldr x13, [x12]");                                      // load the single remaining tail element
    emitter.instruction("add x11, x11, x13");                                   // the gate proves this addition stays in range

    emitter.label(&vector_box);
    if raw_result {
        emitter.instruction("mov x0, x11");                                     // return the raw vector sum without a Mixed cell
        emitter.instruction("ret");                                             // transfer the integer result to the caller
    } else {
        emitter.instruction("mov x0, #0");                                      // int Mixed tag for the vector sum
        emitter.instruction("mov x1, x11");                                     // int payload for the vector sum
        emitter.instruction("mov x2, xzr");                                     // the high payload word is unused for scalars
        emitter.instruction("b __rt_mixed_from_value");                         // box the integer sum and return
    }

    emitter.label(&vector_restart);
    emitter.instruction("add x12, x0, #24");                                    // rebuild the element cursor from the original array base
    emitter.instruction("add x12, x12, x1, lsl #3");                            // address the first element in the requested range
    emitter.instruction("sub x9, x2, x1");                                      // rebuild the exact element count after the failed gate

    emitter.label(&scalar_loop);
    emitter.label(&loop_label);
    emitter.instruction("ldr x13, [x12], #8");                                  // load the next int element and advance the cursor
    emitter.instruction("adds x14, x11, x13");                                  // perform the checked PHP integer addition
    emitter.instruction(&format!("b.vs {promote_label}"));                      // overflow switches the running sum to a double
    emitter.instruction("mov x11, x14");                                        // commit the in-range integer sum
    emitter.instruction("subs x9, x9, #1");                                     // one fewer element remains
    emitter.instruction(&format!("b.ne {loop_label}"));                         // continue while the range is non-empty
    if raw_result {
        emitter.instruction("mov x0, x11");                                     // return the raw integer sum without a Mixed cell
        emitter.instruction("ret");                                             // transfer the integer result to the caller
    } else {
        emitter.instruction("mov x0, #0");                                      // int Mixed tag for the result
        emitter.instruction("mov x1, x11");                                     // int payload for the result
        emitter.instruction("mov x2, xzr");                                     // the high payload word is unused for scalars
        emitter.instruction("b __rt_mixed_from_value");                         // box the integer sum and return
    }
    // -- overflow promotion matches PHP's int-to-float conversion for this step --
    emitter.label(&promote_label);
    emitter.instruction("scvtf d0, x11");                                       // convert the pre-overflow sum to double
    emitter.instruction("scvtf d1, x13");                                       // convert the overflowing element to double
    emitter.instruction("fadd d0, d0, d1");                                     // add them as PHP does after the promotion
    emitter.instruction("subs x9, x9, #1");                                     // the promoting element is consumed
    emitter.instruction(&format!("b.eq {box_float}"));                          // stop when no elements remain
    emitter.label(&float_loop);
    emitter.instruction("ldr x13, [x12], #8");                                  // load the next int element and advance the cursor
    emitter.instruction("scvtf d1, x13");                                       // convert the element to double
    emitter.instruction("fadd d0, d0, d1");                                     // accumulate in PHP double mode
    emitter.instruction("subs x9, x9, #1");                                     // one fewer element remains
    emitter.instruction(&format!("b.ne {float_loop}"));                         // continue while the range is non-empty
    emitter.label(&box_float);
    if raw_result {
        emitter.instruction("b __rt_float_key_to_int");                         // apply PHP's warned float-to-int conversion for the cast
    } else {
        emitter.instruction("fmov x1, d0");                                     // float payload for the result
        emitter.instruction("mov x0, #2");                                      // float Mixed tag for the result
        emitter.instruction("mov x2, xzr");                                     // the high payload word is unused for scalars
        emitter.instruction("b __rt_mixed_from_value");                         // box the float sum and return
    }
    // -- float seed: start the double accumulation from its payload bits --
    emitter.label(&float_init);
    emitter.instruction("fmov d0, x11");                                        // seed the double accumulator from the float payload
    emitter.instruction(&format!("b {float_loop}"));                            // reuse the double accumulation loop
    // -- empty range --
    emitter.label(&empty_label);
    if raw_result {
        emitter.instruction("mov x0, xzr");                                     // an empty range sums to the integer zero
        emitter.instruction("ret");                                             // return the raw integer sum
    } else if raw_seed {
        emitter.instruction("mov x0, #0");                                      // int Mixed tag for the empty sum
        emitter.instruction("mov x1, x3");                                      // the raw seed becomes the integer payload
        emitter.instruction("mov x2, xzr");                                     // the high payload word is unused for scalars
        emitter.instruction("b __rt_mixed_from_value");                         // box the raw seed and return
    } else {
        emitter.instruction("mov x0, x3");                                      // return the seed cell itself
        emitter.instruction("b __rt_incref");                                   // add the reference consumed by the replacing store
    }
}

/// Emits one self-contained x86_64 reducer entry; `raw_seed` selects the integer entry.
fn emit_array_sum_int_x86_64(emitter: &mut Emitter, raw_seed: bool, raw_result: bool) {
    let prefix = if raw_result {
        "__rt_array_sum_int_value"
    } else if raw_seed {
        "__rt_array_sum_int_i64"
    } else {
        "__rt_array_sum_int"
    };
    let loop_label = format!("{prefix}_int_loop");
    let empty_label = format!("{prefix}_empty");
    let promote_label = format!("{prefix}_promote");
    let float_loop = format!("{prefix}_float_loop");
    let box_float = format!("{prefix}_box_float");
    let float_init = format!("{prefix}_float_init");
    let int_gate = format!("{prefix}_int_gate");
    let scalar_loop = format!("{prefix}_scalar_loop");
    let vector_loop = format!("{prefix}_vector_loop");
    let vector_pair = format!("{prefix}_vector_pair");
    let vector_tail = format!("{prefix}_vector_tail");
    let vector_restart = format!("{prefix}_vector_restart");
    let vector_box = format!("{prefix}_vector_box");
    emitter.blank();
    emitter.comment("--- runtime: array_sum int reducer ---");
    emitter.label_global(prefix);
    // -- empty range: raw seeds are boxed; borrowed cells are retained --
    emitter.instruction("mov r10, rdx");                                        // copy the exclusive end before subtracting the start
    emitter.instruction("sub r10, rsi");                                        // count = end - start
    emitter.instruction(&format!("jle {empty_label}"));                         // empty or reversed ranges keep the seed value
    if raw_seed {
        if raw_result {
            emitter.instruction("xor r8d, r8d");                                // the allocation-free value entry always starts from zero
        } else {
            emitter.instruction("mov r8, rcx");                                 // seed the integer accumulator from the raw seed
        }
        emitter.instruction("xor r11d, r11d");                                  // the raw seed starts in integer mode
    } else {
        emitter.instruction("mov r11, QWORD PTR [rcx]");                        // load the seed Mixed tag
        emitter.instruction("mov r8, QWORD PTR [rcx + 8]");                     // load the seed payload word
    }
    emitter.instruction("lea r9, [rdi + 24]");                                  // skip the indexed-array header to the payload base
    emitter.instruction("lea r9, [r9 + rsi * 8]");                              // address the first element in the requested range
    emitter.instruction("test r11, r11");                                       // tag 0 starts the checked integer sequence
    emitter.instruction(&format!("jz {int_gate}"));                             // int seeds enter the SIMD gate for the integer sequence
    emitter.instruction("cmp r11, 2");                                          // tag 2 is a float seed
    emitter.instruction(&format!("je {float_init}"));                           // float seeds accumulate directly as doubles
    emitter.instruction("xor r8d, r8d");                                        // the numeric proof excludes other tags; null reads as zero

    // -- SIMD gate: non-negative seed, at most 2^31-1 elements, and a provable 64-bit total --
    emitter.label(&int_gate);
    emitter.instruction("cmp r10, 16");                                         // tiny ranges keep the scalar loop and its small fixed cost
    emitter.instruction(&format!("jl {scalar_loop}"));                          // vector setup is not worth it below sixteen elements
    emitter.instruction("mov r11d, 0x7fffffff");                                // largest range the 32-bit element bound can still total safely
    emitter.instruction("cmp r10, r11");                                        // does the element count fit the SIMD bound?
    emitter.instruction(&format!("ja {scalar_loop}"));                          // larger ranges restart with the exact scalar sequence
    emitter.instruction("test r8, r8");                                         // negative seeds can promote through intermediate sums
    emitter.instruction(&format!("js {scalar_loop}"));                          // restart with the exact scalar sequence for negative seeds
    emitter.instruction("mov r11, r8");                                         // copy the non-negative seed before isolating its high dword
    emitter.instruction("shr r11, 32");                                         // high dword of the seed contributes its own 2^32 units
    emitter.instruction("add r11, r10");                                        // bound the total as (seed>>32 + count) * 2^32
    emitter.instruction("mov eax, 0x7fffffff");                                 // the total must stay below 2^63
    emitter.instruction("cmp r11, rax");                                        // does the combined headroom keep every partial sum in range?
    emitter.instruction(&format!("ja {scalar_loop}"));                          // otherwise restart with the exact scalar sequence
    emitter.instruction("pxor xmm0, xmm0");                                     // zero the two-lane 64-bit accumulator
    emitter.instruction("pxor xmm1, xmm1");                                     // zero the OR-accumulated high-dword guard

    emitter.label(&vector_loop);
    emitter.instruction("cmp r10, 4");                                          // can two full vectors be consumed together?
    emitter.instruction(&format!("jl {vector_pair}"));                          // fall back to a single vector for two or three elements
    emitter.instruction("movdqu xmm2, XMMWORD PTR [r9]");                       // load the first two consecutive int64 elements
    emitter.instruction("movdqu xmm3, XMMWORD PTR [r9 + 16]");                  // load the next two consecutive int64 elements
    emitter.instruction("add r9, 32");                                          // advance past the four consumed elements
    emitter.instruction("movdqa xmm4, xmm2");                                   // copy the first pair before isolating high dwords
    emitter.instruction("psrlq xmm4, 32");                                      // move each high dword of the first pair into its low lane
    emitter.instruction("movdqa xmm5, xmm3");                                   // copy the second pair before isolating high dwords
    emitter.instruction("psrlq xmm5, 32");                                      // move each high dword of the second pair into its low lane
    emitter.instruction("por xmm1, xmm4");                                      // record any element at or above 2^32 in the guard
    emitter.instruction("por xmm1, xmm5");                                      // record the second pair's high dwords too
    emitter.instruction("paddq xmm0, xmm2");                                    // accumulate the first pair in 64-bit lanes
    emitter.instruction("paddq xmm0, xmm3");                                    // accumulate the second pair in 64-bit lanes
    emitter.instruction("sub r10, 4");                                          // account for the four consumed elements
    emitter.instruction(&format!("jmp {vector_loop}"));                         // continue consuming four elements at a time

    emitter.label(&vector_pair);
    emitter.instruction("cmp r10, 2");                                          // is one full 64-bit pair still available?
    emitter.instruction(&format!("jl {vector_tail}"));                          // an odd tail element stays scalar
    emitter.instruction("movdqu xmm2, XMMWORD PTR [r9]");                       // load the final complete element pair
    emitter.instruction("add r9, 16");                                          // advance past the two consumed elements
    emitter.instruction("movdqa xmm4, xmm2");                                   // copy the pair before isolating high dwords
    emitter.instruction("psrlq xmm4, 32");                                      // isolate the pair's high dwords
    emitter.instruction("por xmm1, xmm4");                                      // record elements at or above 2^32 in the guard
    emitter.instruction("paddq xmm0, xmm2");                                    // accumulate the final complete pair
    emitter.instruction("sub r10, 2");                                          // account for the two consumed elements

    emitter.label(&vector_tail);
    emitter.instruction("movq rax, xmm1");                                      // move the guard's low 64-bit lane to an integer register
    emitter.instruction("psrldq xmm2, 8");                                      // move the guard's high 64-bit lane into the low lane
    emitter.instruction("movq r11, xmm2");                                      // copy the high guard lane into an integer register
    emitter.instruction("or rax, r11");                                         // combine both halves of the high-dword guard
    emitter.instruction(&format!("jnz {vector_restart}"));                      // any large element restarts the exact scalar sequence
    emitter.instruction("movq rax, xmm0");                                      // move the accumulator's low 64-bit lane to an integer register
    emitter.instruction("psrldq xmm2, 8");                                      // move the accumulator's high 64-bit lane into the low lane
    emitter.instruction("movq r11, xmm2");                                      // copy the high accumulator lane into an integer register
    emitter.instruction("add r8, rax");                                         // fold the low accumulated lane into the seed
    emitter.instruction("add r8, r11");                                         // fold the high accumulated lane into the sum
    emitter.instruction("test r10, r10");                                       // stop when the range had an even element count
    emitter.instruction(&format!("jz {vector_box}"));                           // box the completed vector sum
    emitter.instruction("mov rax, QWORD PTR [r9]");                             // load the single remaining tail element
    emitter.instruction("add r8, rax");                                         // the gate proves this addition stays in range

    emitter.label(&vector_box);
    if raw_result {
        emitter.instruction("mov rax, r8");                                     // return the raw vector sum without a Mixed cell
        emitter.instruction("ret");                                             // transfer the integer result to the caller
    } else {
        emitter.instruction("xor eax, eax");                                    // int Mixed tag for the vector sum
        emitter.instruction("mov rdi, r8");                                     // int payload for the vector sum
        emitter.instruction("xor esi, esi");                                    // the high payload word is unused for scalars
        emitter.instruction("jmp __rt_mixed_from_value");                       // box the integer sum and return
    }

    emitter.label(&vector_restart);
    emitter.instruction("lea r9, [rdi + 24]");                                  // rebuild the element cursor from the original array base
    emitter.instruction("lea r9, [r9 + rsi * 8]");                              // address the first element in the requested range
    emitter.instruction("mov r10, rdx");                                        // rebuild the exclusive end before subtracting the start
    emitter.instruction("sub r10, rsi");                                        // rebuild the exact element count after the failed gate

    emitter.label(&scalar_loop);
    emitter.label(&loop_label);
    emitter.instruction("mov rax, QWORD PTR [r9]");                             // load the next int element
    emitter.instruction("add r9, 8");                                           // advance the element cursor
    emitter.instruction("mov r11, r8");                                         // preserve the pre-overflow sum for the float promotion
    emitter.instruction("add r8, rax");                                         // perform the checked PHP integer addition
    emitter.instruction(&format!("jo {promote_label}"));                        // overflow switches the running sum to a double
    emitter.instruction("dec r10");                                             // one fewer element remains
    emitter.instruction(&format!("jnz {loop_label}"));                          // continue while the range is non-empty
    if raw_result {
        emitter.instruction("mov rax, r8");                                     // return the raw integer sum without a Mixed cell
        emitter.instruction("ret");                                             // transfer the integer result to the caller
    } else {
        emitter.instruction("xor eax, eax");                                    // int Mixed tag for the result
        emitter.instruction("mov rdi, r8");                                     // int payload for the result
        emitter.instruction("xor esi, esi");                                    // the high payload word is unused for scalars
        emitter.instruction("jmp __rt_mixed_from_value");                       // box the integer sum and return
    }
    // -- overflow promotion matches PHP's int-to-float conversion for this step --
    emitter.label(&promote_label);
    emitter.instruction("cvtsi2sd xmm0, r11");                                  // convert the pre-overflow sum to double
    emitter.instruction("cvtsi2sd xmm1, rax");                                  // convert the overflowing element to double
    emitter.instruction("addsd xmm0, xmm1");                                    // add them as PHP does after the promotion
    emitter.instruction("dec r10");                                             // the promoting element is consumed
    emitter.instruction(&format!("jz {box_float}"));                            // stop when no elements remain
    emitter.label(&float_loop);
    emitter.instruction("mov rax, QWORD PTR [r9]");                             // load the next int element
    emitter.instruction("add r9, 8");                                           // advance the element cursor
    emitter.instruction("cvtsi2sd xmm1, rax");                                  // convert the element to double
    emitter.instruction("addsd xmm0, xmm1");                                    // accumulate in PHP double mode
    emitter.instruction("dec r10");                                             // one fewer element remains
    emitter.instruction(&format!("jnz {float_loop}"));                          // continue while the range is non-empty
    emitter.label(&box_float);
    if raw_result {
        emitter.instruction("jmp __rt_float_key_to_int");                       // apply PHP's warned float-to-int conversion for the cast
    } else {
        emitter.instruction("movq rdi, xmm0");                                  // float payload for the result
        emitter.instruction("mov eax, 2");                                      // float Mixed tag for the result
        emitter.instruction("xor esi, esi");                                    // the high payload word is unused for scalars
        emitter.instruction("jmp __rt_mixed_from_value");                       // box the float sum and return
    }
    // -- float seed: start the double accumulation from its payload bits --
    emitter.label(&float_init);
    emitter.instruction("movq xmm0, r8");                                       // seed the double accumulator from the float payload
    emitter.instruction(&format!("jmp {float_loop}"));                          // reuse the double accumulation loop
    // -- empty range --
    emitter.label(&empty_label);
    if raw_result {
        emitter.instruction("xor eax, eax");                                    // an empty range sums to the integer zero
        emitter.instruction("ret");                                             // return the raw integer sum
    } else if raw_seed {
        emitter.instruction("xor eax, eax");                                    // int Mixed tag for the empty sum
        emitter.instruction("mov rdi, rcx");                                    // the raw seed becomes the integer payload
        emitter.instruction("xor esi, esi");                                    // the high payload word is unused for scalars
        emitter.instruction("jmp __rt_mixed_from_value");                       // box the raw seed and return
    } else {
        emitter.instruction("mov rax, rcx");                                    // return the seed cell itself
        emitter.instruction("jmp __rt_incref");                                 // add the reference consumed by the replacing store
    }
}


/// Emits the cached dispatch between the small and general raw value entries.
fn emit_array_sum_int_cached_aarch64(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: array_sum int cached value ---");
    e.label_global("__rt_array_sum_int_value_cached");
    abi::emit_load_int_immediate(e, "x2", i64::MIN);                            // sentinel for an invalid preheader probe
    e.instruction("cmp x1, x2");                                                // did the probe produce an exact small sum?
    e.instruction("b.eq __rt_array_sum_int_value_cached_general");              // otherwise use the exact guarded reducer
    e.instruction("mov x0, x1");                                                // return the cached exact sum
    e.instruction("ret");                                                       // transfer the cached result to the caller
    e.label("__rt_array_sum_int_value_cached_general");
    e.instruction("b __rt_array_sum_int_value");                                // fall back to the exact guarded reducer
}

/// Emits the x86_64 cached dispatch between the small and general raw value entries.
fn emit_array_sum_int_cached_x86_64(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: array_sum int cached value ---");
    e.label_global("__rt_array_sum_int_value_cached");
    e.instruction("mov rax, 0x8000000000000000");                               // sentinel for an invalid preheader probe
    e.instruction("cmp rsi, rax");                                              // did the probe produce an exact small sum?
    e.instruction("je __rt_array_sum_int_value_cached_general_x86");            // otherwise use the exact guarded reducer
    e.instruction("mov rax, rsi");                                              // return the cached exact sum
    e.instruction("ret");                                                       // transfer the cached result to the caller
    e.label("__rt_array_sum_int_value_cached_general_x86");
    e.instruction("jmp __rt_array_sum_int_value");                              // fall back to the exact guarded reducer
}
