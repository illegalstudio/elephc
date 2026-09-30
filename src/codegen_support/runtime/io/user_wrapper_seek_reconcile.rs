//! Purpose:
//! Emits `__rt_user_wrapper_seek_reconcile`, php's position reconciliation after a userspace
//! wrapper's `stream_seek` reported success.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - The `fseek()` and `rewind()` wrapper arms, once the wrapper's own seek has succeeded.
//!
//! Key details:
//! - php keeps the position for a userspace stream ITSELF, and `main/streams/userspace.c` calls
//!   `stream_tell` in exactly one place: right after a successful `stream_seek`, to find out where
//!   the wrapper actually landed. The requested offset is NOT used — a wrapper is free to land
//!   somewhere else and say so.
//! - A wrapper with NO `stream_tell` therefore cannot seek at all. php warns
//!   `<Class>::stream_tell is not implemented!` and REFUSES the seek: `fseek()` answers -1,
//!   `rewind()` answers false, and the position stays where it was. MEASURED on `php -n` 8.5.6
//!   with a wrapper whose `stream_seek` returns true and which defines no `stream_tell`.
//! - The caller supplies the head of the message, because php names the function the user called
//!   (`fseek()` or `rewind()`) and not the method it reached.

use crate::codegen_support::runtime::data::WRAPPER_MISSING_HOOK_TAIL_TELL;
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// The `stream_tell` vtable slot, mirroring `__rt_user_wrapper_ftell`.
const VTABLE_SLOT_TELL: usize = 5;

/// The `stream_seek` vtable slot, mirroring `__rt_user_wrapper_fseek`.
const VTABLE_SLOT_SEEK: usize = 6;

/// Emits `__rt_user_wrapper_seek_reconcile(handle, fd, head_ptr, head_len) -> 0 | -1`.
///
/// AArch64 takes `x0`/`x1`/`x2`/`x3`; x86_64 takes `rdi`/`rsi`/`rdx`/`rcx`. Answers 0 when the
/// position was reconciled and -1 when php refuses the seek, having warned.
pub fn emit_user_wrapper_seek_reconcile(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// The AArch64 reconciliation.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: reconcile a wrapper's position after its stream_seek ---");
    emitter.label_global("__rt_user_wrapper_seek_reconcile");
    // Frame: [0] stream handle, [8] wrapper fd, [16] head ptr, [24] head len, [32] linkage.
    emitter.instruction("sub sp, sp, #48");
    emitter.instruction("stp x29, x30, [sp, #32]");
    emitter.instruction("add x29, sp, #32");
    emitter.instruction("stp x0, x1, [sp, #0]");                                // the handle and the synthetic fd
    emitter.instruction("stp x2, x3, [sp, #16]");                               // the caller's name for the warning

    // -- resolve the wrapper object behind the synthetic descriptor --
    emitter.instruction("mov w9, #0x4000");                                     // high half of the synthetic fd base
    emitter.instruction("lsl x9, x9, #16");                                     // form 0x40000000
    emitter.instruction("sub x9, x1, x9");                                      // slot index = fd - base
    super::emit_load_handles_cap(emitter, "x10");
    emitter.instruction("cmp x9, x10");
    emitter.instruction("b.hs __rt_uwsr_refuse");                               // out of range: no object, no seek
    super::emit_load_handles_base(emitter, "x10");
    emitter.instruction("ldr x10, [x10, x9, lsl #3]");                          // obj = _user_wrapper_handles[slot]
    emitter.instruction("cbz x10, __rt_uwsr_refuse");                           // already closed: likewise

    // -- php names the method only when the class does not define it --
    emitter.instruction("ldr x11, [x10]");                                      // class id, at the head of every wrapper object
    abi::emit_symbol_address(emitter, "x12", "_user_wrapper_vtable_ptrs");
    emitter.instruction("ldr x12, [x12, x11, lsl #3]");                         // this class's wrapper vtable
    emitter.instruction("cbz x12, __rt_uwsr_missing");                          // no vtable at all reads as no method
    emitter.instruction(&format!("ldr x12, [x12, #{}]", VTABLE_SLOT_TELL * 8)); // the stream_tell slot
    emitter.instruction("cbnz x12, __rt_uwsr_ask");                             // it exists: ask it where we landed

    emitter.label("__rt_uwsr_missing");
    emitter.instruction("ldr x0, [x10]");                                       // the class id the composer names
    emitter.instruction("ldp x1, x2, [sp, #16]");                               // the caller's head fragment
    abi::emit_symbol_address(emitter, "x3", "_uwmh_tail_tell");
    emitter.instruction(&format!("mov x4, #{}", WRAPPER_MISSING_HOOK_TAIL_TELL.len()));
    emitter.instruction("bl __rt_wrapper_missing_hook_warning");
    emitter.instruction("b __rt_uwsr_refuse");                                  // php refuses the seek it cannot reconcile

    emitter.label("__rt_uwsr_ask");
    emitter.instruction("ldr x0, [sp, #8]");                                    // the synthetic wrapper fd
    emitter.instruction("bl __rt_user_wrapper_ftell");                          // where the wrapper says it landed
    emitter.instruction("cmp x0, #0");
    emitter.instruction("b.lt __rt_uwsr_refuse");                               // it answered a failure of its own
    emitter.instruction("mov x1, x0");                                          // the reconciled position
    emitter.instruction("ldr x0, [sp, #0]");                                    // the opaque stream handle
    emitter.instruction("bl __rt_stream_wrapper_pos_set");
    emitter.instruction("mov x0, #0");                                          // the seek succeeded
    emitter.instruction("b __rt_uwsr_ret");

    emitter.label("__rt_uwsr_refuse");
    emitter.instruction("mov x0, #-1");                                         // php's seek failure, position untouched
    emitter.label("__rt_uwsr_ret");
    emitter.instruction("ldp x29, x30, [sp, #32]");
    emitter.instruction("add sp, sp, #48");
    emitter.instruction("ret");
}

