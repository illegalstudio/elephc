//! Purpose:
//! Emits fused scalar kernels for `array_sum(array_map(fn($x) => $x + C, $a))` and
//! `count(array_filter($a, fn($x) => $x % C === 0))` over packed int arrays.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::managed::emit_managed_runtime`.
//!
//! Key details:
//! - `__rt_array_map_sum_add` takes the array pointer and the literal offset, then
//!   boxes the exact PHP map-then-sum result (int until the first int-to-float
//!   promotion) as a fresh Mixed cell.
//! - `__rt_array_filter_count_mod` takes the array pointer and the literal divisor
//!   and returns a raw PHP int count; powers of two use a mask instead of division.
//! - Both walk the packed payload at `array + 24` and read the element count at
//!   `array + 0`, matching every other indexed-array runtime helper.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits both fused scalar kernels for the active target.
pub fn emit_keyed_kernels(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_map_sum_add_x86_64(emitter);
        emit_map_sum_add_safe_x86_64(emitter);
        emit_filter_count_mod_x86_64(emitter);
        return;
    }
    emit_map_sum_add_aarch64(emitter);
    emit_map_sum_add_safe_aarch64(emitter);
    emit_filter_count_mod_aarch64(emitter);
}

/// Emits `__rt_array_map_sum_add` for AArch64.
fn emit_map_sum_add_aarch64(emitter: &mut Emitter) {
    let loop_label = "__rt_array_map_sum_add_loop";
    let mapped_overflow = "__rt_array_map_sum_add_mapped_overflow";
    let acc_overflow = "__rt_array_map_sum_add_acc_overflow";
    let float_acc = "__rt_array_map_sum_add_float_acc";
    let add_float = "__rt_array_map_sum_add_add_float";
    let done = "__rt_array_map_sum_add_done";
    let box_float = "__rt_array_map_sum_add_box_float";
    let vector_loop = "__rt_array_map_sum_add_vector_loop";
    let vector_pair = "__rt_array_map_sum_add_vector_pair";
    let vector_tail = "__rt_array_map_sum_add_vector_tail";
    let vector_box = "__rt_array_map_sum_add_vector_box";
    let vector_raw = "__rt_array_map_sum_add_vector_raw";
    let vector_restart = "__rt_array_map_sum_add_vector_restart";
    let done_raw = "__rt_array_map_sum_add_done_raw";
    let float_raw = "__rt_array_map_sum_add_float_raw";

    emitter.blank();
    emitter.comment("--- runtime: array_map_sum_add (fused int kernel) ---");
    emitter.label_global("__rt_array_map_sum_add_int");
    emitter.instruction("mov x16, #1");                                         // raw result mode: skip the Mixed box
    emitter.instruction("b __rt_array_map_sum_add_body");                       // share the kernel body with the boxed entry
    emitter.label_global("__rt_array_map_sum_add");
    emitter.instruction("mov x16, #0");                                         // boxed result mode
    emitter.instruction("b __rt_array_map_sum_add_body");                       // share the kernel body with the raw entry
    emitter.label_global("__rt_array_map_sum_add_body");
    emitter.instruction("ldr x9, [x0]");                                        // load the packed element count
    emitter.instruction("add x10, x0, #24");                                    // skip the indexed-array header to the payload base
    emitter.instruction("mov x11, xzr");                                        // start the integer accumulator at zero
    emitter.instruction("mov x12, xzr");                                        // integer mode: promote to double on the first overflow

    // -- SIMD gate: non-negative bounded offset, a bounded count, and 32-bit elements --
    emitter.instruction("cmp x9, #8");                                          // tiny arrays keep the scalar loop and its small fixed cost
    emitter.instruction(&format!("b.lo {loop_label}"));                         // below eight elements the scalar loop wins
    emitter.instruction(&format!("tbnz x1, #63, {loop_label}"));                // negative offsets can promote individual map results
    emitter.instruction("lsr x13, x1, #31");                                    // move the offset's bit 31 into the low bit
    emitter.instruction(&format!("cbnz x13, {loop_label}"));                    // offsets at or above 2^31 need the scalar promotion sequence
    emitter.instruction("mov x13, #0x3fffffff");                                // bound the count so count * (2^32 + 2^31) stays below 2^63
    emitter.instruction("cmp x9, x13");                                         // does the element count fit the vector total bound?
    emitter.instruction(&format!("b.hi {loop_label}"));                         // larger ranges restart with the exact scalar sequence
    emitter.instruction("movi v0.2d, #0");                                      // zero the two-lane 64-bit accumulator
    emitter.instruction("movi v1.2d, #0");                                      // zero the OR-accumulated high-dword guard
    emitter.instruction("dup v2.2d, x1");                                       // broadcast the literal map offset into both lanes

    emitter.label(vector_loop);
    emitter.instruction("cmp x9, #4");                                          // can two full vectors be consumed together?
    emitter.instruction(&format!("b.lo {vector_pair}"));                        // fall back to a single vector for two or three elements
    emitter.instruction("ldp q3, q4, [x10], #32");                              // load four consecutive source elements
    emitter.instruction("ushr v5.2d, v3.2d, #32");                              // isolate the first pair's high dwords
    emitter.instruction("ushr v6.2d, v4.2d, #32");                              // isolate the second pair's high dwords
    emitter.instruction("orr v1.16b, v1.16b, v5.16b");                          // record any element at or above 2^32 in the guard
    emitter.instruction("orr v1.16b, v1.16b, v6.16b");                          // record the second pair's high dwords too
    emitter.instruction("add v3.2d, v3.2d, v2.2d");                             // map the first pair with the literal offset
    emitter.instruction("add v4.2d, v4.2d, v2.2d");                             // map the second pair with the literal offset
    emitter.instruction("add v0.2d, v0.2d, v3.2d");                             // accumulate the first mapped pair
    emitter.instruction("add v0.2d, v0.2d, v4.2d");                             // accumulate the second mapped pair
    emitter.instruction("sub x9, x9, #4");                                      // account for the four consumed elements
    emitter.instruction(&format!("b {vector_loop}"));                           // continue consuming four elements at a time

    emitter.label(vector_pair);
    emitter.instruction("cmp x9, #2");                                          // is one full 64-bit pair still available?
    emitter.instruction(&format!("b.lo {vector_tail}"));                        // an odd tail element stays scalar
    emitter.instruction("ldr q3, [x10], #16");                                  // load the final complete element pair
    emitter.instruction("ushr v5.2d, v3.2d, #32");                              // isolate the pair's high dwords
    emitter.instruction("orr v1.16b, v1.16b, v5.16b");                          // record elements at or above 2^32 in the guard
    emitter.instruction("add v3.2d, v3.2d, v2.2d");                             // map the final pair with the literal offset
    emitter.instruction("add v0.2d, v0.2d, v3.2d");                             // accumulate the final mapped pair
    emitter.instruction("sub x9, x9, #2");                                      // account for the two consumed elements

    emitter.label(vector_tail);
    emitter.instruction("fmov x13, d1");                                        // move the guard's low 64-bit lane to an integer register
    emitter.instruction("mov x14, v1.d[1]");                                    // move the guard's high 64-bit lane to an integer register
    emitter.instruction("orr x13, x13, x14");                                   // combine both halves of the high-dword guard
    emitter.instruction(&format!("cbnz x13, {vector_restart}"));                // any wide element restarts the exact scalar sequence
    emitter.instruction("fmov x13, d0");                                        // move the accumulator's low 64-bit lane to an integer register
    emitter.instruction("mov x14, v0.d[1]");                                    // move the accumulator's high 64-bit lane to an integer register
    emitter.instruction("add x13, x13, x14");                                   // fold both accumulated lanes into the result
    emitter.instruction(&format!("cbz x9, {vector_box}"));                      // stop when the range had an even element count
    emitter.instruction("ldr x14, [x10]");                                      // load the single remaining tail element
    emitter.instruction("add x14, x14, x1");                                    // map the tail element with the literal offset
    emitter.instruction("add x13, x13, x14");                                   // the gate proves this addition stays in range

    emitter.label(vector_box);
    emitter.instruction(&format!("cbnz x16, {vector_raw}"));                    // raw result requested
    emitter.instruction("mov x0, #0");                                          // int Mixed tag for the vector sum
    emitter.instruction("mov x1, x13");                                         // int payload for the vector sum
    emitter.instruction("mov x2, xzr");                                         // the high payload word is unused for scalars
    emitter.instruction("b __rt_mixed_from_value");                             // box the integer sum and return
    emitter.label(vector_raw);
    emitter.instruction("mov x0, x13");                                         // return the raw vector sum
    emitter.instruction("ret");                                                 // transfer the scalar result to the caller

    emitter.label(vector_restart);
    emitter.instruction("add x10, x0, #24");                                    // rebuild the element cursor from the original array base
    emitter.instruction("ldr x9, [x0]");                                        // rebuild the exact element count after the failed gate
    emitter.instruction("mov x11, xzr");                                        // restart the integer accumulator at zero
    emitter.instruction("mov x12, xzr");                                        // restart in integer mode

    emitter.label(loop_label);
    emitter.instruction(&format!("cbz x9, {done}"));                            // stop when every element has been mapped and summed
    emitter.instruction("ldr x13, [x10], #8");                                  // load the next source element
    emitter.instruction("adds x14, x13, x1");                                   // map the element with PHP's checked integer addition
    emitter.instruction(&format!("b.vs {mapped_overflow}"));                    // an overflowed map result becomes a double
    emitter.instruction(&format!("cbnz x12, {float_acc}"));                     // float accumulation converts the mapped integer first
    emitter.instruction("adds x15, x11, x14");                                  // sum the mapped integer with the running accumulator
    emitter.instruction(&format!("b.vs {acc_overflow}"));                       // overflow switches the running sum to a double
    emitter.instruction("mov x11, x15");                                        // commit the in-range integer sum
    emitter.instruction("sub x9, x9, #1");                                      // one fewer element remains
    emitter.instruction(&format!("b {loop_label}"));                            // continue through the packed payload

    emitter.label(acc_overflow);
    emitter.instruction("scvtf d0, x11");                                       // convert the pre-overflow accumulator to double
    emitter.instruction("scvtf d1, x14");                                       // convert the mapped integer to double
    emitter.instruction("fadd d0, d0, d1");                                     // add them as PHP does after the promotion
    emitter.instruction("mov x12, #1");                                         // remain in double mode for the rest of the range
    emitter.instruction("sub x9, x9, #1");                                      // the promoting element is consumed
    emitter.instruction(&format!("b {loop_label}"));                            // continue accumulating as a double

    emitter.label(mapped_overflow);
    emitter.instruction("scvtf d1, x13");                                       // convert the source element to double
    emitter.instruction("scvtf d2, x1");                                        // convert the literal map offset to double
    emitter.instruction("fadd d1, d1, d2");                                     // build the float map result PHP would return
    emitter.instruction(&format!("cbnz x12, {add_float}"));                     // a float accumulator already holds the running sum
    emitter.instruction("scvtf d0, x11");                                       // promote the integer accumulator at this step
    emitter.instruction("mov x12, #1");                                         // remain in double mode for the rest of the range

    emitter.label(add_float);
    emitter.instruction("fadd d0, d0, d1");                                     // add the float map result to the running sum
    emitter.instruction("sub x9, x9, #1");                                      // the promoting element is consumed
    emitter.instruction(&format!("b {loop_label}"));                            // continue accumulating as a double

    emitter.label(float_acc);
    emitter.instruction("scvtf d1, x14");                                       // convert the in-range mapped integer to double
    emitter.instruction("fadd d0, d0, d1");                                     // accumulate it in PHP double mode
    emitter.instruction("sub x9, x9, #1");                                      // one fewer element remains
    emitter.instruction(&format!("b {loop_label}"));                            // continue through the packed payload

    emitter.label(done);
    emitter.instruction(&format!("cbnz x12, {box_float}"));                     // a promoted sum is returned as a float cell
    emitter.instruction(&format!("cbnz x16, {done_raw}"));                      // raw int result requested
    emitter.instruction("mov x0, #0");                                          // int Mixed tag for the integer sum
    emitter.instruction("mov x1, x11");                                         // int payload for the integer sum
    emitter.instruction("mov x2, xzr");                                         // the high payload word is unused for scalars
    emitter.instruction("b __rt_mixed_from_value");                             // box the integer sum and return
    emitter.label(done_raw);
    emitter.instruction("mov x0, x11");                                         // return the raw integer sum
    emitter.instruction("ret");                                                 // transfer the scalar result to the caller

    emitter.label(box_float);
    emitter.instruction(&format!("cbnz x16, {float_raw}"));                     // raw float-to-int result requested
    emitter.instruction("fmov x1, d0");                                         // float payload for the promoted sum
    emitter.instruction("mov x0, #2");                                          // float Mixed tag for the promoted sum
    emitter.instruction("mov x2, xzr");                                         // the high payload word is unused for scalars
    emitter.instruction("b __rt_mixed_from_value");                             // box the float sum and return
    emitter.label(float_raw);
    emitter.instruction("b __rt_float_key_to_int");                             // apply PHP's warned float-to-int conversion
}

