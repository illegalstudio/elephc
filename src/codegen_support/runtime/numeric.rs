//! Purpose:
//! Emits PHP's two shared `float`→`int` conversions on every supported target:
//! `__rt_php_float_to_int`, the modulo-2^64 rule every cast, array-key, and numeric-coercion
//! site applies to a float VALUE, and `__rt_php_float_to_int_cap`, the saturating rule a
//! numeric STRING takes instead. Keeping both here is what stops a call site open-coding either.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()`.
//! - Indirectly from every `crate::codegen_support::abi::emit_php_float_to_int()` call site.
//! - `crate::codegen_support::runtime::strings::str_to_int` calls the cap helper for the
//!   float-form branch of `__rt_str_to_int`.
//!
//! Key details:
//! - The two rules genuinely disagree: `(int)1e19` is negative in PHP because a float value
//!   wraps, while `(int)"1e19"` is `PHP_INT_MAX` because a numeric string saturates. Both cast
//!   NaN and ±INF to `0`.
//! - Reference PHP 8.4 (`zend_dval_to_lval`) casts NaN and ±INF to `0` and reduces any other
//!   out-of-range finite double modulo 2^64 before reinterpreting it as a signed 64-bit value.
//!   Raw hardware truncation does neither: AArch64 `fcvtzs` saturates to `INT64_MIN`/`INT64_MAX`
//!   while x86_64 `cvttsd2si` yields `INT64_MIN` for every invalid input, so the two supported
//!   architectures used to disagree with PHP *and* with each other.
//! - The helper therefore decodes the IEEE-754 fields with integer instructions only. That is
//!   exact by construction and produces bit-identical results on AArch64 and x86_64.
//! - ABI: the double arrives in the float result register (`d0` / `xmm0`); the converted integer
//!   is returned in the *symbol scratch* register (`x9` / `r11`), not in the int result register.
//!   Every other register — including `x0`/`rax` and the whole floating-point file — is
//!   preserved, so the helper can be called from lowering sites that still hold live values.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_php_float_to_int` runtime helper for the active target.
///
/// # Input
/// - AArch64: the source double in `d0`; x86_64: the source double in `xmm0`.
///
/// # Output
/// - AArch64: the PHP integer value in `x9`; x86_64: the PHP integer value in `r11`.
///
/// # Clobbers
/// - Only the output register (plus `x30` on AArch64, as for any `bl`). Callers may keep live
///   values in every other integer and floating-point register across the call.
pub fn emit_php_float_to_int(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_php_float_to_int_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: php_float_to_int ---");
    emitter.label_global("__rt_php_float_to_int");

    // -- decode the IEEE-754 fields of the incoming double --
    emitter.instruction("stp x10, x11, [sp, #-16]!");                           // preserve the two scratch registers this leaf helper needs
    emitter.instruction("fmov x9, d0");                                         // raw IEEE-754 bit pattern of the source double
    emitter.instruction("ubfx x10, x9, #52, #11");                              // biased exponent field
    emitter.instruction("cmp x10, #1023");                                      // is the magnitude below 1.0?
    emitter.instruction("b.lo __rt_php_float_to_int_zero");                     // PHP truncates |d| < 1 (and zero/subnormals) to 0
    emitter.instruction("and x11, x9, #0x000fffffffffffff");                    // 52-bit fraction field
    emitter.instruction("orr x11, x11, #0x0010000000000000");                   // restore the implicit leading significand bit
    emitter.instruction("sub x10, x10, #1075");                                 // binary shift = exponent - bias - mantissa width
    emitter.instruction("cmp x10, #64");                                        // would every significand bit leave the 64-bit window?
    emitter.instruction("b.ge __rt_php_float_to_int_zero");                     // yes: PHP's modulo-2^64 reduction is 0 (also covers NaN/±INF)

    // -- shift the significand into place, wrapping modulo 2^64 exactly like PHP --
    emitter.instruction("tbnz x10, #63, __rt_php_float_to_int_right");          // a negative shift means the value has a fractional part
    emitter.instruction("lsl x11, x11, x10");                                   // scale the significand up, keeping only the low 64 bits
    emitter.instruction("b __rt_php_float_to_int_sign");                        // apply the sign to the computed magnitude

    emitter.label("__rt_php_float_to_int_right");
    emitter.instruction("neg x10, x10");                                        // turn the negative shift into a right-shift distance
    emitter.instruction("lsr x11, x11, x10");                                   // drop the fractional bits, truncating toward zero

    emitter.label("__rt_php_float_to_int_sign");
    emitter.instruction("tbnz x9, #63, __rt_php_float_to_int_negate");          // negative doubles need a two's complement result
    emitter.instruction("mov x9, x11");                                         // non-negative doubles return the magnitude unchanged
    emitter.instruction("b __rt_php_float_to_int_done");                        // fall through to the shared epilogue

    emitter.label("__rt_php_float_to_int_negate");
    emitter.instruction("neg x9, x11");                                         // negate modulo 2^64 for negative doubles
    emitter.instruction("b __rt_php_float_to_int_done");                        // fall through to the shared epilogue

    emitter.label("__rt_php_float_to_int_zero");
    emitter.instruction("mov x9, #0");                                          // PHP casts NaN, ±INF and fully-out-of-window values to 0

    emitter.label("__rt_php_float_to_int_done");
    emitter.instruction("ldp x10, x11, [sp], #16");                             // restore the preserved scratch registers
    emitter.instruction("ret");                                                 // return the PHP integer value in x9
}

