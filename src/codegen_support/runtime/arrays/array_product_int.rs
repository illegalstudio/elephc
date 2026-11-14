//! Purpose:
//! Emits `__rt_array_product_int_i64` and `__rt_array_product_float_i64`, the raw
//! seeded reduction helpers for the fused `array_reduce()` product.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::managed::emit_managed_runtime`.
//!
//! Key details:
//! - `__rt_array_product_int_i64` starts from a raw int seed and multiplies every
//!   `array<int>` element with PHP's checked integer semantics. The first overflow
//!   converts both operands to double, multiplies them, and continues in double mode.
//! - `__rt_array_product_float_i64` starts from raw IEEE-754 seed bits and multiplies
//!   every element in double mode.
//! - An empty range boxes the seed unchanged: int seed as tag 0, float seed as tag 2,
//!   so `-0.0` stays `float(-0)`.
//! - Both entries only use caller-saved registers and tail-call
//!   `__rt_mixed_from_value`, so they need no stack frame and box exactly one result.
//! - Each entry is self-contained so Mach-O dead stripping can remove an unused one
//!   without leaving a branch into removed code.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits every seeded product reducer entry for the active target.
pub fn emit_array_product_int(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter, false);
        emit_x86_64(emitter, true);
        return;
    }
    emit_aarch64(emitter, false);
    emit_aarch64(emitter, true);
}

/// Emits one self-contained AArch64 product entry; `raw_float_seed` selects the
/// double-seeded variant.
fn emit_aarch64(emitter: &mut Emitter, raw_float_seed: bool) {
    let prefix = if raw_float_seed {
        "__rt_array_product_float_i64"
    } else {
        "__rt_array_product_int_i64"
    };
    let loop_label = format!("{prefix}_int_loop");
    let promote_label = format!("{prefix}_promote");
    let float_loop = format!("{prefix}_float_loop");
    let box_int = format!("{prefix}_box_int");
    let box_float = format!("{prefix}_box_float");
    let empty_label = format!("{prefix}_empty");

    emitter.blank();
    emitter.comment("--- runtime: array_product int reducer ---");
    emitter.label_global(prefix);
    emitter.instruction("subs x9, x2, x1");                                     // count = end - start and flag an empty range
    emitter.instruction(&format!("b.le {empty_label}"));                        // empty or reversed ranges keep the seed value
    emitter.instruction("add x10, x0, #24");                                    // skip the indexed-array header to the payload base
    emitter.instruction("add x10, x10, x1, lsl #3");                            // address the first element in the requested range
    if raw_float_seed {
        emitter.instruction("fmov d0, x3");                                     // seed the double accumulator from the raw float bits
        emitter.instruction(&format!("b {float_loop}"));                        // float seeds skip the integer sequence
    } else {
        emitter.instruction("mov x11, x3");                                     // seed the integer accumulator from the raw int seed

        emitter.label(&loop_label);
        emitter.instruction("ldr x13, [x10], #8");                              // load the next int element and advance the cursor
        emitter.instruction("mul x14, x11, x13");                               // compute the low signed product
        emitter.instruction("smulh x15, x11, x13");                             // compute the high signed product
        emitter.instruction("cmp x15, x14, asr #63");                           // does the product fit a signed 64-bit integer?
        emitter.instruction(&format!("b.ne {promote_label}"));                  // a mismatched high word promotes to double
        emitter.instruction("mov x11, x14");                                    // commit the in-range integer product
        emitter.instruction("subs x9, x9, #1");                                 // one fewer element remains
        emitter.instruction(&format!("b.ne {loop_label}"));                     // continue while the range is non-empty
        emitter.instruction(&format!("b {box_int}"));                           // box the completed integer product

        emitter.label(&promote_label);
        emitter.instruction("scvtf d0, x11");                                   // convert the pre-overflow carry to double
        emitter.instruction("scvtf d1, x13");                                   // convert the overflowing element to double
        emitter.instruction("fmul d0, d0, d1");                                 // build PHP's promoted float product
        emitter.instruction("subs x9, x9, #1");                                 // the promoting element is consumed
        emitter.instruction(&format!("b.eq {box_float}"));                      // stop when no elements remain
    }

    emitter.label(&float_loop);
    emitter.instruction("ldr x13, [x10], #8");                                  // load the next int element and advance the cursor
    emitter.instruction("scvtf d1, x13");                                       // convert the element to double
    emitter.instruction("fmul d0, d0, d1");                                     // accumulate in PHP double mode
    emitter.instruction("subs x9, x9, #1");                                     // one fewer element remains
    emitter.instruction(&format!("b.ne {float_loop}"));                         // continue while the range is non-empty

    emitter.label(&box_float);
    emitter.instruction("fmov x1, d0");                                         // float payload for the boxed result
    emitter.instruction("mov x0, #2");                                          // float Mixed tag for the result
    emitter.instruction("mov x2, xzr");                                         // the high payload word is unused for scalars
    emitter.instruction("b __rt_mixed_from_value");                             // box the float product and return

    emitter.label(&box_int);
    emitter.instruction("mov x0, #0");                                          // int Mixed tag for the result
    emitter.instruction("mov x1, x11");                                         // int payload for the result
    emitter.instruction("mov x2, xzr");                                         // the high payload word is unused for scalars
    emitter.instruction("b __rt_mixed_from_value");                             // box the integer product and return

    emitter.label(&empty_label);
    if raw_float_seed {
        emitter.instruction("mov x0, #2");                                      // float Mixed tag for the empty product
        emitter.instruction("mov x1, x3");                                      // the raw float seed becomes the payload
        emitter.instruction("mov x2, xzr");                                     // the high payload word is unused for scalars
    } else {
        emitter.instruction("mov x0, #0");                                      // int Mixed tag for the empty product
        emitter.instruction("mov x1, x3");                                      // the raw int seed becomes the payload
        emitter.instruction("mov x2, xzr");                                     // the high payload word is unused for scalars
    }
    emitter.instruction("b __rt_mixed_from_value");                             // box the seed and return
}

