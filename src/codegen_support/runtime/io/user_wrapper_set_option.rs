//! Purpose:
//! Emits `__rt_user_wrapper_set_option`, the fd-based dispatcher that routes
//! `stream_set_blocking()` / `stream_set_timeout()` on a synthetic userspace
//! wrapper descriptor to the wrapper object's `stream_set_option($option,
//! $arg1, $arg2)` method (vtable slot 13).
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::io`.
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

use crate::codegen_support::value_boxing::emit_box_current_value_as_mixed;
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::types::PhpType;

/// Byte offset of the `stream_set_option` method pointer in the per-class
/// user-wrapper vtable (slot 13 of `USER_WRAPPER_VTABLE_SLOTS`, 8 bytes each).
const VTABLE_SET_OPTION_OFFSET: usize = 13 * 8;

/// `PHP_STREAM_OPTION_BLOCKING`, the one option whose `$arg2` php sends as NULL.
///
/// Every other option this dispatcher serves — the read and write buffers, the read timeout —
/// sends an integer there. MEASURED on `php -n` 8.5.6.
const STREAM_OPTION_BLOCKING: usize = 1;

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

    // Frame: [0]=option [8]=arg1 [16]=arg2 and then its box [24]=wrapper object
    //        [32]=method pointer [40]=the method's result
    emitter.instruction("sub sp, sp, #64");                                     // helper frame for the wrapper dispatch
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // the option being set
    emitter.instruction("str x2, [sp, #8]");                                    // its first argument
    emitter.instruction("str x3, [sp, #16]");                                   // and its second, still raw

    // -- resolve the open wrapper instance from the synthetic fd --
    emitter.instruction("mov x9, #0x40000000");                                 // USER_WRAPPER_FD_BASE
    emitter.instruction("sub x9, x0, x9");                                      // x9 = handle slot index = fd - BASE
    super::emit_load_handles_base(emitter, "x10");
    emitter.instruction("ldr x0, [x10, x9, lsl #3]");                           // obj = _user_wrapper_handles[slot]
    emitter.instruction("cbz x0, __rt_uwsetopt_false");                         // empty slot → false
    emitter.instruction("str x0, [sp, #24]");                                   // the object the call needs back

    // -- resolve stream_set_option (vtable slot 13) for the object's class --
    emitter.instruction("ldr x10, [x0]");                                       // class_id at the head of every wrapper object
    abi::emit_symbol_address(emitter, "x11", "_user_wrapper_vtable_ptrs");
    emitter.instruction("ldr x11, [x11, x10, lsl #3]");                         // per-class user-wrapper vtable
    emitter.instruction(&format!("ldr x11, [x11, #{}]", VTABLE_SET_OPTION_OFFSET)); // load the stream_set_option method pointer (slot 13)
    emitter.instruction("cbz x11, __rt_uwsetopt_false");                        // class did not implement stream_set_option → false
    emitter.instruction("str x11, [sp, #32]");                                  // the boxing call below clobbers it

    // -- `$arg2` is php's `mixed`, and the OPTION says which one --
    //
    // BLOCKING sends NULL and everything else sends an integer, so the box is minted here
    // rather than at the four call sites, which hold their arguments in registers and would
    // each have to spill them around the boxing call. The box is owned HERE: the method
    // borrows it for the length of the call, exactly as the `stream_metadata` hook's third
    // argument is borrowed.
    emitter.instruction("ldr x9, [sp, #0]");                                    // the option
    emitter.instruction(&format!("cmp x9, #{STREAM_OPTION_BLOCKING}"));
    emitter.instruction("b.eq __rt_uwsetopt_null_arg2");
    emitter.instruction("ldr x0, [sp, #16]");                                   // the integer php sends for the rest
    emit_box_current_value_as_mixed(emitter, &PhpType::Int);
    emitter.instruction("b __rt_uwsetopt_arg2_boxed");
    emitter.label("__rt_uwsetopt_null_arg2");
    emitter.instruction("mov x0, #0");                                          // null has no payload
    emit_box_current_value_as_mixed(emitter, &PhpType::Void);
    emitter.label("__rt_uwsetopt_arg2_boxed");
    emitter.instruction("str x0, [sp, #16]");                                   // the boxed `$arg2`, owned here

    // -- call stream_set_option($this, $option, $arg1, $arg2) → bool in x0 --
    emitter.instruction("ldr x0, [sp, #24]");                                   // $this
    emitter.instruction("ldr x1, [sp, #0]");                                    // $option
    emitter.instruction("ldr x2, [sp, #8]");                                    // $arg1
    emitter.instruction("ldr x3, [sp, #16]");                                   // $arg2
    emitter.instruction("ldr x11, [sp, #32]");                                  // the method pointer
    emitter.instruction("blr x11");                                             // invoke stream_set_option on the wrapper object
    emitter.instruction("str x0, [sp, #40]");                                   // hold the bool result across the release
    emitter.instruction("ldr x0, [sp, #16]");                                   // the box nobody else owns
    emitter.instruction("bl __rt_decref_any");
    emitter.instruction("ldr x0, [sp, #40]");                                   // the wrapper's answer
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the wrapper's bool result

    emitter.label("__rt_uwsetopt_false");
    emitter.instruction("mov x0, #0");                                          // false when the handle or method is absent
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return false
}

/// x86_64 implementation of `__rt_user_wrapper_set_option`.
fn emit_user_wrapper_set_option_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_set_option ---");
    emitter.label_global("__rt_user_wrapper_set_option");

    // Frame: [rbp-8]=option [rbp-16]=arg1 [rbp-24]=arg2 and then its box
    //        [rbp-32]=wrapper object [rbp-40]=method pointer [rbp-48]=the method's result
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 64");                                         // helper frame; the push above keeps rsp aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // the option being set
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // its first argument
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // and its second, still raw

    // -- resolve the open wrapper instance from the synthetic fd --
    emitter.instruction("mov r9, rdi");                                         // copy the synthetic fd
    emitter.instruction("sub r9, 0x40000000");                                  // r9 = handle slot index = fd - USER_WRAPPER_FD_BASE
    super::emit_load_handles_base(emitter, "r10");          // handle table base
    emitter.instruction("mov rdi, QWORD PTR [r10 + r9 * 8]");                   // obj = _user_wrapper_handles[slot]
    emitter.instruction("test rdi, rdi");                                       // empty slot?
    emitter.instruction("jz __rt_uwsetopt_false_x86");                          // empty slot → false
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // the object the call needs back

    // -- resolve stream_set_option (vtable slot 13) for the object's class --
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // class_id at the head of every wrapper object
    abi::emit_symbol_address(emitter, "r11", "_user_wrapper_vtable_ptrs");      // base of the per-class vtable pointer table
    emitter.instruction("mov r11, QWORD PTR [r11 + r10 * 8]");                  // per-class user-wrapper vtable
    emitter.instruction(&format!("mov r11, QWORD PTR [r11 + {}]", VTABLE_SET_OPTION_OFFSET)); // load the stream_set_option method pointer (slot 13)
    emitter.instruction("test r11, r11");                                       // class did not implement stream_set_option?
    emitter.instruction("jz __rt_uwsetopt_false_x86");                          // missing method → false
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // the boxing call below clobbers it

    // -- `$arg2` is php's `mixed`; see the AArch64 arm --
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // the option
    emitter.instruction(&format!("cmp r9, {STREAM_OPTION_BLOCKING}"));
    emitter.instruction("je __rt_uwsetopt_null_arg2_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // the integer php sends for the rest
    emit_box_current_value_as_mixed(emitter, &PhpType::Int);
    emitter.instruction("jmp __rt_uwsetopt_arg2_boxed_x86");
    emitter.label("__rt_uwsetopt_null_arg2_x86");
    emitter.instruction("xor eax, eax");                                        // null has no payload
    emit_box_current_value_as_mixed(emitter, &PhpType::Void);
    emitter.label("__rt_uwsetopt_arg2_boxed_x86");
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // the boxed `$arg2`, owned here

    // -- call stream_set_option($this, $option, $arg1, $arg2) → bool in rax --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // $this
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // $option
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // $arg1
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // $arg2
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // the method pointer
    emitter.instruction("call r11");                                            // invoke stream_set_option on the wrapper object
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // hold the bool result across the release
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // the box nobody else owns
    emitter.instruction("call __rt_decref_any");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // the wrapper's answer
    emitter.instruction("leave");                                               // restore rbp and rsp
    emitter.instruction("ret");                                                 // return the wrapper's bool result

    emitter.label("__rt_uwsetopt_false_x86");
    emitter.instruction("xor eax, eax");                                        // false when the handle or method is absent
    emitter.instruction("leave");                                               // restore rbp and rsp
    emitter.instruction("ret");                                                 // return false
}
