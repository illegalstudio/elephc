//! Purpose:
//! Emits the `HH:MM[:SS]` time-only parser sub-routine for `__rt_strtotime`.
//! Combines parsed hour/minute/second with today's date through libc `localtime`/`mktime`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::system::strtotime::mod::emit_strtotime()` when the dispatcher detects a colon-bearing digit prefix.
//!
//! Key details:
//! - Accepts `H:MM`, `HH:MM`, `H:MM:SS`, `HH:MM:SS`. Single-digit hour permitted (PHP-style).
//! - Hour/min/sec are stashed in dispatcher scratch slots `[sp+80/84/88]` so the `today_tm` helper can clobber temporaries.
//! - The parser consumes the whole trimmed input and validates PHP-compatible time ranges before `mktime` normalization.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emit the time-only strategy on both targets.
pub(crate) fn emit_time_only(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_time_only_linux_x86_64(emitter);
        emit_ampm_linux_x86_64(emitter);
        return;
    }

    emit_time_only_arm64(emitter);
    emit_ampm_arm64(emitter);
}

/// Emits ARM64 assembly for the `HH:MM[:SS]` time-only parsing sub-routine.
/// Accepts `H:MM`, `HH:MM`, `H:MM:SS`, `HH:MM:SS` (single-digit hour permitted per PHP).
///
/// Inputs (caller-provided stack slots):
///   - `[sp+48]` — trimmed input pointer
///   - `[sp+56]` — trimmed input length
///
/// Scratch stash slots used to preserve values across the `today_tm` call:
///   - `[sp+80]` — hour
///   - `[sp+84]` — min
///   - `[sp+88]` — second
///
/// Validation: hour 0..24, minute 0..59, second 0..60 (PHP ranges). On success,
/// builds a `tm` struct at `[sp+0..36]` with hour/minute/second patched in,
/// then calls `mktime` and returns through `__rt_strtotime_ret`.
fn emit_time_only_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime: HH:MM[:SS] time-only sub-routine ---");
    emitter.label("__rt_strtotime_time_entry");

    // -- reload input pointer/length --
    emitter.instruction("ldr x1, [sp, #48]");                                   // trimmed input pointer
    emitter.instruction("ldr x2, [sp, #56]");                                   // trimmed input length

    // -- parse hour: single or two digits --
    emitter.instruction("ldrb w9, [x1, #0]");                                   // first hour digit
    emitter.instruction("sub w9, w9, #48");                                     // numeric value of first digit
    emitter.instruction("ldrb w10, [x1, #1]");                                  // second char (digit or ':')
    emitter.instruction("cmp w10, #58");                                        // ':' ?
    emitter.instruction("b.eq __rt_strtotime_time_one_digit_hour");             // → single-digit hour

    // -- two-digit hour --
    emitter.instruction("sub w11, w10, #48");                                   // numeric value of second digit
    emitter.instruction("cmp w11, #9");                                         // digit ?
    emitter.instruction("b.hi __rt_strtotime_fail");                            // not a digit
    emitter.instruction("mov w12, #10");                                        // tens multiplier
    emitter.instruction("mul w9, w9, w12");                                     // first digit * 10
    emitter.instruction("add w9, w9, w11");                                     // hour = digit1*10 + digit2
    emitter.instruction("ldrb w10, [x1, #2]");                                  // expect ':' at offset 2
    emitter.instruction("cmp w10, #58");                                        // ':' ?
    emitter.instruction("b.ne __rt_strtotime_fail");                            // malformed
    emitter.instruction("add x1, x1, #3");                                      // advance past "HH:"
    emitter.instruction("b __rt_strtotime_time_parse_min");                     // continue

    emitter.label("__rt_strtotime_time_one_digit_hour");
    emitter.instruction("add x1, x1, #2");                                      // advance past "H:"

    emitter.label("__rt_strtotime_time_parse_min");
    // w9 = hour; cursor x1 points at first minute digit
    emitter.instruction("ldr x14, [sp, #48]");                                  // reload original trimmed pointer
    emitter.instruction("add x14, x14, x2");                                    // x14 = end-of-input pointer
    emitter.instruction("sub x15, x14, x1");                                    // remaining bytes
    emitter.instruction("cmp x15, #2");                                         // at least 2 chars for MM ?
    emitter.instruction("b.lt __rt_strtotime_fail");                            // no → fail

    emitter.instruction("ldrb w10, [x1, #0]");                                  // first minute digit
    emitter.instruction("sub w10, w10, #48");                                   // numeric value
    emitter.instruction("cmp w10, #9");                                         // digit ?
    emitter.instruction("b.hi __rt_strtotime_fail");                            // not a digit
    emitter.instruction("ldrb w11, [x1, #1]");                                  // second minute digit
    emitter.instruction("sub w11, w11, #48");                                   // numeric value
    emitter.instruction("cmp w11, #9");                                         // digit ?
    emitter.instruction("b.hi __rt_strtotime_fail");                            // not a digit
    emitter.instruction("mov w12, #10");                                        // tens multiplier
    emitter.instruction("mul w10, w10, w12");                                   // first minute digit * 10
    emitter.instruction("add w10, w10, w11");                                   // minute
    emitter.instruction("add x1, x1, #2");                                      // advance past MM
    emitter.instruction("cmp w9, #24");                                         // hour in PHP's accepted 0..24 range ?
    emitter.instruction("b.hi __rt_strtotime_fail");                            // reject invalid hour
    emitter.instruction("cmp w10, #59");                                        // minute in 0..59 ?
    emitter.instruction("b.hi __rt_strtotime_fail");                            // reject invalid minute

    // -- stash hour/min in scratch slots so today_tm can clobber w9..w15 --
    emitter.instruction("str w9, [sp, #80]");                                   // save hour
    emitter.instruction("str w10, [sp, #84]");                                  // save min

    // -- optional :SS suffix, then an optional signed offset ("14:30 +2 days") --
    emitter.instruction("sub x15, x14, x1");                                      // remaining bytes
    emitter.instruction("cbz x15, __rt_strtotime_time_no_secs");                  // no suffix → second = 0
    emitter.instruction("ldrb w11, [x1, #0]");                                    // next byte
    emitter.instruction("cmp w11, #58");                                          // ':' opens the seconds ?
    emitter.instruction("b.ne __rt_strtotime_time_trailing_offset");              // no seconds: only a signed offset may follow
    emitter.instruction("cmp x15, #3");                                           // ':SS' needs three bytes
    emitter.instruction("b.lt __rt_strtotime_fail");                              // reject a truncated suffix
    emitter.instruction("ldrb w11, [x1, #1]");                                    // first second digit
    emitter.instruction("sub w11, w11, #48");                                     // numeric value
    emitter.instruction("cmp w11, #9");                                           // digit ?
    emitter.instruction("b.hi __rt_strtotime_fail");                              // not a digit
    emitter.instruction("ldrb w12, [x1, #2]");                                    // second second digit
    emitter.instruction("sub w12, w12, #48");                                     // numeric value
    emitter.instruction("cmp w12, #9");                                           // digit ?
    emitter.instruction("b.hi __rt_strtotime_fail");                              // not a digit
    emitter.instruction("mov w13, #10");                                          // tens multiplier
    emitter.instruction("mul w11, w11, w13");                                     // first second digit * 10
    emitter.instruction("add w11, w11, w12");                                     // second
    emitter.instruction("cmp w11, #60");                                          // second in PHP's accepted 0..60 range ?
    emitter.instruction("b.hi __rt_strtotime_fail");                              // reject invalid second
    emitter.instruction("str w11, [sp, #88]");                                    // save second
    emitter.instruction("add x1, x1, #3");                                        // consume ':SS'
    emitter.instruction("cmp x1, x14");                                           // anything after the seconds ?
    emitter.instruction("b.ne __rt_strtotime_time_trailing_offset");              // only a signed offset may follow
    emitter.instruction("b __rt_strtotime_time_apply");                           // continue
    emitter.label("__rt_strtotime_time_trailing_offset");
    emitter.label("__rt_strtotime_time_trailing_skip");
    emitter.instruction("cmp x1, x14");                                           // cursor reached the end ?
    emitter.instruction("b.ge __rt_strtotime_fail");                              // nothing follows the separator
    emitter.instruction("ldrb w11, [x1, #0]");                                    // byte under the cursor
    emitter.instruction("cmp w11, #32");                                          // space ?
    emitter.instruction("b.ne __rt_strtotime_time_trailing_check");               // no: classify it
    emitter.instruction("add x1, x1, #1");                                        // skip one separator byte
    emitter.instruction("b __rt_strtotime_time_trailing_skip");                   // keep skipping
    emitter.label("__rt_strtotime_time_trailing_check");
    emitter.instruction("cmp w11, #43");                                          // '+' starts a signed offset ?
    emitter.instruction("b.eq __rt_strtotime_time_trailing_split");               // evaluate the time, then the offset
    emitter.instruction("cmp w11, #45");                                          // '-' starts a signed offset ?
    emitter.instruction("b.ne __rt_strtotime_fail");                              // any other trailing text is not a form we parse
    emitter.label("__rt_strtotime_time_trailing_split");
    emitter.instruction("ldr x3, [sp, #48]");                                     // trimmed input pointer
    emitter.instruction("sub x10, x1, x3");                                       // length of the time-of-day prefix
    emitter.instruction("b __rt_strtotime_kw_trailing_go");                       // the shared two-step evaluation: prefix first, then the offset against its result

    emitter.label("__rt_strtotime_time_no_secs");
    emitter.instruction("str wzr, [sp, #88]");                                  // tm_sec = 0

    emitter.label("__rt_strtotime_time_apply");
    emitter.instruction("bl __rt_strtotime_today_tm");                          // build today midnight tm at [sp+0..36]
    emitter.instruction("ldr w9, [sp, #80]");                                   // reload hour
    emitter.instruction("str w9, [sp, #8]");                                    // tm_hour
    emitter.instruction("ldr w9, [sp, #84]");                                   // reload min
    emitter.instruction("str w9, [sp, #4]");                                    // tm_min
    emitter.instruction("ldr w9, [sp, #88]");                                   // reload second
    emitter.instruction("str w9, [sp, #0]");                                    // tm_sec
    emitter.instruction("mov x0, sp");                                          // x0 = &tm
    emitter.instruction("bl __rt_mktime_shifted");                              // → x0 = ts
    emitter.instruction("b __rt_strtotime_ret");                                // return through shared epilogue
}

