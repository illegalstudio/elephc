//! Purpose:
//! Owns the growable `stream_wrapper_register()` definition table: the
//! `__rt_user_wrappers_reserve` runtime helper plus the shared accessors every
//! scan site uses to load the table base and its capacity.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - Every `_user_wrappers_ptr` consumer (`fopen`, `path_is_wrapper`,
//!   `stream_wrapper_register`/`unregister`, `stream_get_wrappers`,
//!   `stream_record_meta`, `user_wrapper_dir`, `user_wrapper_path_op`,
//!   `user_wrapper_url_stat`).
//!
//! Key details:
//! - PHP places no limit on registered wrappers, so the table is heap-backed and
//!   doubles on demand rather than exposing a fixed capacity. The previous
//!   64-slot `.comm` array silently refused the 65th `stream_wrapper_register()`.
//! - `_user_wrappers_ptr` is null and `_user_wrappers_cap` is zero until the
//!   first reservation, so every scan must tolerate a null base — a program that
//!   never registers a wrapper must not allocate.
//! - Slot indices are stable across growth (the payload is copied verbatim), so
//!   `_user_wrapper_flags_ptr` is grown in lockstep and stays index-aligned.
//! - Every live value is spilled to the frame across `__rt_heap_alloc`: on
//!   AArch64 x9-x17 are caller-saved, and on x86_64 r14/r15 are callee-saved, so
//!   neither may carry the freshly allocated base across the call.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Slots allocated by the first reservation; growth doubles from here.
pub(crate) const USER_WRAPPER_TABLE_INITIAL_SLOTS: u64 = 8;

/// Bytes per registration slot: `(protocol_ptr, protocol_len, class_ptr, class_len)`.
pub(crate) const USER_WRAPPER_SLOT_SIZE: u64 = 32;

/// `log2` of [`USER_WRAPPER_SLOT_SIZE`], used as the shift for index scaling.
const SLOT_SHIFT: u32 = USER_WRAPPER_SLOT_SIZE.trailing_zeros();

/// Emits a load of the registration table base into `dest`.
///
/// The loaded value is zero when no wrapper has ever been registered; callers
/// must treat that as an empty table rather than dereferencing it.
pub(crate) fn emit_load_table_base(emitter: &mut Emitter, dest: &str) {
    abi::emit_symbol_address(emitter, dest, "_user_wrappers_ptr");
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction(&format!("mov {dest}, QWORD PTR [{dest}]"));        // registration table heap base (0 when unregistered)
    } else {
        emitter.instruction(&format!("ldr {dest}, [{dest}]"));                  // registration table heap base (0 when unregistered)
    }
}

/// Emits a load of the registration-flag table base into `dest`.
pub(crate) fn emit_load_flags_base(emitter: &mut Emitter, dest: &str) {
    abi::emit_symbol_address(emitter, dest, "_user_wrapper_flags_ptr");
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction(&format!("mov {dest}, QWORD PTR [{dest}]"));        // registration flag table heap base
    } else {
        emitter.instruction(&format!("ldr {dest}, [{dest}]"));                  // registration flag table heap base
    }
}

/// Emits a load of the current slot capacity into `dest`.
///
/// Scans bound themselves on this value; it is zero before the first
/// reservation, which makes every scan fall through without touching memory.
pub(crate) fn emit_load_table_cap(emitter: &mut Emitter, dest: &str) {
    abi::emit_symbol_address(emitter, dest, "_user_wrappers_cap");
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction(&format!("mov {dest}, QWORD PTR [{dest}]"));        // allocated registration slot count
    } else {
        emitter.instruction(&format!("ldr {dest}, [{dest}]"));                  // allocated registration slot count
    }
}

