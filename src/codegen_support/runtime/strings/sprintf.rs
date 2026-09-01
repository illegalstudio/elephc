//! Purpose:
//! Emits the `__rt_sprintf` runtime helper assembly, the shared PHP `printf`-family
//! formatter behind `sprintf()`, `printf()`, `fprintf()`, and (through `__rt_vsprintf`)
//! the `v*printf()` family. This file owns the AArch64 lowering; the Linux x86_64
//! lowering lives in `sprintf_x86_64.rs` and must stay behaviourally identical.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - The helper parses each `%` specifier itself into numeric registers/frame slots
//!   (argument number, flags, pad character, width, precision, conversion character).
//!   Format bytes supplied by the program are never copied verbatim into the C format
//!   string handed to libc, so an over-long specifier cannot overrun the mini format
//!   buffer and an unknown conversion (notably `%n`) never reaches `snprintf`.
//! - Width and padding are applied by this helper, not by `snprintf`. libc only renders
//!   the unpadded numeric body into a fixed 512-byte scratch, whose worst case
//!   (`%.53f` of `DBL_MAX` → 363 bytes) is bounded because precision is clamped to
//!   PHP's 53-digit maximum. `%s`, `%b`, and `%c` bypass libc entirely.
//! - Every byte written into `_concat_buf` is bounds-checked against the end of that
//!   64 KiB arena; an oversized result is a controlled fatal, never an overrun.
//! - The 16-byte argument records carry a type tag, and each conversion coerces the operand
//!   to what it needs (double↔int, string→number, and deferred boxed non-scalars through the
//!   sprintf-specific Mixed helpers). A record whose tag disagrees with the conversion
//!   character — which happens for `v*printf()`, a runtime-built format string, or
//!   `%1$s`/`%1$d` on one argument — is therefore converted, never printed as a raw pointer.
//! - PHP's `%e`/`%E` exponent is not zero-padded (`1.234568e+4`, not `e+04`), so the
//!   libc output is compacted in place before it is emitted.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::{Arch, Platform};
use crate::codegen_support::runtime::data::{
    SPRINTF_ARGCOUNT_MSG, SPRINTF_OVERFLOW_MSG, SPRINTF_UNKNOWN_SPEC_MSG, SPRINTF_WIDTH_MSG,
};

use super::sprintf_x86_64::emit_sprintf_linux_x86_64;

/// Byte capacity of the shared `_concat_buf` result arena declared in
/// `crate::codegen_support::runtime::data::emit_runtime_data_fixed`. Both `__rt_sprintf`
/// lowerings derive their write limit from this constant, so the two stay in step.
pub(super) const CONCAT_BUF_CAP: u32 = 65536;

/// Byte capacity of the per-conversion `snprintf` scratch buffer. PHP clamps float
/// precision to 53 digits, so the widest libc body is `%.53f` of `DBL_MAX`
/// (309 integer digits + `.` + 53 fraction digits + sign = 364 bytes); 512 leaves
/// headroom and every copy out of it is still clamped to `CONV_SCRATCH_CAP - 1`.
pub(super) const CONV_SCRATCH_CAP: u32 = 512;

/// Emits the `__rt_sprintf` global runtime helper for `printf`-family formatting.
///
/// # Input (AArch64)
/// - `x0`: number of packed variadic argument records pushed by the caller
/// - `x1`: format string pointer
/// - `x2`: format string byte length
/// - `x3`: optional persistent eval context for eval-declared `__toString()` dispatch
/// - `[sp]` of the caller: `x0` records of 16 bytes, `[payload, tag]`, first argument lowest
///
/// # Output (AArch64)
/// - `x1`: result pointer inside `_concat_buf`
/// - `x2`: result byte length
///
/// The record tag word is `0` for int, `1 | (len << 8)` for string, `2` for float, `3` for
/// bool, `7` for a deferred boxed `Mixed`, and `4`/`5`/`6`/`9`/`10`/`11` for raw indexed-array,
/// associative-array, object, resource, callable, or erased-iterable payloads. The helper
/// consults it so a conversion never dereferences a payload that is not a string pointer.
/// `_concat_off` is advanced by the result length and the caller's `arg_count * 16` bytes of
/// records are popped before returning.
///
/// Callee-saved registers used: `x19` = format cursor, `x20` = remaining format bytes,
/// `x21` = next sequential argument index, `x22` = argument record base, `x23` = write
/// cursor in `_concat_buf`, `x24` = result start, `x25` = `_concat_off` address,
/// `x26` = argument count, `x27` = optional eval context.
pub fn emit_sprintf(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_sprintf_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: sprintf ---");
    emitter.label_global("__rt_sprintf");

    // Frame layout (704 bytes). Every stp/ldp offset stays inside the ±504 scaled
    // immediate range, so the saved register pairs live at the bottom of the frame:
    //   sp+0..7     = first variadic slot for snprintf (Apple AArch64 needs it at sp)
    //   sp+8..15    = saved x27 (optional eval context)
    //   sp+16..31   = saved x29, x30
    //   sp+32..95   = saved x19..x26
    //   sp+96..103  = parsed field width
    //   sp+104..111 = parsed precision (-1 when the specifier had no '.')
    //   sp+112..119 = parsed flags: bit0 left-align, bit1 force sign, bit2 alternate form
    //   sp+120..127 = parsed pad character
    //   sp+128..135 = parsed conversion character
    //   sp+136..143 = parsed argument number (0 = consume the next sequential argument)
    //   sp+144..151 = one-past-the-end address of _concat_buf
    //   sp+152..159 = padding
    //   sp+160..191 = mini C format string built by this helper (never copied from input)
    //   sp+192..703 = snprintf conversion scratch (CONV_SCRATCH_CAP bytes)

    emitter.instruction("sub sp, sp, #704");                                    // allocate the sprintf helper frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // set frame pointer

    // -- save callee-saved registers --
    emitter.instruction("stp x19, x20, [sp, #32]");                             // save x19, x20
    emitter.instruction("stp x21, x22, [sp, #48]");                             // save x21, x22
    emitter.instruction("stp x23, x24, [sp, #64]");                             // save x23, x24
    emitter.instruction("stp x25, x26, [sp, #80]");                             // save x25, x26
    emitter.instruction("str x27, [sp, #8]");                                   // preserve the caller's callee-saved register

    // -- initialize state in callee-saved registers --
    emitter.instruction("mov x19, x1");                                         // format cursor
    emitter.instruction("mov x20, x2");                                         // remaining format bytes
    emitter.instruction("mov x26, x0");                                         // packed argument record count
    emitter.instruction("mov x27, x3");                                         // optional eval context for dynamic Stringable dispatch
    emitter.instruction("mov x21, #0");                                         // next sequential argument index
    emitter.instruction("add x22, sp, #704");                                   // argument record base (just past this frame)

    // -- set up the concat_buf destination and its hard write limit --
    abi::emit_symbol_address(emitter, "x25", "_concat_off");
    emitter.instruction("ldr x8, [x25]");                                       // current concat-buffer write offset
    abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x23, x7, x8");                                     // write cursor = buffer base + offset
    emitter.instruction("mov x24, x23");                                        // remember where this result starts
    emitter.instruction(&format!("mov x9, #{}", CONCAT_BUF_CAP));               // total concat-buffer capacity in bytes
    emitter.instruction("add x9, x7, x9");                                      // one-past-the-end address of the concat buffer
    emitter.instruction("str x9, [sp, #144]");                                  // publish the hard write limit for every copy below
    emitter.instruction("str xzr, [sp, #152]");                                 // no formatter-owned temporary string is live

    // ================================================================
    // MAIN SCAN LOOP: literal bytes are copied, '%' starts a specifier
    // ================================================================
    emitter.label("__rt_sprintf_loop");
    emitter.instruction("cbz x20, __rt_sprintf_done");                          // no format bytes left
    emitter.instruction("ldrb w12, [x19], #1");                                 // load the next format byte and advance
    emitter.instruction("sub x20, x20, #1");                                    // account for the consumed format byte
    emitter.instruction("cmp w12, #37");                                        // is it '%'?
    emitter.instruction("b.eq __rt_sprintf_fmt");                               // yes → parse a conversion specifier
    emitter.instruction("ldr x9, [sp, #144]");                                  // reload the concat-buffer write limit
    emitter.instruction("cmp x23, x9");                                         // would this literal byte land outside the arena?
    emitter.instruction("b.hs __rt_sprintf_ofatal");                            // yes → controlled fatal instead of an overrun
    emitter.instruction("strb w12, [x23], #1");                                 // copy the literal byte to the result
    emitter.instruction("b __rt_sprintf_loop");                                 // continue scanning

    emitter.label("__rt_sprintf_fmt");
    emitter.instruction("cbz x20, __rt_sprintf_done");                          // trailing '%' with nothing after it
    emitter.instruction("ldrb w12, [x19]");                                     // peek at the byte after '%'
    emitter.instruction("cmp w12, #37");                                        // is the sequence '%%'?
    emitter.instruction("b.ne __rt_sprintf_spec");                              // no → parse a real specifier
    emitter.instruction("add x19, x19, #1");                                    // consume the second '%'
    emitter.instruction("sub x20, x20, #1");                                    // account for the consumed byte
    emitter.instruction("ldr x9, [sp, #144]");                                  // reload the concat-buffer write limit
    emitter.instruction("cmp x23, x9");                                         // would the literal '%' land outside the arena?
    emitter.instruction("b.hs __rt_sprintf_ofatal");                            // yes → controlled fatal instead of an overrun
    emitter.instruction("strb w12, [x23], #1");                                 // emit the literal '%'
    emitter.instruction("b __rt_sprintf_loop");                                 // continue scanning

    emit_spec_parser(emitter);
    emit_argument_fetch(emitter);
    emit_conversion_dispatch(emitter);
    emit_string_conversion(emitter);
    emit_binary_conversion(emitter);
    emit_char_conversion(emitter);
    emit_integer_conversion(emitter);
    emit_float_conversion(emitter);
    emit_snprintf_result(emitter);
    emit_exponent_compaction(emitter);
    emit_pad_and_copy(emitter);

    // ================================================================
    // DONE: publish the result and pop the caller's argument records
    // ================================================================
    emitter.label("__rt_sprintf_done");
    emitter.instruction("mov x1, x24");                                         // result pointer inside the concat buffer
    emitter.instruction("sub x2, x23, x24");                                    // result byte length

    // -- update concat_off --
    abi::emit_symbol_address(emitter, "x8", "_concat_buf");
    emitter.instruction("sub x8, x23, x8");                                     // derive the absolute cursor after nested concat-producing conversions
    emitter.instruction("str x8, [x25]");                                       // publish the exact new write offset without double-counting

    // -- prepare to pop the caller's packed argument records --
    emitter.instruction("mov x0, x26");                                         // packed argument record count
    emitter.instruction("lsl x0, x0, #4");                                      // records are 16 bytes each

    // -- restore callee-saved registers and unwind --
    emitter.instruction("ldp x19, x20, [sp, #32]");                             // restore x19, x20
    emitter.instruction("ldp x21, x22, [sp, #48]");                             // restore x21, x22
    emitter.instruction("ldp x23, x24, [sp, #64]");                             // restore x23, x24
    emitter.instruction("ldp x25, x26, [sp, #80]");                             // restore x25, x26
    emitter.instruction("ldr x27, [sp, #8]");                                   // restore the caller's eval-context register
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #704");                                    // release the sprintf helper frame
    emitter.instruction("add sp, sp, x0");                                      // pop the caller's packed argument records
    emitter.instruction("ret");                                                 // return the formatted string in x1/x2

    emit_fatal_paths(emitter);
}

