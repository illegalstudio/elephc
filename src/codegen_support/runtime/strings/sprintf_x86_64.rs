//! Purpose:
//! Emits the Linux x86_64 lowering of `__rt_sprintf`, the shared PHP `printf`-family
//! formatter. It is the exact behavioural mirror of the AArch64 lowering in
//! `sprintf.rs`; the two must be changed together.
//!
//! Called from:
//! - `crate::codegen_support::runtime::strings::sprintf::emit_sprintf()`, which dispatches
//!   here for `Arch::X86_64`.
//!
//! Key details:
//! - Specifiers are parsed into frame slots (argument number, flags, pad character, width,
//!   precision, conversion character). No program-supplied byte is ever copied into the C
//!   format string handed to libc, so an over-long specifier cannot overrun the mini format
//!   buffer and an unknown conversion (notably `%n`) never reaches `snprintf`.
//! - Padding is applied by this helper; libc only renders the unpadded numeric body into a
//!   512-byte scratch, bounded because precision is clamped to PHP's 53-digit maximum.
//!   `%s`, `%b`, and `%c` bypass libc entirely.
//! - Every write into `_concat_buf` is bounds-checked against the end of the 64 KiB arena.
//! - Each conversion coerces its operand from the record's type tag (double↔int,
//!   string→number, and deferred boxed non-scalars through sprintf-specific Mixed helpers),
//!   so a record whose tag disagrees with the conversion character is converted, never
//!   printed as a raw pointer.
//! - All parse state lives in the frame because every SysV temporary is clobbered by the
//!   `snprintf` call; only `rbx`, `r12`-`r15` survive it.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::runtime::data::{
    SPRINTF_ARGCOUNT_MSG, SPRINTF_OVERFLOW_MSG, SPRINTF_UNKNOWN_SPEC_MSG, SPRINTF_WIDTH_MSG,
};

use super::sprintf::{CONCAT_BUF_CAP, CONV_SCRATCH_CAP};

/// Emits the `__rt_sprintf` runtime helper for Linux x86_64.
///
/// # Register contract on entry
/// - `rdi`: number of packed variadic argument records pushed by the caller
/// - `rsi`: optional persistent eval context for eval-declared `__toString()` dispatch
/// - `rax`: format string pointer
/// - `rdx`: format string byte length
/// - caller stack above the return address: `rdi` records of 16 bytes, `[payload, tag]`
///
/// # Register contract on exit
/// - `rax`: result pointer inside `_concat_buf`
/// - `rdx`: result byte length
///
/// The record tag word is `0` for int, `1 | (len << 8)` for string, `2` for float, `3` for
/// bool, `7` for a deferred boxed `Mixed`, and `4`/`5`/`6`/`9`/`10`/`11` for raw indexed-array,
/// associative-array, object, resource, callable, or erased-iterable payloads. The helper
/// consults it so a conversion never dereferences a payload that is not a string pointer.
/// `_concat_off` is advanced by the result length and the caller's tagged records are discarded
/// before `ret`.
///
/// Callee-saved registers used: `rbx` = write cursor in `_concat_buf`, `r12` = format
/// cursor, `r13` = remaining format bytes, `r14` = next sequential argument index,
/// `r15` = argument record base.
pub(super) fn emit_sprintf_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: sprintf ---");
    emitter.label_global("__rt_sprintf");

    // Frame layout, relative to rbp:
    //   [rbp-8 .. rbp-40]    = pushed rbx, r12, r13, r14, r15
    //   [rbp-48]             = result start pointer inside _concat_buf
    //   [rbp-56]             = address of the _concat_off symbol
    //   [rbp-64]             = packed argument record count
    //   [rbp-72]             = parsed field width
    //   [rbp-80]             = parsed precision (-1 when the specifier had no '.')
    //   [rbp-88]             = parsed flags: bit0 left-align, bit1 force sign, bit2 alt form
    //   [rbp-96]             = parsed pad character
    //   [rbp-104]            = parsed conversion character
    //   [rbp-112]            = parsed argument number (0 = next sequential argument)
    //   [rbp-120]            = one-past-the-end address of _concat_buf
    //   [rbp-680]            = optional eval context
    //   [rbp-688]            = formatter-owned temporary string
    //   [rbp-160 .. rbp-129] = mini C format string built by this helper
    //   [rbp-672 .. rbp-161] = snprintf conversion scratch (CONV_SCRATCH_CAP bytes)

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for every local slot
    emitter.instruction("push rbx");                                            // preserve the concat-buffer write cursor register
    emitter.instruction("push r12");                                            // preserve the format-cursor register
    emitter.instruction("push r13");                                            // preserve the remaining-format-length register
    emitter.instruction("push r14");                                            // preserve the sequential-argument-index register
    emitter.instruction("push r15");                                            // preserve the argument-record base register
    emitter.instruction("sub rsp, 648");                                        // reserve parse slots, conversion scratch, and formatter state
    emitter.instruction("mov r12, rax");                                        // format cursor
    emitter.instruction("mov r13, rdx");                                        // remaining format bytes
    emitter.instruction("xor r14d, r14d");                                      // next sequential argument index
    emitter.instruction("lea r15, [rbp + 16]");                                 // argument records begin above the saved return address
    emitter.instruction("mov QWORD PTR [rbp - 64], rdi");                       // remember how many records the caller pushed
    emitter.instruction("mov QWORD PTR [rbp - 680], rsi");                      // preserve optional eval context for Stringable dispatch
    emitter.instruction("mov QWORD PTR [rbp - 688], 0");                        // no formatter-owned temporary string is live
    abi::emit_symbol_address(emitter, "r10", "_concat_off");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // current concat-buffer write offset
    abi::emit_symbol_address(emitter, "rcx", "_concat_buf");
    emitter.instruction("lea rbx, [rcx + r11]");                                // write cursor = buffer base + offset
    emitter.instruction("mov QWORD PTR [rbp - 48], rbx");                       // remember where this result starts
    emitter.instruction("mov QWORD PTR [rbp - 56], r10");                       // remember the concat-offset symbol address
    emitter.instruction(&format!("lea rcx, [rcx + {}]", CONCAT_BUF_CAP));       // one-past-the-end address of the concat buffer
    emitter.instruction("mov QWORD PTR [rbp - 120], rcx");                      // publish the hard write limit for every copy below

    // ================================================================
    // MAIN SCAN LOOP: literal bytes are copied, '%' starts a specifier
    // ================================================================
    emitter.label("__rt_sprintf_loop_x64");
    emitter.instruction("test r13, r13");                                       // any format bytes left?
    emitter.instruction("jz __rt_sprintf_done_x64");                            // no → publish the result
    emitter.instruction("movzx r8d, BYTE PTR [r12]");                           // load the next format byte
    emitter.instruction("add r12, 1");                                          // advance the format cursor
    emitter.instruction("sub r13, 1");                                          // account for the consumed format byte
    emitter.instruction("cmp r8b, 37");                                         // is it '%'?
    emitter.instruction("je __rt_sprintf_fmt_x64");                             // yes → parse a conversion specifier
    emitter.instruction("mov r9, QWORD PTR [rbp - 120]");                       // reload the concat-buffer write limit
    emitter.instruction("cmp rbx, r9");                                         // would this literal byte land outside the arena?
    emitter.instruction("jae __rt_sprintf_ofatal_x64");                         // yes → controlled fatal instead of an overrun
    emitter.instruction("mov BYTE PTR [rbx], r8b");                             // copy the literal byte to the result
    emitter.instruction("add rbx, 1");                                          // advance the write cursor
    emitter.instruction("jmp __rt_sprintf_loop_x64");                           // continue scanning

    emitter.label("__rt_sprintf_fmt_x64");
    emitter.instruction("test r13, r13");                                       // trailing '%' with nothing after it?
    emitter.instruction("jz __rt_sprintf_done_x64");                            // yes → publish the result
    emitter.instruction("movzx r8d, BYTE PTR [r12]");                           // peek at the byte after '%'
    emitter.instruction("cmp r8b, 37");                                         // is the sequence '%%'?
    emitter.instruction("jne __rt_sprintf_spec_x64");                           // no → parse a real specifier
    emitter.instruction("add r12, 1");                                          // consume the second '%'
    emitter.instruction("sub r13, 1");                                          // account for the consumed byte
    emitter.instruction("mov r9, QWORD PTR [rbp - 120]");                       // reload the concat-buffer write limit
    emitter.instruction("cmp rbx, r9");                                         // would the literal '%' land outside the arena?
    emitter.instruction("jae __rt_sprintf_ofatal_x64");                         // yes → controlled fatal instead of an overrun
    emitter.instruction("mov BYTE PTR [rbx], r8b");                             // emit the literal '%'
    emitter.instruction("add rbx, 1");                                          // advance the write cursor
    emitter.instruction("jmp __rt_sprintf_loop_x64");                           // continue scanning

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
    // DONE: publish the result and discard the caller's argument records
    // ================================================================
    emitter.label("__rt_sprintf_done_x64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // result pointer inside the concat buffer
    emitter.instruction("mov rdx, rbx");                                        // current write cursor
    emitter.instruction("sub rdx, rax");                                        // result byte length
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // concat-offset symbol address
    abi::emit_symbol_address(emitter, "r11", "_concat_buf");
    emitter.instruction("sub rbx, r11");                                        // derive the absolute cursor after nested concat-producing conversions
    emitter.instruction("mov QWORD PTR [r10], rbx");                            // publish the exact new write offset without double-counting
    emitter.instruction("mov rcx, QWORD PTR [rbp - 64]");                       // packed argument record count
    emitter.instruction("shl rcx, 4");                                          // records are 16 bytes each
    emitter.instruction("add rsp, 648");                                        // release local buffers and formatter state
    emitter.instruction("pop r15");                                             // restore the argument-record base register
    emitter.instruction("pop r14");                                             // restore the sequential-argument-index register
    emitter.instruction("pop r13");                                             // restore the remaining-format-length register
    emitter.instruction("pop r12");                                             // restore the format-cursor register
    emitter.instruction("pop rbx");                                             // restore the concat-buffer write cursor register
    emitter.instruction("mov r11, QWORD PTR [rsp + 8]");                        // save the return address before rethreading the stack
    emitter.instruction("mov rbp, QWORD PTR [rsp]");                            // restore the caller frame pointer
    emitter.instruction("lea rsp, [rsp + rcx + 16]");                           // skip the saved rbp, return address and tagged records
    emitter.instruction("push r11");                                            // recreate the return address on the rethreaded stack
    emitter.instruction("ret");                                                 // return the formatted string in rax/rdx

    emit_fatal_paths(emitter);
}

