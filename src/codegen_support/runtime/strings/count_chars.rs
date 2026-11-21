//! Purpose:
//! Emits the `__rt_count_chars` runtime helper assembly for PHP's `count_chars`: tallies every
//! byte value of the subject and materializes the shape the requested `$mode` selects.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - The 256-entry tally lives on the helper's own frame, so the byte counting pass never
//!   touches shared runtime state.
//! - Modes 0, 1, and 2 build a hash with integer keys and integer values: mode 0 emits every
//!   byte value, mode 1 only the used ones, and mode 2 only the unused ones. Insertion runs
//!   from byte 0 upwards, which is the key order php-src produces.
//! - Modes 3 and 4 render the used / unused byte values as a string. The result is reserved
//!   through `__rt_concat_reserve` (never more than 256 bytes, but the bound is enforced the
//!   same way as for every other producer) and then copied into owned heap storage by
//!   `__rt_str_persist`, which is what the `Fresh` ownership contract on
//!   `RuntimeFnId::CountChars` promises for both result shapes. The reservation itself is
//!   then released through `__rt_heap_free_safe`, which is a no-op for the scratch-backed
//!   case and prevents a leak when the shared scratch was already nearly full.
//! - A mode outside `0..=4` never reaches this helper: the EIR lowering raises php-src's
//!   catchable `ValueError` before the call.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_count_chars` runtime helper.
///
/// ABI (AArch64):
///   Input:  `x1` = subject pointer, `x2` = subject length, `x3` = mode (0..=4).
///   Output: `x0` = tally hash pointer for modes 0-2; `x1`/`x2` = byte-list string pair for
///           modes 3-4.
///
/// ABI (x86_64 System V):
///   Input:  `rax` = subject pointer, `rdx` = subject length, `rdi` = mode (0..=4).
///   Output: `rax` = tally hash pointer for modes 0-2; `rax`/`rdx` = string pair for modes 3-4.
///
/// Clobbers every caller-saved register: the result paths reach `__rt_hash_new`,
/// `__rt_hash_set`, `__rt_concat_reserve`, `__rt_concat_publish`, and `__rt_str_persist`.
pub fn emit_count_chars(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_count_chars_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: count_chars ---");
    emitter.label_global("__rt_count_chars");

    // Frame layout (2128 bytes). The saved register pair sits at offset 0 because the
    // 2 KiB tally would push it far outside `stp`'s scaled immediate range.
    //   [sp, #0]   = saved x29/x30
    //   [sp, #16]  = subject pointer
    //   [sp, #24]  = subject length
    //   [sp, #32]  = requested mode
    //   [sp, #40]  = running result (hash pointer or reserved string start)
    //   [sp, #48]  = result byte count for the string modes
    //   [sp, #56]  = byte-value loop index
    //   [sp, #80]  = 256-entry byte tally
    emitter.instruction("sub sp, sp, #2128");                                   // allocate the tally frame
    emitter.instruction("stp x29, x30, [sp]");                                  // save the frame pointer and return address across the result helper calls
    emitter.instruction("mov x29, sp");                                         // establish the count_chars helper frame pointer
    emitter.instruction("str x1, [sp, #16]");                                   // save the subject pointer for the counting pass
    emitter.instruction("str x2, [sp, #24]");                                   // save the subject length for the counting pass
    emitter.instruction("str x3, [sp, #32]");                                   // save the requested result mode

    // -- clear the 256-entry byte tally --
    emitter.instruction("add x10, sp, #80");                                    // x10 = tally base, recomputed after every helper call
    emitter.instruction("mov x9, #0");                                          // start clearing at byte value zero
    emitter.label("__rt_count_chars_zero");
    emitter.instruction("str xzr, [x10, x9, lsl #3]");                          // clear one byte-value tally
    emitter.instruction("add x9, x9, #1");                                      // advance to the next byte value
    emitter.instruction("cmp x9, #256");                                        // has the whole tally been cleared?
    emitter.instruction("b.lo __rt_count_chars_zero");                          // keep clearing until every byte value is zeroed

    // -- tally every subject byte --
    emitter.instruction("mov x9, #0");                                          // start at the first subject byte
    emitter.label("__rt_count_chars_count");
    emitter.instruction("cmp x9, x2");                                          // has the whole subject been counted?
    emitter.instruction("b.hs __rt_count_chars_count_done");                    // the tally is complete
    emitter.instruction("ldrb w11, [x1, x9]");                                  // load the next subject byte
    emitter.instruction("ldr x12, [x10, x11, lsl #3]");                         // load that byte value's running tally
    emitter.instruction("add x12, x12, #1");                                    // count one more occurrence
    emitter.instruction("str x12, [x10, x11, lsl #3]");                         // publish the updated tally
    emitter.instruction("add x9, x9, #1");                                      // advance to the next subject byte
    emitter.instruction("b __rt_count_chars_count");                            // keep counting subject bytes
    emitter.label("__rt_count_chars_count_done");

    emitter.instruction("ldr x3, [sp, #32]");                                   // reload the requested result mode
    emitter.instruction("cmp x3, #3");                                          // do modes 3 and 4 want the byte-list string?
    emitter.instruction("b.ge __rt_count_chars_string");                        // render the byte list instead of a tally

    // -- modes 0, 1, and 2 build the integer-keyed tally hash --
    emitter.instruction("mov x0, #16");                                         // seed the tally hash with a small capacity
    emitter.instruction("mov x1, xzr");                                         // value_type 0 = integer values
    emitter.instruction("bl __rt_hash_new");                                    // allocate the tally hash
    emitter.instruction("str x0, [sp, #40]");                                   // publish the tally hash pointer
    emitter.instruction("str xzr, [sp, #56]");                                  // start emitting at byte value zero

    emitter.label("__rt_count_chars_array");
    emitter.instruction("ldr x9, [sp, #56]");                                   // reload the byte-value loop index
    emitter.instruction("cmp x9, #256");                                        // have all byte values been considered?
    emitter.instruction("b.hs __rt_count_chars_array_done");                    // the tally hash is complete
    emitter.instruction("add x10, sp, #80");                                    // restore the tally base after the previous helper call
    emitter.instruction("ldr x12, [x10, x9, lsl #3]");                          // load this byte value's tally
    emitter.instruction("ldr x3, [sp, #32]");                                   // reload the requested result mode
    emitter.instruction("cbz x3, __rt_count_chars_array_emit");                 // mode 0 emits every byte value
    emitter.instruction("cmp x3, #1");                                          // is the caller asking for the used byte values only?
    emitter.instruction("b.ne __rt_count_chars_array_unused");                  // mode 2 keeps the unused byte values instead
    emitter.instruction("cbz x12, __rt_count_chars_array_next");                // mode 1 skips byte values the subject never uses
    emitter.instruction("b __rt_count_chars_array_emit");                       // a used byte value is emitted with its tally
    emitter.label("__rt_count_chars_array_unused");
    emitter.instruction("cbnz x12, __rt_count_chars_array_next");               // mode 2 skips byte values the subject does use

    emitter.label("__rt_count_chars_array_emit");
    emitter.instruction("ldr x0, [sp, #40]");                                   // reload the tally hash pointer
    emitter.instruction("mov x1, x9");                                          // key_lo = the byte value
    emitter.instruction("mov x2, #-1");                                         // key_hi = -1 marks an integer key
    emitter.instruction("mov x3, x12");                                         // value_lo = the occurrence tally
    emitter.instruction("mov x4, xzr");                                         // integer tallies carry no high word
    emitter.instruction("mov x5, xzr");                                         // runtime tag 0 marks the tally as an int
    emitter.instruction("bl __rt_hash_set");                                    // insert the byte value's tally
    emitter.instruction("str x0, [sp, #40]");                                   // republish the hash pointer after possible growth

    emitter.label("__rt_count_chars_array_next");
    emitter.instruction("ldr x9, [sp, #56]");                                   // reload the byte-value loop index
    emitter.instruction("add x9, x9, #1");                                      // advance to the next byte value
    emitter.instruction("str x9, [sp, #56]");                                   // publish the advanced loop index
    emitter.instruction("b __rt_count_chars_array");                            // consider the next byte value

    emitter.label("__rt_count_chars_array_done");
    emitter.instruction("ldr x0, [sp, #40]");                                   // return the finished tally hash
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore the frame pointer and return address
    emitter.instruction("add sp, sp, #2128");                                   // release the tally frame
    emitter.instruction("ret");                                                 // return the tally as a PHP array

    // -- modes 3 and 4 render the selected byte values as a string --
    emitter.label("__rt_count_chars_string");
    emitter.instruction("add x10, sp, #80");                                    // x10 = tally base for the selection pass
    emitter.instruction("mov x9, #0");                                          // start at byte value zero
    emitter.instruction("mov x13, #0");                                         // running count of selected byte values

    emitter.label("__rt_count_chars_select");
    emitter.instruction("cmp x9, #256");                                        // have all byte values been considered?
    emitter.instruction("b.hs __rt_count_chars_select_done");                   // the exact result size is known
    emitter.instruction("ldr x12, [x10, x9, lsl #3]");                          // load this byte value's tally
    emitter.instruction("cmp x3, #3");                                          // is the caller asking for the used byte values?
    emitter.instruction("b.ne __rt_count_chars_select_unused");                 // mode 4 selects the unused byte values instead
    emitter.instruction("cbz x12, __rt_count_chars_select_next");               // mode 3 skips byte values the subject never uses
    emitter.instruction("b __rt_count_chars_select_hit");                       // a used byte value joins the result
    emitter.label("__rt_count_chars_select_unused");
    emitter.instruction("cbnz x12, __rt_count_chars_select_next");              // mode 4 skips byte values the subject does use
    emitter.label("__rt_count_chars_select_hit");
    emitter.instruction("add x13, x13, #1");                                    // reserve one more result byte
    emitter.label("__rt_count_chars_select_next");
    emitter.instruction("add x9, x9, #1");                                      // advance to the next byte value
    emitter.instruction("b __rt_count_chars_select");                           // keep sizing the result

    emitter.label("__rt_count_chars_select_done");
    emitter.instruction("str x13, [sp, #48]");                                  // save the exact result byte count
    emitter.instruction("mov x0, x13");                                         // request exactly that many bytes
    emitter.instruction("bl __rt_concat_reserve");                              // reserve scratch or heap storage for the byte list
    emitter.instruction("str x0, [sp, #40]");                                   // save the reserved result start
    emitter.instruction("mov x14, x0");                                         // destination cursor
    emitter.instruction("add x10, sp, #80");                                    // restore the tally base after the reservation call
    emitter.instruction("ldr x3, [sp, #32]");                                   // reload the requested result mode
    emitter.instruction("mov x9, #0");                                          // start at byte value zero

    emitter.label("__rt_count_chars_fill");
    emitter.instruction("cmp x9, #256");                                        // have all byte values been considered?
    emitter.instruction("b.hs __rt_count_chars_fill_done");                     // the byte list is complete
    emitter.instruction("ldr x12, [x10, x9, lsl #3]");                          // load this byte value's tally
    emitter.instruction("cmp x3, #3");                                          // is the caller asking for the used byte values?
    emitter.instruction("b.ne __rt_count_chars_fill_unused");                   // mode 4 writes the unused byte values instead
    emitter.instruction("cbz x12, __rt_count_chars_fill_next");                 // mode 3 skips byte values the subject never uses
    emitter.instruction("b __rt_count_chars_fill_hit");                         // a used byte value joins the result
    emitter.label("__rt_count_chars_fill_unused");
    emitter.instruction("cbnz x12, __rt_count_chars_fill_next");                // mode 4 skips byte values the subject does use
    emitter.label("__rt_count_chars_fill_hit");
    emitter.instruction("strb w9, [x14], #1");                                  // append the selected byte value to the result
    emitter.label("__rt_count_chars_fill_next");
    emitter.instruction("add x9, x9, #1");                                      // advance to the next byte value
    emitter.instruction("b __rt_count_chars_fill");                             // keep filling the byte list

    emitter.label("__rt_count_chars_fill_done");
    emitter.instruction("ldr x1, [sp, #40]");                                   // the byte list starts at the reserved pointer
    emitter.instruction("ldr x2, [sp, #48]");                                   // the byte list is exactly as long as it was sized
    elephc_memoir::strings::concat_scratch::emit_finish_owned(emitter, 40, 56);
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore the frame pointer and return address
    emitter.instruction("add sp, sp, #2128");                                   // release the tally frame
    emitter.instruction("ret");                                                 // return the byte list as a PHP string pair
}