/// The x86_64 reconciliation, written from the AArch64 one line for line.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: reconcile a wrapper's position after its stream_seek ---");
    emitter.label_global("__rt_user_wrapper_seek_reconcile");
    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 48");                                         // keeps rsp aligned for the calls below
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // the stream handle
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // the synthetic fd
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // the caller's head pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // and its length

    emitter.instruction("mov r9, rsi");
    emitter.instruction("sub r9, 0x40000000");                                  // slot index = fd - base
    super::emit_load_handles_cap(emitter, "r10");
    emitter.instruction("cmp r9, r10");
    emitter.instruction("jae __rt_uwsr_refuse_x86");                            // out of range: no object, no seek
    super::emit_load_handles_base(emitter, "r10");
    emitter.instruction("mov r10, QWORD PTR [r10 + r9*8]");                     // obj = _user_wrapper_handles[slot]
    emitter.instruction("test r10, r10");
    emitter.instruction("jz __rt_uwsr_refuse_x86");                             // already closed: likewise
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // hold the object across the vtable probe

    emitter.instruction("mov r11, QWORD PTR [r10]");                            // class id, at the head of every wrapper object
    abi::emit_symbol_address(emitter, "rax", "_user_wrapper_vtable_ptrs");
    emitter.instruction("mov rax, QWORD PTR [rax + r11*8]");                    // this class's wrapper vtable
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_uwsr_missing_x86");                            // no vtable at all reads as no method
    emitter.instruction(&format!("mov rax, QWORD PTR [rax + {}]", VTABLE_SLOT_TELL * 8));
    emitter.instruction("test rax, rax");
    emitter.instruction("jnz __rt_uwsr_ask_x86");                               // it exists: ask it where we landed

    emitter.label("__rt_uwsr_missing_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");
    emitter.instruction("mov rdi, QWORD PTR [r10]");                            // the class id the composer names
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // the caller's head fragment
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");
    abi::emit_symbol_address(emitter, "rcx", "_uwmh_tail_tell");
    emitter.instruction(&format!("mov r8, {}", WRAPPER_MISSING_HOOK_TAIL_TELL.len()));
    emitter.instruction("call __rt_wrapper_missing_hook_warning");
    emitter.instruction("jmp __rt_uwsr_refuse_x86");                            // php refuses the seek it cannot reconcile

    emitter.label("__rt_uwsr_ask_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // the synthetic wrapper fd
    emitter.instruction("call __rt_user_wrapper_ftell");                        // where the wrapper says it landed
    emitter.instruction("cmp rax, 0");
    emitter.instruction("jl __rt_uwsr_refuse_x86");                             // it answered a failure of its own
    emitter.instruction("mov rsi, rax");                                        // the reconciled position
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the opaque stream handle
    emitter.instruction("call __rt_stream_wrapper_pos_set");
    emitter.instruction("xor eax, eax");                                        // the seek succeeded
    emitter.instruction("jmp __rt_uwsr_ret_x86");

    emitter.label("__rt_uwsr_refuse_x86");
    emitter.instruction("mov rax, -1");                                         // php's seek failure, position untouched
    emitter.label("__rt_uwsr_ret_x86");
    emitter.instruction("mov rsp, rbp");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");
}

