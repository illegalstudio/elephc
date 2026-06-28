//! Purpose:
//! Emits the `__rt_date` / `__rt_gmdate` runtime helper assembly for arm64.
//! `__rt_gmdate` shares the formatter body and only swaps `localtime` for `gmtime` (UTC).
//! Keeps PHP builtin semantics, libc/syscall boundaries, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::system::date::emit_date()` for AArch64 targets.
//!
//! Key details:
//! - Formatting reads libc tm fields and fixed date tables using AArch64 pointer/length return conventions.

use crate::codegen::emit::Emitter;

/// Emits the `__rt_date` and `__rt_gmdate` runtime helpers for arm64.
///
/// Both entry points share one body; `__rt_gmdate` sets `x3=1` so the timestamp is
/// decomposed with `gmtime` (UTC), while `__rt_date` sets `x3=0` for `localtime`.
///
/// Input registers:
/// - `x0`: timestamp (i64), or -1 for current time
/// - `x1`: format string pointer
/// - `x2`: format string length
///
/// Output registers:
/// - `x1`: pointer to formatted result in concat buffer
/// - `x2`: length of formatted result
///
/// The function scans the format string, dispatching on each character.
/// Supported format specifiers: Y, y, X, x, m, n, d, j, D, l, N, w, F, M, H, G, h, g,
/// i, s, A, a, U, S, z, t, L, W, o, O, P, Z, e, T, I, u, v, c, r, p, B.
/// A backslash escapes the next character so it is emitted literally; a lone trailing
/// backslash emits nothing. Other literal characters are copied unchanged. Output is
/// written to the concat buffer and `_concat_off` is updated to reflect the bytes appended.
///
/// Uses a 96-byte stack frame with saved x29/x30 and local variables for
/// timestamp, format ptr/len, tm pointer, output position, and format index.
pub(super) fn emit_date_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: date / gmdate ---");
    // gmdate() shares this formatter; it enters here with the UTC flag set so the
    // timestamp is decomposed with gmtime() instead of localtime().
    emitter.label_global("__rt_gmdate");
    emitter.instruction("mov x3, #1");                                          // select UTC decomposition (gmtime)
    emitter.instruction("b __rt_date_entry");                                   // share the formatter body
    emitter.label_global("__rt_date");
    emitter.instruction("mov x3, #0");                                          // select local decomposition (localtime)
    emitter.label("__rt_date_entry");

    // -- set up stack frame --
    // Stack layout:
    //   [sp+0..7]   = timestamp value
    //   [sp+8..15]  = format ptr
    //   [sp+16..23] = format len
    //   [sp+24..31] = tm pointer (from localtime/gmtime)
    //   [sp+32..39] = output buffer position
    //   [sp+40..47] = output start position
    //   [sp+48..55] = format index
    //   [sp+56..63] = UTC flag (1 = gmtime, 0 = localtime)
    //   [sp+80..95] = saved x29, x30
    emitter.instruction("sub sp, sp, #128");                                    // allocate 128 bytes (extra slots for the c/r format-include)
    emitter.instruction("stp x29, x30, [sp, #112]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // set new frame pointer

    // -- save inputs --
    emitter.instruction("str x0, [sp, #0]");                                    // save timestamp
    emitter.instruction("str x1, [sp, #8]");                                    // save format ptr
    emitter.instruction("str x2, [sp, #16]");                                   // save format len
    emitter.instruction("str x3, [sp, #56]");                                   // save UTC flag across libc calls

    // -- if timestamp is -1, get current time --
    emitter.instruction("cmn x0, #1");                                          // compare x0 with -1 (cmn adds 1, checks if zero)
    emitter.instruction("b.ne __rt_date_have_time");                            // skip if timestamp provided (not -1)
    emitter.instruction("mov x0, #0");                                          // NULL argument
    emitter.bl_c("time");                                            // time(NULL) → x0=current timestamp
    emitter.instruction("str x0, [sp, #0]");                                    // save current timestamp

    // -- decompose timestamp via localtime (local) or gmtime (UTC) --
    emitter.label("__rt_date_have_time");
    emitter.instruction("bl __rt_tz_init_utc");                                 // default the timezone to UTC once the timestamp is resolved (PHP-compatible) unless set
    emitter.instruction("add x0, sp, #0");                                      // x0 = pointer to timestamp on stack
    emitter.instruction("ldr x4, [sp, #56]");                                   // reload the UTC-vs-local decomposition flag
    emitter.instruction("cmp x4, #0");                                          // check whether UTC decomposition was requested
    emitter.instruction("b.ne __rt_date_use_gmtime");                           // nonzero flag → decompose as UTC
    emitter.bl_c("localtime");                                       // localtime(&timestamp) → x0=struct tm (local)
    emitter.instruction("b __rt_date_decomposed");                              // skip the UTC decomposition path
    emitter.label("__rt_date_use_gmtime");
    emitter.bl_c("gmtime");                                          // gmtime(&timestamp) → x0=struct tm (UTC)
    emitter.label("__rt_date_decomposed");
    emitter.instruction("str x0, [sp, #24]");                                   // save tm pointer

    // -- set up output buffer in concat_buf --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current concat offset
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x11, x11, x10");                                   // compute write position: buf + offset
    emitter.instruction("str x11, [sp, #32]");                                  // save output write position
    emitter.instruction("str x11, [sp, #40]");                                  // save output start position

    // -- initialize format index --
    emitter.instruction("str xzr, [sp, #48]");                                  // format index = 0
    emitter.instruction("str xzr, [sp, #80]");                                  // format-include save slot empty (not inside a c/r sub-format)

    // -- main format loop --
    emitter.label("__rt_date_loop");
    emitter.instruction("ldr x12, [sp, #48]");                                  // load format index
    emitter.instruction("ldr x13, [sp, #16]");                                  // load format length
    emitter.instruction("cmp x12, x13");                                        // check if we've processed all chars
    emitter.instruction("b.ge __rt_date_check_pop");                            // index >= len: pop a c/r sub-format or finish

    // -- load current format character --
    emitter.instruction("ldr x14, [sp, #8]");                                   // load format ptr
    emitter.instruction("ldrb w15, [x14, x12]");                                // load format char at index

    // -- check each format character --
    // backslash escapes the next character (output it literally)
    emitter.instruction("cmp w15, #92");                                        // compare with '\\' (92)
    emitter.instruction("b.eq __rt_date_escape");                               // handle escaped literal char
    // Y = 4-digit year
    emitter.instruction("cmp w15, #89");                                        // compare with 'Y' (89)
    emitter.instruction("b.eq __rt_date_fmt_Y");                                // handle year
    // m = month 01-12
    emitter.instruction("cmp w15, #109");                                       // compare with 'm' (109)
    emitter.instruction("b.eq __rt_date_fmt_m");                                // handle month
    // d = day 01-31
    emitter.instruction("cmp w15, #100");                                       // compare with 'd' (100)
    emitter.instruction("b.eq __rt_date_fmt_d");                                // handle day
    // H = hour 00-23
    emitter.instruction("cmp w15, #72");                                        // compare with 'H' (72)
    emitter.instruction("b.eq __rt_date_fmt_H");                                // handle hour
    // i = minute 00-59
    emitter.instruction("cmp w15, #105");                                       // compare with 'i' (105)
    emitter.instruction("b.eq __rt_date_fmt_i");                                // handle minute
    // s = second 00-59
    emitter.instruction("cmp w15, #115");                                       // compare with 's' (115)
    emitter.instruction("b.eq __rt_date_fmt_s");                                // handle second
    // j = day 1-31 (no leading zero)
    emitter.instruction("cmp w15, #106");                                       // compare with 'j' (106)
    emitter.instruction("b.eq __rt_date_fmt_j");                                // handle day no padding
    // n = month 1-12 (no leading zero)
    emitter.instruction("cmp w15, #110");                                       // compare with 'n' (110)
    emitter.instruction("b.eq __rt_date_fmt_n");                                // handle month no padding
    // G = hour 0-23 (no leading zero)
    emitter.instruction("cmp w15, #71");                                        // compare with 'G' (71)
    emitter.instruction("b.eq __rt_date_fmt_G");                                // handle hour no padding

    // g = hour 1-12
    emitter.instruction("cmp w15, #103");                                       // compare with 'g' (103)
    emitter.instruction("b.eq __rt_date_fmt_g");                                // handle 12-hour

    // N = day of week 1-7 (Monday=1)
    emitter.instruction("cmp w15, #78");                                        // compare with 'N' (78)
    emitter.instruction("b.eq __rt_date_fmt_N");                                // handle day of week

    // A = AM/PM
    emitter.instruction("cmp w15, #65");                                        // compare with 'A' (65)
    emitter.instruction("b.eq __rt_date_fmt_A");                                // handle AM/PM uppercase

    // a = am/pm
    emitter.instruction("cmp w15, #97");                                        // compare with 'a' (97)
    emitter.instruction("b.eq __rt_date_fmt_a");                                // handle am/pm lowercase

    // U = Unix timestamp
    emitter.instruction("cmp w15, #85");                                        // compare with 'U' (85)
    emitter.instruction("b.eq __rt_date_fmt_U");                                // handle Unix timestamp

    // l = day name (lowercase L)
    emitter.instruction("cmp w15, #108");                                       // compare with 'l' (108)
    emitter.instruction("b.eq __rt_date_fmt_l");                                // handle day name

    // D = short day name
    emitter.instruction("cmp w15, #68");                                        // compare with 'D' (68)
    emitter.instruction("b.eq __rt_date_fmt_D");                                // handle short day name

    // F = full month name
    emitter.instruction("cmp w15, #70");                                        // compare with 'F' (70)
    emitter.instruction("b.eq __rt_date_fmt_F");                                // handle full month name

    // M = short month name
    emitter.instruction("cmp w15, #77");                                        // compare with 'M' (77)
    emitter.instruction("b.eq __rt_date_fmt_M");                                // handle short month name

    // y = 2-digit year
    emitter.instruction("cmp w15, #121");                                       // compare with 'y' (121)
    emitter.instruction("b.eq __rt_date_fmt_y");                                // handle 2-digit year

    // h = hour 01-12 (zero-padded)
    emitter.instruction("cmp w15, #104");                                       // compare with 'h' (104)
    emitter.instruction("b.eq __rt_date_fmt_h");                                // handle 12-hour zero-padded

    // w = day of week 0-6 (Sunday=0)
    emitter.instruction("cmp w15, #119");                                       // compare with 'w' (119)
    emitter.instruction("b.eq __rt_date_fmt_w");                                // handle numeric weekday

    // z = day of year 0-365
    emitter.instruction("cmp w15, #122");                                       // compare with 'z' (122)
    emitter.instruction("b.eq __rt_date_fmt_z");                                // handle day of year

    // S = English ordinal suffix (st/nd/rd/th)
    emitter.instruction("cmp w15, #83");                                        // compare with 'S' (83)
    emitter.instruction("b.eq __rt_date_fmt_S");                                // handle ordinal suffix

    // t = number of days in the month
    emitter.instruction("cmp w15, #116");                                       // compare with 't' (116)
    emitter.instruction("b.eq __rt_date_fmt_t");                                // handle days in month

    // L = leap year flag (1 or 0)
    emitter.instruction("cmp w15, #76");                                        // compare with 'L' (76)
    emitter.instruction("b.eq __rt_date_fmt_L");                                // handle leap year flag

    // W = ISO-8601 week number (01-53)
    emitter.instruction("cmp w15, #87");                                        // compare with 'W' (87)
    emitter.instruction("b.eq __rt_date_fmt_W");                                // handle ISO week number

    // o = ISO-8601 week-numbering year
    emitter.instruction("cmp w15, #111");                                       // compare with 'o' (111)
    emitter.instruction("b.eq __rt_date_fmt_o");                                // handle ISO year

    // O = timezone offset (+hhmm)
    emitter.instruction("cmp w15, #79");                                        // compare with 'O' (79)
    emitter.instruction("b.eq __rt_date_fmt_O");                                // handle timezone offset as +hhmm

    // P = timezone offset (+hh:mm)
    emitter.instruction("cmp w15, #80");                                        // compare with 'P' (80)
    emitter.instruction("b.eq __rt_date_fmt_P");                                // handle timezone offset as +hh:mm

    // Z = timezone offset in seconds
    emitter.instruction("cmp w15, #90");                                        // compare with 'Z' (90)
    emitter.instruction("b.eq __rt_date_fmt_Z");                                // handle timezone offset in seconds

    // e = timezone identifier (e.g. Europe/Paris)
    emitter.instruction("cmp w15, #101");                                       // compare with 'e' (101)
    emitter.instruction("b.eq __rt_date_fmt_e");                                // handle timezone identifier

    // T = timezone abbreviation (e.g. CEST)
    emitter.instruction("cmp w15, #84");                                        // compare with 'T' (84)
    emitter.instruction("b.eq __rt_date_fmt_T");                                // handle timezone abbreviation

    // I = daylight saving flag (1 if DST in effect, else 0)
    emitter.instruction("cmp w15, #73");                                        // compare with 'I' (73)
    emitter.instruction("b.eq __rt_date_fmt_I");                                // handle DST flag
    // u = microseconds (always 000000 for whole-second timestamps)
    emitter.instruction("cmp w15, #117");                                       // compare with 'u' (117)
    emitter.instruction("b.eq __rt_date_fmt_u");                                // handle microseconds
    // v = milliseconds (always 000 for whole-second timestamps)
    emitter.instruction("cmp w15, #118");                                       // compare with 'v' (118)
    emitter.instruction("b.eq __rt_date_fmt_v");                                // handle milliseconds
    // c = ISO 8601 date (Y-m-d\TH:i:sP)
    emitter.instruction("cmp w15, #99");                                        // compare with 'c' (99)
    emitter.instruction("b.eq __rt_date_fmt_c");                                // handle ISO 8601 composite
    // r = RFC 2822 date (D, d M Y H:i:s O)
    emitter.instruction("cmp w15, #114");                                       // compare with 'r' (114)
    emitter.instruction("b.eq __rt_date_fmt_r");                                // handle RFC 2822 composite
    // p = timezone offset (+hh:mm, or 'Z' when the offset is zero)
    emitter.instruction("cmp w15, #112");                                       // compare with 'p' (112)
    emitter.instruction("b.eq __rt_date_fmt_p");                                // handle offset with Z-for-UTC
    // B = Swatch Internet Time (000-999)
    emitter.instruction("cmp w15, #66");                                        // compare with 'B' (66)
    emitter.instruction("b.eq __rt_date_fmt_B");                                // handle Swatch beats
    // X = expanded year (ISO-8601, always signed, minimum 4 digits)
    emitter.instruction("cmp w15, #88");                                        // compare with 'X' (88)
    emitter.instruction("b.eq __rt_date_fmt_X");                                // handle expanded-year (always signed)
    // x = expanded year (signed only for year < 0 or year >= 10000)
    emitter.instruction("cmp w15, #120");                                       // compare with 'x' (120)
    emitter.instruction("b.eq __rt_date_fmt_x");                                // handle expanded-year (signed when out of range)

    // -- not a format char, copy literal --
    emitter.instruction("ldr x9, [sp, #32]");                                   // load output position
    emitter.instruction("strb w15, [x9]");                                      // store literal char
    emitter.instruction("add x9, x9, #1");                                      // advance output position
    emitter.instruction("str x9, [sp, #32]");                                   // save output position
    emitter.instruction("b __rt_date_next");                                    // continue to next format char

    // -- format: Y (4-digit year = tm_year + 1900) --
    emitter.label("__rt_date_fmt_Y");
    emitter.instruction("ldr x0, [sp, #24]");                                   // load tm pointer
    emitter.instruction("ldr w0, [x0, #20]");                                   // load tm_year (offset 20)
    emitter.instruction("sxtw x0, w0");                                         // sign-extend to 64 bits
    emitter.instruction("add x0, x0, #1900");                                   // tm_year + 1900 = actual year
    emitter.instruction("bl __rt_date_write_4digit");                           // write 4-digit number
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- format: m (month 01-12 = tm_mon + 1) --
    emitter.label("__rt_date_fmt_m");
    emitter.instruction("ldr x0, [sp, #24]");                                   // load tm pointer
    emitter.instruction("ldr w0, [x0, #16]");                                   // load tm_mon (offset 16, 0-based)
    emitter.instruction("sxtw x0, w0");                                         // sign-extend to 64 bits
    emitter.instruction("add x0, x0, #1");                                      // convert to 1-based month
    emitter.instruction("bl __rt_date_write_2digit");                           // write zero-padded 2-digit number
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- format: d (day 01-31 = tm_mday) --
    emitter.label("__rt_date_fmt_d");
    emitter.instruction("ldr x0, [sp, #24]");                                   // load tm pointer
    emitter.instruction("ldr w0, [x0, #12]");                                   // load tm_mday (offset 12)
    emitter.instruction("sxtw x0, w0");                                         // sign-extend to 64 bits
    emitter.instruction("bl __rt_date_write_2digit");                           // write zero-padded 2-digit number
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- format: H (hour 00-23 = tm_hour) --
    emitter.label("__rt_date_fmt_H");
    emitter.instruction("ldr x0, [sp, #24]");                                   // load tm pointer
    emitter.instruction("ldr w0, [x0, #8]");                                    // load tm_hour (offset 8)
    emitter.instruction("sxtw x0, w0");                                         // sign-extend to 64 bits
    emitter.instruction("bl __rt_date_write_2digit");                           // write zero-padded 2-digit number
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- format: i (minute 00-59 = tm_min) --
    emitter.label("__rt_date_fmt_i");
    emitter.instruction("ldr x0, [sp, #24]");                                   // load tm pointer
    emitter.instruction("ldr w0, [x0, #4]");                                    // load tm_min (offset 4)
    emitter.instruction("sxtw x0, w0");                                         // sign-extend to 64 bits
    emitter.instruction("bl __rt_date_write_2digit");                           // write zero-padded 2-digit number
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- format: s (second 00-59 = tm_sec) --
    emitter.label("__rt_date_fmt_s");
    emitter.instruction("ldr x0, [sp, #24]");                                   // load tm pointer
    emitter.instruction("ldr w0, [x0, #0]");                                    // load tm_sec (offset 0)
    emitter.instruction("sxtw x0, w0");                                         // sign-extend to 64 bits
    emitter.instruction("bl __rt_date_write_2digit");                           // write zero-padded 2-digit number
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- format: j (day 1-31, no leading zero) --
    emitter.label("__rt_date_fmt_j");
    emitter.instruction("ldr x0, [sp, #24]");                                   // load tm pointer
    emitter.instruction("ldr w0, [x0, #12]");                                   // load tm_mday
    emitter.instruction("sxtw x0, w0");                                         // sign-extend to 64 bits
    emitter.instruction("bl __rt_date_write_num");                              // write number without padding
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- format: n (month 1-12, no leading zero) --
    emitter.label("__rt_date_fmt_n");
    emitter.instruction("ldr x0, [sp, #24]");                                   // load tm pointer
    emitter.instruction("ldr w0, [x0, #16]");                                   // load tm_mon (0-based)
    emitter.instruction("sxtw x0, w0");                                         // sign-extend to 64 bits
    emitter.instruction("add x0, x0, #1");                                      // convert to 1-based
    emitter.instruction("bl __rt_date_write_num");                              // write number without padding
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- format: G (hour 0-23, no leading zero) --
    emitter.label("__rt_date_fmt_G");
    emitter.instruction("ldr x0, [sp, #24]");                                   // load tm pointer
    emitter.instruction("ldr w0, [x0, #8]");                                    // load tm_hour
    emitter.instruction("sxtw x0, w0");                                         // sign-extend to 64 bits
    emitter.instruction("bl __rt_date_write_num");                              // write number without padding
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- format: g (hour 1-12) --
    emitter.label("__rt_date_fmt_g");
    emitter.instruction("ldr x0, [sp, #24]");                                   // load tm pointer
    emitter.instruction("ldr w0, [x0, #8]");                                    // load tm_hour (0-23)
    emitter.instruction("sxtw x0, w0");                                         // sign-extend to 64 bits
    emitter.instruction("cmp x0, #0");                                          // check if hour is 0
    emitter.instruction("b.eq __rt_date_g_12");                                 // midnight → 12
    emitter.instruction("cmp x0, #12");                                         // check if hour > 12
    emitter.instruction("b.le __rt_date_g_write");                              // if <= 12, use as is
    emitter.instruction("sub x0, x0, #12");                                     // convert 13-23 to 1-11
    emitter.instruction("b __rt_date_g_write");                                 // write the value
    emitter.label("__rt_date_g_12");
    emitter.instruction("mov x0, #12");                                         // midnight/noon = 12
    emitter.label("__rt_date_g_write");
    emitter.instruction("bl __rt_date_write_num");                              // write number without padding
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- format: N (day of week 1=Monday, 7=Sunday) --
    emitter.label("__rt_date_fmt_N");
    emitter.instruction("ldr x0, [sp, #24]");                                   // load tm pointer
    emitter.instruction("ldr w0, [x0, #24]");                                   // load tm_wday (0=Sunday)
    emitter.instruction("sxtw x0, w0");                                         // sign-extend to 64 bits
    emitter.instruction("cmp x0, #0");                                          // check if Sunday
    emitter.instruction("b.ne __rt_date_N_ok");                                 // if not Sunday, use wday directly
    emitter.instruction("mov x0, #7");                                          // Sunday = 7 in ISO
    emitter.label("__rt_date_N_ok");
    emitter.instruction("bl __rt_date_write_num");                              // write number
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- format: A (AM/PM) --
    emitter.label("__rt_date_fmt_A");
    emitter.instruction("ldr x0, [sp, #24]");                                   // load tm pointer
    emitter.instruction("ldr w0, [x0, #8]");                                    // load tm_hour
    emitter.instruction("cmp w0, #12");                                         // compare with 12
    emitter.instruction("b.ge __rt_date_A_pm");                                 // if >= 12, it's PM
    // AM
    emitter.instruction("ldr x9, [sp, #32]");                                   // load output position
    emitter.instruction("mov w10, #65");                                        // 'A'
    emitter.instruction("strb w10, [x9]");                                      // write 'A'
    emitter.instruction("mov w10, #77");                                        // 'M'
    emitter.instruction("strb w10, [x9, #1]");                                  // write 'M'
    emitter.instruction("add x9, x9, #2");                                      // advance 2 bytes
    emitter.instruction("str x9, [sp, #32]");                                   // save output position
    emitter.instruction("b __rt_date_next");                                    // continue
    emitter.label("__rt_date_A_pm");
    emitter.instruction("ldr x9, [sp, #32]");                                   // load output position
    emitter.instruction("mov w10, #80");                                        // 'P'
    emitter.instruction("strb w10, [x9]");                                      // write 'P'
    emitter.instruction("mov w10, #77");                                        // 'M'
    emitter.instruction("strb w10, [x9, #1]");                                  // write 'M'
    emitter.instruction("add x9, x9, #2");                                      // advance 2 bytes
    emitter.instruction("str x9, [sp, #32]");                                   // save output position
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- format: a (am/pm) --
    emitter.label("__rt_date_fmt_a");
    emitter.instruction("ldr x0, [sp, #24]");                                   // load tm pointer
    emitter.instruction("ldr w0, [x0, #8]");                                    // load tm_hour
    emitter.instruction("cmp w0, #12");                                         // compare with 12
    emitter.instruction("b.ge __rt_date_a_pm");                                 // if >= 12, it's pm
    // am
    emitter.instruction("ldr x9, [sp, #32]");                                   // load output position
    emitter.instruction("mov w10, #97");                                        // 'a'
    emitter.instruction("strb w10, [x9]");                                      // write 'a'
    emitter.instruction("mov w10, #109");                                       // 'm'
    emitter.instruction("strb w10, [x9, #1]");                                  // write 'm'
    emitter.instruction("add x9, x9, #2");                                      // advance 2 bytes
    emitter.instruction("str x9, [sp, #32]");                                   // save output position
    emitter.instruction("b __rt_date_next");                                    // continue
    emitter.label("__rt_date_a_pm");
    emitter.instruction("ldr x9, [sp, #32]");                                   // load output position
    emitter.instruction("mov w10, #112");                                       // 'p'
    emitter.instruction("strb w10, [x9]");                                      // write 'p'
    emitter.instruction("mov w10, #109");                                       // 'm'
    emitter.instruction("strb w10, [x9, #1]");                                  // write 'm'
    emitter.instruction("add x9, x9, #2");                                      // advance 2 bytes
    emitter.instruction("str x9, [sp, #32]");                                   // save output position
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- format: U (Unix timestamp) --
    emitter.label("__rt_date_fmt_U");
    emitter.instruction("ldr x0, [sp, #0]");                                    // load original timestamp
    emitter.instruction("bl __rt_date_write_uint");                             // write as decimal digits to the live output position
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- format: l (full day name) --
    emitter.label("__rt_date_fmt_l");
    emitter.instruction("ldr x0, [sp, #24]");                                   // load tm pointer
    emitter.instruction("ldr w0, [x0, #24]");                                   // load tm_wday (0=Sunday)
    emitter.instruction("sxtw x0, w0");                                         // sign-extend
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_day_names");      // load page of day names table
    // Each day name entry is 12 bytes: 10 chars + 1 length byte + 1 padding
    emitter.instruction("mov x2, #12");                                         // entry size
    emitter.instruction("mul x2, x0, x2");                                      // offset = wday * 12
    emitter.instruction("add x1, x1, x2");                                      // point to the entry
    emitter.instruction("ldrb w2, [x1, #10]");                                  // load name length byte
    // -- copy day name to output --
    emitter.instruction("ldr x9, [sp, #32]");                                   // load output position
    emitter.instruction("mov x10, #0");                                         // copy index
    emitter.label("__rt_date_l_copy");
    emitter.instruction("cmp x10, x2");                                         // check if all bytes copied
    emitter.instruction("b.ge __rt_date_l_done");                               // done
    emitter.instruction("ldrb w11, [x1, x10]");                                 // load day name byte
    emitter.instruction("strb w11, [x9, x10]");                                 // store to output
    emitter.instruction("add x10, x10, #1");                                    // increment index
    emitter.instruction("b __rt_date_l_copy");                                  // continue
    emitter.label("__rt_date_l_done");
    emitter.instruction("add x9, x9, x2");                                      // advance output position
    emitter.instruction("str x9, [sp, #32]");                                   // save output position
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- format: D (short day name = first 3 chars of day name) --
    emitter.label("__rt_date_fmt_D");
    emitter.instruction("ldr x0, [sp, #24]");                                   // load tm pointer
    emitter.instruction("ldr w0, [x0, #24]");                                   // load tm_wday
    emitter.instruction("sxtw x0, w0");                                         // sign-extend
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_day_names");      // load page of day names
    emitter.instruction("mov x2, #12");                                         // entry size
    emitter.instruction("mul x2, x0, x2");                                      // offset
    emitter.instruction("add x1, x1, x2");                                      // point to entry
    // -- copy first 3 chars --
    emitter.instruction("ldr x9, [sp, #32]");                                   // load output position
    emitter.instruction("ldrb w10, [x1, #0]");                                  // load 1st char
    emitter.instruction("strb w10, [x9, #0]");                                  // store 1st char
    emitter.instruction("ldrb w10, [x1, #1]");                                  // load 2nd char
    emitter.instruction("strb w10, [x9, #1]");                                  // store 2nd char
    emitter.instruction("ldrb w10, [x1, #2]");                                  // load 3rd char
    emitter.instruction("strb w10, [x9, #2]");                                  // store 3rd char
    emitter.instruction("add x9, x9, #3");                                      // advance output by 3
    emitter.instruction("str x9, [sp, #32]");                                   // save output position
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- format: F (full month name) --
    emitter.label("__rt_date_fmt_F");
    emitter.instruction("ldr x0, [sp, #24]");                                   // load tm pointer
    emitter.instruction("ldr w0, [x0, #16]");                                   // load tm_mon (0-based)
    emitter.instruction("sxtw x0, w0");                                         // sign-extend
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_month_names");    // load page of month names
    // Each month entry is 12 bytes: 10 chars + 1 length byte + 1 padding
    emitter.instruction("mov x2, #12");                                         // entry size
    emitter.instruction("mul x2, x0, x2");                                      // offset = mon * 12
    emitter.instruction("add x1, x1, x2");                                      // point to entry
    emitter.instruction("ldrb w2, [x1, #10]");                                  // load name length byte
    // -- copy month name to output --
    emitter.instruction("ldr x9, [sp, #32]");                                   // load output position
    emitter.instruction("mov x10, #0");                                         // copy index
    emitter.label("__rt_date_F_copy");
    emitter.instruction("cmp x10, x2");                                         // check if all bytes copied
    emitter.instruction("b.ge __rt_date_F_done");                               // done
    emitter.instruction("ldrb w11, [x1, x10]");                                 // load month name byte
    emitter.instruction("strb w11, [x9, x10]");                                 // store to output
    emitter.instruction("add x10, x10, #1");                                    // increment index
    emitter.instruction("b __rt_date_F_copy");                                  // continue
    emitter.label("__rt_date_F_done");
    emitter.instruction("add x9, x9, x2");                                      // advance output position
    emitter.instruction("str x9, [sp, #32]");                                   // save output position
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- format: M (short month name = first 3 chars) --
    emitter.label("__rt_date_fmt_M");
    emitter.instruction("ldr x0, [sp, #24]");                                   // load tm pointer
    emitter.instruction("ldr w0, [x0, #16]");                                   // load tm_mon
    emitter.instruction("sxtw x0, w0");                                         // sign-extend
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_month_names");    // load page of month names
    emitter.instruction("mov x2, #12");                                         // entry size
    emitter.instruction("mul x2, x0, x2");                                      // offset
    emitter.instruction("add x1, x1, x2");                                      // point to entry
    // -- copy first 3 chars --
    emitter.instruction("ldr x9, [sp, #32]");                                   // load output position
    emitter.instruction("ldrb w10, [x1, #0]");                                  // load 1st char
    emitter.instruction("strb w10, [x9, #0]");                                  // store 1st char
    emitter.instruction("ldrb w10, [x1, #1]");                                  // load 2nd char
    emitter.instruction("strb w10, [x9, #1]");                                  // store 2nd char
    emitter.instruction("ldrb w10, [x1, #2]");                                  // load 3rd char
    emitter.instruction("strb w10, [x9, #2]");                                  // store 3rd char
    emitter.instruction("add x9, x9, #3");                                      // advance output by 3
    emitter.instruction("str x9, [sp, #32]");                                   // save output position
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- format: y (2-digit year = (tm_year + 1900) mod 100) --
    emitter.label("__rt_date_fmt_y");
    emitter.instruction("ldr x0, [sp, #24]");                                   // load tm pointer
    emitter.instruction("ldr w0, [x0, #20]");                                   // load tm_year (years since 1900)
    emitter.instruction("sxtw x0, w0");                                         // sign-extend to 64 bits
    emitter.instruction("add x0, x0, #1900");                                   // tm_year + 1900 = full year
    emitter.instruction("mov x3, #100");                                        // divisor = 100
    emitter.instruction("udiv x4, x0, x3");                                     // century = year / 100
    emitter.instruction("msub x0, x4, x3, x0");                                 // year mod 100 = year - century*100
    emitter.instruction("bl __rt_date_write_2digit");                           // write zero-padded 2-digit year
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- format: h (hour 01-12, zero-padded) --
    emitter.label("__rt_date_fmt_h");
    emitter.instruction("ldr x0, [sp, #24]");                                   // load tm pointer
    emitter.instruction("ldr w0, [x0, #8]");                                    // load tm_hour (0-23)
    emitter.instruction("sxtw x0, w0");                                         // sign-extend to 64 bits
    emitter.instruction("cmp x0, #0");                                          // check if hour is 0
    emitter.instruction("b.eq __rt_date_h_12");                                 // midnight → 12
    emitter.instruction("cmp x0, #12");                                         // check if hour <= 12
    emitter.instruction("b.le __rt_date_h_write");                              // if <= 12, use as is
    emitter.instruction("sub x0, x0, #12");                                     // convert 13-23 to 1-11
    emitter.instruction("b __rt_date_h_write");                                 // write the value
    emitter.label("__rt_date_h_12");
    emitter.instruction("mov x0, #12");                                         // midnight/noon = 12
    emitter.label("__rt_date_h_write");
    emitter.instruction("bl __rt_date_write_2digit");                           // write zero-padded 2-digit hour
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- format: w (day of week 0-6, Sunday=0) --
    emitter.label("__rt_date_fmt_w");
    emitter.instruction("ldr x0, [sp, #24]");                                   // load tm pointer
    emitter.instruction("ldr w0, [x0, #24]");                                   // load tm_wday (0=Sun..6=Sat)
    emitter.instruction("sxtw x0, w0");                                         // sign-extend to 64 bits
    emitter.instruction("bl __rt_date_write_num");                              // write single-digit weekday
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- format: z (day of year 0-365, no padding) --
    emitter.label("__rt_date_fmt_z");
    emitter.instruction("ldr x0, [sp, #24]");                                   // load tm pointer
    emitter.instruction("ldr w0, [x0, #28]");                                   // load tm_yday (0-365)
    emitter.instruction("sxtw x0, w0");                                         // sign-extend to 64 bits
    emitter.instruction("bl __rt_date_write_uint");                             // write as decimal digits to the live output position
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- format: S (English ordinal suffix for tm_mday) --
    emitter.label("__rt_date_fmt_S");
    emitter.instruction("ldr x0, [sp, #24]");                                   // load tm pointer
    emitter.instruction("ldr w0, [x0, #12]");                                   // load tm_mday (1-31)
    emitter.instruction("sxtw x0, w0");                                         // sign-extend to 64 bits
    emitter.instruction("cmp x0, #11");                                         // 11th → "th"
    emitter.instruction("b.eq __rt_date_S_th");                                 // special-case 11
    emitter.instruction("cmp x0, #12");                                         // 12th → "th"
    emitter.instruction("b.eq __rt_date_S_th");                                 // special-case 12
    emitter.instruction("cmp x0, #13");                                         // 13th → "th"
    emitter.instruction("b.eq __rt_date_S_th");                                 // special-case 13
    emitter.instruction("mov x3, #10");                                         // divisor = 10
    emitter.instruction("udiv x4, x0, x3");                                     // tens = day / 10
    emitter.instruction("msub x5, x4, x3, x0");                                 // last digit = day mod 10
    emitter.instruction("cmp x5, #1");                                          // last digit 1 → "st"
    emitter.instruction("b.eq __rt_date_S_st");                                 // first ordinal
    emitter.instruction("cmp x5, #2");                                          // last digit 2 → "nd"
    emitter.instruction("b.eq __rt_date_S_nd");                                 // second ordinal
    emitter.instruction("cmp x5, #3");                                          // last digit 3 → "rd"
    emitter.instruction("b.eq __rt_date_S_rd");                                 // third ordinal
    emitter.label("__rt_date_S_th");
    emitter.instruction("mov w10, #116");                                       // 't'
    emitter.instruction("mov w11, #104");                                       // 'h'
    emitter.instruction("b __rt_date_S_emit");                                  // emit "th"
    emitter.label("__rt_date_S_st");
    emitter.instruction("mov w10, #115");                                       // 's'
    emitter.instruction("mov w11, #116");                                       // 't'
    emitter.instruction("b __rt_date_S_emit");                                  // emit "st"
    emitter.label("__rt_date_S_nd");
    emitter.instruction("mov w10, #110");                                       // 'n'
    emitter.instruction("mov w11, #100");                                       // 'd'
    emitter.instruction("b __rt_date_S_emit");                                  // emit "nd"
    emitter.label("__rt_date_S_rd");
    emitter.instruction("mov w10, #114");                                       // 'r'
    emitter.instruction("mov w11, #100");                                       // 'd'
    emitter.label("__rt_date_S_emit");
    emitter.instruction("ldr x9, [sp, #32]");                                   // load output position
    emitter.instruction("strb w10, [x9]");                                      // write first suffix char
    emitter.instruction("strb w11, [x9, #1]");                                  // write second suffix char
    emitter.instruction("add x9, x9, #2");                                      // advance output by 2
    emitter.instruction("str x9, [sp, #32]");                                   // save output position
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- format: t (number of days in the month) --
    emitter.label("__rt_date_fmt_t");
    emitter.instruction("ldr x6, [sp, #24]");                                   // load tm pointer (kept in x6)
    emitter.instruction("ldr w1, [x6, #16]");                                   // load tm_mon (0-based)
    emitter.instruction("sxtw x1, w1");                                         // sign-extend month index
    emitter.adrp("x2", "_days_in_month");                            // load page of days-in-month table
    emitter.add_lo12("x2", "x2", "_days_in_month");                      // resolve days-in-month address
    emitter.instruction("ldrb w7, [x2, x1]");                                   // w7 = base days for this month
    emitter.instruction("cmp x1, #1");                                          // is it February (index 1)?
    emitter.instruction("b.ne __rt_date_t_write");                              // non-February uses table value directly
    // -- February: extend to 29 days in leap years --
    emitter.instruction("ldr w0, [x6, #20]");                                   // load tm_year (years since 1900)
    emitter.instruction("sxtw x0, w0");                                         // sign-extend to 64 bits
    emitter.instruction("add x0, x0, #1900");                                   // tm_year + 1900 = full year
    emitter.instruction("mov x3, #4");                                          // divisor = 4
    emitter.instruction("udiv x4, x0, x3");                                     // year / 4
    emitter.instruction("msub x5, x4, x3, x0");                                 // year mod 4
    emitter.instruction("cmp x5, #0");                                          // divisible by 4?
    emitter.instruction("b.ne __rt_date_t_write");                              // not divisible by 4 → 28 days
    emitter.instruction("mov x3, #100");                                        // divisor = 100
    emitter.instruction("udiv x4, x0, x3");                                     // year / 100
    emitter.instruction("msub x5, x4, x3, x0");                                 // year mod 100
    emitter.instruction("cmp x5, #0");                                          // divisible by 100?
    emitter.instruction("b.ne __rt_date_t_feb29");                              // div by 4, not 100 → leap → 29
    emitter.instruction("mov x3, #400");                                        // divisor = 400
    emitter.instruction("udiv x4, x0, x3");                                     // year / 400
    emitter.instruction("msub x5, x4, x3, x0");                                 // year mod 400
    emitter.instruction("cmp x5, #0");                                          // divisible by 400?
    emitter.instruction("b.ne __rt_date_t_write");                              // div by 100, not 400 → 28 days
    emitter.label("__rt_date_t_feb29");
    emitter.instruction("mov w7, #29");                                         // leap February has 29 days
    emitter.label("__rt_date_t_write");
    emitter.instruction("mov x0, x7");                                          // move day count into x0 for the writer
    emitter.instruction("bl __rt_date_write_num");                              // write 1- or 2-digit day count
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- format: L (leap year flag: 1 or 0) --
    emitter.label("__rt_date_fmt_L");
    emitter.instruction("ldr x0, [sp, #24]");                                   // load tm pointer
    emitter.instruction("ldr w0, [x0, #20]");                                   // load tm_year (years since 1900)
    emitter.instruction("sxtw x0, w0");                                         // sign-extend to 64 bits
    emitter.instruction("add x0, x0, #1900");                                   // tm_year + 1900 = full year
    emitter.instruction("mov x3, #4");                                          // divisor = 4
    emitter.instruction("udiv x4, x0, x3");                                     // year / 4
    emitter.instruction("msub x5, x4, x3, x0");                                 // year mod 4
    emitter.instruction("cmp x5, #0");                                          // divisible by 4?
    emitter.instruction("b.ne __rt_date_L_no");                                 // not divisible by 4 → not leap
    emitter.instruction("mov x3, #100");                                        // divisor = 100
    emitter.instruction("udiv x4, x0, x3");                                     // year / 100
    emitter.instruction("msub x5, x4, x3, x0");                                 // year mod 100
    emitter.instruction("cmp x5, #0");                                          // divisible by 100?
    emitter.instruction("b.ne __rt_date_L_yes");                                // div by 4, not 100 → leap
    emitter.instruction("mov x3, #400");                                        // divisor = 400
    emitter.instruction("udiv x4, x0, x3");                                     // year / 400
    emitter.instruction("msub x5, x4, x3, x0");                                 // year mod 400
    emitter.instruction("cmp x5, #0");                                          // divisible by 400?
    emitter.instruction("b.eq __rt_date_L_yes");                                // div by 400 → leap
    emitter.label("__rt_date_L_no");
    emitter.instruction("mov w10, #48");                                        // '0' (not a leap year)
    emitter.instruction("b __rt_date_L_emit");                                  // emit the flag
    emitter.label("__rt_date_L_yes");
    emitter.instruction("mov w10, #49");                                        // '1' (leap year)
    emitter.label("__rt_date_L_emit");
    emitter.instruction("ldr x9, [sp, #32]");                                   // load output position
    emitter.instruction("strb w10, [x9]");                                      // write leap-year flag char
    emitter.instruction("add x9, x9, #1");                                      // advance output by 1
    emitter.instruction("str x9, [sp, #32]");                                   // save output position
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- backslash escape: output the next character literally --
    // -- format: I (1 if daylight saving is in effect, else 0) --
    emitter.label("__rt_date_fmt_I");
    emitter.instruction("ldr x0, [sp, #24]");                                   // load tm pointer
    emitter.instruction("ldr w0, [x0, #32]");                                   // load tm_isdst (offset 32)
    emitter.instruction("ldr x9, [sp, #32]");                                   // load output position
    emitter.instruction("mov w10, #48");                                        // default '0' (not in DST)
    emitter.instruction("cmp w0, #0");                                          // is the DST flag positive?
    emitter.instruction("b.le __rt_date_I_store");                              // <= 0 means not in DST, keep 0
    emitter.instruction("mov w10, #49");                                        // '1' (DST in effect)
    emitter.label("__rt_date_I_store");
    emitter.instruction("strb w10, [x9]");                                      // write the DST flag digit
    emitter.instruction("add x9, x9, #1");                                      // advance output position
    emitter.instruction("str x9, [sp, #32]");                                   // save output position
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- format: u (microseconds; whole-second timestamps carry none) --
    emitter.label("__rt_date_fmt_u");
    emitter.instruction("ldr x9, [sp, #32]");                                   // load output position
    emitter.instruction("mov w10, #48");                                        // ASCII '0'
    emitter.instruction("strb w10, [x9]");                                      // write microsecond digit 1
    emitter.instruction("strb w10, [x9, #1]");                                  // write microsecond digit 2
    emitter.instruction("strb w10, [x9, #2]");                                  // write microsecond digit 3
    emitter.instruction("strb w10, [x9, #3]");                                  // write microsecond digit 4
    emitter.instruction("strb w10, [x9, #4]");                                  // write microsecond digit 5
    emitter.instruction("strb w10, [x9, #5]");                                  // write microsecond digit 6
    emitter.instruction("add x9, x9, #6");                                      // advance past the 6 digits
    emitter.instruction("str x9, [sp, #32]");                                   // save output position
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- format: v (milliseconds; whole-second timestamps carry none) --
    emitter.label("__rt_date_fmt_v");
    emitter.instruction("ldr x9, [sp, #32]");                                   // load output position
    emitter.instruction("mov w10, #48");                                        // ASCII '0'
    emitter.instruction("strb w10, [x9]");                                      // write millisecond digit 1
    emitter.instruction("strb w10, [x9, #1]");                                  // write millisecond digit 2
    emitter.instruction("strb w10, [x9, #2]");                                  // write millisecond digit 3
    emitter.instruction("add x9, x9, #3");                                      // advance past the 3 digits
    emitter.instruction("str x9, [sp, #32]");                                   // save output position
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- format: c (ISO 8601): switch to the sub-format and reuse the main loop --
    emitter.label("__rt_date_fmt_c");
    emitter.instruction("ldr x10, [sp, #8]");                                   // current main format pointer
    emitter.instruction("str x10, [sp, #80]");                                  // save it (also marks that a sub-format is active)
    emitter.instruction("ldr x10, [sp, #16]");                                  // current main format length
    emitter.instruction("str x10, [sp, #88]");                                  // save the main format length
    emitter.instruction("ldr x10, [sp, #48]");                                  // current main index (at this token)
    emitter.instruction("str x10, [sp, #96]");                                  // save the main index
    emitter.adrp("x10", "_date_fmt_c");                                         // page of the ISO 8601 sub-format
    emitter.add_lo12("x10", "x10", "_date_fmt_c");                              // resolve the sub-format address
    emitter.instruction("str x10, [sp, #8]");                                   // switch the format pointer to the sub-format
    emitter.instruction("mov x10, #13");                                        // ISO 8601 sub-format length
    emitter.instruction("str x10, [sp, #16]");                                  // switch the format length
    emitter.instruction("str xzr, [sp, #48]");                                  // restart the index for the sub-format
    emitter.instruction("b __rt_date_loop");                                    // process the sub-format through the main loop

    // -- format: r (RFC 2822): switch to the sub-format and reuse the main loop --
    emitter.label("__rt_date_fmt_r");
    emitter.instruction("ldr x10, [sp, #8]");                                   // current main format pointer
    emitter.instruction("str x10, [sp, #80]");                                  // save it (also marks that a sub-format is active)
    emitter.instruction("ldr x10, [sp, #16]");                                  // current main format length
    emitter.instruction("str x10, [sp, #88]");                                  // save the main format length
    emitter.instruction("ldr x10, [sp, #48]");                                  // current main index (at this token)
    emitter.instruction("str x10, [sp, #96]");                                  // save the main index
    emitter.adrp("x10", "_date_fmt_r");                                         // page of the RFC 2822 sub-format
    emitter.add_lo12("x10", "x10", "_date_fmt_r");                              // resolve the sub-format address
    emitter.instruction("str x10, [sp, #8]");                                   // switch the format pointer to the sub-format
    emitter.instruction("mov x10, #16");                                        // RFC 2822 sub-format length
    emitter.instruction("str x10, [sp, #16]");                                  // switch the format length
    emitter.instruction("str xzr, [sp, #48]");                                  // restart the index for the sub-format
    emitter.instruction("b __rt_date_loop");                                    // process the sub-format through the main loop

    emitter.label("__rt_date_escape");
    emitter.instruction("ldr x12, [sp, #48]");                                  // load format index (points at backslash)
    emitter.instruction("add x12, x12, #1");                                    // advance to the escaped character
    emitter.instruction("str x12, [sp, #48]");                                  // save advanced index
    emitter.instruction("ldr x13, [sp, #16]");                                  // load format length
    emitter.instruction("cmp x12, x13");                                        // is there a character after the backslash?
    emitter.instruction("b.ge __rt_date_loop");                                 // lone trailing backslash → output nothing
    emitter.instruction("ldr x14, [sp, #8]");                                   // load format ptr
    emitter.instruction("ldrb w15, [x14, x12]");                                // load the escaped character
    emitter.instruction("ldr x9, [sp, #32]");                                   // load output position
    emitter.instruction("strb w15, [x9]");                                      // output the escaped character literally
    emitter.instruction("add x9, x9, #1");                                      // advance output position
    emitter.instruction("str x9, [sp, #32]");                                   // save output position
    emitter.instruction("b __rt_date_next");                                    // +1 at next moves past the escaped char

    // -- advance to next format character --
    emitter.label("__rt_date_next");
    emitter.instruction("ldr x12, [sp, #48]");                                  // load format index
    emitter.instruction("add x12, x12, #1");                                    // increment index
    emitter.instruction("str x12, [sp, #48]");                                  // save updated index
    emitter.instruction("b __rt_date_loop");                                    // loop back

    // -- finalize result --

    // -- end of (sub-)format: resume a pending c/r sub-format, or finish --
    emitter.label("__rt_date_check_pop");
    emitter.instruction("ldr x10, [sp, #80]");                                  // saved main format ptr (0 if not inside a sub-format)
    emitter.instruction("cbz x10, __rt_date_done");                             // not in a sub-format -> the whole format is done
    emitter.instruction("str x10, [sp, #8]");                                   // restore the main format pointer
    emitter.instruction("ldr x10, [sp, #88]");                                  // saved main format length
    emitter.instruction("str x10, [sp, #16]");                                  // restore the main format length
    emitter.instruction("ldr x10, [sp, #96]");                                  // saved main index (points at the c/r token)
    emitter.instruction("add x10, x10, #1");                                    // advance past the c/r token
    emitter.instruction("str x10, [sp, #48]");                                  // restore the main index, advanced
    emitter.instruction("str xzr, [sp, #80]");                                  // clear the in-sub marker
    emitter.instruction("b __rt_date_loop");                                    // resume the main format
    emitter.label("__rt_date_done");
    emitter.instruction("ldr x1, [sp, #40]");                                   // x1 = output start
    emitter.instruction("ldr x9, [sp, #32]");                                   // load output end position
    emitter.instruction("sub x2, x9, x1");                                      // x2 = output length

    // -- update concat_off --
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_concat_off");
    emitter.instruction("ldr x11, [x10]");                                      // load current offset
    emitter.instruction("add x11, x11, x2");                                    // add result length
    emitter.instruction("str x11, [x10]");                                      // store updated offset

    // -- tear down stack frame --
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller

    // -- helper: write 2-digit zero-padded number from x0 to output --
    //
    // Input: x0 = value (0-99)
    // Output: writes two ASCII digits to concat buffer at current position,
    //         advances output position by 2, stores updated position to [sp, #32]
    emitter.label("__rt_date_write_2digit");
    emitter.instruction("ldr x9, [sp, #32]");                                   // load output position
    emitter.instruction("mov x3, #10");                                         // divisor = 10
    emitter.instruction("udiv x4, x0, x3");                                     // tens digit
    emitter.instruction("msub x5, x4, x3, x0");                                 // ones digit = value - tens*10
    emitter.instruction("add w4, w4, #48");                                     // convert tens to ASCII
    emitter.instruction("add w5, w5, #48");                                     // convert ones to ASCII
    emitter.instruction("strb w4, [x9]");                                       // write tens digit
    emitter.instruction("strb w5, [x9, #1]");                                   // write ones digit
    emitter.instruction("add x9, x9, #2");                                      // advance by 2
    emitter.instruction("str x9, [sp, #32]");                                   // save output position
    emitter.instruction("ret");                                                 // return to caller

    // -- helper: write 4-digit number from x0 to output --
    //
    // Input: x0 = value (0-9999)
    // Output: writes four ASCII digits to concat buffer at current position,
    //         advances output position by 4, stores updated position to [sp, #32]
    emitter.label("__rt_date_write_4digit");
    emitter.instruction("ldr x9, [sp, #32]");                                   // load output position
    emitter.instruction("mov x3, #1000");                                       // divisor for thousands
    emitter.instruction("udiv x4, x0, x3");                                     // thousands digit
    emitter.instruction("msub x0, x4, x3, x0");                                 // remainder
    emitter.instruction("add w4, w4, #48");                                     // convert to ASCII
    emitter.instruction("strb w4, [x9]");                                       // write thousands digit
    emitter.instruction("mov x3, #100");                                        // divisor for hundreds
    emitter.instruction("udiv x4, x0, x3");                                     // hundreds digit
    emitter.instruction("msub x0, x4, x3, x0");                                 // remainder
    emitter.instruction("add w4, w4, #48");                                     // convert to ASCII
    emitter.instruction("strb w4, [x9, #1]");                                   // write hundreds digit
    emitter.instruction("mov x3, #10");                                         // divisor for tens
    emitter.instruction("udiv x4, x0, x3");                                     // tens digit
    emitter.instruction("msub x5, x4, x3, x0");                                 // ones digit
    emitter.instruction("add w4, w4, #48");                                     // convert to ASCII
    emitter.instruction("add w5, w5, #48");                                     // convert to ASCII
    emitter.instruction("strb w4, [x9, #2]");                                   // write tens digit
    emitter.instruction("strb w5, [x9, #3]");                                   // write ones digit
    emitter.instruction("add x9, x9, #4");                                      // advance by 4
    emitter.instruction("str x9, [sp, #32]");                                   // save output position
    emitter.instruction("ret");                                                 // return to caller

    // -- helper: write number without padding from x0 to output (1-99) --
    //
    // Input: x0 = value (1-99)
    // Output: writes one or two ASCII digits to concat buffer at current position,
    //         advances output position, stores updated position to [sp, #32]
    emitter.label("__rt_date_write_num");
    emitter.instruction("ldr x9, [sp, #32]");                                   // load output position
    emitter.instruction("cmp x0, #10");                                         // check if single digit
    emitter.instruction("b.lt __rt_date_write_num_1");                          // if < 10, write single digit
    // Two digits
    emitter.instruction("mov x3, #10");                                         // divisor
    emitter.instruction("udiv x4, x0, x3");                                     // tens digit
    emitter.instruction("msub x5, x4, x3, x0");                                 // ones digit
    emitter.instruction("add w4, w4, #48");                                     // convert to ASCII
    emitter.instruction("add w5, w5, #48");                                     // convert to ASCII
    emitter.instruction("strb w4, [x9]");                                       // write tens digit
    emitter.instruction("strb w5, [x9, #1]");                                   // write ones digit
    emitter.instruction("add x9, x9, #2");                                      // advance by 2
    emitter.instruction("str x9, [sp, #32]");                                   // save output position
    emitter.instruction("ret");                                                 // return
    emitter.label("__rt_date_write_num_1");
    emitter.instruction("add w0, w0, #48");                                     // convert to ASCII
    emitter.instruction("strb w0, [x9]");                                       // write single digit
    emitter.instruction("add x9, x9, #1");                                      // advance by 1
    emitter.instruction("str x9, [sp, #32]");                                   // save output position
    emitter.instruction("ret");                                                 // return

    // -- format: W (ISO-8601 week number, zero-padded 2 digits) --
    emitter.label("__rt_date_fmt_W");
    emitter.instruction("ldr x0, [sp, #24]");                                   // load tm pointer
    emitter.instruction("bl __rt_date_iso_week");                               // x0 = ISO week, x1 = ISO year
    emitter.instruction("bl __rt_date_write_2digit");                           // write zero-padded 2-digit week
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- format: o (ISO-8601 week-numbering year, 4 digits) --
    emitter.label("__rt_date_fmt_o");
    emitter.instruction("ldr x0, [sp, #24]");                                   // load tm pointer
    emitter.instruction("bl __rt_date_iso_week");                               // x0 = ISO week, x1 = ISO year
    emitter.instruction("mov x0, x1");                                          // move ISO year into the writer register
    emitter.instruction("bl __rt_date_write_4digit");                           // write 4-digit ISO year
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- format: X (expanded year, always signed, minimum 4 digits) --
    emitter.label("__rt_date_fmt_X");
    emitter.instruction("ldr x0, [sp, #24]");                                   // load tm pointer
    emitter.instruction("ldr w0, [x0, #20]");                                   // load tm_year (offset 20)
    emitter.instruction("sxtw x0, w0");                                         // sign-extend to 64 bits
    emitter.instruction("add x0, x0, #1900");                                   // tm_year + 1900 = actual year
    emitter.instruction("mov x1, #1");                                          // force_sign = 1 (X always prints a sign)
    emitter.instruction("b __rt_date_yexp_common");                             // tail-jump to the shared expanded-year body (ends in `b __rt_date_next`)

    // -- format: x (expanded year, signed only outside [0,9999]) --
    emitter.label("__rt_date_fmt_x");
    emitter.instruction("ldr x0, [sp, #24]");                                   // load tm pointer
    emitter.instruction("ldr w0, [x0, #20]");                                   // load tm_year (offset 20)
    emitter.instruction("sxtw x0, w0");                                         // sign-extend to 64 bits
    emitter.instruction("add x0, x0, #1900");                                   // tm_year + 1900 = actual year
    emitter.instruction("mov x1, #0");                                          // force_sign = 0 (x omits sign for [0,9999])
    emitter.instruction("b __rt_date_yexp_common");                             // tail-jump to the shared expanded-year body (ends in `b __rt_date_next`)

    // -- format: Z (timezone offset in seconds, e.g. 7200 or -18000) --
    emitter.label("__rt_date_fmt_Z");
    emitter.instruction("ldr x0, [sp, #24]");                                   // load tm pointer
    emitter.instruction("ldr x0, [x0, #40]");                                   // load tm_gmtoff (signed seconds east of UTC, offset 40)
    emitter.instruction("cmp x0, #0");                                          // is the UTC offset negative?
    emitter.instruction("b.ge __rt_date_Z_mag");                                // non-negative offset prints without a sign
    emitter.instruction("ldr x9, [sp, #32]");                                   // load output position
    emitter.instruction("mov w12, #45");                                        // '-' (45)
    emitter.instruction("strb w12, [x9]");                                      // write the leading minus sign
    emitter.instruction("add x9, x9, #1");                                      // advance output position
    emitter.instruction("str x9, [sp, #32]");                                   // save output position
    emitter.instruction("neg x0, x0");                                          // format the magnitude of the negative offset
    emitter.label("__rt_date_Z_mag");
    emitter.instruction("bl __rt_date_write_uint");                             // write the magnitude as decimal digits
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- helper: write unsigned 64-bit integer in x0 as decimal digits (no padding) --
    //
    // Implementation note: we cannot call `__rt_itoa` from inside the format loop because that
    // helper writes its scratch digits into the shared `_concat_buf` and advances `_concat_off`
    // by 21 bytes, clobbering the format loop's own output area. This helper instead extracts
    // digits right-to-left into the per-frame scratch at [sp+80..112] (32 bytes, more than enough
    // for any 64-bit unsigned value: 20 max digits + headroom) and then copies the resulting
    // byte range into the live output position. Advances [sp, #32] by the digit count. Clobbers
    // x0..x14 (caller-saved per the AAPCS).
    emitter.label("__rt_date_write_uint");
    emitter.instruction("add x10, sp, #111");                                   // digit cursor at end of local 32-byte scratch
    emitter.instruction("mov x11, #0");                                         // digit count = 0
    emitter.label("__rt_date_write_uint_loop");
    emitter.instruction("cbz x0, __rt_date_write_uint_zero_check");             // quotient zero -> all digits extracted
    emitter.instruction("mov x12, #10");                                        // divisor
    emitter.instruction("udiv x13, x0, x12");                                   // quotient = value / 10
    emitter.instruction("msub x14, x13, x12, x0");                              // remainder = value - quotient*10
    emitter.instruction("add w14, w14, #48");                                   // convert remainder to ASCII
    emitter.instruction("strb w14, [x10]");                                     // store one digit at the cursor
    emitter.instruction("sub x10, x10, #1");                                    // move cursor left
    emitter.instruction("add x11, x11, #1");                                    // increment digit count
    emitter.instruction("mov x0, x13");                                         // value = quotient for the next iteration
    emitter.instruction("b __rt_date_write_uint_loop");                         // continue extracting
    emitter.label("__rt_date_write_uint_zero_check");
    emitter.instruction("cbnz x11, __rt_date_write_uint_copy");                 // at least one digit -> skip the zero special case
    emitter.instruction("mov w12, #48");                                        // '0' (48) for the input value 0
    emitter.instruction("strb w12, [sp, #111]");                                // store the lone zero at the end of the scratch
    emitter.instruction("mov x10, sp");                                         // reset cursor so x10+1 lands on the zero byte
    emitter.instruction("add x10, x10, #110");                                  // x10+1 -> the zero byte at [sp+111]
    emitter.instruction("mov x11, #1");                                         // digit count = 1
    emitter.label("__rt_date_write_uint_copy");
    emitter.instruction("add x1, x10, #1");                                     // x1 = start of the digit string
    emitter.instruction("mov x2, x11");                                         // x2 = digit count
    emitter.instruction("ldr x9, [sp, #32]");                                   // load output position
    emitter.instruction("mov x12, #0");                                         // copy index = 0
    emitter.label("__rt_date_write_uint_copy_loop");
    emitter.instruction("cmp x12, x2");                                         // copied every digit?
    emitter.instruction("b.ge __rt_date_write_uint_done");                      // yes -> finish
    emitter.instruction("ldrb w13, [x1, x12]");                                 // load one digit from the scratch
    emitter.instruction("strb w13, [x9, x12]");                                 // store the digit into the output buffer
    emitter.instruction("add x12, x12, #1");                                    // advance the copy index
    emitter.instruction("b __rt_date_write_uint_copy_loop");                    // continue copying
    emitter.label("__rt_date_write_uint_done");
    emitter.instruction("add x9, x9, x2");                                      // advance output position by the digit count
    emitter.instruction("str x9, [sp, #32]");                                   // save output position
    emitter.instruction("ret");                                                 // return to caller

    // -- shared expanded-year body reached via `b` from fmt_X / fmt_x (not a bl-callable fn) --
    //
    // Input: x0 = year (signed), x1 = force_sign (1 => always sign; 0 => sign only for year<0 or year>=10000).
    // Emits a sign then the magnitude with at least 4 digits, reusing write_4digit (<=9999)
    // or write_uint (>=10000). Reached by tail-jump and ends in `b __rt_date_next` (never `ret`), so the
    // `bl write_4digit/write_uint` inside may clobber LR freely like every other format handler.
    emitter.label("__rt_date_yexp_common");
    emitter.instruction("mov x13, #10000");                                     // 4-digit threshold (ARM64 cmp imm is 12-bit; 9999 is not encodable)
    emitter.instruction("cmp x0, #0");                                          // is the year negative?
    emitter.instruction("b.ge __rt_date_yexp_nonneg");                          // non-negative year keeps its magnitude
    emitter.instruction("ldr x9, [sp, #32]");                                   // load output position
    emitter.instruction("mov w12, #45");                                        // '-' (45) for a negative year
    emitter.instruction("strb w12, [x9]");                                      // write the minus sign
    emitter.instruction("add x9, x9, #1");                                      // advance output position
    emitter.instruction("str x9, [sp, #32]");                                   // save output position
    emitter.instruction("neg x0, x0");                                          // magnitude = -year
    emitter.instruction("b __rt_date_yexp_mag");                                // emit the magnitude digits
    emitter.label("__rt_date_yexp_nonneg");
    emitter.instruction("cbnz x1, __rt_date_yexp_plus");                        // X (force_sign) always prints '+'
    emitter.instruction("cmp x0, x13");                                         // x mode: year below the 4-digit threshold?
    emitter.instruction("b.lt __rt_date_yexp_mag");                             // year < 10000 prints no sign
    emitter.label("__rt_date_yexp_plus");
    emitter.instruction("ldr x9, [sp, #32]");                                   // load output position
    emitter.instruction("mov w12, #43");                                        // '+' (43) for a non-negative expanded year
    emitter.instruction("strb w12, [x9]");                                      // write the plus sign
    emitter.instruction("add x9, x9, #1");                                      // advance output position
    emitter.instruction("str x9, [sp, #32]");                                   // save output position
    emitter.label("__rt_date_yexp_mag");
    emitter.instruction("cmp x0, x13");                                         // does the magnitude fit in 4 digits?
    emitter.instruction("b.lt __rt_date_yexp_4");                               // <10000 uses the 4-digit writer
    emitter.instruction("bl __rt_date_write_uint");                             // >=10000 writes the full decimal magnitude
    emitter.instruction("b __rt_date_next");                                    // continue with the next format byte
    emitter.label("__rt_date_yexp_4");
    emitter.instruction("bl __rt_date_write_4digit");                           // write the zero-padded 4-digit magnitude
    emitter.instruction("b __rt_date_next");                                    // continue with the next format byte

    // -- format: O / P (timezone offset as +hhmm or +hh:mm) --
    emitter.label("__rt_date_fmt_O");
    emitter.instruction("mov x12, #0");                                         // colon flag = 0 (no ':' separator for 'O')
    emitter.instruction("str x12, [sp, #72]");                                  // stash the colon flag for the shared body
    emitter.instruction("b __rt_date_OP_common");                               // share the offset body with 'P'
    emitter.label("__rt_date_fmt_P");
    emitter.instruction("mov x12, #1");                                         // colon flag = 1 (insert ':' separator for 'P')
    emitter.instruction("str x12, [sp, #72]");                                  // stash the colon flag for the shared body
    emitter.label("__rt_date_OP_common");
    emitter.instruction("ldr x0, [sp, #24]");                                   // load tm pointer
    emitter.instruction("ldr x0, [x0, #40]");                                   // load tm_gmtoff (signed seconds east of UTC, offset 40)
    emitter.instruction("mov w12, #43");                                        // assume a '+' sign (43)
    emitter.instruction("cmp x0, #0");                                          // is the UTC offset negative?
    emitter.instruction("b.ge __rt_date_OP_sign");                              // non-negative -> keep the '+' sign
    emitter.instruction("mov w12, #45");                                        // '-' (45) for a negative offset
    emitter.instruction("neg x0, x0");                                          // format the magnitude of the negative offset
    emitter.label("__rt_date_OP_sign");
    emitter.instruction("ldr x9, [sp, #32]");                                   // load output position
    emitter.instruction("strb w12, [x9]");                                      // write the sign character
    emitter.instruction("add x9, x9, #1");                                      // advance output position
    emitter.instruction("str x9, [sp, #32]");                                   // save output position
    emitter.instruction("mov x13, #3600");                                      // seconds per hour
    emitter.instruction("udiv x14, x0, x13");                                   // hours = offset / 3600
    emitter.instruction("msub x15, x14, x13, x0");                              // remaining seconds = offset - hours*3600
    emitter.instruction("mov x13, #60");                                        // seconds per minute
    emitter.instruction("udiv x15, x15, x13");                                  // minutes = remaining seconds / 60
    emitter.instruction("str x15, [sp, #64]");                                  // save minutes across the 2-digit writer call
    emitter.instruction("mov x0, x14");                                         // hours -> 2-digit writer input register
    emitter.instruction("bl __rt_date_write_2digit");                           // write zero-padded 2-digit hours
    emitter.instruction("ldr x16, [sp, #72]");                                  // reload the colon flag
    emitter.instruction("cmp x16, #0");                                         // does this specifier use a ':' separator?
    emitter.instruction("b.eq __rt_date_OP_min");                               // no -> skip the colon
    emitter.instruction("ldr x9, [sp, #32]");                                   // load output position
    emitter.instruction("mov w12, #58");                                        // ':' (58)
    emitter.instruction("strb w12, [x9]");                                      // write the ':' separator
    emitter.instruction("add x9, x9, #1");                                      // advance output position
    emitter.instruction("str x9, [sp, #32]");                                   // save output position
    emitter.label("__rt_date_OP_min");
    emitter.instruction("ldr x0, [sp, #64]");                                   // reload minutes
    emitter.instruction("bl __rt_date_write_2digit");                           // write zero-padded 2-digit minutes
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- format: p (timezone offset as +hh:mm, or the literal 'Z' when UTC) --
    emitter.label("__rt_date_fmt_p");
    emitter.instruction("ldr x0, [sp, #24]");                                   // load tm pointer
    emitter.instruction("ldr x0, [x0, #40]");                                   // load tm_gmtoff (signed seconds east of UTC, offset 40)
    emitter.instruction("cbnz x0, __rt_date_p_offset");                         // non-zero offset → render exactly like 'P'
    emitter.instruction("ldr x9, [sp, #32]");                                   // load output position
    emitter.instruction("mov w12, #90");                                        // 'Z' (90) marks a zero UTC offset
    emitter.instruction("strb w12, [x9]");                                      // write the 'Z'
    emitter.instruction("add x9, x9, #1");                                      // advance output position
    emitter.instruction("str x9, [sp, #32]");                                   // save output position
    emitter.instruction("b __rt_date_next");                                    // continue
    emitter.label("__rt_date_p_offset");
    emitter.instruction("mov x12, #1");                                         // colon flag = 1 (insert ':' separator like 'P')
    emitter.instruction("str x12, [sp, #72]");                                  // stash the colon flag for the shared body
    emitter.instruction("b __rt_date_OP_common");                               // share the offset body with 'O'/'P'

    // -- format: B (Swatch Internet Time: beats of the UTC+1 day, 000-999) --
    emitter.label("__rt_date_fmt_B");
    emitter.instruction("ldr x0, [sp, #0]");                                    // load the original Unix timestamp (UTC-based)
    emitter.instruction("add x0, x0, #3600");                                   // shift to Biel Mean Time (UTC+1)
    emitter.instruction("movz x13, #0x5180");                                   // load 86400 (seconds per day), low 16 bits
    emitter.instruction("movk x13, #0x1, lsl #16");                             // load 86400, high bits
    emitter.instruction("sdiv x14, x0, x13");                                   // truncated quotient of the BMT timestamp
    emitter.instruction("msub x15, x14, x13, x0");                              // remainder (carries the dividend's sign)
    emitter.instruction("cmp x15, #0");                                         // negative remainder (pre-epoch timestamp)?
    emitter.instruction("b.ge __rt_date_B_scaled");                             // non-negative → already the seconds of the BMT day
    emitter.instruction("add x15, x15, x13");                                   // floor-mod into [0, 86400)
    emitter.label("__rt_date_B_scaled");
    emitter.instruction("mov x13, #10");                                        // scale so beats = seconds*10/864 (one beat = 86.4 s)
    emitter.instruction("mul x15, x15, x13");                                   // seconds of the BMT day * 10
    emitter.instruction("mov x13, #864");                                       // scaled divisor for one beat
    emitter.instruction("udiv x0, x15, x13");                                   // beats 0-999
    emitter.instruction("mov x13, #100");                                       // split off the hundreds digit
    emitter.instruction("udiv x14, x0, x13");                                   // hundreds digit value
    emitter.instruction("msub x15, x14, x13, x0");                              // beats % 100
    emitter.instruction("mov x13, #10");                                        // split tens and units
    emitter.instruction("udiv x16, x15, x13");                                  // tens digit value
    emitter.instruction("msub x17, x16, x13, x15");                             // units digit value
    emitter.instruction("ldr x9, [sp, #32]");                                   // load output position
    emitter.instruction("add w14, w14, #48");                                   // hundreds digit → ASCII
    emitter.instruction("strb w14, [x9]");                                      // write the hundreds digit
    emitter.instruction("add w16, w16, #48");                                   // tens digit → ASCII
    emitter.instruction("strb w16, [x9, #1]");                                  // write the tens digit
    emitter.instruction("add w17, w17, #48");                                   // units digit → ASCII
    emitter.instruction("strb w17, [x9, #2]");                                  // write the units digit
    emitter.instruction("add x9, x9, #3");                                      // advance output position past the three digits
    emitter.instruction("str x9, [sp, #32]");                                   // save output position
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- format: T (timezone abbreviation from tm_zone, e.g. CEST/CET/UTC) --
    emitter.label("__rt_date_fmt_T");
    emitter.instruction("ldr x4, [sp, #56]");                                   // load the UTC-vs-local flag (1 = gmdate)
    emitter.instruction("cbz x4, __rt_date_T_local");                           // date() → read libc tm_zone
    // gmdate() always reports "GMT" regardless of libc's tm_zone (macOS gmtime yields "UTC").
    emitter.instruction("ldr x9, [sp, #32]");                                   // load output position
    emitter.instruction("mov w11, #71");                                        // 'G'
    emitter.instruction("strb w11, [x9]");                                      // write 'G'
    emitter.instruction("mov w11, #77");                                        // 'M'
    emitter.instruction("strb w11, [x9, #1]");                                  // write 'M'
    emitter.instruction("mov w11, #84");                                        // 'T'
    emitter.instruction("strb w11, [x9, #2]");                                  // write 'T'
    emitter.instruction("add x9, x9, #3");                                      // advance output position past "GMT"
    emitter.instruction("str x9, [sp, #32]");                                   // save the advanced output position
    emitter.instruction("b __rt_date_T_done");                                  // continue
    emitter.label("__rt_date_T_local");
    emitter.instruction("ldr x1, [sp, #24]");                                   // load tm pointer
    emitter.instruction("ldr x1, [x1, #48]");                                   // load tm_zone (char* abbreviation, offset 48)
    emitter.instruction("cbz x1, __rt_date_T_done");                            // no abbreviation available → emit nothing
    emitter.instruction("ldr x9, [sp, #32]");                                   // load output position
    emitter.label("__rt_date_T_copy");
    emitter.instruction("ldrb w11, [x1]");                                      // load one abbreviation byte
    emitter.instruction("cbz w11, __rt_date_T_save");                           // NUL terminator → finish
    emitter.instruction("strb w11, [x9]");                                      // store the byte into the output buffer
    emitter.instruction("add x9, x9, #1");                                      // advance output position
    emitter.instruction("add x1, x1, #1");                                      // advance the abbreviation pointer
    emitter.instruction("b __rt_date_T_copy");                                  // continue copying
    emitter.label("__rt_date_T_save");
    emitter.instruction("str x9, [sp, #32]");                                   // save the advanced output position
    emitter.label("__rt_date_T_done");
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- format: e (timezone identifier: gmdate→UTC, else the configured default zone) --
    emitter.label("__rt_date_fmt_e");
    emitter.instruction("ldr x4, [sp, #56]");                                   // load the UTC-vs-local flag
    emitter.instruction("cbnz x4, __rt_date_e_utc");                            // gmdate() always reports UTC
    crate::codegen::abi::emit_symbol_address(emitter, "x3", "_php_default_tz_len");
    emitter.instruction("ldr x2, [x3]");                                        // load the configured identifier length
    emitter.instruction("cbz x2, __rt_date_e_utc");                             // none configured → UTC
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_php_tz_env");
    emitter.instruction("add x1, x1, #3");                                      // skip the "TZ=" prefix → identifier pointer
    emitter.instruction("b __rt_date_e_copy");                                  // copy the configured identifier
    emitter.label("__rt_date_e_utc");
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_php_tz_utc");
    emitter.instruction("mov x2, #3");                                          // length of "UTC"
    emitter.label("__rt_date_e_copy");
    emitter.instruction("ldr x9, [sp, #32]");                                   // load output position
    emitter.instruction("mov x10, #0");                                         // copy index = 0
    emitter.label("__rt_date_e_loop");
    emitter.instruction("cmp x10, x2");                                         // copied every byte?
    emitter.instruction("b.ge __rt_date_e_done");                               // yes → finish
    emitter.instruction("ldrb w11, [x1, x10]");                                 // load one identifier byte
    emitter.instruction("strb w11, [x9, x10]");                                 // store it into the output buffer
    emitter.instruction("add x10, x10, #1");                                    // advance the copy index
    emitter.instruction("b __rt_date_e_loop");                                  // continue copying
    emitter.label("__rt_date_e_done");
    emitter.instruction("add x9, x9, x2");                                      // advance output position by the identifier length
    emitter.instruction("str x9, [sp, #32]");                                   // save output position
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- helper: ISO-8601 week + year from struct tm (x0=tm ptr → x0=week, x1=year) --
    emitter.label("__rt_date_iso_week");
    emitter.instruction("sub sp, sp, #16");                                     // sub-frame (this helper calls weeks_in_year)
    emitter.instruction("str x30, [sp, #0]");                                   // save link register
    emitter.instruction("ldr w2, [x0, #24]");                                   // tm_wday (0=Sunday)
    emitter.instruction("ldr w3, [x0, #28]");                                   // tm_yday (0-based day of year)
    emitter.instruction("ldr w4, [x0, #20]");                                   // tm_year (years since 1900)
    emitter.instruction("add w4, w4, #1900");                                   // full Gregorian year
    emitter.instruction("mov w5, #7");                                          // ISO weekday: Sunday maps to 7
    emitter.instruction("cmp w2, #0");                                          // is it Sunday?
    emitter.instruction("csel w5, w5, w2, eq");                                 // iso_dow = (wday==0) ? 7 : wday
    emitter.instruction("add w3, w3, #1");                                      // ordinal day = tm_yday + 1
    emitter.instruction("sub w6, w3, w5");                                      // ordinal - iso_dow
    emitter.instruction("add w6, w6, #10");                                     // + 10 (ISO week offset)
    emitter.instruction("mov w7, #7");                                          // days per week
    emitter.instruction("udiv w6, w6, w7");                                     // candidate ISO week number
    emitter.instruction("cmp w6, #1");                                          // week < 1 → belongs to previous year
    emitter.instruction("b.lt __rt_date_iso_prev");                             // handle the early-January case
    emitter.instruction("mov w0, w4");                                          // weeks_in_year argument = this year
    emitter.instruction("bl __rt_date_weeks_in_year");                          // w0 = weeks in this year
    emitter.instruction("cmp w6, w0");                                          // week > weeks_in_year → next year's week 1
    emitter.instruction("b.gt __rt_date_iso_next");                             // handle the late-December case
    emitter.instruction("mov w0, w6");                                          // ISO week = candidate
    emitter.instruction("mov w1, w4");                                          // ISO year = this year
    emitter.instruction("b __rt_date_iso_done");                                // done
    emitter.label("__rt_date_iso_prev");
    emitter.instruction("sub w4, w4, #1");                                      // ISO year = previous year
    emitter.instruction("mov w0, w4");                                          // weeks_in_year argument = previous year
    emitter.instruction("bl __rt_date_weeks_in_year");                          // w0 = ISO week (last week of prev year)
    emitter.instruction("mov w1, w4");                                          // ISO year = previous year
    emitter.instruction("b __rt_date_iso_done");                                // done
    emitter.label("__rt_date_iso_next");
    emitter.instruction("mov w0, #1");                                          // ISO week = 1
    emitter.instruction("add w1, w4, #1");                                      // ISO year = next year
    emitter.label("__rt_date_iso_done");
    emitter.instruction("ldr x30, [sp, #0]");                                   // restore link register
    emitter.instruction("add sp, sp, #16");                                     // release the sub-frame
    emitter.instruction("ret");                                                 // return to caller

    // -- helper: number of ISO weeks in a year (w0=year → w0=52 or 53) --
    emitter.label("__rt_date_weeks_in_year");
    emitter.instruction("sub sp, sp, #16");                                     // sub-frame (this helper calls dow_dec31)
    emitter.instruction("str x30, [sp, #0]");                                   // save link register
    emitter.instruction("str w0, [sp, #8]");                                    // save the year argument
    emitter.instruction("bl __rt_date_dow_dec31");                              // w0 = weekday of 31 Dec of this year
    emitter.instruction("cmp w0, #4");                                          // Thursday? → 53-week year
    emitter.instruction("b.eq __rt_date_wiy_53");                               // yes → 53 weeks
    emitter.instruction("ldr w0, [sp, #8]");                                    // reload the year
    emitter.instruction("sub w0, w0, #1");                                      // previous year
    emitter.instruction("bl __rt_date_dow_dec31");                              // w0 = weekday of 31 Dec of previous year
    emitter.instruction("cmp w0, #3");                                          // Wednesday? → 53-week year (leap)
    emitter.instruction("b.eq __rt_date_wiy_53");                               // yes → 53 weeks
    emitter.instruction("mov w0, #52");                                         // otherwise 52 weeks
    emitter.instruction("b __rt_date_wiy_done");                                // done
    emitter.label("__rt_date_wiy_53");
    emitter.instruction("mov w0, #53");                                         // 53-week year
    emitter.label("__rt_date_wiy_done");
    emitter.instruction("ldr x30, [sp, #0]");                                   // restore link register
    emitter.instruction("add sp, sp, #16");                                     // release the sub-frame
    emitter.instruction("ret");                                                 // return to caller

    // -- helper: weekday of 31 December (w0=year → w0=0..6, 0=Sunday) --
    emitter.label("__rt_date_dow_dec31");
    emitter.instruction("mov w1, w0");                                          // accumulator = year
    emitter.instruction("mov w2, #4");                                          // divisor 4
    emitter.instruction("udiv w3, w0, w2");                                     // year / 4
    emitter.instruction("add w1, w1, w3");                                      // + leap-year contribution
    emitter.instruction("mov w2, #100");                                        // divisor 100
    emitter.instruction("udiv w3, w0, w2");                                     // year / 100
    emitter.instruction("sub w1, w1, w3");                                      // - century contribution
    emitter.instruction("mov w2, #400");                                        // divisor 400
    emitter.instruction("udiv w3, w0, w2");                                     // year / 400
    emitter.instruction("add w1, w1, w3");                                      // + 400-year contribution
    emitter.instruction("mov w2, #7");                                          // days per week
    emitter.instruction("udiv w3, w1, w2");                                     // accumulator / 7
    emitter.instruction("msub w0, w3, w2, w1");                                 // accumulator mod 7 = weekday of 31 Dec
    emitter.instruction("ret");                                                 // return to caller
}