/// Emits `__rt_array_map_sum_add` for Linux x86_64.
fn emit_map_sum_add_x86_64(emitter: &mut Emitter) {
    let loop_label = "__rt_array_map_sum_add_loop_x86";
    let mapped_overflow = "__rt_array_map_sum_add_mapped_overflow_x86";
    let acc_overflow = "__rt_array_map_sum_add_acc_overflow_x86";
    let float_acc = "__rt_array_map_sum_add_float_acc_x86";
    let add_float = "__rt_array_map_sum_add_add_float_x86";
    let done = "__rt_array_map_sum_add_done_x86";
    let box_float = "__rt_array_map_sum_add_box_float_x86";
    let vector_loop = "__rt_array_map_sum_add_vector_loop_x86";
    let vector_pair = "__rt_array_map_sum_add_vector_pair_x86";
    let vector_tail = "__rt_array_map_sum_add_vector_tail_x86";
    let vector_box = "__rt_array_map_sum_add_vector_box_x86";
    let vector_raw = "__rt_array_map_sum_add_vector_raw_x86";
    let vector_restart = "__rt_array_map_sum_add_vector_restart_x86";
    let done_raw = "__rt_array_map_sum_add_done_raw_x86";
    let float_raw = "__rt_array_map_sum_add_float_raw_x86";

    emitter.blank();
    emitter.comment("--- runtime: array_map_sum_add (fused int kernel) ---");
    emitter.label_global("__rt_array_map_sum_add_int");
    emitter.instruction("mov eax, 1");                                          // raw result mode: skip the Mixed box
    emitter.instruction("movd xmm7, eax");                                      // keep the mode in a free caller-saved XMM register
    emitter.instruction("jmp __rt_array_map_sum_add_body");                     // share the kernel body with the boxed entry
    emitter.label_global("__rt_array_map_sum_add");
    emitter.instruction("pxor xmm7, xmm7");                                     // boxed result mode
    emitter.label_global("__rt_array_map_sum_add_body");
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the packed element count
    emitter.instruction("lea r9, [rdi + 24]");                                  // skip the indexed-array header to the payload base
    emitter.instruction("xor r8d, r8d");                                        // start the integer accumulator at zero
    emitter.instruction("xor r11d, r11d");                                      // integer mode: promote to double on the first overflow

    // -- SIMD gate: non-negative bounded offset, a bounded count, and 32-bit elements --
    emitter.instruction("cmp r10, 8");                                          // tiny arrays keep the scalar loop and its small fixed cost
    emitter.instruction(&format!("jl {loop_label}"));                           // below eight elements the scalar loop wins
    emitter.instruction("test rsi, rsi");                                       // negative offsets can promote individual map results
    emitter.instruction(&format!("js {loop_label}"));                           // negative offsets need the scalar promotion sequence
    emitter.instruction("mov rax, rsi");                                        // copy the offset before isolating its bit 31
    emitter.instruction("shr rax, 31");                                         // do offsets at or above 2^31 need the scalar path?
    emitter.instruction(&format!("jnz {loop_label}"));                          // large offsets cannot use the vector total bound
    emitter.instruction("mov eax, 0x3fffffff");                                 // bound the count so count * (2^32 + 2^31) stays below 2^63
    emitter.instruction("cmp r10, rax");                                        // does the element count fit the vector total bound?
    emitter.instruction(&format!("ja {loop_label}"));                           // larger ranges restart with the exact scalar sequence
    emitter.instruction("pxor xmm0, xmm0");                                     // zero the two-lane 64-bit accumulator
    emitter.instruction("pxor xmm1, xmm1");                                     // zero the OR-accumulated high-dword guard
    emitter.instruction("movq xmm2, rsi");                                      // seed the offset broadcast with the low 64-bit lane
    emitter.instruction("punpcklqdq xmm2, xmm2");                               // broadcast the literal map offset into both lanes

    emitter.label(vector_loop);
    emitter.instruction("cmp r10, 4");                                          // can two full vectors be consumed together?
    emitter.instruction(&format!("jl {vector_pair}"));                          // fall back to a single vector for two or three elements
    emitter.instruction("movdqu xmm3, XMMWORD PTR [r9]");                       // load the first two source elements
    emitter.instruction("movdqu xmm4, XMMWORD PTR [r9 + 16]");                  // load the next two source elements
    emitter.instruction("add r9, 32");                                          // advance past the four consumed elements
    emitter.instruction("movdqa xmm5, xmm3");                                   // copy the first pair before isolating high dwords
    emitter.instruction("psrlq xmm5, 32");                                      // move the first pair's high dwords into their low lanes
    emitter.instruction("movdqa xmm6, xmm4");                                   // copy the second pair before isolating high dwords
    emitter.instruction("psrlq xmm6, 32");                                      // move the second pair's high dwords into their low lanes
    emitter.instruction("por xmm1, xmm5");                                      // record any element at or above 2^32 in the guard
    emitter.instruction("por xmm1, xmm6");                                      // record the second pair's high dwords too
    emitter.instruction("paddq xmm3, xmm2");                                    // map the first pair with the literal offset
    emitter.instruction("paddq xmm4, xmm2");                                    // map the second pair with the literal offset
    emitter.instruction("paddq xmm0, xmm3");                                    // accumulate the first mapped pair
    emitter.instruction("paddq xmm0, xmm4");                                    // accumulate the second mapped pair
    emitter.instruction("sub r10, 4");                                          // account for the four consumed elements
    emitter.instruction(&format!("jmp {vector_loop}"));                         // continue consuming four elements at a time

    emitter.label(vector_pair);
    emitter.instruction("cmp r10, 2");                                          // is one full 64-bit pair still available?
    emitter.instruction(&format!("jl {vector_tail}"));                          // an odd tail element stays scalar
    emitter.instruction("movdqu xmm3, XMMWORD PTR [r9]");                       // load the final complete element pair
    emitter.instruction("add r9, 16");                                          // advance past the two consumed elements
    emitter.instruction("movdqa xmm5, xmm3");                                   // copy the pair before isolating high dwords
    emitter.instruction("psrlq xmm5, 32");                                      // isolate the pair's high dwords
    emitter.instruction("por xmm1, xmm5");                                      // record elements at or above 2^32 in the guard
    emitter.instruction("paddq xmm3, xmm2");                                    // map the final pair with the literal offset
    emitter.instruction("paddq xmm0, xmm3");                                    // accumulate the final mapped pair
    emitter.instruction("sub r10, 2");                                          // account for the two consumed elements

    emitter.label(vector_tail);
    emitter.instruction("movq rax, xmm1");                                      // move the guard's low 64-bit lane to an integer register
    emitter.instruction("movdqa xmm5, xmm1");                                   // copy the guard before extracting its high lane
    emitter.instruction("psrldq xmm5, 8");                                      // move the guard's high 64-bit lane into the low lane
    emitter.instruction("movq r11, xmm5");                                      // copy the high guard lane into an integer register
    emitter.instruction("or rax, r11");                                         // combine both halves of the high-dword guard
    emitter.instruction(&format!("jnz {vector_restart}"));                      // any wide element restarts the exact scalar sequence
    emitter.instruction("movq rax, xmm0");                                      // move the accumulator's low 64-bit lane to an integer register
    emitter.instruction("movdqa xmm5, xmm0");                                   // copy the accumulator before extracting its high lane
    emitter.instruction("psrldq xmm5, 8");                                      // move the accumulator's high 64-bit lane into the low lane
    emitter.instruction("movq r11, xmm5");                                      // copy the high accumulator lane into an integer register
    emitter.instruction("add rax, r11");                                        // fold both accumulated lanes into the result
    emitter.instruction("test r10, r10");                                       // stop when the range had an even element count
    emitter.instruction(&format!("jz {vector_box}"));                           // box the completed vector sum
    emitter.instruction("mov rcx, QWORD PTR [r9]");                             // load the single remaining tail element
    emitter.instruction("add rcx, rsi");                                        // map the tail element with the literal offset
    emitter.instruction("add rax, rcx");                                        // the gate proves the tail addition stays in range

    emitter.label(vector_box);
    emitter.instruction("movd ecx, xmm7");                                      // load the result mode
    emitter.instruction("test ecx, ecx");                                       // raw result requested?
    emitter.instruction(&format!("jnz {vector_raw}"));                          // return the raw int sum
    emitter.instruction("mov rdi, rax");                                        // int payload for the vector sum
    emitter.instruction("xor eax, eax");                                        // int Mixed tag for the vector sum
    emitter.instruction("xor esi, esi");                                        // the high payload word is unused for scalars
    emitter.instruction("jmp __rt_mixed_from_value");                           // box the integer sum and return
    emitter.label(vector_raw);
    emitter.instruction("ret");                                                 // rax already holds the raw int sum

    emitter.label(vector_restart);
    emitter.instruction("lea r9, [rdi + 24]");                                  // rebuild the element cursor from the original array base
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // rebuild the exact element count after the failed gate
    emitter.instruction("xor r8d, r8d");                                        // restart the integer accumulator at zero
    emitter.instruction("xor r11d, r11d");                                      // restart in integer mode

    emitter.label(loop_label);
    emitter.instruction("test r10, r10");                                       // stop when every element has been mapped and summed
    emitter.instruction(&format!("jz {done}"));                                 // the range is exhausted
    emitter.instruction("mov rax, QWORD PTR [r9]");                             // load the next source element
    emitter.instruction("add r9, 8");                                           // advance the element cursor
    emitter.instruction("mov rcx, rax");                                        // copy the element before mapping it
    emitter.instruction("add rcx, rsi");                                        // map the element with PHP's checked integer addition
    emitter.instruction(&format!("jo {mapped_overflow}"));                      // an overflowed map result becomes a double
    emitter.instruction("test r11d, r11d");                                     // is the running sum already a double?
    emitter.instruction(&format!("jnz {float_acc}"));                           // float accumulation converts the mapped integer first
    emitter.instruction("mov rdx, r8");                                         // copy the integer accumulator before the checked add
    emitter.instruction("add rdx, rcx");                                        // sum the mapped integer with the running accumulator
    emitter.instruction(&format!("jo {acc_overflow}"));                         // overflow switches the running sum to a double
    emitter.instruction("mov r8, rdx");                                         // commit the in-range integer sum
    emitter.instruction("dec r10");                                             // one fewer element remains
    emitter.instruction(&format!("jmp {loop_label}"));                          // continue through the packed payload

    emitter.label(acc_overflow);
    emitter.instruction("cvtsi2sd xmm0, r8");                                   // convert the pre-overflow accumulator to double
    emitter.instruction("cvtsi2sd xmm1, rcx");                                  // convert the mapped integer to double
    emitter.instruction("addsd xmm0, xmm1");                                    // add them as PHP does after the promotion
    emitter.instruction("mov r11d, 1");                                         // remain in double mode for the rest of the range
    emitter.instruction("dec r10");                                             // the promoting element is consumed
    emitter.instruction(&format!("jmp {loop_label}"));                          // continue accumulating as a double

    emitter.label(mapped_overflow);
    emitter.instruction("cvtsi2sd xmm1, rax");                                  // convert the source element to double
    emitter.instruction("cvtsi2sd xmm2, rsi");                                  // convert the literal map offset to double
    emitter.instruction("addsd xmm1, xmm2");                                    // build the float map result PHP would return
    emitter.instruction("test r11d, r11d");                                     // is the running sum already a double?
    emitter.instruction(&format!("jnz {add_float}"));                           // a float accumulator already holds the running sum
    emitter.instruction("cvtsi2sd xmm0, r8");                                   // promote the integer accumulator at this step
    emitter.instruction("mov r11d, 1");                                         // remain in double mode for the rest of the range

    emitter.label(add_float);
    emitter.instruction("addsd xmm0, xmm1");                                    // add the float map result to the running sum
    emitter.instruction("dec r10");                                             // the promoting element is consumed
    emitter.instruction(&format!("jmp {loop_label}"));                          // continue accumulating as a double

    emitter.label(float_acc);
    emitter.instruction("cvtsi2sd xmm1, rcx");                                  // convert the in-range mapped integer to double
    emitter.instruction("addsd xmm0, xmm1");                                    // accumulate it in PHP double mode
    emitter.instruction("dec r10");                                             // one fewer element remains
    emitter.instruction(&format!("jmp {loop_label}"));                          // continue through the packed payload

    emitter.label(done);
    emitter.instruction("test r11d, r11d");                                     // a promoted sum is returned as a float cell
    emitter.instruction(&format!("jnz {box_float}"));                           // box the promoted double
    emitter.instruction("movd ecx, xmm7");                                      // load the result mode
    emitter.instruction("test ecx, ecx");                                       // raw result requested?
    emitter.instruction(&format!("jnz {done_raw}"));                            // return the raw integer sum
    emitter.instruction("xor eax, eax");                                        // int Mixed tag for the integer sum
    emitter.instruction("mov rdi, r8");                                         // int payload for the integer sum
    emitter.instruction("xor esi, esi");                                        // the high payload word is unused for scalars
    emitter.instruction("jmp __rt_mixed_from_value");                           // box the integer sum and return
    emitter.label(done_raw);
    emitter.instruction("mov rax, r8");                                         // return the raw integer sum
    emitter.instruction("ret");                                                 // transfer the scalar result to the caller

    emitter.label(box_float);
    emitter.instruction("movd ecx, xmm7");                                      // load the result mode
    emitter.instruction("test ecx, ecx");                                       // raw result requested?
    emitter.instruction(&format!("jnz {float_raw}"));                           // apply the warned float-to-int conversion
    emitter.instruction("movq rdi, xmm0");                                      // float payload for the promoted sum
    emitter.instruction("mov eax, 2");                                          // float Mixed tag for the promoted sum
    emitter.instruction("xor esi, esi");                                        // the high payload word is unused for scalars
    emitter.instruction("jmp __rt_mixed_from_value");                           // box the float sum and return
    emitter.label(float_raw);
    emitter.instruction("jmp __rt_float_key_to_int");                           // apply PHP's warned float-to-int conversion
}