/// Emits the `__rt_php_float_to_int_cap` runtime helper for the active target.
///
/// PHP has TWO double-to-int rules, and which one applies depends on where the double came
/// from. A float VALUE is reduced modulo 2^64, which is [`emit_php_float_to_int`]. A numeric
/// STRING is CAPPED: `(int)1e19` is negative in PHP, `(int)"1e19"` is `PHP_INT_MAX`. This is the
/// second rule, kept beside the first so neither is open-coded at a call site -- a bare
/// `fcvtzs` / `cvttsd2si` is what produced the per-target `(int)NAN` divergence the sibling
/// helper exists to prevent.
///
/// # Input
/// - AArch64: the source double in `d0`; x86_64: the source double in `xmm0`.
///
/// # Output
/// - AArch64: the PHP integer value in `x9`; x86_64: the PHP integer value in `r11`.
///
/// # Clobbers
/// - Only the output register (plus `x30` on AArch64, as for any `bl`).
pub fn emit_php_float_to_int_cap(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_php_float_to_int_cap_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: php_float_to_int_cap ---");
    emitter.label_global("__rt_php_float_to_int_cap");

    emitter.instruction("stp x10, x11, [sp, #-16]!");                           // preserve the scratch registers this leaf helper needs
    emitter.instruction("fmov x9, d0");                                         // raw IEEE-754 bit pattern of the source double
    emitter.instruction("lsl x10, x9, #1");                                     // drop the sign bit: NaN and both infinities compare alike
    crate::codegen_support::abi::emit_load_int_immediate(
        emitter,
        "x11",
        0xffe0_0000_0000_0000u64 as i64,
    );                                                                          // twice the exponent-all-ones pattern
    emitter.instruction("cmp x10, x11");                                        // is the magnitude infinite or NaN?
    emitter.instruction("b.hs __rt_php_float_to_int_cap_zero");                 // yes: PHP casts it to 0
    // Every remaining value is finite, so `fcvtzs` is fully defined here: it truncates toward
    // zero and SATURATES at PHP_INT_MAX/MIN, which is exactly the cap PHP applies.
    emitter.instruction("fcvtzs x9, d0");                                       // truncate toward zero, saturating on overflow
    emitter.instruction("b __rt_php_float_to_int_cap_done");                    // share the epilogue

    emitter.label("__rt_php_float_to_int_cap_zero");
    emitter.instruction("mov x9, #0");                                          // NaN and +-INF cast to 0

    emitter.label("__rt_php_float_to_int_cap_done");
    emitter.instruction("ldp x10, x11, [sp], #16");                             // restore the preserved scratch registers
    emitter.instruction("ret");                                                 // return the capped PHP integer value in x9
}

