//! Purpose:
//! Emits `__rt_round_mode`, the shared runtime implementation of PHP's
//! `round($num, $precision, $mode)` for every rounding mode and every supported target.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()`.
//! - `crate::codegen::lower_inst::builtins::round_mode::lower_round_with_mode()`.
//!
//! Key details:
//! - The routine is a line-by-line port of php-src 8.4 `_php_math_round()` +
//!   `php_round_helper()` (`ext/standard/math.c`), including the integral-part correction that
//!   makes `round(1.005, 2)` answer `1.01` instead of `1.0`. Keeping the same structure is the
//!   only way the tie-breaking modes agree with PHP on values such as `0.285` where the binary
//!   double sits just below the decimal tie.
//! - Rounding modes are the php-src integers: 1 `HALF_UP`, 2 `HALF_DOWN`, 3 `HALF_EVEN`,
//!   4 `HALF_ODD`, 5 `CEILING`, 6 `FLOOR`, 7 `TOWARD_ZERO`, 8 `AWAY_FROM_ZERO`. The caller
//!   validates the range and raises PHP's `ValueError`, so this helper trusts `1..=8`.
//! - DIVERGENCE (documented): php-src re-materializes the result through
//!   `snprintf()` + `zend_strtod()` when `abs($precision) >= 23`. This helper always uses the
//!   plain multiply/divide, which can differ by one ULP for those absurd precisions. The
//!   `integral == 0` early return keeps the `0.0 * INF` case from producing `NAN`.
//! - ABI: AArch64 takes the value in `d0`, `$precision` in `x0`, `$mode` in `x1` and returns in
//!   `d0`; x86_64 takes `xmm0`, `rdi`, `rsi` and returns in `xmm0`. `pow()` is the only libc
//!   call and only for `abs($precision) > 22`, so the frame is set up unconditionally.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// IEEE-754 payload of `1e16`, php-src's "beyond our precision" cutoff in `_php_math_round()`.
const ONE_E16_BITS: u64 = 0x4341_C379_37E0_8000;

/// IEEE-754 payload of `1.0`, used to build `copysign(1.0, integral)`.
const ONE_BITS: u64 = 0x3FF0_0000_0000_0000;

/// IEEE-754 payload of `0.5`, used to build `copysign(0.5, integral)`.
const HALF_BITS: u64 = 0x3FE0_0000_0000_0000;

/// IEEE-754 payload of `10.0`, the base of the decimal precision exponent.
const TEN_BITS: u64 = 0x4024_0000_0000_0000;