/// Emits one self-contained x86_64 product entry; `raw_float_seed` selects the
/// double-seeded variant.
fn emit_x86_64(emitter: &mut Emitter, raw_float_seed: bool) {
    let prefix = if raw_float_seed {
        "__rt_array_product_float_i64"
    } else {
        "__rt_array_product_int_i64"
    };
    let loop_label = format!("{prefix}_int_loop");
    let promote_label = format!("{prefix}_promote");
    let float_loop = format!("{prefix}_float_loop");
    let box_int = format!("{prefix}_box_int");
    let box_float = format!("{prefix}_box_float");
    let empty_label = format!("{prefix}_empty");

    emitter.blank();
    emitter.comment("--- runtime: array_product int reducer ---");
    emitter.label_global(prefix);
    emitter.instruction("mov r10, rdx");                                        // copy the exclusive end before subtracting the start
    emitter.instruction("sub r10, rsi");                                        // count = end - start
    emitter.instruction(&format!("jle {empty_label}"));                         // empty or reversed ranges keep the seed value
    emitter.instruction("lea r9, [rdi + 24]");                                  // skip the indexed-array header to the payload base
    emitter.instruction("lea r9, [r9 + rsi * 8]");                              // address the first element in the requested range
    if raw_float_seed {
        emitter.instruction("movq xmm0, rcx");                                  // seed the double accumulator from the raw float bits
        emitter.instruction(&format!("jmp {float_loop}"));                      // float seeds skip the integer sequence
    } else {
        emitter.instruction("mov r8, rcx");                                     // seed the integer accumulator from the raw int seed

        emitter.label(&loop_label);
        emitter.instruction("mov rax, QWORD PTR [r9]");                         // load the next int element
        emitter.instruction("add r9, 8");                                       // advance the element cursor
        emitter.instruction("mov r11, r8");                                     // preserve the carry for the promoted float product
        emitter.instruction("imul r11, rax");                                   // compute the checked signed product
        emitter.instruction(&format!("jo {promote_label}"));                    // overflow promotes the carry to double
        emitter.instruction("mov r8, r11");                                     // commit the in-range integer product
        emitter.instruction("dec r10");                                         // one fewer element remains
        emitter.instruction(&format!("jnz {loop_label}"));                      // continue while the range is non-empty
        emitter.instruction(&format!("jmp {box_int}"));                         // box the completed integer product

        emitter.label(&promote_label);
        emitter.instruction("cvtsi2sd xmm0, r8");                               // convert the pre-overflow carry to double
        emitter.instruction("cvtsi2sd xmm1, rax");                              // convert the overflowing element to double
        emitter.instruction("mulsd xmm0, xmm1");                                // build PHP's promoted float product
        emitter.instruction("dec r10");                                         // the promoting element is consumed
        emitter.instruction(&format!("jz {box_float}"));                        // stop when no elements remain
    }

    emitter.label(&float_loop);
    emitter.instruction("mov rax, QWORD PTR [r9]");                             // load the next int element
    emitter.instruction("add r9, 8");                                           // advance the element cursor
    emitter.instruction("cvtsi2sd xmm1, rax");                                  // convert the element to double
    emitter.instruction("mulsd xmm0, xmm1");                                    // accumulate in PHP double mode
    emitter.instruction("dec r10");                                             // one fewer element remains
    emitter.instruction(&format!("jnz {float_loop}"));                          // continue while the range is non-empty

    emitter.label(&box_float);
    emitter.instruction("movq rdi, xmm0");                                      // float payload for the boxed result
    emitter.instruction("mov eax, 2");                                          // float Mixed tag for the result
    emitter.instruction("xor esi, esi");                                        // the high payload word is unused for scalars
    emitter.instruction("jmp __rt_mixed_from_value");                           // box the float product and return

    emitter.label(&box_int);
    emitter.instruction("xor eax, eax");                                        // int Mixed tag for the result
    emitter.instruction("mov rdi, r8");                                         // int payload for the result
    emitter.instruction("xor esi, esi");                                        // the high payload word is unused for scalars
    emitter.instruction("jmp __rt_mixed_from_value");                           // box the integer product and return

    emitter.label(&empty_label);
    if raw_float_seed {
        emitter.instruction("mov eax, 2");                                      // float Mixed tag for the empty product
        emitter.instruction("mov rdi, rcx");                                    // the raw float seed becomes the payload
        emitter.instruction("xor esi, esi");                                    // the high payload word is unused for scalars
    } else {
        emitter.instruction("xor eax, eax");                                    // int Mixed tag for the empty product
        emitter.instruction("mov rdi, rcx");                                    // the raw int seed becomes the payload
        emitter.instruction("xor esi, esi");                                    // the high payload word is unused for scalars
    }
    emitter.instruction("jmp __rt_mixed_from_value");                           // box the seed and return
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every supported target emits both seeded product entries and the checked
    /// int-overflow promotion path.
    #[test]
    fn array_product_int_emits_all_targets() {
        for target in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let mut emitter = Emitter::new(Target::parse(target).unwrap());
            emit_array_product_int(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_array_product_int_i64"), "{target}");
            assert!(asm.contains("__rt_array_product_float_i64"), "{target}");
            assert!(asm.contains("__rt_mixed_from_value"), "{target}");
            if target == "linux-x86_64" {
                assert!(asm.contains("imul r11, rax"), "{target}");
                assert!(asm.contains("jo __rt_array_product_int_i64_promote"), "{target}");
                assert!(asm.contains("mulsd xmm0, xmm1"), "{target}");
            } else {
                assert!(asm.contains("smulh x15, x11, x13"), "{target}");
                assert!(asm.contains("fmul d0, d0, d1"), "{target}");
                assert!(asm.contains("b.ne __rt_array_product_int_i64_promote"), "{target}");
            }
        }
    }
}
