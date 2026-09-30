//! Purpose:
//! Emits the `__rt_ftoa` runtime helper assembly for float-to-string conversion.
//! Reproduces PHP's default `precision = 14` layout (`echo`, `(string)`, `print_r`,
//! string interpolation) and, through `__rt_ftoa_repr`, PHP's
//! `serialize_precision = -1` layout used by `var_dump`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - String helpers use PHP pointer/length pairs and target ABI return registers; heap-backed results must remain refcount-compatible.
//! - PHP formats a float for `echo` with `zend_gcvt(value, 14, '.', 'E')`. C's `%.14G`
//!   already picks the *same* notation (exponential when the decimal exponent is `>= 14`
//!   or `<= -5`) and the same 14-significant-digit rounding, but it differs in two
//!   byte-level details that `__rt_ftoa` fixes up while copying the snprintf scratch into
//!   `_concat_buf`:
//!   1. `zend_gcvt` always writes a fraction in exponential form, so a one-digit mantissa
//!      becomes `1.0E+300`, never C's `1E+300`.
//!   2. `zend_gcvt` writes the exponent with no leading zeros, so `1.0E-7`, never C's
//!      `1E-07`.
//! - `NAN` is normalized to the unsigned spelling PHP prints; glibc renders a negative
//!   quiet NaN as `-NAN`, which PHP never does.
//! - Since PHP 8.5 the conversion is also a diagnostic: coercing a NAN to a string raises an
//!   E_WARNING. Every PHP form that does so — `echo`, `(string)`, `strval()`, `implode()`,
//!   concatenation, interpolation, `sprintf('%s')`, `print_r()`, the string builtins that take
//!   a float — arrives here, so one probe at this entry covers the whole surface. The forms
//!   php leaves SILENT (`var_dump`, `json_encode`, `number_format`, `sprintf('%f')`) each own
//!   a different formatter and never reach this helper.
//! - `__rt_ftoa_repr` answers `var_dump`'s `%.*H` at `serialize_precision = -1`: the
//!   shortest decimal string that round-trips. The finite case is exactly
//!   `__rt_json_ftoa` with an uppercase `E` marker, so this helper only owns the
//!   `INF`/`-INF`/`NAN` spellings that `__rt_json_ftoa`'s caller normally handles.