/// Emits an AArch64 decimal-number scanner used for the argument number, the field width,
/// and the precision.
///
/// `ptr`/`len` are the source cursor and remaining-byte count; both are advanced past the
/// digits that were consumed. `acc` receives the parsed value and `count` the digit count;
/// both must be zeroed by the caller. `w15` is left holding the first non-digit byte, or
/// zero when the input ran out (so the caller can distinguish "stopped on `$`" from
/// "ran out of format"). `w12`/`x12` are clobbered.
///
/// Accumulation stops after 10 digits and any longer run saturates to `0x80000000`, which
/// keeps the accumulator inside 64 bits and makes "wider than `INT_MAX`" detectable as
/// `acc >> 31 != 0` no matter how many digits the program supplied.
fn emit_scan_decimal(emitter: &mut Emitter, prefix: &str, ptr: &str, len: &str, acc: &str, count: &str) {
    emitter.label(&format!("{}_loop", prefix));
    emitter.instruction(&format!("cbz {}, {}_end0", len, prefix));              // ran out of format bytes
    emitter.instruction(&format!("ldrb w15, [{}]", ptr));                       // peek at the current byte
    emitter.instruction("sub w12, w15, #48");                                   // convert the byte to a digit value
    emitter.instruction("cmp w12, #9");                                         // is it outside '0'..'9'?
    emitter.instruction(&format!("b.hi {}_done", prefix));                      // yes → the number ends here
    emitter.instruction(&format!("cmp {}, #10", count));                        // already accumulated ten digits?
    emitter.instruction(&format!("b.hs {}_skip", prefix));                      // yes → stop accumulating, just count
    emitter.instruction(&format!("add {0}, {0}, {0}, lsl #2", acc));            // accumulator *= 5
    emitter.instruction(&format!("lsl {0}, {0}, #1", acc));                     // accumulator *= 2, so *= 10 overall
    emitter.instruction(&format!("add {0}, {0}, x12", acc));                    // add the current digit
    emitter.label(&format!("{}_skip", prefix));
    emitter.instruction(&format!("add {0}, {0}, #1", count));                   // count the consumed digit
    emitter.instruction(&format!("add {0}, {0}, #1", ptr));                     // advance the source cursor
    emitter.instruction(&format!("sub {0}, {0}, #1", len));                     // account for the consumed byte
    emitter.instruction(&format!("b {}_loop", prefix));                         // scan the next digit
    emitter.label(&format!("{}_end0", prefix));
    emitter.instruction("mov w15, #0");                                         // no lookahead byte is available
    emitter.label(&format!("{}_done", prefix));
    emitter.instruction(&format!("cmp {}, #10", count));                        // did the run exceed ten digits?
    emitter.instruction(&format!("b.ls {}_nosat", prefix));                     // no → keep the accumulated value
    emitter.instruction(&format!("mov {}, #0x80000000", acc));                  // saturate above INT_MAX so the range check fires
    emitter.label(&format!("{}_nosat", prefix));
}

