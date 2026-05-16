//! Purpose:
//! Emits the named-weekday parser sub-routine for `__rt_strtotime` — handles `Monday`, `Mon`, `next Monday`, `last Friday`, `this Wednesday`, etc.
//! Combines target weekday + modifier (next/last/this) with today's date through `__rt_strtotime_today_tm` + `mktime`.
//!
//! Called from:
//! - `crate::codegen::runtime::system::strtotime::mod::emit_strtotime()` when match_word returns a kind in `{6, 7, 8}` (next/last/this) or `{10..16}` (weekday).
//!
//! Key details:
//! - Target weekday encoded with kind = 10 + tm_wday (libc: 0=Sunday..6=Saturday).
//! - Modifier semantics: `next` advances at least 1 day; `last` retreats at least 1 day; `this` may produce delta == 0 if today already matches.
//! - Result is always midnight (00:00:00) of the target day, mirroring PHP's `strtotime("Monday")`.

use crate::codegen::{emit::Emitter, platform::Arch};

/// Emit the weekdays strategy on both targets.
pub(crate) fn emit_weekdays(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_weekdays_linux_x86_64(emitter);
        return;
    }

    emit_weekdays_arm64(emitter);
}

fn emit_weekdays_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime: weekdays (next/last/this <weekday>) sub-routine ---");
    emitter.label("__rt_strtotime_weekdays_entry");
    // Inputs (from match_word): x9 = kind, x10 = consumed bytes.

    // -- branch on kind: 10..16 = direct weekday, 6..8 = modifier needing follow-up weekday --
    emitter.instruction("cmp w9, #10");                                         // direct weekday kind ?
    emitter.instruction("b.ge __rt_strtotime_weekdays_direct");                 // yes → use implicit "this"

    // -- modifier path (kind 6/7/8) --
    emitter.instruction("str w9, [sp, #84]");                                   // save modifier kind to slot before any helpers clobber w-regs
    emitter.instruction("ldr x3, [sp, #48]");                                   // trimmed ptr
    emitter.instruction("ldr x2, [sp, #56]");                                   // trimmed len
    emitter.instruction("add x4, x3, x2");                                      // end pointer
    emitter.instruction("add x3, x3, x10");                                     // advance past modifier word
    emitter.instruction("bl __rt_strtotime_skip_ws");                           // skip WS
    emitter.instruction("cmp x3, x4");                                          // anything left ?
    emitter.instruction("b.ge __rt_strtotime_fail");                            // no → fail

    emitter.instruction("bl __rt_strtotime_lc_cursor");                         // lowercase next 16 bytes
    emitter.instruction("add x6, sp, #64");                                     // candidate ptr
    emitter.adrp("x7", "_strtotime_keyword_tab");                               // table base page
    emitter.add_lo12("x7", "x7", "_strtotime_keyword_tab");                     // resolve table base
    emitter.instruction("sub x8, x4, x3");                                      // remaining bytes
    emitter.instruction("mov x11, #16");                                        // cap to lc16 size
    emitter.instruction("cmp x8, x11");                                         // remaining > 16 ?
    emitter.instruction("csel x8, x8, x11, lo");                                // x8 = min(remaining, 16)
    emitter.instruction("bl __rt_strtotime_match_word");                        // x9 = kind, x10 = consumed

    emitter.instruction("cbz x10, __rt_strtotime_fail");                        // no match after modifier → fail
    emitter.instruction("cmp w9, #10");                                         // weekday kind range ?
    emitter.instruction("b.lt __rt_strtotime_fail");                            // below 10 → not a weekday
    emitter.instruction("cmp w9, #16");                                         // above 16 ?
    emitter.instruction("b.gt __rt_strtotime_fail");                            // out of weekday range
    emitter.instruction("add x3, x3, x10");                                     // advance past weekday
    emitter.instruction("cmp x3, x4");                                          // consumed the whole input ?
    emitter.instruction("b.ne __rt_strtotime_fail");                            // trailing junk after weekday → fail
    emitter.instruction("sub w11, w9, #10");                                    // target_wday = kind - 10
    emitter.instruction("ldr w12, [sp, #84]");                                  // reload modifier kind
    emitter.instruction("b __rt_strtotime_weekdays_compute");                   // continue to delta computation

    emitter.label("__rt_strtotime_weekdays_direct");
    // -- direct weekday (kind 10..16, no explicit modifier) --
    emitter.instruction("sub w11, w9, #10");                                    // target_wday
    emitter.instruction("mov w12, #8");                                         // implicit modifier = "this"

    emitter.label("__rt_strtotime_weekdays_compute");
    // -- save target_wday + modifier across today_tm bl --
    emitter.instruction("str w11, [sp, #80]");                                  // save target_wday
    emitter.instruction("str w12, [sp, #84]");                                  // save modifier
    emitter.instruction("bl __rt_strtotime_today_tm");                          // build today midnight into [sp+0..36]

    emitter.instruction("ldr w11, [sp, #80]");                                  // reload target_wday
    emitter.instruction("ldr w12, [sp, #84]");                                  // reload modifier
    emitter.instruction("ldr w13, [sp, #24]");                                  // current_wday from tm

    // -- delta computation --
    emitter.instruction("cmp w12, #6");                                         // next ?
    emitter.instruction("b.eq __rt_strtotime_weekdays_next");                   // → next semantics
    emitter.instruction("cmp w12, #8");                                         // this ?
    emitter.instruction("b.eq __rt_strtotime_weekdays_this");                   // → this semantics
    // -- last semantics (modifier == 7): delta = -((current - target + 7) mod 7); if 0 → -7 --
    emitter.instruction("sub w14, w13, w11");                                   // current - target
    emitter.instruction("add w14, w14, #7");                                    // + 7 to keep positive (range [1..13])
    emitter.instruction("cmp w14, #7");                                         // need mod 7 (not & 7 since range > 7)
    emitter.instruction("b.lo __rt_strtotime_weekdays_last_negate");            // < 7 → already reduced
    emitter.instruction("sub w14, w14, #7");                                    // ≥ 7 → subtract 7 to fold into [0..6]
    emitter.label("__rt_strtotime_weekdays_last_negate");
    emitter.instruction("neg w14, w14");                                        // delta = -(mod result)
    emitter.instruction("cbnz w14, __rt_strtotime_weekdays_apply");             // non-zero → apply
    emitter.instruction("mov w14, #-7");                                        // zero → step back a full week
    emitter.instruction("b __rt_strtotime_weekdays_apply");                     // apply

    emitter.label("__rt_strtotime_weekdays_next");
    emitter.instruction("sub w14, w11, w13");                                   // target - current
    emitter.instruction("add w14, w14, #7");                                    // + 7 (range [1..13])
    emitter.instruction("cmp w14, #7");                                         // need mod 7
    emitter.instruction("b.lo __rt_strtotime_weekdays_next_check");             // < 7 → already reduced
    emitter.instruction("sub w14, w14, #7");                                    // ≥ 7 → subtract 7
    emitter.label("__rt_strtotime_weekdays_next_check");
    emitter.instruction("cbnz w14, __rt_strtotime_weekdays_apply");             // non-zero → apply
    emitter.instruction("mov w14, #7");                                         // zero → step forward a full week
    emitter.instruction("b __rt_strtotime_weekdays_apply");                     // apply

    emitter.label("__rt_strtotime_weekdays_this");
    emitter.instruction("sub w14, w11, w13");                                   // target - current
    emitter.instruction("add w14, w14, #7");                                    // + 7 (range [1..13])
    emitter.instruction("cmp w14, #7");                                         // need mod 7
    emitter.instruction("b.lo __rt_strtotime_weekdays_apply");                  // < 7 → already reduced
    emitter.instruction("sub w14, w14, #7");                                    // ≥ 7 → subtract 7

    emitter.label("__rt_strtotime_weekdays_apply");
    emitter.instruction("ldr w9, [sp, #12]");                                   // tm_mday
    emitter.instruction("add w9, w9, w14");                                     // tm_mday += delta
    emitter.instruction("str w9, [sp, #12]");                                   // store
    emitter.instruction("mov x0, sp");                                          // x0 = &tm
    emitter.bl_c("mktime");                                                     // → x0 = ts
    emitter.instruction("b __rt_strtotime_ret");                                // return
}

