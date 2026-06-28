//! Purpose:
//! Emits the `@<timestamp>` epoch parser sub-routine consumed by the `__rt_strtotime` dispatcher.
//! Accepts `@`, an optional sign, decimal digits, and an optional fractional part (truncated).
//!
//! Called from:
//! - `crate::codegen::runtime::system::strtotime::mod::emit_strtotime()` via the dispatcher's
//!   first-byte `@` branch.
//!
//! Key details:
//! - Entry label `__rt_strtotime_epoch_entry` (ARM64) / `_linux_x86_64` (x86_64); the dispatcher
//!   frame is already set up, with the trimmed ptr at `[sp+48]` and trimmed len at `[sp+56]`.
//! - The value is a literal UNIX timestamp (UTC), so it is returned directly without `mktime`.
//! - All exits branch to the shared `__rt_strtotime_ret` / `__rt_strtotime_fail` epilogues.

use crate::codegen::{emit::Emitter, platform::Arch};

/// Dispatches to the architecture-specific `@<timestamp>` epoch parser.
pub(crate) fn emit_epoch(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_epoch_linux_x86_64(emitter);
        return;
    }

    emit_epoch_arm64(emitter);
}

/// Emits ARM64 assembly for the `@<timestamp>` epoch parser.
///
/// Entry label: `__rt_strtotime_epoch_entry`. Skips the leading `@`, reads an optional `+`/`-`
/// sign, accumulates decimal digits into a signed 64-bit value, and truncates at a `.` (any
/// fractional part is discarded, matching PHP). Requires at least one digit; otherwise fails.
/// The accumulated value is the result timestamp (returned via `__rt_strtotime_ret`).
fn emit_epoch_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime: @<timestamp> epoch sub-routine ---");
    emitter.label("__rt_strtotime_epoch_entry");

    emitter.instruction("ldr x1, [sp, #48]");                                   // reload trimmed input pointer
    emitter.instruction("ldr x2, [sp, #56]");                                   // reload trimmed input length
    emitter.instruction("cmp x2, #2");                                          // need at least '@' plus one char
    emitter.instruction("b.lt __rt_strtotime_fail");                            // "@" alone is invalid

    emitter.instruction("mov x3, #1");                                          // index = 1 (skip the '@')
    emitter.instruction("mov x0, #0");                                          // accumulator = 0
    emitter.instruction("mov x4, #1");                                          // sign = +1
    emitter.instruction("mov x5, #0");                                          // digit count = 0

    // -- optional leading sign --
    emitter.instruction("ldrb w9, [x1, x3]");                                   // load first char after '@'
    emitter.instruction("cmp w9, #45");                                         // '-' ?
    emitter.instruction("b.ne __rt_strtotime_epoch_check_plus");                // not minus → check plus
    emitter.instruction("mov x4, #-1");                                         // negative timestamp
    emitter.instruction("add x3, x3, #1");                                      // consume the '-'
    emitter.instruction("b __rt_strtotime_epoch_loop");                         // begin digit scan
    emitter.label("__rt_strtotime_epoch_check_plus");
    emitter.instruction("cmp w9, #43");                                         // '+' ?
    emitter.instruction("b.ne __rt_strtotime_epoch_loop");                      // not plus → begin digit scan
    emitter.instruction("add x3, x3, #1");                                      // consume the '+'

    // -- accumulate decimal digits --
    emitter.label("__rt_strtotime_epoch_loop");
    emitter.instruction("cmp x3, x2");                                          // reached end of input?
    emitter.instruction("b.ge __rt_strtotime_epoch_finish");                    // done scanning digits
    emitter.instruction("ldrb w9, [x1, x3]");                                   // load current char
    emitter.instruction("cmp w9, #46");                                         // '.' (fractional part) ?
    emitter.instruction("b.eq __rt_strtotime_epoch_finish");                    // truncate any fractional seconds
    emitter.instruction("sub w9, w9, #48");                                     // convert ASCII to digit value
    emitter.instruction("cmp w9, #9");                                          // is it a decimal digit?
    emitter.instruction("b.hi __rt_strtotime_fail");                            // non-digit → invalid epoch
    emitter.instruction("mov x10, #10");                                        // decimal base
    emitter.instruction("mul x0, x0, x10");                                     // shift accumulator left one decimal place
    emitter.instruction("add x0, x0, x9");                                      // add the new digit
    emitter.instruction("add x5, x5, #1");                                      // count the digit
    emitter.instruction("add x3, x3, #1");                                      // advance to the next char
    emitter.instruction("b __rt_strtotime_epoch_loop");                         // continue scanning

    emitter.label("__rt_strtotime_epoch_finish");
    emitter.instruction("cbz x5, __rt_strtotime_fail");                         // no digits parsed → invalid
    emitter.instruction("mul x0, x0, x4");                                      // apply the sign
    emitter.instruction("b __rt_strtotime_ret");                                // return the literal timestamp
}