/// Emits `__rt_user_wrapper_lacks_seek(fd) -> 1` when the wrapper's class defines no `stream_seek`.
///
/// php separates "this stream has no seek OP" from "the seek failed", and says both when both are
/// true — MEASURED with two wrappers differing only in whether the method exists. Anything that is
/// not a userspace wrapper answers 0: its seek came from somewhere else and failed on its own
/// terms.
pub fn emit_user_wrapper_lacks_seek(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: does this wrapper's class define stream_seek at all ---");
    emitter.label_global("__rt_user_wrapper_lacks_seek");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov w9, #0x4000");                             // high half of the synthetic fd base
            emitter.instruction("lsl x9, x9, #16");                             // form 0x40000000
            emitter.instruction("subs x9, x0, x9");                             // slot index = fd - base
            emitter.instruction("b.lt __rt_uwls_no");                           // a native descriptor is not a wrapper
            super::emit_load_handles_cap(emitter, "x10");
            emitter.instruction("cmp x9, x10");
            emitter.instruction("b.hs __rt_uwls_no");                           // out of range: not a wrapper either
            super::emit_load_handles_base(emitter, "x10");
            emitter.instruction("ldr x10, [x10, x9, lsl #3]");                  // obj = _user_wrapper_handles[slot]
            emitter.instruction("cbz x10, __rt_uwls_no");                       // already closed
            emitter.instruction("ldr x11, [x10]");                              // class id
            abi::emit_symbol_address(emitter, "x12", "_user_wrapper_vtable_ptrs");
            emitter.instruction("ldr x12, [x12, x11, lsl #3]");                 // this class's wrapper vtable
            emitter.instruction("cbz x12, __rt_uwls_yes");                      // no vtable reads as no method
            emitter.instruction(&format!("ldr x12, [x12, #{}]", VTABLE_SLOT_SEEK * 8));
            emitter.instruction("cbnz x12, __rt_uwls_no");                      // the method exists
            emitter.label("__rt_uwls_yes");
            emitter.instruction("mov x0, #1");
            emitter.instruction("ret");
            emitter.label("__rt_uwls_no");
            emitter.instruction("mov x0, #0");
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            emitter.instruction("mov r9, rdi");
            emitter.instruction("sub r9, 0x40000000");                          // slot index = fd - base
            emitter.instruction("js __rt_uwls_no_x86");                         // a native descriptor is not a wrapper
            super::emit_load_handles_cap(emitter, "r10");
            emitter.instruction("cmp r9, r10");
            emitter.instruction("jae __rt_uwls_no_x86");                        // out of range: not a wrapper either
            super::emit_load_handles_base(emitter, "r10");
            emitter.instruction("mov r10, QWORD PTR [r10 + r9*8]");             // obj = _user_wrapper_handles[slot]
            emitter.instruction("test r10, r10");
            emitter.instruction("jz __rt_uwls_no_x86");                         // already closed
            emitter.instruction("mov r11, QWORD PTR [r10]");                    // class id
            abi::emit_symbol_address(emitter, "rax", "_user_wrapper_vtable_ptrs");
            emitter.instruction("mov rax, QWORD PTR [rax + r11*8]");            // this class's wrapper vtable
            emitter.instruction("test rax, rax");
            emitter.instruction("jz __rt_uwls_yes_x86");                        // no vtable reads as no method
            emitter.instruction(&format!("mov rax, QWORD PTR [rax + {}]", VTABLE_SLOT_SEEK * 8));
            emitter.instruction("test rax, rax");
            emitter.instruction("jnz __rt_uwls_no_x86");                        // the method exists
            emitter.label("__rt_uwls_yes_x86");
            emitter.instruction("mov rax, 1");
            emitter.instruction("ret");
            emitter.label("__rt_uwls_no_x86");
            emitter.instruction("xor eax, eax");
            emitter.instruction("ret");
        }
    }
}
