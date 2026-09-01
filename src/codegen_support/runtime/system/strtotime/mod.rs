//! Purpose:
//! Emits the target-aware `__rt_strtotime` wrapper over the on-demand timelib
//! bridge used by php-src's free-form date parser.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` through
//!   `crate::codegen_support::runtime::system`.
//! - `crate::codegen_support::runtime::data::fixed` for legacy lookup data kept
//!   ABI-stable while older cached runtime objects are invalidated.
//!
//! Key details:
//! - The runtime returns the timestamp plus a separate success flag so php-src's
//!   valid `i64::MIN` timestamp cannot collide with parse failure.
//! - Input and timezone strings use explicit pointer/length pairs, so no NUL
//!   termination assumption leaks across the Rust bridge boundary.

mod data;

use crate::codegen_support::{emit::Emitter, platform::Arch};

pub(crate) use data::emit_strtotime_data;

/// Emits `__rt_strtotime` for the active supported architecture.
///
/// Runtime cache variants that cannot reference date parsing receive a local
/// failure stub, avoiding an optional bridge reference in unrelated binaries.
pub(crate) fn emit_strtotime(emitter: &mut Emitter, timelib_enabled: bool) {
    if !timelib_enabled {
        emit_strtotime_unavailable(emitter);
    } else if emitter.target.arch == Arch::X86_64 {
        emit_timelib_strtotime_x86_64(emitter);
    } else {
        emit_timelib_strtotime_arm64(emitter);
    }
}

/// Emits the no-timelib runtime-cache variant's deterministic failure stub.
fn emit_strtotime_unavailable(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strtotime unavailable in this cache variant ---");
    emitter.label_global("__rt_strtotime");
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rax, -9223372036854775808");                   // return the established parse-failure sentinel
        emitter.instruction("xor edx, edx");                                    // success flag = 0 for the unavailable bridge
        emitter.instruction("ret");                                             // return to the caller
    } else {
        emitter.instruction("mov x0, #1");                                      // materialize the sign-bit seed
        emitter.instruction("lsl x0, x0, #63");                                 // return the established parse-failure sentinel
        emitter.instruction("mov x1, #0");                                      // success flag = 0 for the unavailable bridge
        emitter.instruction("ret");                                             // return to the caller
    }
}

/// Emits the ARM64 `__rt_strtotime` wrapper over the timelib bridge.
fn emit_timelib_strtotime_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strtotime via php-src timelib ---");
    emitter.label_global("__rt_strtotime");
    emitter.instruction("sub sp, sp, #64");                                     // reserve aligned storage for inputs and the frame record
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the wrapper frame pointer
    emitter.instruction("stp x1, x2, [sp, #0]");                                // save input pointer and byte length
    emitter.instruction("stp x0, x3, [sp, #16]");                               // save base timestamp and its presence flag
    emitter.instruction("bl __rt_tz_init_utc");                                 // initialize PHP's default timezone state once
    emitter.instruction("bl __rt_date_default_timezone_get");                   // x1/x2 = current timezone pointer/length
    emitter.instruction("mov x4, x1");                                          // bridge arg 5 = timezone pointer
    emitter.instruction("mov x5, x2");                                          // bridge arg 6 = timezone byte length
    emitter.instruction("ldp x0, x1, [sp, #0]");                                // bridge args 1/2 = input pointer/length
    emitter.instruction("ldp x2, x3, [sp, #16]");                               // bridge args 3/4 = base timestamp/presence
    emitter.instruction("add x6, sp, #32");                                     // bridge arg 7 = writable success-flag slot
    emitter.instruction("str xzr, [sp, #32]");                                  // default the bridge success flag to false
    emitter.bl_c("elephc_tz_strtotime");
    emitter.instruction("ldr x1, [sp, #32]");                                   // return the bridge success flag beside the timestamp
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #64");                                     // release wrapper storage
    emitter.instruction("ret");                                                 // return timestamp or i64::MIN failure sentinel
}

/// Emits the Linux x86_64 `__rt_strtotime` wrapper over the timelib bridge.
fn emit_timelib_strtotime_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strtotime via php-src timelib ---");
    emitter.label_global("__rt_strtotime");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the wrapper frame
    emitter.instruction("sub rsp, 48");                                         // reserve aligned inputs, success storage, and the seventh bridge argument
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save input pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save input byte length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save base timestamp
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save base-presence flag
    emitter.instruction("call __rt_tz_init_utc");                               // initialize PHP's default timezone state once
    emitter.instruction("call __rt_date_default_timezone_get");                 // rax/rdx = current timezone pointer/length
    emitter.instruction("mov r8, rax");                                         // bridge arg 5 = timezone pointer
    emitter.instruction("mov r9, rdx");                                         // bridge arg 6 = timezone byte length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // bridge arg 1 = input pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // bridge arg 2 = input byte length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // bridge arg 3 = base timestamp
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // bridge arg 4 = base-presence flag
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // default the bridge success flag to false
    emitter.instruction("lea rax, [rbp - 40]");                                 // address of the writable success-flag slot
    emitter.instruction("mov QWORD PTR [rsp], rax");                            // bridge arg 7 = success-flag pointer on the SysV stack
    emitter.bl_c("elephc_tz_strtotime");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // return the bridge success flag beside the timestamp
    emitter.instruction("leave");                                               // restore the caller frame and stack pointer
    emitter.instruction("ret");                                                 // return timestamp or i64::MIN failure sentinel
}