/// Emits an x86_64 decimal-number scanner used for the argument number, the field width,
/// and the precision.
///
/// `ptr`/`len` are the source cursor and remaining-byte count; both are advanced past the
/// digits consumed. `r10` receives the value and `r11` the digit count; both must be zeroed
/// by the caller. `r8b` is left holding the first non-digit byte, or zero when the input ran
/// out, so the caller can tell "stopped on `$`" from "ran out of format". `r9` is clobbered.
///
/// Accumulation stops after 10 digits and any longer run saturates to `0x80000000`, which
/// keeps the accumulator inside 64 bits and makes "wider than `INT_MAX`" detectable as
/// `value >> 31 != 0` no matter how many digits the program supplied.
fn emit_scan_decimal(emitter: &mut Emitter, prefix: &str, ptr: &str, len: &str) {
    emitter.label(&format!("{}_loop", prefix));
    emitter.instruction(&format!("test {0}, {0}", len));                        // any bytes left to inspect?
    emitter.instruction(&format!("jz {}_end0", prefix));                        // no → the number ends here
    emitter.instruction(&format!("movzx r8d, BYTE PTR [{}]", ptr));             // peek at the current byte
    emitter.instruction("mov r9d, r8d");                                        // copy it before converting to a digit value
    emitter.instruction("sub r9d, 48");                                         // convert the byte to a digit value
    emitter.instruction("cmp r9d, 9");                                          // is it outside '0'..'9'?
    emitter.instruction(&format!("ja {}_done", prefix));                        // yes → the number ends here
    emitter.instruction("cmp r11, 10");                                         // already accumulated ten digits?
    emitter.instruction(&format!("jae {}_skip", prefix));                       // yes → stop accumulating, just count
    emitter.instruction("lea r10, [r10 + r10*4]");                              // accumulator *= 5
    emitter.instruction("add r10, r10");                                        // accumulator *= 2, so *= 10 overall
    emitter.instruction("add r10, r9");                                         // add the current digit
    emitter.label(&format!("{}_skip", prefix));
    emitter.instruction("add r11, 1");                                          // count the consumed digit
    emitter.instruction(&format!("add {}, 1", ptr));                            // advance the source cursor
    emitter.instruction(&format!("sub {}, 1", len));                            // account for the consumed byte
    emitter.instruction(&format!("jmp {}_loop", prefix));                       // scan the next digit
    emitter.label(&format!("{}_end0", prefix));
    emitter.instruction("xor r8d, r8d");                                        // no lookahead byte is available
    emitter.label(&format!("{}_done", prefix));
    emitter.instruction("cmp r11, 10");                                         // did the run exceed ten digits?
    emitter.instruction(&format!("jbe {}_nosat", prefix));                      // no → keep the accumulated value
    emitter.instruction("mov r10d, 2147483648");                                // saturate above INT_MAX so the range check fires
    emitter.label(&format!("{}_nosat", prefix));
}