/// Emits x86_64 Linux assembly for the `HH:MM[:SS]` time-only parsing sub-routine.
/// Accepts `H:MM`, `HH:MM`, `H:MM:SS`, `HH:MM:SS` (single-digit hour permitted per PHP).
///
/// Inputs (caller-provided frame slots relative to rbp):
///   - `[rbp-80]` — trimmed input pointer
///   - `[rbp-72]` — trimmed input length
///
/// Scratch stash slots (relative to rsp — same offsets as ARM64):
///   - `[rbp-48]` / `[rsp+80]` — hour
///   - `[rbp-44]` / `[rsp+84]` — min
///   - `[rbp-40]` / `[rsp+88]` — second
///
/// Validation: hour 0..24, minute 0..59, second 0..60 (PHP ranges). On success,
/// builds a `tm` struct on the stack with hour/minute/second patched in,
/// then calls `mktime` and returns through `__rt_strtotime_ret_linux_x86_64`.
fn emit_time_only_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime: HH:MM[:SS] time-only sub-routine ---");
    emitter.label("__rt_strtotime_time_entry_linux_x86_64");

    // -- reload input ptr/len --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // trimmed input pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // trimmed input length

    // -- parse hour --
    emitter.instruction("movzx eax, BYTE PTR [rdi + 0]");                       // first hour digit
    emitter.instruction("sub eax, 48");                                         // numeric value
    emitter.instruction("movzx ecx, BYTE PTR [rdi + 1]");                       // second char
    emitter.instruction("cmp cl, 58");                                          // ':' ?
    emitter.instruction("je __rt_strtotime_time_one_digit_hour_linux_x86_64");  // → single-digit hour

    emitter.instruction("mov r8d, ecx");                                        // copy second char for digit check
    emitter.instruction("sub r8d, 48");                                         // numeric value of second digit
    emitter.instruction("cmp r8d, 9");                                          // digit ?
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // not a digit
    emitter.instruction("imul eax, eax, 10");                                   // first digit * 10
    emitter.instruction("add eax, r8d");                                        // hour
    emitter.instruction("movzx ecx, BYTE PTR [rdi + 2]");                       // expect ':' at offset 2
    emitter.instruction("cmp cl, 58");                                          // ':' ?
    emitter.instruction("jne __rt_strtotime_fail_linux_x86_64");                // malformed
    emitter.instruction("add rdi, 3");                                          // advance past "HH:"
    emitter.instruction("jmp __rt_strtotime_time_parse_min_linux_x86_64");      // continue

    emitter.label("__rt_strtotime_time_one_digit_hour_linux_x86_64");
    emitter.instruction("add rdi, 2");                                          // advance past "H:"

    emitter.label("__rt_strtotime_time_parse_min_linux_x86_64");
    emitter.instruction("mov r10, QWORD PTR [rbp - 80]");                       // reload original trimmed pointer
    emitter.instruction("add r10, rsi");                                        // r10 = end-of-input
    emitter.instruction("mov r11, r10");                                        // copy for remaining-bytes calc
    emitter.instruction("sub r11, rdi");                                        // remaining bytes
    emitter.instruction("cmp r11, 2");                                          // at least 2 for MM ?
    emitter.instruction("jl __rt_strtotime_fail_linux_x86_64");                 // no → fail

    emitter.instruction("movzx ecx, BYTE PTR [rdi + 0]");                       // first minute digit
    emitter.instruction("sub ecx, 48");                                         // numeric value
    emitter.instruction("cmp ecx, 9");                                          // digit ?
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // not a digit
    emitter.instruction("movzx r8d, BYTE PTR [rdi + 1]");                       // second minute digit
    emitter.instruction("sub r8d, 48");                                         // numeric value
    emitter.instruction("cmp r8d, 9");                                          // digit ?
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // not a digit
    emitter.instruction("imul ecx, ecx, 10");                                   // first minute digit * 10
    emitter.instruction("add ecx, r8d");                                        // minute
    emitter.instruction("add rdi, 2");                                          // advance past MM
    emitter.instruction("cmp eax, 24");                                         // hour in PHP's accepted 0..24 range ?
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // reject invalid hour
    emitter.instruction("cmp ecx, 59");                                         // minute in 0..59 ?
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // reject invalid minute

    // -- stash hour/min in scratch slots [rbp-48]/[rbp-44] = [rsp+80]/[rsp+84] --
    emitter.instruction("mov DWORD PTR [rbp - 48], eax");                       // save hour
    emitter.instruction("mov DWORD PTR [rbp - 44], ecx");                       // save min

    // -- optional :SS, then an optional signed offset ("14:30 +2 days") --
    emitter.instruction("mov r11, r10");                                          // end ptr again
    emitter.instruction("sub r11, rdi");                                          // remaining bytes
    emitter.instruction("test r11, r11");                                         // no suffix ?
    emitter.instruction("jz __rt_strtotime_time_no_secs_linux_x86_64");           // yes → second = 0
    emitter.instruction("movzx r8d, BYTE PTR [rdi + 0]");                         // next byte
    emitter.instruction("cmp r8b, 58");                                           // ':' opens the seconds ?
    emitter.instruction("jne __rt_strtotime_time_trailing_offset_linux_x86_64");  // no seconds: only a signed offset may follow
    emitter.instruction("cmp r11, 3");                                            // ':SS' needs three bytes
    emitter.instruction("jl __rt_strtotime_fail_linux_x86_64");                   // reject a truncated suffix
    emitter.instruction("movzx r8d, BYTE PTR [rdi + 1]");                         // first second digit
    emitter.instruction("sub r8d, 48");                                           // numeric value
    emitter.instruction("cmp r8d, 9");                                            // digit ?
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                   // not a digit
    emitter.instruction("movzx r9d, BYTE PTR [rdi + 2]");                         // second second digit
    emitter.instruction("sub r9d, 48");                                           // numeric value
    emitter.instruction("cmp r9d, 9");                                            // digit ?
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                   // not a digit
    emitter.instruction("imul r8d, r8d, 10");                                     // first second digit * 10
    emitter.instruction("add r8d, r9d");                                          // second
    emitter.instruction("cmp r8d, 60");                                           // second in PHP's accepted 0..60 range ?
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                   // reject invalid second
    emitter.instruction("mov DWORD PTR [rbp - 40], r8d");                         // save second ([rsp+88])
    emitter.instruction("add rdi, 3");                                            // consume ':SS'
    emitter.instruction("cmp rdi, r10");                                          // anything after the seconds ?
    emitter.instruction("jne __rt_strtotime_time_trailing_offset_linux_x86_64");  // only a signed offset may follow
    emitter.instruction("jmp __rt_strtotime_time_apply_linux_x86_64");            // continue
    emitter.label("__rt_strtotime_time_trailing_offset_linux_x86_64");
    emitter.label("__rt_strtotime_time_trailing_skip_linux_x86_64");
    emitter.instruction("cmp rdi, r10");                                          // cursor reached the end ?
    emitter.instruction("jae __rt_strtotime_fail_linux_x86_64");                  // nothing follows the separator
    emitter.instruction("movzx r8d, BYTE PTR [rdi + 0]");                         // byte under the cursor
    emitter.instruction("cmp r8b, 32");                                           // space ?
    emitter.instruction("jne __rt_strtotime_time_trailing_check_linux_x86_64");   // no: classify it
    emitter.instruction("add rdi, 1");                                            // skip one separator byte
    emitter.instruction("jmp __rt_strtotime_time_trailing_skip_linux_x86_64");    // keep skipping
    emitter.label("__rt_strtotime_time_trailing_check_linux_x86_64");
    emitter.instruction("cmp r8b, 43");                                           // '+' starts a signed offset ?
    emitter.instruction("je __rt_strtotime_time_trailing_split_linux_x86_64");    // evaluate the time, then the offset
    emitter.instruction("cmp r8b, 45");                                           // '-' starts a signed offset ?
    emitter.instruction("jne __rt_strtotime_fail_linux_x86_64");                  // any other trailing text is not a form we parse
    emitter.label("__rt_strtotime_time_trailing_split_linux_x86_64");
    emitter.instruction("mov rax, rdi");                                          // cursor
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                         // trimmed input pointer
    emitter.instruction("sub rax, rdi");                                          // length of the time-of-day prefix
    emitter.instruction("jmp __rt_strtotime_kw_trailing_go_linux_x86_64");        // the shared two-step evaluation: prefix first, then the offset against its result

    emitter.label("__rt_strtotime_time_no_secs_linux_x86_64");
    emitter.instruction("mov DWORD PTR [rbp - 40], 0");                         // tm_sec = 0

    emitter.label("__rt_strtotime_time_apply_linux_x86_64");
    emitter.instruction("call __rt_strtotime_today_tm_linux_x86_64");           // build today midnight tm
    emitter.instruction("mov eax, DWORD PTR [rbp - 48]");                       // reload saved hour
    emitter.instruction("mov DWORD PTR [rsp + 8], eax");                        // tm_hour
    emitter.instruction("mov eax, DWORD PTR [rbp - 44]");                       // reload saved min
    emitter.instruction("mov DWORD PTR [rsp + 4], eax");                        // tm_min
    emitter.instruction("mov eax, DWORD PTR [rbp - 40]");                       // reload saved second
    emitter.instruction("mov DWORD PTR [rsp + 0], eax");                        // tm_sec
    emitter.instruction("mov rdi, rsp");                                        // rdi = &tm
    emitter.instruction("call __rt_mktime_shifted");                            // → rax = ts
    emitter.instruction("jmp __rt_strtotime_ret_linux_x86_64");                 // return
}

