//! Purpose:
//! Emits the textual-date parser sub-routine for the `__rt_strtotime` dispatcher.
//! Accepts `D Month Y`, `Month D[,] Y` (full or abbreviated month names), with an optional
//! `HH:MM[:SS]` time suffix.
//!
//! Called from:
//! - `crate::codegen::runtime::system::strtotime::mod::emit_strtotime()`: the dispatcher routes
//!   here when an alpha first word matches a month name (kinds 19-30), and reroutes the digit
//!   branch's offset-bound cases here (day-first) — the day-first path falls back to the offsets
//!   strategy when the word after the number is not a month.
//!
//! Key details:
//! - Entry label `__rt_strtotime_textual_entry` (ARM64) / `_linux_x86_64` (x86_64); the dispatcher
//!   frame is set up, trimmed ptr at `[sp+48]`, trimmed len at `[sp+56]`, lc16 at `[sp+64]`.
//! - Reuses the shared cursor helpers `__rt_strtotime_parse_dec`, `__rt_strtotime_skip_ws`,
//!   `__rt_strtotime_lc_cursor`, and `__rt_strtotime_match_word`. Month names live in the
//!   keyword table (kinds 19-30 → January..December); day/year/time are parsed by `parse_dec`,
//!   which lets `mktime` normalize out-of-range days (e.g. "31 feb" → Mar 2), matching PHP.
//! - A year value <= 69 windows to 2000s, 70-99 to 1900s, else literal. Exits via the shared
//!   `__rt_strtotime_ret` / `__rt_strtotime_fail` (or branches to the offsets strategy on
//!   day-first fallback).

use crate::codegen::{emit::Emitter, platform::Arch};

/// Dispatches to the architecture-specific textual-date parser.
pub(crate) fn emit_textual_date(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_textual_date_linux_x86_64(emitter);
        return;
    }

    emit_textual_date_arm64(emitter);
}