/// Emits the AArch64 specifier parser: argument number, flags, pad character, width and
/// precision are decoded into frame slots and the conversion character is stored last.
///
/// Nothing here copies program-supplied bytes into a buffer, so an arbitrarily long
/// specifier costs scan time only — it can never overrun the mini format buffer.
fn emit_spec_parser(emitter: &mut Emitter) {
    // -- reset the per-specifier state --
    emitter.label("__rt_sprintf_spec");
    emitter.instruction("str xzr, [sp, #96]");                                  // width = 0
    emitter.instruction("mov x9, #-1");                                         // sentinel meaning "no precision given"
    emitter.instruction("str x9, [sp, #104]");                                  // precision = absent
    emitter.instruction("str xzr, [sp, #112]");                                 // flags = none
    emitter.instruction("mov w9, #32");                                         // PHP's default pad character is a space
    emitter.instruction("str x9, [sp, #120]");                                  // pad character = ' '
    emitter.instruction("str xzr, [sp, #136]");                                 // argument number = sequential

    // -- optional "N$" argument number: only committed when a '$' follows the digits --
    emitter.instruction("mov x9, x19");                                         // lookahead cursor (does not consume yet)
    emitter.instruction("mov x10, x20");                                        // lookahead remaining-byte count
    emitter.instruction("mov x11, #0");                                         // argument-number accumulator
    emitter.instruction("mov x14, #0");                                         // argument-number digit count
    emit_scan_decimal(emitter, "__rt_sprintf_an", "x9", "x10", "x11", "x14");
    emitter.instruction("cbz x14, __rt_sprintf_flags");                         // no digits → not an argument number
    emitter.instruction("cmp w15, #36");                                        // is the byte after the digits '$'?
    emitter.instruction("b.ne __rt_sprintf_flags");                             // no → those digits are the field width
    emitter.instruction("str x11, [sp, #136]");                                 // commit the explicit argument number
    emitter.instruction("add x19, x9, #1");                                     // consume the digits and the '$'
    emitter.instruction("sub x20, x10, #1");                                    // account for the consumed '$'

    // -- flags: '-', '+', '0', ' ', '#', and PHP's "'X" custom pad character --
    emitter.label("__rt_sprintf_flags");
    emitter.instruction("cbz x20, __rt_sprintf_endspec");                       // format ended inside the specifier
    emitter.instruction("ldrb w12, [x19]");                                     // peek at the current specifier byte
    emitter.instruction("cmp w12, #45");                                        // '-' left-align flag?
    emitter.instruction("b.eq __rt_sprintf_fl_left");                           // yes → record left alignment
    emitter.instruction("cmp w12, #43");                                        // '+' force-sign flag?
    emitter.instruction("b.eq __rt_sprintf_fl_plus");                           // yes → record the forced sign
    emitter.instruction("cmp w12, #48");                                        // '0' zero-pad flag?
    emitter.instruction("b.eq __rt_sprintf_fl_zero");                           // yes → pad character becomes '0'
    emitter.instruction("cmp w12, #32");                                        // ' ' space-pad flag?
    emitter.instruction("b.eq __rt_sprintf_fl_space");                          // yes → pad character becomes ' '
    emitter.instruction("cmp w12, #35");                                        // '#' alternate-form flag?
    emitter.instruction("b.eq __rt_sprintf_fl_alt");                            // yes → record the alternate form
    emitter.instruction("cmp w12, #39");                                        // "'" custom-pad-character flag?
    emitter.instruction("b.eq __rt_sprintf_fl_pad");                            // yes → the next byte is the pad character
    emitter.instruction("b __rt_sprintf_width");                                // no more flags → parse the width

    emitter.label("__rt_sprintf_fl_left");
    emitter.instruction("ldr x9, [sp, #112]");                                  // load the parsed flags
    emitter.instruction("orr x9, x9, #1");                                      // set the left-align bit
    emitter.instruction("str x9, [sp, #112]");                                  // store the parsed flags
    emitter.instruction("b __rt_sprintf_fl_next");                              // consume the flag byte

    emitter.label("__rt_sprintf_fl_plus");
    emitter.instruction("ldr x9, [sp, #112]");                                  // load the parsed flags
    emitter.instruction("orr x9, x9, #2");                                      // set the force-sign bit
    emitter.instruction("str x9, [sp, #112]");                                  // store the parsed flags
    emitter.instruction("b __rt_sprintf_fl_next");                              // consume the flag byte

    emitter.label("__rt_sprintf_fl_alt");
    emitter.instruction("ldr x9, [sp, #112]");                                  // load the parsed flags
    emitter.instruction("orr x9, x9, #4");                                      // set the alternate-form bit
    emitter.instruction("str x9, [sp, #112]");                                  // store the parsed flags
    emitter.instruction("b __rt_sprintf_fl_next");                              // consume the flag byte

    emitter.label("__rt_sprintf_fl_zero");
    emitter.instruction("mov w9, #48");                                         // '0' becomes the pad character
    emitter.instruction("str x9, [sp, #120]");                                  // store the pad character
    emitter.instruction("b __rt_sprintf_fl_next");                              // consume the flag byte

    emitter.label("__rt_sprintf_fl_space");
    emitter.instruction("mov w9, #32");                                         // ' ' becomes the pad character
    emitter.instruction("str x9, [sp, #120]");                                  // store the pad character
    emitter.instruction("b __rt_sprintf_fl_next");                              // consume the flag byte

    emitter.label("__rt_sprintf_fl_pad");
    emitter.instruction("add x19, x19, #1");                                    // consume the "'" introducer
    emitter.instruction("sub x20, x20, #1");                                    // account for the consumed byte
    emitter.instruction("cbz x20, __rt_sprintf_endspec");                       // "'" at end of format → nothing to pad with
    emitter.instruction("ldrb w9, [x19]");                                      // the next byte is the custom pad character
    emitter.instruction("str x9, [sp, #120]");                                  // store the custom pad character

    emitter.label("__rt_sprintf_fl_next");
    emitter.instruction("add x19, x19, #1");                                    // consume the flag byte
    emitter.instruction("sub x20, x20, #1");                                    // account for the consumed byte
    emitter.instruction("b __rt_sprintf_flags");                                // look for another flag

    // -- field width --
    emitter.label("__rt_sprintf_width");
    emitter.instruction("mov x11, #0");                                         // width accumulator
    emitter.instruction("mov x14, #0");                                         // width digit count
    emit_scan_decimal(emitter, "__rt_sprintf_w", "x19", "x20", "x11", "x14");
    emitter.instruction("str x11, [sp, #96]");                                  // store the parsed field width

    // -- optional ".precision" --
    emitter.instruction("cbz x20, __rt_sprintf_endspec");                       // format ended before the conversion
    emitter.instruction("ldrb w12, [x19]");                                     // peek at the current specifier byte
    emitter.instruction("cmp w12, #46");                                        // '.' precision introducer?
    emitter.instruction("b.ne __rt_sprintf_slength");                           // no → try PHP's optional `l` length modifier
    emitter.instruction("add x19, x19, #1");                                    // consume the '.'
    emitter.instruction("sub x20, x20, #1");                                    // account for the consumed byte
    emitter.instruction("mov x11, #0");                                         // precision accumulator ('.' alone means 0)
    emitter.instruction("mov x14, #0");                                         // precision digit count
    emit_scan_decimal(emitter, "__rt_sprintf_p", "x19", "x20", "x11", "x14");
    emitter.instruction("str x11, [sp, #104]");                                 // store the parsed precision

    // -- optional single `l` modifier --
    emitter.label("__rt_sprintf_slength");
    emitter.instruction("cbz x20, __rt_sprintf_endspec");                       // format ended before the conversion
    emitter.instruction("ldrb w12, [x19]");                                     // peek at the possible length modifier
    emitter.instruction("cmp w12, #108");                                       // ASCII `l`
    emitter.instruction("b.ne __rt_sprintf_stype");                             // current byte is already the conversion
    emitter.instruction("add x19, x19, #1");                                    // consume the modifier
    emitter.instruction("sub x20, x20, #1");                                    // account for the consumed byte

    // -- conversion character --
    emitter.label("__rt_sprintf_stype");
    emitter.instruction("cbz x20, __rt_sprintf_endspec");                       // format ended before the conversion
    emitter.instruction("ldrb w12, [x19], #1");                                 // load the conversion character and advance
    emitter.instruction("sub x20, x20, #1");                                    // account for the consumed byte
    emitter.instruction("str x12, [sp, #128]");                                 // store the conversion character
    emitter.instruction("b __rt_sprintf_arg");                                  // fetch the argument this conversion consumes

    emitter.label("__rt_sprintf_endspec");
    emitter.instruction("b __rt_sprintf_done");                                 // truncated specifier → stop formatting
}

/// Emits the AArch64 argument fetch: resolves the sequential or explicit `N$` argument
/// index, rejects out-of-range indices, and loads the 16-byte record into `x3`/`x4`.
///
/// The range check is what keeps the helper from reading the caller's stack past the
/// pushed records when a format string requests more arguments than were supplied.
fn emit_argument_fetch(emitter: &mut Emitter) {
    emitter.label("__rt_sprintf_arg");
    emitter.instruction("ldr x13, [sp, #136]");                                 // parsed argument number (0 = sequential)
    emitter.instruction("cbz x13, __rt_sprintf_arg_seq");                       // no explicit number → take the next argument
    emitter.instruction("sub x9, x13, #1");                                     // PHP argument numbers are 1-based
    emitter.instruction("b __rt_sprintf_arg_have");                             // index resolved
    emitter.label("__rt_sprintf_arg_seq");
    emitter.instruction("mov x9, x21");                                         // consume the next sequential argument
    emitter.instruction("add x21, x21, #1");                                    // advance the sequential cursor
    emitter.label("__rt_sprintf_arg_have");
    emitter.instruction("cmp x9, x26");                                         // is the index within the supplied records?
    emitter.instruction("b.hs __rt_sprintf_afatal");                            // no → controlled fatal instead of a stack read
    emitter.instruction("lsl x10, x9, #4");                                     // records are 16 bytes each
    emitter.instruction("add x10, x22, x10");                                   // address of the selected record
    emitter.instruction("ldr x3, [x10]");                                       // record payload word
    emitter.instruction("ldr x4, [x10, #8]");                                   // record tag word (tag | length << 8)
    emitter.instruction("ldrb w12, [sp, #128]");                                // reload the conversion character
}

/// Emits the AArch64 conversion dispatch. Only the conversion characters PHP defines are
/// accepted; anything else takes the controlled `ValueError` path rather than being handed
/// to libc, which is what keeps `%n` and other libc-only conversions unreachable.
fn emit_conversion_dispatch(emitter: &mut Emitter) {
    emitter.instruction("cmp w12, #115");                                       // 's' string conversion?
    emitter.instruction("b.eq __rt_sprintf_t_str");                             // yes → string path
    emitter.instruction("cmp w12, #100");                                       // 'd' signed decimal?
    emitter.instruction("b.eq __rt_sprintf_t_int");                             // yes → integer path
    emitter.instruction("cmp w12, #117");                                       // 'u' unsigned decimal?
    emitter.instruction("b.eq __rt_sprintf_t_int");                             // yes → integer path
    emitter.instruction("cmp w12, #111");                                       // 'o' octal?
    emitter.instruction("b.eq __rt_sprintf_t_int");                             // yes → integer path
    emitter.instruction("cmp w12, #120");                                       // 'x' lowercase hexadecimal?
    emitter.instruction("b.eq __rt_sprintf_t_int");                             // yes → integer path
    emitter.instruction("cmp w12, #88");                                        // 'X' uppercase hexadecimal?
    emitter.instruction("b.eq __rt_sprintf_t_int");                             // yes → integer path
    emitter.instruction("cmp w12, #98");                                        // 'b' binary?
    emitter.instruction("b.eq __rt_sprintf_t_int");                             // yes → integer coercion, then the binary body
    emitter.instruction("cmp w12, #99");                                        // 'c' single character?
    emitter.instruction("b.eq __rt_sprintf_t_int");                             // yes → integer coercion, then the single-byte body
    emitter.instruction("cmp w12, #102");                                       // 'f' fixed-point?
    emitter.instruction("b.eq __rt_sprintf_t_flt");                             // yes → float path
    emitter.instruction("cmp w12, #70");                                        // 'F' locale-independent fixed-point?
    emitter.instruction("b.eq __rt_sprintf_t_flt");                             // yes → float path
    emitter.instruction("cmp w12, #101");                                       // 'e' scientific?
    emitter.instruction("b.eq __rt_sprintf_t_flt");                             // yes → float path
    emitter.instruction("cmp w12, #69");                                        // 'E' uppercase scientific?
    emitter.instruction("b.eq __rt_sprintf_t_flt");                             // yes → float path
    emitter.instruction("cmp w12, #103");                                       // 'g' shortest-of-e-or-f?
    emitter.instruction("b.eq __rt_sprintf_t_flt");                             // yes → float path
    emitter.instruction("cmp w12, #71");                                        // 'G' uppercase shortest-of-E-or-f?
    emitter.instruction("b.eq __rt_sprintf_t_flt");                             // yes → float path
    emitter.instruction("b __rt_sprintf_sfatal");                               // PHP rejects every other conversion
}

