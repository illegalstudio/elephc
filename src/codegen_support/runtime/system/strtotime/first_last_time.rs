//! Purpose:
//! Emits the optional time of day that may follow an ordinal date phrase in `__rt_strtotime`:
//! `last day of this month 23:59:59`, `first monday of next month 08:30`.
//!
//! Called from:
//! - `super::first_last_day`, right after the phrase's `month` word, and again in each compute
//!   path once the base tm has been filled.
//!
//! Key details:
//! - PHP reads such a string as the phrase followed by a time; the phrase alone used to be all
//!   this strategy took, so anything after `month` failed the whole string (Symfony's `about`
//!   command asks for the end of the month this way).
//! - The parsed time of day travels as seconds in the dispatcher scratch slot `[sp+96]`
//!   (`[rsp+96]` on x86_64), `-1` meaning "none". The slot is free by then: the phrase is fully
//!   consumed, x86_64 no longer needs its parked cursor, and `now_tm` only fills `[sp+0..36]`.
//! - Accepted shapes: `H:MM`, `HH:MM`, `H:MM:SS`, `HH:MM:SS`, hour 0..23, minutes/seconds 0..59, and the
//!   keywords `noon` (12:00:00) and `midnight` (00:00:00).
//!   Anything else after the phrase still fails through `__rt_strtotime_fail`.

use crate::codegen_support::emit::Emitter;

/// Parses the optional time (ARM64). Entered with the cursor in `x3` right after `month` and its
/// whitespace, end in `x4`; leaves the seconds (or `-1`) at `[sp+96]` and the input consumed.
pub(super) fn emit_fl_trailing_time_arm64(emitter: &mut Emitter) {
    emitter.instruction("mov w13, #-1");                                        // no time of day unless one follows
    emitter.instruction("str w13, [sp, #96]");                                  // record the absence before parsing
    emitter.instruction("cmp x3, x4");                                          // anything left after "month" ?
    emitter.instruction("b.ge __rt_strtotime_fl_time_done");                    // no: the phrase stands alone
    emitter.instruction("bl __rt_strtotime_fl_kw_token");                         // x9 = keyword kind (-1 = none), x10 = consumed
    emitter.instruction("cmp x9, #5");                                            // noon ?
    emitter.instruction("b.eq __rt_strtotime_fl_time_noon");                      // 12:00:00
    emitter.instruction("cmp x9, #4");                                            // midnight ?
    emitter.instruction("b.eq __rt_strtotime_fl_time_midnight");                  // 00:00:00
    emitter.instruction("b __rt_strtotime_fl_time_digits");                       // a numeric time of day
    emitter.label("__rt_strtotime_fl_time_noon");
    emitter.instruction("mov w13, #43200");                                       // twelve hours of seconds
    emitter.instruction("add x3, x3, x10");                                       // consume the word
    emitter.instruction("b __rt_strtotime_fl_time_store");                        // keep the time of day and require the end of the input
    emitter.label("__rt_strtotime_fl_time_midnight");
    emitter.instruction("mov w13, #0");                                           // no seconds past midnight
    emitter.instruction("add x3, x3, x10");                                       // consume the word
    emitter.instruction("b __rt_strtotime_fl_time_store");                        // keep the time of day and require the end of the input
    emitter.label("__rt_strtotime_fl_time_digits");
    emitter.instruction("ldrb w9, [x3]");                                       // first hour digit
    emitter.instruction("sub w9, w9, #48");                                     // ASCII digit → value
    emitter.instruction("cmp w9, #9");                                          // must be 0..9
    emitter.instruction("b.hi __rt_strtotime_fail");                            // trailing junk → fail
    emitter.instruction("add x3, x3, #1");                                      // consume it
    emitter.instruction("mov w11, w9");                                         // hour so far
    emitter.instruction("cmp x3, x4");                                          // a colon or second digit must follow
    emitter.instruction("b.ge __rt_strtotime_fail");                            // a bare number is not a time
    emitter.instruction("ldrb w9, [x3]");                                       // second hour digit or ':'
    emitter.instruction("cmp w9, #58");                                         // ':' ?
    emitter.instruction("b.eq __rt_strtotime_fl_time_colon");                   // single-digit hour
    emitter.instruction("sub w9, w9, #48");                                     // ASCII digit → value
    emitter.instruction("cmp w9, #9");                                          // must be 0..9
    emitter.instruction("b.hi __rt_strtotime_fail");                            // malformed hour → fail
    emitter.instruction("add x3, x3, #1");                                      // consume it
    emitter.instruction("mov w12, #10");                                        // decimal base
    emitter.instruction("madd w11, w11, w12, w9");                              // hour = hour * 10 + digit
    emitter.label("__rt_strtotime_fl_time_colon");
    emitter.instruction("cmp x3, x4");                                          // the ':' must be there
    emitter.instruction("b.ge __rt_strtotime_fail");                            // hour without minutes → fail
    emitter.instruction("ldrb w9, [x3]");                                       // expected ':'
    emitter.instruction("cmp w9, #58");                                         // ':' ?
    emitter.instruction("b.ne __rt_strtotime_fail");                            // anything else → fail
    emitter.instruction("add x3, x3, #1");                                      // consume ':'
    emitter.instruction("cmp w11, #23");                                        // hour must be 0..23
    emitter.instruction("b.hi __rt_strtotime_fail");                            // out of range → fail
    emit_two_digits_arm64(emitter);
    emitter.instruction("cmp w12, #59");                                        // minutes must be 0..59
    emitter.instruction("b.hi __rt_strtotime_fail");                            // out of range → fail
    emitter.instruction("mov w14, #3600");                                      // seconds per hour
    emitter.instruction("mul w13, w11, w14");                                   // hours → seconds
    emitter.instruction("mov w14, #60");                                        // seconds per minute
    emitter.instruction("madd w13, w12, w14, w13");                             // + minutes → seconds
    emitter.instruction("cmp x3, x4");                                          // optional ':SS' ?
    emitter.instruction("b.ge __rt_strtotime_fl_time_store");                   // end of input: no seconds
    emitter.instruction("ldrb w9, [x3]");                                       // next byte
    emitter.instruction("cmp w9, #58");                                         // ':' ?
    emitter.instruction("b.ne __rt_strtotime_fl_time_store");                   // no seconds
    emitter.instruction("add x3, x3, #1");                                      // consume ':'
    emit_two_digits_arm64(emitter);
    emitter.instruction("cmp w12, #59");                                        // seconds must be 0..59
    emitter.instruction("b.hi __rt_strtotime_fail");                            // out of range → fail
    emitter.instruction("add w13, w13, w12");                                   // + seconds
    emitter.label("__rt_strtotime_fl_time_store");
    emitter.instruction("str w13, [sp, #96]");                                  // keep the time of day across the helpers
    emitter.instruction("bl __rt_strtotime_skip_ws");                           // skip trailing whitespace
    emitter.instruction("cmp x3, x4");                                          // must be fully consumed now
    emitter.instruction("b.lt __rt_strtotime_fail");                            // trailing junk → fail
    emitter.label("__rt_strtotime_fl_time_done");
}

