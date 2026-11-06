//! Purpose:
//! Emits the `@<timestamp>` epoch parser sub-routine consumed by the `__rt_strtotime` dispatcher.
//! Accepts `@`, an optional `-`, decimal digits, an optional `.` fraction of 1 to 6 digits, and
//! a trailing timezone token, the subset of PHP's `timestamp`/`timestampms` rules that leaves the
//! timestamp unchanged.
//!
//! Called from:
//! - `crate::codegen_support::runtime::system::strtotime::mod::emit_strtotime()` via the dispatcher's
//!   first-byte `@` branch.
//!
//! Key details:
//! - Entry label `__rt_strtotime_epoch_entry` (ARM64) / `_linux_x86_64` (x86_64); the dispatcher
//!   frame is already set up, with the trimmed ptr at `[sp+48]` and trimmed len at `[sp+56]`.
//! - The value is a literal UNIX timestamp (UTC), so it is returned directly without `mktime`.
//!   A negative value with a non-zero fraction floors (`@-5.5` is `-6`), and a magnitude outside
//!   the `i64` range fails, both as in PHP. The floor is a wrapping one-second subtraction on
//!   purpose: timelib floors the same way, so PHP 8.5 answers `9223372036854775807` for
//!   `@-9223372036854775808.5`, and this parser answers the same (pinned by
//!   `test_strtotime_epoch_range_edges_match_php`). `i64::MIN` itself is a valid result: the
//!   shared epilogue reports success through a flag, not through the value.
//! - PHP parses whatever follows the number with its full grammar, and because the timestamp
//!   already set a timezone, one more timezone token only raises a warning. The tail accepted here
//!   is exactly that: `[ \t,.]` separators around one optional `(`? letters{1,6} `)`? token.
//!   A token that is a strtotime keyword (`noon`, `ago`, a weekday or month name, ...) changes
//!   PHP's result, so it is rejected instead of being mistaken for a timezone; so is every other
//!   tail PHP evaluates (`+1 day`, `12:30`, `Europe/Rome`, ...). Those answer `false`.
//! - Scratch slots: `[sp+80]` timestamp, `[sp+88]` cursor after the token, `[sp+96]` token
//!   length (x86_64: `[rbp-48]`, `[rbp-40]`, `[rbp-32]`), kept across the keyword helpers.
//! - All exits branch to the shared `__rt_strtotime_ret` / `__rt_strtotime_fail` epilogues.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Maximum number of fraction digits PHP's `timestampms` rule accepts after the `.`.
const MAX_FRACTION_DIGITS: u64 = 6;
/// Maximum length of the letters in a timezone token (PHP's `tz` rule: `[A-Za-z]{1,6}`).
const MAX_TZ_LETTERS: u64 = 6;

/// Dispatches to the architecture-specific `@<timestamp>` epoch parser.
pub(crate) fn emit_epoch(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_epoch_linux_x86_64(emitter);
        return;
    }

    emit_epoch_arm64(emitter);
}

/// Emits a branch to `target` when the byte in `reg` is a tail separator (` `, `\t`, `,`, `.`).
fn emit_arm64_branch_if_separator(emitter: &mut Emitter, reg: &str, target: &str) {
    for byte in [32, 9, 44, 46] {
        emitter.instruction(&format!("cmp {reg}, #{byte}"));                    // is this byte a separator?
        emitter.instruction(&format!("b.eq {target}"));                         // skip the separator
    }
}

/// Emits a branch to `target` when the byte in `reg` is a tail separator (` `, `\t`, `,`, `.`).
fn emit_x86_64_branch_if_separator(emitter: &mut Emitter, reg: &str, target: &str) {
    for byte in [32, 9, 44, 46] {
        emitter.instruction(&format!("cmp {reg}, {byte}"));                     // is this byte a separator?
        emitter.instruction(&format!("je {target}"));                           // skip the separator
    }
}

