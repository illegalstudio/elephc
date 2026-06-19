//! Purpose:
//! Emits the American `M/D/Y` slash-date parser sub-routine for the `__rt_strtotime` dispatcher.
//! Accepts `MM/DD/YYYY`, single-digit `M/D/Y`, 2-digit years, and an optional `HH:MM[:SS]` suffix.
//!
//! Called from:
//! - `crate::codegen::runtime::system::strtotime::mod::emit_strtotime()` via the dispatcher's
//!   digit branch when a `/` is seen at offset 1 or 2.
//!
//! Key details:
//! - Entry label `__rt_strtotime_slash_entry` (ARM64) / `_linux_x86_64` (x86_64); the dispatcher
//!   frame is set up, trimmed ptr at `[sp+48]`, trimmed len at `[sp+56]`.
//! - Fields are variable width: month/day are 1-2 digits, year is 1-4 digits. Month is validated
//!   `<= 12` and day `<= 31` (0 is allowed and normalized by `mktime`, matching PHP); a 2-digit
//!   year windows to 2000-2069 / 1970-1999 by value. The result `struct tm` is built at `[sp+0..47]`.
//! - A shared `__rt_strtotime_slash_uint` helper accumulates a capped (<=4 digit) unsigned field.
//! - All exits branch to the shared `__rt_strtotime_ret` / `__rt_strtotime_fail` epilogues.

use crate::codegen::{emit::Emitter, platform::Arch};

/// Dispatches to the architecture-specific slash-date parser.
pub(crate) fn emit_slash_date(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_slash_date_linux_x86_64(emitter);
        return;
    }

    emit_slash_date_arm64(emitter);
}