/// Reads exactly two decimal digits at `x3` into `w12` (ARM64), failing the parse otherwise.
fn emit_two_digits_arm64(emitter: &mut Emitter) {
    emitter.instruction("cmp x3, x4");                                          // first digit present ?
    emitter.instruction("b.ge __rt_strtotime_fail");                            // truncated time → fail
    emitter.instruction("ldrb w9, [x3]");                                       // first digit
    emitter.instruction("sub w9, w9, #48");                                     // ASCII digit → value
    emitter.instruction("cmp w9, #9");                                          // must be 0..9
    emitter.instruction("b.hi __rt_strtotime_fail");                            // malformed → fail
    emitter.instruction("add x3, x3, #1");                                      // consume it
    emitter.instruction("cmp x3, x4");                                          // second digit present ?
    emitter.instruction("b.ge __rt_strtotime_fail");                            // truncated time → fail
    emitter.instruction("ldrb w10, [x3]");                                      // second digit
    emitter.instruction("sub w10, w10, #48");                                   // ASCII digit → value
    emitter.instruction("cmp w10, #9");                                         // must be 0..9
    emitter.instruction("b.hi __rt_strtotime_fail");                            // malformed → fail
    emitter.instruction("add x3, x3, #1");                                      // consume it
    emitter.instruction("mov w12, #10");                                        // decimal base
    emitter.instruction("madd w12, w9, w12, w10");                              // value = first * 10 + second
}

/// Overrides the tm time of day from `[sp+96]` when a time was parsed (ARM64).
///
/// Uses `w13`..`w16` only: the nth-weekday path keeps the target month in `w9` across it.
pub(super) fn emit_fl_apply_time_arm64(emitter: &mut Emitter, suffix: &str) {
    let skip = format!("__rt_strtotime_fl_apply_time_skip_{suffix}");
    emitter.instruction("ldr w13, [sp, #96]");                                  // parsed time of day, or -1
    emitter.instruction(&format!("tbnz w13, #31, {skip}"));                     // none: keep the phrase's own time
    emitter.instruction("mov w14, #3600");                                      // seconds per hour
    emitter.instruction("udiv w15, w13, w14");                                  // hours
    emitter.instruction("msub w13, w15, w14, w13");                             // remaining seconds
    emitter.instruction("mov w14, #60");                                        // seconds per minute
    emitter.instruction("udiv w16, w13, w14");                                  // minutes
    emitter.instruction("msub w13, w16, w14, w13");                             // seconds
    emitter.instruction("str w13, [sp, #0]");                                   // tm_sec
    emitter.instruction("str w16, [sp, #4]");                                   // tm_min
    emitter.instruction("str w15, [sp, #8]");                                   // tm_hour
    emitter.label(&skip);
}