/// Emits the AArch64 `%s` conversion.
///
/// A string record is emitted straight from its pointer/length pair (so the result is
/// binary safe and not capped at any scratch-buffer size); precision truncates it. A
/// record carrying another tag is rendered numerically instead of being dereferenced.
fn emit_string_conversion(emitter: &mut Emitter) {
    emitter.label("__rt_sprintf_t_str");
    emitter.instruction("str xzr, [sp, #152]");                                 // this conversion owns no temporary string yet
    emitter.instruction("and x5, x4, #255");                                    // isolate the record type tag
    emit_branch_if_deferred_tag(emitter, "x5", "__rt_sprintf_str_mixed");
    emitter.instruction("cmp x5, #3");                                          // boolean record?
    emitter.instruction("b.ne __rt_sprintf_str_not_bool");                      // no → ordinary string/numeric dispatch
    emitter.instruction("cbnz x3, __rt_sprintf_str_num");                       // true renders as integer one
    emitter.instruction("mov x3, #0");                                          // false renders as the empty string
    emitter.instruction("mov x4, #0");                                          // zero output bytes
    emitter.instruction("b __rt_sprintf_str_ptr");                              // apply width/precision to the empty body
    emitter.label("__rt_sprintf_str_not_bool");
    emitter.instruction("cmp x5, #1");                                          // is this record actually a string?
    emitter.instruction("b.ne __rt_sprintf_str_num");                           // no → render the payload as a number
    emitter.instruction("lsr x4, x4, #8");                                      // string byte length lives above the tag
    emitter.instruction("cbnz x3, __rt_sprintf_str_ptr");                       // a null pointer carries no bytes
    emitter.instruction("mov x4, #0");                                          // treat a null string pointer as empty
    emitter.label("__rt_sprintf_str_ptr");
    emitter.instruction("ldr x5, [sp, #104]");                                  // parsed precision
    emitter.instruction("tbnz x5, #63, __rt_sprintf_emit");                     // no precision → emit the whole string
    emitter.instruction("cmp x4, x5");                                          // is the string already within the precision?
    emitter.instruction("b.ls __rt_sprintf_emit");                              // yes → emit it unchanged
    emitter.instruction("mov x4, x5");                                          // truncate the string to the precision
    emitter.instruction("b __rt_sprintf_emit");                                 // pad and copy the string body

    // -- non-string record under %s: format the payload instead of dereferencing it --
    emitter.label("__rt_sprintf_str_num");
    emitter.instruction("mov x9, #-1");                                         // the %s precision must not reach the numeric path
    emitter.instruction("str x9, [sp, #104]");                                  // drop the string precision
    emitter.instruction("cmp x5, #2");                                          // is the payload a double?
    emitter.instruction("b.ne __rt_sprintf_str_int");                           // no → render it as a signed integer
    emitter.instruction("mov x9, #14");                                         // PHP renders floats with 14 significant digits
    emitter.instruction("str x9, [sp, #104]");                                  // use that as the conversion precision
    emitter.instruction("mov w12, #71");                                        // reuse the 'G' float conversion
    emitter.instruction("str x12, [sp, #128]");                                 // record the substituted conversion character
    emitter.instruction("b __rt_sprintf_t_flt");                                // format through the float path
    emitter.label("__rt_sprintf_str_int");
    emitter.instruction("mov w12, #100");                                       // reuse the 'd' integer conversion
    emitter.instruction("str x12, [sp, #128]");                                 // record the substituted conversion character
    emitter.instruction("b __rt_sprintf_t_int");                                // format through the integer path

    emitter.label("__rt_sprintf_str_mixed");
    abi::emit_symbol_address(emitter, "x9", "_concat_buf");
    emitter.instruction("sub x10, x23, x9");                                    // publish bytes already written before a nested __toString call
    emitter.instruction("str x10, [x25]");                                      // make nested concat users start after the partial sprintf result
    emitter.instruction("mov x0, x5");                                          // pass the deferred record tag
    emitter.instruction("mov x1, x3");                                          // pass the preserved record payload
    emitter.instruction("mov x2, x27");                                         // pass the optional eval context
    emitter.instruction("bl __rt_sprintf_mixed_to_string");                     // apply array/resource/object string semantics
    emitter.instruction("str x0, [sp, #152]");                                  // release an owned stabilized result after copying
    emitter.instruction("mov x3, x1");                                          // replace the record payload with the coerced string pointer
    emitter.instruction("mov x4, x2");                                          // coerced string byte length
    emitter.instruction("b __rt_sprintf_str_ptr");                              // reuse precision, padding, and copy handling
}

/// Branches when an AArch64 sprintf record tag denotes deferred non-scalar coercion.
fn emit_branch_if_deferred_tag(emitter: &mut Emitter, tag_reg: &str, label: &str) {
    for tag in [4, 5, 6, 7, 9, 10, 11] {
        emitter.instruction(&format!("cmp {tag_reg}, #{tag}"));
        emitter.instruction(&format!("b.eq {label}"));
    }
}

/// Emits the AArch64 `%b` conversion body, which libc has no portable equivalent for.
/// Entered from the shared integer coercion with the operand already in `x3`. Digits are
/// generated backwards into the conversion scratch, so at most 64 bytes are written and the
/// result never carries leading zeros (PHP prints `0` for zero).
fn emit_binary_conversion(emitter: &mut Emitter) {
    emitter.label("__rt_sprintf_bin_go");
    emitter.instruction("add x9, sp, #264");                                    // write backwards from scratch + 72 bytes
    emitter.instruction("mov x4, #0");                                          // generated digit count
    emitter.label("__rt_sprintf_bin_loop");
    emitter.instruction("and x10, x3, #1");                                     // take the low bit of the remaining value
    emitter.instruction("add w10, w10, #48");                                   // turn it into an ASCII digit
    emitter.instruction("sub x9, x9, #1");                                      // step one byte back in the scratch
    emitter.instruction("strb w10, [x9]");                                      // store the digit
    emitter.instruction("add x4, x4, #1");                                      // count the digit
    emitter.instruction("lsr x3, x3, #1");                                      // shift the value right by one bit
    emitter.instruction("cbnz x3, __rt_sprintf_bin_loop");                      // more bits → keep going
    emitter.instruction("mov x3, x9");                                          // body pointer = first generated digit
    emitter.instruction("b __rt_sprintf_emit");                                 // pad and copy the binary body
}

/// Emits the AArch64 `%c` conversion body, entered from the shared integer coercion with the
/// operand already in `x3`. PHP appends the low byte of the argument and ignores width and
/// padding entirely, so the width slot is cleared before emitting.
fn emit_char_conversion(emitter: &mut Emitter) {
    emitter.label("__rt_sprintf_chr_go");
    emitter.instruction("add x9, sp, #192");                                    // reuse the conversion scratch for one byte
    emitter.instruction("strb w3, [x9]");                                       // store the low byte of the argument
    emitter.instruction("mov x3, x9");                                          // body pointer = the stored byte
    emitter.instruction("mov x4, #1");                                          // body length = one byte
    emitter.instruction("str xzr, [sp, #96]");                                  // PHP ignores width for %c
    emitter.instruction("b __rt_sprintf_emit");                                 // copy the single byte
}

