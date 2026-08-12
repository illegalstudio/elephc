//! Purpose:
//! Emits the `__rt_number_format`, `__rt_nf_count` runtime helper assembly for number format.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - String helpers use PHP pointer/length pairs and target ABI return registers; heap-backed results must remain refcount-compatible.
//! - The result is bounded by the fixed `RAW_BUFFER_BYTES` snprintf buffer plus its grouping
//!   separators, so `GROUPED_RESULT_BYTES` is reserved through `__rt_concat_reserve` before the
//!   first store. That keeps a format that lands near the end of the 64 KiB concat scratch
//!   buffer from spilling past it into the adjacent BSS globals.
//! - `$decimals` is a PHP integer, not a digit. A negative value is not an error in PHP: the
//!   number is pre-rounded to that power of ten (half away from zero, on the magnitude) and then
//!   formatted with no decimals, so `number_format(1234.5678, -1)` is `"1,230"`. The precision
//!   actually handed to `snprintf` is therefore always in `0..=MAX_FORMAT_PRECISION` and is
//!   written as two ASCII digits; the previous single-digit `'0' + N` shortcut turned `-1` into
//!   `"%./f"` and `10` into `"%.:f"`, which is where the `"/f"` garbage came from.
//! - `snprintf` returns the length it *would* have written, so that return value is clamped to
//!   the buffer capacity before the grouping pass copies from it. Without both the wider buffer
//!   and that clamp, a wide number read past the old 48-byte buffer into the adjacent frame
//!   slots and rendered the trailing digits from whatever was there.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Bytes of the fixed on-stack buffer `snprintf` renders the ungrouped number into.
///
/// A `double` needs at most 309 integer digits, so 384 bytes holds the widest possible
/// integer part plus the decimal point plus `MAX_FORMAT_PRECISION` decimals without ever
/// truncating.
const RAW_BUFFER_BYTES: i64 = 384;

/// Bytes reserved through `__rt_concat_reserve` for the grouped result.
///
/// The widest raw render plus one thousands separator per three integer digits.
const GROUPED_RESULT_BYTES: i64 = 512;

/// Highest `$decimals` value `snprintf` is asked for.
///
/// Two ASCII precision digits allow `0..=99`; the cap keeps the widest possible render
/// (309 integer digits + `.` + this many decimals) inside `RAW_BUFFER_BYTES`, and the raw
/// length is clamped again after `snprintf` returns as a belt-and-braces bound.
const MAX_FORMAT_PRECISION: i64 = 40;

