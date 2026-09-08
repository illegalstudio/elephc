//! Purpose:
//! Emits the ARM64 `__rt_date` and `__rt_gmdate` wrappers over the vendored
//! php-src timelib bridge.
//!
//! Called from:
//! - `crate::codegen_support::runtime::system::date::emit_date()`.
//!
//! Key details:
//! - Every supported ARM64 target uses timelib for all signed timestamps.
//! - The wrapper preserves Elephc's string result ABI in `x1`/`x2`.

use crate::codegen_support::emit::Emitter;

/// Emits the target ABI wrappers for procedural `date()` and `gmdate()`.
///
/// The caller supplies the timestamp in `x0`, format pointer/length in `x1`/`x2`,
/// and a timestamp-present flag in `x4`. The helper returns the bridge string
/// pointer in `x1` and its byte length in `x2`.
pub(super) fn emit_date_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: date / gmdate via php-src timelib ---");

    emitter.label_global("__rt_gmdate");
    emitter.instruction("mov x3, #1");                                          // select UTC formatting
    emitter.instruction("b __rt_date_entry");                                  // share the timelib wrapper
    emitter.label_global("__rt_date");
    emitter.instruction("mov x3, #0");                                         // select the active PHP timezone
    emitter.label_shared("__rt_date_entry");

    emitter.instruction("sub sp, sp, #80");                                    // reserve aligned locals and the frame record
    emitter.instruction("stp x29, x30, [sp, #64]");                            // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #64");                                   // establish the wrapper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                   // save the requested timestamp
    emitter.instruction("str x1, [sp, #8]");                                   // save the format pointer
    emitter.instruction("str x2, [sp, #16]");                                  // save the format byte length
    emitter.instruction("str x3, [sp, #24]");                                  // save the date-vs-gmdate selector
    emitter.instruction("cmp x4, #0");                                         // was a concrete timestamp supplied?
    emitter.instruction("b.ne __rt_date_have_time");                           // preserve explicit timestamps, including -1
    emitter.instruction("mov x0, #0");                                         // time(NULL)
    emitter.bl_c("time");
    emitter.instruction("str x0, [sp, #0]");                                   // retain the current timestamp

    emitter.label("__rt_date_have_time");
    emitter.instruction("bl __rt_tz_init_utc");                                // initialize the default PHP timezone once
    emitter.instruction("bl __rt_date_default_timezone_get");                  // x1/x2 = timezone pointer/length
    emitter.instruction("mov x4, x1");                                         // bridge arg 5 = timezone pointer
    emitter.instruction("mov x5, x2");                                         // bridge arg 6 = timezone byte length
    emitter.instruction("ldr x0, [sp, #0]");                                   // bridge arg 1 = signed timestamp
    emitter.instruction("mov x1, #0");                                         // bridge arg 2 = procedural microseconds
    emitter.instruction("ldr x2, [sp, #8]");                                   // bridge arg 3 = format pointer
    emitter.instruction("ldr x3, [sp, #16]");                                  // bridge arg 4 = format byte length
    emitter.instruction("ldr x6, [sp, #24]");                                  // reload the UTC selector
    emitter.instruction("eor x6, x6, #1");                                     // bridge arg 7: date=local, gmdate=UTC
    emitter.instruction("add x7, sp, #32");                                    // bridge arg 8 = writable explicit output-length slot
    emitter.instruction("str xzr, [sp, #32]");                                 // initialize the byte-count result
    emitter.bl_c("elephc_tz_format");                                         // x0 = raw date bytes, including embedded NUL
    emitter.instruction("ldr x2, [sp, #32]");                                  // return the bridge-provided byte length
    emitter.instruction("mov x1, x0");                                         // return the raw PHP string pointer
    emitter.instruction("ldp x29, x30, [sp, #64]");                            // restore the caller frame and return address
    emitter.instruction("add sp, sp, #80");                                    // release the wrapper frame
    emitter.instruction("ret");                                                // return the formatted string pair
}