/// Emits the AArch64 integer conversions (`%d`, `%u`, `%o`, `%x`, `%X`) plus the shared
/// operand coercion that `%b` and `%c` also enter through.
///
/// The record tag decides the coercion: a double is truncated toward zero and a string is
/// parsed by `__rt_str_to_int`. Without that string case the helper would print the operand
/// pointer whenever the conversion character and the packed record disagree — which happens
/// for `v*printf()`, for a runtime-built format string, and for `%1$s`/`%1$d` on one
/// argument. The length handed to `__rt_str_to_int` is clamped to the C-string scratch.
///
/// The C format string is assembled from the parsed flags rather than copied from the
/// program, so it is at most `"%+#llX"` plus a NUL. Precision is deliberately omitted:
/// PHP ignores it for integer conversions.
fn emit_integer_conversion(emitter: &mut Emitter) {
    emitter.label("__rt_sprintf_t_int");
    emitter.instruction("and x5, x4, #255");                                    // isolate the record type tag
    emit_branch_if_deferred_tag(emitter, "x5", "__rt_sprintf_int_mixed");
    emitter.instruction("cmp x5, #1");                                          // is the payload a string pointer?
    emitter.instruction("b.eq __rt_sprintf_int_str");                           // yes → parse it instead of printing the pointer
    emitter.instruction("cmp x5, #2");                                          // is the payload a double?
    emitter.instruction("b.ne __rt_sprintf_int_ready");                         // no → the payload is already an integer
    emitter.instruction("fmov d0, x3");                                         // move the double bits into an FP register
    emitter.instruction("fcvtzs x3, d0");                                       // truncate the double toward zero like PHP
    emitter.instruction("b __rt_sprintf_int_ready");                            // the operand is an integer now
    emitter.label("__rt_sprintf_int_str");
    emitter.instruction("mov x1, x3");                                          // string pointer for the numeric parse
    emitter.instruction("lsr x2, x4, #8");                                      // string byte length for the numeric parse
    emitter.instruction("cbz x1, __rt_sprintf_int_str_null");                   // a null pointer parses as zero
    emitter.instruction("cmp x2, #4095");                                       // __rt_cstr copies into a 4096-byte scratch
    emitter.instruction("b.ls __rt_sprintf_int_str_go");                        // the string already fits the C-string scratch
    emitter.instruction("mov x2, #4095");                                       // clamp so the numeric prefix parse stays in bounds
    emitter.label("__rt_sprintf_int_str_go");
    emitter.instruction("bl __rt_str_to_int");                                  // PHP leading-numeric string-to-int conversion
    emitter.instruction("mov x3, x0");                                          // the parsed integer becomes the operand
    emitter.instruction("b __rt_sprintf_int_ready");                            // the operand is an integer now
    emitter.label("__rt_sprintf_int_str_null");
    emitter.instruction("mov x3, #0");                                          // a null string operand formats as zero
    emitter.instruction("b __rt_sprintf_int_ready");                            // join the ordinary integer formatting path
    emitter.label("__rt_sprintf_int_mixed");
    emitter.instruction("mov x0, x5");                                          // pass the deferred record tag
    emitter.instruction("mov x1, x3");                                          // pass the preserved record payload
    emitter.instruction("mov x2, #0");                                          // select integer conversion warning wording
    emitter.instruction("mov x3, x27");                                         // pass the optional eval context for dynamic metadata
    emitter.instruction("bl __rt_sprintf_mixed_to_int");                        // arrays/objects/callables/resources cast without pointer leakage
    emitter.instruction("mov x3, x0");                                          // use the normalized PHP integer as the operand
    emitter.label("__rt_sprintf_int_ready");
    emitter.instruction("ldrb w12, [sp, #128]");                                // reload the conversion character after the parse
    emitter.instruction("cmp w12, #98");                                        // is this the binary conversion?
    emitter.instruction("b.eq __rt_sprintf_bin_go");                            // yes → generate binary digits by hand
    emitter.instruction("cmp w12, #99");                                        // is this the single-character conversion?
    emitter.instruction("b.eq __rt_sprintf_chr_go");                            // yes → emit the low byte directly
    emitter.label("__rt_sprintf_int_go");
    emitter.instruction("add x14, sp, #160");                                   // mini C format cursor
    emitter.instruction("mov w9, #37");                                         // '%' introducer
    emitter.instruction("strb w9, [x14], #1");                                  // write the '%' introducer
    emitter.instruction("cmp w12, #100");                                       // only 'd' is signed, so only it can force a sign
    emitter.instruction("b.ne __rt_sprintf_int_noplus");                        // other integer conversions ignore '+'
    emitter.instruction("ldr x9, [sp, #112]");                                  // parsed flags
    emitter.instruction("tbz x9, #1, __rt_sprintf_int_noplus");                 // force-sign flag not set
    emitter.instruction("mov w9, #43");                                         // '+' flag character
    emitter.instruction("strb w9, [x14], #1");                                  // write the '+' flag
    emitter.label("__rt_sprintf_int_noplus");
    emitter.instruction("cmp w12, #100");                                       // '#' is meaningless for 'd'
    emitter.instruction("b.eq __rt_sprintf_int_noalt");                         // skip the alternate-form flag
    emitter.instruction("cmp w12, #117");                                       // '#' is meaningless for 'u'
    emitter.instruction("b.eq __rt_sprintf_int_noalt");                         // skip the alternate-form flag
    emitter.instruction("ldr x9, [sp, #112]");                                  // parsed flags
    emitter.instruction("tbz x9, #2, __rt_sprintf_int_noalt");                  // alternate-form flag not set
    emitter.instruction("mov w9, #35");                                         // '#' flag character
    emitter.instruction("strb w9, [x14], #1");                                  // write the '#' flag
    emitter.label("__rt_sprintf_int_noalt");
    emitter.instruction("mov w9, #108");                                        // 'l' length modifier character
    emitter.instruction("strb w9, [x14], #1");                                  // write the first 'l'
    emitter.instruction("strb w9, [x14], #1");                                  // write the second 'l' for a 64-bit operand
    emitter.instruction("strb w12, [x14], #1");                                 // write the conversion character
    emitter.instruction("strb wzr, [x14]");                                     // NUL-terminate the mini C format string
    emitter.instruction("str x3, [sp]");                                        // first variadic slot (Apple AArch64 reads it here)
    emitter.instruction("add x0, sp, #192");                                    // conversion scratch destination
    emitter.instruction(&format!("mov x1, #{}", CONV_SCRATCH_CAP));             // conversion scratch capacity
    emitter.instruction("add x2, sp, #160");                                    // the mini C format string
    emitter.bl_c("snprintf");                                                   // render the integer body through libc
    emitter.instruction("b __rt_sprintf_snret");                                // clamp and take the result

}