/// Emits the `__rt_number_format` runtime helper.
///
/// Formats a floating-point number with configurable decimal places and separators,
/// writing the result into storage reserved through `__rt_concat_reserve` and publishing the
/// written length through `__rt_concat_publish`. Dispatches to target-specific implementations.
///
/// Input registers (ARM64): `d0` = number, `x1` = decimals, `x2` = dec_point char, `x3` = thousands_sep (0=none)
/// Output registers (ARM64): `x1` = string pointer, `x2` = string length
/// Input registers (x86_64 SysV): `xmm0` = number, `rdi` = decimals, `rsi` = dec_point, `rdx` = thousands_sep
/// Output registers (x86_64 SysV): `rax` = string pointer, `rdx` = string length
///
/// Stack frame layout (ARM64, 512 bytes):
///   `[sp+48]`     pre-round magnitude scratch (negative `$decimals` only)
///   `[sp+56]`     pre-round sign flag (negative `$decimals` only)
///   `[sp+64..69]` format string `"%.NNf\0"`
///   `[sp+72]`     result start ptr
///   `[sp+80]`     raw snprintf length
///   `[sp+88]`     number (double)
///   `[sp+96]`     decimals
///   `[sp+104]`    dec_point char (one byte)
///   `[sp+105]`    thousands_sep char (one byte)
///   `[sp+112]`    saved x29, x30
///   `[sp+128..511]` snprintf buffer (`RAW_BUFFER_BYTES`)
pub fn emit_number_format(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_number_format_linux_x86_64(emitter);
        return;
    }

    // Stack frame layout (512 bytes):
    //   [sp+48]     pre-round magnitude / scale scratch (negative $decimals only)
    //   [sp+56]     pre-round sign flag (negative $decimals only)
    //   [sp+64..69] format string "%.NNf\0"
    //   [sp+72]     result start ptr
    //   [sp+80]     raw_len
    //   [sp+88]     number (d0)
    //   [sp+96]     decimals
    //   [sp+104]    dec_point char (one byte)
    //   [sp+105]    thousands_sep char (one byte)
    //   [sp+112]    saved x29, x30
    //   [sp+128..511] snprintf buffer (RAW_BUFFER_BYTES)
    emitter.blank();
    emitter.comment("--- runtime: number_format ---");
    emitter.label_global("__rt_number_format");

    // -- set up stack frame (512 bytes) --
    emitter.instruction("sub sp, sp, #512");                                    // allocate the number_format() frame: metadata low, raw snprintf buffer high
    emitter.instruction("stp x29, x30, [sp, #112]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #112");                                   // establish new frame pointer

    // -- save input arguments --
    emitter.instruction("str x1, [sp, #96]");                                   // save decimals count
    emitter.instruction("str d0, [sp, #88]");                                   // save the floating-point number
    emitter.instruction("strb w2, [sp, #104]");                                 // save decimal point character as a byte so it cannot overlap the decimals slot
    emitter.instruction("strb w3, [sp, #105]");                                 // save thousands separator character as its own byte

    // -- negative $decimals: pre-round the magnitude to that power of ten, then use no decimals --
    emitter.instruction("ldr x9, [sp, #96]");                                   // load the requested decimals count
    emitter.instruction("cmp x9, #0");                                          // is the caller asking for fewer significant digits?
    emitter.instruction("b.ge __rt_nf_precision_ready");                        // a non-negative precision goes straight to snprintf
    emitter.instruction("ldr d0, [sp, #88]");                                   // reload the caller's number
    emitter.instruction("fabs d1, d0");                                         // PHP rounds the magnitude, then reapplies the sign
    emitter.instruction("str d1, [sp, #48]");                                   // park the magnitude across the libm calls
    emitter.instruction("fcmp d0, #0.0");                                       // was the caller's number negative?
    emitter.instruction("cset x10, mi");                                        // remember the sign so it can be restored after rounding
    emitter.instruction("str x10, [sp, #56]");                                  // park the sign flag across the libm calls
    emitter.instruction("neg x9, x9");                                          // the power of ten to round to is -$decimals
    emitter.instruction("scvtf d1, x9");                                        // pass that power as the libm pow() exponent
    emitter.instruction("mov x10, #10");                                        // the rounding base is ten
    emitter.instruction("scvtf d0, x10");                                       // pass the base as the libm pow() mantissa argument
    emitter.bl_c("pow");                                                        // d0 = 10 ** -$decimals
    emitter.instruction("ldr d1, [sp, #48]");                                   // reload the parked magnitude
    emitter.instruction("fdiv d1, d1, d0");                                     // scale the magnitude down to the requested precision
    emitter.instruction("str d0, [sp, #48]");                                   // park the scale for the rescale step
    emitter.instruction("fmov d2, #0.5");                                       // half-away-from-zero rounding adds a half before flooring
    emitter.instruction("fadd d0, d1, d2");                                     // bias the scaled magnitude for PHP_ROUND_HALF_UP
    emitter.instruction("frintm d0, d0");                                       // floor the biased magnitude, matching PHP on exact halves
    emitter.instruction("fcmp d0, #0.0");                                       // did the requested precision round the value away entirely?
    emitter.instruction("b.eq __rt_nf_precision_zero");                         // PHP prints a plain "0", never "-0", in that case
    emitter.instruction("ldr d1, [sp, #48]");                                   // reload the parked scale
    emitter.instruction("fmul d0, d0, d1");                                     // rescale the rounded magnitude back up
    emitter.instruction("ldr x10, [sp, #56]");                                  // reload the parked sign flag
    emitter.instruction("cbz x10, __rt_nf_precision_zero");                     // a positive number needs no sign restored
    emitter.instruction("fneg d0, d0");                                         // restore the caller's sign on the rounded magnitude
    emitter.label("__rt_nf_precision_zero");
    emitter.instruction("str d0, [sp, #88]");                                   // publish the pre-rounded number for snprintf
    emitter.instruction("str xzr, [sp, #96]");                                  // a pre-rounded value is formatted with no decimals
    emitter.label("__rt_nf_precision_ready");

    // -- build format string "%.NNf" at [sp+64] --
    emitter.instruction("mov w9, #37");                                         // ASCII '%'
    emitter.instruction("strb w9, [sp, #64]");                                  // write '%' to format string
    emitter.instruction("mov w9, #46");                                         // ASCII '.'
    emitter.instruction("strb w9, [sp, #65]");                                  // write '.' to format string
    emitter.instruction("ldr x9, [sp, #96]");                                   // load the now non-negative decimals count
    emitter.instruction(&format!("cmp x9, #{}", MAX_FORMAT_PRECISION));         // cap the precision at what the raw buffer can hold
    emitter.instruction("b.le __rt_nf_precision_capped");                       // keep the requested precision when it already fits
    emitter.instruction(&format!("mov x9, #{}", MAX_FORMAT_PRECISION));         // clamp an over-wide precision to the buffer limit
    emitter.label("__rt_nf_precision_capped");
    emitter.instruction("mov x10, #10");                                        // split the precision into two ASCII digits
    emitter.instruction("udiv x11, x9, x10");                                   // x11 = tens digit of the precision
    emitter.instruction("msub x12, x11, x10, x9");                              // x12 = units digit of the precision
    emitter.instruction("add w11, w11, #48");                                   // convert the tens digit to ASCII
    emitter.instruction("strb w11, [sp, #66]");                                 // write the tens precision digit
    emitter.instruction("add w12, w12, #48");                                   // convert the units digit to ASCII
    emitter.instruction("strb w12, [sp, #67]");                                 // write the units precision digit
    emitter.instruction("mov w9, #102");                                        // ASCII 'f'
    emitter.instruction("strb w9, [sp, #68]");                                  // write 'f' format specifier
    emitter.instruction("strb wzr, [sp, #69]");                                 // null-terminate the format string

    // -- call snprintf(buf, 48, fmt, d0) --
    emitter.instruction("add x0, sp, #128");                                    // x0 = the raw snprintf buffer above the frame metadata
    emitter.instruction(&format!("mov x1, #{}", RAW_BUFFER_BYTES));             // bound the raw snprintf buffer
    emitter.instruction("add x2, sp, #64");                                     // x2 = format string pointer
    emitter.instruction("ldr d0, [sp, #88]");                                   // reload the float value
    emitter.instruction("str d0, [sp, #-16]!");                                 // push double for variadic ABI, adjust sp
    emitter.bl_c("snprintf");                                        // call snprintf; returns char count in x0
    emitter.instruction("add sp, sp, #16");                                     // pop the variadic argument from stack
    emitter.instruction(&format!("cmp x0, #{}", RAW_BUFFER_BYTES - 1));         // snprintf reports the untruncated length, which may exceed the buffer
    emitter.instruction("b.le __rt_nf_raw_len_ok");                             // keep the reported length when it actually fits
    emitter.instruction(&format!("mov x0, #{}", RAW_BUFFER_BYTES - 1));         // never scan past the raw buffer for a truncated result
    emitter.label("__rt_nf_raw_len_ok");
    emitter.instruction("str x0, [sp, #80]");                                   // save raw string length

    // -- reserve bounded destination storage (48 raw bytes plus grouping separators) --
    emitter.instruction(&format!("mov x0, #{}", GROUPED_RESULT_BYTES));         // the raw snprintf buffer plus its thousands separators can never exceed this
    emitter.instruction("bl __rt_concat_reserve");                              // reserve scratch or heap storage for the grouped number
    emitter.instruction("mov x10, x0");                                         // compute destination pointer
    emitter.instruction("str x10, [sp, #72]");                                  // save result start pointer

    // -- scan raw string to find integer part length --
    emitter.instruction("add x11, sp, #128");                                   // x11 = source ptr (snprintf output)
    emitter.instruction("ldr x12, [sp, #80]");                                  // x12 = raw string length
    emitter.instruction("mov x13, #0");                                         // x13 = integer part digit count

    // -- handle leading minus sign --
    emitter.instruction("ldrb w14, [x11]");                                     // load first character
    emitter.instruction("cmp w14, #45");                                        // check if it's '-' (minus sign)
    emitter.instruction("b.ne __rt_nf_count");                                  // skip if not negative
    emitter.instruction("strb w14, [x10], #1");                                 // copy '-' to output, advance dest
    emitter.instruction("add x11, x11, #1");                                    // advance source past '-'
    emitter.instruction("sub x12, x12, #1");                                    // decrement remaining length

    // -- count integer digits (before decimal point) --
    emitter.label("__rt_nf_count");
    emitter.instruction("mov x15, x11");                                        // save start of integer digits
    emitter.instruction("mov x13, #0");                                         // reset digit counter
    emitter.label("__rt_nf_count_loop");
    emitter.instruction("cbz x12, __rt_nf_count_done");                         // if no chars remain, done counting
    emitter.instruction("ldrb w14, [x11, x13]");                                // load char at current offset
    emitter.instruction("cmp w14, #46");                                        // check if it's '.' (decimal point)
    emitter.instruction("b.eq __rt_nf_count_done");                             // stop counting at decimal point
    emitter.instruction("add x13, x13, #1");                                    // increment integer digit count
    emitter.instruction("sub x12, x12, #1");                                    // decrement remaining chars
    emitter.instruction("b __rt_nf_count_loop");                                // continue scanning

    emitter.label("__rt_nf_count_done");
    // x13=int digit count, x15=start of digits, x12=remaining (decimal part)

    // -- copy integer digits with thousands separator --
    emitter.instruction("mov x16, #0");                                         // source index into integer digits
    emitter.instruction("mov x17, #3");                                         // group size for thousands
    // The quotient lands straight in x14 and `msub` reads it back as a source, so this needs
    // no scratch register at all — x18 is reserved for the OS on Apple AArch64, and every
    // register free here is already carrying loop state.
    emitter.instruction("udiv x14, x13, x17");                                  // number of complete 3-digit groups
    emitter.instruction("msub x14, x14, x17, x13");                             // first group size = digit_count % 3
    emitter.instruction("cbnz x14, __rt_nf_copy_int");                          // if first group non-empty, start copying
    emitter.instruction("mov x14, #3");                                         // first group is full 3 digits

    emitter.label("__rt_nf_copy_int");
    emitter.instruction("cmp x16, x13");                                        // check if all integer digits copied
    emitter.instruction("b.ge __rt_nf_decimal");                                // if done, move to decimal part

    // -- insert thousands separator between groups --
    emitter.instruction("cbz x16, __rt_nf_no_sep");                             // skip separator before first digit
    emitter.instruction("cmp x14, #0");                                         // check if current group is exhausted
    emitter.instruction("b.ne __rt_nf_no_sep");                                 // group not done, no separator yet
    emitter.instruction("ldrb w9, [sp, #105]");                                 // load thousands separator char
    emitter.instruction("cbz x9, __rt_nf_no_sep_reset");                        // skip if separator is 0 (none)
    emitter.instruction("strb w9, [x10], #1");                                  // write separator to output, advance dest
    emitter.label("__rt_nf_no_sep_reset");
    emitter.instruction("mov x14, #3");                                         // reset group counter for next 3 digits

    emitter.label("__rt_nf_no_sep");
    emitter.instruction("ldrb w9, [x15, x16]");                                 // load next integer digit from source
    emitter.instruction("strb w9, [x10], #1");                                  // write digit to output, advance dest
    emitter.instruction("add x16, x16, #1");                                    // advance source index
    emitter.instruction("sub x14, x14, #1");                                    // decrement group counter
    emitter.instruction("b __rt_nf_copy_int");                                  // continue copying integer digits

    // -- copy decimal part, replacing '.' with custom separator --
    emitter.label("__rt_nf_decimal");
    emitter.instruction("add x15, x15, x13");                                   // advance source past integer digits
    emitter.label("__rt_nf_copy_dec");
    emitter.instruction("cbz x12, __rt_nf_done");                               // if no decimal chars remain, done
    emitter.instruction("ldrb w9, [x15], #1");                                  // load next decimal char, advance source
    emitter.instruction("cmp w9, #46");                                         // check if it's '.' (snprintf decimal point)
    emitter.instruction("b.ne __rt_nf_dec_store");                              // if not '.', store as-is
    emitter.instruction("ldrb w9, [sp, #104]");                                 // replace with custom decimal point char
    emitter.label("__rt_nf_dec_store");
    emitter.instruction("strb w9, [x10], #1");                                  // write char to output, advance dest
    emitter.instruction("sub x12, x12, #1");                                    // decrement remaining chars
    emitter.instruction("b __rt_nf_copy_dec");                                  // continue copying decimal part

    // -- finalize: compute length and publish the written bytes --
    emitter.label("__rt_nf_done");
    emitter.instruction("ldr x1, [sp, #72]");                                   // load result start pointer
    emitter.instruction("sub x2, x10, x1");                                     // result length = dest_end - dest_start
    emitter.instruction("bl __rt_concat_publish");                              // advance the concat scratch offset only for scratch-backed results

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #512");                                    // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// ARM64 implementation of the `__rt_number_format` runtime helper.
fn emit_number_format_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: number_format ---");
    emitter.label_global("__rt_number_format");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving local number_format() scratch space
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the raw snprintf buffer, mini format string, and concat-buffer state
    emitter.instruction("push rbx");                                            // preserve the concat-buffer destination cursor across the local formatting and copy loops
    emitter.instruction("push r12");                                            // preserve the concat-buffer start pointer for the final x86_64 string return pair
    emitter.instruction("push r13");                                            // preserve one more callee-saved register so the frame stays 16-byte aligned for the SysV snprintf call
    emitter.instruction("sub rsp, 488");                                        // reserve local storage; the four 8-byte saves above plus this sub leave rsp 0-mod-16 before the SysV snprintf call below
    emitter.instruction("mov QWORD PTR [rbp - 56], rdi");                       // preserve the requested decimal count across the intermediate formatting and copy loops
    emitter.instruction("mov QWORD PTR [rbp - 48], rsi");                       // preserve the decimal-separator byte across the intermediate formatting and copy loops
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // preserve the thousands-separator byte across the intermediate formatting and copy loops
    emitter.instruction("movsd QWORD PTR [rbp - 128], xmm0");                   // park the caller's number so the pre-round libm calls cannot lose it

    // -- negative $decimals: pre-round the magnitude to that power of ten, then use no decimals --
    emitter.instruction("cmp QWORD PTR [rbp - 56], 0");                         // is the caller asking for fewer significant digits?
    emitter.instruction("jge __rt_nf_precision_ready_linux_x86_64");            // a non-negative precision goes straight to snprintf
    emitter.instruction("movq rax, xmm0");                                      // inspect the raw double bits to split off the sign
    emitter.instruction("mov r9, rax");                                         // copy the bits before the sign bit is cleared
    emitter.instruction("shr r9, 63");                                          // isolate the sign bit as a 0/1 flag
    emitter.instruction("mov QWORD PTR [rbp - 32], r9");                        // park the sign flag so it can be reapplied after rounding
    emitter.instruction("btr rax, 63");                                         // clear the sign bit to obtain the magnitude, which PHP rounds
    emitter.instruction("movq xmm0, rax");                                      // move the magnitude back into the floating-point register
    emitter.instruction("movsd QWORD PTR [rbp - 128], xmm0");                   // park the magnitude across the libm calls
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // reload the negative decimals count
    emitter.instruction("neg rax");                                             // the power of ten to round to is -$decimals
    emitter.instruction("cvtsi2sd xmm1, rax");                                  // pass that power as the libm pow() exponent
    emitter.instruction("mov eax, 10");                                         // the rounding base is ten
    emitter.instruction("cvtsi2sd xmm0, eax");                                  // pass the base as the libm pow() mantissa argument
    emitter.bl_c("pow");                                                        // xmm0 = 10 ** -$decimals
    emitter.instruction("movsd xmm2, QWORD PTR [rbp - 128]");                   // reload the parked magnitude
    emitter.instruction("divsd xmm2, xmm0");                                    // scale the magnitude down to the requested precision
    emitter.instruction("movsd QWORD PTR [rbp - 128], xmm0");                   // park the scale for the rescale step
    emitter.instruction("mov eax, 1");                                          // build 0.5 without an immediate double load
    emitter.instruction("cvtsi2sd xmm1, eax");                                  // xmm1 = 1.0
    emitter.instruction("mov eax, 2");                                          // the divisor that turns 1.0 into 0.5
    emitter.instruction("cvtsi2sd xmm3, eax");                                  // xmm3 = 2.0
    emitter.instruction("divsd xmm1, xmm3");                                    // xmm1 = 0.5, the half-away-from-zero bias
    emitter.instruction("addsd xmm2, xmm1");                                    // bias the scaled magnitude for PHP_ROUND_HALF_UP
    emitter.instruction("movapd xmm0, xmm2");                                   // hand the biased magnitude to libm floor()
    emitter.bl_c("floor");                                                      // floor the biased magnitude, matching PHP on exact halves
    emitter.instruction("xorpd xmm1, xmm1");                                    // build a zero to test the rounded magnitude against
    emitter.instruction("ucomisd xmm0, xmm1");                                  // did the requested precision round the value away entirely?
    emitter.instruction("je __rt_nf_precision_zero_linux_x86_64");              // PHP prints a plain "0", never "-0", in that case
    emitter.instruction("mulsd xmm0, QWORD PTR [rbp - 128]");                   // rescale the rounded magnitude back up
    emitter.instruction("cmp QWORD PTR [rbp - 32], 0");                         // was the caller's number negative?
    emitter.instruction("je __rt_nf_precision_zero_linux_x86_64");              // a positive number needs no sign restored
    emitter.instruction("movq rax, xmm0");                                      // inspect the rounded magnitude bits to restore the sign
    emitter.instruction("btc rax, 63");                                         // flip the sign bit back on for a negative input
    emitter.instruction("movq xmm0, rax");                                      // move the signed rounded value back into the floating-point register
    emitter.label("__rt_nf_precision_zero_linux_x86_64");
    emitter.instruction("movsd QWORD PTR [rbp - 128], xmm0");                   // publish the pre-rounded number for snprintf
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // a pre-rounded value is formatted with no decimals
    emitter.label("__rt_nf_precision_ready_linux_x86_64");

    emitter.instruction("mov BYTE PTR [rbp - 72], 37");                         // seed the mini format string with the leading '%' introducer
    emitter.instruction("mov BYTE PTR [rbp - 71], 46");                         // append the '.' precision introducer to the mini format string
    emitter.instruction("mov r8, QWORD PTR [rbp - 56]");                        // reload the now non-negative decimal count before converting it to ASCII
    emitter.instruction(&format!("cmp r8, {}", MAX_FORMAT_PRECISION));          // cap the precision at what the raw buffer can hold
    emitter.instruction("jle __rt_nf_precision_capped_linux_x86_64");           // keep the requested precision when it already fits
    emitter.instruction(&format!("mov r8, {}", MAX_FORMAT_PRECISION));          // clamp an over-wide precision to the buffer limit
    emitter.label("__rt_nf_precision_capped_linux_x86_64");
    emitter.instruction("mov rax, r8");                                         // split the precision into two ASCII digits
    emitter.instruction("xor rdx, rdx");                                        // clear the high dividend half before the digit split
    emitter.instruction("mov r9, 10");                                          // the digit-split divisor
    emitter.instruction("div r9");                                              // rax = tens digit, rdx = units digit
    emitter.instruction("add al, 48");                                          // convert the tens digit to ASCII
    emitter.instruction("mov BYTE PTR [rbp - 70], al");                         // append the tens precision digit to the mini format string
    emitter.instruction("mov rax, rdx");                                        // move the units digit into the byte-addressable accumulator
    emitter.instruction("add al, 48");                                          // convert the units digit to ASCII
    emitter.instruction("mov BYTE PTR [rbp - 69], al");                         // append the units precision digit to the mini format string
    emitter.instruction("mov BYTE PTR [rbp - 68], 102");                        // append the trailing 'f' format type so snprintf renders a fixed-point decimal string
    emitter.instruction("mov BYTE PTR [rbp - 67], 0");                          // null-terminate the mini format string before handing it to snprintf
    emitter.instruction("lea rdi, [rbp - 512]");                                // point snprintf at the fixed local raw-decimal buffer that will be post-processed for thousands separators
    emitter.instruction(&format!("mov esi, {}", RAW_BUFFER_BYTES));             // bound the raw-decimal buffer before the variadic snprintf call
    emitter.instruction("lea rdx, [rbp - 72]");                                 // pass the mini format string to snprintf as the fixed-point format pointer
    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 128]");                   // reload the number, which the pre-round path may have replaced
    emitter.instruction("mov eax, 1");                                          // advertise one live SIMD variadic register because the formatted number is passed in xmm0 on SysV x86_64
    emitter.bl_c("snprintf");                                                   // render the raw fixed-point decimal string into the local snprintf buffer
    emitter.instruction(&format!("cmp rax, {}", RAW_BUFFER_BYTES - 1));         // snprintf reports the untruncated length, which may exceed the buffer
    emitter.instruction("jle __rt_nf_raw_len_ok_linux_x86_64");                 // keep the reported length when it actually fits
    emitter.instruction(&format!("mov rax, {}", RAW_BUFFER_BYTES - 1));         // never scan past the raw buffer for a truncated result
    emitter.label("__rt_nf_raw_len_ok_linux_x86_64");
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // preserve the raw snprintf byte count before the thousands-separator pass consumes caller-saved registers
    emitter.instruction(&format!("mov rax, {}", GROUPED_RESULT_BYTES));         // the raw snprintf buffer plus its thousands separators can never exceed this
    emitter.instruction("call __rt_concat_reserve");                            // reserve scratch or heap storage for the grouped number
    emitter.instruction("mov rbx, rax");                                        // compute the destination cursor where the formatted output will begin
    emitter.instruction("mov r12, rbx");                                        // preserve the reserved start pointer for the final x86_64 string return pair
    emitter.instruction("lea r10, [rbp - 512]");                                // point at the raw snprintf output buffer before scanning for a leading minus sign and decimal point
    emitter.instruction("mov rcx, QWORD PTR [rbp - 64]");                       // reload the raw snprintf byte count before splitting the integer and decimal parts
    emitter.instruction("movzx eax, BYTE PTR [r10]");                           // peek at the first raw formatted byte to detect a leading minus sign
    emitter.instruction("cmp al, 45");                                          // is the first raw formatted byte the leading '-' sign?
    emitter.instruction("jne __rt_nf_count_linux_x86_64");                      // skip the sign-copy fast path when the formatted number is non-negative
    emitter.instruction("mov BYTE PTR [rbx], al");                              // copy the leading minus sign into the concat buffer before processing the remaining digits
    emitter.instruction("add rbx, 1");                                          // advance the concat-buffer destination cursor after copying the leading minus sign
    emitter.instruction("add r10, 1");                                          // advance the raw formatted cursor past the copied leading minus sign
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining raw formatted byte count after removing the leading minus sign

    emitter.label("__rt_nf_count_linux_x86_64");
    emitter.instruction("mov r11, r10");                                        // preserve the start of the integer digit run before scanning forward to the decimal point
    emitter.instruction("xor esi, esi");                                        // start counting integer digits from zero before the decimal-point scan
    emitter.label("__rt_nf_count_loop_linux_x86_64");
    emitter.instruction("test rcx, rcx");                                       // have all remaining raw formatted bytes already been scanned for the decimal point?
    emitter.instruction("jz __rt_nf_count_done_linux_x86_64");                  // stop scanning once the raw formatted string has been fully consumed
    emitter.instruction("movzx eax, BYTE PTR [r10 + rsi]");                     // load the next raw formatted byte from the candidate integer-digit run
    emitter.instruction("cmp al, 46");                                          // is the current raw formatted byte the '.' decimal-point separator from snprintf?
    emitter.instruction("je __rt_nf_count_done_linux_x86_64");                  // stop counting integer digits once the decimal-point separator is reached
    emitter.instruction("add rsi, 1");                                          // count one more integer digit before continuing the decimal-point scan
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining raw formatted byte count after consuming one integer digit
    emitter.instruction("jmp __rt_nf_count_loop_linux_x86_64");                 // continue scanning the integer digit run until the decimal point or end of string is reached

    emitter.label("__rt_nf_count_done_linux_x86_64");
    emitter.instruction("xor edi, edi");                                        // start copying integer digits from logical index zero before inserting thousands separators
    emitter.instruction("mov rax, rsi");                                        // copy the integer-digit count into the dividend register before computing the leading group width
    emitter.instruction("xor edx, edx");                                        // clear the high dividend half before dividing the integer-digit count by the fixed group size
    emitter.instruction("mov r9d, 3");                                          // load the fixed thousands-group width into a scratch divisor register without clobbering the remaining-byte counter
    emitter.instruction("div r9");                                              // divide the integer-digit count by three so the remainder becomes the leading-group width
    emitter.instruction("mov r8, rdx");                                         // preserve the leading-group width remainder before the integer-copy loop mutates general-purpose registers
    emitter.instruction("test r8, r8");                                         // did the integer-digit count divide evenly into 3-digit groups?
    emitter.instruction("jnz __rt_nf_copy_int_linux_x86_64");                   // keep the remainder-derived leading-group width when the first group is shorter than three digits
    emitter.instruction("mov r8, 3");                                           // default the leading-group width to three digits when the integer-digit count is an exact multiple of three

    emitter.label("__rt_nf_copy_int_linux_x86_64");
    emitter.instruction("cmp rdi, rsi");                                        // have all integer digits already been copied into the concat buffer?
    emitter.instruction("jge __rt_nf_decimal_linux_x86_64");                    // move on to the decimal-part copy once the integer digit run has been fully emitted
    emitter.instruction("test rdi, rdi");                                       // is the current integer digit still part of the leading group?
    emitter.instruction("jz __rt_nf_no_sep_linux_x86_64");                      // skip separator insertion before the first emitted integer digit
    emitter.instruction("test r8, r8");                                         // has the current thousands group been exhausted exactly at this copy position?
    emitter.instruction("jnz __rt_nf_no_sep_linux_x86_64");                     // skip separator insertion until the current thousands group has been exhausted
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the configured thousands-separator byte before deciding whether to emit it
    emitter.instruction("test r9, r9");                                         // is thousands grouping disabled because the configured separator byte is zero?
    emitter.instruction("jz __rt_nf_no_sep_reset_linux_x86_64");                // skip emitting a separator when the caller requested no thousands separator
    emitter.instruction("mov BYTE PTR [rbx], r9b");                             // append the configured thousands-separator byte to the concat buffer
    emitter.instruction("add rbx, 1");                                          // advance the concat-buffer destination cursor after inserting one thousands separator byte
    emitter.label("__rt_nf_no_sep_reset_linux_x86_64");
    emitter.instruction("mov r8, 3");                                           // reset the remaining width of the next thousands group after crossing a group boundary

    emitter.label("__rt_nf_no_sep_linux_x86_64");
    emitter.instruction("movzx eax, BYTE PTR [r11 + rdi]");                     // load the next integer digit from the raw snprintf buffer
    emitter.instruction("mov BYTE PTR [rbx], al");                              // append the next integer digit to the concat buffer
    emitter.instruction("add rbx, 1");                                          // advance the concat-buffer destination cursor after copying one integer digit
    emitter.instruction("add rdi, 1");                                          // advance the logical integer-digit index after copying one integer digit
    emitter.instruction("sub r8, 1");                                           // consume one slot from the current thousands-group width after copying one integer digit
    emitter.instruction("jmp __rt_nf_copy_int_linux_x86_64");                   // continue copying integer digits until the full integer run has been emitted

    emitter.label("__rt_nf_decimal_linux_x86_64");
    emitter.instruction("add r11, rsi");                                        // advance the raw formatted cursor to the first decimal-part byte after the integer run
    emitter.label("__rt_nf_copy_dec_linux_x86_64");
    emitter.instruction("test rcx, rcx");                                       // have all remaining decimal-part bytes already been copied into the concat buffer?
    emitter.instruction("jz __rt_nf_done_linux_x86_64");                        // finish once the full decimal part has been copied or omitted
    emitter.instruction("movzx eax, BYTE PTR [r11]");                           // load the next raw decimal-part byte before checking whether it is the snprintf decimal point
    emitter.instruction("add r11, 1");                                          // advance the raw formatted cursor after loading one decimal-part byte
    emitter.instruction("cmp al, 46");                                          // is the current decimal-part byte the '.' decimal-point separator emitted by snprintf?
    emitter.instruction("jne __rt_nf_store_dec_linux_x86_64");                  // copy non-decimal-point bytes directly into the concat buffer without translation
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // reload the configured decimal-separator byte before replacing the snprintf decimal point
    emitter.instruction("mov eax, r9d");                                        // replace the raw snprintf decimal-point byte with the configured decimal-separator byte
    emitter.label("__rt_nf_store_dec_linux_x86_64");
    emitter.instruction("mov BYTE PTR [rbx], al");                              // append the current decimal-part byte to the concat buffer after any separator translation
    emitter.instruction("add rbx, 1");                                          // advance the concat-buffer destination cursor after copying one decimal-part byte
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining raw decimal-part byte count after copying one byte
    emitter.instruction("jmp __rt_nf_copy_dec_linux_x86_64");                   // continue copying the decimal part until every remaining raw byte has been emitted

    emitter.label("__rt_nf_done_linux_x86_64");
    emitter.instruction("mov rax, r12");                                        // return the reserved start pointer of the formatted number in the primary x86_64 string result register
    emitter.instruction("mov rdx, rbx");                                        // copy the destination end cursor so the final formatted-string length can be derived
    emitter.instruction("sub rdx, rax");                                        // derive the formatted-string length from the destination start and end cursors
    emitter.instruction("call __rt_concat_publish");                            // advance the concat scratch offset only for scratch-backed results
    emitter.instruction("add rsp, 488");                                        // release the local raw-buffer and mini-format scratch space before restoring callee-saved registers
    emitter.instruction("pop r13");                                             // restore the callee-saved register kept only to preserve the frame's 16-byte alignment
    emitter.instruction("pop r12");                                             // restore the saved concat-buffer start register after the x86_64 number_format() helper finishes
    emitter.instruction("pop rbx");                                             // restore the saved concat-buffer destination cursor register after the x86_64 number_format() helper finishes
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the x86_64 formatted string pair
    emitter.instruction("ret");                                                 // return the formatted string pointer and length in the standard x86_64 string result registers
}