/// x86_64 twin of [`emit_fl_trailing_time_arm64`]: cursor `rdi`, end `r10`, time at `[rsp+96]`.
pub(super) fn emit_fl_trailing_time_x86_64(emitter: &mut Emitter) {
    emitter.instruction("mov DWORD PTR [rsp + 96], -1");                        // no time of day unless one follows
    emitter.instruction("cmp rdi, r10");                                        // anything left after "month" ?
    emitter.instruction("jae __rt_strtotime_fl_time_done_linux_x86_64");        // no: the phrase stands alone
    emitter.instruction("mov QWORD PTR [rsp + 96], rdi");                         // park the cursor for the keyword helper (it reads [rbp-32])
    emitter.instruction("call __rt_strtotime_fl_kw_token_linux_x86_64");          // rax = consumed, rdx = keyword kind (-1 = none); clobbers rdi/r10
    emitter.instruction("mov rdi, QWORD PTR [rsp + 96]");                         // restore the cursor
    emitter.instruction("mov r10, QWORD PTR [rbp - 80]");                         // recompute end = base ...
    emitter.instruction("add r10, QWORD PTR [rbp - 72]");                         // ... + length
    emitter.instruction("mov DWORD PTR [rsp + 96], -1");                          // no time of day unless one follows
    emitter.instruction("cmp rdx, 5");                                            // noon ?
    emitter.instruction("je __rt_strtotime_fl_time_noon_linux_x86_64");           // 12:00:00
    emitter.instruction("cmp rdx, 4");                                            // midnight ?
    emitter.instruction("je __rt_strtotime_fl_time_midnight_linux_x86_64");       // 00:00:00
    emitter.instruction("jmp __rt_strtotime_fl_time_digits_linux_x86_64");        // a numeric time of day
    emitter.label("__rt_strtotime_fl_time_noon_linux_x86_64");
    emitter.instruction("mov r11d, 43200");                                       // twelve hours of seconds
    emitter.instruction("add rdi, rax");                                          // consume the word
    emitter.instruction("jmp __rt_strtotime_fl_time_store_linux_x86_64");         // keep the time of day and require the end of the input
    emitter.label("__rt_strtotime_fl_time_midnight_linux_x86_64");
    emitter.instruction("xor r11d, r11d");                                        // no seconds past midnight
    emitter.instruction("add rdi, rax");                                          // consume the word
    emitter.instruction("jmp __rt_strtotime_fl_time_store_linux_x86_64");         // keep the time of day and require the end of the input
    emitter.label("__rt_strtotime_fl_time_digits_linux_x86_64");
    emitter.instruction("movzx eax, BYTE PTR [rdi]");                           // first hour digit
    emitter.instruction("sub eax, 48");                                         // ASCII digit → value
    emitter.instruction("cmp eax, 9");                                          // must be 0..9
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // trailing junk → fail
    emitter.instruction("inc rdi");                                             // consume it
    emitter.instruction("mov r11d, eax");                                       // hour so far
    emitter.instruction("cmp rdi, r10");                                        // a colon or second digit must follow
    emitter.instruction("jae __rt_strtotime_fail_linux_x86_64");                // a bare number is not a time
    emitter.instruction("movzx eax, BYTE PTR [rdi]");                           // second hour digit or ':'
    emitter.instruction("cmp eax, 58");                                         // ':' ?
    emitter.instruction("je __rt_strtotime_fl_time_colon_linux_x86_64");        // single-digit hour
    emitter.instruction("sub eax, 48");                                         // ASCII digit → value
    emitter.instruction("cmp eax, 9");                                          // must be 0..9
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // malformed hour → fail
    emitter.instruction("inc rdi");                                             // consume it
    emitter.instruction("imul r11d, r11d, 10");                                 // hour * 10 ...
    emitter.instruction("add r11d, eax");                                       // ... + digit
    emitter.label("__rt_strtotime_fl_time_colon_linux_x86_64");
    emitter.instruction("cmp rdi, r10");                                        // the ':' must be there
    emitter.instruction("jae __rt_strtotime_fail_linux_x86_64");                // hour without minutes → fail
    emitter.instruction("movzx eax, BYTE PTR [rdi]");                           // expected ':'
    emitter.instruction("cmp eax, 58");                                         // ':' ?
    emitter.instruction("jne __rt_strtotime_fail_linux_x86_64");                // anything else → fail
    emitter.instruction("inc rdi");                                             // consume ':'
    emitter.instruction("cmp r11d, 23");                                        // hour must be 0..23
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // out of range → fail
    emit_two_digits_x86_64(emitter);
    emitter.instruction("cmp ecx, 59");                                         // minutes must be 0..59
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // out of range → fail
    emitter.instruction("imul r11d, r11d, 3600");                               // hours → seconds
    emitter.instruction("imul ecx, ecx, 60");                                   // minutes → seconds
    emitter.instruction("add r11d, ecx");                                       // time of day so far
    emitter.instruction("cmp rdi, r10");                                        // optional ':SS' ?
    emitter.instruction("jae __rt_strtotime_fl_time_store_linux_x86_64");       // end of input: no seconds
    emitter.instruction("movzx eax, BYTE PTR [rdi]");                           // next byte
    emitter.instruction("cmp eax, 58");                                         // ':' ?
    emitter.instruction("jne __rt_strtotime_fl_time_store_linux_x86_64");       // no seconds
    emitter.instruction("inc rdi");                                             // consume ':'
    emit_two_digits_x86_64(emitter);
    emitter.instruction("cmp ecx, 59");                                         // seconds must be 0..59
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // out of range → fail
    emitter.instruction("add r11d, ecx");                                       // + seconds
    emitter.label("__rt_strtotime_fl_time_store_linux_x86_64");
    emitter.instruction("mov DWORD PTR [rsp + 96], r11d");                      // keep the time of day across the helpers
    emitter.instruction("call __rt_strtotime_skip_ws_linux_x86_64");            // skip trailing whitespace
    emitter.instruction("cmp rdi, r10");                                        // must be fully consumed now
    emitter.instruction("jb __rt_strtotime_fail_linux_x86_64");                 // trailing junk → fail
    emitter.label("__rt_strtotime_fl_time_done_linux_x86_64");
}