/// `__rt_user_wrappers_reserve() -> slot_index`.
///
/// Guarantees at least one free registration slot exists, allocating or doubling
/// the table as needed, and returns the index of a free slot.
///
/// Output: x0 / rax = index of a free slot, or -1 when the heap is exhausted.
pub fn emit_user_wrappers_reserve(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_user_wrappers_reserve_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: user_wrappers_reserve ---");
    emitter.label_global("__rt_user_wrappers_reserve");
    // Frame: [0]=old base [8]=old cap [16]=new cap [24]=new reg base
    //        [32]=new flag base [40]=pad [48]=x29 [56]=x30
    emitter.instruction("sub sp, sp, #64");                                     // reserve the growth frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer

    // -- scan the existing table for a free slot --
    emit_load_table_base(emitter, "x9");
    emit_load_table_cap(emitter, "x10");
    emitter.instruction("cbz x9, __rt_uwr_grow");                               // never reserved: allocate the initial table
    emitter.instruction("mov x11, #0");                                         // slot index
    emitter.label("__rt_uwr_scan");
    emitter.instruction("cmp x11, x10");                                        // exhausted the allocated slots?
    emitter.instruction("b.ge __rt_uwr_grow");                                  // table full: double it
    emitter.instruction(&format!("add x12, x9, x11, lsl #{SLOT_SHIFT}"));       // slot base = table + index * 32
    emitter.instruction("ldr x13, [x12]");                                      // load the slot's protocol pointer
    emitter.instruction("cbz x13, __rt_uwr_hit");                               // null marks a free slot
    emitter.instruction("add x11, x11, #1");                                    // advance the slot index
    emitter.instruction("b __rt_uwr_scan");                                     // keep scanning

    emitter.label("__rt_uwr_hit");
    emitter.instruction("mov x0, x11");                                         // return the free slot index
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the growth frame
    emitter.instruction("ret");                                                 // return to the caller

    // -- allocate or double the table, preserving slot indices --
    emitter.label("__rt_uwr_grow");
    emitter.instruction("str x9, [sp, #0]");                                    // save the old base
    emitter.instruction("str x10, [sp, #8]");                                   // save the old capacity
    emitter.instruction("lsl x11, x10, #1");                                    // doubled capacity
    emitter.instruction(&format!("mov x12, #{USER_WRAPPER_TABLE_INITIAL_SLOTS}"));
    emitter.instruction("cmp x11, x12");                                        // below the initial floor?
    emitter.instruction("csel x11, x12, x11, lt");                              // clamp the first allocation to the initial slot count
    emitter.instruction("str x11, [sp, #16]");                                  // save the new capacity

    // -- allocate and zero the registration payload --
    emitter.instruction(&format!("lsl x0, x11, #{SLOT_SHIFT}"));                // new_cap * 32 bytes
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = new registration table
    emitter.instruction("cbz x0, __rt_uwr_fail");                               // heap exhausted
    emitter.instruction("str x0, [sp, #24]");                                   // spill the new registration base across later calls
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the new capacity
    emitter.instruction(&format!("lsl x12, x11, #{SLOT_SHIFT}"));               // total bytes
    emitter.instruction("mov x13, #0");                                         // byte cursor
    emitter.label("__rt_uwr_zero");
    emitter.instruction("cmp x13, x12");                                        // cleared every byte?
    emitter.instruction("b.ge __rt_uwr_zeroed");
    emitter.instruction("str xzr, [x0, x13]");                                  // clear one word
    emitter.instruction("add x13, x13, #8");                                    // advance the cursor
    emitter.instruction("b __rt_uwr_zero");
    emitter.label("__rt_uwr_zeroed");

    // -- copy the old registrations over, keeping slot indices stable --
    emitter.instruction("ldr x9, [sp, #0]");                                    // old base
    emitter.instruction("ldr x10, [sp, #8]");                                   // old capacity
    emitter.instruction("cbz x9, __rt_uwr_copied");                             // nothing to migrate on first allocation
    emitter.instruction(&format!("lsl x12, x10, #{SLOT_SHIFT}"));               // old byte count
    emitter.instruction("mov x13, #0");                                         // byte cursor
    emitter.label("__rt_uwr_copy");
    emitter.instruction("cmp x13, x12");                                        // copied every old byte?
    emitter.instruction("b.ge __rt_uwr_copied");
    emitter.instruction("ldr x14, [x9, x13]");                                  // load one old word
    emitter.instruction("str x14, [x0, x13]");                                  // store it at the identical offset
    emitter.instruction("add x13, x13, #8");                                    // advance the cursor
    emitter.instruction("b __rt_uwr_copy");
    emitter.label("__rt_uwr_copied");

    // -- allocate and zero the parallel flag table --
    emitter.instruction("ldr x11, [sp, #16]");                                  // new capacity
    emitter.instruction("lsl x0, x11, #3");                                     // new_cap * 8 bytes
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = new flag table
    emitter.instruction("cbz x0, __rt_uwr_fail");                               // heap exhausted
    emitter.instruction("str x0, [sp, #32]");                                   // spill the new flag base
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the new capacity
    emitter.instruction("lsl x12, x11, #3");                                    // total flag bytes
    emitter.instruction("mov x13, #0");                                         // byte cursor
    emitter.label("__rt_uwr_fzero");
    emitter.instruction("cmp x13, x12");                                        // cleared every flag byte?
    emitter.instruction("b.ge __rt_uwr_fzeroed");
    emitter.instruction("str xzr, [x0, x13]");                                  // clear one flag word
    emitter.instruction("add x13, x13, #8");                                    // advance the cursor
    emitter.instruction("b __rt_uwr_fzero");
    emitter.label("__rt_uwr_fzeroed");

    // -- migrate the old flags at identical indices --
    emit_load_flags_base(emitter, "x9");
    emitter.instruction("cbz x9, __rt_uwr_fcopied");                            // nothing to migrate on first allocation
    emitter.instruction("ldr x10, [sp, #8]");                                   // old capacity
    emitter.instruction("lsl x12, x10, #3");                                    // old flag byte count
    emitter.instruction("mov x13, #0");                                         // byte cursor
    emitter.label("__rt_uwr_fcopy");
    emitter.instruction("cmp x13, x12");                                        // copied every old flag byte?
    emitter.instruction("b.ge __rt_uwr_fcopied");
    emitter.instruction("ldr x14, [x9, x13]");                                  // load one old flag word
    emitter.instruction("str x14, [x0, x13]");                                  // store it at the identical offset
    emitter.instruction("add x13, x13, #8");                                    // advance the cursor
    emitter.instruction("b __rt_uwr_fcopy");
    emitter.label("__rt_uwr_fcopied");

    // -- publish the new tables, then return the first slot of the grown region --
    emitter.instruction("ldr x14, [sp, #24]");                                  // new registration base
    abi::emit_symbol_address(emitter, "x9", "_user_wrappers_ptr");
    emitter.instruction("str x14, [x9]");                                       // publish the registration base
    emitter.instruction("ldr x14, [sp, #32]");                                  // new flag base
    abi::emit_symbol_address(emitter, "x9", "_user_wrapper_flags_ptr");
    emitter.instruction("str x14, [x9]");                                       // publish the flag base
    emitter.instruction("ldr x11, [sp, #16]");                                  // new capacity
    abi::emit_symbol_address(emitter, "x9", "_user_wrappers_cap");
    emitter.instruction("str x11, [x9]");                                       // publish the new capacity
    emitter.instruction("ldr x0, [sp, #8]");                                    // old capacity = index of the first fresh slot
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the growth frame
    emitter.instruction("ret");                                                 // return to the caller

    emitter.label("__rt_uwr_fail");
    emitter.instruction("mov x0, #-1");                                         // report heap exhaustion
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the growth frame
    emitter.instruction("ret");                                                 // return to the caller
}

