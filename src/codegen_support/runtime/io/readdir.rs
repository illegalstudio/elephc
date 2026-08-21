//! Purpose:
//! Emits `__rt_readdir` over a typed `StreamState` directory backend.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - Native `DIR*`, glob iterator state, and userspace wrapper handles are
//!   selected by backend kind rather than descriptor ranges or fixed tables.

use crate::codegen_support::runtime::resources::layout::{
    STREAM_BACKEND_AUX_OFFSET, STREAM_BACKEND_DIRECTORY, STREAM_BACKEND_GLOB_DIRECTORY,
    STREAM_BACKEND_KIND_OFFSET, STREAM_BACKEND_USER_DIRECTORY, STREAM_FD_OFFSET,
};
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits directory iteration from an authoritative `StreamState` pointer.
pub fn emit_readdir(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_readdir_linux_x86_64(emitter);
        return;
    }

    let name_off = emitter.platform.dirent_name_offset();
    emitter.blank();
    emitter.comment("--- runtime: readdir through StreamState backend ownership ---");
    emitter.label_global("__rt_readdir");
    emitter.instruction("sub sp, sp, #16");                                     // preserve the caller frame around nested runtime calls
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("cbz x0, __rt_readdir_end");                            // a missing StreamState has no directory entry
    emitter.instruction(&format!(
        "ldr x9, [x0, #{}]", STREAM_BACKEND_KIND_OFFSET
    ));                                                                         // load the typed directory backend discriminator
    emitter.instruction(&format!(
        "ldr x10, [x0, #{}]", STREAM_BACKEND_AUX_OFFSET
    ));                                                                         // load native or glob backend ownership
    emitter.instruction(&format!(
        "cmp x9, #{}", STREAM_BACKEND_GLOB_DIRECTORY
    ));                                                                         // is this an owned glob iterator?
    emitter.instruction("b.eq __rt_readdir_glob");                              // iterate the glob state directly
    emitter.instruction(&format!(
        "cmp x9, #{}", STREAM_BACKEND_USER_DIRECTORY
    ));                                                                         // is this a userspace directory wrapper?
    emitter.instruction("b.eq __rt_readdir_user");                              // dispatch its directory callback by synthetic handle
    emitter.instruction(&format!(
        "cmp x9, #{}", STREAM_BACKEND_DIRECTORY
    ));                                                                         // is this a native libc directory?
    emitter.instruction("b.ne __rt_readdir_end");                               // reject non-directory stream backends
    emitter.instruction("mov x0, x10");                                         // pass the owning DIR* to libc readdir
    emitter.bl_c("readdir");
    emitter.instruction("cbz x0, __rt_readdir_end");                            // NULL means end of directory
    emitter.instruction(&format!("add x1, x0, #{}", name_off));                 // point at the platform dirent d_name field
    emitter.instruction("mov x2, #0");                                          // begin measuring the entry-name byte length
    emitter.label("__rt_readdir_native_strlen");
    emitter.instruction("ldrb w9, [x1, x2]");                                   // load the next entry-name byte
    emitter.instruction("cbz w9, __rt_readdir_persist");                        // stop at the C-string terminator
    emitter.instruction("add x2, x2, #1");                                      // count one more entry-name byte
    emitter.instruction("b __rt_readdir_native_strlen");                        // continue scanning the native name

    emitter.label("__rt_readdir_glob");
    emitter.instruction("cbz x10, __rt_readdir_end");                           // a detached glob owner is already exhausted
    emitter.instruction("ldr x11, [x10, #8]");                                  // load the glob match count
    emitter.instruction("ldr x12, [x10, #16]");                                 // load the current glob iteration index
    emitter.instruction("cmp x12, x11");                                        // has iteration reached the match count?
    emitter.instruction("b.hs __rt_readdir_end");                               // report end once every match was consumed
    emitter.instruction("ldr x13, [x10, #0]");                                  // load the glob path-vector pointer
    emitter.instruction("ldr x1, [x13, x12, lsl #3]");                          // select the current matched path
    emitter.instruction("add x12, x12, #1");                                    // advance the glob iterator
    emitter.instruction("str x12, [x10, #16]");                                 // persist the next glob index
    emitter.instruction("mov x2, #0");                                          // begin measuring the matched path length
    emitter.label("__rt_readdir_glob_strlen");
    emitter.instruction("ldrb w9, [x1, x2]");                                   // load the next matched-path byte
    emitter.instruction("cbz w9, __rt_readdir_persist");                        // stop at the path terminator
    emitter.instruction("add x2, x2, #1");                                      // count one more matched-path byte
    emitter.instruction("b __rt_readdir_glob_strlen");                          // continue scanning the matched path

    emitter.label("__rt_readdir_user");
    emitter.instruction(&format!("ldr x0, [x0, #{}]", STREAM_FD_OFFSET));       // load the wrapper's synthetic directory handle
    emitter.instruction("bl __rt_user_wrapper_dir_readdir");                    // invoke the userspace dir_readdir callback
    emitter.instruction("b __rt_readdir_ret");                                  // wrapper results already use the string-or-false ABI

    emitter.label("__rt_readdir_persist");
    emitter.instruction("bl __rt_str_persist");                                 // copy the selected name beyond the backend's lifetime
    emitter.instruction("b __rt_readdir_ret");                                  // skip the end-of-directory false marker
    emitter.label("__rt_readdir_end");
    emitter.instruction("mov x1, #0");                                          // null string pointer encodes PHP false
    emitter.instruction("mov x2, #0");                                          // false carries no string length
    emitter.label("__rt_readdir_ret");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the entry string or false marker
}