/// Emits the x86_64 variant of `__rt_php_float_to_int_cap`.
///
/// `cvttsd2si` reports the "integer indefinite" pattern (0x8000000000000000) when the value does
/// not fit, which is already the right answer for a negative overflow; a positive one has to
/// become PHP_INT_MAX instead.
fn emit_php_float_to_int_cap_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: php_float_to_int_cap ---");
    emitter.label_global("__rt_php_float_to_int_cap");

    emitter.instruction("push r10");                                            // preserve the scratch register this leaf helper needs
    emitter.instruction("movq r11, xmm0");                                      // raw IEEE-754 bit pattern of the source double
    emitter.instruction("add r11, r11");                                        // drop the sign bit: NaN and both infinities compare alike
    emitter.instruction("mov r10, 0xffe0000000000000");                         // twice the exponent-all-ones pattern
    emitter.instruction("cmp r11, r10");                                        // is the magnitude infinite or NaN?
    emitter.instruction("jae __rt_php_float_to_int_cap_zero");                  // yes: PHP casts it to 0
    emitter.instruction("cvttsd2si r11, xmm0");                                 // truncate toward zero
    emitter.instruction("mov r10, 0x8000000000000000");                         // the indefinite pattern cvttsd2si reports on overflow
    emitter.instruction("cmp r11, r10");                                        // did the conversion overflow?
    emitter.instruction("jne __rt_php_float_to_int_cap_done");                  // no: the truncated value stands
    emitter.instruction("movq r10, xmm0");                                      // reload the bit pattern to read its sign
    emitter.instruction("test r10, r10");                                       // was the source negative?
    emitter.instruction("js __rt_php_float_to_int_cap_done");                   // yes: PHP_INT_MIN is already in r11
    emitter.instruction("mov r11, 0x7fffffffffffffff");                         // positive overflow caps at PHP_INT_MAX
    emitter.instruction("jmp __rt_php_float_to_int_cap_done");                  // share the epilogue

    emitter.label("__rt_php_float_to_int_cap_zero");
    emitter.instruction("xor r11d, r11d");                                      // NaN and +-INF cast to 0

    emitter.label("__rt_php_float_to_int_cap_done");
    emitter.instruction("pop r10");                                             // restore the preserved scratch register
    emitter.instruction("ret");                                                 // return the capped PHP integer value in r11
}

/// Emits the x86_64 variant of `__rt_php_float_to_int`.
///
/// Mirrors the AArch64 decode exactly: `r11` holds the result, `r10` the raw bit pattern and
/// `rcx` the shift count. Both scratch registers are pushed so the helper only clobbers `r11`.
fn emit_php_float_to_int_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: php_float_to_int ---");
    emitter.label_global("__rt_php_float_to_int");

    // -- decode the IEEE-754 fields of the incoming double --
    emitter.instruction("push rcx");                                            // preserve the shift-count register this leaf helper needs
    emitter.instruction("push r10");                                            // preserve the raw-bit-pattern scratch register
    emitter.instruction("movq r10, xmm0");                                      // raw IEEE-754 bit pattern of the source double
    emitter.instruction("mov rcx, r10");                                        // copy the bit pattern before extracting the exponent
    emitter.instruction("shr rcx, 52");                                         // move the exponent field into the low bits
    emitter.instruction("and ecx, 0x7ff");                                      // biased exponent field
    emitter.instruction("cmp rcx, 1023");                                       // is the magnitude below 1.0?
    emitter.instruction("jb __rt_php_float_to_int_zero_x86_64");                // PHP truncates |d| < 1 (and zero/subnormals) to 0
    emitter.instruction("mov r11, r10");                                        // copy the bit pattern to build the significand
    emitter.instruction("shl r11, 12");                                         // drop the sign and exponent fields
    emitter.instruction("shr r11, 12");                                         // keep only the 52-bit fraction field
    emitter.instruction("bts r11, 52");                                         // restore the implicit leading significand bit
    emitter.instruction("sub rcx, 1075");                                       // binary shift = exponent - bias - mantissa width
    emitter.instruction("cmp rcx, 64");                                         // would every significand bit leave the 64-bit window?
    emitter.instruction("jge __rt_php_float_to_int_zero_x86_64");               // yes: PHP's modulo-2^64 reduction is 0 (also covers NaN/±INF)

    // -- shift the significand into place, wrapping modulo 2^64 exactly like PHP --
    emitter.instruction("test rcx, rcx");                                       // is the shift negative?
    emitter.instruction("js __rt_php_float_to_int_right_x86_64");               // a negative shift means the value has a fractional part
    emitter.instruction("shl r11, cl");                                         // scale the significand up, keeping only the low 64 bits
    emitter.instruction("jmp __rt_php_float_to_int_sign_x86_64");               // apply the sign to the computed magnitude

    emitter.label("__rt_php_float_to_int_right_x86_64");
    emitter.instruction("neg rcx");                                             // turn the negative shift into a right-shift distance
    emitter.instruction("shr r11, cl");                                         // drop the fractional bits, truncating toward zero

    emitter.label("__rt_php_float_to_int_sign_x86_64");
    emitter.instruction("test r10, r10");                                       // was the source double negative?
    emitter.instruction("jns __rt_php_float_to_int_done_x86_64");               // non-negative doubles return the magnitude unchanged
    emitter.instruction("neg r11");                                             // negate modulo 2^64 for negative doubles
    emitter.instruction("jmp __rt_php_float_to_int_done_x86_64");               // fall through to the shared epilogue

    emitter.label("__rt_php_float_to_int_zero_x86_64");
    emitter.instruction("xor r11d, r11d");                                      // PHP casts NaN, ±INF and fully-out-of-window values to 0

    emitter.label("__rt_php_float_to_int_done_x86_64");
    emitter.instruction("pop r10");                                             // restore the raw-bit-pattern scratch register
    emitter.instruction("pop rcx");                                             // restore the shift-count register
    emitter.instruction("ret");                                                 // return the PHP integer value in r11
}

