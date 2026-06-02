//! Purpose:
//! Emits the `__rt_stream_select` runtime helper, which waits for readability,
//! writability, or exceptional conditions across descriptor sets.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//!
//! Key details:
//! - macOS uses `select` (5 args, `timeval`); Linux uses `pselect6` (6 args,
//!   `timespec` + signal mask). The helper sets up six argument registers so a
//!   single `svc` works after `linux_transform` remaps the syscall number.
//! - Descriptor sets are word-0-only bitmaps: descriptors must be below 64.
//!   The three resource arrays are compacted in place to the ready subset.

use crate::codegen::{emit::Emitter, platform::{Arch, Platform}};

/// stream_select: select across three resource arrays, compacting each to the
/// ready subset and returning the number of ready descriptors.
/// Input:  AArch64 x0/x1/x2 = read/write/except arrays, x3 = seconds, x4 = microseconds
///         x86_64  rdi/rsi/rdx = read/write/except arrays, rcx = seconds, r8 = microseconds
/// Output: the count of ready descriptors, or -1 on failure
pub fn emit_stream_select(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_select_linux_x86_64(emitter);
        return;
    }

    let linux = emitter.target.platform == Platform::Linux;

    emitter.blank();
    emitter.comment("--- runtime: stream_select ---");
    emitter.label_global("__rt_stream_select");

    // Frame (160 bytes): [0]=read_arr [8]=write_arr [16]=except_arr
    //   [24]=read_fds [32]=write_fds [40]=except_fds (word-0 bitmaps)
    //   [48]=timeout.sec [56]=timeout.frac [64]=ready count [80]=x29 [88]=x30
    //   [96..152]=loop-register spill slots saved across __rt_user_wrapper_stream_cast
    emitter.instruction("sub sp, sp, #160");                                    // allocate the select state frame
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the read resource array
    emitter.instruction("str x1, [sp, #8]");                                    // save the write resource array
    emitter.instruction("str x2, [sp, #16]");                                   // save the except resource array
    emitter.instruction("str x3, [sp, #48]");                                   // timeout seconds field

    // -- timeout fractional field: timeval microseconds vs timespec nanoseconds --
    if linux {
        emitter.instruction("mov x9, #1000");                                   // a microsecond holds 1000 nanoseconds
        emitter.instruction("mul x4, x4, x9");                                  // convert microseconds to timespec nanoseconds
    }
    emitter.instruction("str x4, [sp, #56]");                                   // timeout fractional field

    // -- clear the three word-0 descriptor bitmaps --
    emitter.instruction("str xzr, [sp, #24]");                                  // clear the read descriptor bitmap
    emitter.instruction("str xzr, [sp, #32]");                                  // clear the write descriptor bitmap
    emitter.instruction("str xzr, [sp, #40]");                                  // clear the except descriptor bitmap

    for (arr_off, fds_off, suffix) in [(0, 24, "r"), (8, 32, "w"), (16, 40, "e")] {
        emit_build_fdset_aarch64(emitter, arr_off, fds_off, suffix);
    }

    // -- select(nfds=64, read, write, except, timeout, sigmask) --
    emitter.instruction("mov x0, #64");                                         // nfds: examine descriptors 0..63
    emitter.instruction("add x1, sp, #24");                                     // read descriptor bitmap pointer
    emitter.instruction("add x2, sp, #32");                                     // write descriptor bitmap pointer
    emitter.instruction("add x3, sp, #40");                                     // except descriptor bitmap pointer
    emitter.instruction("add x4, sp, #48");                                     // timeout struct pointer
    emitter.instruction("mov x5, #0");                                          // signal mask pointer (NULL); ignored by macOS select
    emitter.syscall(93);
    emitter.instruction("str x0, [sp, #64]");                                   // save the ready descriptor count

    for (arr_off, fds_off, suffix) in [(0, 24, "r"), (8, 32, "w"), (16, 40, "e")] {
        emit_compact_aarch64(emitter, arr_off, fds_off, suffix);
    }

    emitter.instruction("ldr x0, [sp, #64]");                                   // return the ready descriptor count
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #160");                                    // release the select state frame
    emitter.instruction("ret");                                                 // return to the caller
}