/// x86_64 Linux variant of `__rt_user_wrappers_reserve`.
fn emit_user_wrappers_reserve_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_wrappers_reserve ---");
    emitter.label_global("__rt_user_wrappers_reserve");
    // Frame: rbp-relative, [-8]=old base [-16]=old cap [-24]=new cap
    //        [-32]=new reg base [-40]=new flag base
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // reserve the growth frame (keeps rsp 16-byte aligned)

    // -- scan the existing table for a free slot --
    emit_load_table_base(emitter, "r8");
    emit_load_table_cap(emitter, "r9");
    emitter.instruction("test r8, r8");                                         // never reserved?
    emitter.instruction("jz __rt_uwr_grow_x");                                  // allocate the initial table
    emitter.instruction("xor r10, r10");                                        // slot index
    emitter.label("__rt_uwr_scan_x");
    emitter.instruction("cmp r10, r9");                                         // exhausted the allocated slots?
    emitter.instruction("jge __rt_uwr_grow_x");                                 // table full: double it
    emitter.instruction("mov r11, r10");                                        // copy the slot index for scaling
    emitter.instruction("shl r11, 5");                                          // slot offset = index * 32
    emitter.instruction("add r11, r8");                                         // slot base = table + offset
    emitter.instruction("mov rax, QWORD PTR [r11]");                            // load the slot's protocol pointer
    emitter.instruction("test rax, rax");                                       // null marks a free slot
    emitter.instruction("jz __rt_uwr_hit_x");
    emitter.instruction("inc r10");                                             // advance the slot index
    emitter.instruction("jmp __rt_uwr_scan_x");                                 // keep scanning

    emitter.label("__rt_uwr_hit_x");
    emitter.instruction("mov rax, r10");                                        // return the free slot index
    emitter.instruction("leave");                                               // restore rbp + rsp
    emitter.instruction("ret");                                                 // return to the caller

    // -- allocate or double the table, preserving slot indices --
    emitter.label("__rt_uwr_grow_x");
    emitter.instruction("mov QWORD PTR [rbp - 8], r8");                         // save the old base
    emitter.instruction("mov QWORD PTR [rbp - 16], r9");                        // save the old capacity
    emitter.instruction("mov r10, r9");                                         // copy the old capacity
    emitter.instruction("shl r10, 1");                                          // doubled capacity
    emitter.instruction(&format!("mov r11, {USER_WRAPPER_TABLE_INITIAL_SLOTS}"));
    emitter.instruction("cmp r10, r11");                                        // below the initial floor?
    emitter.instruction("cmovl r10, r11");                                      // clamp the first allocation to the initial slot count
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the new capacity

    // -- allocate and zero the registration payload --
    emitter.instruction("mov rax, r10");                                        // new capacity
    emitter.instruction("shl rax, 5");                                          // new_cap * 32 bytes
    emitter.instruction("call __rt_heap_alloc");                                // rax = new registration table
    emitter.instruction("test rax, rax");                                       // heap exhausted?
    emitter.instruction("jz __rt_uwr_fail_x");
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // spill the new registration base across later calls
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the new capacity
    emitter.instruction("shl rcx, 5");                                          // total bytes
    emitter.instruction("xor rdx, rdx");                                        // byte cursor
    emitter.label("__rt_uwr_zero_x");
    emitter.instruction("cmp rdx, rcx");                                        // cleared every byte?
    emitter.instruction("jge __rt_uwr_zeroed_x");
    emitter.instruction("mov QWORD PTR [rax + rdx], 0");                        // clear one word
    emitter.instruction("add rdx, 8");                                          // advance the cursor
    emitter.instruction("jmp __rt_uwr_zero_x");
    emitter.label("__rt_uwr_zeroed_x");

    // -- copy the old registrations over, keeping slot indices stable --
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // old base
    emitter.instruction("test r8, r8");                                         // nothing to migrate on first allocation?
    emitter.instruction("jz __rt_uwr_copied_x");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // old capacity
    emitter.instruction("shl rcx, 5");                                          // old byte count
    emitter.instruction("xor rdx, rdx");                                        // byte cursor
    emitter.label("__rt_uwr_copy_x");
    emitter.instruction("cmp rdx, rcx");                                        // copied every old byte?
    emitter.instruction("jge __rt_uwr_copied_x");
    emitter.instruction("mov rsi, QWORD PTR [r8 + rdx]");                       // load one old word
    emitter.instruction("mov QWORD PTR [rax + rdx], rsi");                      // store it at the identical offset
    emitter.instruction("add rdx, 8");                                          // advance the cursor
    emitter.instruction("jmp __rt_uwr_copy_x");
    emitter.label("__rt_uwr_copied_x");

    // -- allocate and zero the parallel flag table --
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // new capacity
    emitter.instruction("shl rax, 3");                                          // new_cap * 8 bytes
    emitter.instruction("call __rt_heap_alloc");                                // rax = new flag table
    emitter.instruction("test rax, rax");                                       // heap exhausted?
    emitter.instruction("jz __rt_uwr_fail_x");
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // spill the new flag base
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the new capacity
    emitter.instruction("shl rcx, 3");                                          // total flag bytes
    emitter.instruction("xor rdx, rdx");                                        // byte cursor
    emitter.label("__rt_uwr_fzero_x");
    emitter.instruction("cmp rdx, rcx");                                        // cleared every flag byte?
    emitter.instruction("jge __rt_uwr_fzeroed_x");
    emitter.instruction("mov QWORD PTR [rax + rdx], 0");                        // clear one flag word
    emitter.instruction("add rdx, 8");                                          // advance the cursor
    emitter.instruction("jmp __rt_uwr_fzero_x");
    emitter.label("__rt_uwr_fzeroed_x");

    // -- migrate the old flags at identical indices --
    emit_load_flags_base(emitter, "r8");
    emitter.instruction("test r8, r8");                                         // nothing to migrate on first allocation?
    emitter.instruction("jz __rt_uwr_fcopied_x");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // old capacity
    emitter.instruction("shl rcx, 3");                                          // old flag byte count
    emitter.instruction("xor rdx, rdx");                                        // byte cursor
    emitter.label("__rt_uwr_fcopy_x");
    emitter.instruction("cmp rdx, rcx");                                        // copied every old flag byte?
    emitter.instruction("jge __rt_uwr_fcopied_x");
    emitter.instruction("mov rsi, QWORD PTR [r8 + rdx]");                       // load one old flag word
    emitter.instruction("mov QWORD PTR [rax + rdx], rsi");                      // store it at the identical offset
    emitter.instruction("add rdx, 8");                                          // advance the cursor
    emitter.instruction("jmp __rt_uwr_fcopy_x");
    emitter.label("__rt_uwr_fcopied_x");

    // -- publish the new tables, then return the first slot of the grown region --
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // new registration base
    abi::emit_symbol_address(emitter, "r8", "_user_wrappers_ptr");
    emitter.instruction("mov QWORD PTR [r8], rcx");                             // publish the registration base
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // new flag base
    abi::emit_symbol_address(emitter, "r8", "_user_wrapper_flags_ptr");
    emitter.instruction("mov QWORD PTR [r8], rcx");                             // publish the flag base
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // new capacity
    abi::emit_symbol_address(emitter, "r8", "_user_wrappers_cap");
    emitter.instruction("mov QWORD PTR [r8], rcx");                             // publish the new capacity
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // old capacity = index of the first fresh slot
    emitter.instruction("leave");                                               // restore rbp + rsp
    emitter.instruction("ret");                                                 // return the free slot index in rax

    emitter.label("__rt_uwr_fail_x");
    emitter.instruction("mov rax, -1");                                         // report heap exhaustion
    emitter.instruction("leave");                                               // restore rbp + rsp
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits a load of the active stream-handle table base into `dest`.
///
/// The loaded value is zero until the first wrapper stream is opened; callers
/// that only read must treat that as an empty table.
pub(crate) fn emit_load_handles_base(emitter: &mut Emitter, dest: &str) {
    abi::emit_symbol_address(emitter, dest, "_user_wrapper_handles_ptr");
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction(&format!("mov {dest}, QWORD PTR [{dest}]"));        // stream-handle table heap base (0 before the first open)
    } else {
        emitter.instruction(&format!("ldr {dest}, [{dest}]"));                  // stream-handle table heap base (0 before the first open)
    }
}

