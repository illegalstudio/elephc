//! Purpose:
//! Emits the ordinal date phrases for `__rt_strtotime`:
//! `first/last day of <modifier> month` and `<ordinal> <weekday> of <modifier> month`
//! (e.g. `first monday of next month`, `last friday of this month`, `third tuesday of …`).
//!
//! Called from:
//! - `crate::codegen::runtime::system::strtotime::mod::emit_strtotime()`: the dispatcher routes the
//!   ordinals `first`..`fifth` (keyword kinds 31-35) and `last` (kind 7) here. When the phrase is
//!   neither `… day of …` nor `… <weekday> of …`, the `last` path falls back to the weekday
//!   strategy (so `last monday` still works).
//!
//! Key details:
//! - Entry label `__rt_strtotime_firstlast_entry` (ARM64) / `_linux_x86_64`. The leading ordinal
//!   gives `n` (0 = "last", 1..5 = first..fifth). The second word selects the mode: `day` (token
//!   kind 0 in `_strtotime_firstlast_tab`) or a weekday name (kinds 10-16 in the keyword table).
//!   Phrase tokens after that (`of`, the `this`/`next`/`last`/`previous` modifier, `month`) are
//!   matched via `_strtotime_firstlast_tab`.
//! - `day of …` preserves the base time of day; `<weekday> of …` resets to midnight, matching PHP.
//! - The base month/year come from `__rt_strtotime_now_tm` (the `_strtotime_clock` global). The
//!   nth-weekday path uses two `mktime` calls: one on day 1 (or day 0 of the next month, for
//!   "last") to read the reference weekday, then a second after placing the nth occurrence.
//! - Scratch slots used by this strategy: wd-or-sentinel `[sp+80]`, ordinal `n` `[sp+88]`, month
//!   delta `[sp+104]` (and on x86 the parked cursor `[rsp+96]`).

use crate::codegen::{emit::Emitter, platform::Arch};

/// Dispatches to the architecture-specific ordinal-date parser.
pub(crate) fn emit_first_last_day(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_first_last_day_linux_x86_64(emitter);
        return;
    }

    emit_first_last_day_arm64(emitter);
}

