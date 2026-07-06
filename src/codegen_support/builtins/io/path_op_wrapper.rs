//! Purpose:
//! Shared codegen helper that routes a single-path filesystem-mutation builtin
//! (`unlink`/`mkdir`/`rmdir`) to a registered userspace stream wrapper when the
//! path's `scheme://` prefix matches, or to the builtin's libc runtime helper
//! otherwise.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::{unlink, mkdir, rmdir}::emit()`, after the
//!   path string has been materialized into the string-result registers
//!   (`x1`/`x2` on AArch64, `rax`/`rdx` on x86_64).
//!
//! Key details:
//! - Mirrors the `readfile()` builtin's wrapper/filesystem split: probe with
//!   `__rt_path_is_wrapper`, then branch to `__rt_user_wrapper_path_op` (with the
//!   method's vtable slot) or the libc helper.
//! - `__rt_path_is_wrapper` takes the path in the SysV first/second args
//!   (`x0`/`x1`, `rdi`/`rsi`); the libc helpers keep consuming the path from the
//!   string-result registers; `__rt_user_wrapper_path_op` takes `x0`=ptr,
//!   `x1`=len, `x2`=slot (`rdi`/`rsi`/`rdx` on x86_64), with the extra int args
//!   zeroed (PHP's `$mode`/`$options` default to wrapper-defined behavior).

use crate::codegen_support::context::Context;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::{abi, platform::Arch};

/// `stream_metadata` vtable slot index in the per-class user-wrapper vtable.
const STREAM_METADATA_SLOT: usize = 14;
/// PHP `STREAM_META_OWNER_NAME` option value (`chown` by string user name).
pub(crate) const STREAM_META_OWNER_NAME: usize = 2;
/// PHP `STREAM_META_OWNER` option value (`chown` by integer uid).
pub(crate) const STREAM_META_OWNER: usize = 3;
/// PHP `STREAM_META_GROUP_NAME` option value (`chgrp` by string group name).
pub(crate) const STREAM_META_GROUP_NAME: usize = 4;
/// PHP `STREAM_META_GROUP` option value (`chgrp` by integer gid).
pub(crate) const STREAM_META_GROUP: usize = 5;

/// Boxes a raw integer (in `x0` / `rax`) into an owned `Mixed` cell, leaving the
/// boxed pointer in `x0` / `rax`.
///
/// PHP passes `stream_metadata`'s `$value` as `mixed`, so an integer value (chmod
/// mode, chown uid, chgrp gid) must be boxed before it is handed to the wrapper
/// method. The caller owns the returned cell and must release it with
/// `__rt_decref_mixed` once the wrapper call has returned (the callee borrows).
pub fn emit_box_int_as_mixed(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x1, x0");                                  // value_lo = the integer
            emitter.instruction("mov x2, #0");                                  // value_hi = 0 for an integer scalar
            emitter.instruction("mov x0, #0");                                  // runtime tag 0 = int
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box the int → x0 = owned Mixed ptr
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, rax");                                // value_lo = the integer
            emitter.instruction("xor esi, esi");                                // value_hi = 0 for an integer scalar
            emitter.instruction("xor eax, eax");                                // runtime tag 0 = int
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box the int → rax = owned Mixed ptr
        }
    }
}

/// Boxes a runtime string (pointer in `x1`/`rax`, length in `x2`/`rdx` — the
/// string-result registers) into an owned `Mixed` cell, leaving the boxed pointer
/// in `x0` / `rax`.
///
/// `__rt_mixed_from_value` persists the string payload for the boxed owner, so the
/// caller owns the returned cell and must release it with `__rt_decref_mixed`
/// after the wrapper call returns.
pub fn emit_box_string_as_mixed(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            // x1 = ptr, x2 = len already match __rt_mixed_from_value's value_lo/value_hi.
            emitter.instruction("mov x0, #1");                                  // runtime tag 1 = string
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // persist+box → x0 = owned Mixed ptr
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, rax");                                // value_lo = string pointer
            emitter.instruction("mov rsi, rdx");                                // value_hi = string length
            emitter.instruction("mov eax, 1");                                  // runtime tag 1 = string
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // persist+box → rax = owned Mixed ptr
        }
    }
}