/// Emits ARM64 assembly for the 12-hour clock forms `H[H][:MM[:SS]][ ]am|pm` (`9am`, `9 am`, `9:30pm`,
/// `12:00:15 AM`). The dispatcher routes every digit-leading input whose last two bytes are `am` or
/// `pm` here. The hour must be 1..12; `12am` is hour 0 and `12pm` is hour 12, as in PHP. The parsed
/// fields are stashed in the same `[sp+80/84/88]` slots the 24-hour parser uses and the shared
/// `__rt_strtotime_time_apply` tail builds the timestamp from the effective clock's date.
fn emit_ampm_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime: H[H][:MM[:SS]][ ]am|pm sub-routine ---");
    emitter.label("__rt_strtotime_ampm_entry");
    emitter.instruction("ldr x1, [sp, #48]");                                   // trimmed input pointer
    emitter.instruction("ldr x2, [sp, #56]");                                   // trimmed input length
    emitter.instruction("ldrb w9, [x1]");                                       // first hour digit (the dispatcher routed a digit-leading input)
    emitter.instruction("sub w9, w9, #48");                                     // numeric value of the first digit
    emitter.instruction("mov x3, #1");                                          // cursor = index of the byte after the first digit
    emitter.instruction("ldrb w10, [x1, x3]");                                  // second byte (the length is at least 3)
    emitter.instruction("sub w11, w10, #48");                                   // numeric value of the second byte
    emitter.instruction("cmp w11, #9");                                         // a second hour digit ?
    emitter.instruction("b.hi __rt_strtotime_ampm_hour_done");                  // no → one-digit hour
    emitter.instruction("mov w12, #10");                                        // tens multiplier
    emitter.instruction("mul w9, w9, w12");                                     // first digit * 10
    emitter.instruction("add w9, w9, w11");                                     // hour = digit1*10 + digit2
    emitter.instruction("add x3, x3, #1");                                      // cursor past the second digit
    emitter.label("__rt_strtotime_ampm_hour_done");
    emitter.instruction("cmp w9, #1");                                          // 12-hour clock starts at 1
    emitter.instruction("b.lo __rt_strtotime_fail");                            // hour 0 has no meridiem form
    emitter.instruction("cmp w9, #12");                                         // 12-hour clock ends at 12
    emitter.instruction("b.hi __rt_strtotime_fail");                            // hour above 12 with a meridiem is invalid
    emitter.instruction("mov w4, #0");                                          // minute = 0
    emitter.instruction("mov w5, #0");                                          // second = 0
    emitter.instruction("cmp x3, x2");                                          // anything left for the meridiem ?
    emitter.instruction("b.ge __rt_strtotime_fail");                            // no → malformed
    emitter.instruction("ldrb w10, [x1, x3]");                                  // byte under the cursor
    emitter.instruction("cmp w10, #58");                                        // ':' opens the minutes ?
    emitter.instruction("b.ne __rt_strtotime_ampm_ws");                         // no → optional space and the meridiem
    emitter.instruction("add x6, x3, #3");                                      // cursor after ":MM"
    emitter.instruction("cmp x6, x2");                                          // does ":MM" fit before the end ?
    emitter.instruction("b.gt __rt_strtotime_fail");                            // no → malformed
    emitter.instruction("add x7, x1, x3");                                      // pointer to the ':'
    emitter.instruction("ldrb w10, [x7, #1]");                                  // first minute digit
    emitter.instruction("sub w10, w10, #48");                                   // numeric value
    emitter.instruction("cmp w10, #9");                                         // digit ?
    emitter.instruction("b.hi __rt_strtotime_fail");                            // not a digit
    emitter.instruction("ldrb w11, [x7, #2]");                                  // second minute digit
    emitter.instruction("sub w11, w11, #48");                                   // numeric value
    emitter.instruction("cmp w11, #9");                                         // digit ?
    emitter.instruction("b.hi __rt_strtotime_fail");                            // not a digit
    emitter.instruction("mov w12, #10");                                        // tens multiplier
    emitter.instruction("mul w4, w10, w12");                                    // first minute digit * 10
    emitter.instruction("add w4, w4, w11");                                     // minute
    emitter.instruction("cmp w4, #59");                                         // minute in 0..59 ?
    emitter.instruction("b.hi __rt_strtotime_fail");                            // reject invalid minute
    emitter.instruction("mov x3, x6");                                          // cursor past the minutes
    emitter.instruction("cmp x3, x2");                                          // anything left for the meridiem ?
    emitter.instruction("b.ge __rt_strtotime_fail");                            // no → malformed
    emitter.instruction("ldrb w10, [x1, x3]");                                  // byte under the cursor
    emitter.instruction("cmp w10, #58");                                        // ':' opens the seconds ?
    emitter.instruction("b.ne __rt_strtotime_ampm_ws");                         // no → optional space and the meridiem
    emitter.instruction("add x6, x3, #3");                                      // cursor after ":SS"
    emitter.instruction("cmp x6, x2");                                          // does ":SS" fit before the end ?
    emitter.instruction("b.gt __rt_strtotime_fail");                            // no → malformed
    emitter.instruction("add x7, x1, x3");                                      // pointer to the ':'
    emitter.instruction("ldrb w10, [x7, #1]");                                  // first second digit
    emitter.instruction("sub w10, w10, #48");                                   // numeric value
    emitter.instruction("cmp w10, #9");                                         // digit ?
    emitter.instruction("b.hi __rt_strtotime_fail");                            // not a digit
    emitter.instruction("ldrb w11, [x7, #2]");                                  // second second digit
    emitter.instruction("sub w11, w11, #48");                                   // numeric value
    emitter.instruction("cmp w11, #9");                                         // digit ?
    emitter.instruction("b.hi __rt_strtotime_fail");                            // not a digit
    emitter.instruction("mov w12, #10");                                        // tens multiplier
    emitter.instruction("mul w5, w10, w12");                                    // first second digit * 10
    emitter.instruction("add w5, w5, w11");                                     // second
    emitter.instruction("cmp w5, #59");                                         // second in 0..59 ?
    emitter.instruction("b.hi __rt_strtotime_fail");                            // reject invalid second
    emitter.instruction("mov x3, x6");                                          // cursor past the seconds
    emitter.label("__rt_strtotime_ampm_ws");
    emitter.instruction("cmp x3, x2");                                          // anything left for the meridiem ?
    emitter.instruction("b.ge __rt_strtotime_fail");                            // no → malformed
    emitter.instruction("ldrb w10, [x1, x3]");                                  // byte under the cursor
    emitter.instruction("cmp w10, #32");                                        // optional single space before the meridiem ?
    emitter.instruction("b.ne __rt_strtotime_ampm_meridiem");                   // no → the byte is the meridiem
    emitter.instruction("add x3, x3, #1");                                      // skip the space
    emitter.label("__rt_strtotime_ampm_meridiem");
    emitter.instruction("sub x6, x2, x3");                                      // bytes left
    emitter.instruction("cmp x6, #2");                                          // exactly "am" or "pm" must remain
    emitter.instruction("b.ne __rt_strtotime_fail");                            // anything else is trailing junk
    emitter.instruction("add x7, x1, x3");                                      // pointer to the meridiem
    emitter.instruction("ldrb w10, [x7]");                                      // a or p
    emitter.instruction("orr w10, w10, #32");                                   // fold ASCII case
    emitter.instruction("ldrb w11, [x7, #1]");                                  // m
    emitter.instruction("orr w11, w11, #32");                                   // fold ASCII case
    emitter.instruction("cmp w11, #109");                                       // 'm' ?
    emitter.instruction("b.ne __rt_strtotime_fail");                            // not a meridiem
    emitter.instruction("cmp w10, #97");                                        // am ?
    emitter.instruction("b.eq __rt_strtotime_ampm_am");                         // yes → ante meridiem
    emitter.instruction("cmp w10, #112");                                       // pm ?
    emitter.instruction("b.ne __rt_strtotime_fail");                            // not a meridiem
    emitter.instruction("cmp w9, #12");                                         // 12pm stays 12 ?
    emitter.instruction("b.eq __rt_strtotime_ampm_store");                      // yes → noon
    emitter.instruction("add w9, w9, #12");                                     // 1pm..11pm → 13..23
    emitter.instruction("b __rt_strtotime_ampm_store");                         // store the fields
    emitter.label("__rt_strtotime_ampm_am");
    emitter.instruction("cmp w9, #12");                                         // 12am is midnight ?
    emitter.instruction("b.ne __rt_strtotime_ampm_store");                      // 1am..11am keep their hour
    emitter.instruction("mov w9, #0");                                          // 12am → hour 0
    emitter.label("__rt_strtotime_ampm_store");
    emitter.instruction("str w9, [sp, #80]");                                   // save hour
    emitter.instruction("str w4, [sp, #84]");                                   // save minute
    emitter.instruction("str w5, [sp, #88]");                                   // save second
    emitter.instruction("b __rt_strtotime_time_apply");                         // build the timestamp from the effective clock's date
}

