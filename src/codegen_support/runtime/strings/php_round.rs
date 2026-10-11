//! Purpose:
//! Emits the `__rt_php_round` runtime helper: a faithful port of php-src's
//! `_php_math_round` (`ext/standard/math.c`, PHP-8.4 branch) restricted to the
//! `PHP_ROUND_HALF_UP` mode, the only mode elephc's `round()` builtin and
//! `number_format()` ever request.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//! - `crate::codegen::lower_inst::builtins::math::lower_round()` (the `round()` builtin).
//! - `crate::codegen_support::runtime::strings::number_format::emit_number_format()` (PHP's
//!   pre-rounding step before `"%.*F"` formatting).
//!
//! Key details:
//! - PHP's CRT-backed `%.Nf`/`round()` round half-to-even; PHP itself rounds half
//!   *away* from zero, with a pre-correction step that compensates for the binary
//!   representation error introduced by scaling by `10**places` (e.g.
//!   `0.285 * 1e10 == 2849999999.9999995`, whose `floor()` is one short). This
//!   helper reproduces both the correction and the half-away-from-zero edge-case
//!   test bit-for-bit against php-src, using hardware `floor`/`ceil` (`frintm`/
//!   `frintp` on AArch64, `roundsd` modes 1/2 on x86_64) and a single `pow()` call
//!   for the `10**abs(places)` exponent (no lookup-table fast path — correctness
//!   over the table's micro-optimization, and libm's `pow` is correctly rounded
//!   for exact integer powers in this range so the result is bit-identical).
//! - `copysign()` is inlined as sign-bit extraction + OR with the compile-time-known
//!   magnitude bits of `0.5`/`1.0` (no libm call needed); `fabs()` uses the native
//!   `fabs` instruction on AArch64 and the sign-mask-AND trick on x86_64, matching
//!   `crate::codegen::lower_inst::builtins::math::emit_float_abs`.
//! - Intentionally omits php-src's `abs(places) >= 23` string-round-trip branch
//!   (snprintf/strtod through a scratch buffer): that path only matters for
//!   precision requests beyond a double's ~15-17 significant decimal digits,
//!   which is already past the point where rounding is meaningful. Places in
//!   that range fall through to the same simple division/multiplication step
//!   used for `abs(places) < 23`, which is what php-src does through
//!   `pow()`-backed `php_intpow10` beyond its 22-entry table anyway.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_php_round` runtime helper (`_php_math_round`, `PHP_ROUND_HALF_UP`).
///
/// # Input
/// - ARM64: `d0` = value, `x1` = places (signed 64-bit precision)
/// - x86_64: `xmm0` = value, `rdi` = places (signed 64-bit precision)
///
/// # Output
/// - ARM64: `d0` = rounded value
/// - x86_64: `xmm0` = rounded value
///
/// # Behavior
/// Non-finite and zero inputs are returned unchanged. Otherwise: `exponent =
/// pow(10.0, abs(places))`; a floor/ceil pre-rounding step (chosen by the sign
/// of `value`) is corrected by one ULP-of-precision when scaling it back
/// exactly reproduces the input (php-src's representation-error fix-up); values
/// whose corrected magnitude is `>= 1e16` are returned unchanged (beyond double
/// precision, matching php-src); otherwise the `HALF_UP` edge case rounds the
/// corrected integral part away from zero when `|value|` reaches the scaled
/// midpoint, and the result is scaled back to the requested precision.
pub fn emit_php_round(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_php_round_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: php_round (_php_math_round port, HALF_UP mode) ---");
    emitter.label_global("__rt_php_round");

    // -- stack frame (128 bytes): value/places/exponent/tmp scratch + saved fp/lr --
    emitter.instruction("sub sp, sp, #128");                                    // allocate the php_round scratch frame
    emitter.instruction("stp x29, x30, [sp, #112]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #112");                                   // establish new frame pointer

    emitter.instruction("str d0, [sp, #0]");                                    // save the original value
    emitter.instruction("str x1, [sp, #8]");                                    // save places (pre-clamp)

    // -- non-finite check: exponent field == 0x7FF means Inf or NaN --
    emitter.instruction("fmov x9, d0");                                         // grab the raw bit pattern of the input double
    emitter.instruction("lsr x10, x9, #52");                                    // shift the exponent field into the low bits
    emitter.instruction("and x10, x10, #0x7ff");                                // isolate the 11-bit exponent
    emitter.instruction("cmp x10, #0x7ff");                                     // is the exponent all ones (Inf/NaN)?
    emitter.instruction("b.eq __rt_php_round_done");                            // non-finite: d0 already holds the unchanged input

    // -- zero check: _php_math_round returns +/-0.0 unchanged --
    emitter.instruction("fcmp d0, #0.0");                                       // compare the input against zero
    emitter.instruction("b.eq __rt_php_round_done");                            // zero: d0 already holds the unchanged input

    // -- clamp places to avoid abs() overflow at i64::MIN (mirrors PHP's INT_MIN+1 clamp) --
    emitter.instruction("mov x9, #1");                                          // build the i64::MIN sentinel via shift (avoids a 64-bit immediate load)
    emitter.instruction("lsl x9, x9, #63");                                     // x9 = i64::MIN (0x8000000000000000)
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload places
    emitter.instruction("cmp x10, x9");                                         // is places exactly i64::MIN?
    emitter.instruction("b.ne __rt_php_round_no_clamp");                        // skip the clamp for every other value
    emitter.instruction("add x10, x10, #1");                                    // clamp i64::MIN to i64::MIN+1 so -places cannot overflow
    emitter.label("__rt_php_round_no_clamp");
    emitter.instruction("str x10, [sp, #8]");                                   // store the clamped places

    // -- exponent = pow(10.0, (double)abs(places)) --
    emitter.instruction("cmp x10, #0");                                         // is places already non-negative?
    emitter.instruction("b.ge __rt_php_round_abs_pos");                         // non-negative places is its own magnitude
    emitter.instruction("neg x11, x10");                                        // magnitude of a negative places
    emitter.instruction("b __rt_php_round_abs_done");                           // continue with the absolute-value bit pattern
    emitter.label("__rt_php_round_abs_pos");
    emitter.instruction("mov x11, x10");                                        // copy the input bits before clearing their sign
    emitter.label("__rt_php_round_abs_done");
    emitter.instruction("scvtf d1, x11");                                       // abs(places) as a double exponent for pow()
    emitter.instruction("fmov d0, #10.0");                                      // base 10.0 for the power computation
    emitter.emit_call_c("pow");                                                        // exponent = 10.0 ** abs(places)
    emitter.instruction("str d0, [sp, #16]");                                   // save the exponent for reuse across every branch below

    // -- scaled = places>0 ? value*exponent : value/exponent --
    emitter.instruction("ldr d0, [sp, #0]");                                    // reload the original value
    emitter.instruction("ldr d2, [sp, #16]");                                   // reload the exponent
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the clamped places
    emitter.instruction("cmp x10, #0");                                         // choose multiplication or division from the place sign
    emitter.instruction("b.le __rt_php_round_scale_div");                       // non-positive places scale by division
    emitter.instruction("fmul d3, d0, d2");                                     // scaled = value * exponent
    emitter.instruction("b __rt_php_round_scale_done");                         // skip the division scaling path
    emitter.label("__rt_php_round_scale_div");
    emitter.instruction("fdiv d3, d0, d2");                                     // scaled = value / exponent
    emitter.label("__rt_php_round_scale_done");

    // -- tmp_value/tmp_value2: floor+1 for non-negative values, ceil-1 for negative --
    emitter.instruction("fcmp d0, #0.0");                                       // branch on the sign of the ORIGINAL value
    emitter.instruction("b.lt __rt_php_round_neg_branch");                      // use the negative-value HALF_UP branch below zero
    emitter.instruction("frintm d4, d3");                                       // tmp_value = floor(scaled)
    emitter.instruction("fmov d5, #1.0");                                       // materialize the positive correction unit
    emitter.instruction("fadd d5, d4, d5");                                     // tmp_value2 = tmp_value + 1.0
    emitter.instruction("b __rt_php_round_tmp_done");                           // skip construction of the negative correction unit
    emitter.label("__rt_php_round_neg_branch");
    emitter.instruction("frintp d4, d3");                                       // tmp_value = ceil(scaled)
    emitter.instruction("fmov d5, #1.0");                                       // materialize the correction unit before negation
    emitter.instruction("fsub d5, d4, d5");                                     // tmp_value2 = tmp_value - 1.0
    emitter.label("__rt_php_round_tmp_done");
    emitter.instruction("str d4, [sp, #24]");                                   // save tmp_value
    emitter.instruction("str d5, [sp, #32]");                                   // save tmp_value2

    // -- representation-error correction: adopt tmp_value2 when it round-trips exactly --
    emitter.instruction("cmp x10, #0");                                         // select how the rounded integer is rescaled for checking
    emitter.instruction("b.le __rt_php_round_check_mul");                       // non-positive places restore scale by multiplication
    emitter.instruction("fdiv d6, d5, d2");                                     // check = tmp_value2 / exponent
    emitter.instruction("b __rt_php_round_check_done");                         // skip the multiplication check path
    emitter.label("__rt_php_round_check_mul");
    emitter.instruction("fmul d6, d5, d2");                                     // check = tmp_value2 * exponent
    emitter.label("__rt_php_round_check_done");
    emitter.instruction("fcmp d6, d0");                                         // does the round-trip reproduce the original value exactly?
    emitter.instruction("b.ne __rt_php_round_no_correction");                   // bypass edge correction when scaling changed the value
    emitter.instruction("str d5, [sp, #24]");                                   // correction applies: tmp_value = tmp_value2
    emitter.label("__rt_php_round_no_correction");

    // -- values beyond double precision (fabs(tmp_value) >= 1e16) are returned unchanged --
    emitter.instruction("ldr d4, [sp, #24]");                                   // reload the (possibly corrected) tmp_value
    emitter.instruction("fabs d6, d4");                                         // compare the rounded magnitude without its sign
    emit_load_1e16_aarch64(emitter, "x11");
    emitter.instruction("fmov d7, x11");                                        // d7 = 1e16
    emitter.instruction("fcmp d6, d7");                                         // test the rounded magnitude against the correction edge
    emitter.instruction("b.lt __rt_php_round_helper");                          // < 1e16: continue to the HALF_UP edge-case test
    emitter.instruction("ldr d0, [sp, #0]");                                    // >= 1e16: restore and return the original value unchanged
    emitter.instruction("b __rt_php_round_done");                               // return the corrected rounded value directly

    // -- HALF_UP edge case: round the integral part away from zero at the midpoint --
    emitter.label("__rt_php_round_helper");
    emitter.instruction("mov x9, #1");                                          // seed the positive HALF_UP correction integer
    emitter.instruction("lsl x9, x9, #63");                                     // sign-bit mask
    emitter.instruction("fmov x12, d4");                                        // bits of tmp_value
    emitter.instruction("and x12, x12, x9");                                    // isolate the sign bit of tmp_value
    emitter.instruction("movz x13, #0x3fe0, lsl #48");                          // magnitude bits of 0.5
    emitter.instruction("orr x13, x12, x13");                                   // x13 = copysign(0.5, tmp_value) bits
    emitter.instruction("fmov d5, x13");                                        // reinterpret the signed correction bits as a double
    emitter.instruction("fadd d5, d4, d5");                                     // tmp_value + copysign(0.5, tmp_value)
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the clamped places
    emitter.instruction("ldr d2, [sp, #16]");                                   // reload the exponent
    emitter.instruction("cmp x10, #0");                                         // select the place-dependent correction rescaling path
    emitter.instruction("b.le __rt_php_round_edge_mul");                        // non-positive places scale the correction by multiplication
    emitter.instruction("fdiv d6, d5, d2");                                     // shrink the correction for positive decimal places
    emitter.instruction("b __rt_php_round_edge_done");                          // skip the multiplication rescaling path
    emitter.label("__rt_php_round_edge_mul");
    emitter.instruction("fmul d6, d5, d2");                                     // enlarge the correction for non-positive places
    emitter.label("__rt_php_round_edge_done");
    emitter.instruction("fabs d6, d6");                                         // edge_case
    emitter.instruction("ldr d0, [sp, #0]");                                    // reload the original value
    emitter.instruction("fabs d1, d0");                                         // value_abs
    emitter.instruction("fcmp d1, d6");                                         // value_abs vs edge_case
    emitter.instruction("b.lt __rt_php_round_no_round_up");                     // value_abs < edge_case: keep tmp_value as-is

    emitter.instruction("mov x9, #1");                                          // seed the positive fallback correction integer
    emitter.instruction("lsl x9, x9, #63");                                     // sign-bit mask
    emitter.instruction("fmov x12, d4");                                        // bits of tmp_value
    emitter.instruction("and x12, x12, x9");                                    // isolate the sign bit of tmp_value
    emitter.instruction("movz x13, #0x3ff0, lsl #48");                          // magnitude bits of 1.0
    emitter.instruction("orr x13, x12, x13");                                   // x13 = copysign(1.0, tmp_value) bits
    emitter.instruction("fmov d5, x13");                                        // reinterpret the signed fallback correction as a double
    emitter.instruction("fadd d4, d4, d5");                                     // round away from zero
    emitter.label("__rt_php_round_no_round_up");

    // -- final: scale tmp_value back to the requested precision --
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the requested decimal-place count
    emitter.instruction("ldr d2, [sp, #16]");                                   // reload the decimal scaling factor
    emitter.instruction("cmp x10, #0");                                         // select the final inverse-scaling operation
    emitter.instruction("b.le __rt_php_round_final_mul");                       // non-positive places restore scale by multiplication
    emitter.instruction("fdiv d0, d4, d2");                                     // restore positive-place scaling by division
    emitter.instruction("b __rt_php_round_done");                               // skip the multiplication result path
    emitter.label("__rt_php_round_final_mul");
    emitter.instruction("fmul d0, d4, d2");                                     // restore non-positive-place scaling by multiplication

    emitter.label("__rt_php_round_done");
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // deallocate stack frame
    emitter.instruction("ret");                                                 // return the PHP-compatible rounded double
}

