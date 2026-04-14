use crate::codegen::{emit::Emitter, platform::Arch};

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
    if emitter.target.arch == Arch::X86_64 {
        emit_strtotime_linux_x86_64(emitter);
        return;
    }

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
    emitter.bl_c("mktime");                                          // mktime(&tm) → x0=timestamp
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

fn emit_strtotime_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strtotime ---");
    emitter.label_global("__rt_strtotime");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before building the temporary libc struct tm
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved input string and scratch struct tm storage
    emitter.instruction("sub rsp, 80");                                         // reserve aligned stack space for the saved input pair and the leading struct tm fields

    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the input string pointer so every parse step can reload it without depending on caller-saved registers
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the input string length so the optional time-component check can reload it later
    emitter.instruction("cmp rsi, 10");                                         // require at least the YYYY-MM-DD prefix before attempting any parsing
    emitter.instruction("jb __rt_strtotime_fail_linux_x86_64");                 // reject inputs shorter than the minimum supported ISO date format

    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the date-string pointer before parsing the four-digit Gregorian year
    emitter.instruction("movzx eax, BYTE PTR [r8 + 0]");                        // load the first year digit from the date string
    emitter.instruction("sub eax, 48");                                         // convert the first year digit from ASCII to its numeric value
    emitter.instruction("imul eax, eax, 1000");                                 // place the first year digit into the thousands column
    emitter.instruction("movzx ecx, BYTE PTR [r8 + 1]");                        // load the second year digit from the date string
    emitter.instruction("sub ecx, 48");                                         // convert the second year digit from ASCII to its numeric value
    emitter.instruction("imul ecx, ecx, 100");                                  // place the second year digit into the hundreds column
    emitter.instruction("add eax, ecx");                                        // accumulate the hundreds contribution into the parsed year value
    emitter.instruction("movzx ecx, BYTE PTR [r8 + 2]");                        // load the third year digit from the date string
    emitter.instruction("sub ecx, 48");                                         // convert the third year digit from ASCII to its numeric value
    emitter.instruction("imul ecx, ecx, 10");                                   // place the third year digit into the tens column
    emitter.instruction("add eax, ecx");                                        // accumulate the tens contribution into the parsed year value
    emitter.instruction("movzx ecx, BYTE PTR [r8 + 3]");                        // load the fourth year digit from the date string
    emitter.instruction("sub ecx, 48");                                         // convert the fourth year digit from ASCII to its numeric value
    emitter.instruction("add eax, ecx");                                        // finish assembling the full Gregorian year from the four ASCII digits
    emitter.instruction("sub eax, 1900");                                       // convert the Gregorian year to libc's year-since-1900 struct tm encoding
    emitter.instruction("mov DWORD PTR [rsp + 20], eax");                       // tm_year = parsed year - 1900

    emitter.instruction("movzx eax, BYTE PTR [r8 + 5]");                        // load the first month digit from the YYYY-MM-DD input
    emitter.instruction("sub eax, 48");                                         // convert the first month digit from ASCII to its numeric value
    emitter.instruction("imul eax, eax, 10");                                   // place the first month digit into the tens column
    emitter.instruction("movzx ecx, BYTE PTR [r8 + 6]");                        // load the second month digit from the YYYY-MM-DD input
    emitter.instruction("sub ecx, 48");                                         // convert the second month digit from ASCII to its numeric value
    emitter.instruction("add eax, ecx");                                        // finish assembling the 1-based calendar month
    emitter.instruction("sub eax, 1");                                          // convert the calendar month from PHP's 1-12 range to libc's 0-11 tm_mon encoding
    emitter.instruction("mov DWORD PTR [rsp + 16], eax");                       // tm_mon = parsed month - 1

    emitter.instruction("movzx eax, BYTE PTR [r8 + 8]");                        // load the first day-of-month digit from the YYYY-MM-DD input
    emitter.instruction("sub eax, 48");                                         // convert the first day-of-month digit from ASCII to its numeric value
    emitter.instruction("imul eax, eax, 10");                                   // place the first day-of-month digit into the tens column
    emitter.instruction("movzx ecx, BYTE PTR [r8 + 9]");                        // load the second day-of-month digit from the YYYY-MM-DD input
    emitter.instruction("sub ecx, 48");                                         // convert the second day-of-month digit from ASCII to its numeric value
    emitter.instruction("add eax, ecx");                                        // finish assembling the day-of-month component
    emitter.instruction("mov DWORD PTR [rsp + 12], eax");                       // tm_mday = parsed day-of-month

    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the input string length before checking for an optional time-of-day suffix
    emitter.instruction("cmp r9, 19");                                          // the full YYYY-MM-DD HH:MM:SS form requires at least 19 bytes
    emitter.instruction("jb __rt_strtotime_notime_linux_x86_64");               // fall back to midnight when the optional time-of-day suffix is absent

    emitter.instruction("movzx eax, BYTE PTR [r8 + 11]");                       // load the first hour digit from the HH:MM:SS suffix
    emitter.instruction("sub eax, 48");                                         // convert the first hour digit from ASCII to its numeric value
    emitter.instruction("imul eax, eax, 10");                                   // place the first hour digit into the tens column
    emitter.instruction("movzx ecx, BYTE PTR [r8 + 12]");                       // load the second hour digit from the HH:MM:SS suffix
    emitter.instruction("sub ecx, 48");                                         // convert the second hour digit from ASCII to its numeric value
    emitter.instruction("add eax, ecx");                                        // finish assembling the hour component
    emitter.instruction("mov DWORD PTR [rsp + 8], eax");                        // tm_hour = parsed hour

    emitter.instruction("movzx eax, BYTE PTR [r8 + 14]");                       // load the first minute digit from the HH:MM:SS suffix
    emitter.instruction("sub eax, 48");                                         // convert the first minute digit from ASCII to its numeric value
    emitter.instruction("imul eax, eax, 10");                                   // place the first minute digit into the tens column
    emitter.instruction("movzx ecx, BYTE PTR [r8 + 15]");                       // load the second minute digit from the HH:MM:SS suffix
    emitter.instruction("sub ecx, 48");                                         // convert the second minute digit from ASCII to its numeric value
    emitter.instruction("add eax, ecx");                                        // finish assembling the minute component
    emitter.instruction("mov DWORD PTR [rsp + 4], eax");                        // tm_min = parsed minute

    emitter.instruction("movzx eax, BYTE PTR [r8 + 17]");                       // load the first second digit from the HH:MM:SS suffix
    emitter.instruction("sub eax, 48");                                         // convert the first second digit from ASCII to its numeric value
    emitter.instruction("imul eax, eax, 10");                                   // place the first second digit into the tens column
    emitter.instruction("movzx ecx, BYTE PTR [r8 + 18]");                       // load the second second digit from the HH:MM:SS suffix
    emitter.instruction("sub ecx, 48");                                         // convert the second second digit from ASCII to its numeric value
    emitter.instruction("add eax, ecx");                                        // finish assembling the second component
    emitter.instruction("mov DWORD PTR [rsp + 0], eax");                        // tm_sec = parsed second
    emitter.instruction("jmp __rt_strtotime_mktime_linux_x86_64");              // skip the midnight-default path after parsing the full time-of-day suffix

    emitter.label("__rt_strtotime_notime_linux_x86_64");
    emitter.instruction("mov DWORD PTR [rsp + 0], 0");                          // default tm_sec to zero when the supported input contains only a calendar date
    emitter.instruction("mov DWORD PTR [rsp + 4], 0");                          // default tm_min to zero when the supported input contains only a calendar date
    emitter.instruction("mov DWORD PTR [rsp + 8], 0");                          // default tm_hour to zero when the supported input contains only a calendar date

    emitter.label("__rt_strtotime_mktime_linux_x86_64");
    emitter.instruction("mov DWORD PTR [rsp + 24], 0");                         // tm_wday = 0 because libc mktime() ignores the caller-supplied weekday field
    emitter.instruction("mov DWORD PTR [rsp + 28], 0");                         // tm_yday = 0 because libc mktime() ignores the caller-supplied yearday field
    emitter.instruction("mov DWORD PTR [rsp + 32], -1");                        // tm_isdst = -1 so libc mktime() infers daylight-saving time automatically
    emitter.instruction("mov rdi, rsp");                                        // pass the temporary struct tm as the first SysV integer argument to libc mktime()
    emitter.instruction("call mktime");                                         // convert the parsed calendar/time components into a Unix timestamp through libc
    emitter.instruction("jmp __rt_strtotime_ret_linux_x86_64");                 // return the parsed Unix timestamp through the standard x86_64 integer result register

    emitter.label("__rt_strtotime_fail_linux_x86_64");
    emitter.instruction("mov rax, -1");                                         // return -1 when the input string is shorter than the supported ISO date formats

    emitter.label("__rt_strtotime_ret_linux_x86_64");
    emitter.instruction("add rsp, 80");                                         // release the saved input pair and temporary struct tm storage before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the Unix timestamp
    emitter.instruction("ret");                                                 // return either the parsed Unix timestamp or -1 through the standard x86_64 integer result register
}