/// Emits x86_64 (Linux) assembly for the `@<timestamp>` epoch parser.
///
/// Entry label: `__rt_strtotime_epoch_entry_linux_x86_64`. Mirrors the ARM64 logic using SysV
/// register/stack conventions: trimmed ptr at `[rsp+48]`, trimmed len at `[rsp+56]`.
fn emit_epoch_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime: @<timestamp> epoch sub-routine ---");
    emitter.label("__rt_strtotime_epoch_entry_linux_x86_64");

    emitter.instruction("mov rdi, QWORD PTR [rsp + 48]");                       // reload trimmed input pointer
    emitter.instruction("mov rsi, QWORD PTR [rsp + 56]");                       // reload trimmed input length
    emitter.instruction("cmp rsi, 2");                                          // need at least '@' plus one char
    emitter.instruction("jb __rt_strtotime_fail_linux_x86_64");                 // "@" alone is invalid

    emitter.instruction("mov rcx, 1");                                          // index = 1 (skip the '@')
    emitter.instruction("xor eax, eax");                                        // accumulator = 0
    emitter.instruction("mov r8, 1");                                           // sign = +1
    emitter.instruction("xor r9, r9");                                          // digit count = 0

    // -- optional leading sign --
    emitter.instruction("movzx edx, BYTE PTR [rdi + rcx]");                     // load first char after '@'
    emitter.instruction("cmp edx, 45");                                         // '-' ?
    emitter.instruction("jne __rt_strtotime_epoch_check_plus_linux_x86_64");    // not minus → check plus
    emitter.instruction("mov r8, -1");                                          // negative timestamp
    emitter.instruction("add rcx, 1");                                          // consume the '-'
    emitter.instruction("jmp __rt_strtotime_epoch_loop_linux_x86_64");          // begin digit scan
    emitter.label("__rt_strtotime_epoch_check_plus_linux_x86_64");
    emitter.instruction("cmp edx, 43");                                         // '+' ?
    emitter.instruction("jne __rt_strtotime_epoch_loop_linux_x86_64");          // not plus → begin digit scan
    emitter.instruction("add rcx, 1");                                          // consume the '+'

    // -- accumulate decimal digits --
    emitter.label("__rt_strtotime_epoch_loop_linux_x86_64");
    emitter.instruction("cmp rcx, rsi");                                        // reached end of input?
    emitter.instruction("jae __rt_strtotime_epoch_finish_linux_x86_64");        // done scanning digits
    emitter.instruction("movzx edx, BYTE PTR [rdi + rcx]");                     // load current char
    emitter.instruction("cmp edx, 46");                                         // '.' (fractional part) ?
    emitter.instruction("je __rt_strtotime_epoch_finish_linux_x86_64");         // truncate any fractional seconds
    emitter.instruction("sub edx, 48");                                         // convert ASCII to digit value
    emitter.instruction("cmp edx, 9");                                          // is it a decimal digit?
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // non-digit → invalid epoch
    emitter.instruction("imul rax, rax, 10");                                   // shift accumulator left one decimal place
    emitter.instruction("add rax, rdx");                                        // add the new digit
    emitter.instruction("add r9, 1");                                           // count the digit
    emitter.instruction("add rcx, 1");                                          // advance to the next char
    emitter.instruction("jmp __rt_strtotime_epoch_loop_linux_x86_64");          // continue scanning

    emitter.label("__rt_strtotime_epoch_finish_linux_x86_64");
    emitter.instruction("test r9, r9");                                         // any digits parsed?
    emitter.instruction("jz __rt_strtotime_fail_linux_x86_64");                 // no digits → invalid
    emitter.instruction("imul rax, r8");                                        // apply the sign
    emitter.instruction("jmp __rt_strtotime_ret_linux_x86_64");                 // return the literal timestamp
}
