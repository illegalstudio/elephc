//! Purpose:
//! Emits the `__rt_checkdate` runtime helper: validates a Gregorian month/day/year triple.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::system`.
//!
//! Key details:
//! - Pure integer logic (no libc): range-checks month/day/year, then compares the day against the
//!   month length, accounting for the leap-year rule. Returns PHP `true`/`false` (1/0). Leaf routine.

use crate::codegen::{emit::Emitter, platform::Arch};

/// Emits `__rt_checkdate`, validating a Gregorian date.
///
/// ## Input registers (System V ABI)
/// - `x0`/`rdi` = month (1-12), `x1`/`rsi` = day, `x2`/`rdx` = year (full Gregorian year)
///
/// ## Output
/// - `x0`/`rax` = 1 when the date is valid, 0 otherwise
///
/// ## Behavior
/// - Rejects month outside 1-12, year outside 1-32767, day below 1, or day above the month length.
/// - February has 29 days in leap years (divisible by 4 and not 100, or divisible by 400).
/// - Leaf routine: uses only caller-clobbered scratch registers and never calls libc.
pub fn emit_checkdate(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: checkdate ---");
    emitter.label_global("__rt_checkdate");
    match emitter.target.arch {
        Arch::AArch64 => {
    emitter.instruction("cmp x0, #1");                                          // month >= 1 ?
    emitter.instruction("b.lt __rt_checkdate_fail");                            // month < 1 → invalid
    emitter.instruction("cmp x0, #12");                                         // month <= 12 ?
    emitter.instruction("b.gt __rt_checkdate_fail");                            // month > 12 → invalid
    emitter.instruction("cmp x1, #1");                                          // day >= 1 ?
    emitter.instruction("b.lt __rt_checkdate_fail");                            // day < 1 → invalid
    emitter.instruction("cmp x2, #1");                                          // year >= 1 ?
    emitter.instruction("b.lt __rt_checkdate_fail");                            // year < 1 → invalid
    emitter.instruction("mov x9, #32767");                                      // PHP's maximum checkdate year
    emitter.instruction("cmp x2, x9");                                          // year <= 32767 ?
    emitter.instruction("b.gt __rt_checkdate_fail");                            // year > 32767 → invalid
    emitter.instruction("mov x3, #31");                                         // default days-in-month = 31
    emitter.instruction("cmp x0, #2");                                          // February?
    emitter.instruction("b.eq __rt_checkdate_feb");                             // handle February's leap-year length
    emitter.instruction("cmp x0, #4");                                          // April?
    emitter.instruction("b.eq __rt_checkdate_d30");                             // 30-day month
    emitter.instruction("cmp x0, #6");                                          // June?
    emitter.instruction("b.eq __rt_checkdate_d30");                             // 30-day month
    emitter.instruction("cmp x0, #9");                                          // September?
    emitter.instruction("b.eq __rt_checkdate_d30");                             // 30-day month
    emitter.instruction("cmp x0, #11");                                         // November?
    emitter.instruction("b.eq __rt_checkdate_d30");                             // 30-day month
    emitter.instruction("b __rt_checkdate_day");                                // all other months have 31 days
    emitter.label("__rt_checkdate_d30");
    emitter.instruction("mov x3, #30");                                         // 30-day month length
    emitter.instruction("b __rt_checkdate_day");                                // validate the day against 30
    emitter.label("__rt_checkdate_feb");
    emitter.instruction("mov x3, #28");                                         // February has 28 days outside leap years
    emitter.instruction("mov x9, #4");                                          // leap test: divisor 4
    emitter.instruction("udiv x10, x2, x9");                                    // year / 4
    emitter.instruction("msub x11, x10, x9, x2");                               // year % 4
    emitter.instruction("cbnz x11, __rt_checkdate_day");                        // not divisible by 4 → common year (28)
    emitter.instruction("mov x9, #100");                                        // leap test: divisor 100
    emitter.instruction("udiv x10, x2, x9");                                    // year / 100
    emitter.instruction("msub x11, x10, x9, x2");                               // year % 100
    emitter.instruction("cbnz x11, __rt_checkdate_leap");                       // divisible by 4 but not 100 → leap (29)
    emitter.instruction("mov x9, #400");                                        // leap test: divisor 400
    emitter.instruction("udiv x10, x2, x9");                                    // year / 400
    emitter.instruction("msub x11, x10, x9, x2");                               // year % 400
    emitter.instruction("cbnz x11, __rt_checkdate_day");                        // divisible by 100 but not 400 → common year (28)
    emitter.label("__rt_checkdate_leap");
    emitter.instruction("mov x3, #29");                                         // leap February has 29 days
    emitter.label("__rt_checkdate_day");
    emitter.instruction("cmp x1, x3");                                          // day <= days-in-month ?
    emitter.instruction("b.gt __rt_checkdate_fail");                            // day too large for the month → invalid
    emitter.instruction("mov x0, #1");                                          // PHP true: the date is valid
    emitter.instruction("ret");                                                 // return to caller
    emitter.label("__rt_checkdate_fail");
    emitter.instruction("mov x0, #0");                                          // PHP false: the date is invalid
    emitter.instruction("ret");                                                 // return to caller
        }
        Arch::X86_64 => {
    emitter.instruction("mov r8, rdx");                                         // preserve year (rdx is clobbered by div below)
    emitter.instruction("cmp rdi, 1");                                          // month >= 1 ?
    emitter.instruction("jl __rt_checkdate_fail_x86");                          // month < 1 → invalid
    emitter.instruction("cmp rdi, 12");                                         // month <= 12 ?
    emitter.instruction("jg __rt_checkdate_fail_x86");                          // month > 12 → invalid
    emitter.instruction("cmp rsi, 1");                                          // day >= 1 ?
    emitter.instruction("jl __rt_checkdate_fail_x86");                          // day < 1 → invalid
    emitter.instruction("cmp r8, 1");                                           // year >= 1 ?
    emitter.instruction("jl __rt_checkdate_fail_x86");                          // year < 1 → invalid
    emitter.instruction("cmp r8, 32767");                                       // year <= 32767 ?
    emitter.instruction("jg __rt_checkdate_fail_x86");                          // year > 32767 → invalid
    emitter.instruction("mov r9, 31");                                          // default days-in-month = 31
    emitter.instruction("cmp rdi, 2");                                          // February?
    emitter.instruction("je __rt_checkdate_feb_x86");                           // handle February's leap-year length
    emitter.instruction("cmp rdi, 4");                                          // April?
    emitter.instruction("je __rt_checkdate_d30_x86");                           // 30-day month
    emitter.instruction("cmp rdi, 6");                                          // June?
    emitter.instruction("je __rt_checkdate_d30_x86");                           // 30-day month
    emitter.instruction("cmp rdi, 9");                                          // September?
    emitter.instruction("je __rt_checkdate_d30_x86");                           // 30-day month
    emitter.instruction("cmp rdi, 11");                                         // November?
    emitter.instruction("je __rt_checkdate_d30_x86");                           // 30-day month
    emitter.instruction("jmp __rt_checkdate_day_x86");                          // all other months have 31 days
    emitter.label("__rt_checkdate_d30_x86");
    emitter.instruction("mov r9, 30");                                          // 30-day month length
    emitter.instruction("jmp __rt_checkdate_day_x86");                          // validate the day against 30
    emitter.label("__rt_checkdate_feb_x86");
    emitter.instruction("mov r9, 28");                                          // February has 28 days outside leap years
    emitter.instruction("mov rax, r8");                                         // leap test: dividend = year
    emitter.instruction("xor edx, edx");                                        // clear the high half before dividing
    emitter.instruction("mov rcx, 4");                                          // leap test: divisor 4
    emitter.instruction("div rcx");                                             // rdx = year % 4
    emitter.instruction("test rdx, rdx");                                       // divisible by 4?
    emitter.instruction("jnz __rt_checkdate_day_x86");                          // not divisible by 4 → common year (28)
    emitter.instruction("mov rax, r8");                                         // leap test: dividend = year
    emitter.instruction("xor edx, edx");                                        // clear the high half before dividing
    emitter.instruction("mov rcx, 100");                                        // leap test: divisor 100
    emitter.instruction("div rcx");                                             // rdx = year % 100
    emitter.instruction("test rdx, rdx");                                       // divisible by 100?
    emitter.instruction("jnz __rt_checkdate_leap_x86");                         // divisible by 4 but not 100 → leap (29)
    emitter.instruction("mov rax, r8");                                         // leap test: dividend = year
    emitter.instruction("xor edx, edx");                                        // clear the high half before dividing
    emitter.instruction("mov rcx, 400");                                        // leap test: divisor 400
    emitter.instruction("div rcx");                                             // rdx = year % 400
    emitter.instruction("test rdx, rdx");                                       // divisible by 400?
    emitter.instruction("jnz __rt_checkdate_day_x86");                          // divisible by 100 but not 400 → common year (28)
    emitter.label("__rt_checkdate_leap_x86");
    emitter.instruction("mov r9, 29");                                          // leap February has 29 days
    emitter.label("__rt_checkdate_day_x86");
    emitter.instruction("cmp rsi, r9");                                         // day <= days-in-month ?
    emitter.instruction("jg __rt_checkdate_fail_x86");                          // day too large for the month → invalid
    emitter.instruction("mov rax, 1");                                          // PHP true: the date is valid
    emitter.instruction("ret");                                                 // return to caller
    emitter.label("__rt_checkdate_fail_x86");
    emitter.instruction("mov rax, 0");                                          // PHP false: the date is invalid
    emitter.instruction("ret");                                                 // return to caller
        }
    }
}