/// Emits `__rt_count_chars` for x86_64 Linux using the System V ABI.
///
/// The 256-entry tally is addressed as `[rbp + index*8 - 2128]`, so no register has to
/// survive the result helper calls to keep it reachable. The saved-value slots start at
/// `[rbp-32]` to stay clear of the `[rbp-8]`..`[rbp-24]` window other runtime emitters
/// reserve for pushed callee-saved registers.
fn emit_count_chars_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: count_chars ---");
    emitter.label_global("__rt_count_chars");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer across the result helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the tally and saved arguments
    emitter.instruction("sub rsp, 2128");                                       // reserve the saved-argument slots plus the 2 KiB byte tally
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the subject pointer for the counting pass
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // save the subject length for the counting pass
    emitter.instruction("mov QWORD PTR [rbp - 48], rdi");                       // save the requested result mode

    // -- clear the 256-entry byte tally --
    emitter.instruction("xor r9d, r9d");                                        // start clearing at byte value zero
    emitter.label("__rt_count_chars_zero_linux_x86_64");
    emitter.instruction("mov QWORD PTR [rbp + r9*8 - 2128], 0");                // clear one byte-value tally
    emitter.instruction("add r9, 1");                                           // advance to the next byte value
    emitter.instruction("cmp r9, 256");                                         // has the whole tally been cleared?
    emitter.instruction("jb __rt_count_chars_zero_linux_x86_64");               // keep clearing until every byte value is zeroed

    // -- tally every subject byte --
    emitter.instruction("xor r9d, r9d");                                        // start at the first subject byte
    emitter.label("__rt_count_chars_count_linux_x86_64");
    emitter.instruction("cmp r9, rdx");                                         // has the whole subject been counted?
    emitter.instruction("jae __rt_count_chars_count_done_linux_x86_64");        // the tally is complete
    emitter.instruction("movzx r10d, BYTE PTR [rax + r9]");                     // load the next subject byte
    emitter.instruction("add QWORD PTR [rbp + r10*8 - 2128], 1");               // count one more occurrence of that byte value
    emitter.instruction("add r9, 1");                                           // advance to the next subject byte
    emitter.instruction("jmp __rt_count_chars_count_linux_x86_64");             // keep counting subject bytes
    emitter.label("__rt_count_chars_count_done_linux_x86_64");

    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // reload the requested result mode
    emitter.instruction("cmp r11, 3");                                          // do modes 3 and 4 want the byte-list string?
    emitter.instruction("jge __rt_count_chars_string_linux_x86_64");            // render the byte list instead of a tally

    // -- modes 0, 1, and 2 build the integer-keyed tally hash --
    emitter.instruction("mov edi, 16");                                         // seed the tally hash with a small capacity
    emitter.instruction("xor esi, esi");                                        // value_type 0 = integer values
    emitter.instruction("call __rt_hash_new");                                  // allocate the tally hash
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // publish the tally hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 72], 0");                         // start emitting at byte value zero

    emitter.label("__rt_count_chars_array_linux_x86_64");
    emitter.instruction("mov r9, QWORD PTR [rbp - 72]");                        // reload the byte-value loop index
    emitter.instruction("cmp r9, 256");                                         // have all byte values been considered?
    emitter.instruction("jae __rt_count_chars_array_done_linux_x86_64");        // the tally hash is complete
    emitter.instruction("mov r10, QWORD PTR [rbp + r9*8 - 2128]");              // load this byte value's tally
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // reload the requested result mode
    emitter.instruction("test r11, r11");                                       // is the caller asking for every byte value?
    emitter.instruction("jz __rt_count_chars_array_emit_linux_x86_64");         // mode 0 emits every byte value
    emitter.instruction("cmp r11, 1");                                          // is the caller asking for the used byte values only?
    emitter.instruction("jne __rt_count_chars_array_unused_linux_x86_64");      // mode 2 keeps the unused byte values instead
    emitter.instruction("test r10, r10");                                       // did the subject use this byte value?
    emitter.instruction("jz __rt_count_chars_array_next_linux_x86_64");         // mode 1 skips byte values the subject never uses
    emitter.instruction("jmp __rt_count_chars_array_emit_linux_x86_64");        // a used byte value is emitted with its tally
    emitter.label("__rt_count_chars_array_unused_linux_x86_64");
    emitter.instruction("test r10, r10");                                       // did the subject use this byte value?
    emitter.instruction("jnz __rt_count_chars_array_next_linux_x86_64");        // mode 2 skips byte values the subject does use

    emitter.label("__rt_count_chars_array_emit_linux_x86_64");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // reload the tally hash pointer
    emitter.instruction("mov rsi, r9");                                         // key_lo = the byte value
    emitter.instruction("mov rdx, -1");                                         // key_hi = -1 marks an integer key
    emitter.instruction("mov rcx, r10");                                        // value_lo = the occurrence tally
    emitter.instruction("xor r8d, r8d");                                        // integer tallies carry no high word
    emitter.instruction("xor r9d, r9d");                                        // runtime tag 0 marks the tally as an int
    emitter.instruction("call __rt_hash_set");                                  // insert the byte value's tally
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // republish the hash pointer after possible growth

    emitter.label("__rt_count_chars_array_next_linux_x86_64");
    emitter.instruction("mov r9, QWORD PTR [rbp - 72]");                        // reload the byte-value loop index
    emitter.instruction("add r9, 1");                                           // advance to the next byte value
    emitter.instruction("mov QWORD PTR [rbp - 72], r9");                        // publish the advanced loop index
    emitter.instruction("jmp __rt_count_chars_array_linux_x86_64");             // consider the next byte value

    emitter.label("__rt_count_chars_array_done_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // return the finished tally hash
    emitter.instruction("add rsp, 2128");                                       // release the tally frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the tally as a PHP array

    // -- modes 3 and 4 render the selected byte values as a string --
    emitter.label("__rt_count_chars_string_linux_x86_64");
    emitter.instruction("xor r9d, r9d");                                        // start at byte value zero
    emitter.instruction("xor ecx, ecx");                                        // running count of selected byte values

    emitter.label("__rt_count_chars_select_linux_x86_64");
    emitter.instruction("cmp r9, 256");                                         // have all byte values been considered?
    emitter.instruction("jae __rt_count_chars_select_done_linux_x86_64");       // the exact result size is known
    emitter.instruction("mov r10, QWORD PTR [rbp + r9*8 - 2128]");              // load this byte value's tally
    emitter.instruction("cmp r11, 3");                                          // is the caller asking for the used byte values?
    emitter.instruction("jne __rt_count_chars_select_unused_linux_x86_64");     // mode 4 selects the unused byte values instead
    emitter.instruction("test r10, r10");                                       // did the subject use this byte value?
    emitter.instruction("jz __rt_count_chars_select_next_linux_x86_64");        // mode 3 skips byte values the subject never uses
    emitter.instruction("jmp __rt_count_chars_select_hit_linux_x86_64");        // a used byte value joins the result
    emitter.label("__rt_count_chars_select_unused_linux_x86_64");
    emitter.instruction("test r10, r10");                                       // did the subject use this byte value?
    emitter.instruction("jnz __rt_count_chars_select_next_linux_x86_64");       // mode 4 skips byte values the subject does use
    emitter.label("__rt_count_chars_select_hit_linux_x86_64");
    emitter.instruction("add rcx, 1");                                          // reserve one more result byte
    emitter.label("__rt_count_chars_select_next_linux_x86_64");
    emitter.instruction("add r9, 1");                                           // advance to the next byte value
    emitter.instruction("jmp __rt_count_chars_select_linux_x86_64");            // keep sizing the result

    emitter.label("__rt_count_chars_select_done_linux_x86_64");
    emitter.instruction("mov QWORD PTR [rbp - 64], rcx");                       // save the exact result byte count
    emitter.instruction("mov rax, rcx");                                        // request exactly that many bytes
    emitter.instruction("call __rt_concat_reserve");                            // reserve scratch or heap storage for the byte list
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save the reserved result start
    emitter.instruction("mov rsi, rax");                                        // destination cursor
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // reload the requested result mode
    emitter.instruction("xor r9d, r9d");                                        // start at byte value zero

    emitter.label("__rt_count_chars_fill_linux_x86_64");
    emitter.instruction("cmp r9, 256");                                         // have all byte values been considered?
    emitter.instruction("jae __rt_count_chars_fill_done_linux_x86_64");         // the byte list is complete
    emitter.instruction("mov r10, QWORD PTR [rbp + r9*8 - 2128]");              // load this byte value's tally
    emitter.instruction("cmp r11, 3");                                          // is the caller asking for the used byte values?
    emitter.instruction("jne __rt_count_chars_fill_unused_linux_x86_64");       // mode 4 writes the unused byte values instead
    emitter.instruction("test r10, r10");                                       // did the subject use this byte value?
    emitter.instruction("jz __rt_count_chars_fill_next_linux_x86_64");          // mode 3 skips byte values the subject never uses
    emitter.instruction("jmp __rt_count_chars_fill_hit_linux_x86_64");          // a used byte value joins the result
    emitter.label("__rt_count_chars_fill_unused_linux_x86_64");
    emitter.instruction("test r10, r10");                                       // did the subject use this byte value?
    emitter.instruction("jnz __rt_count_chars_fill_next_linux_x86_64");         // mode 4 skips byte values the subject does use
    emitter.label("__rt_count_chars_fill_hit_linux_x86_64");
    emitter.instruction("mov BYTE PTR [rsi], r9b");                             // append the selected byte value to the result
    emitter.instruction("add rsi, 1");                                          // advance the destination cursor
    emitter.label("__rt_count_chars_fill_next_linux_x86_64");
    emitter.instruction("add r9, 1");                                           // advance to the next byte value
    emitter.instruction("jmp __rt_count_chars_fill_linux_x86_64");              // keep filling the byte list

    emitter.label("__rt_count_chars_fill_done_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // the byte list starts at the reserved pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 64]");                       // the byte list is exactly as long as it was sized
    elephc_memoir::strings::concat_scratch::emit_finish_owned(emitter, 56, 72);
    emitter.instruction("add rsp, 2128");                                       // release the tally frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the byte list as a PHP string pair
}
