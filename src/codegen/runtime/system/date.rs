use crate::codegen::{emit::Emitter, platform::Arch};

/// __rt_date: format a Unix timestamp according to a PHP date format string.
/// Input:  x0=timestamp (-1 = use current time), x1=format_ptr, x2=format_len
/// Output: x1=result ptr (in concat_buf), x2=result len
///
/// Supports format characters: Y, m, d, H, i, s, l, F, D, M, N, j, n, G, A, a, U, g
///
/// Uses libc _time, _localtime to get struct tm components.
/// struct tm layout: tm_sec(+0), tm_min(+4), tm_hour(+8), tm_mday(+12),
///                   tm_mon(+16), tm_year(+20), tm_wday(+24), tm_yday(+28), tm_isdst(+32)
pub(crate) fn emit_date(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_date_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: date ---");
    emitter.label_global("__rt_date");

    // -- set up stack frame --
    // Stack layout:
    //   [sp+0..7]   = timestamp value
    //   [sp+8..15]  = format ptr
    //   [sp+16..23] = format len
    //   [sp+24..31] = tm pointer (from localtime)
    //   [sp+32..39] = output buffer position
    //   [sp+40..47] = output start position
    //   [sp+48..55] = format index
    //   [sp+64..79] = saved x29, x30
    emitter.instruction("sub sp, sp, #96");                                     // allocate 96 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // set new frame pointer

    // -- save inputs --
    emitter.instruction("str x0, [sp, #0]");                                    // save timestamp
    emitter.instruction("str x1, [sp, #8]");                                    // save format ptr
    emitter.instruction("str x2, [sp, #16]");                                   // save format len

    // -- if timestamp is -1, get current time --
    emitter.instruction("cmn x0, #1");                                          // compare x0 with -1 (cmn adds 1, checks if zero)
    emitter.instruction("b.ne __rt_date_have_time");                            // skip if timestamp provided (not -1)
    emitter.instruction("mov x0, #0");                                          // NULL argument
    emitter.bl_c("time");                                            // time(NULL) → x0=current timestamp
    emitter.instruction("str x0, [sp, #0]");                                    // save current timestamp

    // -- call localtime to decompose timestamp --
    emitter.label("__rt_date_have_time");
    emitter.instruction("add x0, sp, #0");                                      // x0 = pointer to timestamp on stack
    emitter.bl_c("localtime");                                       // localtime(&timestamp) → x0=pointer to struct tm
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

    // -- main format loop --
    emitter.label("__rt_date_loop");
    emitter.instruction("ldr x12, [sp, #48]");                                  // load format index
    emitter.instruction("ldr x13, [sp, #16]");                                  // load format length
    emitter.instruction("cmp x12, x13");                                        // check if we've processed all chars
    emitter.instruction("b.ge __rt_date_done");                                 // if index >= len, done

    // -- load current format character --
    emitter.instruction("ldr x14, [sp, #8]");                                   // load format ptr
    emitter.instruction("ldrb w15, [x14, x12]");                                // load format char at index

    // -- check each format character --
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
    emitter.instruction("bl __rt_itoa");                                        // convert to string → x1/x2
    // -- copy itoa result to output buffer --
    emitter.instruction("ldr x9, [sp, #32]");                                   // load output position
    emitter.instruction("mov x10, #0");                                         // copy index
    emitter.label("__rt_date_U_copy");
    emitter.instruction("cmp x10, x2");                                         // check if all bytes copied
    emitter.instruction("b.ge __rt_date_U_done");                               // done copying
    emitter.instruction("ldrb w11, [x1, x10]");                                 // load byte from itoa result
    emitter.instruction("strb w11, [x9, x10]");                                 // store byte to output
    emitter.instruction("add x10, x10, #1");                                    // increment index
    emitter.instruction("b __rt_date_U_copy");                                  // continue copying
    emitter.label("__rt_date_U_done");
    emitter.instruction("add x9, x9, x2");                                      // advance output position by copied length
    emitter.instruction("str x9, [sp, #32]");                                   // save output position
    emitter.instruction("b __rt_date_next");                                    // continue

    // -- format: l (full day name) --
    emitter.label("__rt_date_fmt_l");
    emitter.instruction("ldr x0, [sp, #24]");                                   // load tm pointer
    emitter.instruction("ldr w0, [x0, #24]");                                   // load tm_wday (0=Sunday)
    emitter.instruction("sxtw x0, w0");                                         // sign-extend
    emitter.adrp("x1", "_day_names");                            // load page of day names table
    emitter.add_lo12("x1", "x1", "_day_names");                      // resolve day names address
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
    emitter.adrp("x1", "_day_names");                            // load page of day names
    emitter.add_lo12("x1", "x1", "_day_names");                      // resolve day names address
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
    emitter.adrp("x1", "_month_names");                          // load page of month names
    emitter.add_lo12("x1", "x1", "_month_names");                    // resolve month names address
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
    emitter.adrp("x1", "_month_names");                          // load page of month names
    emitter.add_lo12("x1", "x1", "_month_names");                    // resolve month names address
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

    // -- advance to next format character --
    emitter.label("__rt_date_next");
    emitter.instruction("ldr x12, [sp, #48]");                                  // load format index
    emitter.instruction("add x12, x12, #1");                                    // increment index
    emitter.instruction("str x12, [sp, #48]");                                  // save updated index
    emitter.instruction("b __rt_date_loop");                                    // loop back

    // -- finalize result --
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
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller

    // -- helper: write 2-digit zero-padded number from x0 to output --
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
}

