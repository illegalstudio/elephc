//! Purpose:
//! Emits the `__rt_hrtime` runtime helper: reads the monotonic clock via libc `clock_gettime` and
//! returns either the total nanoseconds (when the as-number flag is set) or a `[seconds, nanoseconds]`
//! array, boxed into a Mixed cell.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::system`.
//!
//! Key details:
//! - Uses `CLOCK_MONOTONIC`, whose id differs by OS (macOS = 6, Linux = 1); the constant is emitted
//!   per `emitter.target.platform`. The result is fully boxed here (Mixed int or Mixed assoc array),
//!   so the builtin emitter only forwards the value.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::{Arch, Platform};

/// Emits `__rt_hrtime`, returning the monotonic clock as nanoseconds or a `[sec, nsec]` array.
///
/// ## Input registers (System V ABI)
/// - `x0`/`rax` = as-number flag (nonzero → return int nanoseconds; zero → return the array)
///
/// ## Output
/// - `x0`/`rax` = boxed Mixed cell: an int (total nanoseconds) or a 2-element `[sec, nsec]` array
///
/// ## Behavior
/// - Calls libc `clock_gettime(CLOCK_MONOTONIC, &timespec)`; the clock id is OS-specific.
pub fn emit_hrtime(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hrtime_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hrtime ---");
    emitter.label_global("__rt_hrtime");

    let clock_id = if emitter.target.platform == Platform::MacOS { 6 } else { 1 };

    // -- frame: [sp]=tv_sec, [sp+8]=tv_nsec, [sp+16]=flag-then-hash --
    emitter.instruction("sub sp, sp, #48");                                     // allocate frame: timespec + scratch + saved regs
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set the frame pointer
    emitter.instruction("str x0, [sp, #16]");                                   // save the as-number flag across the libc/runtime calls

    // -- clock_gettime(CLOCK_MONOTONIC, &timespec) --
    emitter.instruction(&format!("mov x0, #{clock_id}"));                       // CLOCK_MONOTONIC clock id (macOS=6, Linux=1)
    emitter.instruction("add x1, sp, #0");                                      // x1 = &timespec
    emitter.bl_c("clock_gettime");                                              // fill [sp]=tv_sec, [sp+8]=tv_nsec

    emitter.instruction("ldr x6, [sp, #0]");                                    // x6 = tv_sec
    emitter.instruction("ldr x7, [sp, #8]");                                    // x7 = tv_nsec
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the as-number flag
    emitter.instruction("cbz x0, __rt_hrtime_array");                           // flag clear → return the [sec, nsec] array

    // -- as-number: nanoseconds = tv_sec * 1_000_000_000 + tv_nsec --
    emitter.instruction("movz x8, #0xCA00");                                    // low 16 bits of 1_000_000_000
    emitter.instruction("movk x8, #0x3B9A, lsl #16");                           // x8 = 1_000_000_000 (nanoseconds per second)
    emitter.instruction("mul x6, x6, x8");                                      // tv_sec * 1e9
    emitter.instruction("add x6, x6, x7");                                      // + tv_nsec = total nanoseconds
    emitter.instruction("mov x1, x6");                                          // x1 = nanoseconds (low payload word)
    emitter.instruction("mov x2, #0");                                          // x2 = high payload word (unused)
    emitter.instruction("mov x0, #0");                                          // x0 = runtime tag 0 (int)
    emitter.instruction("bl __rt_mixed_from_value");                            // → x0 = boxed mixed int
    emitter.instruction("b __rt_hrtime_done");                                  // return the boxed nanosecond count

    // -- array: [0 => tv_sec, 1 => tv_nsec] --
    emitter.label("__rt_hrtime_array");
    emitter.instruction("mov x0, #2");                                          // initial capacity (2 entries)
    emitter.instruction("mov x1, #7");                                          // value type = mixed
    emitter.instruction("bl __rt_hash_new");                                    // → x0 = new hash table
    emitter.instruction("str x0, [sp, #16]");                                   // save the hash table pointer
    emitter.instruction("ldr x3, [sp, #0]");                                    // value_lo = tv_sec
    emitter.instruction("mov x4, #0");                                          // value_hi = 0
    emitter.instruction("mov x5, #0");                                          // value tag = int
    emitter.instruction("mov x1, #0");                                          // integer key 0
    emitter.instruction("mov x2, #-1");                                         // key_hi = -1 marks an integer key
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the hash table pointer
    emitter.instruction("bl __rt_hash_set");                                    // insert 0 → seconds
    emitter.instruction("str x0, [sp, #16]");                                   // save the (possibly reallocated) hash table
    emitter.instruction("ldr x3, [sp, #8]");                                    // value_lo = tv_nsec
    emitter.instruction("mov x4, #0");                                          // value_hi = 0
    emitter.instruction("mov x5, #0");                                          // value tag = int
    emitter.instruction("mov x1, #1");                                          // integer key 1
    emitter.instruction("mov x2, #-1");                                         // key_hi = -1 marks an integer key
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the hash table pointer
    emitter.instruction("bl __rt_hash_set");                                    // insert 1 → nanoseconds
    emitter.instruction("str x0, [sp, #16]");                                   // save the (possibly reallocated) hash table
    emitter.instruction("ldr x1, [sp, #16]");                                   // x1 = hash pointer (low payload word)
    emitter.instruction("mov x2, #0");                                          // x2 = high payload word (unused)
    emitter.instruction("mov x0, #5");                                          // x0 = runtime tag 5 (assoc array)
    emitter.instruction("bl __rt_mixed_from_value");                            // → x0 = boxed mixed assoc array

    emitter.label("__rt_hrtime_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate the frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the x86_64 Linux variant of `__rt_hrtime` (CLOCK_MONOTONIC = 1).
fn emit_hrtime_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hrtime ---");
    emitter.label_global("__rt_hrtime");

    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the frame pointer
    emitter.instruction("sub rsp, 48");                                         // reserve timespec + scratch (16-aligned)
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the as-number flag

    // -- clock_gettime(CLOCK_MONOTONIC=1, &timespec) — [rbp-32]=tv_sec, [rbp-24]=tv_nsec --
    emitter.instruction("mov rdi, 1");                                          // CLOCK_MONOTONIC clock id (Linux)
    emitter.instruction("lea rsi, [rbp - 32]");                                 // rsi = &timespec
    emitter.bl_c("clock_gettime");                                              // fill [rbp-32]=tv_sec, [rbp-24]=tv_nsec

    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the as-number flag
    emitter.instruction("test rax, rax");                                       // flag set?
    emitter.instruction("jz __rt_hrtime_array_x86");                            // flag clear → return the [sec, nsec] array

    // -- as-number: nanoseconds = tv_sec * 1_000_000_000 + tv_nsec --
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // rax = tv_sec
    emitter.instruction("imul rax, rax, 1000000000");                           // tv_sec * 1e9
    emitter.instruction("add rax, QWORD PTR [rbp - 24]");                       // + tv_nsec = total nanoseconds
    emitter.instruction("mov rdi, rax");                                        // rdi = nanoseconds (low payload word)
    emitter.instruction("mov rsi, 0");                                          // rsi = high payload word (unused)
    emitter.instruction("mov rax, 0");                                          // rax = runtime tag 0 (int)
    emitter.instruction("call __rt_mixed_from_value");                          // → rax = boxed mixed int
    emitter.instruction("jmp __rt_hrtime_done_x86");                            // return the boxed nanosecond count

    // -- array: [0 => tv_sec, 1 => tv_nsec] --
    emitter.label("__rt_hrtime_array_x86");
    emitter.instruction("mov rdi, 2");                                          // initial capacity (2 entries)
    emitter.instruction("mov rsi, 7");                                          // value type = mixed
    emitter.instruction("call __rt_hash_new");                                  // → rax = new hash table
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the hash table pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // value_lo = tv_sec
    emitter.instruction("mov r8, 0");                                           // value_hi = 0
    emitter.instruction("mov r9, 0");                                           // value tag = int
    emitter.instruction("mov rsi, 0");                                          // integer key 0
    emitter.instruction("mov rdx, -1");                                         // key_hi = -1 marks an integer key
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the hash table pointer
    emitter.instruction("call __rt_hash_set");                                  // insert 0 → seconds
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the (possibly reallocated) hash table
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // value_lo = tv_nsec
    emitter.instruction("mov r8, 0");                                           // value_hi = 0
    emitter.instruction("mov r9, 0");                                           // value tag = int
    emitter.instruction("mov rsi, 1");                                          // integer key 1
    emitter.instruction("mov rdx, -1");                                         // key_hi = -1 marks an integer key
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the hash table pointer
    emitter.instruction("call __rt_hash_set");                                  // insert 1 → nanoseconds
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the (possibly reallocated) hash table
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // rdi = hash pointer (low payload word)
    emitter.instruction("mov rsi, 0");                                          // rsi = high payload word (unused)
    emitter.instruction("mov rax, 5");                                          // rax = runtime tag 5 (assoc array)
    emitter.instruction("call __rt_mixed_from_value");                          // → rax = boxed mixed assoc array

    emitter.label("__rt_hrtime_done_x86");
    emitter.instruction("add rsp, 48");                                         // deallocate the frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}
