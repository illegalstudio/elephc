//! Purpose:
//! Emits `__rt_user_wrapper_set_option`, the fd-based dispatcher that routes
//! `stream_set_blocking()` / `stream_set_timeout()` on a synthetic userspace
//! wrapper descriptor to the wrapper object's `stream_set_option($option,
//! $arg1, $arg2)` method (vtable slot 13).
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via
//!   `crate::codegen::runtime::io`.
//! - The `stream_set_blocking` / `stream_set_timeout` builtin emitters, after a
//!   synthetic-fd check (`fd >= USER_WRAPPER_FD_BASE`) selects the wrapper
//!   branch (mirroring the `flock()` / `ftruncate()` fd-based dispatch).
//!
//! Key details:
//! - The handle/method lookup is inlined here (rather than reusing the private
//!   helpers in `user_wrapper.rs`) so this dispatcher is self-contained: it
//!   resolves the open wrapper instance from `_user_wrapper_handles[fd - BASE]`,
//!   then the method pointer from `_user_wrapper_vtable_ptrs[class_id][13]`.
//! - On entry the option/arg1/arg2 already occupy the method's argument
//!   registers (x1/x2/x3, rsi/rdx/rcx); the lookup only touches x9/x10/x11
//!   (r9/r10/r11), so they survive into the `stream_set_option($this, $option,
//!   $arg1, $arg2)` call with no shuffling.
//! - A missing handle or missing method returns 0 (`false`), matching PHP's
//!   result when a wrapper does not implement `stream_set_option`.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// Byte offset of the `stream_set_option` method pointer in the per-class
/// user-wrapper vtable (slot 13 of `USER_WRAPPER_VTABLE_SLOTS`, 8 bytes each).
const VTABLE_SET_OPTION_OFFSET: usize = 13 * 8;

/// Emits `__rt_user_wrapper_set_option(fd, option, arg1, arg2) -> 1/0`.
///
/// Inputs (AArch64): x0 = synthetic wrapper fd, x1 = option, x2 = arg1,
/// x3 = arg2. (x86_64): rdi = fd, rsi = option, rdx = arg1, rcx = arg2.
/// Output: x0 / rax = the wrapper's `stream_set_option` bool result, or 0 when
/// the handle slot is empty or the class does not implement the method.
pub fn emit_user_wrapper_set_option(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_user_wrapper_set_option_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_set_option ---");
    emitter.label_global("__rt_user_wrapper_set_option");

    emitter.instruction("sub sp, sp, #16");                                     // helper frame for the wrapper dispatch
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer

    // -- resolve the open wrapper instance from the synthetic fd --
    emitter.instruction("mov x9, #0x40000000");                                 // USER_WRAPPER_FD_BASE
    emitter.instruction("sub x9, x0, x9");                                      // x9 = handle slot index = fd - BASE
    abi::emit_symbol_address(emitter, "x10", "_user_wrapper_handles");
    emitter.instruction("ldr x0, [x10, x9, lsl #3]");                           // obj = _user_wrapper_handles[slot]
    emitter.instruction("cbz x0, __rt_uwsetopt_false");                         // empty slot → false

    // -- resolve stream_set_option (vtable slot 13) for the object's class --
    emitter.instruction("ldr x10, [x0]");                                       // class_id at the head of every wrapper object
    abi::emit_symbol_address(emitter, "x11", "_user_wrapper_vtable_ptrs");
    emitter.instruction("ldr x11, [x11, x10, lsl #3]");                         // per-class user-wrapper vtable
    emitter.instruction(&format!("ldr x11, [x11, #{}]", VTABLE_SET_OPTION_OFFSET)); //load the stream_set_option method pointer (slot 13)
    emitter.instruction("cbz x11, __rt_uwsetopt_false");                        // class did not implement stream_set_option → false

    // -- call stream_set_option($this, $option, $arg1, $arg2) → bool in x0 --
    emitter.instruction("blr x11");                                             // invoke stream_set_option on the wrapper object
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the wrapper's bool result

    emitter.label("__rt_uwsetopt_false");
    emitter.instruction("mov x0, #0");                                          // false when the handle or method is absent
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return false
}

/// x86_64 implementation of `__rt_user_wrapper_set_option`.
fn emit_user_wrapper_set_option_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_set_option ---");
    emitter.label_global("__rt_user_wrapper_set_option");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer

    // -- resolve the open wrapper instance from the synthetic fd --
    emitter.instruction("mov r9, rdi");                                         // copy the synthetic fd
    emitter.instruction("sub r9, 0x40000000");                                  // r9 = handle slot index = fd - USER_WRAPPER_FD_BASE
    abi::emit_symbol_address(emitter, "r10", "_user_wrapper_handles");          // handle table base
    emitter.instruction("mov rdi, QWORD PTR [r10 + r9 * 8]");                   // obj = _user_wrapper_handles[slot]
    emitter.instruction("test rdi, rdi");                                       // empty slot?
    emitter.instruction("jz __rt_uwsetopt_false_x86");                          // empty slot → false

    // -- resolve stream_set_option (vtable slot 13) for the object's class --
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // class_id at the head of every wrapper object
    abi::emit_symbol_address(emitter, "r11", "_user_wrapper_vtable_ptrs");      // base of the per-class vtable pointer table
    emitter.instruction("mov r11, QWORD PTR [r11 + r10 * 8]");                  // per-class user-wrapper vtable
    emitter.instruction(&format!("mov r11, QWORD PTR [r11 + {}]", VTABLE_SET_OPTION_OFFSET)); //load the stream_set_option method pointer (slot 13)
    emitter.instruction("test r11, r11");                                       // class did not implement stream_set_option?
    emitter.instruction("jz __rt_uwsetopt_false_x86");                          // missing method → false

    // -- call stream_set_option($this, $option, $arg1, $arg2) → bool in rax --
    emitter.instruction("call r11");                                            // invoke stream_set_option on the wrapper object
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the wrapper's bool result

    emitter.label("__rt_uwsetopt_false_x86");
    emitter.instruction("xor eax, eax");                                        // false when the handle or method is absent
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return false
}
