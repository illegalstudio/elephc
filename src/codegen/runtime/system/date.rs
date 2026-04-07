use crate::codegen::emit::Emitter;

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
    emitter.adrp("x9", "_concat_off");                           // load page of concat offset
    emitter.add_lo12("x9", "x9", "_concat_off");                     // resolve concat offset address
    emitter.instruction("ldr x10, [x9]");                                       // load current concat offset
    emitter.adrp("x11", "_concat_buf");                          // load page of concat buffer
    emitter.add_lo12("x11", "x11", "_concat_buf");                   // resolve concat buffer address
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
    emitter.adrp("x10", "_concat_off");                          // load page of concat offset
    emitter.add_lo12("x10", "x10", "_concat_off");                   // resolve concat offset address
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