/// Set one bit per in-range descriptor of `arr` into the word-0 bitmap at `fds_off`.
/// Handles both raw-int resource arrays (`value_type != 7`) and Mixed-boxed
/// arrays (`value_type == 7`, slots hold Mixed* cells whose payload_lo is the fd).
fn emit_build_fdset_aarch64(emitter: &mut Emitter, arr_off: i64, fds_off: i64, suffix: &str) {
    let loop_l = format!("__rt_stream_select_build_{}_loop", suffix);
    let next_l = format!("__rt_stream_select_build_{}_next", suffix);
    let unbox_l = format!("__rt_stream_select_build_{}_unbox", suffix);
    let after_unbox_l = format!("__rt_stream_select_build_{}_after_unbox", suffix);
    let done_l = format!("__rt_stream_select_build_{}_done", suffix);

    emitter.instruction(&format!("ldr x9, [sp, #{}]", arr_off));                // load the resource array pointer
    emitter.instruction("ldr x10, [x9]");                                       // load the array length
    emitter.instruction("ldr x4, [x9, #-8]");                                   // load the packed indexed-array kind word
    emitter.instruction("lsr x4, x4, #8");                                      // shift the value_type tag into the low byte
    emitter.instruction("and x4, x4, #0x7f");                                   // isolate the value_type tag
    emitter.instruction("add x12, x9, #24");                                    // skip the array header to the data region
    emitter.instruction("mov x11, #0");                                         // descriptor index
    emitter.label(&loop_l);
    emitter.instruction("cmp x11, x10");                                        // have all descriptors been scanned?
    emitter.instruction(&format!("b.ge {}", done_l));                           // bitmap is fully built
    emitter.instruction("ldr x13, [x12, x11, lsl #3]");                         // load the slot value (raw fd or Mixed* cell)
    emitter.instruction("cmp x4, #7");                                          // is this a Mixed-boxed indexed array?
    emitter.instruction(&format!("b.eq {}", unbox_l));                          // unbox the Mixed cell to get the underlying fd
    emitter.instruction(&format!("b {}", after_unbox_l));                       // raw-int array: x13 already holds the fd
    emitter.label(&unbox_l);
    emitter.instruction(&format!("cbz x13, {}", next_l));                       // null Mixed cell → skip the descriptor
    emitter.instruction("ldr x13, [x13, #8]");                                  // payload_lo of the Mixed cell is the fd
    emitter.label(&after_unbox_l);
    // -- resolve synthetic user-wrapper fds to a real selectable fd via stream_cast --
    let cast_skip_l = format!("__rt_stream_select_build_{}_castskip", suffix);
    emitter.instruction("tst x13, #0x40000000");                                // is this a synthetic user-wrapper descriptor?
    emitter.instruction(&format!("b.eq {}", cast_skip_l));                      // ordinary OS fd → use it directly
    emitter.instruction("str x9, [sp, #96]");                                   // spill the array pointer across the cast call
    emitter.instruction("str x10, [sp, #104]");                                 // spill the array length
    emitter.instruction("str x11, [sp, #112]");                                 // spill the descriptor index
    emitter.instruction("str x4, [sp, #120]");                                  // spill the value_type tag
    emitter.instruction("str x12, [sp, #128]");                                 // spill the data-region pointer
    emitter.instruction("mov x0, x13");                                         // synthetic fd → stream_cast argument
    emitter.instruction("mov x1, #3");                                          // STREAM_CAST_FOR_SELECT
    emitter.instruction("bl __rt_user_wrapper_stream_cast");                    // resolve to the wrapper's underlying fd (or -1)
    emitter.instruction("mov x13, x0");                                         // adopt the resolved descriptor
    emitter.instruction("ldr x9, [sp, #96]");                                   // reload the array pointer
    emitter.instruction("ldr x10, [sp, #104]");                                 // reload the array length
    emitter.instruction("ldr x11, [sp, #112]");                                 // reload the descriptor index
    emitter.instruction("ldr x4, [sp, #120]");                                  // reload the value_type tag
    emitter.instruction("ldr x12, [sp, #128]");                                 // reload the data-region pointer
    emitter.label(&cast_skip_l);
    emitter.instruction("cmp x13, #0");                                         // is the descriptor negative?
    emitter.instruction(&format!("b.lt {}", next_l));                           // skip descriptors that cannot be set
    emitter.instruction("cmp x13, #64");                                        // is the descriptor outside the word-0 range?
    emitter.instruction(&format!("b.ge {}", next_l));                           // skip out-of-range descriptors
    emitter.instruction("mov x14, #1");                                         // bit seed for the descriptor
    emitter.instruction("lsl x14, x14, x13");                                   // shift the bit into the descriptor position
    emitter.instruction(&format!("ldr x15, [sp, #{}]", fds_off));               // load the current bitmap word
    emitter.instruction("orr x15, x15, x14");                                   // set the descriptor bit
    emitter.instruction(&format!("str x15, [sp, #{}]", fds_off));               // store the updated bitmap word
    emitter.label(&next_l);
    emitter.instruction("add x11, x11, #1");                                    // advance to the next descriptor
    emitter.instruction(&format!("b {}", loop_l));                              // continue scanning the array
    emitter.label(&done_l);
}