/// Emits Linux x86_64 typed directory iteration.
fn emit_readdir_linux_x86_64(emitter: &mut Emitter) {
    let name_off = emitter.platform.dirent_name_offset();
    emitter.blank();
    emitter.comment("--- runtime: readdir through StreamState backend ownership ---");
    emitter.label_global("__rt_readdir");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("test rdi, rdi");                                       // is an authoritative StreamState available?
    emitter.instruction("jz __rt_readdir_end_x86");                             // a missing state has no directory entry
    emitter.instruction(&format!(
        "mov r9, QWORD PTR [rdi + {}]", STREAM_BACKEND_KIND_OFFSET
    ));                                                                         // load the typed directory backend discriminator
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [rdi + {}]", STREAM_BACKEND_AUX_OFFSET
    ));                                                                         // load native or glob backend ownership
    emitter.instruction(&format!(
        "cmp r9, {}", STREAM_BACKEND_GLOB_DIRECTORY
    ));                                                                         // is this an owned glob iterator?
    emitter.instruction("je __rt_readdir_glob_x86");                            // iterate the glob state directly
    emitter.instruction(&format!(
        "cmp r9, {}", STREAM_BACKEND_USER_DIRECTORY
    ));                                                                         // is this a userspace directory wrapper?
    emitter.instruction("je __rt_readdir_user_x86");                            // dispatch its callback by synthetic handle
    emitter.instruction(&format!("cmp r9, {}", STREAM_BACKEND_DIRECTORY));      // is this a native libc directory?
    emitter.instruction("jne __rt_readdir_end_x86");                            // reject non-directory stream backends
    emitter.instruction("mov rdi, r10");                                        // pass the owning DIR* to libc readdir
    emitter.bl_c("readdir");
    emitter.instruction("test rax, rax");                                       // did libc return a directory entry?
    emitter.instruction("jz __rt_readdir_end_x86");                             // NULL means end of directory
    emitter.instruction(&format!("lea rsi, [rax + {}]", name_off));             // point at the platform dirent d_name field
    emitter.instruction("xor edx, edx");                                        // begin measuring the entry-name byte length
    emitter.label("__rt_readdir_native_strlen_x86");
    emitter.instruction("cmp BYTE PTR [rsi + rdx], 0");                         // test the next entry-name byte
    emitter.instruction("je __rt_readdir_persist_x86");                         // stop at the C-string terminator
    emitter.instruction("add rdx, 1");                                          // count one more entry-name byte
    emitter.instruction("jmp __rt_readdir_native_strlen_x86");                  // continue scanning the native name

    emitter.label("__rt_readdir_glob_x86");
    emitter.instruction("test r10, r10");                                       // is the glob owner still attached?
    emitter.instruction("jz __rt_readdir_end_x86");                             // detached glob state is already exhausted
    emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                        // load the glob match count
    emitter.instruction("mov rdx, QWORD PTR [r10 + 16]");                       // load the current glob iteration index
    emitter.instruction("cmp rdx, r11");                                        // has iteration reached the match count?
    emitter.instruction("jae __rt_readdir_end_x86");                            // report end once every match was consumed
    emitter.instruction("mov r8, QWORD PTR [r10]");                             // load the glob path-vector pointer
    emitter.instruction("mov rsi, QWORD PTR [r8 + rdx * 8]");                   // select the current matched path
    emitter.instruction("add rdx, 1");                                          // advance the glob iterator
    emitter.instruction("mov QWORD PTR [r10 + 16], rdx");                       // persist the next glob index
    emitter.instruction("xor edx, edx");                                        // begin measuring the matched path length
    emitter.label("__rt_readdir_glob_strlen_x86");
    emitter.instruction("cmp BYTE PTR [rsi + rdx], 0");                         // test the next matched-path byte
    emitter.instruction("je __rt_readdir_persist_x86");                         // stop at the path terminator
    emitter.instruction("add rdx, 1");                                          // count one more matched-path byte
    emitter.instruction("jmp __rt_readdir_glob_strlen_x86");                    // continue scanning the matched path

    emitter.label("__rt_readdir_user_x86");
    emitter.instruction(&format!(
        "mov rdi, QWORD PTR [rdi + {}]", STREAM_FD_OFFSET
    ));                                                                         // load the wrapper's synthetic directory handle
    emitter.instruction("call __rt_user_wrapper_dir_readdir");                  // invoke the userspace dir_readdir callback
    emitter.instruction("jmp __rt_readdir_ret_x86");                            // wrapper results already use the string-or-false ABI
    emitter.label("__rt_readdir_persist_x86");
    emitter.instruction("mov rax, rsi");                                        // pass the selected string pointer to persistence
    emitter.instruction("call __rt_str_persist");                               // copy the selected name beyond the backend's lifetime
    emitter.instruction("jmp __rt_readdir_ret_x86");                            // skip the end-of-directory false marker
    emitter.label("__rt_readdir_end_x86");
    emitter.instruction("xor eax, eax");                                        // null string pointer encodes PHP false
    emitter.instruction("xor edx, edx");                                        // false carries no string length
    emitter.label("__rt_readdir_ret_x86");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the entry string or false marker
}