/// Emits the `__rt_round_mode` runtime helper for the active target.
///
/// # Input
/// - AArch64: `d0` = `$num`, `x0` = `$precision`, `x1` = `$mode`.
/// - x86_64: `xmm0` = `$num`, `rdi` = `$precision`, `rsi` = `$mode`.
///
/// # Output
/// - The rounded double in `d0` / `xmm0`.
///
/// # Clobbers
/// - Caller-saved integer and floating-point registers, exactly like any other `__rt_*` call.
pub fn emit_round_mode(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_round_mode_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: round_mode ---");
    emitter.label_global("__rt_round_mode");

    // Frame layout:
    //   [sp, #0]  = $num
    //   [sp, #16] = $precision
    //   [sp, #24] = $mode
    //   [sp, #48] = saved x29/x30
    emitter.instruction("sub sp, sp, #64");                                     // allocate the round-mode frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up the round-mode frame pointer
    emitter.instruction("str d0, [sp, #0]");                                    // spill $num across the optional pow() call
    emitter.instruction("str x0, [sp, #16]");                                   // spill $precision across the optional pow() call
    emitter.instruction("str x1, [sp, #24]");                                   // spill $mode across the optional pow() call

    // -- php-src returns non-finite and zero inputs untouched --
    emitter.instruction("fmov x9, d0");                                         // raw IEEE-754 payload of $num
    emitter.instruction("lsl x10, x9, #1");                                     // drop the sign bit so +0.0 and -0.0 collapse
    emitter.instruction("cbz x10, __rt_round_mode_return_value");               // PHP returns +/-0.0 unchanged
    emitter.instruction("movz x11, #0xffe0, lsl #48");                          // smallest sign-stripped payload with an all-ones exponent
    emitter.instruction("cmp x10, x11");                                        // is the exponent field saturated (INF or NAN)?
    emitter.instruction("b.hs __rt_round_mode_return_value");                   // PHP returns INF/NAN unchanged

    // -- exponent = php_intpow10(abs($precision)) --
    emitter.instruction("ldr x0, [sp, #16]");                                   // x0 = $precision
    emitter.instruction("cmp x0, #0");                                          // is the requested precision negative?
    emitter.instruction("cneg x0, x0, lt");                                     // x0 = abs($precision)
    emitter.instruction("cmp x0, #22");                                         // 10^0..10^22 are exactly representable doubles
    emitter.instruction("b.gt __rt_round_mode_pow_call");                       // anything larger needs libc pow()
    emitter.instruction("fmov d1, #1.0");                                       // start the exact power-of-ten accumulator at 1.0
    emitter.instruction("fmov d2, #10.0");                                      // the decimal base multiplied in per requested place
    emitter.label("__rt_round_mode_pow_loop");
    emitter.instruction("cbz x0, __rt_round_mode_pow_done");                    // every requested decimal place has been applied
    emitter.instruction("fmul d1, d1, d2");                                     // multiply in one exact decimal place
    emitter.instruction("sub x0, x0, #1");                                      // one decimal place consumed
    emitter.instruction("b __rt_round_mode_pow_loop");                          // keep accumulating the exact power of ten
    emitter.label("__rt_round_mode_pow_call");
    emitter.instruction("scvtf d1, x0");                                        // convert abs($precision) into pow()'s exponent argument
    emitter.instruction("fmov d0, #10.0");                                      // pow() base 10.0
    emitter.emit_call_c("pow");
    emitter.instruction("fmov d1, d0");                                         // move the computed exponent into the shared register
    emitter.label("__rt_round_mode_pow_done");

    // -- scale $num into the integral domain of the requested precision --
    emitter.instruction("ldr d0, [sp, #0]");                                    // d0 = $num
    emitter.instruction("ldr x9, [sp, #16]");                                   // x9 = $precision, live for every later branch
    emitter.instruction("cmp x9, #0");                                          // php-src branches on `places > 0` everywhere
    emitter.instruction("b.gt __rt_round_mode_scale_mul");                      // positive precision multiplies by the exponent
    emitter.instruction("fdiv d2, d0, d1");                                     // scale by dividing for zero/negative precision
    emitter.instruction("b __rt_round_mode_scale_done");                        // the scaled value is ready
    emitter.label("__rt_round_mode_scale_mul");
    emitter.instruction("fmul d2, d0, d1");                                     // scale by multiplying for positive precision

    // -- extract the integral part and php-src's off-by-one-ULP correction candidate --
    emitter.label("__rt_round_mode_scale_done");
    emitter.instruction("fcmp d0, #0.0");                                       // php-src splits on the sign of the original value
    emitter.instruction("b.mi __rt_round_mode_negative");                       // negative values take the ceil() branch
    emitter.instruction("frintm d3, d2");                                       // integral = floor(scaled)
    emitter.instruction("fmov d4, #1.0");                                       // the correction candidate is one step away
    emitter.instruction("fadd d4, d3, d4");                                     // candidate = integral + 1.0
    emitter.instruction("b __rt_round_mode_correct");                           // test whether the candidate rebuilds $num exactly
    emitter.label("__rt_round_mode_negative");
    emitter.instruction("frintp d3, d2");                                       // integral = ceil(scaled)
    emitter.instruction("fmov d4, #1.0");                                       // the correction candidate is one step away
    emitter.instruction("fsub d4, d3, d4");                                     // candidate = integral - 1.0

    // -- adopt the candidate when unscaling it reproduces $num bit-for-bit --
    emitter.label("__rt_round_mode_correct");
    emitter.instruction("cmp x9, #0");                                          // unscale the candidate the same way it was scaled
    emitter.instruction("b.gt __rt_round_mode_back_div");                       // positive precision unscales by dividing
    emitter.instruction("fmul d5, d4, d1");                                     // unscale the candidate by multiplying
    emitter.instruction("b __rt_round_mode_back_done");                         // the unscaled candidate is ready
    emitter.label("__rt_round_mode_back_div");
    emitter.instruction("fdiv d5, d4, d1");                                     // unscale the candidate by dividing
    emitter.label("__rt_round_mode_back_done");
    emitter.instruction("fcmp d5, d0");                                         // did the candidate rebuild $num exactly?
    emitter.instruction("fcsel d3, d4, d3, eq");                                // adopt the corrected integral part when it did

    // -- values past the double precision limit are returned untouched --
    emitter.instruction("fabs d6, d3");                                         // magnitude of the integral part
    emitter.instruction("movz x10, #0x4341, lsl #48");                          // build 1e16, php-src's precision cutoff
    emitter.instruction(&format!("movk x10, #0x{:04x}, lsl #32", (ONE_E16_BITS >> 32) & 0xFFFF)); // second halfword of 1e16
    emitter.instruction(&format!("movk x10, #0x{:04x}, lsl #16", (ONE_E16_BITS >> 16) & 0xFFFF)); // third halfword of 1e16
    emitter.instruction(&format!("movk x10, #0x{:04x}", ONE_E16_BITS & 0xFFFF)); // low halfword of 1e16
    emitter.instruction("fmov d7, x10");                                        // d7 = 1e16
    emitter.instruction("fcmp d6, d7");                                         // is the integral part beyond double precision?
    emitter.instruction("b.ge __rt_round_mode_return_value");                   // yes - php-src returns $num unchanged

    // -- php_round_helper(): dispatch on the requested rounding mode --
    emitter.instruction("fabs d6, d0");                                         // d6 = fabs($num), php-src's `value_abs`
    emitter.instruction("ldr x11, [sp, #24]");                                  // x11 = $mode
    emitter.instruction("cmp x11, #7");                                         // mode 7 = TOWARD_ZERO
    emitter.instruction("b.eq __rt_round_mode_finish");                         // truncation keeps the integral part as-is
    emitter.instruction("fmov x10, d3");                                        // raw payload of the integral part
    emitter.instruction("and x10, x10, #0x8000000000000000");                   // isolate its sign bit for the copysign() builds
    emitter.instruction(&format!("movz x12, #0x{:04x}, lsl #48", ONE_BITS >> 48)); // high halfword of 1.0
    emitter.instruction("orr x12, x10, x12");                                   // copysign(1.0, integral)
    emitter.instruction("fmov d7, x12");                                        // d7 = the magnitude step php-src adds
    emitter.instruction("cmp x11, #5");                                         // modes 5, 6 and 8 use the zero edge case
    emitter.instruction("b.ge __rt_round_mode_zero_edge");                      // directional modes skip the half-way edge case

    // -- php_round_get_basic_edge_case(): the exact half-way point of this integral step --
    emitter.instruction(&format!("movz x12, #0x{:04x}, lsl #48", HALF_BITS >> 48)); // high halfword of 0.5
    emitter.instruction("orr x12, x10, x12");                                   // copysign(0.5, integral)
    emitter.instruction("fmov d2, x12");                                        // d2 = the half-way offset
    emitter.instruction("fadd d2, d3, d2");                                     // integral + copysign(0.5, integral)
    emitter.instruction("cmp x9, #0");                                          // unscale the edge case like php-src does
    emitter.instruction("b.gt __rt_round_mode_edge_div");                       // positive precision unscales by dividing
    emitter.instruction("fmul d2, d2, d1");                                     // unscale the edge case by multiplying
    emitter.instruction("b __rt_round_mode_edge_done");                         // the edge case is ready
    emitter.label("__rt_round_mode_edge_div");
    emitter.instruction("fdiv d2, d2, d1");                                     // unscale the edge case by dividing
    emitter.label("__rt_round_mode_edge_done");
    emitter.instruction("fabs d2, d2");                                         // php-src compares magnitudes only
    emitter.instruction("cmp x11, #1");                                         // mode 1 = HALF_UP
    emitter.instruction("b.eq __rt_round_mode_half_up");                        // ties move away from zero
    emitter.instruction("cmp x11, #2");                                         // mode 2 = HALF_DOWN
    emitter.instruction("b.eq __rt_round_mode_half_down");                      // ties move toward zero
    emitter.instruction("cmp x11, #3");                                         // mode 3 = HALF_EVEN
    emitter.instruction("b.eq __rt_round_mode_half_even");                      // ties move to the even neighbour
    emitter.instruction("b __rt_round_mode_half_odd");                          // mode 4 = HALF_ODD

    emitter.label("__rt_round_mode_half_up");
    emitter.instruction("fcmp d6, d2");                                         // compare $num against the half-way point
    emitter.instruction("b.ge __rt_round_mode_bump");                           // ties and everything above round away from zero
    emitter.instruction("b __rt_round_mode_finish");                            // below the half-way point the integral part stands

    emitter.label("__rt_round_mode_half_down");
    emitter.instruction("fcmp d6, d2");                                         // compare $num against the half-way point
    emitter.instruction("b.gt __rt_round_mode_bump");                           // only strictly-above rounds away from zero
    emitter.instruction("b __rt_round_mode_finish");                            // ties stay on the integral part

    emitter.label("__rt_round_mode_half_even");
    emitter.instruction("fcmp d6, d2");                                         // compare $num against the half-way point
    emitter.instruction("b.gt __rt_round_mode_bump");                           // strictly above the tie always rounds away
    emitter.instruction("b.ne __rt_round_mode_finish");                         // strictly below the tie keeps the integral part
    emitter.instruction("fcvtzs x13, d3");                                      // the integral part is exact below 1e16
    emitter.instruction("tbnz x13, #0, __rt_round_mode_bump");                  // an odd integral part must step to the even neighbour
    emitter.instruction("b __rt_round_mode_finish");                            // an even integral part is already correct

    emitter.label("__rt_round_mode_half_odd");
    emitter.instruction("fcmp d6, d2");                                         // compare $num against the half-way point
    emitter.instruction("b.gt __rt_round_mode_bump");                           // strictly above the tie always rounds away
    emitter.instruction("b.ne __rt_round_mode_finish");                         // strictly below the tie keeps the integral part
    emitter.instruction("fcvtzs x13, d3");                                      // the integral part is exact below 1e16
    emitter.instruction("tbz x13, #0, __rt_round_mode_bump");                   // an even integral part must step to the odd neighbour
    emitter.instruction("b __rt_round_mode_finish");                            // an odd integral part is already correct

    // -- php_round_get_zero_edge_case(): the directional modes compare against the step itself --
    emitter.label("__rt_round_mode_zero_edge");
    emitter.instruction("cmp x9, #0");                                          // unscale the integral part like php-src does
    emitter.instruction("b.gt __rt_round_mode_zero_edge_div");                  // positive precision unscales by dividing
    emitter.instruction("fmul d2, d3, d1");                                     // unscale the integral part by multiplying
    emitter.instruction("b __rt_round_mode_zero_edge_done");                    // the zero edge case is ready
    emitter.label("__rt_round_mode_zero_edge_div");
    emitter.instruction("fdiv d2, d3, d1");                                     // unscale the integral part by dividing
    emitter.label("__rt_round_mode_zero_edge_done");
    emitter.instruction("fabs d2, d2");                                         // php-src compares magnitudes only
    emitter.instruction("cmp x11, #5");                                         // mode 5 = CEILING
    emitter.instruction("b.eq __rt_round_mode_ceiling");                        // round toward positive infinity
    emitter.instruction("cmp x11, #6");                                         // mode 6 = FLOOR
    emitter.instruction("b.eq __rt_round_mode_floor");                          // round toward negative infinity
    emitter.instruction("fcmp d6, d2");                                         // mode 8 = AWAY_FROM_ZERO
    emitter.instruction("b.gt __rt_round_mode_bump");                           // any remainder grows the magnitude
    emitter.instruction("b __rt_round_mode_finish");                            // an exact value keeps the integral part

    emitter.label("__rt_round_mode_ceiling");
    emitter.instruction("fcmp d0, #0.0");                                       // CEILING only moves strictly positive values
    emitter.instruction("b.ls __rt_round_mode_finish");                         // non-positive values already sit at the ceiling
    emitter.instruction("fcmp d6, d2");                                         // is there any remainder left to round away?
    emitter.instruction("b.ls __rt_round_mode_finish");                         // an exact value keeps the integral part
    emitter.instruction("fmov d7, #1.0");                                       // CEILING always adds +1.0, never copysign()
    emitter.instruction("b __rt_round_mode_bump");                              // step toward positive infinity

    emitter.label("__rt_round_mode_floor");
    emitter.instruction("fcmp d0, #0.0");                                       // FLOOR only moves strictly negative values
    emitter.instruction("b.ge __rt_round_mode_finish");                         // non-negative values already sit at the floor
    emitter.instruction("fcmp d6, d2");                                         // is there any remainder left to round away?
    emitter.instruction("b.ls __rt_round_mode_finish");                         // an exact value keeps the integral part
    emitter.instruction("fmov d7, #-1.0");                                      // FLOOR always subtracts 1.0, never copysign()

    emitter.label("__rt_round_mode_bump");
    emitter.instruction("fadd d3, d3, d7");                                     // move the integral part one step in the chosen direction

    // -- unscale the rounded integral part back to the requested precision --
    emitter.label("__rt_round_mode_finish");
    emitter.instruction("fcmp d3, #0.0");                                       // a zero integral part already carries the final sign
    emitter.instruction("b.eq __rt_round_mode_return_integral");                // avoid 0.0 * INF turning an absurd precision into NAN
    emitter.instruction("cmp x9, #0");                                          // unscale the result the way php-src does
    emitter.instruction("b.gt __rt_round_mode_result_div");                     // positive precision unscales by dividing
    emitter.instruction("fmul d0, d3, d1");                                     // unscale the rounded value by multiplying
    emitter.instruction("b __rt_round_mode_return");                            // the rounded result is ready
    emitter.label("__rt_round_mode_result_div");
    emitter.instruction("fdiv d0, d3, d1");                                     // unscale the rounded value by dividing
    emitter.instruction("b __rt_round_mode_return");                            // the rounded result is ready

    emitter.label("__rt_round_mode_return_integral");
    emitter.instruction("fmov d0, d3");                                         // return the signed zero unchanged
    emitter.instruction("b __rt_round_mode_return");                            // fall through to the shared epilogue

    emitter.label("__rt_round_mode_return_value");
    emitter.instruction("ldr d0, [sp, #0]");                                    // PHP returns the untouched $num for these inputs

    emitter.label("__rt_round_mode_return");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate the round-mode frame
    emitter.instruction("ret");                                                 // return with d0 = rounded value
}