use super::nan_string_coercion_warning::emit_nan_string_coercion_probe;
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Converts a double-precision float to a PHP-compatible byte string at `precision = 14`.
///
/// # Input
/// - ARM64: `d0` holds the float value
/// - x86_64: `xmm0` holds the float value (SysV variadic ABI)
///
/// # Output
/// - ARM64: `x1` = pointer to string, `x2` = length
/// - x86_64: `rax` = pointer to string, `rdx` = length
///
/// # Behavior
/// Formats the float with `snprintf("%.14G", …)` into a stack scratch buffer, then copies
/// the bytes into the global `_concat_buf` at the current `_concat_off` cursor while
/// applying PHP's `zend_gcvt` fixups (mandatory `.0` mantissa fraction in exponential
/// form, unpadded exponent, unsigned `NAN`), and advances `_concat_off` by the number of
/// bytes actually emitted.
///
/// # ABI Notes
/// - Apple ARM64: variadic floats are passed on the stack, not in SIMD registers
/// - Linux x86_64: delegates to `emit_ftoa_linux_x86_64`; uses SysV variadic ABI with `eax=1` to indicate one SIMD register argument
pub fn emit_ftoa(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ftoa_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: ftoa (precision=14, PHP zend_gcvt layout) ---");
    emitter.label_global("__rt_ftoa");

    // -- set up stack frame (80 bytes: variadic slot, 48-byte scratch, saved FP/LR) --
    emitter.instruction("sub sp, sp, #80");                                     // allocate the variadic slot, snprintf scratch, and saved-register area
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish new frame pointer

    // -- PHP 8.5 reports a NAN coerced to string, once per conversion --
    // The probe sits here rather than on the `NAN` text branch below: that branch keys off the
    // byte snprintf produced, while the helper's sentinel guards need the raw bit pattern, and
    // the float is still untouched at this point. The helper restores `d0`.
    emit_nan_string_coercion_probe(emitter, "__rt_ftoa_no_nan");

    // -- call snprintf(scratch, 48, "%.14G", double) --
    emitter.instruction("add x0, sp, #8");                                      // snprintf destination = stack scratch buffer
    emitter.instruction("mov x1, #48");                                         // scratch buffer size limit
    abi::emit_symbol_address(emitter, "x2", "_fmt_g");
    // -- Apple ARM64 variadic ABI: float arg goes on stack, not in SIMD reg --
    emitter.instruction("str d0, [sp]");                                        // push double onto stack for variadic call
    emitter.bl_c("snprintf");                                                   // format the double at 14 significant digits

    // -- destination cursor inside _concat_buf --
    abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load the current concat write offset
    abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x13, x11, x10");                                   // result start = concat_buf + offset
    emitter.instruction("mov x12, x13");                                        // x12 = write cursor, x13 = result start
    emitter.instruction("add x14, sp, #8");                                     // x14 = read cursor into the snprintf scratch
    emitter.instruction("mov w15, #0");                                         // w15 = "mantissa already has a '.'" flag

    // -- PHP never prints a signed NAN: collapse "NAN"/"-NAN" to "NAN" --
    emitter.instruction("ldrb w16, [x14]");                                     // first formatted byte
    emitter.instruction("cmp w16, #45");                                        // is it an ASCII '-' sign?
    emitter.instruction("b.ne __rt_ftoa_nan_check");                            // unsigned text: inspect the first byte directly
    emitter.instruction("ldrb w16, [x14, #1]");                                 // signed text: inspect the byte after the sign
    emitter.label("__rt_ftoa_nan_check");
    emitter.instruction("cmp w16, #78");                                        // ASCII 'N' can only start the NAN spelling
    emitter.instruction("b.ne __rt_ftoa_copy");                                 // ordinary numeric text: run the copy/fixup loop
    emitter.instruction("mov w17, #78");                                        // ASCII 'N'
    emitter.instruction("strb w17, [x12], #1");                                 // emit 'N'
    emitter.instruction("mov w17, #65");                                        // ASCII 'A'
    emitter.instruction("strb w17, [x12], #1");                                 // emit 'A'
    emitter.instruction("mov w17, #78");                                        // ASCII 'N'
    emitter.instruction("strb w17, [x12], #1");                                 // emit 'N'
    emitter.instruction("b __rt_ftoa_finish");                                  // NAN needs no further fixups

    // -- copy the mantissa, remembering whether it already contains a '.' --
    emitter.label("__rt_ftoa_copy");
    emitter.instruction("ldrb w16, [x14]");                                     // load the next scratch byte
    emitter.instruction("cbz w16, __rt_ftoa_finish");                           // NUL terminator: decimal form needs no fixup
    emitter.instruction("cmp w16, #69");                                        // ASCII 'E' starts the exponent part
    emitter.instruction("b.eq __rt_ftoa_exp");                                  // switch to the exponential fixup path
    emitter.instruction("cmp w16, #46");                                        // ASCII '.' marks an existing mantissa fraction
    emitter.instruction("b.ne __rt_ftoa_copy_store");                           // no fraction marker: just copy the byte
    emitter.instruction("mov w15, #1");                                         // record that the mantissa already has a fraction
    emitter.label("__rt_ftoa_copy_store");
    emitter.instruction("strb w16, [x12], #1");                                 // emit the mantissa byte
    emitter.instruction("add x14, x14, #1");                                    // advance the scratch read cursor
    emitter.instruction("b __rt_ftoa_copy");                                    // continue copying the mantissa

    // -- exponential form: zend_gcvt always writes a fraction, C's %G does not --
    emitter.label("__rt_ftoa_exp");
    emitter.instruction("cbnz w15, __rt_ftoa_exp_marker");                      // mantissa already has a fraction
    emitter.instruction("mov w17, #46");                                        // ASCII '.'
    emitter.instruction("strb w17, [x12], #1");                                 // emit the mandatory decimal point
    emitter.instruction("mov w17, #48");                                        // ASCII '0'
    emitter.instruction("strb w17, [x12], #1");                                 // emit the mandatory "0" fraction digit
    emitter.label("__rt_ftoa_exp_marker");
    emitter.instruction("strb w16, [x12], #1");                                 // emit the 'E' exponent marker
    emitter.instruction("add x14, x14, #1");                                    // advance past 'E' in the scratch
    emitter.instruction("ldrb w16, [x14]");                                     // load the exponent sign byte
    emitter.instruction("strb w16, [x12], #1");                                 // emit the exponent sign
    emitter.instruction("add x14, x14, #1");                                    // advance past the exponent sign

    // -- zend_gcvt writes the exponent unpadded, C's %G pads it to two digits --
    emitter.label("__rt_ftoa_exp_strip");
    emitter.instruction("ldrb w16, [x14]");                                     // load the next exponent digit
    emitter.instruction("cmp w16, #48");                                        // is it a padding ASCII '0'?
    emitter.instruction("b.ne __rt_ftoa_exp_digits");                           // significant digit: stop stripping
    emitter.instruction("ldrb w17, [x14, #1]");                                 // peek at the following byte
    emitter.instruction("cbz w17, __rt_ftoa_exp_digits");                       // keep a lone '0' as the exponent value
    emitter.instruction("add x14, x14, #1");                                    // drop one leading zero
    emitter.instruction("b __rt_ftoa_exp_strip");                               // keep stripping leading zeros

    emitter.label("__rt_ftoa_exp_digits");
    emitter.instruction("ldrb w16, [x14]");                                     // load the next exponent digit
    emitter.instruction("cbz w16, __rt_ftoa_finish");                           // NUL terminator ends the exponent
    emitter.instruction("strb w16, [x12], #1");                                 // emit the exponent digit
    emitter.instruction("add x14, x14, #1");                                    // advance the scratch read cursor
    emitter.instruction("b __rt_ftoa_exp_digits");                              // copy the remaining exponent digits

    // -- publish the result slice and advance the concat cursor --
    emitter.label("__rt_ftoa_finish");
    emitter.instruction("sub x2, x12, x13");                                    // result length = cursor - start
    emitter.instruction("mov x1, x13");                                         // result pointer = start of the emitted text
    abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // reload the original concat offset
    emitter.instruction("add x10, x10, x2");                                    // advance it past the emitted bytes
    emitter.instruction("str x10, [x9]");                                       // publish the updated concat offset

    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the `__rt_ftoa` routine for Linux x86_64.
///
/// # Input
/// - `xmm0` holds the float value (SysV variadic ABI)
///
/// # Output
/// - `rax` = pointer to formatted string, `rdx` = length
///
/// # Behavior
/// Same as `emit_ftoa` but for the Linux x86_64 target: `snprintf("%.14G", …)` into a
/// 48-byte stack scratch buffer, then the same `zend_gcvt` fixup copy into `_concat_buf`.
/// Sets `eax = 1` to signal one SIMD register argument to `snprintf`.
fn emit_ftoa_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ftoa (precision=14, PHP zend_gcvt layout) ---");
    emitter.label_global("__rt_ftoa");

    emitter.instruction("push rbp");                                            // save the caller frame pointer before using stack locals
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the formatting helper
    emitter.instruction("sub rsp, 64");                                         // reserve aligned scratch space for the snprintf result

    // -- PHP 8.5 reports a NAN coerced to string, once per conversion --
    // See the AArch64 counterpart. The helper restores `xmm0`.
    emit_nan_string_coercion_probe(emitter, "__rt_ftoa_no_nan_x");

    emitter.instruction("lea rdi, [rbp - 56]");                                 // snprintf destination = stack scratch buffer
    emitter.instruction("mov esi, 48");                                         // scratch buffer size limit
    abi::emit_symbol_address(emitter, "rdx", "_fmt_g");
    emitter.instruction("mov eax, 1");                                          // SysV variadic ABI: one SIMD register is live for the double argument
    emitter.instruction("call snprintf");                                       // format the double at 14 significant digits

    abi::emit_load_symbol_to_reg(emitter, "r9", "_concat_off", 0);              // current concat write offset
    abi::emit_symbol_address(emitter, "r8", "_concat_buf");
    emitter.instruction("lea r10, [r8 + r9]");                                  // result start = concat_buf + offset
    emitter.instruction("mov r11, r10");                                        // r11 = write cursor, r10 = result start
    emitter.instruction("lea rsi, [rbp - 56]");                                 // rsi = read cursor into the snprintf scratch
    emitter.instruction("xor ecx, ecx");                                        // ecx = "mantissa already has a '.'" flag

    emitter.instruction("movzx eax, BYTE PTR [rsi]");                           // first formatted byte
    emitter.instruction("cmp al, 45");                                          // is it an ASCII '-' sign?
    emitter.instruction("jne __rt_ftoa_nan_check_x");                           // unsigned text: inspect the first byte directly
    emitter.instruction("movzx eax, BYTE PTR [rsi + 1]");                       // signed text: inspect the byte after the sign
    emitter.label("__rt_ftoa_nan_check_x");
    emitter.instruction("cmp al, 78");                                          // ASCII 'N' can only start the NAN spelling
    emitter.instruction("jne __rt_ftoa_copy_x");                                // ordinary numeric text: run the copy/fixup loop
    emitter.instruction("mov BYTE PTR [r11], 78");                              // emit 'N'
    emitter.instruction("mov BYTE PTR [r11 + 1], 65");                          // emit 'A'
    emitter.instruction("mov BYTE PTR [r11 + 2], 78");                          // emit 'N'
    emitter.instruction("add r11, 3");                                          // advance the write cursor past "NAN"
    emitter.instruction("jmp __rt_ftoa_finish_x");                              // NAN needs no further fixups

    emitter.label("__rt_ftoa_copy_x");
    emitter.instruction("movzx eax, BYTE PTR [rsi]");                           // load the next scratch byte
    emitter.instruction("test al, al");                                         // check for the NUL terminator
    emitter.instruction("jz __rt_ftoa_finish_x");                               // decimal form needs no fixup
    emitter.instruction("cmp al, 69");                                          // ASCII 'E' starts the exponent part
    emitter.instruction("je __rt_ftoa_exp_x");                                  // switch to the exponential fixup path
    emitter.instruction("cmp al, 46");                                          // ASCII '.' marks an existing mantissa fraction
    emitter.instruction("jne __rt_ftoa_copy_store_x");                          // no fraction marker: just copy the byte
    emitter.instruction("mov ecx, 1");                                          // record that the mantissa already has a fraction
    emitter.label("__rt_ftoa_copy_store_x");
    emitter.instruction("mov BYTE PTR [r11], al");                              // emit the mantissa byte
    emitter.instruction("inc r11");                                             // advance the write cursor
    emitter.instruction("inc rsi");                                             // advance the scratch read cursor
    emitter.instruction("jmp __rt_ftoa_copy_x");                                // continue copying the mantissa

    emitter.label("__rt_ftoa_exp_x");
    emitter.instruction("test ecx, ecx");                                       // does the mantissa already have a fraction?
    emitter.instruction("jnz __rt_ftoa_exp_marker_x");                          // yes: keep it as formatted
    emitter.instruction("mov BYTE PTR [r11], 46");                              // emit the mandatory decimal point
    emitter.instruction("mov BYTE PTR [r11 + 1], 48");                          // emit the mandatory "0" fraction digit
    emitter.instruction("add r11, 2");                                          // advance the write cursor past ".0"
    emitter.label("__rt_ftoa_exp_marker_x");
    emitter.instruction("mov BYTE PTR [r11], 69");                              // emit the 'E' exponent marker
    emitter.instruction("inc r11");                                             // advance the write cursor
    emitter.instruction("inc rsi");                                             // advance past 'E' in the scratch
    emitter.instruction("movzx eax, BYTE PTR [rsi]");                           // load the exponent sign byte
    emitter.instruction("mov BYTE PTR [r11], al");                              // emit the exponent sign
    emitter.instruction("inc r11");                                             // advance the write cursor
    emitter.instruction("inc rsi");                                             // advance past the exponent sign

    emitter.label("__rt_ftoa_exp_strip_x");
    emitter.instruction("movzx eax, BYTE PTR [rsi]");                           // load the next exponent digit
    emitter.instruction("cmp al, 48");                                          // is it a padding ASCII '0'?
    emitter.instruction("jne __rt_ftoa_exp_digits_x");                          // significant digit: stop stripping
    emitter.instruction("movzx edx, BYTE PTR [rsi + 1]");                       // peek at the following byte
    emitter.instruction("test dl, dl");                                         // is the zero the last exponent digit?
    emitter.instruction("jz __rt_ftoa_exp_digits_x");                           // keep a lone '0' as the exponent value
    emitter.instruction("inc rsi");                                             // drop one leading zero
    emitter.instruction("jmp __rt_ftoa_exp_strip_x");                           // keep stripping leading zeros

    emitter.label("__rt_ftoa_exp_digits_x");
    emitter.instruction("movzx eax, BYTE PTR [rsi]");                           // load the next exponent digit
    emitter.instruction("test al, al");                                         // check for the NUL terminator
    emitter.instruction("jz __rt_ftoa_finish_x");                               // the exponent is complete
    emitter.instruction("mov BYTE PTR [r11], al");                              // emit the exponent digit
    emitter.instruction("inc r11");                                             // advance the write cursor
    emitter.instruction("inc rsi");                                             // advance the scratch read cursor
    emitter.instruction("jmp __rt_ftoa_exp_digits_x");                          // copy the remaining exponent digits

    emitter.label("__rt_ftoa_finish_x");
    emitter.instruction("mov rax, r10");                                        // result pointer = start of the emitted text
    emitter.instruction("mov rdx, r11");                                        // write cursor, one past the last byte
    emitter.instruction("sub rdx, rax");                                        // result length = cursor - start
    abi::emit_load_symbol_to_reg(emitter, "r8", "_concat_off", 0);              // reload the original concat offset
    emitter.instruction("add r8, rdx");                                         // advance it past the emitted bytes
    abi::emit_store_reg_to_symbol(emitter, "r8", "_concat_off", 0);             // publish the updated concat offset

    emitter.instruction("add rsp, 64");                                         // release the local scratch area before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return pointer+length in rax/rdx
}