/// Compact `arr` in place to the descriptors whose bit survives in the bitmap.
/// Handles both raw-int arrays and Mixed-boxed arrays: for Mixed slots, the
/// underlying fd is read from `[slot, #8]` for the bitmap check, but the
/// slot itself (the Mixed* cell) is what gets retained in the compacted
/// prefix so the caller's array keeps its boxed contents.
fn emit_compact_aarch64(emitter: &mut Emitter, arr_off: i64, fds_off: i64, suffix: &str) {
    let loop_l = format!("__rt_stream_select_keep_{}_loop", suffix);
    let next_l = format!("__rt_stream_select_keep_{}_next", suffix);
    let unbox_l = format!("__rt_stream_select_keep_{}_unbox", suffix);
    let after_unbox_l = format!("__rt_stream_select_keep_{}_after_unbox", suffix);
    let done_l = format!("__rt_stream_select_keep_{}_done", suffix);

    emitter.instruction(&format!("ldr x9, [sp, #{}]", arr_off));                // load the resource array pointer
    emitter.instruction("ldr x10, [x9]");                                       // load the array length
    emitter.instruction(&format!("ldr x14, [sp, #{}]", fds_off));               // load the post-select bitmap word
    emitter.instruction("ldr x4, [x9, #-8]");                                   // load the packed indexed-array kind word
    emitter.instruction("lsr x4, x4, #8");                                      // shift the value_type tag into the low byte
    emitter.instruction("and x4, x4, #0x7f");                                   // isolate the value_type tag
    emitter.instruction("add x12, x9, #24");                                    // skip the array header to the data region
    emitter.instruction("mov x11, #0");                                         // source descriptor index
    emitter.instruction("mov x13, #0");                                         // destination (kept) descriptor index
    emitter.label(&loop_l);
    emitter.instruction("cmp x11, x10");                                        // have all descriptors been scanned?
    emitter.instruction(&format!("b.ge {}", done_l));                           // compaction is complete
    emitter.instruction("ldr x15, [x12, x11, lsl #3]");                         // load the raw slot value (preserved for the kept-array store)
    emitter.instruction("mov x16, x15");                                        // copy for fd extraction; x15 stays the slot's stored value
    emitter.instruction("cmp x4, #7");                                          // is this a Mixed-boxed indexed array?
    emitter.instruction(&format!("b.eq {}", unbox_l));                          // unbox the Mixed cell to get the underlying fd
    emitter.instruction(&format!("b {}", after_unbox_l));                       // raw-int array: x16 already holds the fd
    emitter.label(&unbox_l);
    emitter.instruction(&format!("cbz x16, {}", next_l));                       // null Mixed cell → drop the slot
    emitter.instruction("ldr x16, [x16, #8]");                                  // payload_lo of the Mixed cell is the fd
    emitter.label(&after_unbox_l);
    // -- resolve synthetic user-wrapper fds to the same real fd used at build --
    let cast_skip_l = format!("__rt_stream_select_keep_{}_castskip", suffix);
    emitter.instruction("tst x16, #0x40000000");                                // is this a synthetic user-wrapper descriptor?
    emitter.instruction(&format!("b.eq {}", cast_skip_l));                      // ordinary OS fd → use it directly
    emitter.instruction("str x9, [sp, #96]");                                   // spill the array pointer across the cast call
    emitter.instruction("str x10, [sp, #104]");                                 // spill the array length
    emitter.instruction("str x11, [sp, #112]");                                 // spill the source index
    emitter.instruction("str x13, [sp, #120]");                                 // spill the destination (kept) index
    emitter.instruction("str x14, [sp, #136]");                                 // spill the post-select bitmap word
    emitter.instruction("str x4, [sp, #144]");                                  // spill the value_type tag
    emitter.instruction("str x12, [sp, #152]");                                 // spill the data-region pointer
    emitter.instruction("str x15, [sp, #128]");                                 // spill the original slot value (kept on a match)
    emitter.instruction("mov x0, x16");                                         // synthetic fd → stream_cast argument
    emitter.instruction("mov x1, #3");                                          // STREAM_CAST_FOR_SELECT
    emitter.instruction("bl __rt_user_wrapper_stream_cast");                    // resolve to the wrapper's underlying fd (idempotent with build)
    emitter.instruction("mov x16, x0");                                         // adopt the resolved descriptor
    emitter.instruction("ldr x9, [sp, #96]");                                   // reload the array pointer
    emitter.instruction("ldr x10, [sp, #104]");                                 // reload the array length
    emitter.instruction("ldr x11, [sp, #112]");                                 // reload the source index
    emitter.instruction("ldr x13, [sp, #120]");                                 // reload the destination index
    emitter.instruction("ldr x14, [sp, #136]");                                 // reload the post-select bitmap word
    emitter.instruction("ldr x4, [sp, #144]");                                  // reload the value_type tag
    emitter.instruction("ldr x12, [sp, #152]");                                 // reload the data-region pointer
    emitter.instruction("ldr x15, [sp, #128]");                                 // reload the original slot value
    emitter.label(&cast_skip_l);
    emitter.instruction("cmp x16, #0");                                         // is the descriptor negative?
    emitter.instruction(&format!("b.lt {}", next_l));                           // drop descriptors that were never set
    emitter.instruction("cmp x16, #64");                                        // is the descriptor outside the word-0 range?
    emitter.instruction(&format!("b.ge {}", next_l));                           // drop out-of-range descriptors
    emitter.instruction("lsr x6, x14, x16");                                    // shift the descriptor bit to position 0
    emitter.instruction("and x6, x6, #1");                                      // isolate the descriptor's ready bit
    emitter.instruction(&format!("cbz x6, {}", next_l));                        // drop descriptors that are not ready
    emitter.instruction("str x15, [x12, x13, lsl #3]");                         // keep the original slot value (raw fd or Mixed*) at the front
    emitter.instruction("add x13, x13, #1");                                    // advance the kept-descriptor index
    emitter.label(&next_l);
    emitter.instruction("add x11, x11, #1");                                    // advance to the next source descriptor
    emitter.instruction(&format!("b {}", loop_l));                              // continue compacting the array
    emitter.label(&done_l);
    emitter.instruction("str x13, [x9]");                                       // store the compacted array length
}

