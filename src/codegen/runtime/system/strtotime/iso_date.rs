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

/// Emits an unsigned range check rejecting the assembled ISO field in `reg` when it exceeds `max`,
/// mirroring PHP/timelib's regex bounds (month ≤ 12, day ≤ 31, hour ≤ 24, minute ≤ 59, second ≤ 60).
/// In-range calendar overflow (e.g. `02-30`) is still normalized later by `mktime`/`timegm`.
fn emit_arm64_reject_if_above(emitter: &mut Emitter, reg: &str, max: u32) {
    let cmp = format!("cmp {reg}, #{max}");
    emitter.instruction(&cmp);                                                  // bound the assembled ISO field
    emitter.instruction("b.hi __rt_strtotime_fail");                            // reject out-of-range date/time component
}

/// x86_64 counterpart of `emit_arm64_reject_if_above`: rejects the assembled ISO field in `reg`
/// when it exceeds `max` (unsigned), matching PHP/timelib's per-field regex bounds before `mktime`.
fn emit_x86_64_reject_if_above(emitter: &mut Emitter, reg: &str, max: u32) {
    let cmp = format!("cmp {reg}, {max}");
    emitter.instruction(&cmp);                                                  // bound the assembled ISO field
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // reject out-of-range date/time component
}

/// Emits ARM64 assembly for the ISO date/datetime parser sub-routine.
/// Entry label: `__rt_strtotime_iso_entry`.
/// Inputs: trimmed ptr at `[sp+48]`, trimmed len at `[sp+56]`.
/// Parses `YYYY-MM-DD` (10 bytes), `YYYY-MM-DD HH:MM` (16 bytes), and `YYYY-MM-DD HH:MM:SS` (19 bytes).
/// Accepts date/time separator as space, `T`, or `t`.
/// Honors a trailing timezone (flag at `[sp+84]`): a numeric `±HH:MM`/`±HHMM`/`Z`/`UTC`/`GMT`
/// offset (timegm minus offset), or a Continent/City IANA name (set the zone, `mktime`, restore).
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

    // -- detect & strip a trailing explicit zone (Z / +-HH:MM / +-HHMM); offset+flag
    // -- stored as separate 32-bit slots ([sp+80]=offset, [sp+84]=flag) to avoid overlap --
    emitter.instruction("mov w10, #0");                                         // zone_len
    emitter.instruction("mov w11, #0");                                         // offset_seconds
    emitter.instruction("mov w12, #0");                                         // explicit-zone flag
    emitter.instruction("cmp x2, #11");                                         // too short to carry a date plus a zone ?
    emitter.instruction("b.lt __rt_strtotime_iso_zone_done");                   // yes -> no zone
    emitter.instruction("add x13, x1, x2");                                     // end of input
    emitter.instruction("ldrb w9, [x13, #-1]");                                 // last character
    emitter.instruction("orr w14, w9, #0x20");                                  // lowercase
    emitter.instruction("cmp w14, #122");                                       // 'Z'/'z' ?
    emitter.instruction("b.ne __rt_strtotime_iso_zone_colon");                  // no -> try +-HH:MM
    emitter.instruction("mov w10, #1");                                         // zone_len = 1
    emitter.instruction("mov w12, #1");                                         // explicit (UTC, offset 0)
    emitter.instruction("b __rt_strtotime_iso_zone_done");                      // apply the parsed zone (or fall through with none)
    emitter.label("__rt_strtotime_iso_zone_colon");
    emitter.instruction("ldrb w9, [x13, #-6]");                                 // candidate sign
    emitter.instruction("ldrb w14, [x13, #-3]");                                // candidate colon
    emitter.instruction("cmp w14, #58");                                        // ':' at len-3 ?
    emitter.instruction("b.ne __rt_strtotime_iso_zone_compact");                // no
    emitter.instruction("cmp w9, #43");                                         // '+' ?
    emitter.instruction("b.eq __rt_strtotime_iso_zone_colon_go");               // signed offset -> parse the +-HH:MM digits
    emitter.instruction("cmp w9, #45");                                         // '-' ?
    emitter.instruction("b.ne __rt_strtotime_iso_zone_compact");                // no
    emitter.label("__rt_strtotime_iso_zone_colon_go");
    emitter.instruction("ldrb w10, [x13, #-5]");                                // HH tens
    emitter.instruction("sub w10, w10, #48");                                   // to digit
    emitter.instruction("ldrb w14, [x13, #-4]");                                // HH ones
    emitter.instruction("sub w14, w14, #48");                                   // to digit
    emitter.instruction("mov w12, #10");                                        // decimal base for the digit pair
    emitter.instruction("madd w10, w10, w12, w14");                             // HH = tens*10 + ones
    emitter.instruction("ldrb w14, [x13, #-2]");                                // MM tens
    emitter.instruction("sub w14, w14, #48");                                   // to digit
    emitter.instruction("ldrb w12, [x13, #-1]");                                // MM ones
    emitter.instruction("sub w12, w12, #48");                                   // to digit
    emitter.instruction("mov w15, #10");                                        // decimal base for the digit pair
    emitter.instruction("madd w14, w14, w15, w12");                             // MM = tens*10 + ones
    emitter.instruction("mov w12, #3600");                                      // seconds per hour
    emitter.instruction("mul w10, w10, w12");                                   // HH -> seconds
    emitter.instruction("mov w12, #60");                                        // seconds per minute
    emitter.instruction("madd w11, w14, w12, w10");                             // offset = HH*3600 + MM*60
    emitter.instruction("cmp w9, #45");                                         // '-' ?
    emitter.instruction("b.ne __rt_strtotime_iso_zone_colon_done");             // '+' keeps the offset positive
    emitter.instruction("neg w11, w11");                                        // negative
    emitter.label("__rt_strtotime_iso_zone_colon_done");
    emitter.instruction("mov w10, #6");                                         // zone_len = 6
    emitter.instruction("mov w12, #1");                                         // explicit
    emitter.instruction("b __rt_strtotime_iso_zone_done");                      // apply the parsed zone (or fall through with none)
    emitter.label("__rt_strtotime_iso_zone_compact");
    emitter.instruction("ldrb w9, [x13, #-5]");                                 // candidate sign
    emitter.instruction("cmp w9, #43");                                         // '+' ?
    emitter.instruction("b.eq __rt_strtotime_iso_zone_compact_go");             // signed offset -> parse the +-HHMM digits
    emitter.instruction("cmp w9, #45");                                         // '-' ?
    emitter.instruction("b.ne __rt_strtotime_iso_zone_word");                   // not signed -> try a UTC/GMT word
    emitter.label("__rt_strtotime_iso_zone_compact_go");
    emitter.instruction("ldrb w10, [x13, #-4]");                                // first HH digit
    emitter.instruction("sub w10, w10, #48");                                   // to digit
    emitter.instruction("cmp w10, #9");                                         // numeric ?
    emitter.instruction("b.hi __rt_strtotime_iso_zone_done");                   // no -> local
    emitter.instruction("ldrb w10, [x13, #-4]");                                // HH tens
    emitter.instruction("sub w10, w10, #48");                                   // to digit
    emitter.instruction("ldrb w14, [x13, #-3]");                                // HH ones
    emitter.instruction("sub w14, w14, #48");                                   // to digit
    emitter.instruction("mov w12, #10");                                        // decimal base for the digit pair
    emitter.instruction("madd w10, w10, w12, w14");                             // HH = tens*10 + ones
    emitter.instruction("ldrb w14, [x13, #-2]");                                // MM tens
    emitter.instruction("sub w14, w14, #48");                                   // to digit
    emitter.instruction("ldrb w12, [x13, #-1]");                                // MM ones
    emitter.instruction("sub w12, w12, #48");                                   // to digit
    emitter.instruction("mov w15, #10");                                        // decimal base for the digit pair
    emitter.instruction("madd w14, w14, w15, w12");                             // MM = tens*10 + ones
    emitter.instruction("mov w12, #3600");                                      // seconds per hour
    emitter.instruction("mul w10, w10, w12");                                   // HH -> seconds
    emitter.instruction("mov w12, #60");                                        // seconds per minute
    emitter.instruction("madd w11, w14, w12, w10");                             // offset = HH*3600 + MM*60
    emitter.instruction("cmp w9, #45");                                         // '-' ?
    emitter.instruction("b.ne __rt_strtotime_iso_zone_compact_done");           // '+' keeps the offset positive
    emitter.instruction("neg w11, w11");                                        // negative
    emitter.label("__rt_strtotime_iso_zone_compact_done");
    emitter.instruction("mov w10, #5");                                         // zone_len = 5
    emitter.instruction("mov w12, #1");                                         // explicit
    // -- trailing "UTC"/"GMT" word: explicit UTC (offset stays 0, like Z) --
    emitter.label("__rt_strtotime_iso_zone_word");
    emitter.instruction("ldrb w9, [x13, #-3]");                                 // 3rd-from-last char
    emitter.instruction("orr w9, w9, #0x20");                                   // lowercase
    emitter.instruction("ldrb w14, [x13, #-2]");                                // 2nd-from-last char
    emitter.instruction("orr w14, w14, #0x20");                                 // lowercase
    emitter.instruction("ldrb w15, [x13, #-1]");                                // last char
    emitter.instruction("orr w15, w15, #0x20");                                 // lowercase
    emitter.instruction("cmp w9, #117");                                        // 'u' (UTC) ?
    emitter.instruction("b.ne __rt_strtotime_iso_zone_word_gmt");               // no -> try GMT
    emitter.instruction("cmp w14, #116");                                       // 't' ?
    emitter.instruction("b.ne __rt_strtotime_iso_zone_word_gmt");               // not UTC
    emitter.instruction("cmp w15, #99");                                        // 'c' ?
    emitter.instruction("b.ne __rt_strtotime_iso_zone_word_gmt");               // not UTC
    emitter.instruction("b __rt_strtotime_iso_zone_word_ok");                   // matched UTC
    emitter.label("__rt_strtotime_iso_zone_word_gmt");
    emitter.instruction("cmp w9, #103");                                        // 'g' (GMT) ?
    emitter.instruction("b.ne __rt_strtotime_iso_zone_iana");                   // not UTC/GMT -> try an IANA name
    emitter.instruction("cmp w14, #109");                                       // 'm' ?
    emitter.instruction("b.ne __rt_strtotime_iso_zone_iana");                   // not UTC/GMT -> try an IANA name
    emitter.instruction("cmp w15, #116");                                       // 't' ?
    emitter.instruction("b.ne __rt_strtotime_iso_zone_iana");                   // not UTC/GMT -> try an IANA name
    emitter.label("__rt_strtotime_iso_zone_word_ok");
    emitter.instruction("mov w10, #3");                                         // zone_len = 3 (UTC/GMT)
    emitter.instruction("mov w12, #1");                                         // explicit zone (offset stays 0)
    // -- trailing IANA zone name (e.g. "America/New_York"): scan back to the last space; a
    // -- token containing "/" is a Continent/City zone -> tz-switch flag (2). Uses only x9/x14
    // -- x15/x16 so the pending zone_len/offset/flag (w10/w11/w12) survive a fall-through bail --
    emitter.label("__rt_strtotime_iso_zone_iana");
    emitter.instruction("mov x9, x13");                                         // scan pointer = end of input
    emitter.label("__rt_strtotime_iso_iana_scan");
    emitter.instruction("cmp x9, x1");                                          // reached the input start ?
    emitter.instruction("b.le __rt_strtotime_iso_zone_done");                   // no space found -> no zone
    emitter.instruction("sub x9, x9, #1");                                      // step back one byte
    emitter.instruction("ldrb w14, [x9]");                                      // load the byte
    emitter.instruction("cmp w14, #32");                                        // a space ?
    emitter.instruction("b.ne __rt_strtotime_iso_iana_scan");                   // no -> keep scanning back
    emitter.instruction("add x15, x9, #1");                                     // zone token starts after the last space
    emitter.instruction("cmp x15, x13");                                        // is the token empty ?
    emitter.instruction("b.ge __rt_strtotime_iso_zone_done");                   // empty -> no zone
    emitter.instruction("mov x14, x15");                                        // slash-scan pointer
    emitter.label("__rt_strtotime_iso_iana_slash");
    emitter.instruction("cmp x14, x13");                                        // reached the end with no "/" ?
    emitter.instruction("b.ge __rt_strtotime_iso_zone_done");                   // no "/" -> not a Continent/City zone (also the time token)
    emitter.instruction("ldrb w16, [x14]");                                     // load token byte
    emitter.instruction("cmp w16, #47");                                        // a "/" ?
    emitter.instruction("b.eq __rt_strtotime_iso_iana_found");                  // yes -> it is an IANA zone name
    emitter.instruction("add x14, x14, #1");                                    // next byte
    emitter.instruction("b __rt_strtotime_iso_iana_slash");                     // keep scanning for "/"
    emitter.label("__rt_strtotime_iso_iana_found");
    emitter.instruction("sub x16, x13, x15");                                   // zone-name length = end - token start
    emitter.instruction("str x15, [sp, #88]");                                  // save the IANA name pointer
    emitter.instruction("str w16, [sp, #80]");                                  // save the IANA name length (32-bit slot; flag at +84)
    emitter.instruction("mov w16, #2");                                         // flag = 2 (IANA tz-switch)
    emitter.instruction("str w16, [sp, #84]");                                  // store the flag
    emitter.instruction("sub x2, x1, x9");                                      // date/time length = (last space - start), negated below
    emitter.instruction("neg x2, x2");                                          // x2 = last space index = date/time length
    emitter.instruction("str x2, [sp, #56]");                                   // persist the reduced length (date/time only)
    emitter.instruction("b __rt_strtotime_iso_zone_lencheck");                  // validate the date/time and parse
    emitter.label("__rt_strtotime_iso_zone_done");
    emitter.instruction("cbz w10, __rt_strtotime_iso_zone_strip");              // no zone -> nothing to strip
    emitter.instruction("sub x14, x13, x10");                                   // x14 = start of the zone token (end - zone_len)
    emitter.instruction("ldrb w9, [x14, #-1]");                                 // char immediately before the zone
    emitter.instruction("cmp w9, #32");                                         // a separating space ? (e.g. "12:00:00 +0200")
    emitter.instruction("b.ne __rt_strtotime_iso_zone_strip");                  // no -> strip just the zone
    emitter.instruction("add w10, w10, #1");                                    // also strip the separating space
    emitter.label("__rt_strtotime_iso_zone_strip");
    emitter.instruction("str w11, [sp, #80]");                                  // save offset_seconds (32-bit slot)
    emitter.instruction("str w12, [sp, #84]");                                  // save the explicit-zone flag (separate 32-bit slot)
    emitter.instruction("sub x2, x2, w10, sxtw");                               // strip the zone from the effective length
    emitter.instruction("str x2, [sp, #56]");                                   // persist the reduced length

    // -- validate minimum length (10 for YYYY-MM-DD) --
    emitter.label("__rt_strtotime_iso_zone_lencheck");
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
    emit_arm64_reject_if_above(emitter, "w9", 12);
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
    emit_arm64_reject_if_above(emitter, "w9", 31);
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
    emit_arm64_reject_if_above(emitter, "w9", 24);
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
    emit_arm64_reject_if_above(emitter, "w9", 59);
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
    emit_arm64_reject_if_above(emitter, "w9", 60);
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
    emitter.instruction("ldr w12, [sp, #84]");                                  // explicit-zone flag set ?
    emitter.instruction("cbz w12, __rt_strtotime_iso_local_mk");                // no -> interpret in the default zone
    emitter.instruction("cmp w12, #2");                                         // IANA tz-switch ?
    emitter.instruction("b.eq __rt_strtotime_iso_iana_mk");                     // yes -> set the zone, mktime, restore
    // explicit zone: interpret the fields as UTC (timegm) then subtract the offset
    emitter.instruction("mov x0, sp");                                          // x0 = pointer to struct tm
    emitter.bl_c("timegm");                                                     // timegm(&tm) -> x0 = UTC time_t
    emitter.instruction("ldrsw x11, [sp, #80]");                                // reload offset_seconds, sign-extended from the 32-bit slot
    emitter.instruction("sub x0, x0, x11");                                     // UTC instant = timegm(tm) - offset
    emitter.instruction("b __rt_strtotime_ret");                                // return through the shared epilogue
    // -- IANA zone: save the current default zone, set the parsed zone, mktime, restore --
    emitter.label("__rt_strtotime_iso_iana_mk");
    emitter.instruction("bl __rt_date_default_timezone_get");                   // x1 = current zone id ptr, x2 = len
    emitter.instruction("str x2, [sp, #96]");                                   // save the current id length across the calls
    emitter.adrp("x3", "_php_tz_save");                                         // save-buffer page
    emitter.add_lo12("x3", "x3", "_php_tz_save");                               // resolve the save buffer
    emitter.instruction("mov x4, #0");                                          // copy index
    emitter.label("__rt_strtotime_iana_save_copy");
    emitter.instruction("cmp x4, x2");                                          // copied the whole id ?
    emitter.instruction("b.ge __rt_strtotime_iana_save_done");                  // yes
    emitter.instruction("ldrb w5, [x1, x4]");                                   // byte from the current id
    emitter.instruction("strb w5, [x3, x4]");                                   // into the save buffer
    emitter.instruction("add x4, x4, #1");                                      // next byte
    emitter.instruction("b __rt_strtotime_iana_save_copy");                     // continue
    emitter.label("__rt_strtotime_iana_save_done");
    emitter.instruction("ldr x1, [sp, #88]");                                   // IANA name ptr
    emitter.instruction("ldr w2, [sp, #80]");                                   // IANA name len (32-bit slot; flag at +84)
    emitter.instruction("bl __rt_date_default_timezone_set");                   // set the default zone to the parsed IANA name
    emitter.instruction("mov x0, sp");                                          // x0 = struct tm
    emitter.instruction("bl __rt_mktime_shifted");                              // mktime in the IANA zone -> x0 = timestamp
    emitter.instruction("str x0, [sp, #104]");                                  // save the timestamp across the restore call (non-overlapping with +96)
    emitter.adrp("x1", "_php_tz_save");                                         // saved id page
    emitter.add_lo12("x1", "x1", "_php_tz_save");                               // resolve the saved id
    emitter.instruction("ldr x2, [sp, #96]");                                   // saved id length
    emitter.instruction("bl __rt_date_default_timezone_set");                   // restore the previous default zone
    emitter.instruction("ldr x0, [sp, #104]");                                  // reload the timestamp
    emitter.instruction("b __rt_strtotime_ret");                                // return through the shared epilogue
    emitter.label("__rt_strtotime_iso_local_mk");
    emitter.instruction("mov x0, sp");                                          // x0 = pointer to struct tm
    emitter.instruction("bl __rt_mktime_shifted");                              // mktime(&tm) in the default zone -> x0=timestamp
    emitter.instruction("b __rt_strtotime_ret");                                // return through the shared epilogue
}