/// Emits the AArch64 overflow preflight for a declared-int fused map callback.
fn emit_map_sum_add_safe_aarch64(emitter: &mut Emitter) {
    let loop_label = "__rt_array_map_sum_add_safe_loop";
    let safe = "__rt_array_map_sum_add_safe_yes";
    let unsafe_label = "__rt_array_map_sum_add_safe_no";

    emitter.blank();
    emitter.comment("--- runtime: array_map_sum_add_safe (declared-int preflight) ---");
    emitter.label_global("__rt_array_map_sum_add_safe");
    emitter.instruction("ldr x9, [x0]");                                        // load the packed element count
    emitter.instruction("add x10, x0, #24");                                    // skip the indexed-array header to the payload base

    emitter.label(loop_label);
    emitter.instruction(&format!("cbz x9, {safe}"));                            // an exhausted range never overflowed
    emitter.instruction("ldr x13, [x10], #8");                                  // load the next source element
    emitter.instruction("adds x14, x13, x1");                                   // perform the same checked map addition as PHP
    emitter.instruction(&format!("b.vs {unsafe_label}"));                       // one overflowed map value makes the fused arm invalid
    emitter.instruction("sub x9, x9, #1");                                      // one fewer element remains
    emitter.instruction(&format!("b {loop_label}"));                            // continue scanning the packed payload

    emitter.label(safe);
    emitter.instruction("mov x0, #1");                                          // every map result stays an integer
    emitter.instruction("ret");                                                 // report the safe range

    emitter.label(unsafe_label);
    emitter.instruction("mov x0, xzr");                                         // at least one map result promotes to float
    emitter.instruction("ret");                                                 // report the unsafe range
}