/// Emits ARM64 assembly for the `@<timestamp>` epoch parser.
///
/// Entry label: `__rt_strtotime_epoch_entry`. Skips the leading `@`, reads an optional `-`,
/// accumulates the digits as an unsigned magnitude (failing on overflow), checks the signed
/// `i64` range, then reads an optional 1-6 digit fraction and the timezone tail described in
/// the module docs. The result timestamp is returned via `__rt_strtotime_ret`.
fn emit_epoch_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime: @<timestamp> epoch sub-routine ---");
    emitter.label("__rt_strtotime_epoch_entry");

    emitter.instruction("ldr x1, [sp, #48]");                                   // reload trimmed input pointer
    emitter.instruction("ldr x2, [sp, #56]");                                   // reload trimmed input length
    emitter.instruction("mov x3, #1");                                          // index = 1 (skip the '@')
    emitter.instruction("mov x0, #0");                                          // unsigned magnitude = 0
    emitter.instruction("mov x4, #0");                                          // negative flag = 0
    emitter.instruction("mov x5, #0");                                          // digit count = 0

    // -- optional leading '-' (PHP rejects a '+' here) --
    emitter.instruction("cmp x3, x2");                                          // anything after the '@'?
    emitter.instruction("b.ge __rt_strtotime_fail");                            // "@" alone is invalid
    emitter.instruction("ldrb w9, [x1, x3]");                                   // load first char after '@'
    emitter.instruction("cmp w9, #45");                                         // '-' ?
    emitter.instruction("b.ne __rt_strtotime_epoch_loop");                      // no sign → begin digit scan
    emitter.instruction("mov x4, #1");                                          // remember the negative sign
    emitter.instruction("add x3, x3, #1");                                      // consume the '-'

    // -- accumulate decimal digits into the unsigned magnitude --
    emitter.label("__rt_strtotime_epoch_loop");
    emitter.instruction("cmp x3, x2");                                          // reached end of input?
    emitter.instruction("b.ge __rt_strtotime_epoch_digits_done");               // done scanning digits
    emitter.instruction("ldrb w9, [x1, x3]");                                   // load current char
    emitter.instruction("sub w9, w9, #48");                                     // convert ASCII to digit value
    emitter.instruction("cmp w9, #9");                                          // is it a decimal digit?
    emitter.instruction("b.hi __rt_strtotime_epoch_digits_done");               // first non-digit ends the number
    emitter.instruction("mov x10, #10");                                        // decimal base
    emitter.instruction("umulh x11, x0, x10");                                  // high half of magnitude * 10
    emitter.instruction("cbnz x11, __rt_strtotime_fail");                       // magnitude * 10 overflows 64 bits → out of range
    emitter.instruction("mul x0, x0, x10");                                     // shift magnitude left one decimal place
    emitter.instruction("adds x0, x0, x9");                                     // add the new digit
    emitter.instruction("b.cs __rt_strtotime_fail");                            // unsigned carry → out of range
    emitter.instruction("add x5, x5, #1");                                      // count the digit
    emitter.instruction("add x3, x3, #1");                                      // advance to the next char
    emitter.instruction("b __rt_strtotime_epoch_loop");                         // continue scanning

    // -- range check and sign: magnitude <= i64::MAX, or <= 2^63 when negative --
    emitter.label("__rt_strtotime_epoch_digits_done");
    emitter.instruction("cbz x5, __rt_strtotime_fail");                         // no digits parsed → invalid
    emitter.instruction("mov x10, #-1");                                        // all bits set
    emitter.instruction("lsr x10, x10, #1");                                    // x10 = i64::MAX
    emitter.instruction("add x10, x10, x4");                                    // a negative value may reach 2^63
    emitter.instruction("cmp x0, x10");                                         // magnitude within the signed range?
    emitter.instruction("b.hi __rt_strtotime_fail");                            // PHP reports an out-of-range number as false
    emitter.instruction("cbz x4, __rt_strtotime_epoch_signed");                 // positive → magnitude is the value
    emitter.instruction("neg x0, x0");                                          // apply the negative sign (2^63 wraps to i64::MIN)
    emitter.label("__rt_strtotime_epoch_signed");
    emitter.instruction("str x0, [sp, #80]");                                   // keep the timestamp across the tail checks

    // -- optional fraction: '.' followed by 1 to 6 digits --
    emitter.instruction("cmp x3, x2");                                          // any input left after the digits?
    emitter.instruction("b.ge __rt_strtotime_epoch_ok");                        // no → plain @<digits>
    emitter.instruction("ldrb w9, [x1, x3]");                                   // load the char after the digits
    emitter.instruction("cmp w9, #46");                                         // '.' starts a fraction?
    emitter.instruction("b.ne __rt_strtotime_epoch_tail");                      // no → timezone tail
    emitter.instruction("add x3, x3, #1");                                      // consume the '.'
    emitter.instruction("mov x5, #0");                                          // fraction digit count = 0
    emitter.instruction("mov x6, #0");                                          // non-zero fraction digits seen = 0
    emitter.label("__rt_strtotime_epoch_frac_loop");
    emitter.instruction("cmp x3, x2");                                          // reached end of input?
    emitter.instruction("b.ge __rt_strtotime_epoch_frac_done");                 // done scanning the fraction
    emitter.instruction("ldrb w9, [x1, x3]");                                   // load fraction char
    emitter.instruction("sub w9, w9, #48");                                     // convert ASCII to digit value
    emitter.instruction("cmp w9, #9");                                          // is it a decimal digit?
    emitter.instruction("b.hi __rt_strtotime_epoch_frac_done");                 // first non-digit ends the fraction
    emitter.instruction("orr x6, x6, x9");                                      // remember any non-zero fraction digit
    emitter.instruction("add x5, x5, #1");                                      // count the fraction digit
    emitter.instruction("add x3, x3, #1");                                      // advance to the next char
    emitter.instruction("b __rt_strtotime_epoch_frac_loop");                    // continue scanning
    emitter.label("__rt_strtotime_epoch_frac_done");
    emitter.instruction("cbz x5, __rt_strtotime_fail");                         // "@123." has no fraction digits → invalid
    emitter.instruction(&format!("cmp x5, #{MAX_FRACTION_DIGITS}"));            // more than microsecond precision?
    emitter.instruction("b.hi __rt_strtotime_fail");                            // PHP rejects a 7+ digit fraction
    emitter.instruction("cbz x4, __rt_strtotime_epoch_tail");                   // a positive fraction just truncates
    emitter.instruction("cbz x6, __rt_strtotime_epoch_tail");                   // an all-zero fraction changes nothing
    emitter.instruction("ldr x0, [sp, #80]");                                   // reload the negative timestamp
    // -- floor: wraps below i64::MIN exactly as PHP/timelib does (see the module docs) --
    emitter.instruction("sub x0, x0, #1");                                      // floor: @-5.5 is one second before -5
    emitter.instruction("str x0, [sp, #80]");                                   // keep the floored timestamp

    // -- tail: separators, one optional timezone token, separators, end of input --
    emitter.label("__rt_strtotime_epoch_tail");
    emitter.instruction("cmp x3, x2");                                          // reached end of input?
    emitter.instruction("b.ge __rt_strtotime_epoch_ok");                        // only separators followed the number
    emitter.instruction("ldrb w9, [x1, x3]");                                   // load tail char
    emit_arm64_branch_if_separator(emitter, "w9", "__rt_strtotime_epoch_tail_skip");
    emitter.instruction("cmp w9, #40");                                         // '(' opens a parenthesized zone?
    emitter.instruction("b.ne __rt_strtotime_epoch_token");                     // no → scan the zone letters directly
    emitter.instruction("add x3, x3, #1");                                      // consume the '('
    emitter.instruction("b __rt_strtotime_epoch_token");                        // scan the zone letters
    emitter.label("__rt_strtotime_epoch_tail_skip");
    emitter.instruction("add x3, x3, #1");                                      // skip the separator
    emitter.instruction("b __rt_strtotime_epoch_tail");                         // keep scanning the tail

    emitter.label("__rt_strtotime_epoch_token");
    emitter.instruction("mov x7, x3");                                          // token start
    emitter.label("__rt_strtotime_epoch_token_loop");
    emitter.instruction("cmp x3, x2");                                          // reached end of input?
    emitter.instruction("b.ge __rt_strtotime_epoch_token_end");                 // token ends with the input
    emitter.instruction("ldrb w9, [x1, x3]");                                   // load token char
    emitter.instruction("orr w10, w9, #32");                                    // fold ASCII letters to lowercase
    emitter.instruction("sub w10, w10, #97");                                   // offset from 'a'
    emitter.instruction("cmp w10, #25");                                        // is it an ASCII letter?
    emitter.instruction("b.hi __rt_strtotime_epoch_token_end");                 // first non-letter ends the token
    emitter.instruction("add x3, x3, #1");                                      // advance over the letter
    emitter.instruction("b __rt_strtotime_epoch_token_loop");                   // continue scanning letters
    emitter.label("__rt_strtotime_epoch_token_end");
    emitter.instruction("sub x10, x3, x7");                                     // token length
    emitter.instruction("cbz x10, __rt_strtotime_fail");                        // no letters → not a timezone token
    emitter.instruction(&format!("cmp x10, #{MAX_TZ_LETTERS}"));                // longer than a zone abbreviation?
    emitter.instruction("b.hi __rt_strtotime_fail");                            // PHP rejects 7+ letters here
    emitter.instruction("cmp x3, x2");                                          // input left after the letters?
    emitter.instruction("b.ge __rt_strtotime_epoch_keyword");                   // no → check the token
    emitter.instruction("ldrb w9, [x1, x3]");                                   // load the char after the letters
    emitter.instruction("cmp w9, #41");                                         // ')' closes a parenthesized zone?
    emitter.instruction("b.ne __rt_strtotime_epoch_keyword");                   // no → check the token
    emitter.instruction("add x3, x3, #1");                                      // consume the ')'

    // -- a strtotime keyword (weekday, month, noon, ago, ...) is not a timezone --
    emitter.label("__rt_strtotime_epoch_keyword");
    emitter.instruction("str x3, [sp, #88]");                                   // keep the cursor after the token
    emitter.instruction("str x10, [sp, #96]");                                  // keep the token length
    emitter.instruction("add x3, x1, x7");                                      // lowercase from the token start
    emitter.instruction("add x4, x3, x10");                                     // up to the token end
    emitter.instruction("bl __rt_strtotime_lc_cursor");                         // lowercase the token into [sp+64..79]
    emitter.instruction("add x6, sp, #64");                                     // candidate = lowercased token
    abi::emit_symbol_address(emitter, "x7", "_strtotime_keyword_tab");
    emitter.instruction("ldr x8, [sp, #96]");                                   // available bytes = token length
    emitter.instruction("bl __rt_strtotime_match_word");                        // x10 = consumed bytes (0 when no keyword)
    emitter.instruction("cbnz x10, __rt_strtotime_fail");                       // a keyword would change PHP's result
    emitter.instruction("ldr x1, [sp, #48]");                                   // reload trimmed input pointer
    emitter.instruction("ldr x2, [sp, #56]");                                   // reload trimmed input length
    emitter.instruction("ldr x3, [sp, #88]");                                   // reload the cursor after the token

    // -- only separators may follow the timezone token --
    emitter.label("__rt_strtotime_epoch_after_token");
    emitter.instruction("cmp x3, x2");                                          // reached end of input?
    emitter.instruction("b.ge __rt_strtotime_epoch_ok");                        // the tail was one timezone token
    emitter.instruction("ldrb w9, [x1, x3]");                                   // load trailing char
    emit_arm64_branch_if_separator(emitter, "w9", "__rt_strtotime_epoch_after_token_skip");
    emitter.instruction("b __rt_strtotime_fail");                               // a second token (or junk) → invalid
    emitter.label("__rt_strtotime_epoch_after_token_skip");
    emitter.instruction("add x3, x3, #1");                                      // skip the separator
    emitter.instruction("b __rt_strtotime_epoch_after_token");                  // keep scanning

    emitter.label("__rt_strtotime_epoch_ok");
    emitter.instruction("ldr x0, [sp, #80]");                                   // load the literal timestamp
    emitter.instruction("b __rt_strtotime_ret");                                // return it
}