/// Emits `__rt_ftoa_repr`, PHP's `serialize_precision = -1` float rendering.
///
/// This is the layout `var_dump()` prints (`%.*H` with `serialize_precision = -1`): the
/// shortest decimal string that round-trips back to the same `double`, with an uppercase
/// `E` marker, a mandatory `d.d` mantissa in exponential form, an unpadded exponent, and
/// NO trailing `.0` for integer-valued floats (`float(100)`, not `float(100.0)`).
///
/// Finite values are handed to `__rt_json_ftoa` — the tested shortest-round-trip
/// formatter shared with `json_encode`/`serialize` — with `'E'` as the exponent marker.
/// This helper only owns the three non-finite spellings, because `__rt_json_ftoa` relies
/// on its caller to filter them out.
///
/// Input: AArch64 `d0` / x86_64 `xmm0` = the double to render.
/// Output: AArch64 `x1`/`x2`, x86_64 `rax`/`rdx` = pointer/length inside `_concat_buf`,
/// with `_concat_off` advanced past the emitted bytes — the same ABI as `__rt_ftoa`.
pub fn emit_ftoa_repr(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ftoa_repr_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: ftoa_repr (serialize_precision=-1, var_dump layout) ---");
    emitter.label_global("__rt_ftoa_repr");

    // -- classify the double through its raw bits: finite, infinite, or NaN --
    emitter.instruction("fmov x9, d0");                                         // raw IEEE-754 bit pattern of the value
    emitter.instruction("and x10, x9, #0x7fffffffffffffff");                    // drop the sign bit to test the magnitude
    emitter.instruction("movz x11, #0x7ff0, lsl #48");                          // the exact bit pattern of positive infinity
    emitter.instruction("cmp x10, x11");                                        // compare the magnitude against infinity
    emitter.instruction("b.hi __rt_ftoa_repr_nan");                             // above infinity means NaN
    emitter.instruction("b.eq __rt_ftoa_repr_inf");                             // exactly infinity

    // -- finite: PHP's shortest round-trip formatter with an uppercase exponent marker --
    emitter.instruction("mov w0, #69");                                         // ASCII 'E': var_dump uses the uppercase marker
    emitter.instruction("b __rt_json_ftoa");                                    // tail-call the shared shortest-round-trip formatter

    emitter.label("__rt_ftoa_repr_nan");
    emitter.instruction("mov x9, #0");                                          // no sign byte: PHP always prints an unsigned NAN
    emitter.instruction("mov w12, #78");                                        // ASCII 'N' as the first literal byte
    emitter.instruction("mov w13, #65");                                        // ASCII 'A' as the second literal byte
    emitter.instruction("mov w14, #78");                                        // ASCII 'N' as the third literal byte
    emitter.instruction("b __rt_ftoa_repr_emit");                               // emit the three-byte literal

    emitter.label("__rt_ftoa_repr_inf");
    emitter.instruction("mov w12, #73");                                        // ASCII 'I' as the first literal byte
    emitter.instruction("mov w13, #78");                                        // ASCII 'N' as the second literal byte
    emitter.instruction("mov w14, #70");                                        // ASCII 'F' as the third literal byte

    // -- write "[-]XXX" straight into the concat buffer and publish the cursor --
    emitter.label("__rt_ftoa_repr_emit");
    abi::emit_symbol_address(emitter, "x15", "_concat_off");
    emitter.instruction("ldr x16, [x15]");                                      // current concat write offset
    abi::emit_symbol_address(emitter, "x17", "_concat_buf");
    emitter.instruction("add x1, x17, x16");                                    // result start = concat_buf + offset
    emitter.instruction("mov x11, x1");                                         // x11 = write cursor, x1 = result start
    emitter.instruction("tbz x9, #63, __rt_ftoa_repr_body");                    // skip the sign byte for non-negative values
    emitter.instruction("mov w10, #45");                                        // ASCII '-'
    emitter.instruction("strb w10, [x11], #1");                                 // emit the sign byte for -INF
    emitter.label("__rt_ftoa_repr_body");
    emitter.instruction("strb w12, [x11], #1");                                 // emit the first literal byte
    emitter.instruction("strb w13, [x11], #1");                                 // emit the second literal byte
    emitter.instruction("strb w14, [x11], #1");                                 // emit the third literal byte
    emitter.instruction("sub x2, x11, x1");                                     // result length = cursor - start
    emitter.instruction("add x16, x16, x2");                                    // advance the concat cursor past the literal
    emitter.instruction("str x16, [x15]");                                      // publish the updated concat offset
    emitter.instruction("ret");                                                 // return pointer (x1) and length (x2)
}