/// Emits the AArch64 float conversions (`%f`, `%F`, `%e`, `%E`, `%g`, `%G`).
///
/// The record tag decides the coercion: an int/bool payload is widened and a string is
/// parsed by `__rt_str_to_number`, so a mismatched record never reaches libc as raw pointer
/// bits. Precision is clamped to PHP's 53-digit maximum, which is what bounds the libc output
/// to the conversion scratch. `%f`/`%F`/`%e`/`%E` of negative zero print unsigned in PHP
/// (its own float renderer never emits the sign), while `%g`/`%G` keep it.
fn emit_float_conversion(emitter: &mut Emitter) {
    emitter.label("__rt_sprintf_t_flt");
    emitter.instruction("and x5, x4, #255");                                    // isolate the record type tag
    emit_branch_if_deferred_tag(emitter, "x5", "__rt_sprintf_flt_mixed");
    emitter.instruction("cmp x5, #2");                                          // is the payload already a double?
    emitter.instruction("b.eq __rt_sprintf_flt_bits");                          // yes → use its bit pattern directly
    emitter.instruction("cmp x5, #1");                                          // is the payload a string pointer?
    emitter.instruction("b.eq __rt_sprintf_flt_str");                           // yes → parse it instead of reading the pointer bits
    emitter.instruction("scvtf d0, x3");                                        // widen an int/bool payload to a double
    emitter.instruction("fmov x3, d0");                                         // keep the double bits in the integer register
    emitter.instruction("b __rt_sprintf_flt_bits");                             // the operand is a double now
    emitter.label("__rt_sprintf_flt_str");
    emitter.instruction("mov x1, x3");                                          // string pointer for the numeric parse
    emitter.instruction("lsr x2, x4, #8");                                      // string byte length for the numeric parse
    emitter.instruction("cbz x1, __rt_sprintf_flt_str_null");                   // a null pointer parses as zero
    emitter.instruction("cmp x2, #4095");                                       // __rt_cstr copies into a 4096-byte scratch
    emitter.instruction("b.ls __rt_sprintf_flt_str_go");                        // the string already fits the C-string scratch
    emitter.instruction("mov x2, #4095");                                       // clamp so the numeric prefix parse stays in bounds
    emitter.label("__rt_sprintf_flt_str_go");
    emitter.instruction("bl __rt_str_to_number");                               // PHP leading-numeric string-to-float conversion
    emitter.instruction("fmov x3, d0");                                         // keep the parsed double bits in the integer register
    emitter.instruction("ldrb w12, [sp, #128]");                                // reload the conversion character after the parse
    emitter.instruction("b __rt_sprintf_flt_bits");                             // the operand is a double now
    emitter.label("__rt_sprintf_flt_str_null");
    emitter.instruction("mov x3, #0");                                          // a null string operand formats as zero
    emitter.instruction("b __rt_sprintf_flt_bits");                             // join the ordinary floating formatting path
    emitter.label("__rt_sprintf_flt_mixed");
    emitter.instruction("mov x0, x5");                                          // pass the deferred record tag
    emitter.instruction("mov x1, x3");                                          // pass the preserved record payload
    emitter.instruction("mov x2, #1");                                          // select float conversion warning wording
    emitter.instruction("mov x3, x27");                                         // pass the optional eval context for dynamic metadata
    emitter.instruction("bl __rt_sprintf_mixed_to_int");                        // non-scalars share PHP's zero/one/resource-id numeric cast
    emitter.instruction("scvtf d0, x0");                                        // widen the normalized integer to a PHP float operand
    emitter.instruction("fmov x3, d0");                                         // keep the double bits in the record payload register
    emitter.instruction("ldrb w12, [sp, #128]");                                // reload the conversion character clobbered by the helper call
    emitter.label("__rt_sprintf_flt_bits");
    emitter.instruction("cmp w12, #103");                                       // 'g' keeps PHP's negative-zero sign
    emitter.instruction("b.eq __rt_sprintf_flt_nz");                            // skip the negative-zero normalization
    emitter.instruction("cmp w12, #71");                                        // 'G' keeps PHP's negative-zero sign
    emitter.instruction("b.eq __rt_sprintf_flt_nz");                            // skip the negative-zero normalization
    emitter.instruction("lsl x9, x3, #1");                                      // drop the sign bit to test for any zero
    emitter.instruction("cbnz x9, __rt_sprintf_flt_nz");                        // not a zero → leave the value alone
    emitter.instruction("mov x3, #0");                                          // PHP prints -0.0 as 0.000000 under %f/%e
    emitter.label("__rt_sprintf_flt_nz");
    emitter.instruction("add x14, sp, #160");                                   // mini C format cursor
    emitter.instruction("mov w9, #37");                                         // '%' introducer
    emitter.instruction("strb w9, [x14], #1");                                  // write the '%' introducer
    emitter.instruction("ldr x9, [sp, #112]");                                  // parsed flags
    emitter.instruction("tbz x9, #1, __rt_sprintf_flt_noplus");                 // force-sign flag not set
    emitter.instruction("mov w9, #43");                                         // '+' flag character
    emitter.instruction("strb w9, [x14], #1");                                  // write the '+' flag
    emitter.label("__rt_sprintf_flt_noplus");
    emitter.instruction("ldr x9, [sp, #112]");                                  // parsed flags
    emitter.instruction("tbz x9, #2, __rt_sprintf_flt_noalt");                  // alternate-form flag not set
    emitter.instruction("mov w9, #35");                                         // '#' flag character
    emitter.instruction("strb w9, [x14], #1");                                  // write the '#' flag
    emitter.label("__rt_sprintf_flt_noalt");
    emitter.instruction("ldr x5, [sp, #104]");                                  // parsed precision
    emitter.instruction("tbnz x5, #63, __rt_sprintf_flt_noprec");               // absent → libc's default of six digits
    emitter.instruction("cmp x5, #53");                                         // PHP caps float precision at 53 digits
    emitter.instruction("b.ls __rt_sprintf_flt_precok");                        // within the cap
    emitter.instruction("mov x5, #53");                                         // clamp to PHP's maximum precision
    emitter.label("__rt_sprintf_flt_precok");
    emitter.instruction("mov w9, #46");                                         // '.' precision introducer
    emitter.instruction("strb w9, [x14], #1");                                  // write the '.' introducer
    emitter.instruction("cmp x5, #10");                                         // does the precision need two digits?
    emitter.instruction("b.lo __rt_sprintf_flt_prec1");                         // no → a single digit is enough
    emitter.instruction("mov x9, #10");                                         // decimal radix for the split
    emitter.instruction("udiv x10, x5, x9");                                    // tens digit of the clamped precision
    emitter.instruction("msub x11, x10, x9, x5");                               // units digit of the clamped precision
    emitter.instruction("add w10, w10, #48");                                   // turn the tens digit into ASCII
    emitter.instruction("strb w10, [x14], #1");                                 // write the tens digit
    emitter.instruction("add w11, w11, #48");                                   // turn the units digit into ASCII
    emitter.instruction("strb w11, [x14], #1");                                 // write the units digit
    emitter.instruction("b __rt_sprintf_flt_noprec");                           // precision written
    emitter.label("__rt_sprintf_flt_prec1");
    emitter.instruction("add w5, w5, #48");                                     // turn the single digit into ASCII
    emitter.instruction("strb w5, [x14], #1");                                  // write the single precision digit
    emitter.label("__rt_sprintf_flt_noprec");
    emitter.instruction("strb w12, [x14], #1");                                 // write the conversion character
    emitter.instruction("strb wzr, [x14]");                                     // NUL-terminate the mini C format string
    emitter.instruction("str x3, [sp]");                                        // first variadic slot (Apple AArch64 reads it here)
    if emitter.platform == Platform::Linux {
        emitter.instruction("fmov d0, x3");                                     // Linux AArch64 passes the first FP variadic in d0
    }
    emitter.instruction("add x0, sp, #192");                                    // conversion scratch destination
    emitter.instruction(&format!("mov x1, #{}", CONV_SCRATCH_CAP));             // conversion scratch capacity
    emitter.instruction("add x2, sp, #160");                                    // the mini C format string
    emitter.bl_c("snprintf");                                                   // render the float body through libc
    emitter.instruction("b __rt_sprintf_snret");                                // clamp and take the result
}

/// Emits the AArch64 post-`snprintf` clamp.
///
/// libc returns the number of bytes it *would* have written, so the value is clamped to
/// the bytes actually present in the scratch buffer before it is ever used as a length.
/// That clamp is the direct fix for the out-of-bounds stack read this helper used to have.
fn emit_snprintf_result(emitter: &mut Emitter) {
    emitter.label("__rt_sprintf_snret");
    emitter.instruction("sxtw x4, w0");                                         // snprintf returns a signed 32-bit count
    emitter.instruction("tbz x4, #63, __rt_sprintf_snret_nn");                  // non-negative → usable as a length
    emitter.instruction("mov x4, #0");                                          // an encoding error produced no bytes
    emitter.label("__rt_sprintf_snret_nn");
    emitter.instruction(&format!("cmp x4, #{}", CONV_SCRATCH_CAP - 1));         // did libc want more than the scratch holds?
    emitter.instruction("b.ls __rt_sprintf_snret_ok");                          // no → every counted byte is really there
    emitter.instruction(&format!("mov x4, #{}", CONV_SCRATCH_CAP - 1));         // clamp to the bytes actually written
    emitter.label("__rt_sprintf_snret_ok");
    emitter.instruction("add x3, sp, #192");                                    // body pointer = conversion scratch
    emitter.instruction("ldr x9, [sp, #128]");                                  // reload the conversion character
    emitter.instruction("cmp w9, #101");                                        // 'e' needs PHP's exponent form
    emitter.instruction("b.eq __rt_sprintf_expfix");                            // compact the exponent
    emitter.instruction("cmp w9, #69");                                         // 'E' needs PHP's exponent form
    emitter.instruction("b.eq __rt_sprintf_expfix");                            // compact the exponent
    emitter.instruction("b __rt_sprintf_emit");                                 // pad and copy the rendered body
}