fn emit_stream_select_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_select ---");
    emitter.label_global("__rt_stream_select");

    // Frame (rbp-relative): [-8]=read_arr [-16]=write_arr [-24]=except_arr
    //   [-32]=read_fds [-40]=write_fds [-48]=except_fds
    //   [-56]=timeout.sec [-64]=timeout.nsec [-72]=ready count
    //   [-88..-120]=caller-saved loop registers spilled across
    //   __rt_user_wrapper_stream_cast (r12/r13 are callee-saved and survive it)
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 160");                                        // allocate the select state frame (incl. cast-call spill slots)
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the read resource array
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the write resource array
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the except resource array
    emitter.instruction("mov QWORD PTR [rbp - 56], rcx");                       // timeout seconds field
    emitter.instruction("mov rax, r8");                                         // microseconds into a scratch register
    emitter.instruction("imul rax, rax, 1000");                                 // convert microseconds to timespec nanoseconds
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // timeout nanoseconds field

    // -- clear the three word-0 descriptor bitmaps --
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // clear the read descriptor bitmap
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // clear the write descriptor bitmap
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // clear the except descriptor bitmap

    for (arr_off, fds_off, suffix) in [(8, 32, "r"), (16, 40, "w"), (24, 48, "e")] {
        emit_build_fdset_x86(emitter, arr_off, fds_off, suffix);
    }

    // -- pselect6(nfds=64, read, write, except, timeout, sigmask) --
    emitter.instruction("mov edi, 64");                                         // nfds: examine descriptors 0..63
    emitter.instruction("lea rsi, [rbp - 32]");                                 // read descriptor bitmap pointer
    emitter.instruction("lea rdx, [rbp - 40]");                                 // write descriptor bitmap pointer
    emitter.instruction("lea r10, [rbp - 48]");                                 // except descriptor bitmap pointer
    emitter.instruction("lea r8, [rbp - 56]");                                  // timeout struct pointer
    emitter.instruction("xor r9d, r9d");                                        // signal mask pointer (NULL)
    emitter.instruction("mov eax, 270");                                        // Linux x86_64 syscall 270 = pselect6
    emitter.instruction("syscall");                                             // wait for descriptor readiness
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // save the ready descriptor count

    for (arr_off, fds_off, suffix) in [(8, 32, "r"), (16, 40, "w"), (24, 48, "e")] {
        emit_compact_x86(emitter, arr_off, fds_off, suffix);
    }

    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // return the ready descriptor count
    emitter.instruction("add rsp, 160");                                        // release the select state frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller
}