/// Emits x86_64 (Linux) assembly for the ISO date/datetime parser sub-routine.
/// Entry label: `__rt_strtotime_iso_entry_linux_x86_64`.
/// Inputs: trimmed ptr at `[rsp+48]`, trimmed len at `[rsp+56]` (SysV ABI convention).
/// Parses `YYYY-MM-DD` (10 bytes), `YYYY-MM-DD HH:MM` (16 bytes), and `YYYY-MM-DD HH:MM:SS` (19 bytes).
/// Accepts date/time separator as space, `T`, or `t`.
/// Honors a trailing timezone (flag at `[rsp+84]`): a numeric `±HH:MM`/`±HHMM`/`Z`/`UTC`/`GMT`
/// offset (timegm minus offset), or a Continent/City IANA name (set the zone, `mktime`, restore).
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

    // -- detect & strip a trailing explicit zone (Z / +-HH:MM / +-HHMM); offset@[rsp+80],
    // -- flag@[rsp+84] as separate 32-bit slots to avoid an overlapping store --
    emitter.instruction("mov r10d, 0");                                         // zone_len = 0
    emitter.instruction("mov r9d, 0");                                          // offset_seconds = 0
    emitter.instruction("mov r11d, 0");                                         // flag = 0
    emitter.instruction("cmp rsi, 11");                                         // too short to carry a date plus a zone ?
    emitter.instruction("jb __rt_strtotime_iso_zone_done_linux_x86_64");        // yes -> no zone
    emitter.instruction("lea rcx, [rdi + rsi]");                                // rcx = end of input
    emitter.instruction("movzx eax, BYTE PTR [rcx - 1]");                       // last character
    emitter.instruction("or eax, 32");                                          // lowercase
    emitter.instruction("cmp eax, 122");                                        // 'z' ?
    emitter.instruction("jne __rt_strtotime_iso_zone_colon_linux_x86_64");      // no -> try +-HH:MM
    emitter.instruction("mov r10d, 1");                                         // zone_len = 1
    emitter.instruction("mov r11d, 1");                                         // explicit (UTC, offset 0)
    emitter.instruction("jmp __rt_strtotime_iso_zone_done_linux_x86_64");       // apply the parsed zone (or fall through with none)
    emitter.label("__rt_strtotime_iso_zone_colon_linux_x86_64");
    emitter.instruction("movzx eax, BYTE PTR [rcx - 6]");                       // candidate sign
    emitter.instruction("movzx edx, BYTE PTR [rcx - 3]");                       // candidate colon
    emitter.instruction("cmp edx, 58");                                         // ':' at len-3 ?
    emitter.instruction("jne __rt_strtotime_iso_zone_compact_linux_x86_64");    // no
    emitter.instruction("cmp eax, 43");                                         // '+' ?
    emitter.instruction("je __rt_strtotime_iso_zone_colon_go_linux_x86_64");    // signed offset -> parse the +-HH:MM digits
    emitter.instruction("cmp eax, 45");                                         // '-' ?
    emitter.instruction("jne __rt_strtotime_iso_zone_compact_linux_x86_64");    // no
    emitter.label("__rt_strtotime_iso_zone_colon_go_linux_x86_64");
    emitter.instruction("movzx eax, BYTE PTR [rcx -5]");                        // HH tens
    emitter.instruction("sub eax, 48");                                         // to digit
    emitter.instruction("movzx edx, BYTE PTR [rcx -4]");                        // HH ones
    emitter.instruction("sub edx, 48");                                         // to digit
    emitter.instruction("imul eax, eax, 10");                                   // HH tens*10
    emitter.instruction("add eax, edx");                                        // HH
    emitter.instruction("movzx edx, BYTE PTR [rcx -2]");                        // MM tens
    emitter.instruction("sub edx, 48");                                         // to digit
    emitter.instruction("movzx r9d, BYTE PTR [rcx -1]");                        // MM ones
    emitter.instruction("sub r9d, 48");                                         // to digit
    emitter.instruction("imul edx, edx, 10");                                   // MM tens*10
    emitter.instruction("add edx, r9d");                                        // MM
    emitter.instruction("imul eax, eax, 3600");                                 // HH -> seconds
    emitter.instruction("imul edx, edx, 60");                                   // MM -> seconds
    emitter.instruction("lea r9d, [rax + rdx]");                                // offset = HH*3600 + MM*60
    emitter.instruction("movzx eax, BYTE PTR [rcx - 6]");                       // reload sign
    emitter.instruction("cmp eax, 45");                                         // '-' ?
    emitter.instruction("jne __rt_strtotime_iso_zone_colon_done_linux_x86_64"); // '+' keeps the offset positive
    emitter.instruction("neg r9d");                                             // negative offset
    emitter.label("__rt_strtotime_iso_zone_colon_done_linux_x86_64");
    emitter.instruction("mov r10d, 6");                                         // zone_len = 6
    emitter.instruction("mov r11d, 1");                                         // explicit
    emitter.instruction("jmp __rt_strtotime_iso_zone_done_linux_x86_64");       // apply the parsed zone (or fall through with none)
    emitter.label("__rt_strtotime_iso_zone_compact_linux_x86_64");
    emitter.instruction("movzx eax, BYTE PTR [rcx - 5]");                       // candidate sign
    emitter.instruction("cmp eax, 43");                                         // '+' ?
    emitter.instruction("je __rt_strtotime_iso_zone_compact_go_linux_x86_64");  // signed offset -> parse the +-HHMM digits
    emitter.instruction("cmp eax, 45");                                         // '-' ?
    emitter.instruction("jne __rt_strtotime_iso_zone_word_linux_x86_64");       // not signed -> try a UTC/GMT word
    emitter.label("__rt_strtotime_iso_zone_compact_go_linux_x86_64");
    emitter.instruction("movzx eax, BYTE PTR [rcx - 4]");                       // first HH digit
    emitter.instruction("sub eax, 48");                                         // to digit
    emitter.instruction("cmp eax, 9");                                          // numeric ?
    emitter.instruction("ja __rt_strtotime_iso_zone_done_linux_x86_64");        // no
    emitter.instruction("movzx eax, BYTE PTR [rcx -4]");                        // HH tens
    emitter.instruction("sub eax, 48");                                         // to digit
    emitter.instruction("movzx edx, BYTE PTR [rcx -3]");                        // HH ones
    emitter.instruction("sub edx, 48");                                         // to digit
    emitter.instruction("imul eax, eax, 10");                                   // HH tens*10
    emitter.instruction("add eax, edx");                                        // HH
    emitter.instruction("movzx edx, BYTE PTR [rcx -2]");                        // MM tens
    emitter.instruction("sub edx, 48");                                         // to digit
    emitter.instruction("movzx r9d, BYTE PTR [rcx -1]");                        // MM ones
    emitter.instruction("sub r9d, 48");                                         // to digit
    emitter.instruction("imul edx, edx, 10");                                   // MM tens*10
    emitter.instruction("add edx, r9d");                                        // MM
    emitter.instruction("imul eax, eax, 3600");                                 // HH -> seconds
    emitter.instruction("imul edx, edx, 60");                                   // MM -> seconds
    emitter.instruction("lea r9d, [rax + rdx]");                                // offset = HH*3600 + MM*60
    emitter.instruction("movzx eax, BYTE PTR [rcx - 5]");                       // reload sign
    emitter.instruction("cmp eax, 45");                                         // '-' ?
    emitter.instruction("jne __rt_strtotime_iso_zone_compact_done_linux_x86_64"); // '+' keeps the offset positive
    emitter.instruction("neg r9d");                                             // negative offset
    emitter.label("__rt_strtotime_iso_zone_compact_done_linux_x86_64");
    emitter.instruction("mov r10d, 5");                                         // zone_len = 5
    emitter.instruction("mov r11d, 1");                                         // explicit
    // -- trailing "UTC"/"GMT" word: explicit UTC (offset stays 0, like Z) --
    emitter.label("__rt_strtotime_iso_zone_word_linux_x86_64");
    emitter.instruction("movzx eax, BYTE PTR [rcx - 3]");                       // 3rd-from-last char
    emitter.instruction("or eax, 32");                                          // lowercase
    emitter.instruction("movzx edx, BYTE PTR [rcx - 2]");                       // 2nd-from-last char
    emitter.instruction("or edx, 32");                                          // lowercase
    emitter.instruction("movzx r8d, BYTE PTR [rcx - 1]");                       // last char
    emitter.instruction("or r8d, 32");                                          // lowercase
    emitter.instruction("cmp eax, 117");                                        // 'u' (UTC) ?
    emitter.instruction("jne __rt_strtotime_iso_zone_word_gmt_linux_x86_64");   // no -> try GMT
    emitter.instruction("cmp edx, 116");                                        // 't' ?
    emitter.instruction("jne __rt_strtotime_iso_zone_word_gmt_linux_x86_64");   // not UTC
    emitter.instruction("cmp r8d, 99");                                         // 'c' ?
    emitter.instruction("jne __rt_strtotime_iso_zone_word_gmt_linux_x86_64");   // not UTC
    emitter.instruction("jmp __rt_strtotime_iso_zone_word_ok_linux_x86_64");    // matched UTC
    emitter.label("__rt_strtotime_iso_zone_word_gmt_linux_x86_64");
    emitter.instruction("cmp eax, 103");                                        // 'g' (GMT) ?
    emitter.instruction("jne __rt_strtotime_iso_zone_iana_linux_x86_64");       // not UTC/GMT -> try an IANA name
    emitter.instruction("cmp edx, 109");                                        // 'm' ?
    emitter.instruction("jne __rt_strtotime_iso_zone_iana_linux_x86_64");       // not UTC/GMT -> try an IANA name
    emitter.instruction("cmp r8d, 116");                                        // 't' ?
    emitter.instruction("jne __rt_strtotime_iso_zone_iana_linux_x86_64");       // not UTC/GMT -> try an IANA name
    emitter.label("__rt_strtotime_iso_zone_word_ok_linux_x86_64");
    emitter.instruction("mov r10d, 3");                                         // zone_len = 3 (UTC/GMT)
    emitter.instruction("mov r11d, 1");                                         // explicit zone (offset stays 0)
    // -- trailing IANA zone name: scan back to the last space; a token containing "/" is a
    // -- Continent/City zone -> tz-switch flag (2). Uses only rax/rdx/r8 so the pending
    // -- zone_len/offset/flag (r10d/r9d/r11d) survive a fall-through bail to zone_done --
    emitter.label("__rt_strtotime_iso_zone_iana_linux_x86_64");
    emitter.instruction("mov rax, rcx");                                        // scan pointer = end of input
    emitter.label("__rt_strtotime_iso_iana_scan_linux_x86_64");
    emitter.instruction("cmp rax, rdi");                                        // reached the input start ?
    emitter.instruction("jbe __rt_strtotime_iso_zone_done_linux_x86_64");       // no space found -> no zone
    emitter.instruction("sub rax, 1");                                          // step back one byte
    emitter.instruction("movzx edx, BYTE PTR [rax]");                           // load the byte
    emitter.instruction("cmp edx, 32");                                         // a space ?
    emitter.instruction("jne __rt_strtotime_iso_iana_scan_linux_x86_64");       // no -> keep scanning back
    emitter.instruction("lea rdx, [rax + 1]");                                  // slash-scan pointer = zone token start
    emitter.instruction("cmp rdx, rcx");                                        // is the token empty ?
    emitter.instruction("jae __rt_strtotime_iso_zone_done_linux_x86_64");       // empty -> no zone
    emitter.label("__rt_strtotime_iso_iana_slash_linux_x86_64");
    emitter.instruction("cmp rdx, rcx");                                        // reached the end with no "/" ?
    emitter.instruction("jae __rt_strtotime_iso_zone_done_linux_x86_64");       // no "/" -> not a Continent/City zone (also the time token)
    emitter.instruction("movzx r8d, BYTE PTR [rdx]");                           // load token byte
    emitter.instruction("cmp r8d, 47");                                         // a "/" ?
    emitter.instruction("je __rt_strtotime_iso_iana_found_linux_x86_64");       // yes -> it is an IANA zone name
    emitter.instruction("add rdx, 1");                                          // next byte
    emitter.instruction("jmp __rt_strtotime_iso_iana_slash_linux_x86_64");      // keep scanning for "/"
    emitter.label("__rt_strtotime_iso_iana_found_linux_x86_64");
    emitter.instruction("lea r8, [rax + 1]");                                   // zone token start = last space + 1
    emitter.instruction("mov QWORD PTR [rsp + 88], r8");                        // save the IANA name pointer
    emitter.instruction("mov r8, rcx");                                         // end pointer
    emitter.instruction("sub r8, rax");                                         // end - last space
    emitter.instruction("sub r8, 1");                                           // zone-name length = end - (last space + 1)
    emitter.instruction("mov DWORD PTR [rsp + 80], r8d");                       // save the IANA name length (32-bit; flag at +84)
    emitter.instruction("mov DWORD PTR [rsp + 84], 2");                         // flag = 2 (IANA tz-switch)
    emitter.instruction("mov rsi, rax");                                        // last-space pointer
    emitter.instruction("sub rsi, rdi");                                        // date/time length = last space - start
    emitter.instruction("mov QWORD PTR [rsp + 56], rsi");                       // persist the reduced length (date/time only)
    emitter.instruction("jmp __rt_strtotime_iso_zone_lencheck_linux_x86_64");   // validate the date/time and parse
    emitter.label("__rt_strtotime_iso_zone_done_linux_x86_64");
    emitter.instruction("test r10d, r10d");                                     // any zone ?
    emitter.instruction("jz __rt_strtotime_iso_zone_strip_linux_x86_64");       // no -> nothing to strip
    emitter.instruction("mov rdx, rcx");                                        // copy end
    emitter.instruction("movsxd rax, r10d");                                    // zone_len as 64-bit
    emitter.instruction("sub rdx, rax");                                        // start of zone
    emitter.instruction("movzx eax, BYTE PTR [rdx - 1]");                       // char before the zone
    emitter.instruction("cmp eax, 32");                                         // a separating space ?
    emitter.instruction("jne __rt_strtotime_iso_zone_strip_linux_x86_64");      // no
    emitter.instruction("add r10d, 1");                                         // also strip the space
    emitter.label("__rt_strtotime_iso_zone_strip_linux_x86_64");
    emitter.instruction("mov DWORD PTR [rsp + 80], r9d");                       // save offset_seconds (32-bit slot)
    emitter.instruction("mov DWORD PTR [rsp + 84], r11d");                      // save the explicit-zone flag (separate slot)
    emitter.instruction("movsxd rax, r10d");                                    // zone_len as 64-bit
    emitter.instruction("sub rsi, rax");                                        // strip the zone from the length
    emitter.instruction("mov QWORD PTR [rsp + 56], rsi");                       // persist the reduced length
    emitter.label("__rt_strtotime_iso_zone_lencheck_linux_x86_64");
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
    emit_x86_64_reject_if_above(emitter, "eax", 12);
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
    emit_x86_64_reject_if_above(emitter, "eax", 31);
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
    emit_x86_64_reject_if_above(emitter, "eax", 24);
    emitter.instruction("mov DWORD PTR [rsp + 8], eax");                        // tm_hour = parsed hour

    emitter.instruction("movzx eax, BYTE PTR [r8 + 14]");                       // load the first minute digit
    emitter.instruction("sub eax, 48");                                         // convert the first minute digit from ASCII to its numeric value
    emit_x86_64_reject_unless_decimal_digit(emitter, "eax");
    emitter.instruction("imul eax, eax, 10");                                   // place the first minute digit into the tens column
    emitter.instruction("movzx ecx, BYTE PTR [r8 + 15]");                       // load the second minute digit
    emitter.instruction("sub ecx, 48");                                         // convert the second minute digit from ASCII to its numeric value
    emit_x86_64_reject_unless_decimal_digit(emitter, "ecx");
    emitter.instruction("add eax, ecx");                                        // finish assembling the minute component
    emit_x86_64_reject_if_above(emitter, "eax", 59);
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
    emit_x86_64_reject_if_above(emitter, "eax", 60);
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
    emitter.instruction("mov eax, DWORD PTR [rsp + 84]");                       // explicit-zone flag set ?
    emitter.instruction("test eax, eax");                                       // set flags for the explicit-zone test
    emitter.instruction("jz __rt_strtotime_iso_local_mk_linux_x86_64");         // no -> default zone
    emitter.instruction("cmp eax, 2");                                          // IANA tz-switch ?
    emitter.instruction("je __rt_strtotime_iso_iana_mk_linux_x86_64");          // yes -> set the zone, mktime, restore
    // explicit zone: timegm(&tm) then subtract the offset
    emitter.instruction("mov rdi, rsp");                                        // &tm
    emitter.bl_c("timegm");                                                     // timegm(&tm) -> rax = UTC time_t
    emitter.instruction("movsxd r11, DWORD PTR [rsp + 80]");                    // reload offset_seconds, sign-extended
    emitter.instruction("sub rax, r11");                                        // UTC instant = timegm(tm) - offset
    emitter.instruction("jmp __rt_strtotime_ret_linux_x86_64");                 // return through the shared epilogue
    // -- IANA zone: save the current default zone, set the parsed zone, mktime, restore --
    emitter.label("__rt_strtotime_iso_iana_mk_linux_x86_64");
    emitter.instruction("call __rt_date_default_timezone_get");                 // rax = current zone id ptr, rdx = len
    emitter.instruction("mov QWORD PTR [rsp + 96], rdx");                       // save the current id length across the calls
    emitter.instruction("lea rsi, [rip + _php_tz_save]");                       // destination save buffer
    emitter.instruction("mov rcx, 0");                                          // copy index
    emitter.label("__rt_strtotime_iana_save_copy_linux_x86_64");
    emitter.instruction("cmp rcx, rdx");                                        // copied the whole id ?
    emitter.instruction("jae __rt_strtotime_iana_save_done_linux_x86_64");      // yes
    emitter.instruction("movzx r8d, BYTE PTR [rax + rcx]");                     // byte from the current id
    emitter.instruction("mov BYTE PTR [rsi + rcx], r8b");                       // into the save buffer
    emitter.instruction("add rcx, 1");                                          // next byte
    emitter.instruction("jmp __rt_strtotime_iana_save_copy_linux_x86_64");      // continue
    emitter.label("__rt_strtotime_iana_save_done_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rsp + 88]");                       // IANA name ptr
    emitter.instruction("mov edx, DWORD PTR [rsp + 80]");                       // IANA name len (32-bit zero-extends into rdx)
    emitter.instruction("call __rt_date_default_timezone_set");                 // set the default zone to the parsed IANA name
    emitter.instruction("mov rdi, rsp");                                        // &tm
    emitter.instruction("call __rt_mktime_shifted");                            // mktime in the IANA zone -> rax = timestamp
    emitter.instruction("mov QWORD PTR [rsp + 104], rax");                      // save the timestamp across the restore call
    emitter.instruction("lea rax, [rip + _php_tz_save]");                       // saved id ptr
    emitter.instruction("mov rdx, QWORD PTR [rsp + 96]");                       // saved id length
    emitter.instruction("call __rt_date_default_timezone_set");                 // restore the previous default zone
    emitter.instruction("mov rax, QWORD PTR [rsp + 104]");                      // reload the timestamp
    emitter.instruction("jmp __rt_strtotime_ret_linux_x86_64");                 // return through the shared epilogue
    emitter.label("__rt_strtotime_iso_local_mk_linux_x86_64");
    emitter.instruction("mov rdi, rsp");                                        // pass &tm to libc mktime
    emitter.instruction("call __rt_mktime_shifted");                            // convert the parsed components into a Unix timestamp
    emitter.instruction("jmp __rt_strtotime_ret_linux_x86_64");                 // return through the shared epilogue
}