/// `__rt_user_wrapper_handles_reserve() -> slot_index`.
///
/// Guarantees at least one free stream-handle slot exists, allocating or
/// doubling the table as needed, and returns the index of a free slot.
///
/// Output: x0 / rax = index of a free slot, or -1 when the heap is exhausted.
pub fn emit_user_wrapper_handles_reserve(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_user_wrapper_handles_reserve_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_handles_reserve ---");
    emitter.label_global("__rt_user_wrapper_handles_reserve");
    // Frame: [0]=old base [8]=old cap [16]=new cap [24]=new base [32]=pad
    //        [40]=pad [48]=x29 [56]=x30
    emitter.instruction("sub sp, sp, #64");                                     // reserve the growth frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer

    // -- scan the existing table for a free slot --
    emit_load_handles_base(emitter, "x9");
    emit_load_handles_cap(emitter, "x10");
    emitter.instruction("cbz x9, __rt_uwhr_grow");                              // never reserved: allocate the initial table
    emitter.instruction("mov x11, #0");                                         // slot index
    emitter.label("__rt_uwhr_scan");
    emitter.instruction("cmp x11, x10");                                        // exhausted the allocated slots?
    emitter.instruction("b.ge __rt_uwhr_grow");                                 // table full: double it
    emitter.instruction("ldr x12, [x9, x11, lsl #3]");                          // load the slot — null means free
    emitter.instruction("cbz x12, __rt_uwhr_hit");                              // free slot found
    emitter.instruction("add x11, x11, #1");                                    // advance the slot index
    emitter.instruction("b __rt_uwhr_scan");                                    // keep scanning

    emitter.label("__rt_uwhr_hit");
    emitter.instruction("mov x0, x11");                                         // return the free slot index
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the growth frame
    emitter.instruction("ret");                                                 // return to the caller

    // -- allocate or double the table, preserving slot indices --
    emitter.label("__rt_uwhr_grow");
    emitter.instruction("str x9, [sp, #0]");                                    // save the old base
    emitter.instruction("str x10, [sp, #8]");                                   // save the old capacity
    emitter.instruction("lsl x11, x10, #1");                                    // doubled capacity
    emitter.instruction("mov x12, #16");                                        // initial floor for the handle table
    emitter.instruction("cmp x11, x12");                                        // below the initial floor?
    emitter.instruction("csel x11, x12, x11, lt");                              // clamp the first allocation
    emitter.instruction("str x11, [sp, #16]");                                  // save the new capacity

    // -- allocate and zero the new handle payload --
    emitter.instruction("lsl x0, x11, #3");                                     // new_cap * 8 bytes
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = new handle table
    emitter.instruction("cbz x0, __rt_uwhr_fail");                              // heap exhausted
    emitter.instruction("str x0, [sp, #24]");                                   // spill the new base across the copy
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the new capacity
    emitter.instruction("lsl x12, x11, #3");                                    // total bytes
    emitter.instruction("mov x13, #0");                                         // byte cursor
    emitter.label("__rt_uwhr_zero");
    emitter.instruction("cmp x13, x12");                                        // cleared every byte?
    emitter.instruction("b.ge __rt_uwhr_zeroed");
    emitter.instruction("str xzr, [x0, x13]");                                  // clear one slot
    emitter.instruction("add x13, x13, #8");                                    // advance the cursor
    emitter.instruction("b __rt_uwhr_zero");
    emitter.label("__rt_uwhr_zeroed");

    // -- copy the old handles over, keeping slot indices stable --
    // Slot indices are baked into live synthetic fds, so they must not shift.
    emitter.instruction("ldr x9, [sp, #0]");                                    // old base
    emitter.instruction("cbz x9, __rt_uwhr_copied");                            // nothing to migrate on first allocation
    emitter.instruction("ldr x10, [sp, #8]");                                   // old capacity
    emitter.instruction("lsl x12, x10, #3");                                    // old byte count
    emitter.instruction("mov x13, #0");                                         // byte cursor
    emitter.label("__rt_uwhr_copy");
    emitter.instruction("cmp x13, x12");                                        // copied every old byte?
    emitter.instruction("b.ge __rt_uwhr_copied");
    emitter.instruction("ldr x14, [x9, x13]");                                  // load one old slot
    emitter.instruction("str x14, [x0, x13]");                                  // store it at the identical offset
    emitter.instruction("add x13, x13, #8");                                    // advance the cursor
    emitter.instruction("b __rt_uwhr_copy");
    emitter.label("__rt_uwhr_copied");

    // -- publish the new table, then return the first slot of the grown region --
    abi::emit_symbol_address(emitter, "x9", "_user_wrapper_handles_ptr");
    emitter.instruction("str x0, [x9]");                                        // publish the handle base
    emitter.instruction("ldr x11, [sp, #16]");                                  // new capacity
    abi::emit_symbol_address(emitter, "x9", "_user_wrapper_handles_cap");
    emitter.instruction("str x11, [x9]");                                       // publish the new capacity
    emitter.instruction("ldr x0, [sp, #8]");                                    // old capacity = index of the first fresh slot
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the growth frame
    emitter.instruction("ret");                                                 // return to the caller

    emitter.label("__rt_uwhr_fail");
    emitter.instruction("mov x0, #-1");                                         // report heap exhaustion
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the growth frame
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits a load of the current handle-slot capacity into `dest`.
pub(crate) fn emit_load_handles_cap(emitter: &mut Emitter, dest: &str) {
    abi::emit_symbol_address(emitter, dest, "_user_wrapper_handles_cap");
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction(&format!("mov {dest}, QWORD PTR [{dest}]"));        // allocated handle slot count
    } else {
        emitter.instruction(&format!("ldr {dest}, [{dest}]"));                  // allocated handle slot count
    }
}