/// Emits the x86_64 specifier parser: argument number, flags, pad character, width and
/// precision are decoded into frame slots and the conversion character is stored last.
///
/// Nothing here copies program-supplied bytes into a buffer, so an arbitrarily long
/// specifier costs scan time only — it can never overrun the mini format buffer.
fn emit_spec_parser(emitter: &mut Emitter) {
    // -- reset the per-specifier state --
    emitter.label("__rt_sprintf_spec_x64");
    emitter.instruction("mov QWORD PTR [rbp - 72], 0");                         // width = 0
    emitter.instruction("mov QWORD PTR [rbp - 80], -1");                        // precision = absent
    emitter.instruction("mov QWORD PTR [rbp - 88], 0");                         // flags = none
    emitter.instruction("mov QWORD PTR [rbp - 96], 32");                        // pad character = ' '
    emitter.instruction("mov QWORD PTR [rbp - 112], 0");                        // argument number = sequential

    // -- optional "N$" argument number: only committed when a '$' follows the digits --
    emitter.instruction("mov rsi, r12");                                        // lookahead cursor (does not consume yet)
    emitter.instruction("mov rdi, r13");                                        // lookahead remaining-byte count
    emitter.instruction("xor r10d, r10d");                                      // argument-number accumulator
    emitter.instruction("xor r11d, r11d");                                      // argument-number digit count
    emit_scan_decimal(emitter, "__rt_sprintf_an_x64", "rsi", "rdi");
    emitter.instruction("test r11, r11");                                       // were there any digits?
    emitter.instruction("jz __rt_sprintf_flags_x64");                           // no → not an argument number
    emitter.instruction("cmp r8b, 36");                                         // is the byte after the digits '$'?
    emitter.instruction("jne __rt_sprintf_flags_x64");                          // no → those digits are the field width
    emitter.instruction("mov QWORD PTR [rbp - 112], r10");                      // commit the explicit argument number
    emitter.instruction("lea r12, [rsi + 1]");                                  // consume the digits and the '$'
    emitter.instruction("lea r13, [rdi - 1]");                                  // account for the consumed '$'

    // -- flags: '-', '+', '0', ' ', '#', and PHP's "'X" custom pad character --
    emitter.label("__rt_sprintf_flags_x64");
    emitter.instruction("test r13, r13");                                       // format ended inside the specifier?
    emitter.instruction("jz __rt_sprintf_endspec_x64");                         // yes → stop formatting
    emitter.instruction("movzx r8d, BYTE PTR [r12]");                           // peek at the current specifier byte
    emitter.instruction("cmp r8b, 45");                                         // '-' left-align flag?
    emitter.instruction("je __rt_sprintf_fl_left_x64");                         // yes → record left alignment
    emitter.instruction("cmp r8b, 43");                                         // '+' force-sign flag?
    emitter.instruction("je __rt_sprintf_fl_plus_x64");                         // yes → record the forced sign
    emitter.instruction("cmp r8b, 48");                                         // '0' zero-pad flag?
    emitter.instruction("je __rt_sprintf_fl_zero_x64");                         // yes → pad character becomes '0'
    emitter.instruction("cmp r8b, 32");                                         // ' ' space-pad flag?
    emitter.instruction("je __rt_sprintf_fl_space_x64");                        // yes → pad character becomes ' '
    emitter.instruction("cmp r8b, 35");                                         // '#' alternate-form flag?
    emitter.instruction("je __rt_sprintf_fl_alt_x64");                          // yes → record the alternate form
    emitter.instruction("cmp r8b, 39");                                         // "'" custom-pad-character flag?
    emitter.instruction("je __rt_sprintf_fl_pad_x64");                          // yes → the next byte is the pad character
    emitter.instruction("jmp __rt_sprintf_width_x64");                          // no more flags → parse the width

    emitter.label("__rt_sprintf_fl_left_x64");
    emitter.instruction("or QWORD PTR [rbp - 88], 1");                          // set the left-align flag bit
    emitter.instruction("jmp __rt_sprintf_fl_next_x64");                        // consume the flag byte

    emitter.label("__rt_sprintf_fl_plus_x64");
    emitter.instruction("or QWORD PTR [rbp - 88], 2");                          // set the force-sign flag bit
    emitter.instruction("jmp __rt_sprintf_fl_next_x64");                        // consume the flag byte

    emitter.label("__rt_sprintf_fl_alt_x64");
    emitter.instruction("or QWORD PTR [rbp - 88], 4");                          // set the alternate-form flag bit
    emitter.instruction("jmp __rt_sprintf_fl_next_x64");                        // consume the flag byte

    emitter.label("__rt_sprintf_fl_zero_x64");
    emitter.instruction("mov QWORD PTR [rbp - 96], 48");                        // '0' becomes the pad character
    emitter.instruction("jmp __rt_sprintf_fl_next_x64");                        // consume the flag byte

    emitter.label("__rt_sprintf_fl_space_x64");
    emitter.instruction("mov QWORD PTR [rbp - 96], 32");                        // ' ' becomes the pad character
    emitter.instruction("jmp __rt_sprintf_fl_next_x64");                        // consume the flag byte

    emitter.label("__rt_sprintf_fl_pad_x64");
    emitter.instruction("add r12, 1");                                          // consume the "'" introducer
    emitter.instruction("sub r13, 1");                                          // account for the consumed byte
    emitter.instruction("test r13, r13");                                       // "'" at end of format?
    emitter.instruction("jz __rt_sprintf_endspec_x64");                         // yes → nothing to pad with
    emitter.instruction("movzx r9d, BYTE PTR [r12]");                           // the next byte is the custom pad character
    emitter.instruction("mov QWORD PTR [rbp - 96], r9");                        // store the custom pad character

    emitter.label("__rt_sprintf_fl_next_x64");
    emitter.instruction("add r12, 1");                                          // consume the flag byte
    emitter.instruction("sub r13, 1");                                          // account for the consumed byte
    emitter.instruction("jmp __rt_sprintf_flags_x64");                          // look for another flag

    // -- field width --
    emitter.label("__rt_sprintf_width_x64");
    emitter.instruction("xor r10d, r10d");                                      // width accumulator
    emitter.instruction("xor r11d, r11d");                                      // width digit count
    emit_scan_decimal(emitter, "__rt_sprintf_w_x64", "r12", "r13");
    emitter.instruction("mov QWORD PTR [rbp - 72], r10");                       // store the parsed field width

    // -- optional ".precision" --
    emitter.instruction("test r13, r13");                                       // format ended before the conversion?
    emitter.instruction("jz __rt_sprintf_endspec_x64");                         // yes → stop formatting
    emitter.instruction("movzx r8d, BYTE PTR [r12]");                           // peek at the current specifier byte
    emitter.instruction("cmp r8b, 46");                                         // '.' precision introducer?
    emitter.instruction("jne __rt_sprintf_slength_x64");                        // no → try PHP's optional `l` length modifier
    emitter.instruction("add r12, 1");                                          // consume the '.'
    emitter.instruction("sub r13, 1");                                          // account for the consumed byte
    emitter.instruction("xor r10d, r10d");                                      // precision accumulator ('.' alone means 0)
    emitter.instruction("xor r11d, r11d");                                      // precision digit count
    emit_scan_decimal(emitter, "__rt_sprintf_p_x64", "r12", "r13");
    emitter.instruction("mov QWORD PTR [rbp - 80], r10");                       // store the parsed precision

    // -- optional single `l` modifier --
    emitter.label("__rt_sprintf_slength_x64");
    emitter.instruction("test r13, r13");                                       // format ended before the conversion?
    emitter.instruction("jz __rt_sprintf_endspec_x64");                         // yes → stop formatting
    emitter.instruction("movzx r8d, BYTE PTR [r12]");                           // peek at the possible length modifier
    emitter.instruction("cmp r8b, 108");                                        // ASCII `l`
    emitter.instruction("jne __rt_sprintf_stype_x64");                          // current byte is already the conversion
    emitter.instruction("add r12, 1");                                          // consume the modifier
    emitter.instruction("sub r13, 1");                                          // account for the consumed byte

    // -- conversion character --
    emitter.label("__rt_sprintf_stype_x64");
    emitter.instruction("test r13, r13");                                       // format ended before the conversion?
    emitter.instruction("jz __rt_sprintf_endspec_x64");                         // yes → stop formatting
    emitter.instruction("movzx r8d, BYTE PTR [r12]");                           // load the conversion character
    emitter.instruction("add r12, 1");                                          // consume the conversion character
    emitter.instruction("sub r13, 1");                                          // account for the consumed byte
    emitter.instruction("mov QWORD PTR [rbp - 104], r8");                       // store the conversion character
    emitter.instruction("jmp __rt_sprintf_arg_x64");                            // fetch the argument this conversion consumes

    emitter.label("__rt_sprintf_endspec_x64");
    emitter.instruction("jmp __rt_sprintf_done_x64");                           // truncated specifier → stop formatting
}

/// Emits the x86_64 argument fetch: resolves the sequential or explicit `N$` argument index,
/// rejects out-of-range indices, and loads the 16-byte record into `r10`/`r11`.
///
/// The range check is what keeps the helper from reading the caller's stack past the pushed
/// records when a format string requests more arguments than were supplied.
fn emit_argument_fetch(emitter: &mut Emitter) {
    emitter.label("__rt_sprintf_arg_x64");
    emitter.instruction("mov r9, QWORD PTR [rbp - 112]");                       // parsed argument number (0 = sequential)
    emitter.instruction("test r9, r9");                                         // was an explicit number given?
    emitter.instruction("jz __rt_sprintf_arg_seq_x64");                         // no → take the next argument
    emitter.instruction("sub r9, 1");                                           // PHP argument numbers are 1-based
    emitter.instruction("jmp __rt_sprintf_arg_have_x64");                       // index resolved
    emitter.label("__rt_sprintf_arg_seq_x64");
    emitter.instruction("mov r9, r14");                                         // consume the next sequential argument
    emitter.instruction("add r14, 1");                                          // advance the sequential cursor
    emitter.label("__rt_sprintf_arg_have_x64");
    emitter.instruction("cmp r9, QWORD PTR [rbp - 64]");                        // is the index within the supplied records?
    emitter.instruction("jae __rt_sprintf_afatal_x64");                         // no → controlled fatal instead of a stack read
    emitter.instruction("shl r9, 4");                                           // records are 16 bytes each
    emitter.instruction("lea r9, [r15 + r9]");                                  // address of the selected record
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // record payload word
    emitter.instruction("mov r11, QWORD PTR [r9 + 8]");                         // record tag word (tag | length << 8)
    emitter.instruction("movzx r8d, BYTE PTR [rbp - 104]");                     // reload the conversion character
}

/// Emits the x86_64 conversion dispatch. Only the conversion characters PHP defines are
/// accepted; anything else takes the controlled `ValueError` path rather than being handed
/// to libc, which is what keeps `%n` and other libc-only conversions unreachable.
fn emit_conversion_dispatch(emitter: &mut Emitter) {
    emitter.instruction("cmp r8b, 115");                                        // 's' string conversion?
    emitter.instruction("je __rt_sprintf_t_str_x64");                           // yes → string path
    emitter.instruction("cmp r8b, 100");                                        // 'd' signed decimal?
    emitter.instruction("je __rt_sprintf_t_int_x64");                           // yes → integer path
    emitter.instruction("cmp r8b, 117");                                        // 'u' unsigned decimal?
    emitter.instruction("je __rt_sprintf_t_int_x64");                           // yes → integer path
    emitter.instruction("cmp r8b, 111");                                        // 'o' octal?
    emitter.instruction("je __rt_sprintf_t_int_x64");                           // yes → integer path
    emitter.instruction("cmp r8b, 120");                                        // 'x' lowercase hexadecimal?
    emitter.instruction("je __rt_sprintf_t_int_x64");                           // yes → integer path
    emitter.instruction("cmp r8b, 88");                                         // 'X' uppercase hexadecimal?
    emitter.instruction("je __rt_sprintf_t_int_x64");                           // yes → integer path
    emitter.instruction("cmp r8b, 98");                                         // 'b' binary?
    emitter.instruction("je __rt_sprintf_t_int_x64");                           // yes → integer coercion, then the binary body
    emitter.instruction("cmp r8b, 99");                                         // 'c' single character?
    emitter.instruction("je __rt_sprintf_t_int_x64");                           // yes → integer coercion, then the single-byte body
    emitter.instruction("cmp r8b, 102");                                        // 'f' fixed-point?
    emitter.instruction("je __rt_sprintf_t_flt_x64");                           // yes → float path
    emitter.instruction("cmp r8b, 70");                                         // 'F' locale-independent fixed-point?
    emitter.instruction("je __rt_sprintf_t_flt_x64");                           // yes → float path
    emitter.instruction("cmp r8b, 101");                                        // 'e' scientific?
    emitter.instruction("je __rt_sprintf_t_flt_x64");                           // yes → float path
    emitter.instruction("cmp r8b, 69");                                         // 'E' uppercase scientific?
    emitter.instruction("je __rt_sprintf_t_flt_x64");                           // yes → float path
    emitter.instruction("cmp r8b, 103");                                        // 'g' shortest-of-e-or-f?
    emitter.instruction("je __rt_sprintf_t_flt_x64");                           // yes → float path
    emitter.instruction("cmp r8b, 71");                                         // 'G' uppercase shortest-of-E-or-f?
    emitter.instruction("je __rt_sprintf_t_flt_x64");                           // yes → float path
    emitter.instruction("jmp __rt_sprintf_sfatal_x64");                         // PHP rejects every other conversion
}