fn emit_weekdays_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime: weekdays sub-routine ---");
    emitter.label("__rt_strtotime_weekdays_entry_linux_x86_64");
    // Inputs: rdx = kind, rax = consumed bytes.

    emitter.instruction("cmp rdx, 10");                                         // direct weekday kind ?
    emitter.instruction("jge __rt_strtotime_weekdays_direct_linux_x86_64");     // yes → implicit "this"

    // -- modifier path (kind 6/7/8) --
    emitter.instruction("mov DWORD PTR [rsp + 84], edx");                       // save modifier kind
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // cursor = trimmed ptr
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // trimmed length
    emitter.instruction("mov r10, rdi");                                        // r10 = cursor (will become end)
    emitter.instruction("add r10, rsi");                                        // end = ptr + len
    emitter.instruction("add rdi, rax");                                        // advance cursor past modifier word
    emitter.instruction("call __rt_strtotime_skip_ws_linux_x86_64");            // skip WS
    emitter.instruction("cmp rdi, r10");                                        // anything left ?
    emitter.instruction("jge __rt_strtotime_fail_linux_x86_64");                // no → fail

    emitter.instruction("call __rt_strtotime_lc_cursor_linux_x86_64");          // lowercase next 16 bytes
    emitter.instruction("mov QWORD PTR [rsp + 112], rdi");                      // save weekday cursor before match
    emitter.instruction("lea rdi, [rbp - 64]");                                 // candidate ptr = lc16 base
    emitter.instruction("lea rsi, [rip + _strtotime_keyword_tab]");             // keyword table base
    emitter.instruction("mov rcx, r10");                                        // copy end
    emitter.instruction("mov r11, QWORD PTR [rbp - 80]");                       // reload original ptr
    emitter.instruction("add r11, rax");                                        // add modifier-consumed
    // Compute remaining: end - (orig_ptr + modifier_consumed + ws_skipped) — simpler: pass len-capped
    emitter.instruction("mov rcx, 16");                                         // cap to 16
    emitter.instruction("call __rt_strtotime_match_word_linux_x86_64");         // rax = consumed, rdx = kind

    emitter.instruction("test rax, rax");                                       // matched ?
    emitter.instruction("jz __rt_strtotime_fail_linux_x86_64");                 // no → fail
    emitter.instruction("cmp rdx, 10");                                         // weekday range ?
    emitter.instruction("jl __rt_strtotime_fail_linux_x86_64");                 // below → fail
    emitter.instruction("cmp rdx, 16");                                         // above ?
    emitter.instruction("jg __rt_strtotime_fail_linux_x86_64");                 // out of range
    emitter.instruction("mov rdi, QWORD PTR [rsp + 112]");                      // restore weekday cursor
    emitter.instruction("add rdi, rax");                                        // advance past weekday
    emitter.instruction("mov r10, QWORD PTR [rbp - 80]");                       // reload trimmed pointer
    emitter.instruction("add r10, QWORD PTR [rbp - 72]");                       // recompute end pointer
    emitter.instruction("cmp rdi, r10");                                        // consumed the whole input ?
    emitter.instruction("jne __rt_strtotime_fail_linux_x86_64");                // trailing junk after weekday → fail
    emitter.instruction("sub edx, 10");                                         // target_wday
    emitter.instruction("mov DWORD PTR [rsp + 80], edx");                       // save target_wday
    emitter.instruction("jmp __rt_strtotime_weekdays_compute_linux_x86_64");    // continue

    emitter.label("__rt_strtotime_weekdays_direct_linux_x86_64");
    emitter.instruction("sub edx, 10");                                         // target_wday
    emitter.instruction("mov DWORD PTR [rsp + 80], edx");                       // save target_wday
    emitter.instruction("mov DWORD PTR [rsp + 84], 8");                         // implicit modifier = "this"

    emitter.label("__rt_strtotime_weekdays_compute_linux_x86_64");
    emitter.instruction("call __rt_strtotime_today_tm_linux_x86_64");           // build today midnight

    emitter.instruction("mov r11d, DWORD PTR [rsp + 80]");                      // reload target_wday
    emitter.instruction("mov r12d, DWORD PTR [rsp + 84]");                      // reload modifier
    emitter.instruction("mov r13d, DWORD PTR [rsp + 24]");                      // current_wday

    emitter.instruction("cmp r12d, 6");                                         // next ?
    emitter.instruction("je __rt_strtotime_weekdays_next_linux_x86_64");        // modifier is "next"
    emitter.instruction("cmp r12d, 8");                                         // this ?
    emitter.instruction("je __rt_strtotime_weekdays_this_linux_x86_64");        // modifier is "this"
    // -- last semantics: delta = -((current - target + 7) mod 7); if 0 → -7 --
    emitter.instruction("mov r14d, r13d");                                      // current
    emitter.instruction("sub r14d, r11d");                                      // current - target
    emitter.instruction("add r14d, 7");                                         // + 7 (range [1..13])
    emitter.instruction("cmp r14d, 7");                                         // need mod 7
    emitter.instruction("jb __rt_strtotime_weekdays_last_negate_linux_x86_64"); // < 7 → already reduced
    emitter.instruction("sub r14d, 7");                                         // ≥ 7 → subtract 7
    emitter.label("__rt_strtotime_weekdays_last_negate_linux_x86_64");
    emitter.instruction("neg r14d");                                            // negate
    emitter.instruction("test r14d, r14d");                                     // delta == 0 ?
    emitter.instruction("jnz __rt_strtotime_weekdays_apply_linux_x86_64");      // no → apply
    emitter.instruction("mov r14d, -7");                                        // step back a week
    emitter.instruction("jmp __rt_strtotime_weekdays_apply_linux_x86_64");      // apply

    emitter.label("__rt_strtotime_weekdays_next_linux_x86_64");
    emitter.instruction("mov r14d, r11d");                                      // target
    emitter.instruction("sub r14d, r13d");                                      // target - current
    emitter.instruction("add r14d, 7");                                         // + 7 (range [1..13])
    emitter.instruction("cmp r14d, 7");                                         // need mod 7
    emitter.instruction("jb __rt_strtotime_weekdays_next_check_linux_x86_64");  // < 7 → already reduced
    emitter.instruction("sub r14d, 7");                                         // ≥ 7 → subtract 7
    emitter.label("__rt_strtotime_weekdays_next_check_linux_x86_64");
    emitter.instruction("test r14d, r14d");                                     // delta == 0 ?
    emitter.instruction("jnz __rt_strtotime_weekdays_apply_linux_x86_64");      // no → apply
    emitter.instruction("mov r14d, 7");                                         // step forward a week
    emitter.instruction("jmp __rt_strtotime_weekdays_apply_linux_x86_64");      // apply

    emitter.label("__rt_strtotime_weekdays_this_linux_x86_64");
    emitter.instruction("mov r14d, r11d");                                      // target
    emitter.instruction("sub r14d, r13d");                                      // target - current
    emitter.instruction("add r14d, 7");                                         // + 7 (range [1..13])
    emitter.instruction("cmp r14d, 7");                                         // need mod 7
    emitter.instruction("jb __rt_strtotime_weekdays_apply_linux_x86_64");       // < 7 → already reduced
    emitter.instruction("sub r14d, 7");                                         // ≥ 7 → subtract 7

    emitter.label("__rt_strtotime_weekdays_apply_linux_x86_64");
    emitter.instruction("mov eax, DWORD PTR [rsp + 12]");                       // tm_mday
    emitter.instruction("add eax, r14d");                                       // tm_mday += delta
    emitter.instruction("mov DWORD PTR [rsp + 12], eax");                       // store
    emitter.instruction("mov rdi, rsp");                                        // rdi = &tm
    emitter.instruction("call mktime");                                         // → rax = ts
    emitter.instruction("jmp __rt_strtotime_ret_linux_x86_64");                 // return
}