/// Emits `__rt_abs_int`, the raw integer-abs entry used by the abs int sink.
///
/// The ordinary negative value returns `-value`. `INT_MIN` cannot be represented as a positive
/// int, so its PHP result is the float `2^63`; an `(int)` observation of that float applies the
/// shared warned float-to-int conversion, which yields `INT_MIN` again.
pub fn emit_abs_int(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emitter.blank();
        emitter.comment("--- runtime: abs_int ---");
        emitter.label_global("__rt_abs_int");
        emitter.instruction("test rax, rax");                                   // is the integer already non-negative?
        emitter.instruction("jns __rt_abs_int_done_x86");                       // non-negative values are their own absolute value
        emitter.instruction("neg rax");                                         // compute the magnitude modulo 2^64
        emitter.instruction("test rax, rax");                                   // did negation leave the INT_MIN bit pattern?
        emitter.instruction("js __rt_abs_int_min_x86");                         // INT_MIN must promote to the float 2^63
        emitter.instruction("ret");                                             // return the in-range integer magnitude
        emitter.label("__rt_abs_int_min_x86");
        crate::codegen_support::abi::emit_load_int_immediate(emitter, "r10", 0x43e0000000000000_u64 as i64);
        emitter.instruction("movq xmm0, r10");                                  // load the exact double representation of 2^63
        emitter.instruction("jmp __rt_float_key_to_int");                       // apply PHP's warned float-to-int conversion
        emitter.label("__rt_abs_int_done_x86");
        emitter.instruction("ret");                                             // return the non-negative integer unchanged
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: abs_int ---");
    emitter.label_global("__rt_abs_int");
    emitter.instruction("cmp x0, #0");                                          // is the integer already non-negative?
    emitter.instruction("b.ge __rt_abs_int_done");                              // non-negative values are their own absolute value
    emitter.instruction("neg x1, x0");                                          // compute the magnitude modulo 2^64
    emitter.instruction("tbnz x1, #63, __rt_abs_int_min");                      // did negation leave the INT_MIN bit pattern?
    emitter.instruction("mov x0, x1");                                          // return the in-range integer magnitude
    emitter.instruction("ret");                                                 // transfer the absolute value to the caller
    emitter.label("__rt_abs_int_min");
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "x9", 0x43e0000000000000_u64 as i64);
    emitter.instruction("fmov d0, x9");                                         // load the exact double representation of 2^63
    emitter.instruction("b __rt_float_key_to_int");                             // apply PHP's warned float-to-int conversion
    emitter.label("__rt_abs_int_done");
    emitter.instruction("ret");                                                 // return the non-negative integer unchanged
}

/// PHP's exact warning for casting a negative integer square root (NAN) to int.
pub(crate) const SQRT_NAN_WARNING: &str =
    "Warning: The float NAN is not representable as an int, cast occurred\n";