fn emit_build_fdset_x86(emitter: &mut Emitter, arr_off: i64, fds_off: i64, suffix: &str) {
    let loop_l = format!("__rt_stream_select_build_{}_loop_x86", suffix);
    let next_l = format!("__rt_stream_select_build_{}_next_x86", suffix);
    let unbox_l = format!("__rt_stream_select_build_{}_unbox_x86", suffix);
    let after_unbox_l = format!("__rt_stream_select_build_{}_after_unbox_x86", suffix);
    let done_l = format!("__rt_stream_select_build_{}_done_x86", suffix);

    emitter.instruction(&format!("mov r11, QWORD PTR [rbp - {}]", arr_off));    // load the resource array pointer
    emitter.instruction("mov rdi, QWORD PTR [r11]");                            // load the array length
    emitter.instruction("mov r12, QWORD PTR [r11 - 8]");                        // load the packed indexed-array kind word
    emitter.instruction("shr r12, 8");                                          // shift the value_type tag into the low byte
    emitter.instruction("and r12, 0x7f");                                       // isolate the value_type tag
    emitter.instruction("xor rsi, rsi");                                        // descriptor index
    emitter.label(&loop_l);
    emitter.instruction("cmp rsi, rdi");                                        // have all descriptors been scanned?
    emitter.instruction(&format!("jae {}", done_l));                            // bitmap is fully built
    emitter.instruction("mov rdx, QWORD PTR [r11 + 24 + rsi * 8]");             // load the slot value (raw fd or Mixed* cell)
    emitter.instruction("cmp r12, 7");                                          // is this a Mixed-boxed indexed array?
    emitter.instruction(&format!("je {}", unbox_l));                            // unbox the Mixed cell to get the underlying fd
    emitter.instruction(&format!("jmp {}", after_unbox_l));                     // raw-int array: rdx already holds the fd
    emitter.label(&unbox_l);
    emitter.instruction("test rdx, rdx");                                       // null Mixed cell?
    emitter.instruction(&format!("jz {}", next_l));                             // null Mixed cell → skip the descriptor
    emitter.instruction("mov rdx, QWORD PTR [rdx + 8]");                        // payload_lo of the Mixed cell is the fd
    emitter.label(&after_unbox_l);
    // -- resolve synthetic user-wrapper fds to a real selectable fd via stream_cast --
    let cast_skip_l = format!("__rt_stream_select_build_{}_castskip_x86", suffix);
    emitter.instruction("test rdx, 0x40000000");                                // is this a synthetic user-wrapper descriptor?
    emitter.instruction(&format!("jz {}", cast_skip_l));                        // ordinary OS fd → use it directly
    emitter.instruction("mov QWORD PTR [rbp - 88], r11");                       // spill the array pointer across the cast call
    emitter.instruction("mov QWORD PTR [rbp - 96], rdi");                       // spill the array length
    emitter.instruction("mov QWORD PTR [rbp - 104], rsi");                      // spill the descriptor index
    emitter.instruction("mov rdi, rdx");                                        // synthetic fd → stream_cast argument
    emitter.instruction("mov esi, 3");                                          // STREAM_CAST_FOR_SELECT
    emitter.instruction("call __rt_user_wrapper_stream_cast");                  // resolve to the wrapper's underlying fd (or -1)
    emitter.instruction("mov rdx, rax");                                        // adopt the resolved descriptor
    emitter.instruction("mov r11, QWORD PTR [rbp - 88]");                       // reload the array pointer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 96]");                       // reload the array length
    emitter.instruction("mov rsi, QWORD PTR [rbp - 104]");                      // reload the descriptor index
    emitter.label(&cast_skip_l);
    emitter.instruction("cmp rdx, 0");                                          // is the descriptor negative?
    emitter.instruction(&format!("jl {}", next_l));                             // skip descriptors that cannot be set
    emitter.instruction("cmp rdx, 64");                                         // is the descriptor outside the word-0 range?
    emitter.instruction(&format!("jae {}", next_l));                            // skip out-of-range descriptors
    emitter.instruction("mov rcx, rdx");                                        // descriptor position for the shift count
    emitter.instruction("mov rax, 1");                                          // bit seed for the descriptor
    emitter.instruction("shl rax, cl");                                         // shift the bit into the descriptor position
    emitter.instruction(&format!("or QWORD PTR [rbp - {}], rax", fds_off));     // set the descriptor bit
    emitter.label(&next_l);
    emitter.instruction("add rsi, 1");                                          // advance to the next descriptor
    emitter.instruction(&format!("jmp {}", loop_l));                            // continue scanning the array
    emitter.label(&done_l);
}