/// Emits the x86_64 `%s` conversion.
///
/// A string record is emitted straight from its pointer/length pair (so the result is
/// binary safe and not capped at any scratch-buffer size); precision truncates it. A record
/// carrying another tag is rendered numerically instead of being dereferenced.
fn emit_string_conversion(emitter: &mut Emitter) {
    emitter.label("__rt_sprintf_t_str_x64");
    emitter.instruction("mov QWORD PTR [rbp - 688], 0");                        // this conversion owns no temporary string yet
    emitter.instruction("mov rax, r11");                                        // copy the record tag word
    emitter.instruction("and rax, 255");                                        // isolate the record type tag
    emit_branch_if_deferred_tag(emitter, "rax", "__rt_sprintf_str_mixed_x64");
    emitter.instruction("cmp rax, 3");                                          // boolean record?
    emitter.instruction("jne __rt_sprintf_str_not_bool_x64");                   // no → ordinary dispatch
    emitter.instruction("test r10, r10");                                       // true or false?
    emitter.instruction("jnz __rt_sprintf_str_num_x64");                        // true renders as integer one
    emitter.instruction("xor r10d, r10d");                                      // false string pointer
    emitter.instruction("xor r11d, r11d");                                      // false renders zero bytes
    emitter.instruction("jmp __rt_sprintf_str_ptr_x64");                        // apply width/precision to empty body
    emitter.label("__rt_sprintf_str_not_bool_x64");
    emitter.instruction("cmp rax, 1");                                          // is this record actually a string?
    emitter.instruction("jne __rt_sprintf_str_num_x64");                        // no → render the payload as a number
    emitter.instruction("shr r11, 8");                                          // string byte length lives above the tag
    emitter.instruction("test r10, r10");                                       // is the string pointer null?
    emitter.instruction("jnz __rt_sprintf_str_ptr_x64");                        // no → the pointer carries bytes
    emitter.instruction("xor r11d, r11d");                                      // treat a null string pointer as empty
    emitter.label("__rt_sprintf_str_ptr_x64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // parsed precision
    emitter.instruction("test rax, rax");                                       // was a precision given?
    emitter.instruction("js __rt_sprintf_emit_x64");                            // no → emit the whole string
    emitter.instruction("cmp r11, rax");                                        // is the string within the precision?
    emitter.instruction("jbe __rt_sprintf_emit_x64");                           // yes → emit it unchanged
    emitter.instruction("mov r11, rax");                                        // truncate the string to the precision
    emitter.instruction("jmp __rt_sprintf_emit_x64");                           // pad and copy the string body

    // -- non-string record under %s: format the payload instead of dereferencing it --
    emitter.label("__rt_sprintf_str_num_x64");
    emitter.instruction("mov QWORD PTR [rbp - 80], -1");                        // the %s precision must not reach the numeric path
    emitter.instruction("cmp rax, 2");                                          // is the payload a double?
    emitter.instruction("jne __rt_sprintf_str_int_x64");                        // no → render it as a signed integer
    emitter.instruction("mov QWORD PTR [rbp - 80], 14");                        // PHP renders floats with 14 significant digits
    emitter.instruction("mov r8d, 71");                                         // reuse the 'G' float conversion
    emitter.instruction("mov QWORD PTR [rbp - 104], r8");                       // record the substituted conversion character
    emitter.instruction("jmp __rt_sprintf_t_flt_x64");                          // format through the float path
    emitter.label("__rt_sprintf_str_int_x64");
    emitter.instruction("mov r8d, 100");                                        // reuse the 'd' integer conversion
    emitter.instruction("mov QWORD PTR [rbp - 104], r8");                       // record the substituted conversion character
    emitter.instruction("jmp __rt_sprintf_t_int_x64");                          // format through the integer path

    emitter.label("__rt_sprintf_str_mixed_x64");
    abi::emit_symbol_address(emitter, "r9", "_concat_buf");
    emitter.instruction("mov rax, rbx");                                        // copy the partial-result write cursor
    emitter.instruction("sub rax, r9");                                         // compute bytes already written before nested __toString
    emitter.instruction("mov r9, QWORD PTR [rbp - 56]");                        // reload the address of the global concat offset
    emitter.instruction("mov QWORD PTR [r9], rax");                             // make nested concat users start after the partial result
    emitter.instruction("mov rdi, r11");                                        // recover the deferred record metadata
    emitter.instruction("and rdi, 255");                                        // pass only its low-byte tag
    emitter.instruction("mov rsi, r10");                                        // pass the preserved record payload
    emitter.instruction("mov rdx, QWORD PTR [rbp - 680]");                      // pass the optional eval context
    emitter.instruction("call __rt_sprintf_mixed_to_string");                   // apply array/resource/object string semantics
    emitter.instruction("mov QWORD PTR [rbp - 688], rcx");                      // release an owned stabilized result after copying
    emitter.instruction("mov r10, rax");                                        // replace the record payload with the coerced string pointer
    emitter.instruction("mov r11, rdx");                                        // copy the coerced string byte length
    emitter.instruction("jmp __rt_sprintf_str_ptr_x64");                        // reuse precision, padding, and copy handling
}

/// Branches when an x86_64 sprintf record tag denotes deferred non-scalar coercion.
fn emit_branch_if_deferred_tag(emitter: &mut Emitter, tag_reg: &str, label: &str) {
    for tag in [4, 5, 6, 7, 9, 10, 11] {
        emitter.instruction(&format!("cmp {tag_reg}, {tag}"));
        emitter.instruction(&format!("je {label}"));
    }
}

/// Emits the x86_64 `%b` conversion body, which libc has no portable equivalent for.
/// Entered from the shared integer coercion with the operand already in `r10`. Digits are
/// generated backwards into the conversion scratch, so at most 64 bytes are written and the
/// result never carries leading zeros (PHP prints `0` for zero).
fn emit_binary_conversion(emitter: &mut Emitter) {
    emitter.label("__rt_sprintf_bin_go_x64");
    emitter.instruction("lea r9, [rbp - 600]");                                 // write backwards from conversion scratch + 72 bytes
    emitter.instruction("xor r11d, r11d");                                      // generated digit count
    emitter.label("__rt_sprintf_bin_loop_x64");
    emitter.instruction("mov rax, r10");                                        // copy the remaining value
    emitter.instruction("and rax, 1");                                          // take its low bit
    emitter.instruction("add al, 48");                                          // turn the bit into an ASCII digit
    emitter.instruction("sub r9, 1");                                           // step one byte back in the scratch
    emitter.instruction("mov BYTE PTR [r9], al");                               // store the digit
    emitter.instruction("add r11, 1");                                          // count the digit
    emitter.instruction("shr r10, 1");                                          // shift the value right by one bit
    emitter.instruction("jnz __rt_sprintf_bin_loop_x64");                       // more bits → keep going
    emitter.instruction("mov r10, r9");                                         // body pointer = first generated digit
    emitter.instruction("jmp __rt_sprintf_emit_x64");                           // pad and copy the binary body
}

/// Emits the x86_64 `%c` conversion body, entered from the shared integer coercion with the
/// operand already in `r10`. PHP appends the low byte of the argument and ignores width and
/// padding entirely, so the width slot is cleared before emitting.
fn emit_char_conversion(emitter: &mut Emitter) {
    emitter.label("__rt_sprintf_chr_go_x64");
    emitter.instruction("lea r9, [rbp - 672]");                                 // reuse the conversion scratch for one byte
    emitter.instruction("mov BYTE PTR [r9], r10b");                             // store the low byte of the argument
    emitter.instruction("mov r10, r9");                                         // body pointer = the stored byte
    emitter.instruction("mov r11d, 1");                                         // body length = one byte
    emitter.instruction("mov QWORD PTR [rbp - 72], 0");                         // PHP ignores width for %c
    emitter.instruction("jmp __rt_sprintf_emit_x64");                           // copy the single byte
}

/// Emits the x86_64 integer conversions (`%d`, `%u`, `%o`, `%x`, `%X`) plus the shared
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
    emitter.label("__rt_sprintf_t_int_x64");
    emitter.instruction("mov rax, r11");                                        // copy the record tag word
    emitter.instruction("and rax, 255");                                        // isolate the record type tag
    emit_branch_if_deferred_tag(emitter, "rax", "__rt_sprintf_int_mixed_x64");
    emitter.instruction("cmp rax, 1");                                          // is the payload a string pointer?
    emitter.instruction("je __rt_sprintf_int_str_x64");                         // yes → parse it instead of printing the pointer
    emitter.instruction("cmp rax, 2");                                          // is the payload a double?
    emitter.instruction("jne __rt_sprintf_int_ready_x64");                      // no → the payload is already an integer
    emitter.instruction("movq xmm0, r10");                                      // move the double bits into an SSE register
    emitter.instruction("cvttsd2si r10, xmm0");                                 // truncate the double toward zero like PHP
    emitter.instruction("jmp __rt_sprintf_int_ready_x64");                      // the operand is an integer now
    emitter.label("__rt_sprintf_int_str_x64");
    emitter.instruction("mov rax, r10");                                        // string pointer for the numeric parse
    emitter.instruction("mov rdx, r11");                                        // record tag word holding the string length
    emitter.instruction("shr rdx, 8");                                          // string byte length for the numeric parse
    emitter.instruction("test rax, rax");                                       // is the string pointer null?
    emitter.instruction("jz __rt_sprintf_int_str_null_x64");                    // yes → a null pointer parses as zero
    emitter.instruction("cmp rdx, 4095");                                       // __rt_cstr copies into a 4096-byte scratch
    emitter.instruction("jbe __rt_sprintf_int_str_go_x64");                     // the string already fits the C-string scratch
    emitter.instruction("mov edx, 4095");                                       // clamp so the numeric prefix parse stays in bounds
    emitter.label("__rt_sprintf_int_str_go_x64");
    emitter.instruction("call __rt_str_to_int");                                // PHP leading-numeric string-to-int conversion
    emitter.instruction("mov r10, rax");                                        // the parsed integer becomes the operand
    emitter.instruction("jmp __rt_sprintf_int_ready_x64");                      // the operand is an integer now
    emitter.label("__rt_sprintf_int_str_null_x64");
    emitter.instruction("xor r10d, r10d");                                      // a null string operand formats as zero
    emitter.instruction("jmp __rt_sprintf_int_ready_x64");                      // join the ordinary integer formatting path
    emitter.label("__rt_sprintf_int_mixed_x64");
    emitter.instruction("mov rdi, rax");                                        // pass the deferred record tag
    emitter.instruction("mov rsi, r10");                                        // pass the preserved record payload
    emitter.instruction("xor edx, edx");                                        // select integer conversion warning wording
    emitter.instruction("mov rcx, QWORD PTR [rbp - 680]");                      // pass the optional eval context for dynamic metadata
    emitter.instruction("call __rt_sprintf_mixed_to_int");                      // arrays/objects/callables/resources cast without pointer leakage
    emitter.instruction("mov r10, rax");                                        // use the normalized PHP integer as the operand
    emitter.label("__rt_sprintf_int_ready_x64");
    emitter.instruction("movzx r8d, BYTE PTR [rbp - 104]");                     // reload the conversion character after the parse
    emitter.instruction("cmp r8b, 98");                                         // is this the binary conversion?
    emitter.instruction("je __rt_sprintf_bin_go_x64");                          // yes → generate binary digits by hand
    emitter.instruction("cmp r8b, 99");                                         // is this the single-character conversion?
    emitter.instruction("je __rt_sprintf_chr_go_x64");                          // yes → emit the low byte directly
    emitter.label("__rt_sprintf_int_go_x64");
    emitter.instruction("lea r9, [rbp - 160]");                                 // mini C format cursor
    emitter.instruction("mov BYTE PTR [r9], 37");                               // write the '%' introducer
    emitter.instruction("add r9, 1");                                           // advance the mini format cursor
    emitter.instruction("cmp r8b, 100");                                        // only 'd' is signed, so only it can force a sign
    emitter.instruction("jne __rt_sprintf_int_noplus_x64");                     // other integer conversions ignore '+'
    emitter.instruction("test QWORD PTR [rbp - 88], 2");                        // is the force-sign flag set?
    emitter.instruction("jz __rt_sprintf_int_noplus_x64");                      // no → skip the '+' flag
    emitter.instruction("mov BYTE PTR [r9], 43");                               // write the '+' flag
    emitter.instruction("add r9, 1");                                           // advance the mini format cursor
    emitter.label("__rt_sprintf_int_noplus_x64");
    emitter.instruction("cmp r8b, 100");                                        // '#' is meaningless for 'd'
    emitter.instruction("je __rt_sprintf_int_noalt_x64");                       // skip the alternate-form flag
    emitter.instruction("cmp r8b, 117");                                        // '#' is meaningless for 'u'
    emitter.instruction("je __rt_sprintf_int_noalt_x64");                       // skip the alternate-form flag
    emitter.instruction("test QWORD PTR [rbp - 88], 4");                        // is the alternate-form flag set?
    emitter.instruction("jz __rt_sprintf_int_noalt_x64");                       // no → skip the '#' flag
    emitter.instruction("mov BYTE PTR [r9], 35");                               // write the '#' flag
    emitter.instruction("add r9, 1");                                           // advance the mini format cursor
    emitter.label("__rt_sprintf_int_noalt_x64");
    emitter.instruction("mov BYTE PTR [r9], 108");                              // write the first 'l' length modifier
    emitter.instruction("add r9, 1");                                           // advance the mini format cursor
    emitter.instruction("mov BYTE PTR [r9], 108");                              // write the second 'l' for a 64-bit operand
    emitter.instruction("add r9, 1");                                           // advance the mini format cursor
    emitter.instruction("mov BYTE PTR [r9], r8b");                              // write the conversion character
    emitter.instruction("add r9, 1");                                           // advance the mini format cursor
    emitter.instruction("mov BYTE PTR [r9], 0");                                // NUL-terminate the mini C format string
    emitter.instruction("lea rdi, [rbp - 672]");                                // conversion scratch destination
    emitter.instruction(&format!("mov esi, {}", CONV_SCRATCH_CAP));             // conversion scratch capacity
    emitter.instruction("lea rdx, [rbp - 160]");                                // the mini C format string
    emitter.instruction("mov rcx, r10");                                        // the integer operand as the first variadic
    emitter.instruction("xor eax, eax");                                        // no SSE variadic registers are live
    emitter.bl_c("snprintf");                                                   // render the integer body through libc
    emitter.instruction("jmp __rt_sprintf_snret_x64");                          // clamp and take the result
}

