//! Purpose:
//! Emits the `HH:MM[:SS]` time-only parser sub-routine for `__rt_strtotime`.
//! Combines parsed hour/minute/second with today's date through libc `localtime`/`mktime`.
//!
//! Called from:
//! - `crate::codegen::runtime::system::strtotime::mod::emit_strtotime()` when the dispatcher detects a colon-bearing digit prefix.
//!
//! Key details:
//! - Accepts `H:MM`, `HH:MM`, `H:MM:SS`, `HH:MM:SS`. Single-digit hour permitted (PHP-style).
//! - Hour/min/sec are stashed in dispatcher scratch slots `[sp+80/84/88]` so the `today_tm` helper can clobber temporaries.
//! - The parser consumes the whole trimmed input and validates PHP-compatible time ranges before `mktime` normalization.

use crate::codegen::{emit::Emitter, platform::Arch};

/// Emit the time-only strategy on both targets.
pub(crate) fn emit_time_only(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_time_only_linux_x86_64(emitter);
        return;
    }

    emit_time_only_arm64(emitter);
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

    // -- optional :SS suffix --
    emitter.instruction("sub x15, x14, x1");                                    // remaining bytes
    emitter.instruction("cbz x15, __rt_strtotime_time_no_secs");                // no suffix → second = 0
    emitter.instruction("cmp x15, #3");                                         // suffix must be exactly ":SS"
    emitter.instruction("b.ne __rt_strtotime_fail");                            // reject partial or trailing suffix junk
    emitter.instruction("ldrb w11, [x1, #0]");                                  // expect ':'
    emitter.instruction("cmp w11, #58");                                        // ':' ?
    emitter.instruction("b.ne __rt_strtotime_fail");                            // trailing junk without seconds → fail
    emitter.instruction("ldrb w11, [x1, #1]");                                  // first second digit
    emitter.instruction("sub w11, w11, #48");                                   // numeric value
    emitter.instruction("cmp w11, #9");                                         // digit ?
    emitter.instruction("b.hi __rt_strtotime_fail");                            // not a digit
    emitter.instruction("ldrb w12, [x1, #2]");                                  // second second digit
    emitter.instruction("sub w12, w12, #48");                                   // numeric value
    emitter.instruction("cmp w12, #9");                                         // digit ?
    emitter.instruction("b.hi __rt_strtotime_fail");                            // not a digit
    emitter.instruction("mov w13, #10");                                        // tens multiplier
    emitter.instruction("mul w11, w11, w13");                                   // first second digit * 10
    emitter.instruction("add w11, w11, w12");                                   // second
    emitter.instruction("cmp w11, #60");                                        // second in PHP's accepted 0..60 range ?
    emitter.instruction("b.hi __rt_strtotime_fail");                            // reject invalid second
    emitter.instruction("str w11, [sp, #88]");                                  // save second
    emitter.instruction("b __rt_strtotime_time_apply");                         // continue

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
    emitter.bl_c("mktime");                                                     // → x0 = ts
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

    // -- optional :SS --
    emitter.instruction("mov r11, r10");                                        // end ptr again
    emitter.instruction("sub r11, rdi");                                        // remaining bytes
    emitter.instruction("test r11, r11");                                       // no suffix ?
    emitter.instruction("jz __rt_strtotime_time_no_secs_linux_x86_64");         // yes → second = 0
    emitter.instruction("cmp r11, 3");                                          // suffix must be exactly ":SS"
    emitter.instruction("jne __rt_strtotime_fail_linux_x86_64");                // reject partial or trailing suffix junk
    emitter.instruction("movzx r8d, BYTE PTR [rdi + 0]");                       // expect ':'
    emitter.instruction("cmp r8b, 58");                                         // ':' ?
    emitter.instruction("jne __rt_strtotime_fail_linux_x86_64");                // trailing junk without seconds → fail
    emitter.instruction("movzx r8d, BYTE PTR [rdi + 1]");                       // first second digit
    emitter.instruction("sub r8d, 48");                                         // numeric value
    emitter.instruction("cmp r8d, 9");                                          // digit ?
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // not a digit
    emitter.instruction("movzx r9d, BYTE PTR [rdi + 2]");                       // second second digit
    emitter.instruction("sub r9d, 48");                                         // numeric value
    emitter.instruction("cmp r9d, 9");                                          // digit ?
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // not a digit
    emitter.instruction("imul r8d, r8d, 10");                                   // first second digit * 10
    emitter.instruction("add r8d, r9d");                                        // second
    emitter.instruction("cmp r8d, 60");                                         // second in PHP's accepted 0..60 range ?
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // reject invalid second
    emitter.instruction("mov DWORD PTR [rbp - 40], r8d");                       // save second ([rsp+88])
    emitter.instruction("jmp __rt_strtotime_time_apply_linux_x86_64");          // continue

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
    emitter.instruction("call mktime");                                         // → rax = ts
    emitter.instruction("jmp __rt_strtotime_ret_linux_x86_64");                 // return
}