fn emit_compact_x86(emitter: &mut Emitter, arr_off: i64, fds_off: i64, suffix: &str) {
    let loop_l = format!("__rt_stream_select_keep_{}_loop_x86", suffix);
    let next_l = format!("__rt_stream_select_keep_{}_next_x86", suffix);
    let unbox_l = format!("__rt_stream_select_keep_{}_unbox_x86", suffix);
    let after_unbox_l = format!("__rt_stream_select_keep_{}_after_unbox_x86", suffix);
    let done_l = format!("__rt_stream_select_keep_{}_done_x86", suffix);

    emitter.instruction(&format!("mov r11, QWORD PTR [rbp - {}]", arr_off));    // load the resource array pointer
    emitter.instruction("mov rdi, QWORD PTR [r11]");                            // load the array length
    emitter.instruction(&format!("mov r8, QWORD PTR [rbp - {}]", fds_off));     // load the post-select bitmap word
    emitter.instruction("mov r12, QWORD PTR [r11 - 8]");                        // load the packed indexed-array kind word
    emitter.instruction("shr r12, 8");                                          // shift the value_type tag into the low byte
    emitter.instruction("and r12, 0x7f");                                       // isolate the value_type tag
    emitter.instruction("xor rsi, rsi");                                        // source descriptor index
    emitter.instruction("xor r9, r9");                                          // destination (kept) descriptor index
    emitter.label(&loop_l);
    emitter.instruction("cmp rsi, rdi");                                        // have all descriptors been scanned?
    emitter.instruction(&format!("jae {}", done_l));                            // compaction is complete
    emitter.instruction("mov r13, QWORD PTR [r11 + 24 + rsi * 8]");             // load the raw slot value (preserved for the kept-array store)
    emitter.instruction("mov rdx, r13");                                        // copy for fd extraction; r13 stays the slot's stored value
    emitter.instruction("cmp r12, 7");                                          // is this a Mixed-boxed indexed array?
    emitter.instruction(&format!("je {}", unbox_l));                            // unbox the Mixed cell to get the underlying fd
    emitter.instruction(&format!("jmp {}", after_unbox_l));                     // raw-int array: rdx already holds the fd
    emitter.label(&unbox_l);
    emitter.instruction("test rdx, rdx");                                       // null Mixed cell?
    emitter.instruction(&format!("jz {}", next_l));                             // null Mixed cell → drop the slot
    emitter.instruction("mov rdx, QWORD PTR [rdx + 8]");                        // payload_lo of the Mixed cell is the fd
    emitter.label(&after_unbox_l);
    // -- resolve synthetic user-wrapper fds to the same real fd used at build --
    let cast_skip_l = format!("__rt_stream_select_keep_{}_castskip_x86", suffix);
    emitter.instruction("test rdx, 0x40000000");                                // is this a synthetic user-wrapper descriptor?
    emitter.instruction(&format!("jz {}", cast_skip_l));                        // ordinary OS fd → use it directly
    emitter.instruction("mov QWORD PTR [rbp - 88], r11");                       // spill the array pointer across the cast call
    emitter.instruction("mov QWORD PTR [rbp - 96], rdi");                       // spill the array length
    emitter.instruction("mov QWORD PTR [rbp - 104], rsi");                      // spill the source index
    emitter.instruction("mov QWORD PTR [rbp - 112], r8");                       // spill the post-select bitmap word
    emitter.instruction("mov QWORD PTR [rbp - 120], r9");                       // spill the destination (kept) index
    emitter.instruction("mov rdi, rdx");                                        // synthetic fd → stream_cast argument
    emitter.instruction("mov esi, 3");                                          // STREAM_CAST_FOR_SELECT
    emitter.instruction("call __rt_user_wrapper_stream_cast");                  // resolve to the wrapper's underlying fd (idempotent with build)
    emitter.instruction("mov rdx, rax");                                        // adopt the resolved descriptor
    emitter.instruction("mov r11, QWORD PTR [rbp - 88]");                       // reload the array pointer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 96]");                       // reload the array length
    emitter.instruction("mov rsi, QWORD PTR [rbp - 104]");                      // reload the source index
    emitter.instruction("mov r8, QWORD PTR [rbp - 112]");                       // reload the post-select bitmap word
    emitter.instruction("mov r9, QWORD PTR [rbp - 120]");                       // reload the destination index
    emitter.label(&cast_skip_l);
    emitter.instruction("cmp rdx, 0");                                          // is the descriptor negative?
    emitter.instruction(&format!("jl {}", next_l));                             // drop descriptors that were never set
    emitter.instruction("cmp rdx, 64");                                         // is the descriptor outside the word-0 range?
    emitter.instruction(&format!("jae {}", next_l));                            // drop out-of-range descriptors
    emitter.instruction("mov rcx, rdx");                                        // descriptor position for the shift count
    emitter.instruction("mov rax, r8");                                         // copy the bitmap word for testing
    emitter.instruction("shr rax, cl");                                         // shift the descriptor bit to position 0
    emitter.instruction("and rax, 1");                                          // isolate the descriptor's ready bit
    emitter.instruction(&format!("jz {}", next_l));                             // drop descriptors that are not ready
    emitter.instruction("mov QWORD PTR [r11 + 24 + r9 * 8], r13");              // keep the original slot value (raw fd or Mixed*) at the front
    emitter.instruction("add r9, 1");                                           // advance the kept-descriptor index
    emitter.label(&next_l);
    emitter.instruction("add rsi, 1");                                          // advance to the next source descriptor
    emitter.instruction(&format!("jmp {}", loop_l));                            // continue compacting the array
    emitter.label(&done_l);
    emitter.instruction("mov QWORD PTR [r11], r9");                             // store the compacted array length
}
