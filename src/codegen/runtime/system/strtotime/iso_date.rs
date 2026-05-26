//! Purpose:
//! Emits the ISO date/datetime parser sub-routine consumed by the `__rt_strtotime` dispatcher.
//! Accepts `YYYY-MM-DD`, `YYYY-MM-DD HH:MM[:SS]`, and `YYYY-MM-DDTHH:MM[:SS]` shapes.
//! Parses fixed-offset ASCII digits and builds a `struct tm` in the dispatcher-owned scratch slot.
//!
//! Called from:
//! - `crate::codegen::runtime::system::strtotime::mod::emit_strtotime()` via the dispatcher's first-byte digit branch.
//!
//! Key details:
//! - Entry label `__rt_strtotime_iso_entry` (ARM64) / `__rt_strtotime_iso_entry_linux_x86_64` (x86_64) expects the dispatcher frame already set up.
//! - Inputs come from `[sp+48]` (trimmed ptr) and `[sp+56]` (trimmed len); the result `struct tm` is built at `[sp+0..47]`.
//! - All exits branch to the shared `__rt_strtotime_ret` / `__rt_strtotime_fail` epilogues owned by the dispatcher (`mod.rs`).

use crate::codegen::{emit::Emitter, platform::Arch};

/// Dispatches to the architecture-specific ISO date parser.
/// Routes to `emit_iso_date_arm64` or `emit_iso_date_linux_x86_64` based on `emitter.target`.
/// Inputs: trimmed ptr from `[sp+48]`, trimmed len from `[sp+56]`.
/// Output: `struct tm` built at `[sp+0..47]` on the caller's frame.
/// Exits: branches to `__rt_strtotime_fail` or `__rt_strtotime_ret` owned by the dispatcher.
pub(crate) fn emit_iso_date(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_iso_date_linux_x86_64(emitter);
        return;
    }

    emit_iso_date_arm64(emitter);
}

/// Emits a compare-and-branch sequence that rejects `reg` unless it holds a decimal digit (0–9).
/// Used to validate each parsed ASCII digit before it is used in numeric assembly.
/// Injects `b.hi __rt_strtotime_fail` on failure.
fn emit_arm64_reject_unless_decimal_digit(emitter: &mut Emitter, reg: &str) {
    let cmp = format!("cmp {reg}, #9");
    emitter.instruction(&cmp);                                                  // ensure parsed byte is a decimal digit
    emitter.instruction("b.hi __rt_strtotime_fail");                            // reject malformed ISO numeric fields
}

/// Emits a compare-and-branch sequence that rejects `reg` unless it holds a decimal digit (0–9).
/// Used to validate each parsed ASCII digit before it is used in numeric assembly.
/// Injects `ja __rt_strtotime_fail_linux_x86_64` on failure.
fn emit_x86_64_reject_unless_decimal_digit(emitter: &mut Emitter, reg: &str) {
    let cmp = format!("cmp {reg}, 9");
    emitter.instruction(&cmp);                                                  // ensure parsed byte is a decimal digit
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // reject malformed ISO numeric fields
}