/// Reads exactly two decimal digits at `rdi` into `ecx` (x86_64), failing the parse otherwise.
fn emit_two_digits_x86_64(emitter: &mut Emitter) {
    emitter.instruction("cmp rdi, r10");                                        // first digit present ?
    emitter.instruction("jae __rt_strtotime_fail_linux_x86_64");                // truncated time → fail
    emitter.instruction("movzx eax, BYTE PTR [rdi]");                           // first digit
    emitter.instruction("sub eax, 48");                                         // ASCII digit → value
    emitter.instruction("cmp eax, 9");                                          // must be 0..9
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // malformed → fail
    emitter.instruction("inc rdi");                                             // consume it
    emitter.instruction("cmp rdi, r10");                                        // second digit present ?
    emitter.instruction("jae __rt_strtotime_fail_linux_x86_64");                // truncated time → fail
    emitter.instruction("movzx edx, BYTE PTR [rdi]");                           // second digit
    emitter.instruction("sub edx, 48");                                         // ASCII digit → value
    emitter.instruction("cmp edx, 9");                                          // must be 0..9
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // malformed → fail
    emitter.instruction("inc rdi");                                             // consume it
    emitter.instruction("imul eax, eax, 10");                                   // first * 10 ...
    emitter.instruction("add eax, edx");                                        // ... + second
    emitter.instruction("mov ecx, eax");                                        // value in ecx
}

/// x86_64 twin of [`emit_fl_apply_time_arm64`]. Clobbers `eax`, `ecx`, `edx` only.
pub(super) fn emit_fl_apply_time_x86_64(emitter: &mut Emitter, suffix: &str) {
    let skip = format!("__rt_strtotime_fl_apply_time_skip_{suffix}_linux_x86_64");
    emitter.instruction("mov eax, DWORD PTR [rsp + 96]");                       // parsed time of day, or -1
    emitter.instruction("test eax, eax");                                       // negative = none
    emitter.instruction(&format!("js {skip}"));                                 // keep the phrase's own time
    emitter.instruction("xor edx, edx");                                        // clear the dividend's high half
    emitter.instruction("mov ecx, 3600");                                       // seconds per hour
    emitter.instruction("div ecx");                                             // eax = hours, edx = remaining seconds
    emitter.instruction("mov DWORD PTR [rsp + 8], eax");                        // tm_hour
    emitter.instruction("mov eax, edx");                                        // remaining seconds
    emitter.instruction("xor edx, edx");                                        // clear the dividend's high half
    emitter.instruction("mov ecx, 60");                                         // seconds per minute
    emitter.instruction("div ecx");                                             // eax = minutes, edx = seconds
    emitter.instruction("mov DWORD PTR [rsp + 4], eax");                        // tm_min
    emitter.instruction("mov DWORD PTR [rsp + 0], edx");                        // tm_sec
    emitter.label(&skip);
}
