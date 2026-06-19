//! Purpose:
//! Dispatches the `__rt_strtotime` runtime helper to strategy-specific emitter modules.
//! The module owns the public entry point `__rt_strtotime`, the dispatcher frame, and the shared epilogue (`__rt_strtotime_ret` / `__rt_strtotime_fail`).
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::system`.
//! - `crate::codegen::runtime::data::fixed` for the `emit_strtotime_data()` lookup tables.
//!
//! Key details:
//! - Public label `__rt_strtotime`: `x1=ptr, x2=len, x0=baseTimestamp, x3=has_base → x0=timestamp`; `i64::MIN` sentinel on parse failure (boxed to `false` by the builtin).
//!   When `x3 != 0` the relative/keyword/time-only strategies base on `x0` (via the `_strtotime_clock` global) instead of the current time; this is how strtotime's 2nd argument and `DateTime::modify()` work. (x86_64: `rdi=ptr, rsi=len, rdx=base, rcx=has_base → rax`.)
//! - 128-byte stack frame layout (ARM64; x86_64 mirrors numerically via `[rbp - 128 + N]`):
//!     `[sp+ 0..47]` struct tm scratch     —   9 ints for libc mktime
//!     `[sp+48..55]` saved trimmed ptr
//!     `[sp+56..63]` saved trimmed len
//!     `[sp+64..79]` lc16 buffer            — first 16 lowercased input bytes, zero-padded
//!     `[sp+80..111]` scratch slots          — used by today_tm and future strategies
//!     `[sp+112..127]` saved x29/x30 (ARM64)
//! - Dispatcher first-byte switch: digit → iso_date; ASCII alpha → keyword table; else → fail.

mod data;
mod epoch;
mod first_last_day;
mod iso_date;
mod keywords;
mod slash_date;
mod textual_date;
mod offsets;
mod shared;
mod time_only;
mod weekdays;

use crate::codegen::{emit::Emitter, platform::Arch};
use crate::codegen::abi;

pub(crate) use data::emit_strtotime_data;

/// Emits the `__rt_strtotime` runtime entry point, dispatcher, and all strategy emitters
/// (ISO date, time-only, offsets, keywords, weekdays, shared helpers) for the current target.
///
/// Dispatches to ARM64 or x86_64 strategy emitters based on `emitter.target.arch`.
/// Each strategy is emitted as a labeled subroutine that the dispatcher branches to;
/// strategies return via the shared epilogue labels (`__rt_strtotime_ret` / `__rt_strtotime_fail`).
pub(crate) fn emit_strtotime(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_dispatcher_linux_x86_64(emitter);
        iso_date::emit_iso_date(emitter);
        epoch::emit_epoch(emitter);
        slash_date::emit_slash_date(emitter);
        textual_date::emit_textual_date(emitter);
        first_last_day::emit_first_last_day(emitter);
        time_only::emit_time_only(emitter);
        offsets::emit_offsets(emitter);
        keywords::emit_keywords(emitter);
        weekdays::emit_weekdays(emitter);
        shared::emit_helpers(emitter);
        emit_epilogue_linux_x86_64(emitter);
        return;
    }

    emit_dispatcher_arm64(emitter);
    iso_date::emit_iso_date(emitter);
    epoch::emit_epoch(emitter);
    slash_date::emit_slash_date(emitter);
    textual_date::emit_textual_date(emitter);
    first_last_day::emit_first_last_day(emitter);
    time_only::emit_time_only(emitter);
    offsets::emit_offsets(emitter);
    keywords::emit_keywords(emitter);
    weekdays::emit_weekdays(emitter);
    shared::emit_helpers(emitter);
    emit_epilogue_arm64(emitter);
}

/// Emits the ARM64 dispatcher for `__rt_strtotime`.
///
/// Sets up a 128-byte stack frame, saves input ptr/len, trims whitespace,
/// lowercases the first 16 bytes into `[sp+64..79]`, then classifies the first
/// character and branches to the appropriate strategy entry point:
/// - digit → time/ISO/offsets entry
/// - '+' / '-' → offsets entry
/// - alpha (a-z) → keyword table match, then weekdays or offsets
/// - otherwise → fail
///
/// Strategy entry points are emitted by sub-modules and must return via
/// `__rt_strtotime_ret` (success) or `__rt_strtotime_fail` (failure).
fn emit_dispatcher_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strtotime ---");
    emitter.label_global("__rt_strtotime");

    // -- set up dispatcher frame (128 bytes, 16-byte aligned) --
    emitter.instruction("sub sp, sp, #128");                                    // allocate dispatcher frame
    emitter.instruction("stp x29, x30, [sp, #112]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #112");                                   // new frame pointer

    // -- save original input ptr/len into reserved slots --
    emitter.instruction("str x1, [sp, #48]");                                   // save input pointer
    emitter.instruction("str x2, [sp, #56]");                                   // save input length

    // -- resolve the effective clock: base timestamp (x0) if x3!=0, else current time --
    // Stored in the _strtotime_clock global so the now_tm / today_tm / kw_now helpers
    // base every relative/keyword/time-only result on it (this is how strtotime's 2nd
    // baseTimestamp argument, and therefore DateTime::modify(), are honored).
    emitter.instruction("cmp x3, #0");                                          // was a base timestamp supplied?
    emitter.instruction("b.eq __rt_strtotime_clock_from_now");                  // no base → fall back to the current time
    emitter.adrp("x9", "_strtotime_clock");                                     // page of the clock global
    emitter.add_lo12("x9", "x9", "_strtotime_clock");                           // resolve the clock global address
    emitter.instruction("str x0, [x9]");                                        // store the supplied base timestamp as the clock
    emitter.instruction("b __rt_strtotime_clock_ready");                        // skip the current-time path
    emitter.label("__rt_strtotime_clock_from_now");
    emitter.instruction("bl __rt_time");                                        // x0 = current Unix timestamp (ptr/len already saved)
    emitter.adrp("x9", "_strtotime_clock");                                     // page of the clock global
    emitter.add_lo12("x9", "x9", "_strtotime_clock");                           // resolve the clock global address
    emitter.instruction("str x0, [x9]");                                        // store the current time as the clock
    emitter.label("__rt_strtotime_clock_ready");
    emitter.instruction("bl __rt_tz_init_utc");                                 // default the timezone to UTC on first use (PHP-compatible) unless already set

    // -- trim leading/trailing ASCII whitespace --
    emitter.instruction("bl __rt_strtotime_trim");                              // [sp+48]/[sp+56] now hold trimmed values
    emitter.instruction("ldr x2, [sp, #56]");                                   // reload trimmed length
    emitter.instruction("cbz x2, __rt_strtotime_fail");                         // empty after trim → fail

    // -- lowercase first 16 bytes into [sp+64..79] --
    emitter.instruction("bl __rt_strtotime_lc16");                              // fills lc16 buffer

    // -- classify on first lowercased char --
    emitter.instruction("ldrb w9, [sp, #64]");                                  // load first lc16 byte
    emitter.instruction("cmp w9, #64");                                         // '@' epoch form ?
    emitter.instruction("b.eq __rt_strtotime_epoch_entry");                     // → @<timestamp> strategy
    emitter.instruction("sub w10, w9, #48");                                    // '0' = 48
    emitter.instruction("cmp w10, #9");                                         // digit (0-9) ?
    emitter.instruction("b.hi __rt_strtotime_classify_alpha");                  // not digit → try alpha

    // -- digit: probe for HH:MM[:SS] then ISO date, else offsets --
    emitter.instruction("ldr x2, [sp, #56]");                                   // reload trimmed length
    emitter.instruction("cmp x2, #4");                                          // shortest time-only is "H:MM" = 4 chars
    emitter.instruction("b.lt __rt_strtotime_offsets_entry");                   // too short for date/time → offsets
    emitter.instruction("ldrb w11, [sp, #65]");                                 // lc16[1] (second char)
    emitter.instruction("cmp w11, #58");                                        // ':' ?
    emitter.instruction("b.eq __rt_strtotime_time_entry");                      // → time-only (H:MM[:SS])
    emitter.instruction("ldrb w11, [sp, #66]");                                 // lc16[2] (third char)
    emitter.instruction("cmp w11, #58");                                        // ':' ?
    emitter.instruction("b.eq __rt_strtotime_time_entry");                      // → time-only (HH:MM[:SS])
    emitter.instruction("cmp w11, #47");                                        // lc16[2] == '/' (MM/DD/... slash date) ?
    emitter.instruction("b.eq __rt_strtotime_slash_entry");                     // → slash-date strategy
    emitter.instruction("ldrb w11, [sp, #65]");                                 // lc16[1] (second char)
    emitter.instruction("cmp w11, #47");                                        // lc16[1] == '/' (M/D/... slash date) ?
    emitter.instruction("b.eq __rt_strtotime_slash_entry");                     // → slash-date strategy
    emitter.instruction("cmp x2, #10");                                         // ISO date needs ≥ 10 chars
    emitter.instruction("b.lt __rt_strtotime_textual_entry");                   // too short for ISO → try textual (D Month Y), else offsets
    emitter.instruction("ldrb w11, [sp, #68]");                                 // lc16[4] (offset 4 of date)
    emitter.instruction("cmp w11, #45");                                        // '-' ?
    emitter.instruction("b.eq __rt_strtotime_iso_entry");                       // YYYY-MM-DD → ISO
    emitter.instruction("b __rt_strtotime_textual_entry");                      // default for digit-starting: try textual (D Month Y), else offsets

    emitter.label("__rt_strtotime_classify_alpha");
    // -- check for '+' / '-' signs → offsets entry --
    emitter.instruction("cmp w9, #43");                                         // '+' ?
    emitter.instruction("b.eq __rt_strtotime_offsets_entry");                   // → offsets
    emitter.instruction("cmp w9, #45");                                         // '-' ?
    emitter.instruction("b.eq __rt_strtotime_offsets_entry");                   // → offsets
    emitter.instruction("sub w10, w9, #97");                                    // 'a' = 97
    emitter.instruction("cmp w10, #25");                                        // ASCII alpha (a-z) ?
    emitter.instruction("b.hi __rt_strtotime_fail");                            // not alpha → fail
    emitter.instruction("cmp w9, #97");                                         // possible "a/an <unit>" article-relative form ?
    emitter.instruction("b.eq __rt_strtotime_offsets_entry");                   // let offsets parse or reject the article form

    // -- alpha: try keyword table match --
    emitter.instruction("add x6, sp, #64");                                     // x6 = lc16 buffer ptr (candidate)
    abi::emit_symbol_address(emitter, "x7", "_strtotime_keyword_tab");          // load page of keyword table
    emitter.instruction("ldr x8, [sp, #56]");                                   // x8 = trimmed input length
    emitter.instruction("mov x11, #16");                                        // cap candidate window to lc16 size
    emitter.instruction("cmp x8, x11");                                         // available > 16 ?
    emitter.instruction("csel x8, x8, x11, lo");                                // x8 = min(len, 16)
    emitter.instruction("bl __rt_strtotime_match_word");                        // → x9=kind (-1 if no match), x10=consumed

    emitter.instruction("cbz x10, __rt_strtotime_fail");                        // no match → fail
    emitter.instruction("cmp x9, #5");                                          // kind in 0..5 = bare keyword ?
    emitter.instruction("b.hi __rt_strtotime_alpha_not_keyword");               // no → check modifiers/weekdays
    emitter.instruction("ldr x8, [sp, #56]");                                   // reload trimmed input length
    emitter.instruction("cmp x10, x8");                                         // keyword consumed the whole input ?
    emitter.instruction("b.ne __rt_strtotime_fail");                            // trailing junk after keyword → fail
    emitter.instruction("b __rt_strtotime_kw_entry");                           // → keyword strategy

    emitter.label("__rt_strtotime_alpha_not_keyword");

    // relative-unit intercept: "this/next/last <unit>" (non-week). On a unit match this
    // reuses the offsets accumulators + apply; otherwise it restores x9/x10 and falls
    // through to the weekday / first-last strategies (which re-derive their own state).
    emitter.instruction("cmp w9, #6");                                          // modifier kind next(6)/last(7)/this(8) ?
    emitter.instruction("b.lt __rt_strtotime_relunit_skip");                    // kind < 6 -> not a modifier, keep existing routing
    emitter.instruction("cmp w9, #8");                                          // compare against the upper modifier kind (8 = this)
    emitter.instruction("b.gt __rt_strtotime_relunit_skip");                    // kind > 8 -> not a modifier, keep existing routing
    emitter.instruction("str w9, [sp, #80]");                                   // stash modifier kind in the (still-unused) sec accumulator slot
    emitter.instruction("str x10, [sp, #84]");                                  // stash modifier consumed bytes in the min accumulator slot
    emitter.instruction("ldr x3, [sp, #48]");                                   // trimmed input pointer
    emitter.instruction("ldr x2, [sp, #56]");                                   // trimmed input length
    emitter.instruction("add x4, x3, x2");                                      // end pointer
    emitter.instruction("add x3, x3, x10");                                     // advance past the modifier word
    emitter.instruction("bl __rt_strtotime_skip_ws");                           // skip whitespace before the unit
    emitter.instruction("cmp x3, x4");                                          // anything after the modifier ?
    emitter.instruction("b.ge __rt_strtotime_relunit_restore");                 // no -> restore and fall back
    emitter.instruction("bl __rt_strtotime_lc_cursor");                         // lowercase the next word into the lc16 buffer
    emitter.instruction("add x6, sp, #64");                                     // candidate = lc16 buffer
    emitter.adrp("x7", "_strtotime_unit_tab");                                  // unit table base page
    emitter.add_lo12("x7", "x7", "_strtotime_unit_tab");                        // resolve the unit table base
    emitter.instruction("sub x8, x4, x3");                                      // remaining bytes
    emitter.instruction("mov x11, #16");                                        // cap candidate window to lc16 size
    emitter.instruction("cmp x8, x11");                                         // remaining bytes > 16 ?
    emitter.instruction("csel x8, x8, x11, lo");                                // x8 = min(remaining, 16)
    emitter.instruction("bl __rt_strtotime_match_word");                        // x9 = unit kind (0-6), x10 = consumed (0 = none)
    emitter.instruction("cbz x10, __rt_strtotime_relunit_restore");             // no unit after modifier -> restore and fall back
    emitter.instruction("add x3, x3, x10");                                     // advance past the unit word
    emitter.instruction("bl __rt_strtotime_skip_ws");                           // skip trailing whitespace
    emitter.instruction("cmp x3, x4");                                          // is the whole input consumed ?
    emitter.instruction("b.lt __rt_strtotime_relunit_restore");                 // trailing junk -> not a clean "<mod> <unit>", fall back
    // matched a clean "<modifier> <unit>" (non-week): magnitude N = +1/0/-1
    emitter.instruction("ldr w12, [sp, #80]");                                  // reload the saved modifier kind
    emitter.instruction("mov w13, #0");                                         // N = 0 (this)
    emitter.instruction("cmp w12, #6");                                         // next ?
    emitter.instruction("b.ne __rt_strtotime_relunit_n_last");                  // not "next" -> check "last"
    emitter.instruction("mov w13, #1");                                         // next -> N = +1
    emitter.instruction("b __rt_strtotime_relunit_n_done");                     // next handled, N = +1 set
    emitter.label("__rt_strtotime_relunit_n_last");
    emitter.instruction("cmp w12, #7");                                         // last ?
    emitter.instruction("b.ne __rt_strtotime_relunit_n_done");                  // not "last" -> N stays 0 (this)
    emitter.instruction("mov w13, #-1");                                        // last -> N = -1
    emitter.label("__rt_strtotime_relunit_n_done");
    emitter.instruction("str xzr, [sp, #80]");                                  // zero sec + min accumulators (clears the stashed temps too)
    emitter.instruction("str xzr, [sp, #88]");                                  // zero hour + mday accumulators
    emitter.instruction("str xzr, [sp, #96]");                                  // zero mon + year accumulators
    emitter.instruction("str wzr, [sp, #108]");                                 // clear the trailing-ago flag
    emitter.instruction("cmp w9, #4");                                          // week unit (Monday-anchored) ?
    emitter.instruction("b.eq __rt_strtotime_relunit_week");                    // yes -> compute the anchored mday offset
    emitter.instruction("cmp w9, #4");                                          // unit kind <= 3 (sec/min/hour/day) needs no shift
    emitter.instruction("b.le __rt_strtotime_relunit_store");                   // unit kind 0-3 -> no slot shift
    emitter.instruction("sub w9, w9, #1");                                      // month(5)->4, year(6)->5 to match the 6-slot accumulator layout
    emitter.label("__rt_strtotime_relunit_store");
    emitter.instruction("lsl x14, x9, #2");                                     // accumulator byte offset = kind * 4 ...
    emitter.instruction("add x14, x14, #80");                                   // ... + 80 (accumulator base)
    emitter.instruction("str w13, [sp, x14]");                                  // set the single accumulator to N
    emitter.instruction("b __rt_strtotime_offsets_after_loop");                 // reuse the offsets now_tm + mktime apply
    // -- "<modifier> week": Monday-anchored (mday = N*7 - (ISO weekday - 1)) --
    emitter.label("__rt_strtotime_relunit_week");
    emitter.instruction("str w13, [sp, #96]");                                  // stash N in the zeroed mon accumulator slot across now_tm
    emitter.instruction("bl __rt_strtotime_now_tm");                            // base tm -> [sp+0..32] (tm_wday at [sp+24])
    emitter.instruction("ldr w14, [sp, #24]");                                  // tm_wday (0=Sunday)
    emitter.instruction("cmp w14, #0");                                         // Sunday ?
    emitter.instruction("b.ne __rt_strtotime_relunit_week_iso");                // no -> tm_wday already matches ISO 1-6
    emitter.instruction("mov w14, #7");                                         // ISO Sunday = 7
    emitter.label("__rt_strtotime_relunit_week_iso");
    emitter.instruction("ldr w13, [sp, #96]");                                  // reload N
    emitter.instruction("mov w15, #7");                                         // days per week
    emitter.instruction("mul w13, w13, w15");                                   // N * 7
    emitter.instruction("sub w14, w14, #1");                                    // days since Monday = ISO weekday - 1
    emitter.instruction("sub w13, w13, w14");                                   // mday offset = N*7 - (ISO weekday - 1)
    emitter.instruction("str wzr, [sp, #96]");                                  // restore the mon accumulator to 0
    emitter.instruction("str w13, [sp, #92]");                                  // mday accumulator = the anchored offset
    emitter.instruction("b __rt_strtotime_offsets_after_loop");                 // reuse the now_tm + mktime apply
    emitter.label("__rt_strtotime_relunit_restore");
    emitter.instruction("ldr w9, [sp, #80]");                                   // restore the modifier kind for the fall-back strategies
    emitter.instruction("ldr x10, [sp, #84]");                                  // restore the modifier consumed-byte count
    emitter.label("__rt_strtotime_relunit_skip");
    emitter.instruction("cmp x9, #7");                                          // "last" → first/last-day strategy ...
    emitter.instruction("b.eq __rt_strtotime_firstlast_entry");                 // ... which falls back to "last <weekday>"
    emitter.instruction("cmp x9, #8");                                          // kind 6/8 = next/this modifier ?
    emitter.instruction("b.ls __rt_strtotime_weekdays_entry");                  // → weekdays strategy with modifier
    emitter.instruction("cmp x9, #10");                                         // kind 9 = bare "ago" (not a top-level term)
    emitter.instruction("b.lt __rt_strtotime_fail");                            // → fail
    emitter.instruction("cmp x9, #16");                                         // kind 10..16 = weekday name ?
    emitter.instruction("b.le __rt_strtotime_alpha_direct_weekday");            // yes → direct weekday strategy
    emitter.instruction("cmp x9, #18");                                         // kind 17..18 = a/an relative magnitude ?
    emitter.instruction("b.le __rt_strtotime_offsets_entry");                   // let the offsets strategy parse the full relative expression
    emitter.instruction("cmp x9, #30");                                         // kind 19..30 = month name (textual date) ?
    emitter.instruction("b.le __rt_strtotime_textual_entry");                   // → textual-date strategy (month-first)
    emitter.instruction("cmp x9, #31");                                         // kind 31..35 = ordinal (first..fifth)
    emitter.instruction("b.ge __rt_strtotime_firstlast_entry");                 // → first/last-day / nth-weekday strategy
    emitter.instruction("b __rt_strtotime_fail");                               // unknown kind → fail
    emitter.label("__rt_strtotime_alpha_direct_weekday");
    emitter.instruction("ldr x8, [sp, #56]");                                   // reload trimmed input length
    emitter.instruction("cmp x10, x8");                                         // weekday consumed the whole input ?
    emitter.instruction("b.ne __rt_strtotime_fail");                            // trailing junk after weekday → fail
    emitter.instruction("b __rt_strtotime_weekdays_entry");                     // → weekdays strategy
}

/// Emits the shared ARM64 epilogue: `__rt_strtotime_fail` and `__rt_strtotime_ret`.
///
/// `__rt_strtotime_fail` sets `x0 = i64::MIN` (the parse-failure sentinel) then falls through to `__rt_strtotime_ret`,
/// which restores `x29/x30`, deallocates the 128-byte frame, and returns.
///
/// Strategy emitters branch here instead of emitting their own epilogue.
fn emit_epilogue_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime: shared epilogue ---");
    emitter.label("__rt_strtotime_fail");
    emitter.instruction("movz x0, #0x8000, lsl #48");                           // failure sentinel = i64::MIN (-1 is a valid pre-epoch timestamp)

    emitter.label("__rt_strtotime_ret");
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // deallocate dispatcher frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the x86_64 Linux dispatcher for `__rt_strtotime`.
///
/// Sets up a 128-byte stack frame via `rbp/rsp`, saves input ptr/len, trims whitespace,
/// lowercases the first 16 bytes into `[rbp-64..rbp-49]`, then classifies the first
/// character and branches to the appropriate strategy entry point (suffixed `_linux_x86_64`).
/// Mirrors the ARM64 dispatcher logic but uses x86_64 calling conventions and register names.
///
/// Strategy entry points are emitted by sub-modules and must return via
/// `__rt_strtotime_ret_linux_x86_64` (success) or `__rt_strtotime_fail_linux_x86_64` (failure).
fn emit_dispatcher_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strtotime ---");
    emitter.label_global("__rt_strtotime");

    // -- set up dispatcher frame (128 bytes, 16-byte aligned) --
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish dispatcher frame base
    emitter.instruction("sub rsp, 128");                                        // reserve dispatcher locals; rsp 16-byte aligned (128 % 16 == 0)

    // -- save original input ptr/len --
    emitter.instruction("mov QWORD PTR [rbp - 80], rdi");                       // save input pointer ([rsp+48] in dispatcher view)
    emitter.instruction("mov QWORD PTR [rbp - 72], rsi");                       // save input length ([rsp+56] in dispatcher view)

    // -- resolve the effective clock: base timestamp (rdx) if rcx!=0, else current time --
    // Stored in the _strtotime_clock global so the now_tm / today_tm / kw_now helpers base
    // every relative/keyword/time-only result on it (honoring strtotime's 2nd baseTimestamp
    // argument and therefore DateTime::modify()).
    emitter.instruction("test rcx, rcx");                                       // was a base timestamp supplied?
    emitter.instruction("jz __rt_strtotime_clock_from_now_linux_x86_64");       // no base → fall back to the current time
    emitter.instruction("mov QWORD PTR [rip + _strtotime_clock], rdx");         // store the supplied base timestamp as the clock
    emitter.instruction("jmp __rt_strtotime_clock_ready_linux_x86_64");         // skip the current-time path
    emitter.label("__rt_strtotime_clock_from_now_linux_x86_64");
    emitter.instruction("call __rt_time");                                      // rax = current Unix timestamp (ptr/len already saved)
    emitter.instruction("mov QWORD PTR [rip + _strtotime_clock], rax");         // store the current time as the clock
    emitter.label("__rt_strtotime_clock_ready_linux_x86_64");
    emitter.instruction("call __rt_tz_init_utc");                               // default the timezone to UTC on first use (PHP-compatible) unless already set

    // -- trim leading/trailing whitespace --
    emitter.instruction("call __rt_strtotime_trim_linux_x86_64");               // [rbp-80]/[rbp-72] now hold trimmed values
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // reload trimmed length
    emitter.instruction("test rsi, rsi");                                       // empty after trim ?
    emitter.instruction("jz __rt_strtotime_fail_linux_x86_64");                 // yes → fail

    // -- lowercase first 16 bytes into [rbp-64..rbp-49] --
    emitter.instruction("call __rt_strtotime_lc16_linux_x86_64");               // fills lc16 buffer

    // -- classify on first lowercased char --
    emitter.instruction("movzx eax, BYTE PTR [rbp - 64]");                      // load first lc16 byte
    emitter.instruction("cmp al, 64");                                          // '@' epoch form ?
    emitter.instruction("je __rt_strtotime_epoch_entry_linux_x86_64");          // → @<timestamp> strategy
    emitter.instruction("mov ecx, eax");                                        // copy for range checks
    emitter.instruction("sub ecx, 48");                                         // '0' = 48
    emitter.instruction("cmp ecx, 9");                                          // digit ?
    emitter.instruction("ja __rt_strtotime_classify_alpha_linux_x86_64");       // not digit → try alpha

    // -- digit: probe for HH:MM[:SS] then ISO date, else offsets --
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // reload trimmed length
    emitter.instruction("cmp rsi, 4");                                          // shortest time-only is "H:MM"
    emitter.instruction("jl __rt_strtotime_offsets_entry_linux_x86_64");        // too short for date/time → offsets
    emitter.instruction("movzx r8d, BYTE PTR [rbp - 63]");                      // lc16[1]
    emitter.instruction("cmp r8b, 58");                                         // ':' ?
    emitter.instruction("je __rt_strtotime_time_entry_linux_x86_64");           // → time-only (H:MM[:SS])
    emitter.instruction("movzx r8d, BYTE PTR [rbp - 62]");                      // lc16[2]
    emitter.instruction("cmp r8b, 58");                                         // ':' ?
    emitter.instruction("je __rt_strtotime_time_entry_linux_x86_64");           // → time-only (HH:MM[:SS])
    emitter.instruction("cmp r8b, 47");                                         // lc16[2] == '/' (MM/DD/... slash date) ?
    emitter.instruction("je __rt_strtotime_slash_entry_linux_x86_64");          // → slash-date strategy
    emitter.instruction("movzx r8d, BYTE PTR [rbp - 63]");                      // lc16[1]
    emitter.instruction("cmp r8b, 47");                                         // lc16[1] == '/' (M/D/... slash date) ?
    emitter.instruction("je __rt_strtotime_slash_entry_linux_x86_64");          // → slash-date strategy
    emitter.instruction("cmp rsi, 10");                                         // ISO date needs ≥ 10 chars
    emitter.instruction("jl __rt_strtotime_textual_entry_linux_x86_64");        // too short for ISO → try textual (D Month Y), else offsets
    emitter.instruction("movzx r8d, BYTE PTR [rbp - 60]");                      // lc16[4] (offset 4 of date)
    emitter.instruction("cmp r8b, 45");                                         // '-' ?
    emitter.instruction("je __rt_strtotime_iso_entry_linux_x86_64");            // YYYY-MM-DD → ISO
    emitter.instruction("jmp __rt_strtotime_textual_entry_linux_x86_64");       // default for digit-starting: try textual (D Month Y), else offsets

    emitter.label("__rt_strtotime_classify_alpha_linux_x86_64");
    // -- check for '+' / '-' signs → offsets entry --
    emitter.instruction("cmp al, 43");                                          // '+' ?
    emitter.instruction("je __rt_strtotime_offsets_entry_linux_x86_64");        // → offsets
    emitter.instruction("cmp al, 45");                                          // '-' ?
    emitter.instruction("je __rt_strtotime_offsets_entry_linux_x86_64");        // → offsets
    emitter.instruction("mov ecx, eax");                                        // refresh range check
    emitter.instruction("sub ecx, 97");                                         // 'a' = 97
    emitter.instruction("cmp ecx, 25");                                         // ASCII alpha ?
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // not alpha → fail
    emitter.instruction("cmp al, 97");                                          // possible "a/an <unit>" article-relative form ?
    emitter.instruction("je __rt_strtotime_offsets_entry_linux_x86_64");        // let offsets parse or reject the article form

    // -- alpha: try keyword table match --
    // Args (caller-saved): rdi = candidate ptr, rsi = table base, rcx = available bytes.
    // Returns: rax = consumed bytes (0 = no match), rdx = kind (-1 = no match).
    emitter.instruction("lea rdi, [rbp - 64]");                                 // rdi = candidate prefix (lc16 buffer)
    abi::emit_symbol_address(emitter, "rsi", "_strtotime_keyword_tab");         // rsi = keyword table base
    emitter.instruction("mov rcx, QWORD PTR [rbp - 72]");                       // rcx = trimmed input length
    emitter.instruction("mov r8, 16");                                          // cap candidate window to 16
    emitter.instruction("cmp rcx, r8");                                         // len > 16 ?
    emitter.instruction("cmovae rcx, r8");                                      // rcx = min(len, 16)
    emitter.instruction("call __rt_strtotime_match_word_linux_x86_64");         // rax=consumed, rdx=kind (-1 if no match)

    emitter.instruction("test rax, rax");                                       // no match ?
    emitter.instruction("jz __rt_strtotime_fail_linux_x86_64");                 // yes → fail
    emitter.instruction("cmp rdx, 5");                                          // kind in 0..5 = bare keyword ?
    emitter.instruction("ja __rt_strtotime_alpha_not_keyword_linux_x86_64");    // no → check modifiers/weekdays
    emitter.instruction("cmp rax, QWORD PTR [rbp - 72]");                       // keyword consumed the whole input ?
    emitter.instruction("jne __rt_strtotime_fail_linux_x86_64");                // trailing junk after keyword → fail
    emitter.instruction("jmp __rt_strtotime_kw_entry_linux_x86_64");            // yes → keyword strategy

    emitter.label("__rt_strtotime_alpha_not_keyword_linux_x86_64");

    // relative-unit intercept: "this/next/last <unit>" (non-week). On a unit match this
    // reuses the offsets accumulators + apply; otherwise it restores rdx/rax and falls
    // through to the weekday / first-last strategies (which re-derive their own state).
    emitter.instruction("cmp rdx, 6");                                          // modifier kind next(6)/last(7)/this(8) ?
    emitter.instruction("jl __rt_strtotime_relunit_skip_linux_x86_64");         // kind < 6 -> not a modifier, keep existing routing
    emitter.instruction("cmp rdx, 8");                                          // compare against the upper modifier kind (8 = this)
    emitter.instruction("jg __rt_strtotime_relunit_skip_linux_x86_64");         // kind > 8 -> not a modifier, keep existing routing
    emitter.instruction("mov DWORD PTR [rsp + 80], edx");                       // stash modifier kind in the unused sec accumulator slot
    emitter.instruction("mov QWORD PTR [rsp + 84], rax");                       // stash modifier consumed bytes in the min/hour slots
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // trimmed input pointer
    emitter.instruction("mov r10, rdi");                                        // end = ptr ...
    emitter.instruction("add r10, QWORD PTR [rbp - 72]");                       // ... + trimmed length
    emitter.instruction("add rdi, rax");                                        // advance the cursor past the modifier word
    emitter.instruction("call __rt_strtotime_skip_ws_linux_x86_64");            // skip whitespace before the unit
    emitter.instruction("cmp rdi, r10");                                        // anything after the modifier ?
    emitter.instruction("jge __rt_strtotime_relunit_restore_linux_x86_64");     // no -> restore and fall back
    emitter.instruction("call __rt_strtotime_lc_cursor_linux_x86_64");          // lowercase the next word into the lc16 buffer
    emitter.instruction("mov QWORD PTR [rsp + 112], rdi");                      // save cursor across match_word
    emitter.instruction("lea rdi, [rbp - 64]");                                 // candidate = lc16 buffer
    emitter.instruction("lea rsi, [rip + _strtotime_unit_tab]");                // unit table base
    emitter.instruction("mov rcx, r10");                                        // remaining = end ...
    emitter.instruction("sub rcx, QWORD PTR [rsp + 112]");                      // ... - cursor
    emitter.instruction("mov r8, 16");                                          // cap candidate window to lc16 size
    emitter.instruction("cmp rcx, r8");                                         // remaining bytes > 16 ?
    emitter.instruction("cmovae rcx, r8");                                      // rcx = min(remaining, 16)
    emitter.instruction("call __rt_strtotime_match_word_linux_x86_64");         // rax = consumed, rdx = unit kind
    emitter.instruction("mov rdi, QWORD PTR [rsp + 112]");                      // restore cursor
    emitter.instruction("mov r10, QWORD PTR [rbp - 80]");                       // recompute end = ptr ...
    emitter.instruction("add r10, QWORD PTR [rbp - 72]");                       // ... + length
    emitter.instruction("test rax, rax");                                       // unit matched ?
    emitter.instruction("jz __rt_strtotime_relunit_restore_linux_x86_64");      // no unit after modifier -> restore and fall back
    emitter.instruction("add rdi, rax");                                        // advance past the unit word
    emitter.instruction("call __rt_strtotime_skip_ws_linux_x86_64");            // skip trailing whitespace
    emitter.instruction("cmp rdi, r10");                                        // is the whole input consumed ?
    emitter.instruction("jl __rt_strtotime_relunit_restore_linux_x86_64");      // trailing junk -> not a clean "<mod> <unit>"
    // matched a clean "<modifier> <unit>" (non-week): magnitude N = +1/0/-1
    emitter.instruction("mov ecx, DWORD PTR [rsp + 80]");                       // reload the saved modifier kind
    emitter.instruction("mov r11d, 0");                                         // N = 0 (this)
    emitter.instruction("cmp ecx, 6");                                          // next ?
    emitter.instruction("jne __rt_strtotime_relunit_n_last_linux_x86_64");      // not "next" -> check "last"
    emitter.instruction("mov r11d, 1");                                         // next -> N = +1
    emitter.instruction("jmp __rt_strtotime_relunit_n_done_linux_x86_64");      // next handled, N = +1 set
    emitter.label("__rt_strtotime_relunit_n_last_linux_x86_64");
    emitter.instruction("cmp ecx, 7");                                          // last ?
    emitter.instruction("jne __rt_strtotime_relunit_n_done_linux_x86_64");      // not "last" -> N stays 0 (this)
    emitter.instruction("mov r11d, -1");                                        // last -> N = -1
    emitter.label("__rt_strtotime_relunit_n_done_linux_x86_64");
    emitter.instruction("mov QWORD PTR [rsp + 80], 0");                         // zero sec + min accumulators (clears the stashed temps)
    emitter.instruction("mov QWORD PTR [rsp + 88], 0");                         // zero hour + mday accumulators
    emitter.instruction("mov QWORD PTR [rsp + 96], 0");                         // zero mon + year accumulators
    emitter.instruction("mov DWORD PTR [rsp + 108], 0");                        // clear the trailing-ago flag
    emitter.instruction("cmp edx, 4");                                          // week unit (Monday-anchored) ?
    emitter.instruction("je __rt_strtotime_relunit_week_linux_x86_64");         // yes -> compute the anchored mday offset
    emitter.instruction("cmp edx, 4");                                          // unit kind <= 3 (sec/min/hour/day) needs no shift
    emitter.instruction("jle __rt_strtotime_relunit_store_linux_x86_64");       // unit kind 0-3 -> no slot shift
    emitter.instruction("sub edx, 1");                                          // month(5)->4, year(6)->5 for the 6-slot accumulator layout
    emitter.label("__rt_strtotime_relunit_store_linux_x86_64");
    emitter.instruction("movsxd rcx, edx");                                     // widen kind for address math
    emitter.instruction("lea rcx, [rcx*4 + 80]");                               // accumulator byte offset = 80 + kind * 4
    emitter.instruction("mov DWORD PTR [rsp + rcx], r11d");                     // set the single accumulator to N
    emitter.instruction("jmp __rt_strtotime_offsets_after_loop_linux_x86_64");  // reuse the offsets now_tm + mktime apply
    // -- "<modifier> week": Monday-anchored (mday = N*7 - (ISO weekday - 1)) --
    emitter.label("__rt_strtotime_relunit_week_linux_x86_64");
    emitter.instruction("mov DWORD PTR [rsp + 96], r11d");                      // stash N in the zeroed mon accumulator slot across now_tm
    emitter.instruction("call __rt_strtotime_now_tm_linux_x86_64");             // base tm -> [rbp-128..] (tm_wday at [rbp-104])
    emitter.instruction("mov eax, DWORD PTR [rbp - 104]");                      // tm_wday (0=Sunday)
    emitter.instruction("test eax, eax");                                       // Sunday ?
    emitter.instruction("jne __rt_strtotime_relunit_week_iso_linux_x86_64");    // no -> tm_wday already matches ISO 1-6
    emitter.instruction("mov eax, 7");                                          // ISO Sunday = 7
    emitter.label("__rt_strtotime_relunit_week_iso_linux_x86_64");
    emitter.instruction("mov r11d, DWORD PTR [rsp + 96]");                      // reload N
    emitter.instruction("imul r11d, r11d, 7");                                  // N * 7
    emitter.instruction("sub eax, 1");                                          // days since Monday = ISO weekday - 1
    emitter.instruction("sub r11d, eax");                                       // mday offset = N*7 - (ISO weekday - 1)
    emitter.instruction("mov DWORD PTR [rsp + 96], 0");                         // restore the mon accumulator to 0
    emitter.instruction("mov DWORD PTR [rsp + 92], r11d");                      // mday accumulator = the anchored offset
    emitter.instruction("jmp __rt_strtotime_offsets_after_loop_linux_x86_64");  // reuse the now_tm + mktime apply
    emitter.label("__rt_strtotime_relunit_restore_linux_x86_64");
    emitter.instruction("mov edx, DWORD PTR [rsp + 80]");                       // restore the modifier kind for the fall-back strategies
    emitter.instruction("mov rax, QWORD PTR [rsp + 84]");                       // restore the modifier consumed-byte count
    emitter.label("__rt_strtotime_relunit_skip_linux_x86_64");
    emitter.instruction("cmp rdx, 7");                                          // "last" → first/last-day strategy ...
    emitter.instruction("je __rt_strtotime_firstlast_entry_linux_x86_64");      // ... which falls back to "last <weekday>"
    emitter.instruction("cmp rdx, 8");                                          // kind 6/8 = next/this modifier ?
    emitter.instruction("jbe __rt_strtotime_weekdays_entry_linux_x86_64");      // yes → weekdays with modifier
    emitter.instruction("cmp rdx, 10");                                         // kind 9 = bare "ago" → fail
    emitter.instruction("jl __rt_strtotime_fail_linux_x86_64");                 // below 10 → fail
    emitter.instruction("cmp rdx, 16");                                         // weekday name ?
    emitter.instruction("jle __rt_strtotime_alpha_direct_weekday_linux_x86_64"); // yes → direct weekday strategy
    emitter.instruction("cmp rdx, 18");                                         // kind 17..18 = a/an relative magnitude ?
    emitter.instruction("jle __rt_strtotime_offsets_entry_linux_x86_64");       // let the offsets strategy parse the full relative expression
    emitter.instruction("cmp rdx, 30");                                         // kind 19..30 = month name (textual date) ?
    emitter.instruction("jle __rt_strtotime_textual_entry_linux_x86_64");       // → textual-date strategy (month-first)
    emitter.instruction("cmp rdx, 31");                                         // kind 31..35 = ordinal (first..fifth)
    emitter.instruction("jge __rt_strtotime_firstlast_entry_linux_x86_64");     // → first/last-day / nth-weekday strategy
    emitter.instruction("jmp __rt_strtotime_fail_linux_x86_64");                // unknown kind → fail
    emitter.label("__rt_strtotime_alpha_direct_weekday_linux_x86_64");
    emitter.instruction("cmp rax, QWORD PTR [rbp - 72]");                       // weekday consumed the whole input ?
    emitter.instruction("jne __rt_strtotime_fail_linux_x86_64");                // trailing junk after weekday → fail
    emitter.instruction("jmp __rt_strtotime_weekdays_entry_linux_x86_64");      // yes → weekdays
}

/// Emits the shared x86_64 Linux epilogue: `__rt_strtotime_fail_linux_x86_64` and `__rt_strtotime_ret_linux_x86_64`.
///
/// `__rt_strtotime_fail_linux_x86_64` sets `rax = -1` then falls through to `__rt_strtotime_ret_linux_x86_64`,
/// which deallocates the 128-byte frame, restores `rbp`, and returns.
///
/// Strategy emitters branch here instead of emitting their own epilogue.
fn emit_epilogue_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime: shared epilogue ---");
    emitter.label("__rt_strtotime_fail_linux_x86_64");
    emitter.instruction("movabs rax, -9223372036854775808");                    // failure sentinel = i64::MIN (-1 is a valid pre-epoch timestamp)

    emitter.label("__rt_strtotime_ret_linux_x86_64");
    emitter.instruction("add rsp, 128");                                        // deallocate dispatcher locals
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}