/// Emits `__rt_array_filter_count_mod` for AArch64.
fn emit_filter_count_mod_aarch64(emitter: &mut Emitter) {
    let normalize = "__rt_array_filter_count_mod_normalize";
    let power_check = "__rt_array_filter_count_mod_power_check";
    let mask_loop = "__rt_array_filter_count_mod_mask_loop";
    let mask_skip = "__rt_array_filter_count_mod_mask_skip";
    let mod_loop = "__rt_array_filter_count_mod_loop";
    let mod_skip = "__rt_array_filter_count_mod_skip";
    let done = "__rt_array_filter_count_mod_done";
    let vector_loop = "__rt_array_filter_count_mod_vector_loop";
    let vector_pair = "__rt_array_filter_count_mod_vector_pair";
    let vector_tail = "__rt_array_filter_count_mod_vector_tail";

    emitter.blank();
    emitter.comment("--- runtime: array_filter_count_mod (fused int kernel) ---");
    emitter.label_global("__rt_array_filter_count_mod");
    emitter.instruction("ldr x9, [x0]");                                        // load the packed element count
    emitter.instruction("add x10, x0, #24");                                    // skip the indexed-array header to the payload base
    emitter.instruction("mov x11, xzr");                                        // count elements divisible by the literal divisor
    emitter.instruction(&format!("cbz x9, {done}"));                            // an empty array counts zero matches
    emitter.instruction("mov x12, x1");                                         // copy the divisor before normalizing its sign
    emitter.instruction(&format!("tbnz x12, #63, {normalize}"));                // negative divisors use their absolute value

    emitter.label(power_check);
    emitter.instruction("sub x14, x12, #1");                                    // mask for divisibility by a power of two
    emitter.instruction("tst x14, x12");                                        // is the normalized divisor a power of two?
    emitter.instruction(&format!("b.ne {mod_loop}"));                           // non-powers of two need a real remainder

    // -- SIMD mask count for power-of-two divisors --
    emitter.instruction("cmp x9, #8");                                          // tiny arrays keep the scalar mask loop
    emitter.instruction(&format!("b.lo {mask_loop}"));                          // below eight elements the scalar loop wins
    emitter.instruction("movi v0.2d, #0");                                      // zero the two-lane 64-bit match counter
    emitter.instruction("dup v2.2d, x14");                                      // broadcast the divisibility mask into both lanes

    emitter.label(vector_loop);
    emitter.instruction("cmp x9, #4");                                          // can two full vectors be consumed together?
    emitter.instruction(&format!("b.lo {vector_pair}"));                        // fall back to a single vector for two or three elements
    emitter.instruction("ldp q3, q4, [x10], #32");                              // load four consecutive source elements
    emitter.instruction("and v3.16b, v3.16b, v2.16b");                          // keep only the divisibility bits of the first pair
    emitter.instruction("and v4.16b, v4.16b, v2.16b");                          // keep only the divisibility bits of the second pair
    emitter.instruction("cmeq v3.2d, v3.2d, #0");                               // all-ones lanes mark divisible elements
    emitter.instruction("cmeq v4.2d, v4.2d, #0");                               // all-ones lanes mark the second pair's matches
    emitter.instruction("sub v0.2d, v0.2d, v3.2d");                             // add one per matching 64-bit lane
    emitter.instruction("sub v0.2d, v0.2d, v4.2d");                             // add one per second-pair match
    emitter.instruction("sub x9, x9, #4");                                      // account for the four consumed elements
    emitter.instruction(&format!("b {vector_loop}"));                           // continue consuming four elements at a time

    emitter.label(vector_pair);
    emitter.instruction("cmp x9, #2");                                          // is one full 64-bit pair still available?
    emitter.instruction(&format!("b.lo {vector_tail}"));                        // an odd tail element stays scalar
    emitter.instruction("ldr q3, [x10], #16");                                  // load the final complete element pair
    emitter.instruction("and v3.16b, v3.16b, v2.16b");                          // keep only the pair's divisibility bits
    emitter.instruction("cmeq v3.2d, v3.2d, #0");                               // all-ones lanes mark divisible elements
    emitter.instruction("sub v0.2d, v0.2d, v3.2d");                             // add one per matching lane
    emitter.instruction("sub x9, x9, #2");                                      // account for the two consumed elements

    emitter.label(vector_tail);
    emitter.instruction("fmov x13, d0");                                        // move the counter's low 64-bit lane to an integer register
    emitter.instruction("mov x15, v0.d[1]");                                    // move the counter's high 64-bit lane to an integer register
    emitter.instruction("add x13, x13, x15");                                   // fold both counted lanes into the result
    emitter.instruction("add x11, x11, x13");                                   // add the vector matches to the scalar count
    emitter.instruction(&format!("cbz x9, {done}"));                            // stop when the range had an even element count
    emitter.instruction("sub x14, x12, #1");                                    // restore the scalar divisibility mask for the tail
    emitter.instruction(&format!("b {mask_loop}"));                             // consume the single remaining element scalar

    emitter.label(mask_loop);
    emitter.instruction("ldr x13, [x10], #8");                                  // load the next source element
    emitter.instruction("tst x13, x14");                                        // is the element a multiple of the power-of-two divisor?
    emitter.instruction(&format!("b.ne {mask_skip}"));                          // non-multiples do not increment the count
    emitter.instruction("add x11, x11, #1");                                    // record a divisible element
    emitter.label(mask_skip);
    emitter.instruction("subs x9, x9, #1");                                     // one fewer element remains
    emitter.instruction(&format!("b.ne {mask_loop}"));                          // continue through the packed payload
    emitter.instruction(&format!("b {done}"));                                  // return the completed count

    emitter.label(normalize);
    emitter.instruction("neg x12, x12");                                        // divisibility is unchanged by the divisor's sign
    emitter.instruction(&format!("b {power_check}"));                           // retry the power-of-two recognition

    emitter.label(mod_loop);
    emitter.instruction("ldr x13, [x10], #8");                                  // load the next source element
    emitter.instruction("sdiv x15, x13, x12");                                  // form the truncated quotient
    emitter.instruction("msub x15, x15, x12, x13");                             // derive PHP's truncated remainder
    emitter.instruction(&format!("cbnz x15, {mod_skip}"));                      // non-zero remainders do not increment the count
    emitter.instruction("add x11, x11, #1");                                    // record a divisible element
    emitter.label(mod_skip);
    emitter.instruction("subs x9, x9, #1");                                     // one fewer element remains
    emitter.instruction(&format!("b.ne {mod_loop}"));                           // continue through the packed payload

    emitter.label(done);
    emitter.instruction("mov x0, x11");                                         // return the raw PHP int count
    emitter.instruction("ret");                                                 // transfer the count to the caller
}