/// Emits `__rt_sqrt_add_int`, the fused `(int)sqrt(a + b)` reducer for integer operands.
///
/// # Input
/// - AArch64: `x0`/`x1` = the two integer addends; x86_64: `rdi`/`rsi`.
///
/// # Output
/// - AArch64: `x0` = the truncated square root; x86_64: `rax`.
///
/// The integer sum follows PHP's checked-add promotion: an overflowing add is recomputed as a
/// double sum before the square root. A negative sum yields NAN, which PHP's int cast warns
/// about and converts to zero. Every non-negative result is at most `sqrt(2^64)`, so hardware
/// truncation is always in range.
pub fn emit_sqrt_add_int(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emitter.blank();
        emitter.comment("--- runtime: sqrt_add_int ---");
        emitter.label_global("__rt_sqrt_add_int");
        emitter.instruction("mov rax, rdi");                                    // start from the left addend
        emitter.instruction("add rax, rsi");                                    // apply PHP's checked integer addition
        emitter.instruction("jo __rt_sqrt_add_int_overflow_x86");               // an overflowing sum promotes to a double
        emitter.instruction("cvtsi2sd xmm0, rax");                              // convert the in-range sum to double
        emitter.instruction("jmp __rt_sqrt_add_int_sqrt_x86");                  // share the square-root and result tail
        emitter.label("__rt_sqrt_add_int_overflow_x86");
        emitter.instruction("cvtsi2sd xmm0, rdi");                              // convert the left addend for the promoted sum
        emitter.instruction("cvtsi2sd xmm1, rsi");                              // convert the right addend for the promoted sum
        emitter.instruction("addsd xmm0, xmm1");                                // compute PHP's promoted double sum
        emitter.label("__rt_sqrt_add_int_sqrt_x86");
        emitter.instruction("sqrtsd xmm0, xmm0");                               // compute the hardware square root
        emitter.instruction("ucomisd xmm0, xmm0");                              // identify the NAN produced by a negative sum
        emitter.instruction("jp __rt_sqrt_add_int_nan_x86");                    // negative sums warn and cast to zero
        emitter.instruction("cvttsd2si rax, xmm0");                             // truncate toward zero like PHP's cast
        emitter.instruction("ret");                                             // return the integer square root
        emitter.label("__rt_sqrt_add_int_nan_x86");
        emitter.instruction("sub rsp, 8");                                      // align the stack for the System V diagnostic call
        crate::codegen_support::abi::emit_symbol_address(emitter, "rdi", "_diag_sqrt_nan_warning");
        crate::codegen_support::abi::emit_load_int_immediate(emitter, "rsi", SQRT_NAN_WARNING.len() as i64);
        crate::codegen_support::abi::emit_call_label(emitter, "__rt_diag_warning");
        emitter.instruction("add rsp, 8");                                      // release the diagnostic alignment slot
        emitter.instruction("xor eax, eax");                                    // PHP casts NAN to zero after the warning
        emitter.instruction("ret");                                             // return the warned zero result
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: sqrt_add_int ---");
    emitter.label_global("__rt_sqrt_add_int");
    emitter.instruction("adds x2, x0, x1");                                     // apply PHP's checked integer addition
    emitter.instruction("b.vs __rt_sqrt_add_int_overflow");                     // an overflowing sum promotes to a double
    emitter.instruction("scvtf d0, x2");                                        // convert the in-range sum to double
    emitter.instruction("b __rt_sqrt_add_int_sqrt");                            // share the square-root and result tail
    emitter.label("__rt_sqrt_add_int_overflow");
    emitter.instruction("scvtf d0, x0");                                        // convert the left addend for the promoted sum
    emitter.instruction("scvtf d1, x1");                                        // convert the right addend for the promoted sum
    emitter.instruction("fadd d0, d0, d1");                                     // compute PHP's promoted double sum
    emitter.label("__rt_sqrt_add_int_sqrt");
    emitter.instruction("fsqrt d0, d0");                                        // compute the hardware square root
    emitter.instruction("fcmp d0, d0");                                         // identify the NAN produced by a negative sum
    emitter.instruction("b.vs __rt_sqrt_add_int_nan");                          // negative sums warn and cast to zero
    emitter.instruction("fcvtzs x0, d0");                                       // truncate toward zero like PHP's cast
    emitter.instruction("ret");                                                 // return the integer square root
    emitter.label("__rt_sqrt_add_int_nan");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x1", "_diag_sqrt_nan_warning");
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "x2", SQRT_NAN_WARNING.len() as i64);
    crate::codegen_support::abi::emit_call_label(emitter, "__rt_diag_warning");
    emitter.instruction("mov x0, xzr");                                         // PHP casts NAN to zero after the warning
    emitter.instruction("ret");                                                 // return the warned zero result
}