/// Emits ARM64 assembly for the `M/D/Y[ HH:MM[:SS]]` slash-date parser.
///
/// Entry label: `__rt_strtotime_slash_entry`. Parses month/day/year via the
/// `__rt_strtotime_slash_uint` helper, validates field widths and ranges, applies PHP's
/// 2-digit-year windowing, parses an optional space-separated time, then builds `struct tm`
/// and calls `mktime`. Exits via `__rt_strtotime_fail` or `__rt_strtotime_ret`.
fn emit_slash_date_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime: M/D/Y slash-date sub-routine ---");
    emitter.label("__rt_strtotime_slash_entry");

    emitter.instruction("ldr x1, [sp, #48]");                                   // reload trimmed input pointer
    emitter.instruction("ldr x2, [sp, #56]");                                   // reload trimmed input length
    emitter.instruction("cmp x2, #5");                                          // shortest slash date is "M/D/Y"
    emitter.instruction("b.lt __rt_strtotime_fail");                            // too short → fail
    emitter.instruction("mov x3, #0");                                          // scan index = 0

    // -- month: 1-2 digits, 0..12 --
    emitter.instruction("bl __rt_strtotime_slash_uint");                        // x5 = month, x6 = digit count
    emitter.instruction("cbz x6, __rt_strtotime_fail");                         // no month digits → fail
    emitter.instruction("cmp x6, #2");                                          // month is at most 2 digits
    emitter.instruction("b.gt __rt_strtotime_fail");                            // too many month digits → fail
    emitter.instruction("cmp x5, #12");                                         // PHP rejects month > 12
    emitter.instruction("b.gt __rt_strtotime_fail");                            // invalid month → fail
    emitter.instruction("sub w5, w5, #1");                                      // tm_mon = month - 1 (0 → -1, normalized later)
    emitter.instruction("str w5, [sp, #16]");                                   // store tm_mon
    emitter.instruction("bl __rt_strtotime_slash_expect_slash");                // require and consume a '/'

    // -- day: 1-2 digits, 0..31 --
    emitter.instruction("bl __rt_strtotime_slash_uint");                        // x5 = day, x6 = digit count
    emitter.instruction("cbz x6, __rt_strtotime_fail");                         // no day digits → fail
    emitter.instruction("cmp x6, #2");                                          // day is at most 2 digits
    emitter.instruction("b.gt __rt_strtotime_fail");                            // too many day digits → fail
    emitter.instruction("cmp x5, #31");                                         // PHP rejects day > 31
    emitter.instruction("b.gt __rt_strtotime_fail");                            // invalid day → fail
    emitter.instruction("str w5, [sp, #12]");                                   // store tm_mday (0 normalized by mktime)
    emitter.instruction("bl __rt_strtotime_slash_expect_slash");                // require and consume a '/'

    // -- year: 1-4 digits, with PHP 2-digit windowing --
    emitter.instruction("bl __rt_strtotime_slash_uint");                        // x5 = year, x6 = digit count
    emitter.instruction("cbz x6, __rt_strtotime_fail");                         // no year digits → fail
    emitter.instruction("cmp x5, #69");                                         // value <= 69 → 2000s
    emitter.instruction("b.gt __rt_strtotime_slash_year_window_1900");          // otherwise try the 1900s window
    emitter.instruction("add x5, x5, #2000");                                   // 0..69 → 2000..2069
    emitter.instruction("b __rt_strtotime_slash_year_done");                    // year resolved
    emitter.label("__rt_strtotime_slash_year_window_1900");
    emitter.instruction("cmp x5, #99");                                         // value 70..99 → 1900s
    emitter.instruction("b.gt __rt_strtotime_slash_year_done");                 // >= 100 → literal year
    emitter.instruction("add x5, x5, #1900");                                   // 70..99 → 1970..1999
    emitter.label("__rt_strtotime_slash_year_done");
    emitter.instruction("sub w5, w5, #1900");                                   // tm_year = year - 1900
    emitter.instruction("str w5, [sp, #20]");                                   // store tm_year

    // -- default time to midnight; optional " HH:MM[:SS]" suffix overrides --
    emitter.instruction("str wzr, [sp, #0]");                                   // tm_sec = 0
    emitter.instruction("str wzr, [sp, #4]");                                   // tm_min = 0
    emitter.instruction("str wzr, [sp, #8]");                                   // tm_hour = 0
    emitter.instruction("cmp x3, x2");                                          // any trailing characters?
    emitter.instruction("b.ge __rt_strtotime_slash_mktime");                    // none → midnight
    emitter.instruction("ldrb w9, [x1, x3]");                                   // load the separator after the year
    emitter.instruction("cmp w9, #32");                                         // must be a space before a time
    emitter.instruction("b.ne __rt_strtotime_fail");                            // trailing junk → fail
    emitter.instruction("add x3, x3, #1");                                      // consume the space

    // -- hour --
    emitter.instruction("bl __rt_strtotime_slash_uint");                        // x5 = hour, x6 = digit count
    emitter.instruction("cbz x6, __rt_strtotime_fail");                         // no hour digits → fail
    emitter.instruction("cmp x6, #2");                                          // hour is at most 2 digits
    emitter.instruction("b.gt __rt_strtotime_fail");                            // too many hour digits → fail
    emitter.instruction("str w5, [sp, #8]");                                    // store tm_hour
    emitter.instruction("bl __rt_strtotime_slash_expect_colon");                // require and consume a ':'

    // -- minute --
    emitter.instruction("bl __rt_strtotime_slash_uint");                        // x5 = minute, x6 = digit count
    emitter.instruction("cbz x6, __rt_strtotime_fail");                         // no minute digits → fail
    emitter.instruction("cmp x6, #2");                                          // minute is at most 2 digits
    emitter.instruction("b.gt __rt_strtotime_fail");                            // too many minute digits → fail
    emitter.instruction("str w5, [sp, #4]");                                    // store tm_min

    // -- optional :seconds --
    emitter.instruction("cmp x3, x2");                                          // any more characters?
    emitter.instruction("b.ge __rt_strtotime_slash_mktime");                    // no seconds → done
    emitter.instruction("ldrb w9, [x1, x3]");                                   // load the next separator
    emitter.instruction("cmp w9, #58");                                         // must be ':' for seconds
    emitter.instruction("b.ne __rt_strtotime_fail");                            // trailing junk → fail
    emitter.instruction("add x3, x3, #1");                                      // consume the ':'
    emitter.instruction("bl __rt_strtotime_slash_uint");                        // x5 = second, x6 = digit count
    emitter.instruction("cbz x6, __rt_strtotime_fail");                         // no second digits → fail
    emitter.instruction("cmp x6, #2");                                          // second is at most 2 digits
    emitter.instruction("b.gt __rt_strtotime_fail");                            // too many second digits → fail
    emitter.instruction("str w5, [sp, #0]");                                    // store tm_sec
    emitter.instruction("cmp x3, x2");                                          // must be fully consumed now
    emitter.instruction("b.lt __rt_strtotime_fail");                            // trailing junk after seconds → fail

    emitter.label("__rt_strtotime_slash_mktime");
    emitter.instruction("str wzr, [sp, #24]");                                  // tm_wday = 0 (mktime ignores)
    emitter.instruction("str wzr, [sp, #28]");                                  // tm_yday = 0 (mktime ignores)
    emitter.instruction("mov w9, #-1");                                         // tm_isdst sentinel
    emitter.instruction("str w9, [sp, #32]");                                   // tm_isdst = -1 (let mktime infer DST)
    emitter.instruction("mov x0, sp");                                          // x0 = &tm
    emitter.instruction("bl __rt_mktime_shifted");                              // mktime(&tm) → x0 = timestamp
    emitter.instruction("b __rt_strtotime_ret");                                // return through shared epilogue

    // -- helper: require and consume a '/' at the current index --
    emitter.label("__rt_strtotime_slash_expect_slash");
    emitter.instruction("cmp x3, x2");                                          // bounds check
    emitter.instruction("b.ge __rt_strtotime_fail");                            // ran out of input → fail
    emitter.instruction("ldrb w9, [x1, x3]");                                   // load the candidate separator
    emitter.instruction("cmp w9, #47");                                         // require '/'
    emitter.instruction("b.ne __rt_strtotime_fail");                            // not a slash → fail
    emitter.instruction("add x3, x3, #1");                                      // consume it
    emitter.instruction("ret");                                                 // return to the parser

    // -- helper: require and consume a ':' at the current index --
    emitter.label("__rt_strtotime_slash_expect_colon");
    emitter.instruction("cmp x3, x2");                                          // bounds check
    emitter.instruction("b.ge __rt_strtotime_fail");                            // ran out of input → fail
    emitter.instruction("ldrb w9, [x1, x3]");                                   // load the candidate separator
    emitter.instruction("cmp w9, #58");                                         // require ':'
    emitter.instruction("b.ne __rt_strtotime_fail");                            // not a colon → fail
    emitter.instruction("add x3, x3, #1");                                      // consume it
    emitter.instruction("ret");                                                 // return to the parser

    // -- helper: parse a capped unsigned field from [x1+x3], advancing x3 --
    emitter.label("__rt_strtotime_slash_uint");
    emitter.instruction("mov x5, #0");                                          // accumulated value
    emitter.instruction("mov x6, #0");                                          // digit count
    emitter.label("__rt_strtotime_slash_uint_loop");
    emitter.instruction("cmp x3, x2");                                          // reached end of input?
    emitter.instruction("b.ge __rt_strtotime_slash_uint_done");                 // stop scanning
    emitter.instruction("ldrb w9, [x1, x3]");                                   // load current char
    emitter.instruction("sub w9, w9, #48");                                     // convert ASCII to digit value
    emitter.instruction("cmp w9, #9");                                          // is it a decimal digit?
    emitter.instruction("b.hi __rt_strtotime_slash_uint_done");                 // non-digit → stop
    emitter.instruction("mov x10, #10");                                        // decimal base
    emitter.instruction("mul x5, x5, x10");                                     // shift accumulator one place
    emitter.instruction("add x5, x5, x9");                                      // add the new digit
    emitter.instruction("add x6, x6, #1");                                      // count the digit
    emitter.instruction("add x3, x3, #1");                                      // advance the index
    emitter.instruction("cmp x6, #4");                                          // cap at 4 digits (year width)
    emitter.instruction("b.ge __rt_strtotime_slash_uint_done");                 // stop after 4 digits
    emitter.instruction("b __rt_strtotime_slash_uint_loop");                    // continue scanning
    emitter.label("__rt_strtotime_slash_uint_done");
    emitter.instruction("ret");                                                 // return value in x5, count in x6
}