/// Emits x86_64 Linux assembly for the 12-hour clock forms `H[H][:MM[:SS]][ ]am|pm`; the ARM64 twin
/// documents the grammar. Fields land in `[rbp-48]`/`[rbp-44]`/`[rbp-40]` and the shared
/// `__rt_strtotime_time_apply_linux_x86_64` tail builds the timestamp.
fn emit_ampm_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime: H[H][:MM[:SS]][ ]am|pm sub-routine ---");
    emitter.label("__rt_strtotime_ampm_entry_linux_x86_64");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // trimmed input pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // trimmed input length
    emitter.instruction("movzx eax, BYTE PTR [rdi]");                           // first hour digit (the dispatcher routed a digit-leading input)
    emitter.instruction("sub eax, 48");                                         // numeric value of the first digit
    emitter.instruction("mov r8d, 1");                                          // cursor = index of the byte after the first digit
    emitter.instruction("movzx ecx, BYTE PTR [rdi + r8]");                      // second byte (the length is at least 3)
    emitter.instruction("mov r9d, ecx");                                        // copy for the digit test
    emitter.instruction("sub r9d, 48");                                         // numeric value of the second byte
    emitter.instruction("cmp r9d, 9");                                          // a second hour digit ?
    emitter.instruction("ja __rt_strtotime_ampm_hour_done_linux_x86_64");       // no → one-digit hour
    emitter.instruction("imul eax, eax, 10");                                   // first digit * 10
    emitter.instruction("add eax, r9d");                                        // hour = digit1*10 + digit2
    emitter.instruction("add r8, 1");                                           // cursor past the second digit
    emitter.label("__rt_strtotime_ampm_hour_done_linux_x86_64");
    emitter.instruction("cmp eax, 1");                                          // 12-hour clock starts at 1
    emitter.instruction("jb __rt_strtotime_fail_linux_x86_64");                 // hour 0 has no meridiem form
    emitter.instruction("cmp eax, 12");                                         // 12-hour clock ends at 12
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // hour above 12 with a meridiem is invalid
    emitter.instruction("xor r10d, r10d");                                      // minute = 0
    emitter.instruction("xor r11d, r11d");                                      // second = 0
    emitter.instruction("cmp r8, rsi");                                         // anything left for the meridiem ?
    emitter.instruction("jge __rt_strtotime_fail_linux_x86_64");                // no → malformed
    emitter.instruction("movzx ecx, BYTE PTR [rdi + r8]");                      // byte under the cursor
    emitter.instruction("cmp cl, 58");                                          // ':' opens the minutes ?
    emitter.instruction("jne __rt_strtotime_ampm_ws_linux_x86_64");             // no → optional space and the meridiem
    emitter.instruction("lea r9, [r8 + 3]");                                    // cursor after ":MM"
    emitter.instruction("cmp r9, rsi");                                         // does ":MM" fit before the end ?
    emitter.instruction("jg __rt_strtotime_fail_linux_x86_64");                 // no → malformed
    emitter.instruction("movzx edx, BYTE PTR [rdi + r8 + 1]");                  // first minute digit
    emitter.instruction("sub edx, 48");                                         // numeric value
    emitter.instruction("cmp edx, 9");                                          // digit ?
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // not a digit
    emitter.instruction("movzx ecx, BYTE PTR [rdi + r8 + 2]");                  // second minute digit
    emitter.instruction("sub ecx, 48");                                         // numeric value
    emitter.instruction("cmp ecx, 9");                                          // digit ?
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // not a digit
    emitter.instruction("imul edx, edx, 10");                                   // first minute digit * 10
    emitter.instruction("add edx, ecx");                                        // minute
    emitter.instruction("cmp edx, 59");                                         // minute in 0..59 ?
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // reject invalid minute
    emitter.instruction("mov r10d, edx");                                       // keep the minute
    emitter.instruction("mov r8, r9");                                          // cursor past the minutes
    emitter.instruction("cmp r8, rsi");                                         // anything left for the meridiem ?
    emitter.instruction("jge __rt_strtotime_fail_linux_x86_64");                // no → malformed
    emitter.instruction("movzx ecx, BYTE PTR [rdi + r8]");                      // byte under the cursor
    emitter.instruction("cmp cl, 58");                                          // ':' opens the seconds ?
    emitter.instruction("jne __rt_strtotime_ampm_ws_linux_x86_64");             // no → optional space and the meridiem
    emitter.instruction("lea r9, [r8 + 3]");                                    // cursor after ":SS"
    emitter.instruction("cmp r9, rsi");                                         // does ":SS" fit before the end ?
    emitter.instruction("jg __rt_strtotime_fail_linux_x86_64");                 // no → malformed
    emitter.instruction("movzx edx, BYTE PTR [rdi + r8 + 1]");                  // first second digit
    emitter.instruction("sub edx, 48");                                         // numeric value
    emitter.instruction("cmp edx, 9");                                          // digit ?
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // not a digit
    emitter.instruction("movzx ecx, BYTE PTR [rdi + r8 + 2]");                  // second second digit
    emitter.instruction("sub ecx, 48");                                         // numeric value
    emitter.instruction("cmp ecx, 9");                                          // digit ?
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // not a digit
    emitter.instruction("imul edx, edx, 10");                                   // first second digit * 10
    emitter.instruction("add edx, ecx");                                        // second
    emitter.instruction("cmp edx, 59");                                         // second in 0..59 ?
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // reject invalid second
    emitter.instruction("mov r11d, edx");                                       // keep the second
    emitter.instruction("mov r8, r9");                                          // cursor past the seconds
    emitter.label("__rt_strtotime_ampm_ws_linux_x86_64");
    emitter.instruction("cmp r8, rsi");                                         // anything left for the meridiem ?
    emitter.instruction("jge __rt_strtotime_fail_linux_x86_64");                // no → malformed
    emitter.instruction("movzx ecx, BYTE PTR [rdi + r8]");                      // byte under the cursor
    emitter.instruction("cmp cl, 32");                                          // optional single space before the meridiem ?
    emitter.instruction("jne __rt_strtotime_ampm_meridiem_linux_x86_64");       // no → the byte is the meridiem
    emitter.instruction("add r8, 1");                                           // skip the space
    emitter.label("__rt_strtotime_ampm_meridiem_linux_x86_64");
    emitter.instruction("mov r9, rsi");                                         // length ...
    emitter.instruction("sub r9, r8");                                          // ... minus cursor = bytes left
    emitter.instruction("cmp r9, 2");                                           // exactly "am" or "pm" must remain
    emitter.instruction("jne __rt_strtotime_fail_linux_x86_64");                // anything else is trailing junk
    emitter.instruction("movzx ecx, BYTE PTR [rdi + r8]");                      // a or p
    emitter.instruction("or ecx, 32");                                          // fold ASCII case
    emitter.instruction("movzx edx, BYTE PTR [rdi + r8 + 1]");                  // m
    emitter.instruction("or edx, 32");                                          // fold ASCII case
    emitter.instruction("cmp dl, 109");                                         // 'm' ?
    emitter.instruction("jne __rt_strtotime_fail_linux_x86_64");                // not a meridiem
    emitter.instruction("cmp cl, 97");                                          // am ?
    emitter.instruction("je __rt_strtotime_ampm_am_linux_x86_64");              // yes → ante meridiem
    emitter.instruction("cmp cl, 112");                                         // pm ?
    emitter.instruction("jne __rt_strtotime_fail_linux_x86_64");                // not a meridiem
    emitter.instruction("cmp eax, 12");                                         // 12pm stays 12 ?
    emitter.instruction("je __rt_strtotime_ampm_store_linux_x86_64");           // yes → noon
    emitter.instruction("add eax, 12");                                         // 1pm..11pm → 13..23
    emitter.instruction("jmp __rt_strtotime_ampm_store_linux_x86_64");          // store the fields
    emitter.label("__rt_strtotime_ampm_am_linux_x86_64");
    emitter.instruction("cmp eax, 12");                                         // 12am is midnight ?
    emitter.instruction("jne __rt_strtotime_ampm_store_linux_x86_64");          // 1am..11am keep their hour
    emitter.instruction("xor eax, eax");                                        // 12am → hour 0
    emitter.label("__rt_strtotime_ampm_store_linux_x86_64");
    emitter.instruction("mov DWORD PTR [rbp - 48], eax");                       // save hour
    emitter.instruction("mov DWORD PTR [rbp - 44], r10d");                      // save minute
    emitter.instruction("mov DWORD PTR [rbp - 40], r11d");                      // save second
    emitter.instruction("jmp __rt_strtotime_time_apply_linux_x86_64");          // build the timestamp from the effective clock's date
}