/// Emits the x86_64 overflow preflight for a declared-int fused map callback.
fn emit_map_sum_add_safe_x86_64(emitter: &mut Emitter) {
    let loop_label = "__rt_array_map_sum_add_safe_loop_x86";
    let safe = "__rt_array_map_sum_add_safe_yes_x86";
    let unsafe_label = "__rt_array_map_sum_add_safe_no_x86";

    emitter.blank();
    emitter.comment("--- runtime: array_map_sum_add_safe (declared-int preflight) ---");
    emitter.label_global("__rt_array_map_sum_add_safe");
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the packed element count
    emitter.instruction("lea r9, [rdi + 24]");                                  // skip the indexed-array header to the payload base

    emitter.label(loop_label);
    emitter.instruction("test r10, r10");                                       // an exhausted range never overflowed
    emitter.instruction(&format!("jz {safe}"));                                 // report the safe range
    emitter.instruction("mov rax, QWORD PTR [r9]");                             // load the next source element
    emitter.instruction("add r9, 8");                                           // advance the element cursor
    emitter.instruction("mov rcx, rax");                                        // copy the element before mapping it
    emitter.instruction("add rcx, rsi");                                        // perform the same checked map addition as PHP
    emitter.instruction(&format!("jo {unsafe_label}"));                         // one overflowed map value makes the fused arm invalid
    emitter.instruction("dec r10");                                             // one fewer element remains
    emitter.instruction(&format!("jmp {loop_label}"));                          // continue scanning the packed payload

    emitter.label(safe);
    emitter.instruction("mov eax, 1");                                          // every map result stays an integer
    emitter.instruction("ret");                                                 // report the safe range

    emitter.label(unsafe_label);
    emitter.instruction("xor eax, eax");                                        // at least one map result promotes to float
    emitter.instruction("ret");                                                 // report the unsafe range
}