/// Emits the AArch64 exponent compaction for `%e`/`%E`.
///
/// C always pads the exponent to at least two digits (`1.234568e+04`) while PHP does not
/// (`1.234568e+4`), so the leading zeros of the exponent field are removed in place, always
/// leaving at least one digit.
fn emit_exponent_compaction(emitter: &mut Emitter) {
    emitter.label("__rt_sprintf_expfix");
    emitter.instruction("mov x9, x3");                                          // read cursor over the rendered body
    emitter.instruction("mov x10, x3");                                         // write cursor for the compacted body
    emitter.instruction("add x11, x3, x4");                                     // one past the last rendered byte
    emitter.label("__rt_sprintf_expfix_scan");
    emitter.instruction("cmp x9, x11");                                         // reached the end without an exponent?
    emitter.instruction("b.hs __rt_sprintf_expfix_done");                       // yes → nothing to compact
    emitter.instruction("ldrb w13, [x9]");                                      // load the current body byte
    emitter.instruction("cmp w13, #101");                                       // lowercase exponent marker?
    emitter.instruction("b.eq __rt_sprintf_expfix_hit");                        // yes → compact from here
    emitter.instruction("cmp w13, #69");                                        // uppercase exponent marker?
    emitter.instruction("b.eq __rt_sprintf_expfix_hit");                        // yes → compact from here
    emitter.instruction("strb w13, [x10]");                                     // keep the mantissa byte
    emitter.instruction("add x9, x9, #1");                                      // advance the read cursor
    emitter.instruction("add x10, x10, #1");                                    // advance the write cursor
    emitter.instruction("b __rt_sprintf_expfix_scan");                          // keep scanning for the exponent
    emitter.label("__rt_sprintf_expfix_hit");
    emitter.instruction("strb w13, [x10]");                                     // keep the exponent marker
    emitter.instruction("add x9, x9, #1");                                      // advance the read cursor
    emitter.instruction("add x10, x10, #1");                                    // advance the write cursor
    emitter.instruction("cmp x9, x11");                                         // is there anything after the marker?
    emitter.instruction("b.hs __rt_sprintf_expfix_done");                       // no → the body ends here
    emitter.instruction("ldrb w13, [x9]");                                      // load the exponent sign byte
    emitter.instruction("cmp w13, #43");                                        // '+' exponent sign?
    emitter.instruction("b.eq __rt_sprintf_expfix_sign");                       // yes → keep it
    emitter.instruction("cmp w13, #45");                                        // '-' exponent sign?
    emitter.instruction("b.ne __rt_sprintf_expfix_zeros");                      // no sign at all → go straight to the digits
    emitter.label("__rt_sprintf_expfix_sign");
    emitter.instruction("strb w13, [x10]");                                     // keep the exponent sign
    emitter.instruction("add x9, x9, #1");                                      // advance the read cursor
    emitter.instruction("add x10, x10, #1");                                    // advance the write cursor
    emitter.label("__rt_sprintf_expfix_zeros");
    emitter.instruction("sub x15, x11, #1");                                    // index of the final exponent digit
    emitter.label("__rt_sprintf_expfix_zloop");
    emitter.instruction("cmp x9, x15");                                         // never drop the last exponent digit
    emitter.instruction("b.hs __rt_sprintf_expfix_tail");                       // one digit left → stop stripping
    emitter.instruction("ldrb w13, [x9]");                                      // load the current exponent digit
    emitter.instruction("cmp w13, #48");                                        // is it a padding zero?
    emitter.instruction("b.ne __rt_sprintf_expfix_tail");                       // no → the exponent starts here
    emitter.instruction("add x9, x9, #1");                                      // skip the padding zero
    emitter.instruction("b __rt_sprintf_expfix_zloop");                         // check the next exponent digit
    emitter.label("__rt_sprintf_expfix_tail");
    emitter.instruction("cmp x9, x11");                                         // copied every remaining byte?
    emitter.instruction("b.hs __rt_sprintf_expfix_done");                       // yes → compaction finished
    emitter.instruction("ldrb w13, [x9]");                                      // load the next exponent byte
    emitter.instruction("strb w13, [x10]");                                     // keep the exponent byte
    emitter.instruction("add x9, x9, #1");                                      // advance the read cursor
    emitter.instruction("add x10, x10, #1");                                    // advance the write cursor
    emitter.instruction("b __rt_sprintf_expfix_tail");                          // copy the rest of the exponent
    emitter.label("__rt_sprintf_expfix_done");
    emitter.instruction("sub x4, x10, x3");                                     // compacted body length
}

/// Emits the AArch64 pad-and-copy stage shared by every conversion.
///
/// `x3`/`x4` carry the conversion body. The field width is validated against PHP's
/// `0..INT_MAX` range and the whole padded result is bounds-checked against the end of
/// `_concat_buf` *before* a single byte is written, so neither an absurd width nor a
/// long body can walk off the arena. Zero padding is inserted after a leading sign,
/// matching PHP's `sprintf("%05d", -42)` → `-0042`.
fn emit_pad_and_copy(emitter: &mut Emitter) {
    emitter.label("__rt_sprintf_emit");
    emitter.instruction("ldr x5, [sp, #96]");                                   // parsed field width
    emitter.instruction("lsr x9, x5, #31");                                     // any bit above INT_MAX set?
    emitter.instruction("cbnz x9, __rt_sprintf_wfatal");                        // yes → PHP rejects the width
    emitter.instruction("mov x11, #0");                                         // padding byte count
    emitter.instruction("cmp x5, x4");                                          // is the body already at least as wide?
    emitter.instruction("b.ls __rt_sprintf_emit_nopad");                        // yes → no padding needed
    emitter.instruction("sub x11, x5, x4");                                     // padding = width - body length
    emitter.label("__rt_sprintf_emit_nopad");
    emitter.instruction("add x13, x4, x11");                                    // total bytes this conversion emits
    emitter.instruction("add x13, x13, x23");                                   // address just past the emitted bytes
    emitter.instruction("ldr x15, [sp, #144]");                                 // concat-buffer write limit
    emitter.instruction("cmp x13, x15");                                        // would the conversion leave the arena?
    emitter.instruction("b.hi __rt_sprintf_ofatal");                            // yes → controlled fatal instead of an overrun
    emitter.instruction("mov x15, x13");                                        // preserve the end of the complete padded output range
    emitter.instruction("ldr x10, [sp, #112]");                                 // parsed flags
    emitter.instruction("mov x13, x23");                                        // default final body destination for a left-aligned field
    emitter.instruction("tbnz x10, #0, __rt_sprintf_overlap_dest");             // left alignment keeps the body at the write cursor
    emitter.instruction("add x13, x13, x11");                                   // right alignment places the body after its leading padding
    emitter.label("__rt_sprintf_overlap_dest");
    emitter.instruction("cmp x3, x13");                                         // is the body already at its final destination?
    emitter.instruction("b.eq __rt_sprintf_overlap_done");                      // yes → no relocation is needed
    emitter.instruction("add x14, x3, x4");                                     // one-past-the-end source address
    emitter.instruction("cmp x14, x23");                                        // does the source finish before the complete output starts?
    emitter.instruction("b.ls __rt_sprintf_overlap_done");                      // yes → ordinary forward copy cannot clobber it
    emitter.instruction("cmp x15, x3");                                         // does the complete padded output finish before the source starts?
    emitter.instruction("b.ls __rt_sprintf_overlap_done");                      // yes → the ranges do not overlap
    emitter.instruction("cmp x13, x3");                                         // which direction makes this memmove safe?
    emitter.instruction("b.lo __rt_sprintf_overlap_forward");                   // lower destination copies from the beginning
    emitter.instruction("mov x5, x4");                                          // backward-copy byte count
    emitter.label("__rt_sprintf_overlap_backward");
    emitter.instruction("cbz x5, __rt_sprintf_overlap_moved");                  // every overlapping byte has reached its final slot
    emitter.instruction("sub x5, x5, #1");                                      // walk both ranges from their final byte
    emitter.instruction("ldrb w12, [x3, x5]");                                  // load before a higher destination can overwrite the source
    emitter.instruction("strb w12, [x13, x5]");                                 // place the byte at its final body position
    emitter.instruction("b __rt_sprintf_overlap_backward");                     // continue toward the start of the body
    emitter.label("__rt_sprintf_overlap_forward");
    emitter.instruction("mov x5, #0");                                          // forward-copy byte index
    emitter.label("__rt_sprintf_overlap_forward_loop");
    emitter.instruction("cmp x5, x4");                                          // copied the whole overlapping body?
    emitter.instruction("b.hs __rt_sprintf_overlap_moved");                     // yes → publish the relocated source
    emitter.instruction("ldrb w12, [x3, x5]");                                  // read the next byte before the lower destination touches it
    emitter.instruction("strb w12, [x13, x5]");                                 // place the byte at its final body position
    emitter.instruction("add x5, x5, #1");                                      // advance through the body
    emitter.instruction("b __rt_sprintf_overlap_forward_loop");                 // keep copying toward the end
    emitter.label("__rt_sprintf_overlap_moved");
    emitter.instruction("mov x3, x13");                                         // subsequent padding/copy reads from the safe final location
    emitter.label("__rt_sprintf_overlap_done");
    emitter.instruction("ldr x9, [sp, #120]");                                  // pad character
    emitter.instruction("tbnz x10, #0, __rt_sprintf_emit_left");                // left-aligned → body first, padding after
    emitter.instruction("cbz x11, __rt_sprintf_emit_pad");                      // no padding → copy the body directly
    emitter.instruction("cmp w9, #48");                                         // only '0' padding moves ahead of the sign
    emitter.instruction("b.ne __rt_sprintf_emit_pad");                          // other pad characters stay before the sign
    emitter.instruction("cbz x4, __rt_sprintf_emit_pad");                       // an empty body has no sign to hoist
    emitter.instruction("ldrb w13, [x3]");                                      // first body byte
    emitter.instruction("cmp w13, #45");                                        // is it a minus sign?
    emitter.instruction("b.eq __rt_sprintf_emit_sign");                         // yes → emit it before the zeros
    emitter.instruction("cmp w13, #43");                                        // is it a plus sign?
    emitter.instruction("b.ne __rt_sprintf_emit_pad");                          // no sign → pad normally
    emitter.label("__rt_sprintf_emit_sign");
    emitter.instruction("strb w13, [x23], #1");                                 // emit the sign ahead of the zero padding
    emitter.instruction("add x3, x3, #1");                                      // the sign is no longer part of the body
    emitter.instruction("sub x4, x4, #1");                                      // shorten the body accordingly
    emitter.label("__rt_sprintf_emit_pad");
    emitter.instruction("cbz x11, __rt_sprintf_emit_copy");                     // padding written → copy the body
    emitter.instruction("strb w9, [x23], #1");                                  // emit one padding byte
    emitter.instruction("sub x11, x11, #1");                                    // one padding byte fewer to write
    emitter.instruction("b __rt_sprintf_emit_pad");                             // keep padding
    emitter.label("__rt_sprintf_emit_copy");
    emitter.instruction("cbz x4, __rt_sprintf_emit_done");                      // body copied → release any temporary owner
    emitter.instruction("ldrb w13, [x3], #1");                                  // load the next body byte
    emitter.instruction("strb w13, [x23], #1");                                 // emit the body byte
    emitter.instruction("sub x4, x4, #1");                                      // one body byte fewer to copy
    emitter.instruction("b __rt_sprintf_emit_copy");                            // keep copying
    emitter.label("__rt_sprintf_emit_left");
    emitter.instruction("cbz x4, __rt_sprintf_emit_lpad");                      // body copied → append the padding
    emitter.instruction("ldrb w13, [x3], #1");                                  // load the next body byte
    emitter.instruction("strb w13, [x23], #1");                                 // emit the body byte
    emitter.instruction("sub x4, x4, #1");                                      // one body byte fewer to copy
    emitter.instruction("b __rt_sprintf_emit_left");                            // keep copying
    emitter.label("__rt_sprintf_emit_lpad");
    emitter.instruction("cbz x11, __rt_sprintf_emit_done");                     // padding written → release any temporary owner
    emitter.instruction("strb w9, [x23], #1");                                  // emit one trailing padding byte
    emitter.instruction("sub x11, x11, #1");                                    // one padding byte fewer to write
    emitter.instruction("b __rt_sprintf_emit_lpad");                            // keep padding
    emitter.label("__rt_sprintf_emit_done");
    emitter.instruction("ldr x0, [sp, #152]");                                  // formatter-owned string produced by __toString, if any
    emitter.instruction("cbz x0, __rt_sprintf_loop");                           // borrowed/numeric bodies need no cleanup
    emitter.instruction("str xzr, [sp, #152]");                                 // prevent stale ownership from crossing conversions
    emitter.instruction("bl __rt_heap_free_safe");                              // release only after every output byte was copied
    emitter.instruction("b __rt_sprintf_loop");                                 // scan the next format byte
}

