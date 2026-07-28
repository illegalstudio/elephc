//! Purpose:
//! Emits the `__rt_sprintf`, `__rt_sprintf_loop` runtime helper assembly for sprintf formatting.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - Formatting helpers parse format strings and marshal values through target ABI calls or emitted formatting paths.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::{Arch, Platform};

use super::sprintf_x86_64::emit_sprintf_linux_x86_64;

/// Emits the `__rt_sprintf` global runtime helper for sprintf-style formatting.
/// Uses x0=arg_count, x1=fmt_ptr, x2=fmt_len on entry; args pushed on stack (16 bytes each).
/// Returns x1=result_ptr, x2=result_len in concat_buf. Updates `_concat_off` atomically.
///
/// Each stack argument is [value, type_tag] where type_tag: 0=int, 1=str(len<<8), 2=float, 3=bool.
/// The runtime pops arg_count*16 bytes from the caller's stack before returning.
///
/// Callee-saved registers used: x19=fmt_ptr, x20=fmt_remaining_len, x21=arg_index,
/// x22=args_base, x23=dest_ptr, x24=result_start, x25=concat_off_ptr, x26=arg_count.
///
/// Delegates format specifier processing (flags, width, precision, type char) to libc snprintf
/// for correct handling. On Apple ARM64, variadic arguments for snprintf are passed at [sp].
pub fn emit_sprintf(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_sprintf_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: sprintf ---");
    emitter.label_global("__rt_sprintf");

    // Frame layout (288 bytes):
    //   sp+0..7     = variadic arg slot for snprintf (must be at sp)
    //   sp+8..15    = (padding for 16-byte alignment of variadic)
    //   sp+16..23   = saved x19
    //   sp+24..31   = saved x20
    //   sp+32..39   = saved x21
    //   sp+40..47   = saved x22
    //   sp+48..55   = saved x23
    //   sp+56..63   = saved x24
    //   sp+64..71   = saved x25
    //   sp+72..79   = saved x26
    //   sp+80..111  = mini format string buffer (32 bytes)
    //   sp+112..239 = snprintf output buffer (128 bytes)
    //   sp+240..367 = string null-term copy buffer (128 bytes)
    //   sp+368..375 = saved x29
    //   sp+376..383 = saved x30
    //
    // Callee-saved register usage:
    //   x19 = fmt_ptr (current position in format string)
    //   x20 = fmt_remaining_len
    //   x21 = arg_index
    //   x22 = args_base pointer (points to pushed args from caller)
    //   x23 = dest pointer (current write position in concat_buf)
    //   x24 = result_start pointer (beginning of result in concat_buf)
    //   x25 = concat_off pointer
    //   x26 = arg_count

    emitter.instruction("sub sp, sp, #384");                                    // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #368]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #368");                                   // set frame pointer
    emitter.instruction("mov x9, #-1");                                         // no positional specifier is active yet
    emitter.instruction("str x9, [sp, #240]");                                  // slot freed by the native %s path: saved sequential index

    // -- save callee-saved registers --
    emitter.instruction("stp x19, x20, [sp, #16]");                             // save x19, x20
    emitter.instruction("stp x21, x22, [sp, #32]");                             // save x21, x22
    emitter.instruction("stp x23, x24, [sp, #48]");                             // save x23, x24
    emitter.instruction("stp x25, x26, [sp, #64]");                             // save x25, x26

    // -- initialize state in callee-saved registers --
    emitter.instruction("mov x19, x1");                                         // fmt_ptr
    emitter.instruction("mov x20, x2");                                         // fmt_remaining_len
    emitter.instruction("mov x26, x0");                                         // arg_count
    emitter.instruction("mov x21, #0");                                         // arg_index = 0
    emitter.instruction("add x22, sp, #384");                                   // args_base (past our frame)

    // -- set up concat_buf destination --
    crate::codegen_support::abi::emit_symbol_address(emitter, "x25", "_concat_off");
    emitter.instruction("ldr x8, [x25]");                                       // load current offset
    crate::codegen_support::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x23, x7, x8");                                     // dest pointer = buf + offset
    emitter.instruction("mov x24, x23");                                        // save result start

    // -- main format scanning loop --
    emitter.label("__rt_sprintf_loop");
    emitter.instruction("cbz x20, __rt_sprintf_done");                          // no format chars left
    emitter.instruction("ldrb w12, [x19], #1");                                 // load format char, advance
    emitter.instruction("sub x20, x20, #1");                                    // decrement remaining
    emitter.instruction("cmp w12, #37");                                        // is it '%'?
    emitter.instruction("b.eq __rt_sprintf_fmt");                               // yes → process format specifier

    // -- literal char: copy to output --
    emitter.instruction("strb w12, [x23], #1");                                 // copy literal char to output
    emitter.instruction("b __rt_sprintf_loop");                                 // next char

    // -- process format specifier --
    emitter.label("__rt_sprintf_fmt");
    emitter.instruction("cbz x20, __rt_sprintf_done");                          // no char after % → done
    emitter.instruction("ldrb w12, [x19]");                                     // peek at next char

    // -- %% → literal % --
    emitter.instruction("cmp w12, #37");                                        // is it '%'?
    emitter.instruction("b.ne __rt_sprintf_scan_spec");                         // no → scan full specifier
    emitter.instruction("add x19, x19, #1");                                    // consume the second '%'
    emitter.instruction("sub x20, x20, #1");                                    // decrement remaining
    emitter.instruction("strb w12, [x23], #1");                                 // write literal '%' to output
    emitter.instruction("b __rt_sprintf_loop");                                 // next

    // -- scan format specifier into mini buffer at sp+80 --
    // Build: '%' + [flags] + [width] + [.precision] + [ll] + type_char + '\0'
    emitter.label("__rt_sprintf_scan_spec");
    // A positional specifier borrows the argument index without moving it: php's
    // sequential counter is untouched, so printf("%s|%2$s|%s", a, b, c) is "a|b|b".
    // The previous specifier's borrow is returned here rather than at each of the
    // conversion paths' many exits.
    emitter.instruction("ldr x9, [sp, #240]");                                  // did the previous specifier borrow the index?
    emitter.instruction("cmn x9, #1");                                          // -1 means no borrow was in effect
    emitter.instruction("b.eq __rt_sprintf_no_borrow");                         // nothing to return
    emitter.instruction("mov x21, x9");                                         // give the sequential index back
    emitter.instruction("mov x9, #-1");                                         // clear the borrow marker
    emitter.instruction("str x9, [sp, #240]");                                  // the borrow is settled
    emitter.label("__rt_sprintf_no_borrow");
    emitter.instruction("add x10, sp, #80");                                    // mini format buffer start
    emitter.instruction("mov w15, #37");                                        // '%' character
    emitter.instruction("strb w15, [x10], #1");                                 // write '%' to mini buffer
    emitter.instruction("mov x16, #0");                                         // mini-buffer offset of the precision, 0 until one is scanned
    emitter.instruction("mov x9, #-1");                                         // no custom pad character captured yet
    emitter.instruction("str x9, [sp, #248]");                                  // slot freed by the native %s path

    // -- optional "N$" argument number, a php extension --
    // The digits are ambiguous with a width, so they are only committed once a '$'
    // follows; otherwise the cursor rewinds and the width scanner reads them again.
    emitter.instruction("mov x13, x19");                                        // remember the cursor before the digits
    emitter.instruction("mov x14, x20");                                        // remember the remaining format length
    emitter.instruction("mov x15, #0");                                         // accumulated argument number
    emitter.label("__rt_sprintf_argnum_scan");
    emitter.instruction("cbz x20, __rt_sprintf_argnum_none");                   // format string ended: no argument number
    emitter.instruction("ldrb w12, [x19]");                                     // load the next format byte
    emitter.instruction("cmp w12, #48");                                        // below '0'?
    emitter.instruction("b.lt __rt_sprintf_argnum_dollar");                     // not a digit: check for the marker
    emitter.instruction("cmp w12, #57");                                        // above '9'?
    emitter.instruction("b.gt __rt_sprintf_argnum_dollar");                     // not a digit: check for the marker
    emitter.instruction("mov x11, #10");                                        // decimal base
    emitter.instruction("mul x15, x15, x11");                                   // shift the accumulated number one decimal place
    emitter.instruction("sub w12, w12, #48");                                   // digit value
    emitter.instruction("add x15, x15, x12");                                   // accumulate the digit
    emitter.instruction("add x19, x19, #1");                                    // consume the digit
    emitter.instruction("sub x20, x20, #1");                                    // decrement the remaining format length
    emitter.instruction("b __rt_sprintf_argnum_scan");                          // keep reading digits
    emitter.label("__rt_sprintf_argnum_dollar");
    emitter.instruction("cmp x19, x13");                                        // were there any digits at all?
    emitter.instruction("b.eq __rt_sprintf_argnum_none");                       // no digits: this is not an argument number
    emitter.instruction("cbz x20, __rt_sprintf_argnum_none");                   // nothing follows the digits
    emitter.instruction("ldrb w12, [x19]");                                     // load the byte after the digits
    emitter.instruction("cmp w12, #36");                                        // is it the '$' marker?
    emitter.instruction("b.ne __rt_sprintf_argnum_none");                       // the digits were a width after all
    emitter.instruction("cbz x15, __rt_sprintf_argnum_none");                   // php numbers arguments from one; 0 is not one
    emitter.instruction("add x19, x19, #1");                                    // consume the '$'
    emitter.instruction("sub x20, x20, #1");                                    // decrement the remaining format length
    emitter.instruction("str x21, [sp, #240]");                                 // borrow the index, remembering where to give it back
    emitter.instruction("sub x21, x15, #1");                                    // php argument numbers are 1-based
    emitter.instruction("b __rt_sprintf_scan_flags");                           // the specifier continues with its flags
    emitter.label("__rt_sprintf_argnum_none");
    emitter.instruction("mov x19, x13");                                        // rewind so the width scanner re-reads the digits
    emitter.instruction("mov x20, x14");                                        // restore the remaining format length

    // -- scan flags: '-', '+', '0', ' ', '#' --
    emitter.label("__rt_sprintf_scan_flags");
    emitter.instruction("cbz x20, __rt_sprintf_end_spec");                      // no chars left
    emitter.instruction("ldrb w12, [x19]");                                     // peek at current char
    emitter.instruction("cmp w12, #45");                                        // '-' flag?
    emitter.instruction("b.eq __rt_sprintf_copy_flag");                         // yes → copy it
    emitter.instruction("cmp w12, #43");                                        // '+' flag?
    emitter.instruction("b.eq __rt_sprintf_copy_flag");                         // yes → copy it
    emitter.instruction("cmp w12, #48");                                        // '0' flag?
    emitter.instruction("b.eq __rt_sprintf_copy_flag");                         // yes → copy it
    emitter.instruction("cmp w12, #32");                                        // ' ' flag?
    emitter.instruction("b.eq __rt_sprintf_drop_flag");                         // php accepts the space flag but gives it no meaning
    emitter.instruction("cmp w12, #35");                                        // '#' flag?
    emitter.instruction("b.eq __rt_sprintf_copy_flag");                         // yes → copy it
    emitter.instruction("cmp w12, #39");                                        // php's pad-character introducer?
    emitter.instruction("b.eq __rt_sprintf_padchar");                           // the byte after it is the pad character, whatever it is
    emitter.instruction("b __rt_sprintf_scan_width");                           // no flag → try width

    // -- php's "'X": the byte after the quote becomes the pad character, even when
    //    it is '-' or a digit. libc has no such flag, so it never reaches snprintf.
    emitter.label("__rt_sprintf_padchar");
    emitter.instruction("cmp x20, #2");                                         // is there a byte after the quote?
    emitter.instruction("b.lt __rt_sprintf_scan_width");                        // malformed: leave it to the width scanner
    emitter.instruction("ldrb w12, [x19, #1]");                                 // load the pad character
    emitter.instruction("cmp w12, #48");                                        // is the pad character '0'?
    emitter.instruction("b.ne __rt_sprintf_padchar_custom");                    // any other character pads uniformly
    emitter.instruction("mov w12, #48");                                        // '0' is the zero flag: sign-aware, so libc keeps it
    emitter.instruction("strb w12, [x10], #1");                                 // copy it to the mini format as an ordinary flag
    emitter.instruction("b __rt_sprintf_padchar_done");                         // and let snprintf do the padding
    emitter.label("__rt_sprintf_padchar_custom");
    emitter.instruction("str x12, [sp, #248]");                                 // remember the pad character for the field renderer
    emitter.label("__rt_sprintf_padchar_done");
    emitter.instruction("add x19, x19, #2");                                    // consume the quote and the pad character
    emitter.instruction("sub x20, x20, #2");                                    // decrement the remaining format length
    emitter.instruction("b __rt_sprintf_scan_flags");                           // keep scanning flags

    emitter.label("__rt_sprintf_copy_flag");
    emitter.instruction("strb w12, [x10], #1");                                 // copy flag char to mini buffer
    emitter.instruction("add x19, x19, #1");                                    // consume char from format
    emitter.instruction("sub x20, x20, #1");                                    // decrement remaining
    emitter.instruction("b __rt_sprintf_scan_flags");                           // check for more flags

    // -- flags php parses but ignores: consumed without reaching snprintf, which
    //    would otherwise reserve a sign column the way C does --
    emitter.label("__rt_sprintf_drop_flag");
    emitter.instruction("add x19, x19, #1");                                    // consume the flag from the format string
    emitter.instruction("sub x20, x20, #1");                                    // decrement the remaining format length
    emitter.instruction("b __rt_sprintf_scan_flags");                           // keep scanning flags

    // -- scan width: digits --
    emitter.label("__rt_sprintf_scan_width");
    emitter.instruction("cbz x20, __rt_sprintf_end_spec");                      // no chars left
    emitter.instruction("ldrb w12, [x19]");                                     // peek at current char
    emitter.instruction("cmp w12, #48");                                        // < '0'?
    emitter.instruction("b.lt __rt_sprintf_scan_dot");                          // yes → try precision dot
    emitter.instruction("cmp w12, #57");                                        // > '9'?
    emitter.instruction("b.gt __rt_sprintf_scan_dot");                          // yes → try precision dot
    emitter.instruction("strb w12, [x10], #1");                                 // copy width digit to mini buffer
    emitter.instruction("add x19, x19, #1");                                    // consume char
    emitter.instruction("sub x20, x20, #1");                                    // decrement remaining
    emitter.instruction("b __rt_sprintf_scan_width");                           // check for more digits

    // -- scan precision: '.' followed by digits --
    emitter.label("__rt_sprintf_scan_dot");
    emitter.instruction("cmp w12, #46");                                        // '.' ?
    emitter.instruction("b.ne __rt_sprintf_scan_type");                         // no → must be type char
    emitter.instruction("mov x16, x10");                                        // remember where the precision starts, so integer types can drop it
    emitter.instruction("strb w12, [x10], #1");                                 // copy '.' to mini buffer
    emitter.instruction("add x19, x19, #1");                                    // consume '.'
    emitter.instruction("sub x20, x20, #1");                                    // decrement remaining

    emitter.label("__rt_sprintf_scan_prec");
    emitter.instruction("cbz x20, __rt_sprintf_end_spec");                      // no chars left
    emitter.instruction("ldrb w12, [x19]");                                     // peek at current char
    emitter.instruction("cmp w12, #48");                                        // < '0'?
    emitter.instruction("b.lt __rt_sprintf_scan_type");                         // no → type char
    emitter.instruction("cmp w12, #57");                                        // > '9'?
    emitter.instruction("b.gt __rt_sprintf_scan_type");                         // no → type char
    emitter.instruction("strb w12, [x10], #1");                                 // copy precision digit
    emitter.instruction("add x19, x19, #1");                                    // consume char
    emitter.instruction("sub x20, x20, #1");                                    // decrement remaining
    emitter.instruction("b __rt_sprintf_scan_prec");                            // check for more digits

    // -- read type character --
    emitter.label("__rt_sprintf_scan_type");
    emitter.instruction("cbz x20, __rt_sprintf_end_spec");                      // no chars left
    emitter.instruction("ldrb w12, [x19], #1");                                 // load type char, consume it
    emitter.instruction("sub x20, x20, #1");                                    // decrement remaining

    // Dispatch by type character
    emitter.instruction("cmp w12, #102");                                       // 'f' ?
    emitter.instruction("b.eq __rt_sprintf_type_float");                        // yes → float
    emitter.instruction("cmp w12, #101");                                       // 'e' ?
    emitter.instruction("b.eq __rt_sprintf_type_float");                        // yes → float
    emitter.instruction("cmp w12, #103");                                       // 'g' ?
    emitter.instruction("b.eq __rt_sprintf_type_float");                        // yes → float
    emitter.instruction("cmp w12, #69");                                        // 'E' ?
    emitter.instruction("b.eq __rt_sprintf_type_float");                        // yes → float
    emitter.instruction("cmp w12, #71");                                        // 'G' ?
    emitter.instruction("b.eq __rt_sprintf_type_float");                        // yes → float
    emitter.instruction("cmp w12, #115");                                       // 's' ?
    emitter.instruction("b.eq __rt_sprintf_type_str");                          // yes → string
    emitter.instruction("cmp w12, #98");                                        // 'b' ?
    emitter.instruction("b.eq __rt_sprintf_type_b");                            // yes → php's binary conversion, which C has no equivalent for
    emitter.instruction("b __rt_sprintf_type_int_prec");                        // default → integer

    // -- integer conversions: php does not give precision the C meaning --
    // %d, %u and %c ignore it outright; %x, %X and %o render nothing at all. Neither
    // is "minimum digit count", so the precision never reaches snprintf.
    emitter.label("__rt_sprintf_type_int_prec");
    emitter.instruction("cbz x16, __rt_sprintf_type_int");                      // no precision was scanned
    emitter.instruction("cmp w12, #120");                                       // is the conversion 'x'?
    emitter.instruction("b.eq __rt_sprintf_type_int_blank");                    // php renders nothing for a precise hex
    emitter.instruction("cmp w12, #88");                                        // is the conversion 'X'?
    emitter.instruction("b.eq __rt_sprintf_type_int_blank");                    // php renders nothing for a precise hex
    emitter.instruction("cmp w12, #111");                                       // is the conversion 'o'?
    emitter.instruction("b.eq __rt_sprintf_type_int_blank");                    // php renders nothing for a precise octal
    emitter.instruction("mov x10, x16");                                        // drop the precision so snprintf never sees it
    emitter.instruction("b __rt_sprintf_type_int");                             // format the integer without it

    // -- %b: php's binary conversion, which C's formatter does not have --
    // The value is expanded as unsigned 64-bit, so %b of -1 is sixty-four ones, and
    // the digits then go through the same field renderer as %s so width, alignment
    // and the pad character behave identically.
    emitter.label("__rt_sprintf_type_b");
    emitter.instruction("cbnz x16, __rt_sprintf_type_int_blank");               // a precision renders nothing, as it does for %x and %o
    emitter.instruction("lsl x15, x21, #4");                                    // arg offset = index * 16
    emitter.instruction("add x15, x22, x15");                                   // arg address in caller's stack
    emitter.instruction("ldr x1, [x15]");                                       // load the integer payload
    emitter.instruction("add x21, x21, #1");                                    // increment arg index
    emitter.instruction("add x3, sp, #176");                                    // build backwards from the far end of the scratch buffer
    emitter.instruction("mov x4, #0");                                          // digit count
    emitter.label("__rt_sprintf_b_digit");
    emitter.instruction("and x9, x1, #1");                                      // take the low bit
    emitter.instruction("add x9, x9, #48");                                     // render it as '0' or '1'
    emitter.instruction("sub x3, x3, #1");                                      // move one byte left
    emitter.instruction("strb w9, [x3]");                                       // store the digit
    emitter.instruction("add x4, x4, #1");                                      // count it
    emitter.instruction("lsr x1, x1, #1");                                      // shift the value right, unsigned
    emitter.instruction("cbnz x1, __rt_sprintf_b_digit");                       // keep going while bits remain
    emitter.instruction("b __rt_sprintf_field_render");                         // pad and align exactly as %s does

    emitter.label("__rt_sprintf_type_int_blank");
    emitter.instruction("mov x10, x16");                                        // drop the precision from the scanned prefix
    emitter.instruction("add x21, x21, #1");                                    // php still consumes the argument
    emitter.instruction("mov x4, #0");                                          // nothing to render: the field is pure padding
    emitter.instruction("b __rt_sprintf_field_render");                         // reuse the string field renderer

    // -- incomplete specifier at end of format string --
    emitter.label("__rt_sprintf_end_spec");
    emitter.instruction("b __rt_sprintf_done");                                 // bail out

    // ================================================================
    // FLOAT: %f, %e, %g, %E, %G (with optional flags/width/precision)
    // Passes the double value on the stack at [sp] for variadic ABI.
    // ================================================================
    emitter.label("__rt_sprintf_type_float");
    emitter.instruction("strb w12, [x10], #1");                                 // copy type char to mini buffer
    emitter.instruction("strb wzr, [x10]");                                     // null-terminate format string

    // -- load next arg (float bits) --
    emitter.instruction("lsl x15, x21, #4");                                    // arg offset = index * 16
    emitter.instruction("add x15, x22, x15");                                   // arg address in caller's stack
    emitter.instruction("ldr x3, [x15]");                                       // load float bits as integer
    emitter.instruction("add x21, x21, #1");                                    // increment arg index

    if emitter.platform == Platform::Linux {
        emitter.instruction("fmov d0, x3");                                     // pass first variadic double in the Linux AArch64 FP register
    }

    // -- store variadic arg on stack for snprintf --
    emitter.instruction("str x3, [sp]");                                        // variadic float bits at [sp]

    // -- call snprintf(buf, 128, fmt) with variadic float on stack --
    emitter.instruction("add x0, sp, #112");                                    // output buffer at sp+112
    emitter.instruction("mov x1, #128");                                        // buffer size
    emitter.instruction("add x2, sp, #80");                                     // mini format string at sp+80
    emitter.bl_c("snprintf");                                        // call libc snprintf
    // x0 = number of chars written

    // snprintf reports the length it *would* have written, which can exceed the
    // 128-byte scratch. Copying that many bytes reads past the buffer and emits
    // adjacent stack memory, so an oversized result is re-rendered straight into the
    // destination, which has the whole 64 KiB concat buffer behind it.
    emitter.instruction("cmp x0, #128");                                        // did the whole result fit in the scratch buffer?
    emitter.instruction("b.ge __rt_sprintf_overflow");                          // it did not: render it at the destination instead

    // -- PHP parity: %e/%E (and any exponential-form %g/%G) exponent uses the
    // -- minimum digit count (no leading zero), but CRT snprintf pads to at
    // -- least 2 digits. A double's decimal exponent never exceeds 3 digits and
    // -- 3-digit exponents are never zero-padded (they start at magnitude 100),
    // -- so the only possible padding is a single leading '0' in a 2-digit
    // -- exponent; strip it in place and shrink the byte count by one.
    emitter.instruction("add x5, sp, #112");                                    // scan cursor over the freshly formatted snprintf output
    emitter.instruction("mov x6, x0");                                          // remaining bytes to scan for the 'e'/'E' exponent marker
    emitter.label("__rt_sprintf_etrim_scan");
    emitter.instruction("cbz x6, __rt_sprintf_etrim_done");                     // no exponent (e.g. %f) -> nothing to trim
    emitter.instruction("ldrb w7, [x5]");                                       // load the next output byte
    emitter.instruction("cmp w7, #101");                                        // is it 'e'?
    emitter.instruction("b.eq __rt_sprintf_etrim_found");                       // found the exponent marker
    emitter.instruction("cmp w7, #69");                                         // is it 'E'?
    emitter.instruction("b.eq __rt_sprintf_etrim_found");                       // found the exponent marker
    emitter.instruction("add x5, x5, #1");                                      // advance the scan cursor
    emitter.instruction("sub x6, x6, #1");                                      // decrement the remaining scan length
    emitter.instruction("b __rt_sprintf_etrim_scan");                           // keep scanning for the exponent marker
    emitter.label("__rt_sprintf_etrim_found");
    emitter.instruction("add x5, x5, #1");                                      // advance past the 'e'/'E' marker
    emitter.instruction("sub x6, x6, #1");                                      // decrement the remaining scan length
    emitter.instruction("cbz x6, __rt_sprintf_etrim_done");                     // malformed: exponent marker was the last byte -> bail defensively
    emitter.instruction("ldrb w7, [x5]");                                       // load the byte after the exponent marker
    emitter.instruction("cmp w7, #43");                                         // is it '+'?
    emitter.instruction("b.eq __rt_sprintf_etrim_sign");                        // consume the exponent sign
    emitter.instruction("cmp w7, #45");                                         // is it '-'?
    emitter.instruction("b.ne __rt_sprintf_etrim_done");                        // C99 always emits an exponent sign; bail defensively if absent
    emitter.label("__rt_sprintf_etrim_sign");
    emitter.instruction("add x5, x5, #1");                                      // advance past the exponent sign
    emitter.instruction("sub x6, x6, #1");                                      // decrement the remaining scan length
    emitter.instruction("cmp x6, #2");                                          // need at least two remaining bytes to test "0<digit>"
    emitter.instruction("b.lt __rt_sprintf_etrim_done");                        // too short to be a padded 2-digit exponent
    emitter.instruction("ldrb w7, [x5]");                                       // load the first exponent digit
    emitter.instruction("cmp w7, #48");                                         // is it '0'?
    emitter.instruction("b.ne __rt_sprintf_etrim_done");                        // not zero-padded -> nothing to strip
    emitter.instruction("ldrb w8, [x5, #1]");                                   // load the byte after the leading zero
    emitter.instruction("cmp w8, #48");                                         // is it below '0'?
    emitter.instruction("b.lt __rt_sprintf_etrim_done");                        // not a digit -> the '0' was the only exponent digit, keep it
    emitter.instruction("cmp w8, #57");                                         // is it above '9'?
    emitter.instruction("b.gt __rt_sprintf_etrim_done");                        // not a digit -> keep the only exponent digit
    // -- guard: a right-justified WIDTH field pads BEFORE the sign/mantissa with
    // -- ' ' or '0'. Stripping a byte from the exponent would shrink the total
    // -- field width, so detect that padding and skip the strip entirely rather
    // -- than corrupt the requested width (a documented, bounded residual gap;
    // -- the no-width and left-justified-width cases below are fully handled).
    emitter.instruction("add x9, sp, #112");                                    // buffer start
    emitter.instruction("ldrb w12, [x9]");                                      // first output byte
    emitter.instruction("cmp w12, #32");                                        // is it a space (space-padded field)?
    emitter.instruction("b.eq __rt_sprintf_etrim_done");                        // space padding present -> skip the strip
    emitter.instruction("mov x14, x9");                                         // cursor for the (optional) leading '-'/'+' sign
    emitter.instruction("cmp w12, #45");                                        // is the first byte a '-' sign?
    emitter.instruction("b.eq __rt_sprintf_etrim_lead_sign");                   // yes -> skip past it before checking for zero-padding
    emitter.instruction("cmp w12, #43");                                        // is the first byte a '+' sign (the '+' flag)?
    emitter.instruction("b.ne __rt_sprintf_etrim_lead_check");                  // no sign -> check directly
    emitter.label("__rt_sprintf_etrim_lead_sign");
    emitter.instruction("add x14, x14, #1");                                    // skip past the sign before checking for zero-padding
    emitter.label("__rt_sprintf_etrim_lead_check");
    emitter.instruction("ldrb w12, [x14]");                                     // byte after the optional sign
    emitter.instruction("cmp w12, #48");                                        // is it '0'?
    emitter.instruction("b.ne __rt_sprintf_etrim_shift_setup");                 // not zero -> no zero-padding, safe to strip
    emitter.instruction("ldrb w12, [x14, #1]");                                 // byte after that leading zero
    emitter.instruction("cmp w12, #46");                                        // is it '.' (the leading zero IS the legitimate mantissa digit)?
    emitter.instruction("b.eq __rt_sprintf_etrim_shift_setup");                 // legitimate "0.xxx" mantissa -> safe to strip the exponent
    emitter.instruction("b __rt_sprintf_etrim_done");                           // zero-padded width field -> skip the strip
    // -- confirmed a padded 2-digit exponent ("0" + digit) with no right-justify padding: shift the tail left by one byte, dropping the leading zero --
    emitter.label("__rt_sprintf_etrim_shift_setup");
    emitter.instruction("add x9, x9, x0");                                      // x9 = original buffer end (using the pre-trim snprintf byte count)
    emitter.instruction("ldrb w12, [x9, #-1]");                                 // last output byte (before this trim)
    emitter.instruction("cmp w12, #32");                                        // was the field left-justify space-padded?
    emitter.instruction("cset x13, eq");                                        // x13 = 1 when trailing-space padding is present
    emitter.instruction("add x11, x5, #1");                                     // source cursor = byte after the leading zero
    emitter.instruction("mov x9, x5");                                          // dest cursor = the leading zero's position
    emitter.instruction("add x8, sp, #112");                                    // recompute the scratch buffer base
    emitter.instruction("add x8, x8, x0");                                      // x8 = original buffer end (fixed, independent of the shift cursors)
    emitter.label("__rt_sprintf_etrim_shift");
    emitter.instruction("cmp x11, x8");                                         // reached the end of the original output?
    emitter.instruction("b.ge __rt_sprintf_etrim_shift_done");                  // shift complete
    emitter.instruction("ldrb w14, [x11], #1");                                 // load the next byte to shift down
    emitter.instruction("strb w14, [x9], #1");                                  // shift it left by one position
    emitter.instruction("b __rt_sprintf_etrim_shift");                          // continue shifting
    emitter.label("__rt_sprintf_etrim_shift_done");
    emitter.instruction("cbz x13, __rt_sprintf_etrim_shrink");                  // no trailing padding -> just shrink the byte count
    emitter.instruction("mov w12, #32");                                        // ASCII space
    emitter.instruction("strb w12, [x9]");                                      // restore the requested field width with one more trailing pad space
    emitter.instruction("b __rt_sprintf_etrim_done");                           // keep x0 unchanged: total field width is preserved
    emitter.label("__rt_sprintf_etrim_shrink");
    emitter.instruction("sub x0, x0, #1");                                      // no padding to preserve -> one byte shorter after dropping the leading exponent zero
    emitter.label("__rt_sprintf_etrim_done");


    // -- php's "'X" pad character on a conversion libc rendered --
    // libc has no such flag, so snprintf padded with spaces. A numeric conversion
    // never contains a space of its own once the space flag is dropped, so the
    // padding is exactly the run of spaces at one end or the other.
    emitter.instruction("ldr x9, [sp, #248]");                                  // did the specifier name a pad character?
    emitter.instruction("cmn x9, #1");                                          // -1 means it did not
    emitter.instruction("b.eq __rt_sprintf_pad_done_f");                        // leave the spaces alone
    emitter.instruction("add x5, sp, #112");                                    // scan the freshly rendered output
    emitter.instruction("mov x6, x0");                                          // bytes rendered
    emitter.label("__rt_sprintf_pad_lead_f");
    emitter.instruction("cbz x6, __rt_sprintf_pad_done_f");                     // the whole field was padding
    emitter.instruction("ldrb w11, [x5]");                                      // load the next byte
    emitter.instruction("cmp w11, #32");                                        // is it a padding space?
    emitter.instruction("b.ne __rt_sprintf_pad_trail_f");                       // the value starts here: try the other end
    emitter.instruction("strb w9, [x5]");                                       // substitute the pad character
    emitter.instruction("add x5, x5, #1");                                      // advance
    emitter.instruction("sub x6, x6, #1");                                      // one fewer byte to inspect
    emitter.instruction("b __rt_sprintf_pad_lead_f");                           // keep substituting
    emitter.label("__rt_sprintf_pad_trail_f");
    emitter.instruction("add x5, sp, #112");                                    // back to the start of the output
    emitter.instruction("add x5, x5, x0");                                      // one past its end
    emitter.instruction("mov x7, x0");                                          // bytes still to inspect
    emitter.label("__rt_sprintf_pad_trail_loop_f");
    emitter.instruction("cbz x7, __rt_sprintf_pad_done_f");                     // nothing left
    emitter.instruction("sub x5, x5, #1");                                      // step back one byte
    emitter.instruction("ldrb w11, [x5]");                                      // load it
    emitter.instruction("cmp w11, #32");                                        // is it a padding space?
    emitter.instruction("b.ne __rt_sprintf_pad_done_f");                        // the value ends here
    emitter.instruction("strb w9, [x5]");                                       // substitute the pad character
    emitter.instruction("sub x7, x7, #1");                                      // one fewer byte to inspect
    emitter.instruction("b __rt_sprintf_pad_trail_loop_f");                     // keep substituting
    emitter.label("__rt_sprintf_pad_done_f");
    // -- copy snprintf result to concat_buf --
    emitter.instruction("mov x4, x0");                                          // chars to copy
    emitter.instruction("add x3, sp, #112");                                    // source buffer

    emitter.label("__rt_sprintf_copy_f");
    emitter.instruction("cbz x4, __rt_sprintf_copy_f_done");                    // no bytes left
    emitter.instruction("ldrb w15, [x3], #1");                                  // load byte from snprintf output
    emitter.instruction("strb w15, [x23], #1");                                 // write to concat_buf
    emitter.instruction("sub x4, x4, #1");                                      // decrement counter
    emitter.instruction("b __rt_sprintf_copy_f");                               // continue copying

    emitter.label("__rt_sprintf_copy_f_done");
    emitter.instruction("b __rt_sprintf_loop");                                 // next format char

    // ================================================================
    // INTEGER: %d, %x, %o, %c, etc. (with optional flags/width/precision)
    // Uses %lld/%llx/%llo for 64-bit ints (except %c which stays 32-bit).
    // Passes the integer value on the stack at [sp] for variadic ABI.
    // ================================================================
    emitter.label("__rt_sprintf_type_int");

    // For 'd', 'x', 'o' we need 'll' prefix for 64-bit; 'c' stays as-is
    emitter.instruction("cmp w12, #99");                                        // 'c' ?
    emitter.instruction("b.eq __rt_sprintf_int_noprefix");                      // skip 'll' for %c

    // Write 'll' length modifier for 64-bit integer types
    emitter.instruction("mov w15, #108");                                       // 'l' character
    emitter.instruction("strb w15, [x10], #1");                                 // write first 'l' to mini buffer
    emitter.instruction("strb w15, [x10], #1");                                 // write second 'l' to mini buffer

    emitter.label("__rt_sprintf_int_noprefix");
    emitter.instruction("strb w12, [x10], #1");                                 // copy type char to mini buffer
    emitter.instruction("strb wzr, [x10]");                                     // null-terminate format string

    // -- load next arg (int value) --
    emitter.instruction("lsl x15, x21, #4");                                    // arg offset = index * 16
    emitter.instruction("add x15, x22, x15");                                   // arg address in caller's stack
    emitter.instruction("ldr x3, [x15]");                                       // load integer value
    emitter.instruction("add x21, x21, #1");                                    // increment arg index

    // -- store variadic arg on stack for snprintf --
    emitter.instruction("str x3, [sp]");                                        // variadic int at [sp]

    // -- call snprintf(buf, 128, fmt) with variadic int on stack --
    emitter.instruction("add x0, sp, #112");                                    // output buffer at sp+112
    emitter.instruction("mov x1, #128");                                        // buffer size
    emitter.instruction("add x2, sp, #80");                                     // mini format string at sp+80
    emitter.bl_c("snprintf");                                        // call libc snprintf
    // x0 = number of chars written

    // snprintf reports the length it *would* have written, which can exceed the
    // 128-byte scratch. Copying that many bytes reads past the buffer and emits
    // adjacent stack memory, so an oversized result is re-rendered straight into the
    // destination, which has the whole 64 KiB concat buffer behind it.
    emitter.instruction("cmp x0, #128");                                        // did the whole result fit in the scratch buffer?
    emitter.instruction("b.ge __rt_sprintf_overflow");                          // it did not: render it at the destination instead


    // -- php's "'X" pad character on a conversion libc rendered --
    // libc has no such flag, so snprintf padded with spaces. A numeric conversion
    // never contains a space of its own once the space flag is dropped, so the
    // padding is exactly the run of spaces at one end or the other.
    emitter.instruction("ldr x9, [sp, #248]");                                  // did the specifier name a pad character?
    emitter.instruction("cmn x9, #1");                                          // -1 means it did not
    emitter.instruction("b.eq __rt_sprintf_pad_done_i");                        // leave the spaces alone
    emitter.instruction("add x5, sp, #112");                                    // scan the freshly rendered output
    emitter.instruction("mov x6, x0");                                          // bytes rendered
    emitter.label("__rt_sprintf_pad_lead_i");
    emitter.instruction("cbz x6, __rt_sprintf_pad_done_i");                     // the whole field was padding
    emitter.instruction("ldrb w11, [x5]");                                      // load the next byte
    emitter.instruction("cmp w11, #32");                                        // is it a padding space?
    emitter.instruction("b.ne __rt_sprintf_pad_trail_i");                       // the value starts here: try the other end
    emitter.instruction("strb w9, [x5]");                                       // substitute the pad character
    emitter.instruction("add x5, x5, #1");                                      // advance
    emitter.instruction("sub x6, x6, #1");                                      // one fewer byte to inspect
    emitter.instruction("b __rt_sprintf_pad_lead_i");                           // keep substituting
    emitter.label("__rt_sprintf_pad_trail_i");
    emitter.instruction("add x5, sp, #112");                                    // back to the start of the output
    emitter.instruction("add x5, x5, x0");                                      // one past its end
    emitter.instruction("mov x7, x0");                                          // bytes still to inspect
    emitter.label("__rt_sprintf_pad_trail_loop_i");
    emitter.instruction("cbz x7, __rt_sprintf_pad_done_i");                     // nothing left
    emitter.instruction("sub x5, x5, #1");                                      // step back one byte
    emitter.instruction("ldrb w11, [x5]");                                      // load it
    emitter.instruction("cmp w11, #32");                                        // is it a padding space?
    emitter.instruction("b.ne __rt_sprintf_pad_done_i");                        // the value ends here
    emitter.instruction("strb w9, [x5]");                                       // substitute the pad character
    emitter.instruction("sub x7, x7, #1");                                      // one fewer byte to inspect
    emitter.instruction("b __rt_sprintf_pad_trail_loop_i");                     // keep substituting
    emitter.label("__rt_sprintf_pad_done_i");
    // -- copy snprintf result to concat_buf --
    emitter.instruction("mov x4, x0");                                          // chars to copy
    emitter.instruction("add x3, sp, #112");                                    // source buffer

    emitter.label("__rt_sprintf_copy_i");
    emitter.instruction("cbz x4, __rt_sprintf_copy_i_done");                    // no bytes left
    emitter.instruction("ldrb w15, [x3], #1");                                  // load byte from snprintf output
    emitter.instruction("strb w15, [x23], #1");                                 // write to concat_buf
    emitter.instruction("sub x4, x4, #1");                                      // decrement counter
    emitter.instruction("b __rt_sprintf_copy_i");                               // continue copying

    emitter.label("__rt_sprintf_copy_i_done");
    emitter.instruction("b __rt_sprintf_loop");                                 // next format char

    // ================================================================
    // STRING: %s (with optional width/padding)
    // snprintf needs a null-terminated C string. Our strings are ptr+len,
    // so we copy the string to a temp buffer at sp+240 and null-terminate it.
    // The variadic pointer goes on the stack at [sp].
    // ================================================================
    // php's %s is only "truncate to precision, then pad to width with the pad
    // character", so it needs no C formatter. Routing it through snprintf required a
    // NUL-terminated copy in a 128-byte buffer, which silently truncated every
    // argument at 127 bytes and cut the string short at any NUL byte -- php strings
    // are binary and may contain them. The bytes now go straight to the destination
    // using the stored length: binary-safe, and with no length ceiling.
    emitter.label("__rt_sprintf_type_str");

    // -- load next arg (string: ptr + tag|len) --
    emitter.instruction("lsl x15, x21, #4");                                    // arg offset = index * 16
    emitter.instruction("add x15, x22, x15");                                   // arg address in caller's stack
    emitter.instruction("ldr x3, [x15]");                                       // load string pointer
    emitter.instruction("ldr x4, [x15, #8]");                                   // load tag|length word
    emitter.instruction("lsr x4, x4, #8");                                      // extract length (shift right 8)
    emitter.instruction("add x21, x21, #1");                                    // increment arg index

    // -- re-read the scanned prefix (sp+80 .. x10) for flags, width and precision --
    emitter.label("__rt_sprintf_field_render");
    emitter.instruction("add x5, sp, #81");                                     // first byte after the '%'
    emitter.instruction("mov x6, #0");                                          // left-align flag, clear by default
    emitter.instruction("mov x7, #32");                                         // pad character, space by default
    emitter.instruction("ldr x9, [sp, #248]");                                  // did the specifier name its own pad character?
    emitter.instruction("cmn x9, #1");                                          // -1 means it did not
    emitter.instruction("csel x7, x9, x7, ne");                                 // php's "'X" overrides the default
    emitter.instruction("mov x9, #0");                                          // field width
    emitter.instruction("mov x11, #-1");                                        // precision, -1 when absent
    emitter.label("__rt_sprintf_str_flag");
    emitter.instruction("cmp x5, x10");                                         // consumed the whole scanned prefix?
    emitter.instruction("b.ge __rt_sprintf_str_field");                         // nothing left: render the field
    emitter.instruction("ldrb w12, [x5]");                                      // load the next prefix byte
    emitter.instruction("cmp w12, #45");                                        // is it '-'?
    emitter.instruction("b.ne __rt_sprintf_str_flag_zero");                     // not the left-align flag
    emitter.instruction("mov x6, #1");                                          // left-align the field
    emitter.instruction("add x5, x5, #1");                                      // consume the flag
    emitter.instruction("b __rt_sprintf_str_flag");                             // keep reading flags
    emitter.label("__rt_sprintf_str_flag_zero");
    emitter.instruction("cmp w12, #48");                                        // is it '0'?
    emitter.instruction("b.ne __rt_sprintf_str_flag_skip");                     // not the zero-pad flag
    emitter.instruction("mov x7, #48");                                         // pad with '0' instead of space
    emitter.instruction("add x5, x5, #1");                                      // consume the flag
    emitter.instruction("b __rt_sprintf_str_flag");                             // keep reading flags
    emitter.label("__rt_sprintf_str_flag_skip");
    emitter.instruction("cmp w12, #43");                                        // is it '+'?
    emitter.instruction("b.eq __rt_sprintf_str_flag_drop");                     // php ignores '+' on strings
    emitter.instruction("cmp w12, #32");                                        // is it ' '?
    emitter.instruction("b.ne __rt_sprintf_str_width");                         // no flags left: read the width
    emitter.label("__rt_sprintf_str_flag_drop");
    emitter.instruction("add x5, x5, #1");                                      // consume the ignored flag
    emitter.instruction("b __rt_sprintf_str_flag");                             // keep reading flags
    emitter.label("__rt_sprintf_str_width");
    emitter.instruction("cmp x5, x10");                                         // consumed the whole scanned prefix?
    emitter.instruction("b.ge __rt_sprintf_str_field");                         // nothing left: render the field
    emitter.instruction("ldrb w12, [x5]");                                      // load the next prefix byte
    emitter.instruction("cmp w12, #46");                                        // is it the precision '.'?
    emitter.instruction("b.eq __rt_sprintf_str_prec_start");                    // switch to reading the precision
    emitter.instruction("cmp w12, #48");                                        // below '0'?
    emitter.instruction("b.lt __rt_sprintf_str_field");                         // not a digit: the width is complete
    emitter.instruction("cmp w12, #57");                                        // above '9'?
    emitter.instruction("b.gt __rt_sprintf_str_field");                         // not a digit: the width is complete
    emitter.instruction("mov x13, #10");                                        // decimal base
    emitter.instruction("mul x9, x9, x13");                                     // shift the accumulated width one decimal place
    emitter.instruction("sub w12, w12, #48");                                   // digit value
    emitter.instruction("add x9, x9, x12");                                     // accumulate the width digit
    emitter.instruction("add x5, x5, #1");                                      // consume the digit
    emitter.instruction("b __rt_sprintf_str_width");                            // keep reading width digits
    emitter.label("__rt_sprintf_str_prec_start");
    emitter.instruction("add x5, x5, #1");                                      // consume the '.'
    emitter.instruction("mov x11, #0");                                         // an explicit precision starts at zero
    emitter.label("__rt_sprintf_str_prec");
    emitter.instruction("cmp x5, x10");                                         // consumed the whole scanned prefix?
    emitter.instruction("b.ge __rt_sprintf_str_field");                         // nothing left: render the field
    emitter.instruction("ldrb w12, [x5]");                                      // load the next prefix byte
    emitter.instruction("cmp w12, #48");                                        // below '0'?
    emitter.instruction("b.lt __rt_sprintf_str_field");                         // not a digit: the precision is complete
    emitter.instruction("cmp w12, #57");                                        // above '9'?
    emitter.instruction("b.gt __rt_sprintf_str_field");                         // not a digit: the precision is complete
    emitter.instruction("mov x13, #10");                                        // decimal base
    emitter.instruction("mul x11, x11, x13");                                   // shift the accumulated precision one decimal place
    emitter.instruction("sub w12, w12, #48");                                   // digit value
    emitter.instruction("add x11, x11, x12");                                   // accumulate the precision digit
    emitter.instruction("add x5, x5, #1");                                      // consume the digit
    emitter.instruction("b __rt_sprintf_str_prec");                             // keep reading precision digits
    emitter.label("__rt_sprintf_str_field");
    emitter.instruction("cmn x11, #1");                                         // was a precision given?
    emitter.instruction("b.eq __rt_sprintf_str_pad_count");                     // no precision: keep the whole string
    emitter.instruction("cmp x4, x11");                                         // is the string longer than the precision?
    emitter.instruction("csel x4, x11, x4, gt");                                // php truncates the string to the precision
    emitter.label("__rt_sprintf_str_pad_count");
    emitter.instruction("sub x13, x9, x4");                                     // padding = width - rendered length
    emitter.instruction("cmp x13, #0");                                         // is the field already at least as wide?
    emitter.instruction("csel x13, xzr, x13, lt");                              // never pad a negative amount
    emitter.instruction("cbnz x6, __rt_sprintf_str_copy_first");                // left-align writes the bytes first
    emitter.label("__rt_sprintf_str_pad_lead");
    emitter.instruction("cbz x13, __rt_sprintf_str_copy_tail");                 // leading padding written
    emitter.instruction("strb w7, [x23], #1");                                  // write one pad character
    emitter.instruction("sub x13, x13, #1");                                    // one fewer pad character to write
    emitter.instruction("b __rt_sprintf_str_pad_lead");                         // keep padding
    emitter.label("__rt_sprintf_str_copy_tail");
    emitter.instruction("cbz x4, __rt_sprintf_loop");                           // the whole string is written
    emitter.instruction("ldrb w12, [x3], #1");                                  // load one argument byte
    emitter.instruction("strb w12, [x23], #1");                                 // append it to the destination
    emitter.instruction("sub x4, x4, #1");                                      // one fewer byte to copy
    emitter.instruction("b __rt_sprintf_str_copy_tail");                        // keep copying
    emitter.label("__rt_sprintf_str_copy_first");
    emitter.instruction("cbz x4, __rt_sprintf_str_pad_trail");                  // the whole string is written
    emitter.instruction("ldrb w12, [x3], #1");                                  // load one argument byte
    emitter.instruction("strb w12, [x23], #1");                                 // append it to the destination
    emitter.instruction("sub x4, x4, #1");                                      // one fewer byte to copy
    emitter.instruction("b __rt_sprintf_str_copy_first");                       // keep copying
    emitter.label("__rt_sprintf_str_pad_trail");
    emitter.instruction("cbz x13, __rt_sprintf_loop");                          // trailing padding written
    emitter.instruction("strb w7, [x23], #1");                                  // write one pad character
    emitter.instruction("sub x13, x13, #1");                                    // one fewer pad character to write
    emitter.instruction("b __rt_sprintf_str_pad_trail");                        // keep padding

    // ================================================================
    // OVERFLOW: the conversion did not fit the scratch buffer
    // The variadic argument is still staged at [sp], so the same mini format can be
    // rendered a second time, this time writing at the destination cursor with the
    // concat buffer's remaining capacity as the bound.
    // ================================================================
    emitter.label("__rt_sprintf_overflow");
    emitter.instruction("mov x0, x23");                                         // write at the destination cursor
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_concat_buf");
    emitter.instruction("sub x11, x23, x9");                                    // bytes of the concat buffer already used
    emitter.instruction("mov x9, #65536");                                      // total concat buffer size
    emitter.instruction("sub x1, x9, x11");                                     // remaining capacity, including the terminator
    emitter.instruction("add x2, sp, #80");                                     // the same one-specifier mini format string

    // The first call clobbered the variadic argument registers, which are
    // caller-saved. Apple's ABI passes every variadic argument on the stack, so the
    // copy at [sp] alone carried the re-render there and this omission was invisible
    // on macOS; AAPCS64 passes the first one in x3 (or d0 for a double), so on Linux
    // the second call formatted whatever the first had left behind -- right length,
    // wrong bytes. The stack copy is still intact, so it is the restore source.
    emitter.instruction("ldr x3, [sp]");                                        // reload the variadic payload for the integer register
    if emitter.platform == Platform::Linux {
        emitter.instruction("fmov d0, x3");                                     // and for the FP register, which AAPCS64 uses for a double
    }
    emitter.bl_c("snprintf");                                        // re-render at the destination
    emitter.instruction("cmp x0, x1");                                          // did the second render fill the remaining capacity?
    emitter.instruction("b.lt __rt_sprintf_overflow_done");                     // it fit: advance by what was written
    emitter.instruction("sub x0, x1, #1");                                      // clamp to what actually landed, excluding the terminator
    emitter.label("__rt_sprintf_overflow_done");
    emitter.instruction("add x23, x23, x0");                                    // advance the destination cursor past the rendered value
    emitter.instruction("b __rt_sprintf_loop");                                 // continue scanning the format string

    // ================================================================
    // DONE: finalize result and clean up
    // ================================================================
    emitter.label("__rt_sprintf_done");
    emitter.instruction("mov x1, x24");                                         // result start ptr in concat_buf
    emitter.instruction("sub x2, x23, x24");                                    // result length

    // -- update concat_off --
    emitter.instruction("ldr x8, [x25]");                                       // current concat offset
    emitter.instruction("add x8, x8, x2");                                      // advance by result length
    emitter.instruction("str x8, [x25]");                                       // store updated offset

    // -- prepare to pop args from caller's stack --
    emitter.instruction("mov x0, x26");                                         // arg_count
    emitter.instruction("lsl x0, x0, #4");                                      // bytes = count * 16

    // -- restore callee-saved registers --
    emitter.instruction("ldp x19, x20, [sp, #16]");                             // restore x19, x20
    emitter.instruction("ldp x21, x22, [sp, #32]");                             // restore x21, x22
    emitter.instruction("ldp x23, x24, [sp, #48]");                             // restore x23, x24
    emitter.instruction("ldp x25, x26, [sp, #64]");                             // restore x25, x26
    emitter.instruction("ldp x29, x30, [sp, #368]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #384");                                    // deallocate our frame
    emitter.instruction("add sp, sp, x0");                                      // pop caller's args from stack
    emitter.instruction("ret");                                                 // return
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::platform::{Arch, Target};

    use super::*;

    /// Verifies the `%E`/`%G` uppercase specifiers dispatch to the float path
    /// alongside `%f`/`%e`/`%g` on AArch64 (a WF10b fix: they previously fell
    /// through to the integer path, reinterpreting the double's raw bits as an
    /// integer and producing garbage).
    #[test]
    fn test_emit_sprintf_aarch64_dispatches_uppercase_e_and_g_to_float_path() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_sprintf(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains("cmp w12, #69\n"), "'E' (69) must be checked");
        assert!(asm.contains("cmp w12, #71\n"), "'G' (71) must be checked");
    }

    /// Verifies the AArch64 `%e`/`%E` exponent-trim (PHP's minimum-digit
    /// exponent) and its right-justified-width padding guard are present,
    /// mirroring the x86_64 fix.
    #[test]
    fn test_emit_sprintf_aarch64_float_path_has_exponent_trim_with_padding_guard() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_sprintf(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains("__rt_sprintf_etrim_scan\n"));
        assert!(asm.contains("__rt_sprintf_etrim_shift_setup\n"));
        assert!(
            asm.contains("cmp w12, #32\n"),
            "must detect space-padded (right-justified) fields to guard the strip"
        );
    }

    /// Verifies the oversized re-render restores the variadic argument first.
    ///
    /// `snprintf` reports the length it *would* have written, so a result wider than
    /// the 128-byte scratch is rendered a second time straight into the destination.
    /// The argument registers are caller-saved and the first call clobbers them, so
    /// the second call has to reload the payload from the stack copy.
    ///
    /// Apple's ABI passes every variadic argument on the stack, which makes the
    /// stack copy sufficient on macOS and hides an omission here entirely. AAPCS64
    /// passes the first one in `x3`, or `d0` when the conversion names a double, so
    /// only Linux shows the defect -- as wrong bytes at the right length, which a
    /// length comparison cannot see. Hence a check on the emitted text.
    #[test]
    fn test_emit_sprintf_overflow_restores_variadic_argument() {
        for (platform, wants_fp_register) in
            [(Platform::Linux, true), (Platform::MacOS, false)]
        {
            let mut emitter = Emitter::new(Target::new(platform, Arch::AArch64));
            emit_sprintf(&mut emitter);
            let asm = emitter.output();

            let start = asm
                .find("__rt_sprintf_overflow:\n")
                .expect("overflow label missing");
            let end = asm[start..]
                .find("__rt_sprintf_overflow_done:\n")
                .map(|offset| start + offset)
                .expect("overflow-done label missing after the overflow section");
            let section = &asm[start..end];

            assert!(
                section.contains("ldr x3, [sp]\n"),
                "{:?}: the re-render must reload the variadic payload",
                platform
            );
            assert_eq!(
                section.contains("fmov d0, x3\n"),
                wants_fp_register,
                "{:?}: the FP register is AAPCS64's variadic double slot only",
                platform
            );
            assert!(
                section.find("ldr x3, [sp]\n") < section.find("bl _snprintf").or(section.find("bl snprintf")),
                "{:?}: the reload must precede the re-render call",
                platform
            );
        }
    }
}