/// Emits the x86_64 System V variant of `__rt_round_mode`.
///
/// Mirrors the AArch64 logic instruction for instruction; only the register convention, the
/// 64-bit immediate materialization, and the SSE spelling of `floor`/`ceil`/`fabs` differ.
/// `roundsd` requires SSE4.1, which the backend already assumes for `floor()`/`ceil()`.
fn emit_round_mode_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: round_mode ---");
    emitter.label_global("__rt_round_mode");

    // Frame layout:
    //   [rbp - 8]  = $num
    //   [rbp - 16] = $precision
    //   [rbp - 24] = $mode
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the spill slots
    emitter.instruction("sub rsp, 64");                                         // reserve aligned spill slots so pow() stays 16-byte aligned
    emitter.instruction("movsd QWORD PTR [rbp - 8], xmm0");                     // spill $num across the optional pow() call
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // spill $precision across the optional pow() call
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // spill $mode across the optional pow() call

    // -- php-src returns non-finite and zero inputs untouched --
    emitter.instruction("movq rax, xmm0");                                      // raw IEEE-754 payload of $num
    emitter.instruction("add rax, rax");                                        // drop the sign bit so +0.0 and -0.0 collapse
    emitter.instruction("je __rt_round_mode_return_value_x86");                 // PHP returns +/-0.0 unchanged
    emitter.instruction("mov rcx, 0xffe0000000000000");                         // smallest sign-stripped payload with an all-ones exponent
    emitter.instruction("cmp rax, rcx");                                        // is the exponent field saturated (INF or NAN)?
    emitter.instruction("jae __rt_round_mode_return_value_x86");                // PHP returns INF/NAN unchanged

    // -- exponent = php_intpow10(abs($precision)) --
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // rax = $precision
    emitter.instruction("mov rdx, rax");                                        // copy it to build the arithmetic absolute value
    emitter.instruction("sar rdx, 63");                                         // expand the sign bit into an all-zero or all-one mask
    emitter.instruction("xor rax, rdx");                                        // flip the payload bits for negative precisions
    emitter.instruction("sub rax, rdx");                                        // rax = abs($precision)
    emitter.instruction("cmp rax, 22");                                         // 10^0..10^22 are exactly representable doubles
    emitter.instruction("jg __rt_round_mode_pow_call_x86");                     // anything larger needs libc pow()
    emitter.instruction(&format!("mov rcx, 0x{:x}", ONE_BITS));                 // IEEE-754 payload of 1.0
    emitter.instruction("movq xmm1, rcx");                                      // start the exact power-of-ten accumulator at 1.0
    emitter.instruction(&format!("mov rcx, 0x{:x}", TEN_BITS));                 // IEEE-754 payload of 10.0
    emitter.instruction("movq xmm2, rcx");                                      // the decimal base multiplied in per requested place
    emitter.label("__rt_round_mode_pow_loop_x86");
    emitter.instruction("test rax, rax");                                       // every requested decimal place applied?
    emitter.instruction("je __rt_round_mode_pow_done_x86");                     // yes - the exact exponent is ready
    emitter.instruction("mulsd xmm1, xmm2");                                    // multiply in one exact decimal place
    emitter.instruction("dec rax");                                             // one decimal place consumed
    emitter.instruction("jmp __rt_round_mode_pow_loop_x86");                    // keep accumulating the exact power of ten
    emitter.label("__rt_round_mode_pow_call_x86");
    emitter.instruction("cvtsi2sd xmm1, rax");                                  // convert abs($precision) into pow()'s exponent argument
    emitter.instruction(&format!("mov rcx, 0x{:x}", TEN_BITS));                 // IEEE-754 payload of 10.0
    emitter.instruction("movq xmm0, rcx");                                      // pow() base 10.0
    emitter.emit_call_c("pow");
    emitter.instruction("movsd xmm1, xmm0");                                    // move the computed exponent into the shared register
    emitter.label("__rt_round_mode_pow_done_x86");

    // -- scale $num into the integral domain of the requested precision --
    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 8]");                     // xmm0 = $num
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // r9 = $precision, live for every later branch
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // r10 = $mode, live for every later branch
    emitter.instruction("movsd xmm2, xmm0");                                    // xmm2 = the value being scaled
    emitter.instruction("cmp r9, 0");                                           // php-src branches on `places > 0` everywhere
    emitter.instruction("jg __rt_round_mode_scale_mul_x86");                    // positive precision multiplies by the exponent
    emitter.instruction("divsd xmm2, xmm1");                                    // scale by dividing for zero/negative precision
    emitter.instruction("jmp __rt_round_mode_scale_done_x86");                  // the scaled value is ready
    emitter.label("__rt_round_mode_scale_mul_x86");
    emitter.instruction("mulsd xmm2, xmm1");                                    // scale by multiplying for positive precision

    // -- extract the integral part and php-src's off-by-one-ULP correction candidate --
    emitter.label("__rt_round_mode_scale_done_x86");
    emitter.instruction(&format!("mov rcx, 0x{:x}", ONE_BITS));                 // IEEE-754 payload of 1.0
    emitter.instruction("movq xmm5, rcx");                                      // xmm5 = 1.0, the correction step
    emitter.instruction("xorpd xmm7, xmm7");                                    // xmm7 = 0.0 for the sign test
    emitter.instruction("comisd xmm0, xmm7");                                   // php-src splits on the sign of the original value
    emitter.instruction("jb __rt_round_mode_negative_x86");                     // negative values take the ceil() branch
    emitter.instruction("roundsd xmm3, xmm2, 1");                               // integral = floor(scaled)
    emitter.instruction("movsd xmm4, xmm3");                                    // copy the integral part for the candidate
    emitter.instruction("addsd xmm4, xmm5");                                    // candidate = integral + 1.0
    emitter.instruction("jmp __rt_round_mode_correct_x86");                     // test whether the candidate rebuilds $num exactly
    emitter.label("__rt_round_mode_negative_x86");
    emitter.instruction("roundsd xmm3, xmm2, 2");                               // integral = ceil(scaled)
    emitter.instruction("movsd xmm4, xmm3");                                    // copy the integral part for the candidate
    emitter.instruction("subsd xmm4, xmm5");                                    // candidate = integral - 1.0

    // -- adopt the candidate when unscaling it reproduces $num bit-for-bit --
    emitter.label("__rt_round_mode_correct_x86");
    emitter.instruction("movsd xmm5, xmm4");                                    // xmm5 = the candidate being unscaled
    emitter.instruction("cmp r9, 0");                                           // unscale the candidate the same way it was scaled
    emitter.instruction("jg __rt_round_mode_back_div_x86");                     // positive precision unscales by dividing
    emitter.instruction("mulsd xmm5, xmm1");                                    // unscale the candidate by multiplying
    emitter.instruction("jmp __rt_round_mode_back_done_x86");                   // the unscaled candidate is ready
    emitter.label("__rt_round_mode_back_div_x86");
    emitter.instruction("divsd xmm5, xmm1");                                    // unscale the candidate by dividing
    emitter.label("__rt_round_mode_back_done_x86");
    emitter.instruction("ucomisd xmm5, xmm0");                                  // did the candidate rebuild $num exactly?
    emitter.instruction("jp __rt_round_mode_no_correct_x86");                   // an unordered compare is never an exact match
    emitter.instruction("jne __rt_round_mode_no_correct_x86");                  // a different value leaves the integral part alone
    emitter.instruction("movsd xmm3, xmm4");                                    // adopt the corrected integral part

    // -- values past the double precision limit are returned untouched --
    emitter.label("__rt_round_mode_no_correct_x86");
    emitter.instruction("movq rcx, xmm3");                                      // raw payload of the integral part
    emitter.instruction("shl rcx, 1");                                          // drop the sign bit to take the magnitude
    emitter.instruction("shr rcx, 1");                                          // restore the exponent/mantissa alignment
    emitter.instruction("movq xmm6, rcx");                                      // xmm6 = fabs(integral)
    emitter.instruction(&format!("mov rcx, 0x{:x}", ONE_E16_BITS));             // 1e16, php-src's precision cutoff
    emitter.instruction("movq xmm7, rcx");                                      // xmm7 = 1e16
    emitter.instruction("comisd xmm6, xmm7");                                   // is the integral part beyond double precision?
    emitter.instruction("jae __rt_round_mode_return_value_x86");                // yes - php-src returns $num unchanged

    // -- php_round_helper(): dispatch on the requested rounding mode --
    emitter.instruction("movq rcx, xmm0");                                      // raw payload of $num
    emitter.instruction("shl rcx, 1");                                          // drop the sign bit to take the magnitude
    emitter.instruction("shr rcx, 1");                                          // restore the exponent/mantissa alignment
    emitter.instruction("movq xmm6, rcx");                                      // xmm6 = fabs($num), php-src's `value_abs`
    emitter.instruction("cmp r10, 7");                                          // mode 7 = TOWARD_ZERO
    emitter.instruction("je __rt_round_mode_finish_x86");                       // truncation keeps the integral part as-is
    emitter.instruction("movq rdx, xmm3");                                      // raw payload of the integral part
    emitter.instruction("mov rcx, 0x8000000000000000");                         // IEEE-754 sign-bit mask
    emitter.instruction("and rdx, rcx");                                        // isolate the sign bit for the copysign() builds
    emitter.instruction(&format!("mov rcx, 0x{:x}", ONE_BITS));                 // IEEE-754 payload of 1.0
    emitter.instruction("mov r8, rdx");                                         // copy the isolated sign bit
    emitter.instruction("or r8, rcx");                                          // copysign(1.0, integral)
    emitter.instruction("movq xmm7, r8");                                       // xmm7 = the magnitude step php-src adds
    emitter.instruction("cmp r10, 5");                                          // modes 5, 6 and 8 use the zero edge case
    emitter.instruction("jge __rt_round_mode_zero_edge_x86");                   // directional modes skip the half-way edge case

    // -- php_round_get_basic_edge_case(): the exact half-way point of this integral step --
    emitter.instruction(&format!("mov rcx, 0x{:x}", HALF_BITS));                // IEEE-754 payload of 0.5
    emitter.instruction("or rdx, rcx");                                         // copysign(0.5, integral)
    emitter.instruction("movq xmm2, rdx");                                      // xmm2 = the half-way offset
    emitter.instruction("addsd xmm2, xmm3");                                    // integral + copysign(0.5, integral)
    emitter.instruction("cmp r9, 0");                                           // unscale the edge case like php-src does
    emitter.instruction("jg __rt_round_mode_edge_div_x86");                     // positive precision unscales by dividing
    emitter.instruction("mulsd xmm2, xmm1");                                    // unscale the edge case by multiplying
    emitter.instruction("jmp __rt_round_mode_edge_done_x86");                   // the edge case is ready
    emitter.label("__rt_round_mode_edge_div_x86");
    emitter.instruction("divsd xmm2, xmm1");                                    // unscale the edge case by dividing
    emitter.label("__rt_round_mode_edge_done_x86");
    emitter.instruction("movq rcx, xmm2");                                      // raw payload of the edge case
    emitter.instruction("shl rcx, 1");                                          // drop the sign bit to take the magnitude
    emitter.instruction("shr rcx, 1");                                          // restore the exponent/mantissa alignment
    emitter.instruction("movq xmm2, rcx");                                      // php-src compares magnitudes only
    emitter.instruction("cmp r10, 1");                                          // mode 1 = HALF_UP
    emitter.instruction("je __rt_round_mode_half_up_x86");                      // ties move away from zero
    emitter.instruction("cmp r10, 2");                                          // mode 2 = HALF_DOWN
    emitter.instruction("je __rt_round_mode_half_down_x86");                    // ties move toward zero
    emitter.instruction("cmp r10, 3");                                          // mode 3 = HALF_EVEN
    emitter.instruction("je __rt_round_mode_half_even_x86");                    // ties move to the even neighbour
    emitter.instruction("jmp __rt_round_mode_half_odd_x86");                    // mode 4 = HALF_ODD

    emitter.label("__rt_round_mode_half_up_x86");
    emitter.instruction("comisd xmm6, xmm2");                                   // compare $num against the half-way point
    emitter.instruction("jae __rt_round_mode_bump_x86");                        // ties and everything above round away from zero
    emitter.instruction("jmp __rt_round_mode_finish_x86");                      // below the half-way point the integral part stands

    emitter.label("__rt_round_mode_half_down_x86");
    emitter.instruction("comisd xmm6, xmm2");                                   // compare $num against the half-way point
    emitter.instruction("ja __rt_round_mode_bump_x86");                         // only strictly-above rounds away from zero
    emitter.instruction("jmp __rt_round_mode_finish_x86");                      // ties stay on the integral part

    emitter.label("__rt_round_mode_half_even_x86");
    emitter.instruction("comisd xmm6, xmm2");                                   // compare $num against the half-way point
    emitter.instruction("ja __rt_round_mode_bump_x86");                         // strictly above the tie always rounds away
    emitter.instruction("jne __rt_round_mode_finish_x86");                      // strictly below the tie keeps the integral part
    emitter.instruction("cvttsd2si r11, xmm3");                                 // the integral part is exact below 1e16
    emitter.instruction("test r11, 1");                                         // is the integral part odd?
    emitter.instruction("jne __rt_round_mode_bump_x86");                        // an odd integral part must step to the even neighbour
    emitter.instruction("jmp __rt_round_mode_finish_x86");                      // an even integral part is already correct

    emitter.label("__rt_round_mode_half_odd_x86");
    emitter.instruction("comisd xmm6, xmm2");                                   // compare $num against the half-way point
    emitter.instruction("ja __rt_round_mode_bump_x86");                         // strictly above the tie always rounds away
    emitter.instruction("jne __rt_round_mode_finish_x86");                      // strictly below the tie keeps the integral part
    emitter.instruction("cvttsd2si r11, xmm3");                                 // the integral part is exact below 1e16
    emitter.instruction("test r11, 1");                                         // is the integral part odd?
    emitter.instruction("je __rt_round_mode_bump_x86");                         // an even integral part must step to the odd neighbour
    emitter.instruction("jmp __rt_round_mode_finish_x86");                      // an odd integral part is already correct

    // -- php_round_get_zero_edge_case(): the directional modes compare against the step itself --
    emitter.label("__rt_round_mode_zero_edge_x86");
    emitter.instruction("movsd xmm2, xmm3");                                    // xmm2 = the integral part being unscaled
    emitter.instruction("cmp r9, 0");                                           // unscale the integral part like php-src does
    emitter.instruction("jg __rt_round_mode_zero_edge_div_x86");                // positive precision unscales by dividing
    emitter.instruction("mulsd xmm2, xmm1");                                    // unscale the integral part by multiplying
    emitter.instruction("jmp __rt_round_mode_zero_edge_done_x86");              // the zero edge case is ready
    emitter.label("__rt_round_mode_zero_edge_div_x86");
    emitter.instruction("divsd xmm2, xmm1");                                    // unscale the integral part by dividing
    emitter.label("__rt_round_mode_zero_edge_done_x86");
    emitter.instruction("movq rcx, xmm2");                                      // raw payload of the zero edge case
    emitter.instruction("shl rcx, 1");                                          // drop the sign bit to take the magnitude
    emitter.instruction("shr rcx, 1");                                          // restore the exponent/mantissa alignment
    emitter.instruction("movq xmm2, rcx");                                      // php-src compares magnitudes only
    emitter.instruction("cmp r10, 5");                                          // mode 5 = CEILING
    emitter.instruction("je __rt_round_mode_ceiling_x86");                      // round toward positive infinity
    emitter.instruction("cmp r10, 6");                                          // mode 6 = FLOOR
    emitter.instruction("je __rt_round_mode_floor_x86");                        // round toward negative infinity
    emitter.instruction("comisd xmm6, xmm2");                                   // mode 8 = AWAY_FROM_ZERO
    emitter.instruction("ja __rt_round_mode_bump_x86");                         // any remainder grows the magnitude
    emitter.instruction("jmp __rt_round_mode_finish_x86");                      // an exact value keeps the integral part

    emitter.label("__rt_round_mode_ceiling_x86");
    emitter.instruction("xorpd xmm5, xmm5");                                    // xmm5 = 0.0 for the sign test
    emitter.instruction("comisd xmm0, xmm5");                                   // CEILING only moves strictly positive values
    emitter.instruction("jbe __rt_round_mode_finish_x86");                      // non-positive values already sit at the ceiling
    emitter.instruction("comisd xmm6, xmm2");                                   // is there any remainder left to round away?
    emitter.instruction("jbe __rt_round_mode_finish_x86");                      // an exact value keeps the integral part
    emitter.instruction(&format!("mov rcx, 0x{:x}", ONE_BITS));                 // IEEE-754 payload of 1.0
    emitter.instruction("movq xmm7, rcx");                                      // CEILING always adds +1.0, never copysign()
    emitter.instruction("jmp __rt_round_mode_bump_x86");                        // step toward positive infinity

    emitter.label("__rt_round_mode_floor_x86");
    emitter.instruction("xorpd xmm5, xmm5");                                    // xmm5 = 0.0 for the sign test
    emitter.instruction("comisd xmm5, xmm0");                                   // FLOOR only moves strictly negative values
    emitter.instruction("jbe __rt_round_mode_finish_x86");                      // non-negative values already sit at the floor
    emitter.instruction("comisd xmm6, xmm2");                                   // is there any remainder left to round away?
    emitter.instruction("jbe __rt_round_mode_finish_x86");                      // an exact value keeps the integral part
    emitter.instruction(&format!("mov rcx, 0x{:x}", ONE_BITS | (1u64 << 63)));  // IEEE-754 payload of -1.0
    emitter.instruction("movq xmm7, rcx");                                      // FLOOR always subtracts 1.0, never copysign()

    emitter.label("__rt_round_mode_bump_x86");
    emitter.instruction("addsd xmm3, xmm7");                                    // move the integral part one step in the chosen direction

    // -- unscale the rounded integral part back to the requested precision --
    emitter.label("__rt_round_mode_finish_x86");
    emitter.instruction("xorpd xmm5, xmm5");                                    // xmm5 = 0.0 for the zero test
    emitter.instruction("ucomisd xmm3, xmm5");                                  // a zero integral part already carries the final sign
    emitter.instruction("jp __rt_round_mode_finish_scale_x86");                 // an unordered compare cannot be a zero
    emitter.instruction("je __rt_round_mode_return_integral_x86");              // avoid 0.0 * INF turning an absurd precision into NAN
    emitter.label("__rt_round_mode_finish_scale_x86");
    emitter.instruction("cmp r9, 0");                                           // unscale the result the way php-src does
    emitter.instruction("jg __rt_round_mode_result_div_x86");                   // positive precision unscales by dividing
    emitter.instruction("mulsd xmm3, xmm1");                                    // unscale the rounded value by multiplying
    emitter.instruction("movsd xmm0, xmm3");                                    // move the rounded value into the result register
    emitter.instruction("jmp __rt_round_mode_return_x86");                      // the rounded result is ready
    emitter.label("__rt_round_mode_result_div_x86");
    emitter.instruction("divsd xmm3, xmm1");                                    // unscale the rounded value by dividing
    emitter.instruction("movsd xmm0, xmm3");                                    // move the rounded value into the result register
    emitter.instruction("jmp __rt_round_mode_return_x86");                      // the rounded result is ready

    emitter.label("__rt_round_mode_return_integral_x86");
    emitter.instruction("movsd xmm0, xmm3");                                    // return the signed zero unchanged
    emitter.instruction("jmp __rt_round_mode_return_x86");                      // fall through to the shared epilogue

    emitter.label("__rt_round_mode_return_value_x86");
    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 8]");                     // PHP returns the untouched $num for these inputs

    emitter.label("__rt_round_mode_return_x86");
    emitter.instruction("add rsp, 64");                                         // release the round-mode spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return with xmm0 = rounded value
}