/// Emits the AArch64 controlled-fatal exits for invalid widths/specifiers/argument counts,
/// concat overflow. Each writes a PHP-shaped diagnostic
/// to stderr and exits with PHP's fatal-error status (255).
fn emit_fatal_paths(emitter: &mut Emitter) {
    emit_fatal(emitter, "__rt_sprintf_wfatal", "_sprintf_width_msg", SPRINTF_WIDTH_MSG.len());
    emit_fatal(emitter, "__rt_sprintf_ofatal", "_sprintf_overflow_msg", SPRINTF_OVERFLOW_MSG.len());
    emit_fatal(emitter, "__rt_sprintf_afatal", "_sprintf_argcount_msg", SPRINTF_ARGCOUNT_MSG.len());
    emit_fatal(emitter, "__rt_sprintf_sfatal", "_sprintf_unknown_spec_msg", SPRINTF_UNKNOWN_SPEC_MSG.len());
}

/// Emits one AArch64 fatal exit block: write `len` bytes of `symbol` to stderr, then exit
/// with status 255 (the status PHP uses for an uncaught fatal error).
fn emit_fatal(emitter: &mut Emitter, label: &str, symbol: &str, len: usize) {
    emitter.label(label);
    emitter.instruction("mov x0, #2");                                          // write the diagnostic to stderr
    abi::emit_symbol_address(emitter, "x1", symbol);
    emitter.instruction(&format!("mov x2, #{}", len));                          // exact diagnostic byte length
    emitter.syscall(4);
    emitter.instruction("mov x0, #255");                                        // PHP exits with 255 on a fatal error
    emitter.syscall(1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Emits `__rt_sprintf` for one target and returns the assembly text.
    fn sprintf_asm(target: Target) -> String {
        let mut emitter = Emitter::new(target);
        emit_sprintf(&mut emitter);
        emitter.output()
    }

    /// Both lowerings must clamp the `snprintf` return value to the bytes that are really
    /// present in the conversion scratch. Copying `snprintf`'s "would have written" count
    /// out of a fixed buffer is what leaked stack memory into `sprintf()` results.
    #[test]
    fn snprintf_return_is_clamped_to_the_scratch_buffer() {
        let arm = sprintf_asm(Target::new(Platform::MacOS, Arch::AArch64));
        assert!(arm.contains("sxtw x4, w0"), "{arm}");
        assert!(arm.contains(&format!("cmp x4, #{}", CONV_SCRATCH_CAP - 1)), "{arm}");
        assert!(arm.contains(&format!("mov x4, #{}", CONV_SCRATCH_CAP - 1)), "{arm}");

        let x64 = sprintf_asm(Target::new(Platform::Linux, Arch::X86_64));
        assert!(x64.contains("movsxd r11, eax"), "{x64}");
        assert!(x64.contains(&format!("cmp r11, {}", CONV_SCRATCH_CAP - 1)), "{x64}");
        assert!(x64.contains(&format!("mov r11d, {}", CONV_SCRATCH_CAP - 1)), "{x64}");
    }

    /// Both lowerings must bound every conversion against the end of `_concat_buf` and
    /// reject widths outside PHP's `0..INT_MAX` range instead of writing past the arena.
    #[test]
    fn writes_are_bounded_and_absurd_widths_are_rejected() {
        let arm = sprintf_asm(Target::new(Platform::MacOS, Arch::AArch64));
        assert!(arm.contains("b.hi __rt_sprintf_ofatal"), "{arm}");
        assert!(arm.contains("b.hs __rt_sprintf_ofatal"), "{arm}");
        assert!(arm.contains("lsr x9, x5, #31"), "{arm}");
        assert!(arm.contains("cbnz x9, __rt_sprintf_wfatal"), "{arm}");
        assert!(arm.contains("b.hs __rt_sprintf_afatal"), "{arm}");

        let x64 = sprintf_asm(Target::new(Platform::Linux, Arch::X86_64));
        assert!(x64.contains("ja __rt_sprintf_ofatal_x64"), "{x64}");
        assert!(x64.contains("jae __rt_sprintf_ofatal_x64"), "{x64}");
        assert!(x64.contains("shr rcx, 31"), "{x64}");
        assert!(x64.contains("jnz __rt_sprintf_wfatal_x64"), "{x64}");
        assert!(x64.contains("jae __rt_sprintf_afatal_x64"), "{x64}");
    }

    /// A dynamic object/resource string may already live in `_concat_buf`. Both lowerings
    /// must move an overlapping body before padding and publish the absolute final cursor,
    /// otherwise leading padding corrupts the source and nested concat use is counted twice.
    #[test]
    fn concat_backed_string_bodies_are_relocated_and_counted_once() {
        let arm = sprintf_asm(Target::new(Platform::MacOS, Arch::AArch64));
        assert!(arm.contains("__rt_sprintf_overlap_backward:"), "{arm}");
        assert!(arm.contains("__rt_sprintf_overlap_forward:"), "{arm}");
        assert!(arm.contains("sub x8, x23, x8"), "{arm}");

        let x64 = sprintf_asm(Target::new(Platform::Linux, Arch::X86_64));
        assert!(x64.contains("__rt_sprintf_overlap_backward_x64:"), "{x64}");
        assert!(x64.contains("__rt_sprintf_overlap_forward_x64:"), "{x64}");
        assert!(x64.contains("sub rbx, r11"), "{x64}");
    }

    /// The C format string handed to libc is assembled from parsed state, so an unknown
    /// conversion character must reach the `ValueError` exit rather than `snprintf`. This is
    /// what keeps `%n` — an arbitrary-write primitive — unreachable from PHP source.
    #[test]
    fn unknown_conversions_never_reach_libc() {
        let arm = sprintf_asm(Target::new(Platform::MacOS, Arch::AArch64));
        assert!(arm.contains("b __rt_sprintf_sfatal"), "{arm}");
        assert!(arm.contains("_sprintf_unknown_spec_msg"), "{arm}");

        let x64 = sprintf_asm(Target::new(Platform::Linux, Arch::X86_64));
        assert!(x64.contains("jmp __rt_sprintf_sfatal_x64"), "{x64}");
        assert!(x64.contains("_sprintf_unknown_spec_msg"), "{x64}");
    }
}