/// Materializes the IEEE-754 bit pattern of `1e16` into `reg` via `movz`/`movk`
/// (the ARM64 `fmov` immediate encoding cannot represent it directly).
fn emit_load_1e16_aarch64(emitter: &mut Emitter, reg: &str) {
    emitter.instruction(&format!("movz {}, #0x8000", reg));                     // low 16 bits of 1e16 (0x4341C37937E08000)
    emitter.instruction(&format!("movk {}, #0x37e0, lsl #16", reg));            // next 16 bits
    emitter.instruction(&format!("movk {}, #0xc379, lsl #32", reg));            // next 16 bits
    emitter.instruction(&format!("movk {}, #0x4341, lsl #48", reg));            // top 16 bits
}

/// Emits the x86_64 variant of `__rt_php_round` (Linux, macOS, and windows-x86_64).
/// Mirrors [`emit_php_round`]'s AArch64 body exactly; see that function's doc
/// comment for the algorithm. Every `pow()` call routes through
/// [`Emitter::emit_call_c`] so windows-x86_64 reaches the registered
/// `__rt_sys_pow` msvcrt shim instead of a raw SysV-staged import call.
fn emit_php_round_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: php_round (_php_math_round port, HALF_UP mode) ---");
    emitter.label_global("__rt_php_round");

    emitter.instruction("push rbp");                                            // save the caller frame pointer before using stack locals
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the php_round scratch slots
    emitter.instruction("sub rsp, 64");                                         // reserve scratch (value/places/exponent/tmp_value/tmp_value2), 16-byte aligned

    emitter.instruction("movsd QWORD PTR [rbp - 8], xmm0");                     // save the original value
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // save places (pre-clamp)

    // -- non-finite check: exponent field == 0x7FF means Inf or NaN --
    emitter.instruction("movq r9, xmm0");                                       // grab the raw bit pattern of the input double
    emitter.instruction("mov r10, r9");                                         // copy the bits for exponent extraction
    emitter.instruction("shr r10, 52");                                         // shift the exponent field into the low bits
    emitter.instruction("and r10, 0x7ff");                                      // isolate the 11-bit exponent
    emitter.instruction("cmp r10, 0x7ff");                                      // is the exponent all ones (Inf/NaN)?
    emitter.instruction("je __rt_php_round_done_x");                            // non-finite: xmm0 already holds the unchanged input

    // -- zero check: _php_math_round returns +/-0.0 unchanged --
    emitter.instruction("xorpd xmm1, xmm1");                                    // materialize a canonical 0.0 comparison operand
    emitter.instruction("ucomisd xmm0, xmm1");                                  // compare the input against zero
    emitter.instruction("je __rt_php_round_done_x");                            // zero: xmm0 already holds the unchanged input

    // -- clamp places to avoid abs() overflow at i64::MIN (mirrors PHP's INT_MIN+1 clamp) --
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload places
    emitter.instruction("mov r10, 0x8000000000000000");                         // i64::MIN sentinel
    emitter.instruction("cmp r9, r10");                                         // is places exactly i64::MIN?
    emitter.instruction("jne __rt_php_round_no_clamp_x");                       // skip the clamp for every other value
    emitter.instruction("add r9, 1");                                           // clamp i64::MIN to i64::MIN+1 so -places cannot overflow
    emitter.label("__rt_php_round_no_clamp_x");
    emitter.instruction("mov QWORD PTR [rbp - 16], r9");                        // store the clamped places

    // -- exponent = pow(10.0, (double)abs(places)) --
    emitter.instruction("cmp r9, 0");                                           // is places already non-negative?
    emitter.instruction("jge __rt_php_round_abs_pos_x");                        // non-negative places is its own magnitude
    emitter.instruction("mov r10, r9");                                         // copy the input bits before clearing their sign
    emitter.instruction("neg r10");                                             // magnitude of a negative places
    emitter.instruction("jmp __rt_php_round_abs_done_x");                       // continue with the absolute-value bit pattern
    emitter.label("__rt_php_round_abs_pos_x");
    emitter.instruction("mov r10, r9");                                         // copy negative input bits before clearing their sign
    emitter.label("__rt_php_round_abs_done_x");
    emitter.instruction("cvtsi2sd xmm1, r10");                                  // abs(places) as a double exponent for pow()
    emitter.instruction("mov rax, 0x4024000000000000");                         // IEEE-754 payload for 10.0
    emitter.instruction("movq xmm0, rax");                                      // base 10.0 for the power computation
    emitter.emit_call_c("pow");                                                // exponent = 10.0 ** abs(places)
    emitter.instruction("movsd QWORD PTR [rbp - 24], xmm0");                    // save the exponent for reuse across every branch below

    // -- scaled = places>0 ? value*exponent : value/exponent --
    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 8]");                     // reload the original value
    emitter.instruction("movsd xmm2, QWORD PTR [rbp - 24]");                    // reload the exponent
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the clamped places
    emitter.instruction("cmp r10, 0");                                          // choose multiplication or division from the place sign
    emitter.instruction("jle __rt_php_round_scale_div_x");                      // non-positive places scale by division
    emitter.instruction("movsd xmm3, xmm0");                                    // copy the absolute input before multiplying its scale
    emitter.instruction("mulsd xmm3, xmm2");                                    // scaled = value * exponent
    emitter.instruction("jmp __rt_php_round_scale_done_x");                     // skip the division scaling path
    emitter.label("__rt_php_round_scale_div_x");
    emitter.instruction("movsd xmm3, xmm0");                                    // copy the absolute input before dividing its scale
    emitter.instruction("divsd xmm3, xmm2");                                    // scaled = value / exponent
    emitter.label("__rt_php_round_scale_done_x");

    // -- tmp_value/tmp_value2: floor+1 for non-negative values, ceil-1 for negative --
    emitter.instruction("xorpd xmm4, xmm4");                                    // canonical 0.0 for the sign branch
    emitter.instruction("ucomisd xmm0, xmm4");                                  // branch on the sign of the ORIGINAL value
    emitter.instruction("jb __rt_php_round_neg_branch_x");                      // use the negative-value HALF_UP branch below zero
    emitter.instruction("roundsd xmm4, xmm3, 1");                               // tmp_value = floor(scaled)
    emitter.instruction("mov rax, 0x3ff0000000000000");                         // IEEE-754 payload for 1.0
    emitter.instruction("movq xmm5, rax");                                      // materialize the positive correction unit
    emitter.instruction("movsd xmm6, xmm4");                                    // copy the scaled positive magnitude for adjustment
    emitter.instruction("addsd xmm6, xmm5");                                    // tmp_value2 = tmp_value + 1.0
    emitter.instruction("jmp __rt_php_round_tmp_done_x");                       // skip construction of the negative correction unit
    emitter.label("__rt_php_round_neg_branch_x");
    emitter.instruction("roundsd xmm4, xmm3, 2");                               // tmp_value = ceil(scaled)
    emitter.instruction("mov rax, 0x3ff0000000000000");                         // IEEE-754 payload for 1.0
    emitter.instruction("movq xmm5, rax");                                      // materialize the negative correction unit
    emitter.instruction("movsd xmm6, xmm4");                                    // copy the scaled negative magnitude for adjustment
    emitter.instruction("subsd xmm6, xmm5");                                    // tmp_value2 = tmp_value - 1.0
    emitter.label("__rt_php_round_tmp_done_x");
    emitter.instruction("movsd QWORD PTR [rbp - 32], xmm4");                    // save tmp_value
    emitter.instruction("movsd QWORD PTR [rbp - 40], xmm6");                    // save tmp_value2

    // -- representation-error correction: adopt tmp_value2 when it round-trips exactly --
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the clamped places
    emitter.instruction("movsd xmm2, QWORD PTR [rbp - 24]");                    // reload the exponent
    emitter.instruction("cmp r10, 0");                                          // select how the rounded integer is rescaled for checking
    emitter.instruction("jle __rt_php_round_check_mul_x");                      // non-positive places restore scale by multiplication
    emitter.instruction("movsd xmm7, xmm6");                                    // copy the rounded integer for division rescaling
    emitter.instruction("divsd xmm7, xmm2");                                    // check = tmp_value2 / exponent
    emitter.instruction("jmp __rt_php_round_check_done_x");                     // skip the multiplication check path
    emitter.label("__rt_php_round_check_mul_x");
    emitter.instruction("movsd xmm7, xmm6");                                    // copy the rounded integer for multiplication rescaling
    emitter.instruction("mulsd xmm7, xmm2");                                    // check = tmp_value2 * exponent
    emitter.label("__rt_php_round_check_done_x");
    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 8]");                     // reload the original value for the comparison
    emitter.instruction("ucomisd xmm7, xmm0");                                  // does the round-trip reproduce the original value exactly?
    emitter.instruction("jne __rt_php_round_no_correction_x");                  // bypass edge correction when scaling changed the value
    emitter.instruction("movsd QWORD PTR [rbp - 32], xmm6");                    // correction applies: tmp_value = tmp_value2
    emitter.label("__rt_php_round_no_correction_x");

    // -- values beyond double precision (fabs(tmp_value) >= 1e16) are returned unchanged --
    emitter.instruction("movsd xmm4, QWORD PTR [rbp - 32]");                    // reload the (possibly corrected) tmp_value
    emitter.instruction("movq r10, xmm4");                                      // extract rounded bits for absolute-value comparison
    emitter.instruction("mov r11, 0x7fffffffffffffff");                         // fabs mask
    emitter.instruction("and r10, r11");                                        // clear the rounded value's sign bit
    emitter.instruction("movq xmm6, r10");                                      // xmm6 = fabs(tmp_value)
    emitter.instruction("mov rax, 0x4341c37937e08000");                         // IEEE-754 payload for 1e16
    emitter.instruction("movq xmm7, rax");                                      // materialize the magnitude correction threshold
    emitter.instruction("ucomisd xmm6, xmm7");                                  // compare the rounded magnitude with the threshold
    emitter.instruction("jb __rt_php_round_helper_x");                          // < 1e16: continue to the HALF_UP edge-case test
    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 8]");                     // >= 1e16: restore and return the original value unchanged
    emitter.instruction("jmp __rt_php_round_done_x");                           // return the corrected rounded value directly

    // -- HALF_UP edge case: round the integral part away from zero at the midpoint --
    emitter.label("__rt_php_round_helper_x");
    emitter.instruction("movq r9, xmm4");                                       // bits of tmp_value
    emitter.instruction("mov r10, 0x8000000000000000");                         // sign-bit mask
    emitter.instruction("and r9, r10");                                         // isolate the sign bit of tmp_value
    emitter.instruction("mov r11, 0x3fe0000000000000");                         // magnitude bits of 0.5
    emitter.instruction("or r9, r11");                                          // r9 = copysign(0.5, tmp_value) bits
    emitter.instruction("movq xmm5, r9");                                       // materialize the signed edge correction unit
    emitter.instruction("movsd xmm6, xmm4");                                    // copy the scaled magnitude for edge adjustment
    emitter.instruction("addsd xmm6, xmm5");                                    // tmp_value + copysign(0.5, tmp_value)
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the clamped places
    emitter.instruction("movsd xmm2, QWORD PTR [rbp - 24]");                    // reload the exponent
    emitter.instruction("cmp r10, 0");                                          // select the place-dependent correction rescaling path
    emitter.instruction("jle __rt_php_round_edge_mul_x");                       // non-positive places scale the correction by multiplication
    emitter.instruction("divsd xmm6, xmm2");                                    // shrink the correction for positive decimal places
    emitter.instruction("jmp __rt_php_round_edge_done_x");                      // skip the multiplication rescaling path
    emitter.label("__rt_php_round_edge_mul_x");
    emitter.instruction("mulsd xmm6, xmm2");                                    // enlarge the correction for non-positive places
    emitter.label("__rt_php_round_edge_done_x");
    emitter.instruction("movq r10, xmm6");                                      // extract correction bits for an absolute-value comparison
    emitter.instruction("mov r11, 0x7fffffffffffffff");                         // fabs mask
    emitter.instruction("and r10, r11");                                        // clear the correction value's sign bit
    emitter.instruction("movq xmm6, r10");                                      // edge_case = fabs(...)
    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 8]");                     // reload the original value
    emitter.instruction("movq r10, xmm0");                                      // extract input bits for an absolute-value comparison
    emitter.instruction("and r10, r11");                                        // clear the original input's sign bit
    emitter.instruction("movq xmm1, r10");                                      // value_abs = fabs(value)
    emitter.instruction("ucomisd xmm1, xmm6");                                  // value_abs vs edge_case
    emitter.instruction("jb __rt_php_round_no_round_up_x");                     // value_abs < edge_case: keep tmp_value as-is

    emitter.instruction("movq r9, xmm4");                                       // bits of tmp_value
    emitter.instruction("mov r10, 0x8000000000000000");                         // sign-bit mask
    emitter.instruction("and r9, r10");                                         // isolate the sign bit of tmp_value
    emitter.instruction("mov r11, 0x3ff0000000000000");                         // magnitude bits of 1.0
    emitter.instruction("or r9, r11");                                          // r9 = copysign(1.0, tmp_value) bits
    emitter.instruction("movq xmm5, r9");                                       // materialize the signed fallback correction unit
    emitter.instruction("addsd xmm4, xmm5");                                    // round away from zero
    emitter.label("__rt_php_round_no_round_up_x");

    // -- final: scale tmp_value back to the requested precision --
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the requested decimal-place count
    emitter.instruction("movsd xmm2, QWORD PTR [rbp - 24]");                    // reload the decimal scaling factor
    emitter.instruction("cmp r10, 0");                                          // select the final inverse-scaling operation
    emitter.instruction("jle __rt_php_round_final_mul_x");                      // non-positive places restore scale by multiplication
    emitter.instruction("movsd xmm0, xmm4");                                    // copy the rounded value for division rescaling
    emitter.instruction("divsd xmm0, xmm2");                                    // restore positive-place scaling by division
    emitter.instruction("jmp __rt_php_round_done_x");                           // skip the multiplication result path
    emitter.label("__rt_php_round_final_mul_x");
    emitter.instruction("movsd xmm0, xmm4");                                    // copy the rounded value for multiplication rescaling
    emitter.instruction("mulsd xmm0, xmm2");                                    // restore non-positive-place scaling by multiplication

    emitter.label("__rt_php_round_done_x");
    emitter.instruction("add rsp, 64");                                         // release the local scratch area before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the rounded value in xmm0
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::platform::{Arch, Platform, Target};

    use super::*;

    /// Verifies the shared label and the AArch64 non-finite/zero early-return
    /// checks (mirroring `__rt_ftoa`'s proven exponent-field test) are present.
    #[test]
    fn test_emit_php_round_aarch64_early_returns() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_php_round(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains("__rt_php_round:\n"));
        assert!(asm.contains("and x10, x10, #0x7ff\n"));
        assert!(asm.contains("fcmp d0, #0.0\n"));
        assert!(asm.contains("bl _pow\n"));
    }

    /// Verifies the HALF_UP edge-case copysign bit tricks and the 1e16 overflow
    /// guard are present on AArch64 (no libm `copysign`/`fabs` calls).
    #[test]
    fn test_emit_php_round_aarch64_uses_bit_tricks_not_libm_copysign() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_php_round(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains("movz x13, #0x3fe0, lsl #48\n"));
        assert!(asm.contains("movz x13, #0x3ff0, lsl #48\n"));
        assert!(!asm.contains("bl _copysign\n"));
        assert!(!asm.contains("bl _fabs\n"));
    }

    /// Verifies x86_64 routes `pow()` through `emit_call_c` (Windows-shim-safe)
    /// and uses the `roundsd` floor/ceil immediates already proven by
    /// `lower_floor`/`lower_ceil`.
    #[test]
    fn test_emit_php_round_x86_64_uses_roundsd_and_call_c_pow() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_php_round(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains("__rt_php_round:\n"));
        assert!(asm.contains("roundsd xmm4, xmm3, 1\n"));
        assert!(asm.contains("roundsd xmm4, xmm3, 2\n"));
        assert!(asm.contains("call pow\n"));
    }

    /// Regression guard: windows-x86_64 must route `pow()` through the
    /// registered `__rt_sys_pow` shim, not a bare `call pow` (the Class-1
    /// SysV/MSx64 ABI bug this codebase has hit before).
    #[test]
    fn test_emit_php_round_windows_x86_64_routes_pow_through_shim() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_php_round(&mut emitter);
        let asm = emitter.output();

        assert!(!asm.contains("call pow\n"));
        assert!(asm.contains("call __rt_sys_pow\n"));
    }
}