/// Emits ARM64 assembly for the ordinal date phrases. Cursor in `x3`, end in `x4` (preserved
/// across the helper calls). `fl_token` matches `_strtotime_firstlast_tab`; `fl_kw_token` matches
/// the keyword table for the weekday. Both park the link register in `x15` so the sp-relative lc16
/// buffer stays addressable.
fn emit_first_last_day_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime: ordinal date (day-of-month / nth-weekday) sub-routine ---");
    emitter.label("__rt_strtotime_firstlast_entry");

    // -- leading ordinal: kind 7 = "last" (n=0); kinds 31..35 = first..fifth (n=1..5) --
    emitter.instruction("cmp x9, #7");                                          // "last" ordinal ?
    emitter.instruction("b.ne __rt_strtotime_fl_ord");                          // otherwise first..fifth
    emitter.instruction("mov x12, #0");                                         // n = 0 marks "last"
    emitter.instruction("b __rt_strtotime_fl_ord_done");                        // ordinal resolved
    emitter.label("__rt_strtotime_fl_ord");
    emitter.instruction("sub x12, x9, #30");                                    // 31..35 → 1..5
    emitter.label("__rt_strtotime_fl_ord_done");
    emitter.instruction("ldr x1, [sp, #48]");                                   // trimmed input pointer (base)
    emitter.instruction("ldr x2, [sp, #56]");                                   // trimmed input length
    emitter.instruction("add x4, x1, x2");                                      // x4 = end pointer
    emitter.instruction("add x3, x1, x10");                                     // cursor past the leading ordinal word
    emitter.instruction("str x12, [sp, #88]");                                  // stash n
    emitter.instruction("bl __rt_strtotime_skip_ws");                           // skip whitespace before day/weekday

    // -- second word: "day" (kind 0) or a weekday name (kinds 10-16) --
    emitter.instruction("bl __rt_strtotime_fl_token");                          // x9 = kind, x10 = consumed
    emitter.instruction("cmp x9, #0");                                          // is the word "day" ?
    emitter.instruction("b.eq __rt_strtotime_fl_set_day");                      // yes → day-of-month mode
    emitter.instruction("bl __rt_strtotime_fl_kw_token");                       // try a weekday (keyword table)
    emitter.instruction("cmp x9, #10");                                         // weekday kinds start at 10
    emitter.instruction("b.lt __rt_strtotime_fl_neither");                      // not a weekday → neither
    emitter.instruction("cmp x9, #16");                                         // weekday kinds end at 16
    emitter.instruction("b.gt __rt_strtotime_fl_neither");                      // not a weekday → neither
    emitter.instruction("sub x9, x9, #10");                                     // target weekday 0..6 (Sun..Sat)
    emitter.instruction("str x9, [sp, #80]");                                   // stash target weekday
    emitter.instruction("add x3, x3, x10");                                     // advance past the weekday name
    emitter.instruction("b __rt_strtotime_fl_weekday_tail");                    // require "of month" or bare-fall-back

    emitter.label("__rt_strtotime_fl_set_day");
    emitter.instruction("ldr x11, [sp, #88]");                                  // ordinal n
    emitter.instruction("cmp x11, #1");                                         // "day" is only valid for first/last
    emitter.instruction("b.gt __rt_strtotime_fail");                            // "second day of ..." → fail
    emitter.instruction("mov x9, #-1");                                         // wd = -1 marks day-of-month mode
    emitter.instruction("str x9, [sp, #80]");                                   // stash the sentinel
    emitter.instruction("add x3, x3, x10");                                     // advance past "day"
    emitter.instruction("b __rt_strtotime_fl_tail");                            // parse the shared "of <mod> month"

    emitter.label("__rt_strtotime_fl_neither");
    emitter.instruction("ldr x9, [sp, #88]");                                   // ordinal n
    emitter.instruction("cmp x9, #0");                                          // was the leading word "last" ?
    emitter.instruction("b.ne __rt_strtotime_fail");                            // first/2nd/.. without day/weekday → fail
    emitter.instruction("mov x9, #7");                                          // restore weekday-modifier kind for "last"
    emitter.instruction("mov x10, #4");                                         // consumed length of "last"
    emitter.instruction("b __rt_strtotime_weekdays_entry");                     // fall back to "last <weekday>"

    // -- "of" parse: day path requires it; weekday path may bare-fall-back to the weekday strategy --
    emitter.label("__rt_strtotime_fl_tail");
    emitter.instruction("bl __rt_strtotime_skip_ws");                           // skip whitespace before "of"
    emitter.instruction("bl __rt_strtotime_fl_token");                          // x9 = kind, x10 = consumed
    emitter.instruction("cmp x9, #1");                                          // is the word "of" ?
    emitter.instruction("b.ne __rt_strtotime_fail");                            // day path requires "of" → fail
    emitter.instruction("add x3, x3, x10");                                     // advance past "of"
    emitter.instruction("b __rt_strtotime_fl_modmonth");                        // parse the modifier + "month"
    emitter.label("__rt_strtotime_fl_weekday_tail");
    emitter.instruction("bl __rt_strtotime_skip_ws");                           // skip whitespace before "of"
    emitter.instruction("bl __rt_strtotime_fl_token");                          // x9 = kind, x10 = consumed
    emitter.instruction("cmp x9, #1");                                          // is the word "of" ?
    emitter.instruction("b.eq __rt_strtotime_fl_wd_of");                        // "of" → nth-weekday of month
    emitter.instruction("ldr x9, [sp, #88]");                                   // ordinal n
    emitter.instruction("cbz x9, __rt_strtotime_fl_wd_fallback");               // bare "last <weekday>" → fall back
    emitter.instruction("b __rt_strtotime_fail");                               // bare "first/second.. <weekday>" → unsupported
    emitter.label("__rt_strtotime_fl_wd_fallback");
    emitter.instruction("mov x9, #7");                                          // restore weekday-modifier kind for "last"
    emitter.instruction("mov x10, #4");                                         // consumed length of "last"
    emitter.instruction("b __rt_strtotime_weekdays_entry");                     // fall back to "last <weekday>"
    emitter.label("__rt_strtotime_fl_wd_of");
    emitter.instruction("add x3, x3, x10");                                     // advance past "of"
    emitter.label("__rt_strtotime_fl_modmonth");
    emitter.instruction("bl __rt_strtotime_skip_ws");                           // skip whitespace before the modifier
    emitter.instruction("bl __rt_strtotime_fl_token");                          // x9 = kind, x10 = consumed
    emitter.instruction("cmp x9, #3");                                          // modifier kinds start at 3
    emitter.instruction("b.lt __rt_strtotime_fail");                            // not a modifier → fail
    emitter.instruction("cmp x9, #5");                                          // modifier kinds end at 5
    emitter.instruction("b.gt __rt_strtotime_fail");                            // not a modifier → fail
    emitter.instruction("mov x6, #0");                                          // delta default (this)
    emitter.instruction("cmp x9, #4");                                          // "next" ?
    emitter.instruction("b.ne __rt_strtotime_fl_mod_chk5");                     // no → check "last/previous"
    emitter.instruction("mov x6, #1");                                          // next → +1 month
    emitter.instruction("b __rt_strtotime_fl_mod_done");                        // delta resolved
    emitter.label("__rt_strtotime_fl_mod_chk5");
    emitter.instruction("cmp x9, #5");                                          // "last"/"previous"/"prev" ?
    emitter.instruction("b.ne __rt_strtotime_fl_mod_done");                     // "this" keeps delta 0
    emitter.instruction("mov x6, #-1");                                         // last/previous → -1 month
    emitter.label("__rt_strtotime_fl_mod_done");
    emitter.instruction("str x6, [sp, #104]");                                  // stash delta (helpers/now_tm clobber x6)
    emitter.instruction("add x3, x3, x10");                                     // advance past the modifier
    emitter.instruction("bl __rt_strtotime_skip_ws");                           // skip whitespace before "month"
    emitter.instruction("bl __rt_strtotime_fl_token");                          // x9 = kind, x10 = consumed
    emitter.instruction("cmp x9, #2");                                          // is the word "month" ?
    emitter.instruction("b.ne __rt_strtotime_fail");                            // malformed phrase → fail
    emitter.instruction("add x3, x3, x10");                                     // advance past "month"
    emitter.instruction("bl __rt_strtotime_skip_ws");                           // skip any trailing whitespace
    emitter.instruction("cmp x3, x4");                                          // must be fully consumed now
    emitter.instruction("b.lt __rt_strtotime_fail");                            // trailing junk → fail
    emitter.instruction("ldr x10, [sp, #80]");                                  // wd sentinel
    emitter.instruction("cmp x10, #0");                                         // wd < 0 → day-of-month mode
    emitter.instruction("b.lt __rt_strtotime_fl_day_compute");                  // → day-of-month compute
    emitter.instruction("b __rt_strtotime_fl_nthwd_compute");                   // → nth-weekday compute

    // -- day-of-month compute (preserves the base time of day) --
    emitter.label("__rt_strtotime_fl_day_compute");
    emitter.instruction("bl __rt_strtotime_now_tm");                            // fill [sp+0..36] from the clock
    emitter.instruction("ldr x6, [sp, #104]");                                  // reload month delta
    emitter.instruction("ldr w9, [sp, #16]");                                   // base tm_mon
    emitter.instruction("add w9, w9, w6");                                      // apply the month delta
    emitter.instruction("ldr x10, [sp, #88]");                                  // ordinal n (0 = last, 1 = first)
    emitter.instruction("cmp x10, #1");                                         // first ?
    emitter.instruction("b.eq __rt_strtotime_fl_day_first");                    // yes → day = 1
    emitter.instruction("add w9, w9, #1");                                      // last: target month + 1 ...
    emitter.instruction("str w9, [sp, #16]");                                   // ... store tm_mon
    emitter.instruction("str wzr, [sp, #12]");                                  // tm_mday = 0 → last day of target month
    emitter.instruction("b __rt_strtotime_fl_mktime");                          // normalize via mktime
    emitter.label("__rt_strtotime_fl_day_first");
    emitter.instruction("str w9, [sp, #16]");                                   // store tm_mon
    emitter.instruction("mov w11, #1");                                         // first day = 1
    emitter.instruction("str w11, [sp, #12]");                                  // tm_mday = 1
    emitter.label("__rt_strtotime_fl_mktime");
    emitter.instruction("mov x0, sp");                                          // x0 = &tm
    emitter.instruction("bl __rt_mktime_shifted");                              // mktime(&tm) → x0 = timestamp
    emitter.instruction("b __rt_strtotime_ret");                                // return through shared epilogue

    // -- nth-weekday compute (resets time to midnight) --
    emitter.label("__rt_strtotime_fl_nthwd_compute");
    emitter.instruction("bl __rt_strtotime_now_tm");                            // fill [sp+0..36] from the clock
    emitter.instruction("ldr x6, [sp, #104]");                                  // reload month delta
    emitter.instruction("ldr w9, [sp, #16]");                                   // base tm_mon
    emitter.instruction("add w9, w9, w6");                                      // target tm_mon
    emitter.instruction("str wzr, [sp, #0]");                                   // tm_sec = 0 (midnight)
    emitter.instruction("str wzr, [sp, #4]");                                   // tm_min = 0
    emitter.instruction("str wzr, [sp, #8]");                                   // tm_hour = 0
    emitter.instruction("ldr x11, [sp, #88]");                                  // ordinal n
    emitter.instruction("cbz x11, __rt_strtotime_fl_nthwd_last");               // n = 0 → "last <weekday>"
    // -- first..fifth: reference weekday is day 1 of the target month --
    emitter.instruction("str w9, [sp, #16]");                                   // tm_mon = target
    emitter.instruction("mov w12, #1");                                         // day 1
    emitter.instruction("str w12, [sp, #12]");                                  // tm_mday = 1
    emitter.instruction("mov w13, #-1");                                        // tm_isdst sentinel
    emitter.instruction("str w13, [sp, #32]");                                  // tm_isdst = -1
    emitter.instruction("mov x0, sp");                                          // x0 = &tm
    emitter.instruction("bl __rt_mktime_shifted");                              // normalize; tm_wday now valid
    emitter.instruction("ldr w12, [sp, #24]");                                  // tm_wday of day 1
    emitter.instruction("ldr w14, [sp, #80]");                                  // target weekday
    emitter.instruction("sub w15, w14, w12");                                   // wd - wday1
    emitter.instruction("add w15, w15, #7");                                    // bias positive before mod 7
    emitter.instruction("mov w16, #7");                                         // modulus
    emitter.instruction("udiv w17, w15, w16");                                  // w15 / 7
    emitter.instruction("msub w15, w17, w16, w15");                             // offset = (wd - wday1 + 7) mod 7
    emitter.instruction("add w15, w15, #1");                                    // first occurrence day = 1 + offset
    emitter.instruction("ldr w11, [sp, #88]");                                  // ordinal n
    emitter.instruction("sub w11, w11, #1");                                    // (n - 1)
    emitter.instruction("mov w16, #7");                                         // a week
    emitter.instruction("mul w11, w11, w16");                                   // (n - 1) * 7
    emitter.instruction("add w15, w15, w11");                                   // nth occurrence day
    emitter.instruction("str w15, [sp, #12]");                                  // tm_mday = nth occurrence (mktime normalizes)
    emitter.instruction("mov w13, #-1");                                        // tm_isdst sentinel
    emitter.instruction("str w13, [sp, #32]");                                  // tm_isdst = -1
    emitter.instruction("mov x0, sp");                                          // x0 = &tm
    emitter.instruction("bl __rt_mktime_shifted");                              // mktime(&tm) → x0 = timestamp
    emitter.instruction("b __rt_strtotime_ret");                                // return through shared epilogue
    // -- last <weekday>: reference weekday is the last day of the target month --
    emitter.label("__rt_strtotime_fl_nthwd_last");
    emitter.instruction("add w9, w9, #1");                                      // target month + 1 ...
    emitter.instruction("str w9, [sp, #16]");                                   // ... store tm_mon
    emitter.instruction("str wzr, [sp, #12]");                                  // tm_mday = 0 → last day of target month
    emitter.instruction("mov w13, #-1");                                        // tm_isdst sentinel
    emitter.instruction("str w13, [sp, #32]");                                  // tm_isdst = -1
    emitter.instruction("mov x0, sp");                                          // x0 = &tm
    emitter.instruction("bl __rt_mktime_shifted");                              // normalize; tm_mday/tm_wday now valid
    emitter.instruction("ldr w12, [sp, #24]");                                  // tm_wday of the last day
    emitter.instruction("ldr w14, [sp, #80]");                                  // target weekday
    emitter.instruction("sub w15, w12, w14");                                   // wday_last - wd
    emitter.instruction("add w15, w15, #7");                                    // bias positive before mod 7
    emitter.instruction("mov w16, #7");                                         // modulus
    emitter.instruction("udiv w17, w15, w16");                                  // w15 / 7
    emitter.instruction("msub w15, w17, w16, w15");                             // offset = (wday_last - wd + 7) mod 7
    emitter.instruction("ldr w13, [sp, #12]");                                  // last day-of-month number
    emitter.instruction("sub w13, w13, w15");                                   // last occurrence day = lastday - offset
    emitter.instruction("str w13, [sp, #12]");                                  // tm_mday = last occurrence
    emitter.instruction("mov w16, #-1");                                        // tm_isdst sentinel
    emitter.instruction("str w16, [sp, #32]");                                  // tm_isdst = -1
    emitter.instruction("mov x0, sp");                                          // x0 = &tm
    emitter.instruction("bl __rt_mktime_shifted");                              // mktime(&tm) → x0 = timestamp
    emitter.instruction("b __rt_strtotime_ret");                                // return through shared epilogue

    // -- helper: match a phrase token against the first/last table (LR parked in x15) --
    emitter.label("__rt_strtotime_fl_token");
    emitter.instruction("mov x15, x30");                                        // park the return address (lc16 is sp-relative)
    emitter.instruction("bl __rt_strtotime_lc_cursor");                         // lowercase 16 bytes from the cursor into lc16
    emitter.instruction("add x6, sp, #64");                                     // candidate = lc16 buffer
    emitter.adrp("x7", "_strtotime_firstlast_tab");                             // phrase-token table page
    emitter.add_lo12("x7", "x7", "_strtotime_firstlast_tab");                   // resolve table address
    emitter.instruction("sub x8, x4, x3");                                      // remaining bytes
    emitter.instruction("mov x11, #16");                                        // capped to lc16 width
    emitter.instruction("cmp x8, x11");                                         // remaining > 16 ?
    emitter.instruction("csel x8, x8, x11, lo");                                // x8 = min(remaining, 16)
    emitter.instruction("bl __rt_strtotime_match_word");                        // x9 = kind, x10 = consumed
    emitter.instruction("mov x30, x15");                                        // restore the return address
    emitter.instruction("ret");                                                 // return to the parser

    // -- helper: match a weekday against the keyword table (LR parked in x15) --
    emitter.label("__rt_strtotime_fl_kw_token");
    emitter.instruction("mov x15, x30");                                        // park the return address (lc16 is sp-relative)
    emitter.instruction("bl __rt_strtotime_lc_cursor");                         // lowercase 16 bytes from the cursor into lc16
    emitter.instruction("add x6, sp, #64");                                     // candidate = lc16 buffer
    emitter.adrp("x7", "_strtotime_keyword_tab");                               // keyword table page
    emitter.add_lo12("x7", "x7", "_strtotime_keyword_tab");                     // resolve table address
    emitter.instruction("sub x8, x4, x3");                                      // remaining bytes
    emitter.instruction("mov x11, #16");                                        // capped to lc16 width
    emitter.instruction("cmp x8, x11");                                         // remaining > 16 ?
    emitter.instruction("csel x8, x8, x11, lo");                                // x8 = min(remaining, 16)
    emitter.instruction("bl __rt_strtotime_match_word");                        // x9 = kind, x10 = consumed
    emitter.instruction("mov x30, x15");                                        // restore the return address
    emitter.instruction("ret");                                                 // return to the parser
}

