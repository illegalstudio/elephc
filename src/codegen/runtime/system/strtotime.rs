use crate::codegen::emit::Emitter;

/// __rt_strtotime: parse a date/time string into a Unix timestamp.
/// Input:  x1=string ptr, x2=string len
/// Output: x0=Unix timestamp (or -1 on failure)
///
/// Supports formats:
///   "YYYY-MM-DD HH:MM:SS"  (19 chars)
///   "YYYY-MM-DD"           (10 chars)
///
/// Parses digits manually, builds struct tm, calls _mktime.
pub fn emit_strtotime(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strtotime ---");
    emitter.label_global("__rt_strtotime");

    // -- set up stack frame --
    // Stack: [sp+0..47] = struct tm, [sp+48..55] = string ptr, [sp+56..63] = string len
    emitter.instruction("sub sp, sp, #96");                                     // allocate 96 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // set new frame pointer

    // -- save inputs --
    emitter.instruction("str x1, [sp, #48]");                                   // save string ptr
    emitter.instruction("str x2, [sp, #56]");                                   // save string len

    // -- validate minimum length (10 for YYYY-MM-DD) --
    emitter.instruction("cmp x2, #10");                                         // need at least 10 chars
    emitter.instruction("b.lt __rt_strtotime_fail");                            // fail if too short

    // -- parse YYYY (4 digits at offset 0) --
    emitter.instruction("ldrb w9, [x1, #0]");                                   // load 1st year digit
    emitter.instruction("sub w9, w9, #48");                                     // convert from ASCII
    emitter.instruction("mov w10, #1000");                                      // multiplier for thousands
    emitter.instruction("mul w9, w9, w10");                                     // thousands place
    emitter.instruction("ldrb w10, [x1, #1]");                                  // load 2nd year digit
    emitter.instruction("sub w10, w10, #48");                                   // convert from ASCII
    emitter.instruction("mov w11, #100");                                       // multiplier for hundreds
    emitter.instruction("mul w10, w10, w11");                                   // hundreds place
    emitter.instruction("add w9, w9, w10");                                     // accumulate
    emitter.instruction("ldrb w10, [x1, #2]");                                  // load 3rd year digit
    emitter.instruction("sub w10, w10, #48");                                   // convert from ASCII
    emitter.instruction("mov w11, #10");                                        // multiplier for tens
    emitter.instruction("mul w10, w10, w11");                                   // tens place
    emitter.instruction("add w9, w9, w10");                                     // accumulate
    emitter.instruction("ldrb w10, [x1, #3]");                                  // load 4th year digit
    emitter.instruction("sub w10, w10, #48");                                   // convert from ASCII
    emitter.instruction("add w9, w9, w10");                                     // w9 = year (e.g. 2024)
    emitter.instruction("mov w10, #1900");                                      // year base for struct tm
    emitter.instruction("sub w9, w9, w10");                                     // tm_year = year - 1900
    emitter.instruction("str w9, [sp, #20]");                                   // store tm_year

    // -- parse MM (2 digits at offset 5) --
    emitter.instruction("ldrb w9, [x1, #5]");                                   // load 1st month digit
    emitter.instruction("sub w9, w9, #48");                                     // convert from ASCII
    emitter.instruction("mov w10, #10");                                        // multiplier
    emitter.instruction("mul w9, w9, w10");                                     // tens place
    emitter.instruction("ldrb w10, [x1, #6]");                                  // load 2nd month digit
    emitter.instruction("sub w10, w10, #48");                                   // convert from ASCII
    emitter.instruction("add w9, w9, w10");                                     // w9 = month (1-12)
    emitter.instruction("sub w9, w9, #1");                                      // tm_mon = month - 1 (0-based)
    emitter.instruction("str w9, [sp, #16]");                                   // store tm_mon

    // -- parse DD (2 digits at offset 8) --
    emitter.instruction("ldrb w9, [x1, #8]");                                   // load 1st day digit
    emitter.instruction("sub w9, w9, #48");                                     // convert from ASCII
    emitter.instruction("mov w10, #10");                                        // multiplier
    emitter.instruction("mul w9, w9, w10");                                     // tens place
    emitter.instruction("ldrb w10, [x1, #9]");                                  // load 2nd day digit
    emitter.instruction("sub w10, w10, #48");                                   // convert from ASCII
    emitter.instruction("add w9, w9, w10");                                     // w9 = day
    emitter.instruction("str w9, [sp, #12]");                                   // store tm_mday

    // -- check if time component exists (length >= 19 for "YYYY-MM-DD HH:MM:SS") --
    emitter.instruction("ldr x2, [sp, #56]");                                   // reload string length
    emitter.instruction("cmp x2, #19");                                         // check for full datetime
    emitter.instruction("b.lt __rt_strtotime_notime");                          // no time component

    // -- parse HH (2 digits at offset 11) --
    emitter.instruction("ldrb w9, [x1, #11]");                                  // load 1st hour digit
    emitter.instruction("sub w9, w9, #48");                                     // convert from ASCII
    emitter.instruction("mov w10, #10");                                        // multiplier
    emitter.instruction("mul w9, w9, w10");                                     // tens place
    emitter.instruction("ldrb w10, [x1, #12]");                                 // load 2nd hour digit
    emitter.instruction("sub w10, w10, #48");                                   // convert from ASCII
    emitter.instruction("add w9, w9, w10");                                     // w9 = hour
    emitter.instruction("str w9, [sp, #8]");                                    // store tm_hour

    // -- parse MM (2 digits at offset 14) --
    emitter.instruction("ldrb w9, [x1, #14]");                                  // load 1st minute digit
    emitter.instruction("sub w9, w9, #48");                                     // convert from ASCII
    emitter.instruction("mov w10, #10");                                        // multiplier
    emitter.instruction("mul w9, w9, w10");                                     // tens place
    emitter.instruction("ldrb w10, [x1, #15]");                                 // load 2nd minute digit
    emitter.instruction("sub w10, w10, #48");                                   // convert from ASCII
    emitter.instruction("add w9, w9, w10");                                     // w9 = minute
    emitter.instruction("str w9, [sp, #4]");                                    // store tm_min

    // -- parse SS (2 digits at offset 17) --
    emitter.instruction("ldrb w9, [x1, #17]");                                  // load 1st second digit
    emitter.instruction("sub w9, w9, #48");                                     // convert from ASCII
    emitter.instruction("mov w10, #10");                                        // multiplier
    emitter.instruction("mul w9, w9, w10");                                     // tens place
    emitter.instruction("ldrb w10, [x1, #18]");                                 // load 2nd second digit
    emitter.instruction("sub w10, w10, #48");                                   // convert from ASCII
    emitter.instruction("add w9, w9, w10");                                     // w9 = second
    emitter.instruction("str w9, [sp, #0]");                                    // store tm_sec
    emitter.instruction("b __rt_strtotime_mktime");                             // proceed to mktime

    // -- no time component, default to 00:00:00 --
    emitter.label("__rt_strtotime_notime");
    emitter.instruction("str wzr, [sp, #0]");                                   // tm_sec = 0
    emitter.instruction("str wzr, [sp, #4]");                                   // tm_min = 0
    emitter.instruction("str wzr, [sp, #8]");                                   // tm_hour = 0

    // -- fill remaining tm fields and call mktime --
    emitter.label("__rt_strtotime_mktime");
    emitter.instruction("str wzr, [sp, #24]");                                  // tm_wday = 0
    emitter.instruction("str wzr, [sp, #28]");                                  // tm_yday = 0
    emitter.instruction("mov w9, #-1");                                         // tm_isdst = -1
    emitter.instruction("str w9, [sp, #32]");                                   // store tm_isdst

    // -- call mktime --
    emitter.instruction("mov x0, sp");                                          // x0 = pointer to struct tm
    emitter.instruction("bl _mktime");                                          // mktime(&tm) → x0=timestamp
    emitter.instruction("b __rt_strtotime_ret");                                // return result

    // -- failure: return -1 --
    emitter.label("__rt_strtotime_fail");
    emitter.instruction("mov x0, #-1");                                         // return -1 on failure

    // -- tear down and return --
    emitter.label("__rt_strtotime_ret");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