/// Emits x86_64 (Linux) assembly for the `M/D/Y[ HH:MM[:SS]]` slash-date parser.
///
/// Entry label: `__rt_strtotime_slash_entry_linux_x86_64`. Mirrors the ARM64 logic using SysV
/// conventions: trimmed ptr at `[rsp+48]`, trimmed len at `[rsp+56]`, scan index in `rcx`,
/// field value in `rax`, digit count in `r9`. Shared helpers parse fields and separators.
fn emit_slash_date_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime: M/D/Y slash-date sub-routine ---");
    emitter.label("__rt_strtotime_slash_entry_linux_x86_64");

    emitter.instruction("mov rdi, QWORD PTR [rsp + 48]");                       // reload trimmed input pointer
    emitter.instruction("mov rsi, QWORD PTR [rsp + 56]");                       // reload trimmed input length
    emitter.instruction("cmp rsi, 5");                                          // shortest slash date is "M/D/Y"
    emitter.instruction("jb __rt_strtotime_fail_linux_x86_64");                 // too short → fail
    emitter.instruction("xor rcx, rcx");                                        // scan index = 0

    // -- month: 1-2 digits, 0..12 --
    emitter.instruction("call __rt_strtotime_slash_uint_linux_x86_64");         // rax = month, r9 = digit count
    emitter.instruction("test r9, r9");                                         // any month digits?
    emitter.instruction("jz __rt_strtotime_fail_linux_x86_64");                 // none → fail
    emitter.instruction("cmp r9, 2");                                           // month is at most 2 digits
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // too many digits → fail
    emitter.instruction("cmp rax, 12");                                         // PHP rejects month > 12
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // invalid month → fail
    emitter.instruction("sub eax, 1");                                          // tm_mon = month - 1
    emitter.instruction("mov DWORD PTR [rsp + 16], eax");                       // store tm_mon
    emitter.instruction("call __rt_strtotime_slash_expect_slash_linux_x86_64"); // require and consume '/'

    // -- day: 1-2 digits, 0..31 --
    emitter.instruction("call __rt_strtotime_slash_uint_linux_x86_64");         // rax = day, r9 = digit count
    emitter.instruction("test r9, r9");                                         // any day digits?
    emitter.instruction("jz __rt_strtotime_fail_linux_x86_64");                 // none → fail
    emitter.instruction("cmp r9, 2");                                           // day is at most 2 digits
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // too many digits → fail
    emitter.instruction("cmp rax, 31");                                         // PHP rejects day > 31
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // invalid day → fail
    emitter.instruction("mov DWORD PTR [rsp + 12], eax");                       // store tm_mday
    emitter.instruction("call __rt_strtotime_slash_expect_slash_linux_x86_64"); // require and consume '/'

    // -- year: 1-4 digits, with PHP 2-digit windowing --
    emitter.instruction("call __rt_strtotime_slash_uint_linux_x86_64");         // rax = year, r9 = digit count
    emitter.instruction("test r9, r9");                                         // any year digits?
    emitter.instruction("jz __rt_strtotime_fail_linux_x86_64");                 // none → fail
    emitter.instruction("cmp rax, 69");                                         // value <= 69 → 2000s
    emitter.instruction("ja __rt_strtotime_slash_year_window_1900_linux_x86_64"); // otherwise try 1900s
    emitter.instruction("add rax, 2000");                                       // 0..69 → 2000..2069
    emitter.instruction("jmp __rt_strtotime_slash_year_done_linux_x86_64");     // year resolved
    emitter.label("__rt_strtotime_slash_year_window_1900_linux_x86_64");
    emitter.instruction("cmp rax, 99");                                         // value 70..99 → 1900s
    emitter.instruction("ja __rt_strtotime_slash_year_done_linux_x86_64");      // >= 100 → literal year
    emitter.instruction("add rax, 1900");                                       // 70..99 → 1970..1999
    emitter.label("__rt_strtotime_slash_year_done_linux_x86_64");
    emitter.instruction("sub eax, 1900");                                       // tm_year = year - 1900
    emitter.instruction("mov DWORD PTR [rsp + 20], eax");                       // store tm_year

    // -- default time to midnight; optional " HH:MM[:SS]" suffix overrides --
    emitter.instruction("mov DWORD PTR [rsp + 0], 0");                          // tm_sec = 0
    emitter.instruction("mov DWORD PTR [rsp + 4], 0");                          // tm_min = 0
    emitter.instruction("mov DWORD PTR [rsp + 8], 0");                          // tm_hour = 0
    emitter.instruction("cmp rcx, rsi");                                        // any trailing characters?
    emitter.instruction("jae __rt_strtotime_slash_mktime_linux_x86_64");        // none → midnight
    emitter.instruction("movzx eax, BYTE PTR [rdi + rcx]");                     // load the separator after the year
    emitter.instruction("cmp eax, 32");                                         // must be a space before a time
    emitter.instruction("jne __rt_strtotime_fail_linux_x86_64");                // trailing junk → fail
    emitter.instruction("add rcx, 1");                                          // consume the space

    // -- hour --
    emitter.instruction("call __rt_strtotime_slash_uint_linux_x86_64");         // rax = hour, r9 = digit count
    emitter.instruction("test r9, r9");                                         // any hour digits?
    emitter.instruction("jz __rt_strtotime_fail_linux_x86_64");                 // none → fail
    emitter.instruction("cmp r9, 2");                                           // hour is at most 2 digits
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // too many digits → fail
    emitter.instruction("mov DWORD PTR [rsp + 8], eax");                        // store tm_hour
    emitter.instruction("call __rt_strtotime_slash_expect_colon_linux_x86_64"); // require and consume ':'

    // -- minute --
    emitter.instruction("call __rt_strtotime_slash_uint_linux_x86_64");         // rax = minute, r9 = digit count
    emitter.instruction("test r9, r9");                                         // any minute digits?
    emitter.instruction("jz __rt_strtotime_fail_linux_x86_64");                 // none → fail
    emitter.instruction("cmp r9, 2");                                           // minute is at most 2 digits
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // too many digits → fail
    emitter.instruction("mov DWORD PTR [rsp + 4], eax");                        // store tm_min

    // -- optional :seconds --
    emitter.instruction("cmp rcx, rsi");                                        // any more characters?
    emitter.instruction("jae __rt_strtotime_slash_mktime_linux_x86_64");        // no seconds → done
    emitter.instruction("movzx eax, BYTE PTR [rdi + rcx]");                     // load the next separator
    emitter.instruction("cmp eax, 58");                                         // must be ':' for seconds
    emitter.instruction("jne __rt_strtotime_fail_linux_x86_64");                // trailing junk → fail
    emitter.instruction("add rcx, 1");                                          // consume the ':'
    emitter.instruction("call __rt_strtotime_slash_uint_linux_x86_64");         // rax = second, r9 = digit count
    emitter.instruction("test r9, r9");                                         // any second digits?
    emitter.instruction("jz __rt_strtotime_fail_linux_x86_64");                 // none → fail
    emitter.instruction("cmp r9, 2");                                           // second is at most 2 digits
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // too many digits → fail
    emitter.instruction("mov DWORD PTR [rsp + 0], eax");                        // store tm_sec
    emitter.instruction("cmp rcx, rsi");                                        // must be fully consumed now
    emitter.instruction("jb __rt_strtotime_fail_linux_x86_64");                 // trailing junk after seconds → fail

    emitter.label("__rt_strtotime_slash_mktime_linux_x86_64");
    emitter.instruction("mov DWORD PTR [rsp + 24], 0");                         // tm_wday = 0 (mktime ignores)
    emitter.instruction("mov DWORD PTR [rsp + 28], 0");                         // tm_yday = 0 (mktime ignores)
    emitter.instruction("mov DWORD PTR [rsp + 32], -1");                        // tm_isdst = -1 (let mktime infer DST)
    emitter.instruction("mov rdi, rsp");                                        // rdi = &tm
    emitter.instruction("call __rt_mktime_shifted");                            // mktime(&tm) → rax = timestamp
    emitter.instruction("jmp __rt_strtotime_ret_linux_x86_64");                 // return through shared epilogue

    // -- helper: require and consume a '/' at the current index --
    emitter.label("__rt_strtotime_slash_expect_slash_linux_x86_64");
    emitter.instruction("cmp rcx, rsi");                                        // bounds check
    emitter.instruction("jae __rt_strtotime_fail_linux_x86_64");                // ran out of input → fail
    emitter.instruction("movzx eax, BYTE PTR [rdi + rcx]");                     // load the candidate separator
    emitter.instruction("cmp eax, 47");                                         // require '/'
    emitter.instruction("jne __rt_strtotime_fail_linux_x86_64");                // not a slash → fail
    emitter.instruction("add rcx, 1");                                          // consume it
    emitter.instruction("ret");                                                 // return to the parser

    // -- helper: require and consume a ':' at the current index --
    emitter.label("__rt_strtotime_slash_expect_colon_linux_x86_64");
    emitter.instruction("cmp rcx, rsi");                                        // bounds check
    emitter.instruction("jae __rt_strtotime_fail_linux_x86_64");                // ran out of input → fail
    emitter.instruction("movzx eax, BYTE PTR [rdi + rcx]");                     // load the candidate separator
    emitter.instruction("cmp eax, 58");                                         // require ':'
    emitter.instruction("jne __rt_strtotime_fail_linux_x86_64");                // not a colon → fail
    emitter.instruction("add rcx, 1");                                          // consume it
    emitter.instruction("ret");                                                 // return to the parser

    // -- helper: parse a capped unsigned field from [rdi+rcx], advancing rcx --
    emitter.label("__rt_strtotime_slash_uint_linux_x86_64");
    emitter.instruction("xor rax, rax");                                        // accumulated value
    emitter.instruction("xor r9, r9");                                          // digit count
    emitter.label("__rt_strtotime_slash_uint_loop_linux_x86_64");
    emitter.instruction("cmp rcx, rsi");                                        // reached end of input?
    emitter.instruction("jae __rt_strtotime_slash_uint_done_linux_x86_64");     // stop scanning
    emitter.instruction("movzx edx, BYTE PTR [rdi + rcx]");                     // load current char
    emitter.instruction("sub edx, 48");                                         // convert ASCII to digit value
    emitter.instruction("cmp edx, 9");                                          // is it a decimal digit?
    emitter.instruction("ja __rt_strtotime_slash_uint_done_linux_x86_64");      // non-digit → stop
    emitter.instruction("imul rax, rax, 10");                                   // shift accumulator one place
    emitter.instruction("add rax, rdx");                                        // add the new digit
    emitter.instruction("add r9, 1");                                           // count the digit
    emitter.instruction("add rcx, 1");                                          // advance the index
    emitter.instruction("cmp r9, 4");                                           // cap at 4 digits (year width)
    emitter.instruction("jae __rt_strtotime_slash_uint_done_linux_x86_64");     // stop after 4 digits
    emitter.instruction("jmp __rt_strtotime_slash_uint_loop_linux_x86_64");     // continue scanning
    emitter.label("__rt_strtotime_slash_uint_done_linux_x86_64");
    emitter.instruction("ret");                                                 // return value in rax, count in r9
}