/// Emits x86_64 (Linux) assembly for the ordinal date phrases. The cursor is parked in `[rsp+96]`
/// across the token helpers (whose `match_word` clobbers `rdi`/`r10`); the end pointer is
/// recomputed from `[rsp+48]`/`[rsp+56]` afterwards. Token kind is returned in `rdx`, consumed
/// bytes in `rax`.
fn emit_first_last_day_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime: ordinal date (day-of-month / nth-weekday) sub-routine ---");
    emitter.label("__rt_strtotime_firstlast_entry_linux_x86_64");

    // -- leading ordinal: kind 7 = "last" (n=0); kinds 31..35 = first..fifth (n=1..5) --
    emitter.instruction("cmp rdx, 7");                                          // "last" ordinal ?
    emitter.instruction("jne __rt_strtotime_fl_ord_linux_x86_64");              // otherwise first..fifth
    emitter.instruction("mov r11d, 0");                                         // n = 0 marks "last"
    emitter.instruction("jmp __rt_strtotime_fl_ord_done_linux_x86_64");         // ordinal resolved
    emitter.label("__rt_strtotime_fl_ord_linux_x86_64");
    emitter.instruction("lea r11d, [rdx - 30]");                                // 31..35 → 1..5
    emitter.label("__rt_strtotime_fl_ord_done_linux_x86_64");
    emitter.instruction("mov DWORD PTR [rsp + 88], r11d");                      // stash n
    emitter.instruction("mov rdi, QWORD PTR [rsp + 48]");                       // cursor = base ...
    emitter.instruction("add rdi, rax");                                        // ... past the leading ordinal word
    emitter.instruction("mov r10, QWORD PTR [rsp + 48]");                       // end = base ...
    emitter.instruction("add r10, QWORD PTR [rsp + 56]");                       // ... + length
    emitter.instruction("call __rt_strtotime_skip_ws_linux_x86_64");            // skip whitespace before day/weekday

    // -- second word: "day" (kind 0) or a weekday name (kinds 10-16) --
    emitter.instruction("mov QWORD PTR [rsp + 96], rdi");                       // park cursor across the token match
    emitter.instruction("call __rt_strtotime_fl_token_linux_x86_64");           // rax = consumed, rdx = kind
    emitter.instruction("cmp rdx, 0");                                          // is the word "day" ?
    emitter.instruction("je __rt_strtotime_fl_set_day_linux_x86_64");           // yes → day-of-month mode
    emitter.instruction("call __rt_strtotime_fl_kw_token_linux_x86_64");        // try a weekday (keyword table)
    emitter.instruction("cmp rdx, 10");                                         // weekday kinds start at 10
    emitter.instruction("jl __rt_strtotime_fl_neither_linux_x86_64");           // not a weekday → neither
    emitter.instruction("cmp rdx, 16");                                         // weekday kinds end at 16
    emitter.instruction("jg __rt_strtotime_fl_neither_linux_x86_64");           // not a weekday → neither
    emitter.instruction("sub edx, 10");                                         // target weekday 0..6 (Sun..Sat)
    emitter.instruction("mov DWORD PTR [rsp + 80], edx");                       // stash target weekday
    emitter.instruction("mov rdi, QWORD PTR [rsp + 96]");                       // restore cursor ...
    emitter.instruction("add rdi, rax");                                        // ... advance past the weekday name
    emitter.instruction("mov r10, QWORD PTR [rsp + 48]");                       // recompute end = base ...
    emitter.instruction("add r10, QWORD PTR [rsp + 56]");                       // ... + length
    emitter.instruction("jmp __rt_strtotime_fl_weekday_tail_linux_x86_64");     // require "of month" or bare-fall-back

    emitter.label("__rt_strtotime_fl_set_day_linux_x86_64");
    emitter.instruction("mov r11d, DWORD PTR [rsp + 88]");                      // ordinal n
    emitter.instruction("cmp r11d, 1");                                         // "day" is only valid for first/last
    emitter.instruction("jg __rt_strtotime_fail_linux_x86_64");                 // "second day of ..." → fail
    emitter.instruction("mov DWORD PTR [rsp + 80], -1");                        // wd = -1 marks day-of-month mode
    emitter.instruction("mov rdi, QWORD PTR [rsp + 96]");                       // restore cursor ...
    emitter.instruction("add rdi, rax");                                        // ... advance past "day"
    emitter.instruction("mov r10, QWORD PTR [rsp + 48]");                       // recompute end = base ...
    emitter.instruction("add r10, QWORD PTR [rsp + 56]");                       // ... + length
    emitter.instruction("jmp __rt_strtotime_fl_tail_linux_x86_64");             // parse the shared "of <mod> month"

    emitter.label("__rt_strtotime_fl_neither_linux_x86_64");
    emitter.instruction("mov r11d, DWORD PTR [rsp + 88]");                      // ordinal n
    emitter.instruction("cmp r11d, 0");                                         // was the leading word "last" ?
    emitter.instruction("jne __rt_strtotime_fail_linux_x86_64");                // first/2nd/.. without day/weekday → fail
    emitter.instruction("mov rdx, 7");                                          // restore weekday-modifier kind for "last"
    emitter.instruction("mov rax, 4");                                          // consumed length of "last"
    emitter.instruction("jmp __rt_strtotime_weekdays_entry_linux_x86_64");      // fall back to "last <weekday>"

    // -- "of" parse: day path requires it; weekday path may bare-fall-back to the weekday strategy --
    emitter.label("__rt_strtotime_fl_tail_linux_x86_64");
    emitter.instruction("call __rt_strtotime_skip_ws_linux_x86_64");            // skip whitespace before "of"
    emitter.instruction("mov QWORD PTR [rsp + 96], rdi");                       // park cursor across the token match
    emitter.instruction("call __rt_strtotime_fl_token_linux_x86_64");           // rax = consumed, rdx = kind
    emitter.instruction("cmp rdx, 1");                                          // is the word "of" ?
    emitter.instruction("jne __rt_strtotime_fail_linux_x86_64");                // day path requires "of" → fail
    emitter.instruction("mov rdi, QWORD PTR [rsp + 96]");                       // restore cursor ...
    emitter.instruction("add rdi, rax");                                        // ... advance past "of"
    emitter.instruction("mov r10, QWORD PTR [rsp + 48]");                       // recompute end = base ...
    emitter.instruction("add r10, QWORD PTR [rsp + 56]");                       // ... + length
    emitter.instruction("jmp __rt_strtotime_fl_modmonth_linux_x86_64");         // parse the modifier + "month"
    emitter.label("__rt_strtotime_fl_weekday_tail_linux_x86_64");
    emitter.instruction("call __rt_strtotime_skip_ws_linux_x86_64");            // skip whitespace before "of"
    emitter.instruction("mov QWORD PTR [rsp + 96], rdi");                       // park cursor across the token match
    emitter.instruction("call __rt_strtotime_fl_token_linux_x86_64");           // rax = consumed, rdx = kind
    emitter.instruction("cmp rdx, 1");                                          // is the word "of" ?
    emitter.instruction("je __rt_strtotime_fl_wd_of_linux_x86_64");             // "of" → nth-weekday of month
    emitter.instruction("mov r11d, DWORD PTR [rsp + 88]");                      // ordinal n
    emitter.instruction("test r11d, r11d");                                     // bare "last <weekday>" ?
    emitter.instruction("jz __rt_strtotime_fl_wd_fallback_linux_x86_64");       // yes → fall back
    emitter.instruction("jmp __rt_strtotime_fail_linux_x86_64");                // bare "first/second.. <weekday>" → unsupported
    emitter.label("__rt_strtotime_fl_wd_fallback_linux_x86_64");
    emitter.instruction("mov rdx, 7");                                          // restore weekday-modifier kind for "last"
    emitter.instruction("mov rax, 4");                                          // consumed length of "last"
    emitter.instruction("jmp __rt_strtotime_weekdays_entry_linux_x86_64");      // fall back to "last <weekday>"
    emitter.label("__rt_strtotime_fl_wd_of_linux_x86_64");
    emitter.instruction("mov rdi, QWORD PTR [rsp + 96]");                       // restore cursor ...
    emitter.instruction("add rdi, rax");                                        // ... advance past "of"
    emitter.instruction("mov r10, QWORD PTR [rsp + 48]");                       // recompute end = base ...
    emitter.instruction("add r10, QWORD PTR [rsp + 56]");                       // ... + length
    emitter.label("__rt_strtotime_fl_modmonth_linux_x86_64");
    emitter.instruction("call __rt_strtotime_skip_ws_linux_x86_64");            // skip whitespace before the modifier
    emitter.instruction("mov QWORD PTR [rsp + 96], rdi");                       // park cursor across the token match
    emitter.instruction("call __rt_strtotime_fl_token_linux_x86_64");           // rax = consumed, rdx = kind
    emitter.instruction("cmp rdx, 3");                                          // modifier kinds start at 3
    emitter.instruction("jl __rt_strtotime_fail_linux_x86_64");                 // not a modifier → fail
    emitter.instruction("cmp rdx, 5");                                          // modifier kinds end at 5
    emitter.instruction("jg __rt_strtotime_fail_linux_x86_64");                 // not a modifier → fail
    emitter.instruction("mov r8d, 0");                                          // delta default (this)
    emitter.instruction("cmp rdx, 4");                                          // "next" ?
    emitter.instruction("jne __rt_strtotime_fl_mod_chk5_linux_x86_64");         // no → check "last/previous"
    emitter.instruction("mov r8d, 1");                                          // next → +1 month
    emitter.instruction("jmp __rt_strtotime_fl_mod_done_linux_x86_64");         // delta resolved
    emitter.label("__rt_strtotime_fl_mod_chk5_linux_x86_64");
    emitter.instruction("cmp rdx, 5");                                          // "last"/"previous"/"prev" ?
    emitter.instruction("jne __rt_strtotime_fl_mod_done_linux_x86_64");         // "this" keeps delta 0
    emitter.instruction("mov r8d, -1");                                         // last/previous → -1 month
    emitter.label("__rt_strtotime_fl_mod_done_linux_x86_64");
    emitter.instruction("mov DWORD PTR [rsp + 104], r8d");                      // stash delta
    emitter.instruction("mov rdi, QWORD PTR [rsp + 96]");                       // restore cursor ...
    emitter.instruction("add rdi, rax");                                        // ... advance past the modifier
    emitter.instruction("mov r10, QWORD PTR [rsp + 48]");                       // recompute end = base ...
    emitter.instruction("add r10, QWORD PTR [rsp + 56]");                       // ... + length
    emitter.instruction("call __rt_strtotime_skip_ws_linux_x86_64");            // skip whitespace before "month"
    emitter.instruction("mov QWORD PTR [rsp + 96], rdi");                       // park cursor across the token match
    emitter.instruction("call __rt_strtotime_fl_token_linux_x86_64");           // rax = consumed, rdx = kind
    emitter.instruction("cmp rdx, 2");                                          // is the word "month" ?
    emitter.instruction("jne __rt_strtotime_fail_linux_x86_64");                // malformed phrase → fail
    emitter.instruction("mov rdi, QWORD PTR [rsp + 96]");                       // restore cursor ...
    emitter.instruction("add rdi, rax");                                        // ... advance past "month"
    emitter.instruction("mov r10, QWORD PTR [rsp + 48]");                       // recompute end = base ...
    emitter.instruction("add r10, QWORD PTR [rsp + 56]");                       // ... + length
    emitter.instruction("call __rt_strtotime_skip_ws_linux_x86_64");            // skip any trailing whitespace
    emitter.instruction("cmp rdi, r10");                                        // must be fully consumed now
    emitter.instruction("jb __rt_strtotime_fail_linux_x86_64");                 // trailing junk → fail
    emitter.instruction("mov r10d, DWORD PTR [rsp + 80]");                      // wd sentinel
    emitter.instruction("cmp r10d, 0");                                         // wd < 0 → day-of-month mode
    emitter.instruction("jl __rt_strtotime_fl_day_compute_linux_x86_64");       // → day-of-month compute

    // -- nth-weekday compute (resets time to midnight) --
    emitter.instruction("call __rt_strtotime_now_tm_linux_x86_64");             // fill [rsp+0..36] from the clock
    emitter.instruction("mov r8d, DWORD PTR [rsp + 104]");                      // reload month delta
    emitter.instruction("mov eax, DWORD PTR [rsp + 16]");                       // base tm_mon
    emitter.instruction("add eax, r8d");                                        // target tm_mon
    emitter.instruction("mov DWORD PTR [rsp + 0], 0");                          // tm_sec = 0 (midnight)
    emitter.instruction("mov DWORD PTR [rsp + 4], 0");                          // tm_min = 0
    emitter.instruction("mov DWORD PTR [rsp + 8], 0");                          // tm_hour = 0
    emitter.instruction("mov r11d, DWORD PTR [rsp + 88]");                      // ordinal n
    emitter.instruction("test r11d, r11d");                                     // n = 0 → "last <weekday>"
    emitter.instruction("jz __rt_strtotime_fl_nthwd_last_linux_x86_64");        // → last-weekday path
    // -- first..fifth: reference weekday is day 1 of the target month --
    emitter.instruction("mov DWORD PTR [rsp + 16], eax");                       // tm_mon = target
    emitter.instruction("mov DWORD PTR [rsp + 12], 1");                         // tm_mday = 1
    emitter.instruction("mov DWORD PTR [rsp + 32], -1");                        // tm_isdst = -1
    emitter.instruction("mov rdi, rsp");                                        // rdi = &tm
    emitter.instruction("call __rt_mktime_shifted");                            // normalize; tm_wday now valid
    emitter.instruction("mov ecx, DWORD PTR [rsp + 24]");                       // tm_wday of day 1
    emitter.instruction("mov eax, DWORD PTR [rsp + 80]");                       // target weekday
    emitter.instruction("sub eax, ecx");                                        // wd - wday1
    emitter.instruction("add eax, 7");                                          // bias positive before mod 7
    emitter.instruction("xor edx, edx");                                        // clear high half before dividing
    emitter.instruction("mov ecx, 7");                                          // modulus
    emitter.instruction("div ecx");                                             // edx = (wd - wday1 + 7) mod 7
    emitter.instruction("lea eax, [rdx + 1]");                                  // first occurrence day = 1 + offset
    emitter.instruction("mov r11d, DWORD PTR [rsp + 88]");                      // ordinal n
    emitter.instruction("sub r11d, 1");                                         // (n - 1)
    emitter.instruction("imul r11d, r11d, 7");                                  // (n - 1) * 7
    emitter.instruction("add eax, r11d");                                       // nth occurrence day
    emitter.instruction("mov DWORD PTR [rsp + 12], eax");                       // tm_mday = nth occurrence (mktime normalizes)
    emitter.instruction("mov DWORD PTR [rsp + 32], -1");                        // tm_isdst = -1
    emitter.instruction("mov rdi, rsp");                                        // rdi = &tm
    emitter.instruction("call __rt_mktime_shifted");                            // mktime(&tm) → rax = timestamp
    emitter.instruction("jmp __rt_strtotime_ret_linux_x86_64");                 // return through shared epilogue
    // -- last <weekday>: reference weekday is the last day of the target month --
    emitter.label("__rt_strtotime_fl_nthwd_last_linux_x86_64");
    emitter.instruction("add eax, 1");                                          // target month + 1 ...
    emitter.instruction("mov DWORD PTR [rsp + 16], eax");                       // ... store tm_mon
    emitter.instruction("mov DWORD PTR [rsp + 12], 0");                         // tm_mday = 0 → last day of target month
    emitter.instruction("mov DWORD PTR [rsp + 32], -1");                        // tm_isdst = -1
    emitter.instruction("mov rdi, rsp");                                        // rdi = &tm
    emitter.instruction("call __rt_mktime_shifted");                            // normalize; tm_mday/tm_wday now valid
    emitter.instruction("mov ecx, DWORD PTR [rsp + 24]");                       // tm_wday of the last day
    emitter.instruction("mov eax, ecx");                                        // wday_last ...
    emitter.instruction("sub eax, DWORD PTR [rsp + 80]");                       // ... - wd
    emitter.instruction("add eax, 7");                                          // bias positive before mod 7
    emitter.instruction("xor edx, edx");                                        // clear high half before dividing
    emitter.instruction("mov ecx, 7");                                          // modulus
    emitter.instruction("div ecx");                                             // edx = (wday_last - wd + 7) mod 7
    emitter.instruction("mov eax, DWORD PTR [rsp + 12]");                       // last day-of-month number
    emitter.instruction("sub eax, edx");                                        // last occurrence day = lastday - offset
    emitter.instruction("mov DWORD PTR [rsp + 12], eax");                       // tm_mday = last occurrence
    emitter.instruction("mov DWORD PTR [rsp + 32], -1");                        // tm_isdst = -1
    emitter.instruction("mov rdi, rsp");                                        // rdi = &tm
    emitter.instruction("call __rt_mktime_shifted");                            // mktime(&tm) → rax = timestamp
    emitter.instruction("jmp __rt_strtotime_ret_linux_x86_64");                 // return through shared epilogue

    // -- day-of-month compute (preserves the base time of day) --
    emitter.label("__rt_strtotime_fl_day_compute_linux_x86_64");
    emitter.instruction("call __rt_strtotime_now_tm_linux_x86_64");             // fill [rsp+0..36] from the clock
    emitter.instruction("mov r8d, DWORD PTR [rsp + 104]");                      // reload month delta
    emitter.instruction("mov eax, DWORD PTR [rsp + 16]");                       // base tm_mon
    emitter.instruction("add eax, r8d");                                        // apply the month delta
    emitter.instruction("mov r11d, DWORD PTR [rsp + 88]");                      // ordinal n (0 = last, 1 = first)
    emitter.instruction("cmp r11d, 1");                                         // first ?
    emitter.instruction("je __rt_strtotime_fl_day_first_linux_x86_64");         // yes → day = 1
    emitter.instruction("add eax, 1");                                          // last: target month + 1 ...
    emitter.instruction("mov DWORD PTR [rsp + 16], eax");                       // ... store tm_mon
    emitter.instruction("mov DWORD PTR [rsp + 12], 0");                         // tm_mday = 0 → last day of target month
    emitter.instruction("jmp __rt_strtotime_fl_mktime_linux_x86_64");           // normalize via mktime
    emitter.label("__rt_strtotime_fl_day_first_linux_x86_64");
    emitter.instruction("mov DWORD PTR [rsp + 16], eax");                       // store tm_mon
    emitter.instruction("mov DWORD PTR [rsp + 12], 1");                         // tm_mday = 1
    emitter.label("__rt_strtotime_fl_mktime_linux_x86_64");
    emitter.instruction("mov rdi, rsp");                                        // rdi = &tm
    emitter.instruction("call __rt_mktime_shifted");                            // mktime(&tm) → rax = timestamp
    emitter.instruction("jmp __rt_strtotime_ret_linux_x86_64");                 // return through shared epilogue

    // -- helper: match a phrase token against the first/last table --
    emitter.label("__rt_strtotime_fl_token_linux_x86_64");
    emitter.instruction("call __rt_strtotime_lc_cursor_linux_x86_64");          // lowercase 16 bytes from the cursor into lc16
    emitter.instruction("mov rcx, r10");                                        // remaining = end ...
    emitter.instruction("sub rcx, rdi");                                        // ... - cursor
    emitter.instruction("cmp rcx, 16");                                         // remaining > 16 ?
    emitter.instruction("jbe __rt_strtotime_fl_token_avail_linux_x86_64");      // keep remaining when <= 16
    emitter.instruction("mov rcx, 16");                                         // cap available to lc16 width
    emitter.label("__rt_strtotime_fl_token_avail_linux_x86_64");
    emitter.instruction("lea rdi, [rbp - 64]");                                 // candidate = lc16 buffer (rbp-relative)
    emitter.instruction("lea rsi, [rip + _strtotime_firstlast_tab]");           // phrase-token table
    emitter.instruction("call __rt_strtotime_match_word_linux_x86_64");         // rax = consumed, rdx = kind
    emitter.instruction("ret");                                                 // return to the parser

    // -- helper: match a weekday against the keyword table --
    // Called right after fl_token (which clobbered rdi/r10): reload the parked cursor first.
    // Inside this `call` frame rsp is shifted, so address the dispatcher slots rbp-relative:
    // [rsp+96]=[rbp-32] (parked cursor), [rsp+48]=[rbp-80] (ptr), [rsp+56]=[rbp-72] (len).
    emitter.label("__rt_strtotime_fl_kw_token_linux_x86_64");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the parked cursor
    emitter.instruction("mov r10, QWORD PTR [rbp - 80]");                       // recompute end = base ...
    emitter.instruction("add r10, QWORD PTR [rbp - 72]");                       // ... + length
    emitter.instruction("call __rt_strtotime_lc_cursor_linux_x86_64");          // lowercase 16 bytes from the cursor into lc16
    emitter.instruction("mov rcx, r10");                                        // remaining = end ...
    emitter.instruction("sub rcx, rdi");                                        // ... - cursor
    emitter.instruction("cmp rcx, 16");                                         // remaining > 16 ?
    emitter.instruction("jbe __rt_strtotime_fl_kw_token_avail_linux_x86_64");   // keep remaining when <= 16
    emitter.instruction("mov rcx, 16");                                         // cap available to lc16 width
    emitter.label("__rt_strtotime_fl_kw_token_avail_linux_x86_64");
    emitter.instruction("lea rdi, [rbp - 64]");                                 // candidate = lc16 buffer (rbp-relative)
    emitter.instruction("lea rsi, [rip + _strtotime_keyword_tab]");             // keyword table
    emitter.instruction("call __rt_strtotime_match_word_linux_x86_64");         // rax = consumed, rdx = kind
    emitter.instruction("ret");                                                 // return to the parser
}