/// x86_64 Linux variant of `__rt_user_wrapper_handles_reserve`.
fn emit_user_wrapper_handles_reserve_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_handles_reserve ---");
    emitter.label_global("__rt_user_wrapper_handles_reserve");
    // Frame: rbp-relative, [-8]=old base [-16]=old cap [-24]=new cap [-32]=new base
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve the growth frame (keeps rsp 16-byte aligned)

    // -- scan the existing table for a free slot --
    emit_load_handles_base(emitter, "r8");
    emit_load_handles_cap(emitter, "r9");
    emitter.instruction("test r8, r8");                                         // never reserved?
    emitter.instruction("jz __rt_uwhr_grow_x");                                 // allocate the initial table
    emitter.instruction("xor r10, r10");                                        // slot index
    emitter.label("__rt_uwhr_scan_x");
    emitter.instruction("cmp r10, r9");                                         // exhausted the allocated slots?
    emitter.instruction("jge __rt_uwhr_grow_x");                                // table full: double it
    emitter.instruction("mov r11, QWORD PTR [r8 + r10 * 8]");                   // load the slot — null means free
    emitter.instruction("test r11, r11");                                       // is this slot free?
    emitter.instruction("jz __rt_uwhr_hit_x");                                  // free slot found
    emitter.instruction("inc r10");                                             // advance the slot index
    emitter.instruction("jmp __rt_uwhr_scan_x");                                // keep scanning

    emitter.label("__rt_uwhr_hit_x");
    emitter.instruction("mov rax, r10");                                        // return the free slot index
    emitter.instruction("leave");                                               // restore rbp + rsp
    emitter.instruction("ret");                                                 // return to the caller

    // -- allocate or double the table, preserving slot indices --
    emitter.label("__rt_uwhr_grow_x");
    emitter.instruction("mov QWORD PTR [rbp - 8], r8");                         // save the old base
    emitter.instruction("mov QWORD PTR [rbp - 16], r9");                        // save the old capacity
    emitter.instruction("mov r10, r9");                                         // copy the old capacity
    emitter.instruction("shl r10, 1");                                          // doubled capacity
    emitter.instruction("mov r11, 16");                                         // initial floor for the handle table
    emitter.instruction("cmp r10, r11");                                        // below the initial floor?
    emitter.instruction("cmovl r10, r11");                                      // clamp the first allocation
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the new capacity

    // -- allocate and zero the new handle payload --
    emitter.instruction("mov rax, r10");                                        // new capacity
    emitter.instruction("shl rax, 3");                                          // new_cap * 8 bytes
    emitter.instruction("call __rt_heap_alloc");                                // rax = new handle table
    emitter.instruction("test rax, rax");                                       // heap exhausted?
    emitter.instruction("jz __rt_uwhr_fail_x");
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // spill the new base across the copy
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the new capacity
    emitter.instruction("shl rcx, 3");                                          // total bytes
    emitter.instruction("xor rdx, rdx");                                        // byte cursor
    emitter.label("__rt_uwhr_zero_x");
    emitter.instruction("cmp rdx, rcx");                                        // cleared every byte?
    emitter.instruction("jge __rt_uwhr_zeroed_x");
    emitter.instruction("mov QWORD PTR [rax + rdx], 0");                        // clear one slot
    emitter.instruction("add rdx, 8");                                          // advance the cursor
    emitter.instruction("jmp __rt_uwhr_zero_x");
    emitter.label("__rt_uwhr_zeroed_x");

    // -- copy the old handles over, keeping slot indices stable --
    // Slot indices are baked into live synthetic fds, so they must not shift.
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // old base
    emitter.instruction("test r8, r8");                                         // nothing to migrate on first allocation?
    emitter.instruction("jz __rt_uwhr_copied_x");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // old capacity
    emitter.instruction("shl rcx, 3");                                          // old byte count
    emitter.instruction("xor rdx, rdx");                                        // byte cursor
    emitter.label("__rt_uwhr_copy_x");
    emitter.instruction("cmp rdx, rcx");                                        // copied every old byte?
    emitter.instruction("jge __rt_uwhr_copied_x");
    emitter.instruction("mov rsi, QWORD PTR [r8 + rdx]");                       // load one old slot
    emitter.instruction("mov QWORD PTR [rax + rdx], rsi");                      // store it at the identical offset
    emitter.instruction("add rdx, 8");                                          // advance the cursor
    emitter.instruction("jmp __rt_uwhr_copy_x");
    emitter.label("__rt_uwhr_copied_x");

    // -- publish the new table, then return the first slot of the grown region --
    abi::emit_symbol_address(emitter, "r8", "_user_wrapper_handles_ptr");
    emitter.instruction("mov QWORD PTR [r8], rax");                             // publish the handle base
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // new capacity
    abi::emit_symbol_address(emitter, "r8", "_user_wrapper_handles_cap");
    emitter.instruction("mov QWORD PTR [r8], rcx");                             // publish the new capacity
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // old capacity = index of the first fresh slot
    emitter.instruction("leave");                                               // restore rbp + rsp
    emitter.instruction("ret");                                                 // return the free slot index in rax

    emitter.label("__rt_uwhr_fail_x");
    emitter.instruction("mov rax, -1");                                         // report heap exhaustion
    emitter.instruction("leave");                                               // restore rbp + rsp
    emitter.instruction("ret");                                                 // return to the caller
}