/// Emits ARM64 assembly for the textual-date parser. Cursor in `x3`, end pointer in `x4`.
/// Month matching uses the lc16 buffer (start) for month-first or `lc_cursor` (mid-string) for
/// day-first; the shared `match_word` leaves `x3`/`x4` intact so they survive each call.
fn emit_textual_date_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime: textual-date sub-routine ---");
    emitter.label("__rt_strtotime_textual_entry");

    emitter.instruction("ldr x1, [sp, #48]");                                   // trimmed input pointer (base)
    emitter.instruction("ldr x2, [sp, #56]");                                   // trimmed input length
    emitter.instruction("add x4, x1, x2");                                      // x4 = end pointer
    emitter.instruction("ldrb w9, [x1]");                                       // first byte
    emitter.instruction("sub w10, w9, #48");                                    // digit test
    emitter.instruction("cmp w10, #9");                                         // is the first byte a digit?
    emitter.instruction("b.ls __rt_strtotime_textual_dayfirst");                // digit → "D Month Y" form

    // -- month-first: "Month D[,] Y" — month is at the start, already in lc16 --
    emitter.instruction("add x6, sp, #64");                                     // candidate = lc16 buffer
    emitter.adrp("x7", "_strtotime_keyword_tab");                               // month/keyword table page
    emitter.add_lo12("x7", "x7", "_strtotime_keyword_tab");                     // resolve table address
    emitter.instruction("mov x8, x2");                                          // available bytes = len
    emitter.instruction("mov x11, #16");                                        // capped to lc16 width
    emitter.instruction("cmp x8, x11");                                         // len > 16 ?
    emitter.instruction("csel x8, x8, x11, lo");                                // x8 = min(len, 16)
    emitter.instruction("bl __rt_strtotime_match_word");                        // x9 = kind, x10 = consumed
    emitter.instruction("cmp x9, #19");                                         // month kinds start at 19
    emitter.instruction("b.lt __rt_strtotime_fail");                            // not a month → fail
    emitter.instruction("cmp x9, #30");                                         // month kinds end at 30
    emitter.instruction("b.gt __rt_strtotime_fail");                            // not a month → fail
    emitter.instruction("sub x9, x9, #19");                                     // month index 0-11
    emitter.instruction("str w9, [sp, #16]");                                   // tm_mon
    emitter.instruction("add x3, x1, x10");                                     // cursor = base + consumed (past month)
    emitter.instruction("bl __rt_strtotime_skip_ws");                           // skip whitespace before the day
    emitter.instruction("mov x12, x3");                                         // remember cursor to detect "no digits"
    emitter.instruction("bl __rt_strtotime_parse_dec");                         // x5 = day
    emitter.instruction("cmp x3, x12");                                         // did the day consume any digit?
    emitter.instruction("b.eq __rt_strtotime_fail");                            // no day → fail
    emitter.instruction("str w5, [sp, #12]");                                   // tm_mday (mktime normalizes overflow)
    emitter.instruction("cmp x3, x4");                                          // need a year after the day
    emitter.instruction("b.ge __rt_strtotime_fail");                            // missing year → fail
    emitter.instruction("ldrb w9, [x3]");                                       // peek separator after the day
    emitter.instruction("cmp w9, #44");                                         // optional ',' ?
    emitter.instruction("b.ne __rt_strtotime_textual_mf_sp");                   // no comma → skip whitespace
    emitter.instruction("add x3, x3, #1");                                      // consume the ','
    emitter.label("__rt_strtotime_textual_mf_sp");
    emitter.instruction("bl __rt_strtotime_skip_ws");                           // skip whitespace before the year
    emitter.instruction("b __rt_strtotime_textual_year");                       // join the common year/time tail

    // -- day-first: "D Month Y" — parse the day, then require a month name --
    emitter.label("__rt_strtotime_textual_dayfirst");
    emitter.instruction("mov x3, x1");                                          // cursor = base
    emitter.instruction("mov x12, x3");                                         // remember cursor to detect "no digits"
    emitter.instruction("bl __rt_strtotime_parse_dec");                         // x5 = day
    emitter.instruction("cmp x3, x12");                                         // any leading digits?
    emitter.instruction("b.eq __rt_strtotime_offsets_entry");                   // none → let offsets try
    emitter.instruction("str w5, [sp, #12]");                                   // tm_mday (tentative)
    emitter.instruction("bl __rt_strtotime_skip_ws");                           // skip the whitespace after the number
    emitter.instruction("cmp x3, x4");                                          // bare number with no word?
    emitter.instruction("b.ge __rt_strtotime_offsets_entry");                   // yes → let offsets try
    emitter.instruction("bl __rt_strtotime_lc_cursor");                         // lowercase 16 bytes from the cursor into lc16
    emitter.instruction("add x6, sp, #64");                                     // candidate = lc16 buffer
    emitter.adrp("x7", "_strtotime_keyword_tab");                               // month/keyword table page
    emitter.add_lo12("x7", "x7", "_strtotime_keyword_tab");                     // resolve table address
    emitter.instruction("sub x8, x4, x3");                                      // remaining input bytes
    emitter.instruction("mov x11, #16");                                        // capped to lc16 width
    emitter.instruction("cmp x8, x11");                                         // remaining > 16 ?
    emitter.instruction("csel x8, x8, x11, lo");                                // x8 = min(remaining, 16)
    emitter.instruction("bl __rt_strtotime_match_word");                        // x9 = kind, x10 = consumed
    emitter.instruction("cmp x9, #19");                                         // is the word a month name?
    emitter.instruction("b.lt __rt_strtotime_offsets_entry");                   // not a month → it's a relative offset
    emitter.instruction("cmp x9, #30");                                         // month kinds end at 30
    emitter.instruction("b.gt __rt_strtotime_offsets_entry");                   // not a month → offsets fallback
    emitter.instruction("sub x9, x9, #19");                                     // month index 0-11
    emitter.instruction("str w9, [sp, #16]");                                   // tm_mon
    emitter.instruction("add x3, x3, x10");                                     // advance cursor past the month name
    emitter.instruction("bl __rt_strtotime_skip_ws");                           // skip whitespace before the year

    // -- common tail: year (with 2-digit windowing) + optional time + mktime --
    emitter.label("__rt_strtotime_textual_year");
    emitter.instruction("mov x12, x3");                                         // remember cursor to detect "no digits"
    emitter.instruction("bl __rt_strtotime_parse_dec");                         // x5 = year
    emitter.instruction("cmp x3, x12");                                         // any year digits?
    emitter.instruction("b.eq __rt_strtotime_fail");                            // missing year → fail
    emitter.instruction("cmp x5, #69");                                         // value <= 69 → 2000s
    emitter.instruction("b.gt __rt_strtotime_textual_y1900");                   // otherwise try the 1900s window
    emitter.instruction("add x5, x5, #2000");                                   // 0..69 → 2000..2069
    emitter.instruction("b __rt_strtotime_textual_ydone");                      // year resolved
    emitter.label("__rt_strtotime_textual_y1900");
    emitter.instruction("cmp x5, #99");                                         // value 70..99 → 1900s
    emitter.instruction("b.gt __rt_strtotime_textual_ydone");                   // >= 100 → literal year
    emitter.instruction("add x5, x5, #1900");                                   // 70..99 → 1970..1999
    emitter.label("__rt_strtotime_textual_ydone");
    emitter.instruction("sub w5, w5, #1900");                                   // tm_year = year - 1900
    emitter.instruction("str w5, [sp, #20]");                                   // store tm_year
    emitter.instruction("str wzr, [sp, #0]");                                   // tm_sec = 0
    emitter.instruction("str wzr, [sp, #4]");                                   // tm_min = 0
    emitter.instruction("str wzr, [sp, #8]");                                   // tm_hour = 0
    emitter.instruction("bl __rt_strtotime_skip_ws");                           // skip whitespace before an optional time
    emitter.instruction("cmp x3, x4");                                          // anything left to parse?
    emitter.instruction("b.ge __rt_strtotime_textual_mktime");                  // no → midnight

    // -- optional HH:MM[:SS] --
    emitter.instruction("mov x12, x3");                                         // remember cursor to detect "no digits"
    emitter.instruction("bl __rt_strtotime_parse_dec");                         // x5 = hour
    emitter.instruction("cmp x3, x12");                                         // any hour digits?
    emitter.instruction("b.eq __rt_strtotime_fail");                            // trailing junk → fail
    emitter.instruction("str w5, [sp, #8]");                                    // tm_hour
    emitter.instruction("cmp x3, x4");                                          // need a ':' next
    emitter.instruction("b.ge __rt_strtotime_fail");                            // missing minutes → fail
    emitter.instruction("ldrb w9, [x3]");                                       // load the hour/minute separator
    emitter.instruction("cmp w9, #58");                                         // require ':'
    emitter.instruction("b.ne __rt_strtotime_fail");                            // malformed time → fail
    emitter.instruction("add x3, x3, #1");                                      // consume the ':'
    emitter.instruction("mov x12, x3");                                         // remember cursor to detect "no digits"
    emitter.instruction("bl __rt_strtotime_parse_dec");                         // x5 = minute
    emitter.instruction("cmp x3, x12");                                         // any minute digits?
    emitter.instruction("b.eq __rt_strtotime_fail");                            // malformed time → fail
    emitter.instruction("str w5, [sp, #4]");                                    // tm_min
    emitter.instruction("cmp x3, x4");                                          // optional :seconds?
    emitter.instruction("b.ge __rt_strtotime_textual_mktime");                  // no seconds → done
    emitter.instruction("ldrb w9, [x3]");                                       // load the minute/second separator
    emitter.instruction("cmp w9, #58");                                         // require ':'
    emitter.instruction("b.ne __rt_strtotime_fail");                            // trailing junk → fail
    emitter.instruction("add x3, x3, #1");                                      // consume the ':'
    emitter.instruction("mov x12, x3");                                         // remember cursor to detect "no digits"
    emitter.instruction("bl __rt_strtotime_parse_dec");                         // x5 = second
    emitter.instruction("cmp x3, x12");                                         // any second digits?
    emitter.instruction("b.eq __rt_strtotime_fail");                            // malformed time → fail
    emitter.instruction("str w5, [sp, #0]");                                    // tm_sec
    emitter.instruction("cmp x3, x4");                                          // must be fully consumed now
    emitter.instruction("b.lt __rt_strtotime_fail");                            // trailing junk → fail

    emitter.label("__rt_strtotime_textual_mktime");
    emitter.instruction("str wzr, [sp, #24]");                                  // tm_wday = 0 (mktime ignores)
    emitter.instruction("str wzr, [sp, #28]");                                  // tm_yday = 0 (mktime ignores)
    emitter.instruction("mov w9, #-1");                                         // tm_isdst sentinel
    emitter.instruction("str w9, [sp, #32]");                                   // tm_isdst = -1 (let mktime infer DST)
    emitter.instruction("mov x0, sp");                                          // x0 = &tm
    emitter.instruction("bl __rt_mktime_shifted");                              // mktime(&tm) → x0 = timestamp
    emitter.instruction("b __rt_strtotime_ret");                                // return through shared epilogue
}