/// Emits the wrapper-vs-filesystem dispatch for a single-path mutation builtin.
///
/// On entry the path string occupies the string-result registers (`x1`=ptr,
/// `x2`=len on AArch64; `rax`=ptr, `rdx`=len on x86_64). When the path's scheme
/// matches a registered wrapper, calls `__rt_user_wrapper_path_op(path, len,
/// slot, 0, 0)`; otherwise calls `libc_helper` (e.g. `__rt_unlink`) which
/// consumes the path from the string-result registers. The bool result is left
/// in the standard return register (`x0`/`rax`).
pub fn emit_single_path_wrapper_dispatch(
    emitter: &mut Emitter,
    ctx: &mut Context,
    libc_helper: &str,
    vtable_slot: usize,
) {
    let wrapper = ctx.next_label("path_op_wrapper");
    let after = ctx.next_label("path_op_after");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #16");                             // scratch: [sp,#0] path ptr, [sp,#8] path len
            emitter.instruction("str x1, [sp, #0]");                            // preserve path ptr across the wrapper-scheme probe
            emitter.instruction("str x2, [sp, #8]");                            // preserve path len across the wrapper-scheme probe
            emitter.instruction("mov x0, x1");                                  // path_is_wrapper arg0 = path ptr
            emitter.instruction("mov x1, x2");                                  // path_is_wrapper arg1 = path len
            abi::emit_call_label(emitter, "__rt_path_is_wrapper");              // x0 = 1 when the scheme matches a registered wrapper
            emitter.instruction("ldr x1, [sp, #0]");                            // restore path ptr for the chosen helper
            emitter.instruction("ldr x2, [sp, #8]");                            // restore path len for the chosen helper
            emitter.instruction(&format!("cbnz x0, {}", wrapper));              // registered wrapper scheme → wrapper path-op
            abi::emit_call_label(emitter, libc_helper);                         // normal path: libc filesystem helper (path in x1/x2)
            emitter.instruction(&format!("b {}", after));                       // skip the wrapper path
            emitter.label(&wrapper);
            emitter.instruction("mov x0, x1");                                  // user_wrapper_path_op arg0 = path ptr
            emitter.instruction("mov x1, x2");                                  // user_wrapper_path_op arg1 = path len
            emitter.instruction(&format!("mov x2, #{}", vtable_slot));          // arg2 = the method's vtable slot index
            emitter.instruction("mov x3, #0");                                  // arg3 = 0 (mode/options default)
            emitter.instruction("mov x4, #0");                                  // arg4 = 0 (options default)
            abi::emit_call_label(emitter, "__rt_user_wrapper_path_op");         // dispatch into the wrapper's path method
            emitter.label(&after);
            emitter.instruction("add sp, sp, #16");                             // release the scratch frame
        }
        Arch::X86_64 => {
            emitter.instruction("sub rsp, 16");                                 // scratch: [rsp+0] path ptr, [rsp+8] path len
            emitter.instruction("mov QWORD PTR [rsp + 0], rax");                // preserve path ptr across the wrapper-scheme probe
            emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                // preserve path len across the wrapper-scheme probe
            emitter.instruction("mov rdi, rax");                                // path_is_wrapper arg0 = path ptr
            emitter.instruction("mov rsi, rdx");                                // path_is_wrapper arg1 = path len
            abi::emit_call_label(emitter, "__rt_path_is_wrapper");              // rax = 1 when the scheme matches a registered wrapper
            emitter.instruction("test rax, rax");                               // matched a registered wrapper scheme?
            emitter.instruction("mov rax, QWORD PTR [rsp + 0]");                // restore path ptr for the chosen helper
            emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                // restore path len for the chosen helper
            emitter.instruction(&format!("jnz {}", wrapper));                   // registered wrapper scheme → wrapper path-op
            abi::emit_call_label(emitter, libc_helper);                         // normal path: libc filesystem helper (path in rax/rdx)
            emitter.instruction(&format!("jmp {}", after));                     // skip the wrapper path
            emitter.label(&wrapper);
            emitter.instruction("mov rdi, rax");                                // user_wrapper_path_op arg0 = path ptr
            emitter.instruction("mov rsi, rdx");                                // user_wrapper_path_op arg1 = path len
            emitter.instruction(&format!("mov rdx, {}", vtable_slot));          // arg2 = the method's vtable slot index
            emitter.instruction("xor ecx, ecx");                                // arg3 = 0 (mode/options default)
            emitter.instruction("xor r8d, r8d");                                // arg4 = 0 (options default)
            abi::emit_call_label(emitter, "__rt_user_wrapper_path_op");         // dispatch into the wrapper's path method
            emitter.label(&after);
            emitter.instruction("add rsp, 16");                                 // release the scratch frame
        }
    }
}