/// Emits the Linux x86_64 variant of `__rt_ftoa_repr`.
///
/// Mirrors the AArch64 helper: classify the double from its raw bits, tail-call
/// `__rt_json_ftoa` with `'E'` for finite values, and emit `INF` / `-INF` / `NAN` directly
/// into `_concat_buf` otherwise.
///
/// Input: `xmm0` = the double to render. Output: `rax`/`rdx` = pointer/length.
fn emit_ftoa_repr_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ftoa_repr (serialize_precision=-1, var_dump layout) ---");
    emitter.label_global("__rt_ftoa_repr");

    emitter.instruction("movq r9, xmm0");                                       // raw IEEE-754 bit pattern of the value
    emitter.instruction("movabs r10, 0x7fffffffffffffff");                      // mask that drops the sign bit
    emitter.instruction("and r10, r9");                                         // magnitude bits of the value
    emitter.instruction("movabs r11, 0x7ff0000000000000");                      // the exact bit pattern of positive infinity
    emitter.instruction("cmp r10, r11");                                        // compare the magnitude against infinity
    emitter.instruction("ja __rt_ftoa_repr_nan_x");                             // above infinity means NaN
    emitter.instruction("je __rt_ftoa_repr_inf_x");                             // exactly infinity

    emitter.instruction("mov edi, 69");                                         // ASCII 'E': var_dump uses the uppercase marker
    emitter.instruction("jmp __rt_json_ftoa");                                  // tail-call the shared shortest-round-trip formatter

    emitter.label("__rt_ftoa_repr_nan_x");
    emitter.instruction("xor r9d, r9d");                                        // no sign byte: PHP always prints an unsigned NAN
    emitter.instruction("mov ecx, 78");                                         // ASCII 'N' as the first literal byte
    emitter.instruction("mov esi, 65");                                         // ASCII 'A' as the second literal byte
    emitter.instruction("mov edi, 78");                                         // ASCII 'N' as the third literal byte
    emitter.instruction("jmp __rt_ftoa_repr_emit_x");                           // emit the three-byte literal

    emitter.label("__rt_ftoa_repr_inf_x");
    emitter.instruction("mov ecx, 73");                                         // ASCII 'I' as the first literal byte
    emitter.instruction("mov esi, 78");                                         // ASCII 'N' as the second literal byte
    emitter.instruction("mov edi, 70");                                         // ASCII 'F' as the third literal byte

    emitter.label("__rt_ftoa_repr_emit_x");
    abi::emit_load_symbol_to_reg(emitter, "r10", "_concat_off", 0);             // current concat write offset
    abi::emit_symbol_address(emitter, "r11", "_concat_buf");
    emitter.instruction("lea rax, [r11 + r10]");                                // result start = concat_buf + offset
    emitter.instruction("mov r8, rax");                                         // r8 = write cursor, rax = result start
    emitter.instruction("test r9, r9");                                         // is the sign bit set?
    emitter.instruction("jns __rt_ftoa_repr_body_x");                           // skip the sign byte for non-negative values
    emitter.instruction("mov BYTE PTR [r8], 45");                               // emit the ASCII '-' for -INF
    emitter.instruction("inc r8");                                              // advance the write cursor
    emitter.label("__rt_ftoa_repr_body_x");
    emitter.instruction("mov BYTE PTR [r8], cl");                               // emit the first literal byte
    emitter.instruction("mov BYTE PTR [r8 + 1], sil");                          // emit the second literal byte
    emitter.instruction("mov BYTE PTR [r8 + 2], dil");                          // emit the third literal byte
    emitter.instruction("add r8, 3");                                           // advance the write cursor past the literal
    emitter.instruction("mov rdx, r8");                                         // write cursor, one past the last byte
    emitter.instruction("sub rdx, rax");                                        // result length = cursor - start
    emitter.instruction("add r10, rdx");                                        // advance the concat cursor past the literal
    abi::emit_store_reg_to_symbol(emitter, "r10", "_concat_off", 0);            // publish the updated concat offset
    emitter.instruction("ret");                                                 // return pointer (rax) and length (rdx)
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::platform::{Arch, Platform, Target};

    use super::*;

    /// Verifies that `emit_ftoa` on Linux x86_64 uses the SysV variadic calling convention
    /// by checking that `eax` is set to 1 (one SIMD register argument) before calling
    /// `snprintf`, and that the fixup copy returns pointer/length in `rax`/`rdx`.
    #[test]
    fn test_emit_ftoa_linux_x86_64_uses_sysv_variadic_call() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_ftoa(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains("__rt_ftoa:\n"));
        assert!(asm.contains("mov eax, 1\n"));
        assert!(asm.contains("call snprintf\n"));
        assert!(asm.contains("sub rdx, rax\n"));
    }

    /// Verifies that both targets emit the `zend_gcvt` fixup path: the mandatory `.0`
    /// mantissa fraction (ASCII 46/48) and the exponent leading-zero strip loop.
    #[test]
    fn test_emit_ftoa_applies_php_gcvt_fixups() {
        for arch in [Arch::AArch64, Arch::X86_64] {
            let mut emitter = Emitter::new(Target::new(Platform::Linux, arch));
            emit_ftoa(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_ftoa_exp"), "missing exponential fixup for {:?}", arch);
            assert!(
                asm.contains("__rt_ftoa_exp_strip") || asm.contains("__rt_ftoa_exp_strip_x"),
                "missing exponent zero-strip for {:?}",
                arch
            );
        }
    }

    /// Verifies that `__rt_ftoa_repr` delegates finite values to the shared
    /// shortest-round-trip formatter with the uppercase `E` marker (ASCII 69) and owns the
    /// non-finite spellings itself.
    #[test]
    fn test_emit_ftoa_repr_delegates_to_json_ftoa() {
        for arch in [Arch::AArch64, Arch::X86_64] {
            let mut emitter = Emitter::new(Target::new(Platform::Linux, arch));
            emit_ftoa_repr(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_ftoa_repr:\n"), "missing entry point for {:?}", arch);
            assert!(asm.contains("__rt_json_ftoa"), "missing delegation for {:?}", arch);
            assert!(asm.contains("69"), "missing uppercase 'E' marker for {:?}", arch);
        }
    }
}