/// Emits the x86_64 float conversions (`%f`, `%F`, `%e`, `%E`, `%g`, `%G`).
///
/// The record tag decides the coercion: an int/bool payload is widened and a string is
/// parsed by `__rt_str_to_number`, so a mismatched record never reaches libc as raw pointer
/// bits. Precision is clamped to PHP's 53-digit maximum, which is what bounds the libc output to
/// the conversion scratch. `%f`/`%F`/`%e`/`%E` of negative zero print unsigned in PHP (its
/// own float renderer never emits the sign), while `%g`/`%G` keep it.
fn emit_float_conversion(emitter: &mut Emitter) {
    emitter.label("__rt_sprintf_t_flt_x64");
    emitter.instruction("mov rax, r11");                                        // copy the record tag word
    emitter.instruction("and rax, 255");                                        // isolate the record type tag
    emit_branch_if_deferred_tag(emitter, "rax", "__rt_sprintf_flt_mixed_x64");
    emitter.instruction("cmp rax, 2");                                          // is the payload already a double?
    emitter.instruction("je __rt_sprintf_flt_bits_x64");                        // yes → use its bit pattern directly
    emitter.instruction("cmp rax, 1");                                          // is the payload a string pointer?
    emitter.instruction("je __rt_sprintf_flt_str_x64");                         // yes → parse it instead of reading the pointer bits
    emitter.instruction("cvtsi2sd xmm0, r10");                                  // widen an int/bool payload to a double
    emitter.instruction("movq r10, xmm0");                                      // keep the double bits in the integer register
    emitter.instruction("jmp __rt_sprintf_flt_bits_x64");                       // the operand is a double now
    emitter.label("__rt_sprintf_flt_str_x64");
    emitter.instruction("mov rax, r10");                                        // string pointer for the numeric parse
    emitter.instruction("mov rdx, r11");                                        // record tag word holding the string length
    emitter.instruction("shr rdx, 8");                                          // string byte length for the numeric parse
    emitter.instruction("test rax, rax");                                       // is the string pointer null?
    emitter.instruction("jz __rt_sprintf_flt_str_null_x64");                    // yes → a null pointer parses as zero
    emitter.instruction("cmp rdx, 4095");                                       // __rt_cstr copies into a 4096-byte scratch
    emitter.instruction("jbe __rt_sprintf_flt_str_go_x64");                     // the string already fits the C-string scratch
    emitter.instruction("mov edx, 4095");                                       // clamp so the numeric prefix parse stays in bounds
    emitter.label("__rt_sprintf_flt_str_go_x64");
    emitter.instruction("call __rt_str_to_number");                             // PHP leading-numeric string-to-float conversion
    emitter.instruction("movq r10, xmm0");                                      // keep the parsed double bits in the integer register
    emitter.instruction("movzx r8d, BYTE PTR [rbp - 104]");                     // reload the conversion character after the parse
    emitter.instruction("jmp __rt_sprintf_flt_bits_x64");                       // the operand is a double now
    emitter.label("__rt_sprintf_flt_str_null_x64");
    emitter.instruction("xor r10d, r10d");                                      // a null string operand formats as zero
    emitter.instruction("jmp __rt_sprintf_flt_bits_x64");                       // join the ordinary floating formatting path
    emitter.label("__rt_sprintf_flt_mixed_x64");
    emitter.instruction("mov rdi, rax");                                        // pass the deferred record tag
    emitter.instruction("mov rsi, r10");                                        // pass the preserved record payload
    emitter.instruction("mov edx, 1");                                          // select float conversion warning wording
    emitter.instruction("mov rcx, QWORD PTR [rbp - 680]");                      // pass the optional eval context for dynamic metadata
    emitter.instruction("call __rt_sprintf_mixed_to_int");                      // non-scalars share PHP's zero/one/resource-id numeric cast
    emitter.instruction("cvtsi2sd xmm0, rax");                                  // widen the normalized integer to a PHP float operand
    emitter.instruction("movq r10, xmm0");                                      // keep the double bits in the record payload register
    emitter.instruction("movzx r8d, BYTE PTR [rbp - 104]");                     // reload the conversion character clobbered by the helper call
    emitter.label("__rt_sprintf_flt_bits_x64");
    emitter.instruction("cmp r8b, 103");                                        // 'g' keeps PHP's negative-zero sign
    emitter.instruction("je __rt_sprintf_flt_nz_x64");                          // skip the negative-zero normalization
    emitter.instruction("cmp r8b, 71");                                         // 'G' keeps PHP's negative-zero sign
    emitter.instruction("je __rt_sprintf_flt_nz_x64");                          // skip the negative-zero normalization
    emitter.instruction("mov rax, r10");                                        // copy the double bits
    emitter.instruction("add rax, rax");                                        // drop the sign bit to test for any zero
    emitter.instruction("test rax, rax");                                       // is the value a zero of either sign?
    emitter.instruction("jnz __rt_sprintf_flt_nz_x64");                         // no → leave the value alone
    emitter.instruction("xor r10d, r10d");                                      // PHP prints -0.0 as 0.000000 under %f/%e
    emitter.label("__rt_sprintf_flt_nz_x64");
    emitter.instruction("lea r9, [rbp - 160]");                                 // mini C format cursor
    emitter.instruction("mov BYTE PTR [r9], 37");                               // write the '%' introducer
    emitter.instruction("add r9, 1");                                           // advance the mini format cursor
    emitter.instruction("test QWORD PTR [rbp - 88], 2");                        // is the force-sign flag set?
    emitter.instruction("jz __rt_sprintf_flt_noplus_x64");                      // no → skip the '+' flag
    emitter.instruction("mov BYTE PTR [r9], 43");                               // write the '+' flag
    emitter.instruction("add r9, 1");                                           // advance the mini format cursor
    emitter.label("__rt_sprintf_flt_noplus_x64");
    emitter.instruction("test QWORD PTR [rbp - 88], 4");                        // is the alternate-form flag set?
    emitter.instruction("jz __rt_sprintf_flt_noalt_x64");                       // no → skip the '#' flag
    emitter.instruction("mov BYTE PTR [r9], 35");                               // write the '#' flag
    emitter.instruction("add r9, 1");                                           // advance the mini format cursor
    emitter.label("__rt_sprintf_flt_noalt_x64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // parsed precision
    emitter.instruction("test rax, rax");                                       // was a precision given?
    emitter.instruction("js __rt_sprintf_flt_noprec_x64");                      // no → libc's default of six digits
    emitter.instruction("cmp rax, 53");                                         // PHP caps float precision at 53 digits
    emitter.instruction("jbe __rt_sprintf_flt_precok_x64");                     // within the cap
    emitter.instruction("mov rax, 53");                                         // clamp to PHP's maximum precision
    emitter.label("__rt_sprintf_flt_precok_x64");
    emitter.instruction("mov BYTE PTR [r9], 46");                               // write the '.' precision introducer
    emitter.instruction("add r9, 1");                                           // advance the mini format cursor
    emitter.instruction("cmp rax, 10");                                         // does the precision need two digits?
    emitter.instruction("jb __rt_sprintf_flt_prec1_x64");                       // no → a single digit is enough
    emitter.instruction("xor rdx, rdx");                                        // clear the high half of the dividend
    emitter.instruction("mov rcx, 10");                                         // decimal radix for the split
    emitter.instruction("div rcx");                                             // rax = tens digit, rdx = units digit
    emitter.instruction("add al, 48");                                          // turn the tens digit into ASCII
    emitter.instruction("mov BYTE PTR [r9], al");                               // write the tens digit
    emitter.instruction("add r9, 1");                                           // advance the mini format cursor
    emitter.instruction("add dl, 48");                                          // turn the units digit into ASCII
    emitter.instruction("mov BYTE PTR [r9], dl");                               // write the units digit
    emitter.instruction("add r9, 1");                                           // advance the mini format cursor
    emitter.instruction("jmp __rt_sprintf_flt_noprec_x64");                     // precision written
    emitter.label("__rt_sprintf_flt_prec1_x64");
    emitter.instruction("add al, 48");                                          // turn the single digit into ASCII
    emitter.instruction("mov BYTE PTR [r9], al");                               // write the single precision digit
    emitter.instruction("add r9, 1");                                           // advance the mini format cursor
    emitter.label("__rt_sprintf_flt_noprec_x64");
    emitter.instruction("mov BYTE PTR [r9], r8b");                              // write the conversion character
    emitter.instruction("add r9, 1");                                           // advance the mini format cursor
    emitter.instruction("mov BYTE PTR [r9], 0");                                // NUL-terminate the mini C format string
    emitter.instruction("lea rdi, [rbp - 672]");                                // conversion scratch destination
    emitter.instruction(&format!("mov esi, {}", CONV_SCRATCH_CAP));             // conversion scratch capacity
    emitter.instruction("lea rdx, [rbp - 160]");                                // the mini C format string
    emitter.instruction("movq xmm0, r10");                                      // the double operand as the first SSE variadic
    emitter.instruction("mov eax, 1");                                          // one SSE variadic register is live
    emitter.bl_c("snprintf");                                                   // render the float body through libc
    emitter.instruction("jmp __rt_sprintf_snret_x64");                          // clamp and take the result
}

/// Emits the x86_64 post-`snprintf` clamp.
///
/// libc returns the number of bytes it *would* have written, so the value is clamped to the
/// bytes actually present in the scratch buffer before it is ever used as a length. That
/// clamp is the direct fix for the out-of-bounds stack read this helper used to have.
fn emit_snprintf_result(emitter: &mut Emitter) {
    emitter.label("__rt_sprintf_snret_x64");
    emitter.instruction("movsxd r11, eax");                                     // snprintf returns a signed 32-bit count
    emitter.instruction("test r11, r11");                                       // is the count negative?
    emitter.instruction("jns __rt_sprintf_snret_nn_x64");                       // no → usable as a length
    emitter.instruction("xor r11d, r11d");                                      // an encoding error produced no bytes
    emitter.label("__rt_sprintf_snret_nn_x64");
    emitter.instruction(&format!("cmp r11, {}", CONV_SCRATCH_CAP - 1));         // did libc want more than the scratch holds?
    emitter.instruction("jbe __rt_sprintf_snret_ok_x64");                       // no → every counted byte is really there
    emitter.instruction(&format!("mov r11d, {}", CONV_SCRATCH_CAP - 1));        // clamp to the bytes actually written
    emitter.label("__rt_sprintf_snret_ok_x64");
    emitter.instruction("lea r10, [rbp - 672]");                                // body pointer = conversion scratch
    emitter.instruction("movzx r9d, BYTE PTR [rbp - 104]");                     // reload the conversion character
    emitter.instruction("cmp r9b, 101");                                        // 'e' needs PHP's exponent form
    emitter.instruction("je __rt_sprintf_expfix_x64");                          // compact the exponent
    emitter.instruction("cmp r9b, 69");                                         // 'E' needs PHP's exponent form
    emitter.instruction("je __rt_sprintf_expfix_x64");                          // compact the exponent
    emitter.instruction("jmp __rt_sprintf_emit_x64");                           // pad and copy the rendered body
}

/// Emits the x86_64 exponent compaction for `%e`/`%E`.
///
/// C always pads the exponent to at least two digits (`1.234568e+04`) while PHP does not
/// (`1.234568e+4`), so the leading zeros of the exponent field are removed in place, always
/// leaving at least one digit.
fn emit_exponent_compaction(emitter: &mut Emitter) {
    emitter.label("__rt_sprintf_expfix_x64");
    emitter.instruction("mov rax, r10");                                        // read cursor over the rendered body
    emitter.instruction("mov rcx, r10");                                        // write cursor for the compacted body
    emitter.instruction("lea rdx, [r10 + r11]");                                // one past the last rendered byte
    emitter.label("__rt_sprintf_expfix_scan_x64");
    emitter.instruction("cmp rax, rdx");                                        // reached the end without an exponent?
    emitter.instruction("jae __rt_sprintf_expfix_done_x64");                    // yes → nothing to compact
    emitter.instruction("movzx r8d, BYTE PTR [rax]");                           // load the current body byte
    emitter.instruction("cmp r8b, 101");                                        // lowercase exponent marker?
    emitter.instruction("je __rt_sprintf_expfix_hit_x64");                      // yes → compact from here
    emitter.instruction("cmp r8b, 69");                                         // uppercase exponent marker?
    emitter.instruction("je __rt_sprintf_expfix_hit_x64");                      // yes → compact from here
    emitter.instruction("mov BYTE PTR [rcx], r8b");                             // keep the mantissa byte
    emitter.instruction("add rax, 1");                                          // advance the read cursor
    emitter.instruction("add rcx, 1");                                          // advance the write cursor
    emitter.instruction("jmp __rt_sprintf_expfix_scan_x64");                    // keep scanning for the exponent
    emitter.label("__rt_sprintf_expfix_hit_x64");
    emitter.instruction("mov BYTE PTR [rcx], r8b");                             // keep the exponent marker
    emitter.instruction("add rax, 1");                                          // advance the read cursor
    emitter.instruction("add rcx, 1");                                          // advance the write cursor
    emitter.instruction("cmp rax, rdx");                                        // is there anything after the marker?
    emitter.instruction("jae __rt_sprintf_expfix_done_x64");                    // no → the body ends here
    emitter.instruction("movzx r8d, BYTE PTR [rax]");                           // load the exponent sign byte
    emitter.instruction("cmp r8b, 43");                                         // '+' exponent sign?
    emitter.instruction("je __rt_sprintf_expfix_sign_x64");                     // yes → keep it
    emitter.instruction("cmp r8b, 45");                                         // '-' exponent sign?
    emitter.instruction("jne __rt_sprintf_expfix_zeros_x64");                   // no sign at all → go straight to the digits
    emitter.label("__rt_sprintf_expfix_sign_x64");
    emitter.instruction("mov BYTE PTR [rcx], r8b");                             // keep the exponent sign
    emitter.instruction("add rax, 1");                                          // advance the read cursor
    emitter.instruction("add rcx, 1");                                          // advance the write cursor
    emitter.label("__rt_sprintf_expfix_zeros_x64");
    emitter.instruction("lea rsi, [rdx - 1]");                                  // address of the final exponent digit
    emitter.label("__rt_sprintf_expfix_zloop_x64");
    emitter.instruction("cmp rax, rsi");                                        // never drop the last exponent digit
    emitter.instruction("jae __rt_sprintf_expfix_tail_x64");                    // one digit left → stop stripping
    emitter.instruction("movzx r8d, BYTE PTR [rax]");                           // load the current exponent digit
    emitter.instruction("cmp r8b, 48");                                         // is it a padding zero?
    emitter.instruction("jne __rt_sprintf_expfix_tail_x64");                    // no → the exponent starts here
    emitter.instruction("add rax, 1");                                          // skip the padding zero
    emitter.instruction("jmp __rt_sprintf_expfix_zloop_x64");                   // check the next exponent digit
    emitter.label("__rt_sprintf_expfix_tail_x64");
    emitter.instruction("cmp rax, rdx");                                        // copied every remaining byte?
    emitter.instruction("jae __rt_sprintf_expfix_done_x64");                    // yes → compaction finished
    emitter.instruction("movzx r8d, BYTE PTR [rax]");                           // load the next exponent byte
    emitter.instruction("mov BYTE PTR [rcx], r8b");                             // keep the exponent byte
    emitter.instruction("add rax, 1");                                          // advance the read cursor
    emitter.instruction("add rcx, 1");                                          // advance the write cursor
    emitter.instruction("jmp __rt_sprintf_expfix_tail_x64");                    // copy the rest of the exponent
    emitter.label("__rt_sprintf_expfix_done_x64");
    emitter.instruction("mov r11, rcx");                                        // compacted end address
    emitter.instruction("sub r11, r10");                                        // compacted body length
}

/// Emits the x86_64 pad-and-copy stage shared by every conversion.
///
/// `r10`/`r11` carry the conversion body. The field width is validated against PHP's
/// `0..INT_MAX` range and the whole padded result is bounds-checked against the end of
/// `_concat_buf` *before* a single byte is written, so neither an absurd width nor a long
/// body can walk off the arena. Zero padding is inserted after a leading sign, matching
/// PHP's `sprintf("%05d", -42)` → `-0042`.
fn emit_pad_and_copy(emitter: &mut Emitter) {
    emitter.label("__rt_sprintf_emit_x64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // parsed field width
    emitter.instruction("mov rcx, rax");                                        // copy it for the range test
    emitter.instruction("shr rcx, 31");                                         // any bit above INT_MAX set?
    emitter.instruction("test rcx, rcx");                                       // is the width out of PHP's range?
    emitter.instruction("jnz __rt_sprintf_wfatal_x64");                         // yes → PHP rejects the width
    emitter.instruction("xor rcx, rcx");                                        // padding byte count
    emitter.instruction("cmp rax, r11");                                        // is the body already at least as wide?
    emitter.instruction("jbe __rt_sprintf_emit_nopad_x64");                     // yes → no padding needed
    emitter.instruction("mov rcx, rax");                                        // padding = width ...
    emitter.instruction("sub rcx, r11");                                        // ... minus the body length
    emitter.label("__rt_sprintf_emit_nopad_x64");
    emitter.instruction("mov rdx, r11");                                        // body length
    emitter.instruction("add rdx, rcx");                                        // total bytes this conversion emits
    emitter.instruction("add rdx, rbx");                                        // address just past the emitted bytes
    emitter.instruction("cmp rdx, QWORD PTR [rbp - 120]");                      // would the conversion leave the arena?
    emitter.instruction("ja __rt_sprintf_ofatal_x64");                          // yes → controlled fatal instead of an overrun
    emitter.instruction("mov r8, rdx");                                         // preserve the end of the complete padded output range
    emitter.instruction("mov rdx, rbx");                                        // default final body destination for a left-aligned field
    emitter.instruction("test QWORD PTR [rbp - 88], 1");                        // is the left-align flag set?
    emitter.instruction("jnz __rt_sprintf_overlap_dest_x64");                   // yes → the body starts at the write cursor
    emitter.instruction("add rdx, rcx");                                        // right alignment places the body after its leading padding
    emitter.label("__rt_sprintf_overlap_dest_x64");
    emitter.instruction("cmp r10, rdx");                                        // is the body already at its final destination?
    emitter.instruction("je __rt_sprintf_overlap_done_x64");                    // yes → no relocation is needed
    emitter.instruction("lea rax, [r10 + r11]");                                // one-past-the-end source address
    emitter.instruction("cmp rax, rbx");                                        // does the source finish before the complete output starts?
    emitter.instruction("jbe __rt_sprintf_overlap_done_x64");                   // yes → ordinary forward copy cannot clobber it
    emitter.instruction("cmp r8, r10");                                         // does the complete padded output finish before the source starts?
    emitter.instruction("jbe __rt_sprintf_overlap_done_x64");                   // yes → the ranges do not overlap
    emitter.instruction("cmp rdx, r10");                                        // which direction makes this memmove safe?
    emitter.instruction("jb __rt_sprintf_overlap_forward_x64");                 // lower destination copies from the beginning
    emitter.instruction("mov rsi, r11");                                        // backward-copy byte count
    emitter.label("__rt_sprintf_overlap_backward_x64");
    emitter.instruction("test rsi, rsi");                                       // every overlapping byte has reached its final slot?
    emitter.instruction("jz __rt_sprintf_overlap_moved_x64");                   // yes → publish the relocated source
    emitter.instruction("sub rsi, 1");                                          // walk both ranges from their final byte
    emitter.instruction("movzx edi, BYTE PTR [r10 + rsi]");                     // load before a higher destination can overwrite the source
    emitter.instruction("mov BYTE PTR [rdx + rsi], dil");                       // place the byte at its final body position
    emitter.instruction("jmp __rt_sprintf_overlap_backward_x64");               // continue toward the start of the body
    emitter.label("__rt_sprintf_overlap_forward_x64");
    emitter.instruction("xor esi, esi");                                        // forward-copy byte index
    emitter.label("__rt_sprintf_overlap_forward_loop_x64");
    emitter.instruction("cmp rsi, r11");                                        // copied the whole overlapping body?
    emitter.instruction("jae __rt_sprintf_overlap_moved_x64");                  // yes → publish the relocated source
    emitter.instruction("movzx edi, BYTE PTR [r10 + rsi]");                     // read the next byte before the lower destination touches it
    emitter.instruction("mov BYTE PTR [rdx + rsi], dil");                       // place the byte at its final body position
    emitter.instruction("add rsi, 1");                                          // advance through the body
    emitter.instruction("jmp __rt_sprintf_overlap_forward_loop_x64");           // keep copying toward the end
    emitter.label("__rt_sprintf_overlap_moved_x64");
    emitter.instruction("mov r10, rdx");                                        // subsequent padding/copy reads from the safe final location
    emitter.label("__rt_sprintf_overlap_done_x64");
    emitter.instruction("movzx r9d, BYTE PTR [rbp - 96]");                      // pad character
    emitter.instruction("test QWORD PTR [rbp - 88], 1");                        // is the left-align flag set?
    emitter.instruction("jnz __rt_sprintf_emit_left_x64");                      // yes → body first, padding after
    emitter.instruction("test rcx, rcx");                                       // is there any padding at all?
    emitter.instruction("jz __rt_sprintf_emit_pad_x64");                        // no → copy the body directly
    emitter.instruction("cmp r9b, 48");                                         // only '0' padding moves ahead of the sign
    emitter.instruction("jne __rt_sprintf_emit_pad_x64");                       // other pad characters stay before the sign
    emitter.instruction("test r11, r11");                                       // is the body empty?
    emitter.instruction("jz __rt_sprintf_emit_pad_x64");                        // yes → there is no sign to hoist
    emitter.instruction("movzx r8d, BYTE PTR [r10]");                           // first body byte
    emitter.instruction("cmp r8b, 45");                                         // is it a minus sign?
    emitter.instruction("je __rt_sprintf_emit_sign_x64");                       // yes → emit it before the zeros
    emitter.instruction("cmp r8b, 43");                                         // is it a plus sign?
    emitter.instruction("jne __rt_sprintf_emit_pad_x64");                       // no sign → pad normally
    emitter.label("__rt_sprintf_emit_sign_x64");
    emitter.instruction("mov BYTE PTR [rbx], r8b");                             // emit the sign ahead of the zero padding
    emitter.instruction("add rbx, 1");                                          // advance the write cursor
    emitter.instruction("add r10, 1");                                          // the sign is no longer part of the body
    emitter.instruction("sub r11, 1");                                          // shorten the body accordingly
    emitter.label("__rt_sprintf_emit_pad_x64");
    emitter.instruction("test rcx, rcx");                                       // any padding bytes left?
    emitter.instruction("jz __rt_sprintf_emit_copy_x64");                       // no → copy the body
    emitter.instruction("mov BYTE PTR [rbx], r9b");                             // emit one padding byte
    emitter.instruction("add rbx, 1");                                          // advance the write cursor
    emitter.instruction("sub rcx, 1");                                          // one padding byte fewer to write
    emitter.instruction("jmp __rt_sprintf_emit_pad_x64");                       // keep padding
    emitter.label("__rt_sprintf_emit_copy_x64");
    emitter.instruction("test r11, r11");                                       // any body bytes left?
    emitter.instruction("jz __rt_sprintf_emit_done_x64");                       // no → release any temporary owner
    emitter.instruction("movzx r8d, BYTE PTR [r10]");                           // load the next body byte
    emitter.instruction("mov BYTE PTR [rbx], r8b");                             // emit the body byte
    emitter.instruction("add r10, 1");                                          // advance the body cursor
    emitter.instruction("add rbx, 1");                                          // advance the write cursor
    emitter.instruction("sub r11, 1");                                          // one body byte fewer to copy
    emitter.instruction("jmp __rt_sprintf_emit_copy_x64");                      // keep copying
    emitter.label("__rt_sprintf_emit_left_x64");
    emitter.instruction("test r11, r11");                                       // any body bytes left?
    emitter.instruction("jz __rt_sprintf_emit_lpad_x64");                       // no → append the padding
    emitter.instruction("movzx r8d, BYTE PTR [r10]");                           // load the next body byte
    emitter.instruction("mov BYTE PTR [rbx], r8b");                             // emit the body byte
    emitter.instruction("add r10, 1");                                          // advance the body cursor
    emitter.instruction("add rbx, 1");                                          // advance the write cursor
    emitter.instruction("sub r11, 1");                                          // one body byte fewer to copy
    emitter.instruction("jmp __rt_sprintf_emit_left_x64");                      // keep copying
    emitter.label("__rt_sprintf_emit_lpad_x64");
    emitter.instruction("test rcx, rcx");                                       // any trailing padding left?
    emitter.instruction("jz __rt_sprintf_emit_done_x64");                       // no → release any temporary owner
    emitter.instruction("mov BYTE PTR [rbx], r9b");                             // emit one trailing padding byte
    emitter.instruction("add rbx, 1");                                          // advance the write cursor
    emitter.instruction("sub rcx, 1");                                          // one padding byte fewer to write
    emitter.instruction("jmp __rt_sprintf_emit_lpad_x64");                      // keep padding
    emitter.label("__rt_sprintf_emit_done_x64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 688]");                      // formatter-owned string produced by __toString, if any
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_sprintf_loop_x64");                            // borrowed/numeric bodies need no cleanup
    emitter.instruction("mov QWORD PTR [rbp - 688], 0");                        // prevent stale ownership from crossing conversions
    emitter.instruction("call __rt_heap_free_safe");                            // release only after every output byte was copied
    emitter.instruction("jmp __rt_sprintf_loop_x64");                           // scan the next format byte
}

/// Emits the x86_64 controlled-fatal exits for invalid widths/specifiers/argument counts,
/// concat overflow, and non-stringable boxed values. Each writes a PHP-shaped diagnostic
/// to stderr and exits with PHP's fatal-error status (255).
fn emit_fatal_paths(emitter: &mut Emitter) {
    emit_fatal(emitter, "__rt_sprintf_wfatal_x64", "_sprintf_width_msg", SPRINTF_WIDTH_MSG.len());
    emit_fatal(emitter, "__rt_sprintf_ofatal_x64", "_sprintf_overflow_msg", SPRINTF_OVERFLOW_MSG.len());
    emit_fatal(emitter, "__rt_sprintf_afatal_x64", "_sprintf_argcount_msg", SPRINTF_ARGCOUNT_MSG.len());
    emit_fatal(emitter, "__rt_sprintf_sfatal_x64", "_sprintf_unknown_spec_msg", SPRINTF_UNKNOWN_SPEC_MSG.len());
}

/// Emits one x86_64 fatal exit block: write `len` bytes of `symbol` to stderr with the
/// Linux `write` syscall, then exit with status 255 (PHP's fatal-error status).
fn emit_fatal(emitter: &mut Emitter, label: &str, symbol: &str, len: usize) {
    emitter.label(label);
    emitter.instruction("mov edi, 2");                                          // write the diagnostic to stderr
    abi::emit_symbol_address(emitter, "rsi", symbol);
    emitter.instruction(&format!("mov edx, {}", len));                          // exact diagnostic byte length
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // emit the diagnostic before terminating
    emitter.instruction("mov edi, 255");                                        // PHP exits with 255 on a fatal error
    emitter.instruction("mov eax, 60");                                         // Linux x86_64 syscall 60 = exit
    emitter.instruction("syscall");                                             // terminate the process
}