/// Emits the wrapper-vs-filesystem dispatch for a string-named ownership metadata
/// change — `chown`/`chgrp` with a user/group NAME instead of a numeric id.
///
/// Precondition: the path string was just pushed (`stp x1,x2,[sp,#-16]!` on
/// AArch64, `emit_push_reg_pair(rax,rdx)` on x86_64) and the name string is in the
/// string-result registers (`x1`/`x2` on AArch64, `rax`/`rdx` on x86_64). The
/// wrapper is probed FIRST so the libc fallback keeps the raw name: a registered
/// scheme boxes the name as `Mixed` and calls
/// `__rt_user_wrapper_path_op(path, len, slot=14, option, boxed_value)` invoking
/// `stream_metadata($path, $option, $value)` (releasing the boxed value after); a
/// non-wrapper path calls `libc_helper(path, name_ptr, name_len)`
/// (`__rt_chown_user` / `__rt_chgrp_group`). Bool result in `x0` / `rax`.
pub fn emit_owner_group_name_wrapper_dispatch(
    emitter: &mut Emitter,
    ctx: &mut Context,
    option: usize,
    libc_helper: &str,
) {
    let wrapper = ctx.next_label("meta_name_wrapper");
    let after = ctx.next_label("meta_name_after");
    match emitter.target.arch {
        Arch::AArch64 => {
            // On entry: path at [sp,#0]/[sp,#8] (caller's push), name in x1/x2.
            emitter.instruction("sub sp, sp, #16");                             // name scratch: [sp,#0] name ptr, [sp,#8] name len (path now at [sp,#16]/[sp,#24])
            emitter.instruction("str x1, [sp, #0]");                            // save the name pointer
            emitter.instruction("str x2, [sp, #8]");                            // save the name length
            emitter.instruction("ldr x0, [sp, #16]");                           // path_is_wrapper arg0 = path ptr
            emitter.instruction("ldr x1, [sp, #24]");                           // path_is_wrapper arg1 = path len
            abi::emit_call_label(emitter, "__rt_path_is_wrapper");              // x0 = 1 when the scheme matches a registered wrapper
            emitter.instruction(&format!("cbnz x0, {}", wrapper));              // registered wrapper scheme -> stream_metadata
            emitter.instruction("ldr x1, [sp, #16]");                           // libc path ptr -> x1
            emitter.instruction("ldr x2, [sp, #24]");                           // libc path len -> x2
            emitter.instruction("ldr x3, [sp, #0]");                            // libc name ptr -> x3
            emitter.instruction("ldr x4, [sp, #8]");                            // libc name len -> x4
            emitter.instruction("add sp, sp, #32");                             // release the name scratch and the caller's path push
            abi::emit_call_label(emitter, libc_helper);                         // normal path: resolve the name and call libc chown
            emitter.instruction(&format!("b {}", after));                       // skip the wrapper path
            emitter.label(&wrapper);
            emitter.instruction("ldr x1, [sp, #0]");                            // reload the name pointer for boxing
            emitter.instruction("ldr x2, [sp, #8]");                            // reload the name length for boxing
            emit_box_string_as_mixed(emitter);                                 // box $value as mixed -> x0 = owned Mixed(string)
            emitter.instruction("str x0, [sp, #0]");                            // stash the boxed value pointer (name ptr slot reused)
            emitter.instruction("ldr x0, [sp, #16]");                           // wrapper path ptr -> x0
            emitter.instruction("ldr x1, [sp, #24]");                           // wrapper path len -> x1
            emitter.instruction(&format!("mov x2, #{}", STREAM_METADATA_SLOT)); // stream_metadata vtable slot
            emitter.instruction(&format!("mov x3, #{}", option));               // option = STREAM_META_OWNER_NAME/GROUP_NAME
            emitter.instruction("ldr x4, [sp, #0]");                            // value = boxed mixed pointer
            abi::emit_call_label(emitter, "__rt_user_wrapper_path_op");         // dispatch into the wrapper's stream_metadata
            emitter.instruction("str x0, [sp, #8]");                            // stash the bool result (free name-len slot) across the value release
            emitter.instruction("ldr x0, [sp, #0]");                            // reload the boxed value pointer
            abi::emit_call_label(emitter, "__rt_decref_mixed");                 // release the boxed $value (caller owns; the method borrowed it)
            emitter.instruction("ldr x0, [sp, #8]");                            // restore the bool result
            emitter.instruction("add sp, sp, #32");                             // release the name scratch and the caller's path push
            emitter.label(&after);
        }
        Arch::X86_64 => {
            // On entry: path at [rsp+0]/[rsp+8] (caller's push), name in rax/rdx.
            emitter.instruction("sub rsp, 16");                                 // name scratch: [rsp+0] name ptr, [rsp+8] name len (path now at [rsp+16]/[rsp+24])
            emitter.instruction("mov QWORD PTR [rsp + 0], rax");                // save the name pointer
            emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                // save the name length
            emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");               // path_is_wrapper arg0 = path ptr
            emitter.instruction("mov rsi, QWORD PTR [rsp + 24]");               // path_is_wrapper arg1 = path len
            abi::emit_call_label(emitter, "__rt_path_is_wrapper");              // rax = 1 when the scheme matches a registered wrapper
            emitter.instruction("test rax, rax");                               // matched a registered wrapper scheme?
            emitter.instruction(&format!("jnz {}", wrapper));                   // registered wrapper scheme -> stream_metadata
            emitter.instruction("mov rax, QWORD PTR [rsp + 16]");               // libc path ptr -> rax
            emitter.instruction("mov rdx, QWORD PTR [rsp + 24]");               // libc path len -> rdx
            emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");                // libc name ptr -> rdi
            emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");                // libc name len -> rsi
            emitter.instruction("add rsp, 32");                                 // release the name scratch and the caller's path push
            abi::emit_call_label(emitter, libc_helper);                         // normal path: resolve the name and call libc chown
            emitter.instruction(&format!("jmp {}", after));                     // skip the wrapper path
            emitter.label(&wrapper);
            emitter.instruction("mov rax, QWORD PTR [rsp + 0]");                // reload the name pointer for boxing
            emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                // reload the name length for boxing
            emit_box_string_as_mixed(emitter);                                 // box $value as mixed -> rax = owned Mixed(string)
            emitter.instruction("mov QWORD PTR [rsp + 0], rax");                // stash the boxed value pointer (name ptr slot reused)
            emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");               // wrapper path ptr -> rdi
            emitter.instruction("mov rsi, QWORD PTR [rsp + 24]");               // wrapper path len -> rsi
            emitter.instruction(&format!("mov rdx, {}", STREAM_METADATA_SLOT)); // stream_metadata vtable slot
            emitter.instruction(&format!("mov rcx, {}", option));               // option = STREAM_META_OWNER_NAME/GROUP_NAME
            emitter.instruction("mov r8, QWORD PTR [rsp + 0]");                 // value = boxed mixed pointer
            abi::emit_call_label(emitter, "__rt_user_wrapper_path_op");         // dispatch into the wrapper's stream_metadata
            emitter.instruction("mov QWORD PTR [rsp + 8], rax");                // stash the bool result (free name-len slot) across the value release
            emitter.instruction("mov rax, QWORD PTR [rsp + 0]");                // reload the boxed value pointer
            abi::emit_call_label(emitter, "__rt_decref_mixed");                 // release the boxed $value (caller owns; the method borrowed it)
            emitter.instruction("mov rax, QWORD PTR [rsp + 8]");                // restore the bool result
            emitter.instruction("add rsp, 32");                                 // release the name scratch and the caller's path push
            emitter.label(&after);
        }
    }
}

