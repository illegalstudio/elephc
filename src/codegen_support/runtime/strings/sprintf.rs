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
    emitter.instruction("add x10, sp, #80");                                    // mini format buffer start
    emitter.instruction("mov w15, #37");                                        // '%' character
    emitter.instruction("strb w15, [x10], #1");                                 // write '%' to mini buffer

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
    emitter.instruction("b.eq __rt_sprintf_copy_flag");                         // yes → copy it
    emitter.instruction("cmp w12, #35");                                        // '#' flag?
    emitter.instruction("b.eq __rt_sprintf_copy_flag");                         // yes → copy it
    emitter.instruction("b __rt_sprintf_scan_width");                           // no flag → try width

    emitter.label("__rt_sprintf_copy_flag");
    emitter.instruction("strb w12, [x10], #1");                                 // copy flag char to mini buffer
    emitter.instruction("add x19, x19, #1");                                    // consume char from format
    emitter.instruction("sub x20, x20, #1");                                    // decrement remaining
    emitter.instruction("b __rt_sprintf_scan_flags");                           // check for more flags

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
    emitter.instruction("b __rt_sprintf_type_int");                             // default → integer

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
    emitter.label("__rt_sprintf_type_str");
    emitter.instruction("strb w12, [x10], #1");                                 // copy 's' to mini buffer
    emitter.instruction("strb wzr, [x10]");                                     // null-terminate format string

    // -- load next arg (string: ptr + tag|len) --
    emitter.instruction("lsl x15, x21, #4");                                    // arg offset = index * 16
    emitter.instruction("add x15, x22, x15");                                   // arg address in caller's stack
    emitter.instruction("ldr x3, [x15]");                                       // load string pointer
    emitter.instruction("ldr x4, [x15, #8]");                                   // load tag|length word
    emitter.instruction("lsr x4, x4, #8");                                      // extract length (shift right 8)
    emitter.instruction("add x21, x21, #1");                                    // increment arg index

    // -- copy string to temp buffer at sp+240 and null-terminate --
    // Limit copy to 127 bytes to fit in our 128-byte buffer
    emitter.instruction("cmp x4, #127");                                        // string longer than buffer?
    emitter.instruction("b.le __rt_sprintf_str_len_ok");                        // no → use actual length
    emitter.instruction("mov x4, #127");                                        // clamp to 127 bytes

    emitter.label("__rt_sprintf_str_len_ok");
    emitter.instruction("add x6, sp, #240");                                    // temp buffer for null-terminated copy
    emitter.instruction("mov x7, x4");                                          // bytes to copy

    emitter.label("__rt_sprintf_strcopy");
    emitter.instruction("cbz x7, __rt_sprintf_strcopy_done");                   // done copying
    emitter.instruction("ldrb w15, [x3], #1");                                  // load source byte
    emitter.instruction("strb w15, [x6], #1");                                  // write to temp buffer
    emitter.instruction("sub x7, x7, #1");                                      // decrement counter
    emitter.instruction("b __rt_sprintf_strcopy");                              // continue copying

    emitter.label("__rt_sprintf_strcopy_done");
    emitter.instruction("strb wzr, [x6]");                                      // null-terminate the copy

    // -- store variadic arg (pointer to null-terminated copy) on stack --
    emitter.instruction("add x3, sp, #240");                                    // pointer to null-terminated string
    emitter.instruction("str x3, [sp]");                                        // variadic string ptr at [sp]

    // -- call snprintf(buf, 128, fmt) with variadic string ptr on stack --
    emitter.instruction("add x0, sp, #112");                                    // output buffer at sp+112
    emitter.instruction("mov x1, #128");                                        // buffer size
    emitter.instruction("add x2, sp, #80");                                     // mini format string at sp+80
    emitter.bl_c("snprintf");                                        // call libc snprintf
    // x0 = number of chars written

    // -- copy snprintf result to concat_buf --
    emitter.instruction("mov x4, x0");                                          // chars to copy
    emitter.instruction("add x3, sp, #112");                                    // source buffer

    emitter.label("__rt_sprintf_copy_s");
    emitter.instruction("cbz x4, __rt_sprintf_copy_s_done");                    // no bytes left
    emitter.instruction("ldrb w15, [x3], #1");                                  // load byte from snprintf output
    emitter.instruction("strb w15, [x23], #1");                                 // write to concat_buf
    emitter.instruction("sub x4, x4, #1");                                      // decrement counter
    emitter.instruction("b __rt_sprintf_copy_s");                               // continue copying

    emitter.label("__rt_sprintf_copy_s_done");
    emitter.instruction("b __rt_sprintf_loop");                                 // next format char

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
}