/// Emits x86_64 (Linux) assembly for the `@<timestamp>` epoch parser.
///
/// Entry label: `__rt_strtotime_epoch_entry_linux_x86_64`. Mirrors the ARM64 logic using SysV
/// register/stack conventions: trimmed ptr at `[rbp-80]`, trimmed len at `[rbp-72]`, scratch at
/// `[rbp-48]` (timestamp), `[rbp-40]` (cursor after the token) and `[rbp-32]` (token length).
fn emit_epoch_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime: @<timestamp> epoch sub-routine ---");
    emitter.label("__rt_strtotime_epoch_entry_linux_x86_64");

    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // reload trimmed input pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // reload trimmed input length
    emitter.instruction("mov rcx, 1");                                          // index = 1 (skip the '@')
    emitter.instruction("xor eax, eax");                                        // unsigned magnitude = 0
    emitter.instruction("xor r8d, r8d");                                        // negative flag = 0
    emitter.instruction("xor r9d, r9d");                                        // digit count = 0

    // -- optional leading '-' (PHP rejects a '+' here) --
    emitter.instruction("cmp rcx, rsi");                                        // anything after the '@'?
    emitter.instruction("jae __rt_strtotime_fail_linux_x86_64");                // "@" alone is invalid
    emitter.instruction("movzx edx, BYTE PTR [rdi + rcx]");                     // load first char after '@'
    emitter.instruction("cmp edx, 45");                                         // '-' ?
    emitter.instruction("jne __rt_strtotime_epoch_loop_linux_x86_64");          // no sign → begin digit scan
    emitter.instruction("mov r8d, 1");                                          // remember the negative sign
    emitter.instruction("add rcx, 1");                                          // consume the '-'

    // -- accumulate decimal digits into the unsigned magnitude --
    emitter.label("__rt_strtotime_epoch_loop_linux_x86_64");
    emitter.instruction("cmp rcx, rsi");                                        // reached end of input?
    emitter.instruction("jae __rt_strtotime_epoch_digits_done_linux_x86_64");   // done scanning digits
    emitter.instruction("movzx r10d, BYTE PTR [rdi + rcx]");                    // load current char
    emitter.instruction("sub r10d, 48");                                        // convert ASCII to digit value
    emitter.instruction("cmp r10d, 9");                                         // is it a decimal digit?
    emitter.instruction("ja __rt_strtotime_epoch_digits_done_linux_x86_64");    // first non-digit ends the number
    emitter.instruction("mov r11d, 10");                                        // decimal base
    emitter.instruction("mul r11");                                             // rdx:rax = magnitude * 10
    emitter.instruction("jc __rt_strtotime_fail_linux_x86_64");                 // high half set → out of range
    emitter.instruction("add rax, r10");                                        // add the new digit
    emitter.instruction("jc __rt_strtotime_fail_linux_x86_64");                 // unsigned carry → out of range
    emitter.instruction("add r9, 1");                                           // count the digit
    emitter.instruction("add rcx, 1");                                          // advance to the next char
    emitter.instruction("jmp __rt_strtotime_epoch_loop_linux_x86_64");          // continue scanning

    // -- range check and sign: magnitude <= i64::MAX, or <= 2^63 when negative --
    emitter.label("__rt_strtotime_epoch_digits_done_linux_x86_64");
    emitter.instruction("test r9, r9");                                         // any digits parsed?
    emitter.instruction("jz __rt_strtotime_fail_linux_x86_64");                 // no digits → invalid
    emitter.instruction("mov r10, -1");                                         // all bits set
    emitter.instruction("shr r10, 1");                                          // r10 = i64::MAX
    emitter.instruction("add r10, r8");                                         // a negative value may reach 2^63
    emitter.instruction("cmp rax, r10");                                        // magnitude within the signed range?
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // PHP reports an out-of-range number as false
    emitter.instruction("test r8, r8");                                         // negative sign?
    emitter.instruction("jz __rt_strtotime_epoch_signed_linux_x86_64");         // positive → magnitude is the value
    emitter.instruction("neg rax");                                             // apply the negative sign (2^63 wraps to i64::MIN)
    emitter.label("__rt_strtotime_epoch_signed_linux_x86_64");
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // keep the timestamp across the tail checks

    // -- optional fraction: '.' followed by 1 to 6 digits --
    emitter.instruction("cmp rcx, rsi");                                        // any input left after the digits?
    emitter.instruction("jae __rt_strtotime_epoch_ok_linux_x86_64");            // no → plain @<digits>
    emitter.instruction("movzx edx, BYTE PTR [rdi + rcx]");                     // load the char after the digits
    emitter.instruction("cmp edx, 46");                                         // '.' starts a fraction?
    emitter.instruction("jne __rt_strtotime_epoch_tail_linux_x86_64");          // no → timezone tail
    emitter.instruction("add rcx, 1");                                          // consume the '.'
    emitter.instruction("xor r9d, r9d");                                        // fraction digit count = 0
    emitter.instruction("xor r11d, r11d");                                      // non-zero fraction digits seen = 0
    emitter.label("__rt_strtotime_epoch_frac_loop_linux_x86_64");
    emitter.instruction("cmp rcx, rsi");                                        // reached end of input?
    emitter.instruction("jae __rt_strtotime_epoch_frac_done_linux_x86_64");     // done scanning the fraction
    emitter.instruction("movzx edx, BYTE PTR [rdi + rcx]");                     // load fraction char
    emitter.instruction("sub edx, 48");                                         // convert ASCII to digit value
    emitter.instruction("cmp edx, 9");                                          // is it a decimal digit?
    emitter.instruction("ja __rt_strtotime_epoch_frac_done_linux_x86_64");      // first non-digit ends the fraction
    emitter.instruction("or r11, rdx");                                         // remember any non-zero fraction digit
    emitter.instruction("add r9, 1");                                           // count the fraction digit
    emitter.instruction("add rcx, 1");                                          // advance to the next char
    emitter.instruction("jmp __rt_strtotime_epoch_frac_loop_linux_x86_64");     // continue scanning
    emitter.label("__rt_strtotime_epoch_frac_done_linux_x86_64");
    emitter.instruction("test r9, r9");                                         // any fraction digits?
    emitter.instruction("jz __rt_strtotime_fail_linux_x86_64");                 // "@123." has no fraction digits → invalid
    emitter.instruction(&format!("cmp r9, {MAX_FRACTION_DIGITS}"));             // more than microsecond precision?
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // PHP rejects a 7+ digit fraction
    emitter.instruction("test r8, r8");                                         // negative timestamp?
    emitter.instruction("jz __rt_strtotime_epoch_tail_linux_x86_64");           // a positive fraction just truncates
    emitter.instruction("test r11, r11");                                       // any non-zero fraction digit?
    emitter.instruction("jz __rt_strtotime_epoch_tail_linux_x86_64");           // an all-zero fraction changes nothing
    // -- floor: wraps below i64::MIN exactly as PHP/timelib does (see the module docs) --
    emitter.instruction("sub QWORD PTR [rbp - 48], 1");                         // floor: @-5.5 is one second before -5

    // -- tail: separators, one optional timezone token, separators, end of input --
    emitter.label("__rt_strtotime_epoch_tail_linux_x86_64");
    emitter.instruction("cmp rcx, rsi");                                        // reached end of input?
    emitter.instruction("jae __rt_strtotime_epoch_ok_linux_x86_64");            // only separators followed the number
    emitter.instruction("movzx edx, BYTE PTR [rdi + rcx]");                     // load tail char
    emit_x86_64_branch_if_separator(emitter, "edx", "__rt_strtotime_epoch_tail_skip_linux_x86_64");
    emitter.instruction("cmp edx, 40");                                         // '(' opens a parenthesized zone?
    emitter.instruction("jne __rt_strtotime_epoch_token_linux_x86_64");         // no → scan the zone letters directly
    emitter.instruction("add rcx, 1");                                          // consume the '('
    emitter.instruction("jmp __rt_strtotime_epoch_token_linux_x86_64");         // scan the zone letters
    emitter.label("__rt_strtotime_epoch_tail_skip_linux_x86_64");
    emitter.instruction("add rcx, 1");                                          // skip the separator
    emitter.instruction("jmp __rt_strtotime_epoch_tail_linux_x86_64");          // keep scanning the tail

    emitter.label("__rt_strtotime_epoch_token_linux_x86_64");
    emitter.instruction("mov r8, rcx");                                         // token start
    emitter.label("__rt_strtotime_epoch_token_loop_linux_x86_64");
    emitter.instruction("cmp rcx, rsi");                                        // reached end of input?
    emitter.instruction("jae __rt_strtotime_epoch_token_end_linux_x86_64");     // token ends with the input
    emitter.instruction("movzx edx, BYTE PTR [rdi + rcx]");                     // load token char
    emitter.instruction("or edx, 32");                                          // fold ASCII letters to lowercase
    emitter.instruction("sub edx, 97");                                         // offset from 'a'
    emitter.instruction("cmp edx, 25");                                         // is it an ASCII letter?
    emitter.instruction("ja __rt_strtotime_epoch_token_end_linux_x86_64");      // first non-letter ends the token
    emitter.instruction("add rcx, 1");                                          // advance over the letter
    emitter.instruction("jmp __rt_strtotime_epoch_token_loop_linux_x86_64");    // continue scanning letters
    emitter.label("__rt_strtotime_epoch_token_end_linux_x86_64");
    emitter.instruction("mov r9, rcx");                                         // copy the token end
    emitter.instruction("sub r9, r8");                                          // token length
    emitter.instruction("jz __rt_strtotime_fail_linux_x86_64");                 // no letters → not a timezone token
    emitter.instruction(&format!("cmp r9, {MAX_TZ_LETTERS}"));                  // longer than a zone abbreviation?
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // PHP rejects 7+ letters here
    emitter.instruction("cmp rcx, rsi");                                        // input left after the letters?
    emitter.instruction("jae __rt_strtotime_epoch_keyword_linux_x86_64");       // no → check the token
    emitter.instruction("movzx edx, BYTE PTR [rdi + rcx]");                     // load the char after the letters
    emitter.instruction("cmp edx, 41");                                         // ')' closes a parenthesized zone?
    emitter.instruction("jne __rt_strtotime_epoch_keyword_linux_x86_64");       // no → check the token
    emitter.instruction("add rcx, 1");                                          // consume the ')'

    // -- a strtotime keyword (weekday, month, noon, ago, ...) is not a timezone --
    emitter.label("__rt_strtotime_epoch_keyword_linux_x86_64");
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // keep the cursor after the token
    emitter.instruction("mov QWORD PTR [rbp - 32], r9");                        // keep the token length
    emitter.instruction("lea rdi, [rdi + r8]");                                 // lowercase from the token start
    emitter.instruction("lea r10, [rdi + r9]");                                 // up to the token end
    emitter.instruction("call __rt_strtotime_lc_cursor_linux_x86_64");          // lowercase the token into [rbp-64..rbp-49]
    emitter.instruction("lea rdi, [rbp - 64]");                                 // candidate = lowercased token
    abi::emit_symbol_address(emitter, "rsi", "_strtotime_keyword_tab");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // available bytes = token length
    emitter.instruction("call __rt_strtotime_match_word_linux_x86_64");         // rax = consumed bytes (0 when no keyword)
    emitter.instruction("test rax, rax");                                       // did the token match a keyword?
    emitter.instruction("jnz __rt_strtotime_fail_linux_x86_64");                // a keyword would change PHP's result
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // reload trimmed input pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // reload trimmed input length
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the cursor after the token

    // -- only separators may follow the timezone token --
    emitter.label("__rt_strtotime_epoch_after_token_linux_x86_64");
    emitter.instruction("cmp rcx, rsi");                                        // reached end of input?
    emitter.instruction("jae __rt_strtotime_epoch_ok_linux_x86_64");            // the tail was one timezone token
    emitter.instruction("movzx edx, BYTE PTR [rdi + rcx]");                     // load trailing char
    emit_x86_64_branch_if_separator(emitter, "edx", "__rt_strtotime_epoch_after_token_skip_linux_x86_64");
    emitter.instruction("jmp __rt_strtotime_fail_linux_x86_64");                // a second token (or junk) → invalid
    emitter.label("__rt_strtotime_epoch_after_token_skip_linux_x86_64");
    emitter.instruction("add rcx, 1");                                          // skip the separator
    emitter.instruction("jmp __rt_strtotime_epoch_after_token_linux_x86_64");   // keep scanning

    emitter.label("__rt_strtotime_epoch_ok_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // load the literal timestamp
    emitter.instruction("jmp __rt_strtotime_ret_linux_x86_64");                 // return it
}