/// Emits the wrapper-vs-filesystem dispatch for an integer-valued ownership
/// metadata change — `chown`/`chgrp` with a numeric uid/gid.
///
/// Precondition: the path string was just pushed (`stp x1,x2,[sp,#-16]!` on
/// AArch64, `emit_push_reg_pair(rax, rdx)` on x86_64) and the uid/gid value is in
/// `x0`/`rax`. When the path scheme matches a registered wrapper, routes to
/// `__rt_user_wrapper_path_op(path, len, slot=14, option, value)` invoking the
/// wrapper's `stream_metadata($path, $option, $value)`; otherwise calls the libc
/// `__rt_chown(path, uid, gid)` with the value placed in uid (`option`==OWNER) or
/// gid (`option`==GROUP) and the other field left at -1. Bool result in `x0`/`rax`.
pub fn emit_owner_group_wrapper_dispatch(emitter: &mut Emitter, ctx: &mut Context, option: usize) {
    let wrapper = ctx.next_label("meta_owngrp_wrapper");
    let after = ctx.next_label("meta_owngrp_after");
    let is_owner = option == STREAM_META_OWNER;
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x9, x0");                                  // stash the uid/gid value across the path pop
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore path ptr/len from the caller's push
            emitter.instruction("sub sp, sp, #32");                             // scratch: [sp,#0] path ptr, [sp,#8] path len, [sp,#16] value
            emitter.instruction("str x1, [sp, #0]");                            // save path ptr
            emitter.instruction("str x2, [sp, #8]");                            // save path len
            emitter.instruction("str x9, [sp, #16]");                           // save the uid/gid value across the probe
            emitter.instruction("mov x0, x1");                                  // path_is_wrapper arg0 = path ptr
            emitter.instruction("mov x1, x2");                                  // path_is_wrapper arg1 = path len
            abi::emit_call_label(emitter, "__rt_path_is_wrapper");              // x0 = 1 when the scheme matches a registered wrapper
            emitter.instruction(&format!("cbnz x0, {}", wrapper));              // registered wrapper scheme → stream_metadata
            emitter.instruction("ldr x1, [sp, #0]");                            // libc path ptr → x1
            emitter.instruction("ldr x2, [sp, #8]");                            // libc path len → x2
            if is_owner {
                emitter.instruction("ldr x3, [sp, #16]");                       // uid = value
                emitter.instruction("mov x4, #-1");                             // gid = -1 (leave group unchanged)
            } else {
                emitter.instruction("mov x3, #-1");                             // uid = -1 (leave owner unchanged)
                emitter.instruction("ldr x4, [sp, #16]");                       // gid = value
            }
            emitter.instruction("add sp, sp, #32");                             // release the scratch frame before the call
            abi::emit_call_label(emitter, "__rt_chown");                        // normal path: libc chown(path, uid, gid)
            emitter.instruction(&format!("b {}", after));                       // skip the wrapper path
            emitter.label(&wrapper);
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the uid/gid integer
            emit_box_int_as_mixed(emitter);                                     // box $value as mixed → x0 = owned Mixed(int)
            emitter.instruction("str x0, [sp, #16]");                           // stash the boxed value pointer (value slot reused)
            emitter.instruction("ldr x0, [sp, #0]");                            // wrapper path ptr → x0
            emitter.instruction("ldr x1, [sp, #8]");                            // wrapper path len → x1
            emitter.instruction(&format!("mov x2, #{}", STREAM_METADATA_SLOT)); // stream_metadata vtable slot
            emitter.instruction(&format!("mov x3, #{}", option));               // option = STREAM_META_OWNER/GROUP
            emitter.instruction("ldr x4, [sp, #16]");                           // value = boxed mixed pointer
            abi::emit_call_label(emitter, "__rt_user_wrapper_path_op");         // dispatch into the wrapper's stream_metadata
            emitter.instruction("str x0, [sp, #0]");                            // stash the bool result across the value release
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the boxed value pointer
            abi::emit_call_label(emitter, "__rt_decref_mixed");                 // release the boxed $value (caller owns; the method borrowed it)
            emitter.instruction("ldr x0, [sp, #0]");                            // restore the bool result
            emitter.instruction("add sp, sp, #32");                             // release the scratch frame
            emitter.label(&after);
        }
        Arch::X86_64 => {
            emitter.instruction("mov r9, rax");                                 // stash the uid/gid value across the path pop
            abi::emit_pop_reg_pair(emitter, "rax", "rdx"); // restore path ptr/len from the caller's push
            emitter.instruction("sub rsp, 32");                                 // scratch: [rsp+0] path ptr, [rsp+8] path len, [rsp+16] value
            emitter.instruction("mov QWORD PTR [rsp + 0], rax");                // save path ptr
            emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                // save path len
            emitter.instruction("mov QWORD PTR [rsp + 16], r9");                // save the uid/gid value across the probe
            emitter.instruction("mov rdi, rax");                                // path_is_wrapper arg0 = path ptr
            emitter.instruction("mov rsi, rdx");                                // path_is_wrapper arg1 = path len
            abi::emit_call_label(emitter, "__rt_path_is_wrapper");              // rax = 1 when the scheme matches a registered wrapper
            emitter.instruction("test rax, rax");                               // matched a registered wrapper scheme?
            emitter.instruction(&format!("jnz {}", wrapper));                   // registered wrapper scheme → stream_metadata
            emitter.instruction("mov rax, QWORD PTR [rsp + 0]");                // libc path ptr → rax
            emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                // libc path len → rdx
            if is_owner {
                emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");           // uid = value
                emitter.instruction("mov rsi, -1");                             // gid = -1 (leave group unchanged)
            } else {
                emitter.instruction("mov rdi, -1");                             // uid = -1 (leave owner unchanged)
                emitter.instruction("mov rsi, QWORD PTR [rsp + 16]");           // gid = value
            }
            emitter.instruction("add rsp, 32");                                 // release the scratch frame before the call
            abi::emit_call_label(emitter, "__rt_chown");                        // normal path: libc chown(path, uid, gid)
            emitter.instruction(&format!("jmp {}", after));                     // skip the wrapper path
            emitter.label(&wrapper);
            emitter.instruction("mov rax, QWORD PTR [rsp + 16]");               // reload the uid/gid integer
            emit_box_int_as_mixed(emitter);                                     // box $value as mixed → rax = owned Mixed(int)
            emitter.instruction("mov QWORD PTR [rsp + 16], rax");               // stash the boxed value pointer (value slot reused)
            emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");                // wrapper path ptr → rdi
            emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");                // wrapper path len → rsi
            emitter.instruction(&format!("mov rdx, {}", STREAM_METADATA_SLOT)); // stream_metadata vtable slot
            emitter.instruction(&format!("mov rcx, {}", option));               // option = STREAM_META_OWNER/GROUP
            emitter.instruction("mov r8, QWORD PTR [rsp + 16]");                // value = boxed mixed pointer
            abi::emit_call_label(emitter, "__rt_user_wrapper_path_op");         // dispatch into the wrapper's stream_metadata
            emitter.instruction("mov QWORD PTR [rsp + 0], rax");                // stash the bool result across the value release
            emitter.instruction("mov rax, QWORD PTR [rsp + 16]");               // reload the boxed value pointer
            abi::emit_call_label(emitter, "__rt_decref_mixed");                 // release the boxed $value (caller owns; the method borrowed it)
            emitter.instruction("mov rax, QWORD PTR [rsp + 0]");                // restore the bool result
            emitter.instruction("add rsp, 32");                                 // release the scratch frame
            emitter.label(&after);
        }
    }
}
