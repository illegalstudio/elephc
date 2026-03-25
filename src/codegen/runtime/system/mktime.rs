use crate::codegen::emit::Emitter;

/// __rt_mktime: create a Unix timestamp from date components.
/// Input:  x0=hour, x1=minute, x2=second, x3=month (1-12), x4=day, x5=year
/// Output: x0=Unix timestamp
///
/// Builds a struct tm on the stack and calls libc _mktime.
/// struct tm layout: tm_sec(+0), tm_min(+4), tm_hour(+8), tm_mday(+12),
///                   tm_mon(+16), tm_year(+20), tm_wday(+24), tm_yday(+28), tm_isdst(+32)
pub fn emit_mktime(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mktime ---");
    emitter.label("__rt_mktime");

    // -- set up stack frame --
    // Need 44 bytes for struct tm (rounded up to 48 for alignment) + 16 for frame
    emitter.instruction("sub sp, sp, #80");                                     // allocate 80 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set new frame pointer

    // -- build struct tm at sp+0 --
    emitter.instruction("str w2, [sp, #0]");                                    // tm_sec = second
    emitter.instruction("str w1, [sp, #4]");                                    // tm_min = minute
    emitter.instruction("str w0, [sp, #8]");                                    // tm_hour = hour
    emitter.instruction("str w4, [sp, #12]");                                   // tm_mday = day
    emitter.instruction("sub w3, w3, #1");                                      // convert month from 1-based to 0-based
    emitter.instruction("str w3, [sp, #16]");                                   // tm_mon = month - 1
    emitter.instruction("mov w9, #1900");                                       // load 1900 for year adjustment
    emitter.instruction("sub w5, w5, w9");                                      // tm_year = year - 1900
    emitter.instruction("str w5, [sp, #20]");                                   // store tm_year
    emitter.instruction("str wzr, [sp, #24]");                                  // tm_wday = 0 (ignored by mktime)
    emitter.instruction("str wzr, [sp, #28]");                                  // tm_yday = 0 (ignored by mktime)
    emitter.instruction("mov w9, #-1");                                         // tm_isdst = -1 (let mktime determine DST)
    emitter.instruction("str w9, [sp, #32]");                                   // store tm_isdst

    // -- call libc mktime --
    emitter.instruction("mov x0, sp");                                          // x0 = pointer to struct tm
    emitter.instruction("bl _mktime");                                          // mktime(&tm) → x0=time_t

    // -- tear down stack frame --
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