fn emit_date_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: date ---");
    emitter.label_global("__rt_date");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before the date formatter uses stack-backed locals and helpers
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved timestamp, format metadata, and decimal scratch buffer
    emitter.instruction("sub rsp, 128");                                        // reserve aligned local storage for the formatter state plus a small decimal scratch buffer

    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the requested Unix timestamp so helper paths can reload it after libc and formatting calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // save the format-string pointer so the main loop can reload it without depending on caller-saved registers
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // save the format-string length so the loop bound survives helper calls
    emitter.instruction("cmp rax, -1");                                         // check whether the builtin requested "current time" instead of an explicit timestamp
    emitter.instruction("jne __rt_date_have_time_linux_x86_64");                // skip the libc time() query when the caller already supplied an explicit Unix timestamp
    emitter.instruction("xor edi, edi");                                        // pass NULL to libc time() so it only returns the current Unix timestamp value
    emitter.instruction("call time");                                           // query libc for the current Unix timestamp when PHP date() was called without an explicit timestamp
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // store the current Unix timestamp so the rest of the formatter can treat both code paths uniformly

    emitter.label("__rt_date_have_time_linux_x86_64");
    emitter.instruction("lea rdi, [rbp - 8]");                                  // pass a pointer to the saved Unix timestamp as the first argument to libc localtime()
    emitter.instruction("call localtime");                                      // decompose the Unix timestamp into libc's struct tm fields in the current local timezone
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the returned struct tm pointer so each format-token branch can reload the decomposed calendar fields

    emitter.instruction("mov r8, QWORD PTR [rip + _concat_off]");               // load the current concat-buffer offset before appending the formatted date output
    emitter.instruction("mov QWORD PTR [rbp - 64], r8");                        // preserve the original concat-buffer offset for the final global offset update
    emitter.instruction("lea r9, [rip + _concat_buf]");                         // load the base address of the shared concat buffer used for transient string results
    emitter.instruction("add r9, r8");                                          // compute the initial write cursor inside the concat buffer from the saved relative offset
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // save the live write cursor so every token helper can append to the same destination buffer
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // save the formatted string start pointer for the final return value
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // start scanning the format string at byte index zero

    emitter.label("__rt_date_loop_linux_x86_64");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // reload the current format-string byte index before checking for loop completion
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 24]");                       // stop once the format-string byte index reaches the saved format length
    emitter.instruction("jae __rt_date_done_linux_x86_64");                     // finish the formatter once every format byte has been consumed
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload the format-string pointer before reading the current format character
    emitter.instruction("movzx eax, BYTE PTR [r8 + rcx]");                      // load the current format character as an unsigned byte for the token dispatch ladder

    emitter.instruction("cmp al, 89");                                          // check whether the current token is 'Y' for a four-digit Gregorian year
    emitter.instruction("je __rt_date_fmt_Y_linux_x86_64");                     // handle the four-digit Gregorian year token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 109");                                         // check whether the current token is 'm' for a zero-padded month number
    emitter.instruction("je __rt_date_fmt_m_linux_x86_64");                     // handle the zero-padded month token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 100");                                         // check whether the current token is 'd' for a zero-padded day-of-month number
    emitter.instruction("je __rt_date_fmt_d_linux_x86_64");                     // handle the zero-padded day token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 72");                                          // check whether the current token is 'H' for a zero-padded 24-hour clock value
    emitter.instruction("je __rt_date_fmt_H_linux_x86_64");                     // handle the zero-padded 24-hour token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 105");                                         // check whether the current token is 'i' for a zero-padded minute value
    emitter.instruction("je __rt_date_fmt_i_linux_x86_64");                     // handle the zero-padded minute token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 115");                                         // check whether the current token is 's' for a zero-padded second value
    emitter.instruction("je __rt_date_fmt_s_linux_x86_64");                     // handle the zero-padded second token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 106");                                         // check whether the current token is 'j' for an unpadded day-of-month number
    emitter.instruction("je __rt_date_fmt_j_linux_x86_64");                     // handle the unpadded day token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 110");                                         // check whether the current token is 'n' for an unpadded month number
    emitter.instruction("je __rt_date_fmt_n_linux_x86_64");                     // handle the unpadded month token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 71");                                          // check whether the current token is 'G' for an unpadded 24-hour clock value
    emitter.instruction("je __rt_date_fmt_G_linux_x86_64");                     // handle the unpadded 24-hour token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 103");                                         // check whether the current token is 'g' for an unpadded 12-hour clock value
    emitter.instruction("je __rt_date_fmt_g_linux_x86_64");                     // handle the unpadded 12-hour token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 78");                                          // check whether the current token is 'N' for the ISO weekday number
    emitter.instruction("je __rt_date_fmt_N_linux_x86_64");                     // handle the ISO weekday token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 65");                                          // check whether the current token is 'A' for the uppercase AM/PM marker
    emitter.instruction("je __rt_date_fmt_A_linux_x86_64");                     // handle the uppercase AM/PM token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 97");                                          // check whether the current token is 'a' for the lowercase am/pm marker
    emitter.instruction("je __rt_date_fmt_a_linux_x86_64");                     // handle the lowercase am/pm token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 85");                                          // check whether the current token is 'U' for the Unix timestamp decimal form
    emitter.instruction("je __rt_date_fmt_U_linux_x86_64");                     // handle the Unix timestamp token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 108");                                         // check whether the current token is 'l' for the full weekday name
    emitter.instruction("je __rt_date_fmt_l_linux_x86_64");                     // handle the full weekday-name token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 68");                                          // check whether the current token is 'D' for the short weekday name
    emitter.instruction("je __rt_date_fmt_D_linux_x86_64");                     // handle the short weekday-name token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 70");                                          // check whether the current token is 'F' for the full month name
    emitter.instruction("je __rt_date_fmt_F_linux_x86_64");                     // handle the full month-name token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 77");                                          // check whether the current token is 'M' for the short month name
    emitter.instruction("je __rt_date_fmt_M_linux_x86_64");                     // handle the short month-name token through the dedicated x86_64 helper path

    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the live output cursor for literal bytes that are copied directly from the format string
    emitter.instruction("mov BYTE PTR [r9], al");                               // copy the current non-token literal format byte into the output buffer unchanged
    emitter.instruction("add r9, 1");                                           // advance the live output cursor after writing one literal byte
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // publish the advanced output cursor after the literal-byte append
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after copying a literal character

    emitter.label("__rt_date_fmt_Y_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the saved year-since-1900 field
    emitter.instruction("mov eax, DWORD PTR [r8 + 20]");                        // load tm_year from the libc struct tm
    emitter.instruction("add eax, 1900");                                       // convert the libc year-since-1900 encoding into a full Gregorian year
    emitter.instruction("call __rt_date_write_4digit_linux_x86_64");            // append the four-digit Gregorian year to the output buffer
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the year token

    emitter.label("__rt_date_fmt_m_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the zero-based month field
    emitter.instruction("mov eax, DWORD PTR [r8 + 16]");                        // load tm_mon from the libc struct tm
    emitter.instruction("add eax, 1");                                          // convert the libc zero-based month encoding into PHP's 1-based calendar month
    emitter.instruction("call __rt_date_write_2digit_linux_x86_64");            // append the zero-padded calendar month to the output buffer
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the month token

    emitter.label("__rt_date_fmt_d_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the day-of-month field
    emitter.instruction("mov eax, DWORD PTR [r8 + 12]");                        // load tm_mday from the libc struct tm
    emitter.instruction("call __rt_date_write_2digit_linux_x86_64");            // append the zero-padded day-of-month to the output buffer
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the day token

    emitter.label("__rt_date_fmt_H_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the 24-hour field
    emitter.instruction("mov eax, DWORD PTR [r8 + 8]");                         // load tm_hour from the libc struct tm
    emitter.instruction("call __rt_date_write_2digit_linux_x86_64");            // append the zero-padded 24-hour clock value to the output buffer
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the hour token

    emitter.label("__rt_date_fmt_i_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the minute field
    emitter.instruction("mov eax, DWORD PTR [r8 + 4]");                         // load tm_min from the libc struct tm
    emitter.instruction("call __rt_date_write_2digit_linux_x86_64");            // append the zero-padded minute value to the output buffer
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the minute token

    emitter.label("__rt_date_fmt_s_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the second field
    emitter.instruction("mov eax, DWORD PTR [r8 + 0]");                         // load tm_sec from the libc struct tm
    emitter.instruction("call __rt_date_write_2digit_linux_x86_64");            // append the zero-padded second value to the output buffer
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the second token

    emitter.label("__rt_date_fmt_j_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the day-of-month field
    emitter.instruction("mov eax, DWORD PTR [r8 + 12]");                        // load tm_mday from the libc struct tm
    emitter.instruction("call __rt_date_write_num_linux_x86_64");               // append the unpadded day-of-month to the output buffer
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the unpadded day token

    emitter.label("__rt_date_fmt_n_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the zero-based month field
    emitter.instruction("mov eax, DWORD PTR [r8 + 16]");                        // load tm_mon from the libc struct tm
    emitter.instruction("add eax, 1");                                          // convert the libc zero-based month encoding into PHP's 1-based calendar month
    emitter.instruction("call __rt_date_write_num_linux_x86_64");               // append the unpadded calendar month to the output buffer
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the unpadded month token

    emitter.label("__rt_date_fmt_G_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the 24-hour field
    emitter.instruction("mov eax, DWORD PTR [r8 + 8]");                         // load tm_hour from the libc struct tm
    emitter.instruction("call __rt_date_write_num_linux_x86_64");               // append the unpadded 24-hour clock value to the output buffer
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the unpadded hour token

    emitter.label("__rt_date_fmt_g_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the 24-hour field for 12-hour conversion
    emitter.instruction("mov eax, DWORD PTR [r8 + 8]");                         // load tm_hour from the libc struct tm
    emitter.instruction("cmp eax, 0");                                          // detect midnight so PHP's 12-hour token can print 12 instead of 0
    emitter.instruction("je __rt_date_g_midnight_linux_x86_64");                // map midnight to 12 before appending the unpadded 12-hour clock value
    emitter.instruction("cmp eax, 12");                                         // detect afternoon hours that need the 13-23 -> 1-11 conversion
    emitter.instruction("jle __rt_date_g_write_linux_x86_64");                  // keep morning and noon values unchanged when they are already in the 1-12 range
    emitter.instruction("sub eax, 12");                                         // convert afternoon hours from the 24-hour range into the PHP 12-hour range
    emitter.instruction("jmp __rt_date_g_write_linux_x86_64");                  // append the converted 12-hour value after subtracting the noon offset
    emitter.label("__rt_date_g_midnight_linux_x86_64");
    emitter.instruction("mov eax, 12");                                         // map midnight to 12 so PHP's 'g' token matches the expected 12-hour clock convention
    emitter.label("__rt_date_g_write_linux_x86_64");
    emitter.instruction("call __rt_date_write_num_linux_x86_64");               // append the unpadded 12-hour clock value to the output buffer
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the 12-hour token

    emitter.label("__rt_date_fmt_N_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the weekday field
    emitter.instruction("mov eax, DWORD PTR [r8 + 24]");                        // load tm_wday where libc uses Sunday=0 and Monday=1
    emitter.instruction("cmp eax, 0");                                          // detect Sunday so PHP's ISO weekday token can remap it to 7
    emitter.instruction("jne __rt_date_N_write_linux_x86_64");                  // keep Monday-Saturday unchanged because libc already stores them as 1-6
    emitter.instruction("mov eax, 7");                                          // remap Sunday from libc's 0 to PHP's ISO weekday value 7
    emitter.label("__rt_date_N_write_linux_x86_64");
    emitter.instruction("call __rt_date_write_num_linux_x86_64");               // append the ISO weekday number to the output buffer
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the ISO weekday token

    emitter.label("__rt_date_fmt_A_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the hour for the AM/PM decision
    emitter.instruction("mov eax, DWORD PTR [r8 + 8]");                         // load tm_hour from the libc struct tm
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the live output cursor before appending the AM/PM marker
    emitter.instruction("cmp eax, 12");                                         // distinguish morning hours from afternoon hours for PHP's uppercase AM/PM token
    emitter.instruction("jge __rt_date_A_pm_linux_x86_64");                     // choose the PM branch when the hour is 12 or later
    emitter.instruction("mov BYTE PTR [r9 + 0], 65");                           // append 'A' for the uppercase morning marker
    emitter.instruction("mov BYTE PTR [r9 + 1], 77");                           // append 'M' for the uppercase morning marker
    emitter.instruction("add r9, 2");                                           // advance the output cursor after writing the two-byte uppercase AM marker
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // publish the updated output cursor after the uppercase AM append
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the uppercase AM token
    emitter.label("__rt_date_A_pm_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r9 + 0], 80");                           // append 'P' for the uppercase afternoon marker
    emitter.instruction("mov BYTE PTR [r9 + 1], 77");                           // append 'M' for the uppercase afternoon marker
    emitter.instruction("add r9, 2");                                           // advance the output cursor after writing the two-byte uppercase PM marker
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // publish the updated output cursor after the uppercase PM append
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the uppercase PM token

    emitter.label("__rt_date_fmt_a_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the hour for the am/pm decision
    emitter.instruction("mov eax, DWORD PTR [r8 + 8]");                         // load tm_hour from the libc struct tm
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the live output cursor before appending the lowercase am/pm marker
    emitter.instruction("cmp eax, 12");                                         // distinguish morning hours from afternoon hours for PHP's lowercase am/pm token
    emitter.instruction("jge __rt_date_a_pm_linux_x86_64");                     // choose the lowercase pm branch when the hour is 12 or later
    emitter.instruction("mov BYTE PTR [r9 + 0], 97");                           // append 'a' for the lowercase morning marker
    emitter.instruction("mov BYTE PTR [r9 + 1], 109");                          // append 'm' for the lowercase morning marker
    emitter.instruction("add r9, 2");                                           // advance the output cursor after writing the two-byte lowercase am marker
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // publish the updated output cursor after the lowercase am append
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the lowercase am token
    emitter.label("__rt_date_a_pm_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r9 + 0], 112");                          // append 'p' for the lowercase afternoon marker
    emitter.instruction("mov BYTE PTR [r9 + 1], 109");                          // append 'm' for the lowercase afternoon marker
    emitter.instruction("add r9, 2");                                           // advance the output cursor after writing the two-byte lowercase pm marker
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // publish the updated output cursor after the lowercase pm append
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the lowercase pm token

    emitter.label("__rt_date_fmt_U_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the original Unix timestamp so the decimal formatter can append it directly to the output buffer
    emitter.instruction("call __rt_date_write_int64_linux_x86_64");             // append the full Unix timestamp as an unpadded decimal integer without disturbing the global concat cursor
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the Unix timestamp token

    emitter.label("__rt_date_fmt_l_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the weekday index for the full weekday name table
    emitter.instruction("mov eax, DWORD PTR [r8 + 24]");                        // load tm_wday where libc uses Sunday=0 and Saturday=6
    emitter.instruction("imul rax, rax, 12");                                   // convert the weekday index into the 12-byte table stride used by the runtime day-name data
    emitter.instruction("lea r9, [rip + _day_names]");                          // load the base address of the runtime weekday-name lookup table
    emitter.instruction("add r9, rax");                                         // advance to the selected weekday-name entry inside the runtime lookup table
    emitter.instruction("movzx ecx, BYTE PTR [r9 + 10]");                       // load the selected weekday-name length from the table metadata byte
    emitter.instruction("xor r10, r10");                                        // start a byte-copy index at zero before appending the full weekday name
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the live output cursor before copying the selected weekday-name bytes
    emitter.label("__rt_date_l_copy_linux_x86_64");
    emitter.instruction("cmp r10, rcx");                                        // stop once every byte of the selected full weekday name has been copied
    emitter.instruction("jae __rt_date_l_done_linux_x86_64");                   // finish the full weekday-name copy once the saved length has been exhausted
    emitter.instruction("mov al, BYTE PTR [r9 + r10]");                         // load one byte from the selected full weekday-name entry
    emitter.instruction("mov BYTE PTR [r11 + r10], al");                        // write that byte into the current output buffer position
    emitter.instruction("add r10, 1");                                          // advance the full weekday-name copy index after moving one byte
    emitter.instruction("jmp __rt_date_l_copy_linux_x86_64");                   // continue copying bytes until the full weekday name is exhausted
    emitter.label("__rt_date_l_done_linux_x86_64");
    emitter.instruction("add r11, rcx");                                        // advance the live output cursor by the copied weekday-name byte count
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // publish the updated output cursor after copying the full weekday name
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the full weekday-name token

    emitter.label("__rt_date_fmt_D_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the weekday index for the short weekday name table
    emitter.instruction("mov eax, DWORD PTR [r8 + 24]");                        // load tm_wday where libc uses Sunday=0 and Saturday=6
    emitter.instruction("imul rax, rax, 12");                                   // convert the weekday index into the 12-byte table stride used by the runtime day-name data
    emitter.instruction("lea r9, [rip + _day_names]");                          // load the base address of the runtime weekday-name lookup table
    emitter.instruction("add r9, rax");                                         // advance to the selected weekday-name entry inside the runtime lookup table
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the live output cursor before appending the three-byte short weekday name
    emitter.instruction("mov al, BYTE PTR [r9 + 0]");                           // load the first byte of the selected weekday name
    emitter.instruction("mov BYTE PTR [r11 + 0], al");                          // write the first byte of the short weekday name into the output buffer
    emitter.instruction("mov al, BYTE PTR [r9 + 1]");                           // load the second byte of the selected weekday name
    emitter.instruction("mov BYTE PTR [r11 + 1], al");                          // write the second byte of the short weekday name into the output buffer
    emitter.instruction("mov al, BYTE PTR [r9 + 2]");                           // load the third byte of the selected weekday name
    emitter.instruction("mov BYTE PTR [r11 + 2], al");                          // write the third byte of the short weekday name into the output buffer
    emitter.instruction("add r11, 3");                                          // advance the output cursor by the fixed three-byte short weekday-name width
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // publish the updated output cursor after copying the short weekday name
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the short weekday-name token

    emitter.label("__rt_date_fmt_F_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the month index for the full month-name table
    emitter.instruction("mov eax, DWORD PTR [r8 + 16]");                        // load tm_mon where libc uses January=0 and December=11
    emitter.instruction("imul rax, rax, 12");                                   // convert the month index into the 12-byte table stride used by the runtime month-name data
    emitter.instruction("lea r9, [rip + _month_names]");                        // load the base address of the runtime month-name lookup table
    emitter.instruction("add r9, rax");                                         // advance to the selected month-name entry inside the runtime lookup table
    emitter.instruction("movzx ecx, BYTE PTR [r9 + 10]");                       // load the selected month-name length from the table metadata byte
    emitter.instruction("xor r10, r10");                                        // start a byte-copy index at zero before appending the full month name
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the live output cursor before copying the selected month-name bytes
    emitter.label("__rt_date_F_copy_linux_x86_64");
    emitter.instruction("cmp r10, rcx");                                        // stop once every byte of the selected full month name has been copied
    emitter.instruction("jae __rt_date_F_done_linux_x86_64");                   // finish the full month-name copy once the saved length has been exhausted
    emitter.instruction("mov al, BYTE PTR [r9 + r10]");                         // load one byte from the selected full month-name entry
    emitter.instruction("mov BYTE PTR [r11 + r10], al");                        // write that byte into the current output buffer position
    emitter.instruction("add r10, 1");                                          // advance the full month-name copy index after moving one byte
    emitter.instruction("jmp __rt_date_F_copy_linux_x86_64");                   // continue copying bytes until the full month name is exhausted
    emitter.label("__rt_date_F_done_linux_x86_64");
    emitter.instruction("add r11, rcx");                                        // advance the live output cursor by the copied month-name byte count
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // publish the updated output cursor after copying the full month name
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the full month-name token

    emitter.label("__rt_date_fmt_M_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the month index for the short month-name table
    emitter.instruction("mov eax, DWORD PTR [r8 + 16]");                        // load tm_mon where libc uses January=0 and December=11
    emitter.instruction("imul rax, rax, 12");                                   // convert the month index into the 12-byte table stride used by the runtime month-name data
    emitter.instruction("lea r9, [rip + _month_names]");                        // load the base address of the runtime month-name lookup table
    emitter.instruction("add r9, rax");                                         // advance to the selected month-name entry inside the runtime lookup table
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the live output cursor before appending the three-byte short month name
    emitter.instruction("mov al, BYTE PTR [r9 + 0]");                           // load the first byte of the selected month name
    emitter.instruction("mov BYTE PTR [r11 + 0], al");                          // write the first byte of the short month name into the output buffer
    emitter.instruction("mov al, BYTE PTR [r9 + 1]");                           // load the second byte of the selected month name
    emitter.instruction("mov BYTE PTR [r11 + 1], al");                          // write the second byte of the short month name into the output buffer
    emitter.instruction("mov al, BYTE PTR [r9 + 2]");                           // load the third byte of the selected month name
    emitter.instruction("mov BYTE PTR [r11 + 2], al");                          // write the third byte of the short month name into the output buffer
    emitter.instruction("add r11, 3");                                          // advance the output cursor by the fixed three-byte short month-name width
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // publish the updated output cursor after copying the short month name
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the short month-name token

    emitter.label("__rt_date_next_linux_x86_64");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // reload the current format-string byte index before stepping to the next token or literal
    emitter.instruction("add rcx, 1");                                          // advance the format-string byte index after consuming one token or literal character
    emitter.instruction("mov QWORD PTR [rbp - 56], rcx");                       // publish the advanced format-string byte index for the next loop iteration
    emitter.instruction("jmp __rt_date_loop_linux_x86_64");                     // continue scanning the format string until every byte has been consumed

    emitter.label("__rt_date_done_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // return the formatted-string start pointer in the standard x86_64 string result register
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // reload the live output cursor so the final string length can be computed from the written byte count
    emitter.instruction("sub rdx, rax");                                        // compute the formatted-string length from the distance between the output cursor and the start pointer
    emitter.instruction("mov r8, QWORD PTR [rbp - 64]");                        // reload the original concat-buffer offset that was active before formatting started
    emitter.instruction("add r8, rdx");                                         // advance the global concat-buffer offset by the number of bytes written by the formatter
    emitter.instruction("mov QWORD PTR [rip + _concat_off], r8");               // publish the updated concat-buffer offset for later transient string helpers
    emitter.instruction("add rsp, 128");                                        // release the formatter locals and decimal scratch buffer before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the formatted date string
    emitter.instruction("ret");                                                 // return the formatted date string pointer and length through the standard x86_64 string result registers

    emitter.label("__rt_date_write_2digit_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // reload the live output cursor before appending the zero-padded two-digit decimal field
    emitter.instruction("xor edx, edx");                                        // clear the implicit high half of the dividend before the unsigned divide-by-10 step
    emitter.instruction("mov ecx, 10");                                         // load the constant decimal divisor used to split the value into tens and ones digits
    emitter.instruction("div ecx");                                             // divide the input value by ten so eax=quotient and edx=remainder for decimal digit emission
    emitter.instruction("add al, 48");                                          // convert the tens digit quotient to its ASCII decimal character
    emitter.instruction("mov BYTE PTR [r8 + 0], al");                           // append the tens digit to the output buffer
    emitter.instruction("add dl, 48");                                          // convert the ones digit remainder to its ASCII decimal character
    emitter.instruction("mov BYTE PTR [r8 + 1], dl");                           // append the ones digit to the output buffer
    emitter.instruction("add r8, 2");                                           // advance the live output cursor after appending the two decimal digits
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // publish the updated output cursor after the two-digit append
    emitter.instruction("ret");                                                 // return to the caller token branch after appending the zero-padded two-digit field

    emitter.label("__rt_date_write_4digit_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // reload the live output cursor before appending the four-digit decimal field
    emitter.instruction("xor edx, edx");                                        // clear the implicit high half of the dividend before the divide-by-1000 step
    emitter.instruction("mov ecx, 1000");                                       // load the constant decimal divisor used to extract the thousands digit
    emitter.instruction("div ecx");                                             // split the input into the thousands digit in eax and the remaining three digits in edx
    emitter.instruction("add al, 48");                                          // convert the thousands digit to its ASCII decimal character
    emitter.instruction("mov BYTE PTR [r8 + 0], al");                           // append the thousands digit to the output buffer
    emitter.instruction("mov eax, edx");                                        // move the remaining three digits into the dividend register for the hundreds extraction step
    emitter.instruction("xor edx, edx");                                        // clear the implicit high half of the dividend before the divide-by-100 step
    emitter.instruction("mov ecx, 100");                                        // load the constant decimal divisor used to extract the hundreds digit
    emitter.instruction("div ecx");                                             // split the remaining three digits into the hundreds digit in eax and the remaining two digits in edx
    emitter.instruction("add al, 48");                                          // convert the hundreds digit to its ASCII decimal character
    emitter.instruction("mov BYTE PTR [r8 + 1], al");                           // append the hundreds digit to the output buffer
    emitter.instruction("mov eax, edx");                                        // move the remaining two digits into the dividend register for the final divide-by-10 step
    emitter.instruction("xor edx, edx");                                        // clear the implicit high half of the dividend before the divide-by-10 step
    emitter.instruction("mov ecx, 10");                                         // load the constant decimal divisor used to extract the tens and ones digits
    emitter.instruction("div ecx");                                             // split the remaining two digits into the tens digit in eax and the ones digit in edx
    emitter.instruction("add al, 48");                                          // convert the tens digit to its ASCII decimal character
    emitter.instruction("mov BYTE PTR [r8 + 2], al");                           // append the tens digit to the output buffer
    emitter.instruction("add dl, 48");                                          // convert the ones digit to its ASCII decimal character
    emitter.instruction("mov BYTE PTR [r8 + 3], dl");                           // append the ones digit to the output buffer
    emitter.instruction("add r8, 4");                                           // advance the live output cursor after appending the four decimal digits
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // publish the updated output cursor after the four-digit append
    emitter.instruction("ret");                                                 // return to the caller token branch after appending the four-digit field

    emitter.label("__rt_date_write_num_linux_x86_64");
    emitter.instruction("cmp eax, 10");                                         // check whether the decimal value fits in a single digit before choosing the emission path
    emitter.instruction("jl __rt_date_write_num_single_linux_x86_64");          // use the single-digit path when the value is strictly smaller than ten
    emitter.instruction("call __rt_date_write_2digit_linux_x86_64");            // reuse the zero-padded two-digit helper when the value naturally occupies two decimal digits
    emitter.instruction("ret");                                                 // return to the caller token branch after appending the two-digit decimal field
    emitter.label("__rt_date_write_num_single_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // reload the live output cursor before appending the single-digit decimal field
    emitter.instruction("add al, 48");                                          // convert the single decimal digit to its ASCII character representation
    emitter.instruction("mov BYTE PTR [r8 + 0], al");                           // append the single decimal digit to the output buffer
    emitter.instruction("add r8, 1");                                           // advance the live output cursor after appending one decimal digit
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // publish the updated output cursor after the single-digit append
    emitter.instruction("ret");                                                 // return to the caller token branch after appending the single-digit field

    emitter.label("__rt_date_write_int64_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // reload the live output cursor before appending the variable-width decimal integer field
    emitter.instruction("lea r9, [rbp - 96]");                                  // point at the local scratch buffer used to stage decimal digits in reverse order
    emitter.instruction("xor rcx, rcx");                                        // start the decimal scratch length at zero before extracting any digits
    emitter.instruction("cmp rax, 0");                                          // check whether the integer to append is exactly zero before entering the division loop
    emitter.instruction("jne __rt_date_write_int64_loop_linux_x86_64");         // skip the dedicated zero case when at least one non-zero digit must be extracted
    emitter.instruction("mov BYTE PTR [r9 + 0], 48");                           // stage the single ASCII digit '0' in the decimal scratch buffer for the zero value
    emitter.instruction("mov rcx, 1");                                          // record that the zero case staged exactly one decimal digit in the scratch buffer
    emitter.instruction("jmp __rt_date_write_int64_copy_linux_x86_64");         // skip the division loop once the dedicated zero case has staged its single digit

    emitter.label("__rt_date_write_int64_loop_linux_x86_64");
    emitter.instruction("xor edx, edx");                                        // clear the implicit high half of the dividend before the unsigned divide-by-10 step
    emitter.instruction("mov r10, 10");                                         // load the constant decimal divisor used to peel off one least-significant digit at a time
    emitter.instruction("div r10");                                             // divide the integer by ten so rax=quotient and rdx=remainder for decimal digit extraction
    emitter.instruction("add dl, 48");                                          // convert the extracted least-significant digit remainder to its ASCII decimal character
    emitter.instruction("mov BYTE PTR [r9 + rcx], dl");                         // stage the extracted decimal digit into the reverse-order scratch buffer
    emitter.instruction("add rcx, 1");                                          // advance the scratch-buffer length after staging one more extracted decimal digit
    emitter.instruction("test rax, rax");                                       // stop the extraction loop once no higher-order decimal digits remain
    emitter.instruction("jne __rt_date_write_int64_loop_linux_x86_64");         // continue extracting digits until the quotient reaches zero

    emitter.label("__rt_date_write_int64_copy_linux_x86_64");
    emitter.instruction("cmp rcx, 0");                                          // stop once every staged decimal digit has been copied back out in forward order
    emitter.instruction("je __rt_date_write_int64_done_linux_x86_64");          // finish the decimal integer append once the reverse-order scratch buffer is exhausted
    emitter.instruction("sub rcx, 1");                                          // step backward through the reverse-order scratch buffer to restore forward decimal order
    emitter.instruction("mov al, BYTE PTR [r9 + rcx]");                         // load the next forward-order decimal digit from the reverse-order scratch buffer
    emitter.instruction("mov BYTE PTR [r8 + 0], al");                           // append the next forward-order decimal digit to the output buffer
    emitter.instruction("add r8, 1");                                           // advance the live output cursor after appending one decimal digit
    emitter.instruction("jmp __rt_date_write_int64_copy_linux_x86_64");         // continue copying digits out until the scratch buffer has been fully drained

    emitter.label("__rt_date_write_int64_done_linux_x86_64");
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // publish the updated output cursor after appending the variable-width decimal integer
    emitter.instruction("ret");                                                 // return to the caller token branch after appending the full decimal integer field
}
