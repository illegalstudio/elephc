//! Purpose:
//! Emits `__rt_array_splice_str` and `__rt_array_splice_insert_str`, the `array_splice()` runtime
//! helpers for indexed arrays whose payload slots are string `{pointer, length}` pairs.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//! - The EIR lowering of `array_splice()` in `crate::codegen::lower_inst::builtins::arrays`.
//!
//! Key details:
//! - Indexed string arrays use 16-byte slots (`__rt_array_new(n, 16)`), not the 8-byte slots the
//!   scalar and refcounted splice helpers move. Running those over a string array copied one word
//!   per element, so the removed-elements array came back holding raw pointers read as integers
//!   and the source array kept a half-shifted payload.
//! - The removal MOVES each string slot into the result. An indexed string array owns its
//!   persisted payloads exclusively (`__rt_array_clone_shallow` re-persists them on a
//!   copy-on-write split and `__rt_array_free_deep` frees each one), so transferring the pointer
//!   keeps exactly one owner per string. Retaining or copying here would double-free or leak.
//! - The insertion DUPLICATES each replacement string through `__rt_str_persist`, because the
//!   replacement array keeps owning its own payloads and is released by the caller afterwards.
//! - The destination is grown before anything is written, and the tail slide walks backwards, so
//!   a replacement longer than the removed window never overwrites a slot it has not moved yet.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::runtime::arrays::slice_bounds::emit_slice_bounds;
use elephc_memoir::values::{array_splice_insert, array_splice_remove};

/// Emits the `__rt_array_splice_str` runtime helper for the active target.
///
/// Removes `$length` string slots starting at `$offset`, returns them in a freshly allocated
/// 16-byte-slot indexed array, and compacts the source payload over the gap.
///
/// ## ARM64 ABI
/// - **Input**: `x0` = source indexed array, `x1` = `$offset`, `x2` = `$length`, `x3` = 1 when a
///   `$length` was supplied and 0 when it was omitted or `null`
/// - **Output**: `x0` = the removed-elements array, `x1` = the normalized removal offset, i.e.
///   the index a `$replacement` is inserted at
///
/// ## x86_64 ABI
/// - **Input**: `rdi`, `rsi`, `rdx`, `rcx` with the same meaning
/// - **Output**: `rax` = the removed-elements array, `rdx` = the normalized removal offset
///
/// The window is normalized by the shared `emit_slice_bounds` prologue, so the removal count is
/// always in `[0, length - offset]` and neither loop can step outside the source payload.
pub fn emit_array_splice_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_splice_str_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_splice_str ---");
    emitter.label_global("__rt_array_splice_str");

    // Stack layout: [sp,#0] source array, [sp,#8] normalized offset, [sp,#16] removal count,
    //               [sp,#24] result array, [sp,#32] saved x29/x30.
    emitter.instruction("sub sp, sp, #48");                                     // reserve the string-splice bookkeeping slots plus the saved frame registers
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the source indexed-array pointer across the result-array constructor

    // -- normalize the requested removal window against PHP's offset/length rules --
    emit_slice_bounds(emitter, "__rt_array_splice_str");
    array_splice_remove::emit_remove_strings(emitter);
}

/// Emits the x86_64 System V variant of `__rt_array_splice_str`.
///
/// Mirrors the ARM64 sequence instruction for instruction; only the register encoding differs.
/// See [`emit_array_splice_str`] for the full ABI and the ownership contract.
fn emit_array_splice_str_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_splice_str ---");
    emitter.label_global("__rt_array_splice_str");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving the string-splice spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the source, window, and result bookkeeping
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill slots while keeping the constructor call 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the source indexed-array pointer across the result-array constructor

    // -- normalize the requested removal window against PHP's offset/length rules --
    emit_slice_bounds(emitter, "__rt_array_splice_str");
    array_splice_remove::emit_remove_strings(emitter);
}