/// Emits ARM64 assembly for the ISO date/datetime parser sub-routine.
/// Entry label: `__rt_strtotime_iso_entry`.
/// Inputs: trimmed ptr at `[sp+48]`, trimmed len at `[sp+56]`.
/// Parses `YYYY-MM-DD` (10 bytes), `YYYY-MM-DD HH:MM` (16 bytes), and `YYYY-MM-DD HH:MM:SS` (19 bytes).
/// Accepts date/time separator as space, `T`, or `t`.
/// Builds `struct tm` at `[sp+0..47]`; fills tm_wday, tm_yday, tm_isdst before calling `mktime`.
/// Exits via `__rt_strtotime_fail` on parse error, or `__rt_strtotime_ret` after `mktime`.
/// Clobbers: x0–x12, w9–w11, lr (via `bl mktime`).
fn emit_iso_date_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime: ISO date sub-routine ---");
    emitter.label("__rt_strtotime_iso_entry");

    // -- reload trimmed ptr/len from dispatcher slots --
    emitter.instruction("ldr x1, [sp, #48]");                                   // reload trimmed input pointer
    emitter.instruction("ldr x2, [sp, #56]");                                   // reload trimmed input length

    // -- validate minimum length (10 for YYYY-MM-DD) --
    emitter.instruction("cmp x2, #10");                                         // need at least 10 chars
    emitter.instruction("b.lt __rt_strtotime_fail");                            // fail if too short

    // -- accept only exact supported ISO lengths --
    emitter.instruction("cmp x2, #10");                                         // check for YYYY-MM-DD
    emitter.instruction("b.eq __rt_strtotime_iso_validate_date");               // validate date-only shape
    emitter.instruction("cmp x2, #16");                                         // check for YYYY-MM-DD HH:MM
    emitter.instruction("b.eq __rt_strtotime_iso_validate_datetime");           // validate datetime without seconds
    emitter.instruction("cmp x2, #19");                                         // check for YYYY-MM-DD HH:MM:SS
    emitter.instruction("b.eq __rt_strtotime_iso_validate_datetime");           // validate full datetime shape
    emitter.instruction("b __rt_strtotime_fail");                               // reject trailing junk or partial times

    // -- validate separators before fixed-offset digit parsing --
    emitter.label("__rt_strtotime_iso_validate_datetime");
    emitter.instruction("ldrb w9, [x1, #10]");                                  // load date/time separator
    emitter.instruction("cmp w9, #32");                                         // accept a space separator
    emitter.instruction("b.eq __rt_strtotime_iso_datetime_separator_ok");       // continue after space separator
    emitter.instruction("cmp w9, #84");                                         // accept uppercase ISO T separator
    emitter.instruction("b.eq __rt_strtotime_iso_datetime_separator_ok");       // continue after uppercase T separator
    emitter.instruction("cmp w9, #116");                                        // accept lowercase ISO t separator
    emitter.instruction("b.ne __rt_strtotime_fail");                            // reject unsupported date/time separator
    emitter.label("__rt_strtotime_iso_datetime_separator_ok");
    emitter.instruction("ldrb w9, [x1, #13]");                                  // load hour/minute separator
    emitter.instruction("cmp w9, #58");                                         // require ':' after the hour
    emitter.instruction("b.ne __rt_strtotime_fail");                            // reject malformed hour/minute separator
    emitter.instruction("cmp x2, #19");                                         // full datetime needs a second separator
    emitter.instruction("b.ne __rt_strtotime_iso_validate_date");               // no seconds separator in HH:MM form
    emitter.instruction("ldrb w9, [x1, #16]");                                  // load minute/second separator
    emitter.instruction("cmp w9, #58");                                         // require ':' after the minute
    emitter.instruction("b.ne __rt_strtotime_fail");                            // reject malformed minute/second separator

    emitter.label("__rt_strtotime_iso_validate_date");
    emitter.instruction("ldrb w9, [x1, #4]");                                   // load first date separator
    emitter.instruction("cmp w9, #45");                                         // require '-' after the year
    emitter.instruction("b.ne __rt_strtotime_fail");                            // reject malformed year/month separator
    emitter.instruction("ldrb w9, [x1, #7]");                                   // load second date separator
    emitter.instruction("cmp w9, #45");                                         // require '-' after the month
    emitter.instruction("b.ne __rt_strtotime_fail");                            // reject malformed month/day separator

    // -- parse YYYY (4 digits at offset 0) --
    emitter.instruction("ldrb w9, [x1, #0]");                                   // load 1st year digit
    emitter.instruction("sub w9, w9, #48");                                     // convert from ASCII
    emit_arm64_reject_unless_decimal_digit(emitter, "w9");
    emitter.instruction("mov w10, #1000");                                      // multiplier for thousands
    emitter.instruction("mul w9, w9, w10");                                     // thousands place
    emitter.instruction("ldrb w10, [x1, #1]");                                  // load 2nd year digit
    emitter.instruction("sub w10, w10, #48");                                   // convert from ASCII
    emit_arm64_reject_unless_decimal_digit(emitter, "w10");
    emitter.instruction("mov w11, #100");                                       // multiplier for hundreds
    emitter.instruction("mul w10, w10, w11");                                   // hundreds place
    emitter.instruction("add w9, w9, w10");                                     // accumulate
    emitter.instruction("ldrb w10, [x1, #2]");                                  // load 3rd year digit
    emitter.instruction("sub w10, w10, #48");                                   // convert from ASCII
    emit_arm64_reject_unless_decimal_digit(emitter, "w10");
    emitter.instruction("mov w11, #10");                                        // multiplier for tens
    emitter.instruction("mul w10, w10, w11");                                   // tens place
    emitter.instruction("add w9, w9, w10");                                     // accumulate
    emitter.instruction("ldrb w10, [x1, #3]");                                  // load 4th year digit
    emitter.instruction("sub w10, w10, #48");                                   // convert from ASCII
    emit_arm64_reject_unless_decimal_digit(emitter, "w10");
    emitter.instruction("add w9, w9, w10");                                     // w9 = year (e.g. 2024)
    emitter.instruction("mov w10, #1900");                                      // year base for struct tm
    emitter.instruction("sub w9, w9, w10");                                     // tm_year = year - 1900
    emitter.instruction("str w9, [sp, #20]");                                   // store tm_year

    // -- parse MM (2 digits at offset 5) --
    emitter.instruction("ldrb w9, [x1, #5]");                                   // load 1st month digit
    emitter.instruction("sub w9, w9, #48");                                     // convert from ASCII
    emit_arm64_reject_unless_decimal_digit(emitter, "w9");
    emitter.instruction("mov w10, #10");                                        // multiplier
    emitter.instruction("mul w9, w9, w10");                                     // tens place
    emitter.instruction("ldrb w10, [x1, #6]");                                  // load 2nd month digit
    emitter.instruction("sub w10, w10, #48");                                   // convert from ASCII
    emit_arm64_reject_unless_decimal_digit(emitter, "w10");
    emitter.instruction("add w9, w9, w10");                                     // w9 = month (1-12)
    emitter.instruction("sub w9, w9, #1");                                      // tm_mon = month - 1 (0-based)
    emitter.instruction("str w9, [sp, #16]");                                   // store tm_mon

    // -- parse DD (2 digits at offset 8) --
    emitter.instruction("ldrb w9, [x1, #8]");                                   // load 1st day digit
    emitter.instruction("sub w9, w9, #48");                                     // convert from ASCII
    emit_arm64_reject_unless_decimal_digit(emitter, "w9");
    emitter.instruction("mov w10, #10");                                        // multiplier
    emitter.instruction("mul w9, w9, w10");                                     // tens place
    emitter.instruction("ldrb w10, [x1, #9]");                                  // load 2nd day digit
    emitter.instruction("sub w10, w10, #48");                                   // convert from ASCII
    emit_arm64_reject_unless_decimal_digit(emitter, "w10");
    emitter.instruction("add w9, w9, w10");                                     // w9 = day
    emitter.instruction("str w9, [sp, #12]");                                   // store tm_mday

    // -- check if time component exists (length >= 16 for "YYYY-MM-DD HH:MM") --
    emitter.instruction("cmp x2, #16");                                         // check for hour/minute datetime
    emitter.instruction("b.lt __rt_strtotime_iso_notime");                      // no time component

    // -- parse HH (2 digits at offset 11) --
    emitter.instruction("ldrb w9, [x1, #11]");                                  // load 1st hour digit
    emitter.instruction("sub w9, w9, #48");                                     // convert from ASCII
    emit_arm64_reject_unless_decimal_digit(emitter, "w9");
    emitter.instruction("mov w10, #10");                                        // multiplier
    emitter.instruction("mul w9, w9, w10");                                     // tens place
    emitter.instruction("ldrb w10, [x1, #12]");                                 // load 2nd hour digit
    emitter.instruction("sub w10, w10, #48");                                   // convert from ASCII
    emit_arm64_reject_unless_decimal_digit(emitter, "w10");
    emitter.instruction("add w9, w9, w10");                                     // w9 = hour
    emitter.instruction("str w9, [sp, #8]");                                    // store tm_hour

    // -- parse MM (2 digits at offset 14) --
    emitter.instruction("ldrb w9, [x1, #14]");                                  // load 1st minute digit
    emitter.instruction("sub w9, w9, #48");                                     // convert from ASCII
    emit_arm64_reject_unless_decimal_digit(emitter, "w9");
    emitter.instruction("mov w10, #10");                                        // multiplier
    emitter.instruction("mul w9, w9, w10");                                     // tens place
    emitter.instruction("ldrb w10, [x1, #15]");                                 // load 2nd minute digit
    emitter.instruction("sub w10, w10, #48");                                   // convert from ASCII
    emit_arm64_reject_unless_decimal_digit(emitter, "w10");
    emitter.instruction("add w9, w9, w10");                                     // w9 = minute
    emitter.instruction("str w9, [sp, #4]");                                    // store tm_min
    emitter.instruction("cmp x2, #19");                                         // full datetime includes seconds?
    emitter.instruction("b.lt __rt_strtotime_iso_no_seconds");                  // partial datetime defaults seconds to zero

    // -- parse SS (2 digits at offset 17) --
    emitter.instruction("ldrb w9, [x1, #17]");                                  // load 1st second digit
    emitter.instruction("sub w9, w9, #48");                                     // convert from ASCII
    emit_arm64_reject_unless_decimal_digit(emitter, "w9");
    emitter.instruction("mov w10, #10");                                        // multiplier
    emitter.instruction("mul w9, w9, w10");                                     // tens place
    emitter.instruction("ldrb w10, [x1, #18]");                                 // load 2nd second digit
    emitter.instruction("sub w10, w10, #48");                                   // convert from ASCII
    emit_arm64_reject_unless_decimal_digit(emitter, "w10");
    emitter.instruction("add w9, w9, w10");                                     // w9 = second
    emitter.instruction("str w9, [sp, #0]");                                    // store tm_sec
    emitter.instruction("b __rt_strtotime_iso_mktime");                         // proceed to mktime

    emitter.label("__rt_strtotime_iso_no_seconds");
    emitter.instruction("str wzr, [sp, #0]");                                   // tm_sec = 0 for YYYY-MM-DD HH:MM
    emitter.instruction("b __rt_strtotime_iso_mktime");                         // proceed to mktime

    // -- no time component, default to 00:00:00 --
    emitter.label("__rt_strtotime_iso_notime");
    emitter.instruction("str wzr, [sp, #0]");                                   // tm_sec = 0
    emitter.instruction("str wzr, [sp, #4]");                                   // tm_min = 0
    emitter.instruction("str wzr, [sp, #8]");                                   // tm_hour = 0

    // -- fill remaining tm fields and call mktime --
    emitter.label("__rt_strtotime_iso_mktime");
    emitter.instruction("str wzr, [sp, #24]");                                  // tm_wday = 0
    emitter.instruction("str wzr, [sp, #28]");                                  // tm_yday = 0
    emitter.instruction("mov w9, #-1");                                         // tm_isdst = -1
    emitter.instruction("str w9, [sp, #32]");                                   // store tm_isdst
    emitter.instruction("mov x0, sp");                                          // x0 = pointer to struct tm
    emitter.bl_c("mktime");                                                     // mktime(&tm) → x0=timestamp
    emitter.instruction("b __rt_strtotime_ret");                                // return through shared epilogue
}