/// Emits `__rt_array_filter_count_mod` for Linux x86_64.
fn emit_filter_count_mod_x86_64(emitter: &mut Emitter) {
    let mask_loop = "__rt_array_filter_count_mod_mask_loop_x86";
    let mask_skip = "__rt_array_filter_count_mod_mask_skip_x86";
    let mod_loop = "__rt_array_filter_count_mod_loop_x86";
    let mod_skip = "__rt_array_filter_count_mod_skip_x86";
    let done = "__rt_array_filter_count_mod_done_x86";
    let vector_loop = "__rt_array_filter_count_mod_vector_loop_x86";
    let vector_pair = "__rt_array_filter_count_mod_vector_pair_x86";
    let vector_tail = "__rt_array_filter_count_mod_vector_tail_x86";

    emitter.blank();
    emitter.comment("--- runtime: array_filter_count_mod (fused int kernel) ---");
    emitter.label_global("__rt_array_filter_count_mod");
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the packed element count
    emitter.instruction("lea r9, [rdi + 24]");                                  // skip the indexed-array header to the payload base
    emitter.instruction("xor r8d, r8d");                                        // count elements divisible by the literal divisor
    emitter.instruction("mov r11, rsi");                                        // copy the divisor before normalizing its sign
    emitter.instruction("test r11, r11");                                       // is the divisor negative?
    emitter.instruction("jns __rt_array_filter_count_mod_positive_x86");        // positive divisors already name their absolute value
    emitter.instruction("neg r11");                                             // divisibility is unchanged by the divisor's sign
    emitter.label("__rt_array_filter_count_mod_positive_x86");
    emitter.instruction("test r10, r10");                                       // an empty array counts zero matches
    emitter.instruction(&format!("jz {done}"));                                 // return before touching the payload
    emitter.instruction("lea rcx, [r11 - 1]");                                  // mask for divisibility by a power of two
    emitter.instruction("test rcx, r11");                                       // is the normalized divisor a power of two?
    emitter.instruction(&format!("jnz {mod_loop}"));                            // non-powers of two need a real remainder

    // -- SIMD mask count for power-of-two divisors --
    emitter.instruction("cmp r10, 8");                                          // tiny arrays keep the scalar mask loop
    emitter.instruction(&format!("jl {mask_loop}"));                            // below eight elements the scalar loop wins
    emitter.instruction("movq xmm2, rcx");                                      // seed the mask broadcast with the low 64-bit lane
    emitter.instruction("punpcklqdq xmm2, xmm2");                               // broadcast the divisibility mask into both lanes
    emitter.instruction("pxor xmm0, xmm0");                                     // zero the two-lane 64-bit match counter
    emitter.instruction("pxor xmm7, xmm7");                                     // zero the comparison vector

    emitter.label(vector_loop);
    emitter.instruction("cmp r10, 4");                                          // can two full vectors be consumed together?
    emitter.instruction(&format!("jl {vector_pair}"));                          // fall back to a single vector for two or three elements
    emitter.instruction("movdqu xmm3, XMMWORD PTR [r9]");                       // load the first two source elements
    emitter.instruction("movdqu xmm4, XMMWORD PTR [r9 + 16]");                  // load the next two source elements
    emitter.instruction("add r9, 32");                                          // advance past the four consumed elements
    emitter.instruction("pand xmm3, xmm2");                                     // keep only the divisibility bits of the first pair
    emitter.instruction("pand xmm4, xmm2");                                     // keep only the divisibility bits of the second pair
    emitter.instruction("pcmpeqd xmm3, xmm7");                                  // all-ones dwords mark zero divisibility bits
    emitter.instruction("pcmpeqd xmm4, xmm7");                                  // all-ones dwords mark the second pair's matches
    emitter.instruction("pshufd xmm5, xmm3, 0xb1");                             // swap adjacent dwords to combine both halves
    emitter.instruction("pshufd xmm6, xmm4, 0xb1");                             // swap the second pair's adjacent dwords
    emitter.instruction("pand xmm3, xmm5");                                     // both dwords now mark a fully divisible element
    emitter.instruction("pand xmm4, xmm6");                                     // both dwords now mark the second pair's matches
    emitter.instruction("psubq xmm0, xmm3");                                    // add one per matching 64-bit lane
    emitter.instruction("psubq xmm0, xmm4");                                    // add one per second-pair match
    emitter.instruction("sub r10, 4");                                          // account for the four consumed elements
    emitter.instruction(&format!("jmp {vector_loop}"));                         // continue consuming four elements at a time

    emitter.label(vector_pair);
    emitter.instruction("cmp r10, 2");                                          // is one full 64-bit pair still available?
    emitter.instruction(&format!("jl {vector_tail}"));                          // an odd tail element stays scalar
    emitter.instruction("movdqu xmm3, XMMWORD PTR [r9]");                       // load the final complete element pair
    emitter.instruction("add r9, 16");                                          // advance past the two consumed elements
    emitter.instruction("pand xmm3, xmm2");                                     // keep only the pair's divisibility bits
    emitter.instruction("pcmpeqd xmm3, xmm7");                                  // all-ones dwords mark zero divisibility bits
    emitter.instruction("pshufd xmm5, xmm3, 0xb1");                             // swap adjacent dwords to combine both halves
    emitter.instruction("pand xmm3, xmm5");                                     // both dwords now mark a fully divisible element
    emitter.instruction("psubq xmm0, xmm3");                                    // add one per matching 64-bit lane
    emitter.instruction("sub r10, 2");                                          // account for the two consumed elements

    emitter.label(vector_tail);
    emitter.instruction("movq rax, xmm0");                                      // move the counter's low 64-bit lane to an integer register
    emitter.instruction("movdqa xmm5, xmm0");                                   // copy the counter before extracting its high lane
    emitter.instruction("psrldq xmm5, 8");                                      // move the counter's high 64-bit lane into the low lane
    emitter.instruction("movq r11, xmm5");                                      // copy the high counted lane into an integer register
    emitter.instruction("add rax, r11");                                        // fold both counted lanes into the result
    emitter.instruction("add r8, rax");                                         // add the vector matches to the scalar count
    emitter.instruction("test r10, r10");                                       // stop when the range had an even element count
    emitter.instruction(&format!("jz {done}"));                                 // return the completed count
    emitter.instruction(&format!("jmp {mask_loop}"));                           // consume the single remaining element scalar

    emitter.label(mask_loop);
    emitter.instruction("mov rax, QWORD PTR [r9]");                             // load the next source element
    emitter.instruction("add r9, 8");                                           // advance the element cursor
    emitter.instruction("test rax, rcx");                                       // is the element a multiple of the power-of-two divisor?
    emitter.instruction(&format!("jnz {mask_skip}"));                           // non-multiples do not increment the count
    emitter.instruction("inc r8");                                              // record a divisible element
    emitter.label(mask_skip);
    emitter.instruction("dec r10");                                             // one fewer element remains
    emitter.instruction(&format!("jnz {mask_loop}"));                           // continue through the packed payload
    emitter.instruction(&format!("jmp {done}"));                                // return the completed count

    emitter.label(mod_loop);
    emitter.instruction("mov rax, QWORD PTR [r9]");                             // load the next source element
    emitter.instruction("add r9, 8");                                           // advance the element cursor
    emitter.instruction("cqo");                                                 // sign-extend the dividend into the high division word
    emitter.instruction("idiv r11");                                            // derive the truncated quotient and remainder
    emitter.instruction(&format!("test rdx, rdx"));                             // is the remainder zero?
    emitter.instruction(&format!("jnz {mod_skip}"));                            // non-zero remainders do not increment the count
    emitter.instruction("inc r8");                                              // record a divisible element
    emitter.label(mod_skip);
    emitter.instruction("dec r10");                                             // one fewer element remains
    emitter.instruction(&format!("jnz {mod_loop}"));                            // continue through the packed payload

    emitter.label(done);
    emitter.instruction("mov rax, r8");                                         // return the raw PHP int count
    emitter.instruction("ret");                                                 // transfer the count to the caller
}