/// Emits the `__rt_array_splice_insert_str` runtime helper for the active target.
///
/// Writes `$replacement`'s strings into the 16-byte-slot gap the removal opened, duplicating each
/// payload with `__rt_str_persist` so the destination and the replacement array each own their own
/// bytes.
///
/// ## ARM64 ABI
/// - **Input**: `x0` = destination indexed array, `x1` = insertion index, `x2` = replacement
///   indexed array (0 inserts nothing)
/// - **Output**: `x0` = the possibly-relocated destination indexed array
///
/// ## x86_64 ABI
/// - **Input**: `rdi` = destination, `rsi` = insertion index, `rdx` = replacement
/// - **Output**: `rax` = the possibly-relocated destination indexed array
///
/// A destination that is still empty carries the 8-byte `array<never>` shape its literal was
/// allocated with, so the first write re-scales the capacity into 16-byte slots and stamps the
/// string `value_type` exactly like `__rt_array_push_str` does. The insertion index is clamped to
/// `[0, length]`, so an unnormalized offset still cannot write outside the payload.
pub fn emit_array_splice_insert_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_splice_insert_str_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_splice_insert_str ---");
    emitter.label_global("__rt_array_splice_insert_str");

    // Stack layout: [sp,#0] destination array, [sp,#8] replacement array,
    //               [sp,#16] insertion index, [sp,#24] copy loop index, [sp,#48] saved x29/x30.
    emitter.instruction("sub sp, sp, #64");                                     // reserve the insertion bookkeeping slots plus the saved frame registers
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the destination indexed-array pointer across growth
    emitter.instruction("str x2, [sp, #8]");                                    // preserve the replacement indexed-array pointer across growth
    emitter.instruction("str x1, [sp, #16]");                                   // preserve the requested insertion index across growth
    emitter.instruction("str xzr, [sp, #24]");                                  // start the replacement copy loop at the first replacement slot
    emitter.instruction("cbz x2, __rt_asis_done");                              // a null replacement inserts nothing
    emitter.instruction("ldr x9, [x2]");                                        // x9 = replacement element count
    emitter.instruction("cbz x9, __rt_asis_done");                              // an empty replacement inserts nothing

    // -- specialize a still-empty destination to 16-byte string slots before the first write --
    emitter.instruction("ldr x10, [x0]");                                       // load the destination logical length
    emitter.instruction("cbnz x10, __rt_asis_shape_ready");                     // a non-empty destination already has its string shape fixed
    emitter.instruction("ldr x11, [x0, #16]");                                  // x11 = old elem_size (8 for an empty array<never> buffer)
    emitter.instruction("ldr x12, [x0, #8]");                                   // x12 = old capacity counted in old-elem_size slots
    emitter.instruction("mul x12, x12, x11");                                   // x12 = backing-store data bytes already reserved
    emitter.instruction("lsr x12, x12, #4");                                    // reinterpret the same bytes as 16-byte string slots
    emitter.instruction("str x12, [x0, #8]");                                   // publish slot-accurate capacity before any 16-byte write
    emitter.instruction("mov x11, #16");                                        // string payload slots carry a pointer and a length
    emitter.instruction("str x11, [x0, #16]");                                  // elem_size = 16 before any future grow copies live string slots
    emitter.label("__rt_asis_shape_ready");

    // -- the destination stores string pointer/length pairs whatever it held before --
    emitter.instruction("ldr x10, [x0, #-8]");                                  // load the destination packed array kind word
    emitter.instruction("mov x12, #0x80ff");                                    // preserve the destination kind byte and persistent COW flag
    emitter.instruction("and x10, x10, x12");                                   // drop stale destination value_type bits before restamping
    emitter.instruction("mov x11, #0x100");                                     // runtime value_type tag 1 marks string payload slots
    emitter.instruction("orr x10, x10, x11");                                   // combine the destination kind bits with the string value_type tag
    emitter.instruction("str x10, [x0, #-8]");                                  // persist the string value_type tag

    array_splice_insert::emit_open_string_gap(emitter);

    // -- duplicate the replacement strings into the opened gap --
    emitter.label("__rt_asis_copy");
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload the replacement copy index
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the replacement indexed-array pointer
    emitter.instruction("ldr x9, [x2]");                                        // reload the replacement element count
    emitter.instruction("cmp x4, x9");                                          // has every replacement string been copied?
    emitter.instruction("b.ge __rt_asis_set_len");                              // yes, publish the extended destination length
    emitter.instruction("add x5, x2, #24");                                     // compute the replacement string payload base address
    emitter.instruction("lsl x6, x4, #4");                                      // scale the copy index by the 16-byte string slot size
    emitter.instruction("add x5, x5, x6");                                      // compute this element's replacement string slot address
    emitter.instruction("ldr x1, [x5]");                                        // load the borrowed replacement string pointer
    emitter.instruction("ldr x2, [x5, #8]");                                    // load the borrowed replacement string length
    emitter.instruction("bl __rt_str_persist");                                 // duplicate it so the destination owns its own bytes
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload the replacement copy index after the persist call
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the destination indexed-array pointer
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload the clamped insertion index
    emitter.instruction("add x5, x0, #24");                                     // compute the destination string payload base address
    emitter.instruction("add x6, x3, x4");                                      // compute the destination slot for this replacement element
    emitter.instruction("lsl x6, x6, #4");                                      // scale that slot index by the string slot size
    emitter.instruction("add x6, x5, x6");                                      // compute the destination string slot address
    emitter.instruction("str x1, [x6]");                                        // store the owned string pointer into the opened gap
    emitter.instruction("str x2, [x6, #8]");                                    // store the matching owned string length
    emitter.instruction("add x4, x4, #1");                                      // advance to the next replacement element
    emitter.instruction("str x4, [sp, #24]");                                   // persist the updated replacement copy index
    emitter.instruction("b __rt_asis_copy");                                    // continue copying replacement strings

    // -- publish the extended destination length --
    emitter.label("__rt_asis_set_len");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the destination indexed-array pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the replacement indexed-array pointer
    emitter.instruction("ldr x9, [x2]");                                        // reload the replacement element count
    emitter.instruction("ldr x10, [x0]");                                       // reload the previous destination logical length
    emitter.instruction("add x10, x10, x9");                                    // extend it by the inserted element count
    emitter.instruction("str x10, [x0]");                                       // persist the extended destination logical length

    emitter.label("__rt_asis_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // return the possibly-relocated destination pointer
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the insertion bookkeeping slots
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits the x86_64 System V variant of `__rt_array_splice_insert_str`.
///
/// Mirrors the ARM64 sequence; only the register encoding differs. Note that `__rt_str_persist`
/// takes its source pointer in `rax` (not the first SysV argument register) and returns the owned
/// pointer in `rax` with the length in `rdx`.
fn emit_array_splice_insert_str_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_splice_insert_str ---");
    emitter.label_global("__rt_array_splice_insert_str");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before the insertion spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the insertion bookkeeping
    emitter.instruction("sub rsp, 64");                                         // reserve aligned spill slots for destination, replacement, and indexes
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the destination indexed-array pointer across growth
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the replacement indexed-array pointer across growth
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // preserve the requested insertion index across growth
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // start the replacement copy loop at the first replacement slot
    emitter.instruction("test rdx, rdx");                                       // is the replacement pointer null?
    emitter.instruction("jz __rt_asis_done_x86");                               // a null replacement inserts nothing
    emitter.instruction("mov r9, QWORD PTR [rdx]");                             // r9 = replacement element count
    emitter.instruction("test r9, r9");                                         // is the replacement array empty?
    emitter.instruction("jz __rt_asis_done_x86");                               // an empty replacement inserts nothing

    // -- specialize a still-empty destination to 16-byte string slots before the first write --
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the destination logical length
    emitter.instruction("test r10, r10");                                       // is this the first write into a still-empty destination?
    emitter.instruction("jnz __rt_asis_shape_ready_x86");                       // a non-empty destination already has its string shape fixed
    emitter.instruction("mov r10, QWORD PTR [rdi + 16]");                       // r10 = old elem_size (8 for an empty array<never> buffer)
    emitter.instruction("mov r11, QWORD PTR [rdi + 8]");                        // r11 = old capacity counted in old-elem_size slots
    emitter.instruction("imul r11, r10");                                       // r11 = backing-store data bytes already reserved
    emitter.instruction("shr r11, 4");                                          // reinterpret the same bytes as 16-byte string slots
    emitter.instruction("mov QWORD PTR [rdi + 8], r11");                        // publish slot-accurate capacity before any 16-byte write
    emitter.instruction("mov QWORD PTR [rdi + 16], 16");                        // elem_size = 16 before any future grow copies live string slots
    emitter.label("__rt_asis_shape_ready_x86");

    // -- the destination stores string pointer/length pairs whatever it held before --
    emitter.instruction("mov r10, QWORD PTR [rdi - 8]");                        // load the destination packed array kind word
    emitter.instruction("mov r11, 0xffffffff000080ff");                         // preserve heap marker, indexed-array kind, and persistent COW metadata
    emitter.instruction("and r10, r11");                                        // drop stale destination value_type bits before restamping
    emitter.instruction("or r10, 0x100");                                       // runtime value_type tag 1 marks string payload slots
    emitter.instruction("mov QWORD PTR [rdi - 8], r10");                        // persist the string value_type tag

    array_splice_insert::emit_open_string_gap(emitter);

    // -- duplicate the replacement strings into the opened gap --
    emitter.label("__rt_asis_copy_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the replacement copy index
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the replacement indexed-array pointer
    emitter.instruction("mov r9, QWORD PTR [rdx]");                             // reload the replacement element count
    emitter.instruction("cmp rcx, r9");                                         // has every replacement string been copied?
    emitter.instruction("jge __rt_asis_set_len_x86");                           // yes, publish the extended destination length
    emitter.instruction("lea r10, [rdx + 24]");                                 // compute the replacement string payload base address
    emitter.instruction("mov rax, rcx");                                        // copy the replacement copy index before scaling it
    emitter.instruction("shl rax, 4");                                          // scale it by the 16-byte string slot size
    emitter.instruction("mov r11, QWORD PTR [r10 + rax]");                      // load the borrowed replacement string pointer
    emitter.instruction("mov rdx, QWORD PTR [r10 + rax + 8]");                  // load the borrowed replacement string length
    emitter.instruction("mov rax, r11");                                        // __rt_str_persist reads its source pointer from rax, not rdi
    emitter.instruction("call __rt_str_persist");                               // duplicate it so the destination owns its own bytes
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the replacement copy index after the persist call
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the destination indexed-array pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // reload the clamped insertion index
    emitter.instruction("lea r10, [rdi + 24]");                                 // compute the destination string payload base address
    emitter.instruction("mov r11, rsi");                                        // copy the insertion index before computing the target slot
    emitter.instruction("add r11, rcx");                                        // compute the destination slot for this replacement element
    emitter.instruction("shl r11, 4");                                          // scale that slot index by the string slot size
    emitter.instruction("mov QWORD PTR [r10 + r11], rax");                      // store the owned string pointer into the opened gap
    emitter.instruction("mov QWORD PTR [r10 + r11 + 8], rdx");                  // store the matching owned string length
    emitter.instruction("add rcx, 1");                                          // advance to the next replacement element
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // persist the updated replacement copy index
    emitter.instruction("jmp __rt_asis_copy_x86");                              // continue copying replacement strings

    // -- publish the extended destination length --
    emitter.label("__rt_asis_set_len_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the destination indexed-array pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the replacement indexed-array pointer
    emitter.instruction("mov r9, QWORD PTR [rdx]");                             // reload the replacement element count
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // reload the previous destination logical length
    emitter.instruction("add r10, r9");                                         // extend it by the inserted element count
    emitter.instruction("mov QWORD PTR [rdi], r10");                            // persist the extended destination logical length

    emitter.label("__rt_asis_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the possibly-relocated destination pointer
    emitter.instruction("add rsp, 64");                                         // release the insertion bookkeeping slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller
}