/// Emits x86_64 (Linux) assembly for the textual-date parser. Cursor in `rdi`, end pointer in
/// `r10` (the convention used by the shared cursor helpers). Because `match_word` clobbers `r10`,
/// the day-first path parks the month cursor in a dispatcher scratch slot and recomputes the end
/// pointer afterwards from `[rsp+48]`/`[rsp+56]`.
fn emit_textual_date_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime: textual-date sub-routine ---");
    emitter.label("__rt_strtotime_textual_entry_linux_x86_64");

    emitter.instruction("mov rdi, QWORD PTR [rsp + 48]");                       // cursor = trimmed input pointer (base)
    emitter.instruction("mov r10, QWORD PTR [rsp + 48]");                       // end = base ...
    emitter.instruction("add r10, QWORD PTR [rsp + 56]");                       // ... + length
    emitter.instruction("movzx eax, BYTE PTR [rdi]");                           // first byte
    emitter.instruction("mov ecx, eax");                                        // copy for digit test
    emitter.instruction("sub ecx, 48");                                         // digit test
    emitter.instruction("cmp ecx, 9");                                          // is the first byte a digit?
    emitter.instruction("jbe __rt_strtotime_textual_dayfirst_linux_x86_64");    // digit → "D Month Y" form

    // -- month-first: "Month D[,] Y" — month is at the start, already in lc16 --
    emitter.instruction("lea rdi, [rsp + 64]");                                 // candidate = lc16 buffer
    emitter.instruction("lea rsi, [rip + _strtotime_keyword_tab]");             // month/keyword table
    emitter.instruction("mov rcx, QWORD PTR [rsp + 56]");                       // available bytes = len
    emitter.instruction("cmp rcx, 16");                                         // len > 16 ?
    emitter.instruction("jbe __rt_strtotime_textual_mf_avail_linux_x86_64");    // keep len when <= 16
    emitter.instruction("mov rcx, 16");                                         // cap available to lc16 width
    emitter.label("__rt_strtotime_textual_mf_avail_linux_x86_64");
    emitter.instruction("call __rt_strtotime_match_word_linux_x86_64");         // rax = consumed, rdx = kind
    emitter.instruction("cmp rdx, 19");                                         // month kinds start at 19
    emitter.instruction("jl __rt_strtotime_fail_linux_x86_64");                 // not a month → fail
    emitter.instruction("cmp rdx, 30");                                         // month kinds end at 30
    emitter.instruction("jg __rt_strtotime_fail_linux_x86_64");                 // not a month → fail
    emitter.instruction("sub edx, 19");                                         // month index 0-11
    emitter.instruction("mov DWORD PTR [rsp + 16], edx");                       // tm_mon
    emitter.instruction("mov rdi, QWORD PTR [rsp + 48]");                       // cursor = base ...
    emitter.instruction("add rdi, rax");                                        // ... + consumed (past month)
    emitter.instruction("mov r10, QWORD PTR [rsp + 48]");                       // recompute end = base ...
    emitter.instruction("add r10, QWORD PTR [rsp + 56]");                       // ... + length (match_word clobbered r10)
    emitter.instruction("call __rt_strtotime_skip_ws_linux_x86_64");            // skip whitespace before the day
    emitter.instruction("mov rdx, rdi");                                        // remember cursor to detect "no digits"
    emitter.instruction("call __rt_strtotime_parse_dec_linux_x86_64");          // rax = day
    emitter.instruction("cmp rdi, rdx");                                        // did the day consume any digit?
    emitter.instruction("je __rt_strtotime_fail_linux_x86_64");                 // no day → fail
    emitter.instruction("mov DWORD PTR [rsp + 12], eax");                       // tm_mday (mktime normalizes overflow)
    emitter.instruction("cmp rdi, r10");                                        // need a year after the day
    emitter.instruction("jae __rt_strtotime_fail_linux_x86_64");                // missing year → fail
    emitter.instruction("movzx eax, BYTE PTR [rdi]");                           // peek separator after the day
    emitter.instruction("cmp al, 44");                                          // optional ',' ?
    emitter.instruction("jne __rt_strtotime_textual_mf_sp_linux_x86_64");       // no comma → skip whitespace
    emitter.instruction("inc rdi");                                             // consume the ','
    emitter.label("__rt_strtotime_textual_mf_sp_linux_x86_64");
    emitter.instruction("call __rt_strtotime_skip_ws_linux_x86_64");            // skip whitespace before the year
    emitter.instruction("jmp __rt_strtotime_textual_year_linux_x86_64");        // join the common year/time tail

    // -- day-first: "D Month Y" — parse the day, then require a month name --
    emitter.label("__rt_strtotime_textual_dayfirst_linux_x86_64");
    emitter.instruction("mov rdx, rdi");                                        // remember cursor to detect "no digits"
    emitter.instruction("call __rt_strtotime_parse_dec_linux_x86_64");          // rax = day
    emitter.instruction("cmp rdi, rdx");                                        // any leading digits?
    emitter.instruction("je __rt_strtotime_offsets_entry_linux_x86_64");        // none → let offsets try
    emitter.instruction("mov DWORD PTR [rsp + 12], eax");                       // tm_mday (tentative)
    emitter.instruction("call __rt_strtotime_skip_ws_linux_x86_64");            // skip the whitespace after the number
    emitter.instruction("cmp rdi, r10");                                        // bare number with no word?
    emitter.instruction("jae __rt_strtotime_offsets_entry_linux_x86_64");       // yes → let offsets try
    emitter.instruction("mov QWORD PTR [rsp + 88], rdi");                       // park the month cursor across match_word
    emitter.instruction("call __rt_strtotime_lc_cursor_linux_x86_64");          // lowercase 16 bytes from the cursor into lc16
    emitter.instruction("mov rcx, r10");                                        // available bytes = end ...
    emitter.instruction("sub rcx, QWORD PTR [rsp + 88]");                       // ... - cursor (remaining)
    emitter.instruction("cmp rcx, 16");                                         // remaining > 16 ?
    emitter.instruction("jbe __rt_strtotime_textual_df_avail_linux_x86_64");    // keep remaining when <= 16
    emitter.instruction("mov rcx, 16");                                         // cap available to lc16 width
    emitter.label("__rt_strtotime_textual_df_avail_linux_x86_64");
    emitter.instruction("lea rdi, [rsp + 64]");                                 // candidate = lc16 buffer
    emitter.instruction("lea rsi, [rip + _strtotime_keyword_tab]");             // month/keyword table
    emitter.instruction("call __rt_strtotime_match_word_linux_x86_64");         // rax = consumed, rdx = kind
    emitter.instruction("cmp rdx, 19");                                         // is the word a month name?
    emitter.instruction("jl __rt_strtotime_offsets_entry_linux_x86_64");        // not a month → relative offset
    emitter.instruction("cmp rdx, 30");                                         // month kinds end at 30
    emitter.instruction("jg __rt_strtotime_offsets_entry_linux_x86_64");        // not a month → offsets fallback
    emitter.instruction("sub edx, 19");                                         // month index 0-11
    emitter.instruction("mov DWORD PTR [rsp + 16], edx");                       // tm_mon
    emitter.instruction("mov rdi, QWORD PTR [rsp + 88]");                       // cursor = parked month cursor ...
    emitter.instruction("add rdi, rax");                                        // ... + consumed (past month)
    emitter.instruction("mov r10, QWORD PTR [rsp + 48]");                       // recompute end = base ...
    emitter.instruction("add r10, QWORD PTR [rsp + 56]");                       // ... + length (match_word clobbered r10)
    emitter.instruction("call __rt_strtotime_skip_ws_linux_x86_64");            // skip whitespace before the year

    // -- common tail: year (with 2-digit windowing) + optional time + mktime --
    emitter.label("__rt_strtotime_textual_year_linux_x86_64");
    emitter.instruction("mov rdx, rdi");                                        // remember cursor to detect "no digits"
    emitter.instruction("call __rt_strtotime_parse_dec_linux_x86_64");          // rax = year
    emitter.instruction("cmp rdi, rdx");                                        // any year digits?
    emitter.instruction("je __rt_strtotime_fail_linux_x86_64");                 // missing year → fail
    emitter.instruction("cmp rax, 69");                                         // value <= 69 → 2000s
    emitter.instruction("ja __rt_strtotime_textual_y1900_linux_x86_64");        // otherwise try the 1900s window
    emitter.instruction("add rax, 2000");                                       // 0..69 → 2000..2069
    emitter.instruction("jmp __rt_strtotime_textual_ydone_linux_x86_64");       // year resolved
    emitter.label("__rt_strtotime_textual_y1900_linux_x86_64");
    emitter.instruction("cmp rax, 99");                                         // value 70..99 → 1900s
    emitter.instruction("ja __rt_strtotime_textual_ydone_linux_x86_64");        // >= 100 → literal year
    emitter.instruction("add rax, 1900");                                       // 70..99 → 1970..1999
    emitter.label("__rt_strtotime_textual_ydone_linux_x86_64");
    emitter.instruction("sub eax, 1900");                                       // tm_year = year - 1900
    emitter.instruction("mov DWORD PTR [rsp + 20], eax");                       // store tm_year
    emitter.instruction("mov DWORD PTR [rsp + 0], 0");                          // tm_sec = 0
    emitter.instruction("mov DWORD PTR [rsp + 4], 0");                          // tm_min = 0
    emitter.instruction("mov DWORD PTR [rsp + 8], 0");                          // tm_hour = 0
    emitter.instruction("call __rt_strtotime_skip_ws_linux_x86_64");            // skip whitespace before an optional time
    emitter.instruction("cmp rdi, r10");                                        // anything left to parse?
    emitter.instruction("jae __rt_strtotime_textual_mktime_linux_x86_64");      // no → midnight

    // -- optional HH:MM[:SS] --
    emitter.instruction("mov rdx, rdi");                                        // remember cursor to detect "no digits"
    emitter.instruction("call __rt_strtotime_parse_dec_linux_x86_64");          // rax = hour
    emitter.instruction("cmp rdi, rdx");                                        // any hour digits?
    emitter.instruction("je __rt_strtotime_fail_linux_x86_64");                 // trailing junk → fail
    emitter.instruction("mov DWORD PTR [rsp + 8], eax");                        // tm_hour
    emitter.instruction("cmp rdi, r10");                                        // need a ':' next
    emitter.instruction("jae __rt_strtotime_fail_linux_x86_64");                // missing minutes → fail
    emitter.instruction("movzx eax, BYTE PTR [rdi]");                           // load the hour/minute separator
    emitter.instruction("cmp al, 58");                                          // require ':'
    emitter.instruction("jne __rt_strtotime_fail_linux_x86_64");                // malformed time → fail
    emitter.instruction("inc rdi");                                             // consume the ':'
    emitter.instruction("mov rdx, rdi");                                        // remember cursor to detect "no digits"
    emitter.instruction("call __rt_strtotime_parse_dec_linux_x86_64");          // rax = minute
    emitter.instruction("cmp rdi, rdx");                                        // any minute digits?
    emitter.instruction("je __rt_strtotime_fail_linux_x86_64");                 // malformed time → fail
    emitter.instruction("mov DWORD PTR [rsp + 4], eax");                        // tm_min
    emitter.instruction("cmp rdi, r10");                                        // optional :seconds?
    emitter.instruction("jae __rt_strtotime_textual_mktime_linux_x86_64");      // no seconds → done
    emitter.instruction("movzx eax, BYTE PTR [rdi]");                           // load the minute/second separator
    emitter.instruction("cmp al, 58");                                          // require ':'
    emitter.instruction("jne __rt_strtotime_fail_linux_x86_64");                // trailing junk → fail
    emitter.instruction("inc rdi");                                             // consume the ':'
    emitter.instruction("mov rdx, rdi");                                        // remember cursor to detect "no digits"
    emitter.instruction("call __rt_strtotime_parse_dec_linux_x86_64");          // rax = second
    emitter.instruction("cmp rdi, rdx");                                        // any second digits?
    emitter.instruction("je __rt_strtotime_fail_linux_x86_64");                 // malformed time → fail
    emitter.instruction("mov DWORD PTR [rsp + 0], eax");                        // tm_sec
    emitter.instruction("cmp rdi, r10");                                        // must be fully consumed now
    emitter.instruction("jb __rt_strtotime_fail_linux_x86_64");                 // trailing junk → fail

    emitter.label("__rt_strtotime_textual_mktime_linux_x86_64");
    emitter.instruction("mov DWORD PTR [rsp + 24], 0");                         // tm_wday = 0 (mktime ignores)
    emitter.instruction("mov DWORD PTR [rsp + 28], 0");                         // tm_yday = 0 (mktime ignores)
    emitter.instruction("mov DWORD PTR [rsp + 32], -1");                        // tm_isdst = -1 (let mktime infer DST)
    emitter.instruction("mov rdi, rsp");                                        // rdi = &tm
    emitter.instruction("call __rt_mktime_shifted");                            // mktime(&tm) → rax = timestamp
    emitter.instruction("jmp __rt_strtotime_ret_linux_x86_64");                 // return through shared epilogue
}