/// Emits x86_64 (Linux) assembly for the ISO date/datetime parser sub-routine.
/// Entry label: `__rt_strtotime_iso_entry_linux_x86_64`.
/// Inputs: trimmed ptr at `[rsp+48]`, trimmed len at `[rsp+56]` (SysV ABI convention).
/// Parses `YYYY-MM-DD` (10 bytes), `YYYY-MM-DD HH:MM` (16 bytes), and `YYYY-MM-DD HH:MM:SS` (19 bytes).
/// Accepts date/time separator as space, `T`, or `t`.
/// Builds `struct tm` at `[rsp+0..47]`; fills tm_wday, tm_yday, tm_isdst before calling `mktime`.
/// Exits via `__rt_strtotime_fail_linux_x86_64` on parse error, or `__rt_strtotime_ret_linux_x86_64` after `mktime`.
/// Clobbers: rax, rcx, rdx, r8, rdi, rsi (via `call mktime`).
fn emit_iso_date_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime: ISO date sub-routine ---");
    emitter.label("__rt_strtotime_iso_entry_linux_x86_64");

    // -- reload trimmed ptr/len from dispatcher slots --
    emitter.instruction("mov rdi, QWORD PTR [rsp + 48]");                       // reload trimmed input pointer from dispatcher slot
    emitter.instruction("mov rsi, QWORD PTR [rsp + 56]");                       // reload trimmed input length from dispatcher slot
    emitter.instruction("cmp rsi, 10");                                         // require at least the YYYY-MM-DD prefix
    emitter.instruction("jb __rt_strtotime_fail_linux_x86_64");                 // reject too-short inputs through the shared fail label

    emitter.instruction("mov r8, rdi");                                         // pin the date-string pointer for repeated relative byte loads

    emitter.instruction("cmp rsi, 10");                                         // check for YYYY-MM-DD
    emitter.instruction("je __rt_strtotime_iso_validate_date_linux_x86_64");    // validate date-only shape
    emitter.instruction("cmp rsi, 16");                                         // check for YYYY-MM-DD HH:MM
    emitter.instruction("je __rt_strtotime_iso_validate_datetime_linux_x86_64"); // validate datetime without seconds
    emitter.instruction("cmp rsi, 19");                                         // check for YYYY-MM-DD HH:MM:SS
    emitter.instruction("je __rt_strtotime_iso_validate_datetime_linux_x86_64"); // validate full datetime shape
    emitter.instruction("jmp __rt_strtotime_fail_linux_x86_64");                // reject trailing junk or partial times

    emitter.label("__rt_strtotime_iso_validate_datetime_linux_x86_64");
    emitter.instruction("movzx eax, BYTE PTR [r8 + 10]");                       // load date/time separator
    emitter.instruction("cmp eax, 32");                                         // accept a space separator
    emitter.instruction("je __rt_strtotime_iso_datetime_separator_ok_linux_x86_64"); // continue after space separator
    emitter.instruction("cmp eax, 84");                                         // accept uppercase ISO T separator
    emitter.instruction("je __rt_strtotime_iso_datetime_separator_ok_linux_x86_64"); // continue after uppercase T separator
    emitter.instruction("cmp eax, 116");                                        // accept lowercase ISO t separator
    emitter.instruction("jne __rt_strtotime_fail_linux_x86_64");                // reject unsupported date/time separator
    emitter.label("__rt_strtotime_iso_datetime_separator_ok_linux_x86_64");
    emitter.instruction("movzx eax, BYTE PTR [r8 + 13]");                       // load hour/minute separator
    emitter.instruction("cmp eax, 58");                                         // require ':' after the hour
    emitter.instruction("jne __rt_strtotime_fail_linux_x86_64");                // reject malformed hour/minute separator
    emitter.instruction("cmp rsi, 19");                                         // full datetime needs a second separator
    emitter.instruction("jne __rt_strtotime_iso_validate_date_linux_x86_64");   // no seconds separator in HH:MM form
    emitter.instruction("movzx eax, BYTE PTR [r8 + 16]");                       // load minute/second separator
    emitter.instruction("cmp eax, 58");                                         // require ':' after the minute
    emitter.instruction("jne __rt_strtotime_fail_linux_x86_64");                // reject malformed minute/second separator

    emitter.label("__rt_strtotime_iso_validate_date_linux_x86_64");
    emitter.instruction("movzx eax, BYTE PTR [r8 + 4]");                        // load first date separator
    emitter.instruction("cmp eax, 45");                                         // require '-' after the year
    emitter.instruction("jne __rt_strtotime_fail_linux_x86_64");                // reject malformed year/month separator
    emitter.instruction("movzx eax, BYTE PTR [r8 + 7]");                        // load second date separator
    emitter.instruction("cmp eax, 45");                                         // require '-' after the month
    emitter.instruction("jne __rt_strtotime_fail_linux_x86_64");                // reject malformed month/day separator

    emitter.instruction("movzx eax, BYTE PTR [r8 + 0]");                        // load the first year digit from the date string
    emitter.instruction("sub eax, 48");                                         // convert the first year digit from ASCII to its numeric value
    emit_x86_64_reject_unless_decimal_digit(emitter, "eax");
    emitter.instruction("imul eax, eax, 1000");                                 // place the first year digit into the thousands column
    emitter.instruction("movzx ecx, BYTE PTR [r8 + 1]");                        // load the second year digit from the date string
    emitter.instruction("sub ecx, 48");                                         // convert the second year digit from ASCII to its numeric value
    emit_x86_64_reject_unless_decimal_digit(emitter, "ecx");
    emitter.instruction("imul ecx, ecx, 100");                                  // place the second year digit into the hundreds column
    emitter.instruction("add eax, ecx");                                        // accumulate the hundreds contribution
    emitter.instruction("movzx ecx, BYTE PTR [r8 + 2]");                        // load the third year digit from the date string
    emitter.instruction("sub ecx, 48");                                         // convert the third year digit from ASCII to its numeric value
    emit_x86_64_reject_unless_decimal_digit(emitter, "ecx");
    emitter.instruction("imul ecx, ecx, 10");                                   // place the third year digit into the tens column
    emitter.instruction("add eax, ecx");                                        // accumulate the tens contribution
    emitter.instruction("movzx ecx, BYTE PTR [r8 + 3]");                        // load the fourth year digit from the date string
    emitter.instruction("sub ecx, 48");                                         // convert the fourth year digit from ASCII to its numeric value
    emit_x86_64_reject_unless_decimal_digit(emitter, "ecx");
    emitter.instruction("add eax, ecx");                                        // finish assembling the full Gregorian year
    emitter.instruction("sub eax, 1900");                                       // convert the Gregorian year to libc's tm_year encoding
    emitter.instruction("mov DWORD PTR [rsp + 20], eax");                       // tm_year = parsed year - 1900

    emitter.instruction("movzx eax, BYTE PTR [r8 + 5]");                        // load the first month digit
    emitter.instruction("sub eax, 48");                                         // convert the first month digit from ASCII to its numeric value
    emit_x86_64_reject_unless_decimal_digit(emitter, "eax");
    emitter.instruction("imul eax, eax, 10");                                   // place the first month digit into the tens column
    emitter.instruction("movzx ecx, BYTE PTR [r8 + 6]");                        // load the second month digit
    emitter.instruction("sub ecx, 48");                                         // convert the second month digit from ASCII to its numeric value
    emit_x86_64_reject_unless_decimal_digit(emitter, "ecx");
    emitter.instruction("add eax, ecx");                                        // finish assembling the calendar month
    emitter.instruction("sub eax, 1");                                          // convert the month from PHP's 1-12 to libc's 0-11 tm_mon
    emitter.instruction("mov DWORD PTR [rsp + 16], eax");                       // tm_mon = parsed month - 1

    emitter.instruction("movzx eax, BYTE PTR [r8 + 8]");                        // load the first day-of-month digit
    emitter.instruction("sub eax, 48");                                         // convert the first day-of-month digit from ASCII to its numeric value
    emit_x86_64_reject_unless_decimal_digit(emitter, "eax");
    emitter.instruction("imul eax, eax, 10");                                   // place the first day-of-month digit into the tens column
    emitter.instruction("movzx ecx, BYTE PTR [r8 + 9]");                        // load the second day-of-month digit
    emitter.instruction("sub ecx, 48");                                         // convert the second day-of-month digit from ASCII to its numeric value
    emit_x86_64_reject_unless_decimal_digit(emitter, "ecx");
    emitter.instruction("add eax, ecx");                                        // finish assembling the day-of-month component
    emitter.instruction("mov DWORD PTR [rsp + 12], eax");                       // tm_mday = parsed day-of-month

    emitter.instruction("cmp rsi, 16");                                         // the YYYY-MM-DD HH:MM form requires at least 16 bytes
    emitter.instruction("jb __rt_strtotime_iso_notime_linux_x86_64");           // fall back to midnight when the time suffix is absent

    emitter.instruction("movzx eax, BYTE PTR [r8 + 11]");                       // load the first hour digit
    emitter.instruction("sub eax, 48");                                         // convert the first hour digit from ASCII to its numeric value
    emit_x86_64_reject_unless_decimal_digit(emitter, "eax");
    emitter.instruction("imul eax, eax, 10");                                   // place the first hour digit into the tens column
    emitter.instruction("movzx ecx, BYTE PTR [r8 + 12]");                       // load the second hour digit
    emitter.instruction("sub ecx, 48");                                         // convert the second hour digit from ASCII to its numeric value
    emit_x86_64_reject_unless_decimal_digit(emitter, "ecx");
    emitter.instruction("add eax, ecx");                                        // finish assembling the hour component
    emitter.instruction("mov DWORD PTR [rsp + 8], eax");                        // tm_hour = parsed hour

    emitter.instruction("movzx eax, BYTE PTR [r8 + 14]");                       // load the first minute digit
    emitter.instruction("sub eax, 48");                                         // convert the first minute digit from ASCII to its numeric value
    emit_x86_64_reject_unless_decimal_digit(emitter, "eax");
    emitter.instruction("imul eax, eax, 10");                                   // place the first minute digit into the tens column
    emitter.instruction("movzx ecx, BYTE PTR [r8 + 15]");                       // load the second minute digit
    emitter.instruction("sub ecx, 48");                                         // convert the second minute digit from ASCII to its numeric value
    emit_x86_64_reject_unless_decimal_digit(emitter, "ecx");
    emitter.instruction("add eax, ecx");                                        // finish assembling the minute component
    emitter.instruction("mov DWORD PTR [rsp + 4], eax");                        // tm_min = parsed minute
    emitter.instruction("cmp rsi, 19");                                         // full datetime includes seconds?
    emitter.instruction("jb __rt_strtotime_iso_no_seconds_linux_x86_64");       // partial datetime defaults seconds to zero

    emitter.instruction("movzx eax, BYTE PTR [r8 + 17]");                       // load the first second digit
    emitter.instruction("sub eax, 48");                                         // convert the first second digit from ASCII to its numeric value
    emit_x86_64_reject_unless_decimal_digit(emitter, "eax");
    emitter.instruction("imul eax, eax, 10");                                   // place the first second digit into the tens column
    emitter.instruction("movzx ecx, BYTE PTR [r8 + 18]");                       // load the second second digit
    emitter.instruction("sub ecx, 48");                                         // convert the second second digit from ASCII to its numeric value
    emit_x86_64_reject_unless_decimal_digit(emitter, "ecx");
    emitter.instruction("add eax, ecx");                                        // finish assembling the second component
    emitter.instruction("mov DWORD PTR [rsp + 0], eax");                        // tm_sec = parsed second
    emitter.instruction("jmp __rt_strtotime_iso_mktime_linux_x86_64");          // skip the midnight-default path

    emitter.label("__rt_strtotime_iso_no_seconds_linux_x86_64");
    emitter.instruction("mov DWORD PTR [rsp + 0], 0");                          // tm_sec = 0 for YYYY-MM-DD HH:MM
    emitter.instruction("jmp __rt_strtotime_iso_mktime_linux_x86_64");          // proceed with defaulted seconds

    emitter.label("__rt_strtotime_iso_notime_linux_x86_64");
    emitter.instruction("mov DWORD PTR [rsp + 0], 0");                          // default tm_sec to zero when the date-only form was given
    emitter.instruction("mov DWORD PTR [rsp + 4], 0");                          // default tm_min to zero when the date-only form was given
    emitter.instruction("mov DWORD PTR [rsp + 8], 0");                          // default tm_hour to zero when the date-only form was given

    emitter.label("__rt_strtotime_iso_mktime_linux_x86_64");
    emitter.instruction("mov DWORD PTR [rsp + 24], 0");                         // tm_wday = 0 (mktime ignores)
    emitter.instruction("mov DWORD PTR [rsp + 28], 0");                         // tm_yday = 0 (mktime ignores)
    emitter.instruction("mov DWORD PTR [rsp + 32], -1");                        // tm_isdst = -1 so libc mktime infers DST automatically
    emitter.instruction("mov rdi, rsp");                                        // pass &tm as the first SysV argument to libc mktime
    emitter.instruction("call mktime");                                         // convert the parsed components into a Unix timestamp
    emitter.instruction("jmp __rt_strtotime_ret_linux_x86_64");                 // return through the shared epilogue
}
