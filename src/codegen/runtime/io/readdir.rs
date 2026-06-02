//! Purpose:
//! Emits the `__rt_readdir` runtime helper, which reads the next entry name
//! from a directory stream opened by `opendir()`.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//!
//! Key details:
//! - The `DIR*` recorded by `__rt_opendir` in `_dir_handles` is handed to libc
//!   `readdir`; the `d_name` field is copied to the heap so the name survives
//!   the next `readdir`/`closedir` call.
//! - A null pointer result marks end-of-directory and is boxed as PHP `false`.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// readdir: read the next directory entry name.
/// Input:  AArch64 x0 = directory descriptor / x86_64 rdi = directory descriptor
/// Output: AArch64 x1/x2 = entry name / x86_64 rax/rdx = entry name
///         a null pointer marks end-of-directory
pub fn emit_readdir(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_readdir_linux_x86_64(emitter);
        return;
    }

    let name_off = emitter.platform.dirent_name_offset();

    emitter.blank();
    emitter.comment("--- runtime: readdir ---");
    emitter.label_global("__rt_readdir");

    emitter.instruction("sub sp, sp, #16");                                     // minimal frame across the libc/runtime calls
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer

    // -- glob:// path takes precedence: probe _glob_handles[fd] first --
    abi::emit_symbol_address(emitter, "x9", "_glob_handles");
    emitter.instruction("ldr x10, [x9, x0, lsl #3]");                           // glob_state* = _glob_handles[fd]
    emitter.instruction("cbz x10, __rt_readdir_libc");                          // no glob handle: fall through to libc readdir
    // glob_state layout: [0)=gl_pathv, [8)=gl_pathc, [16)=index.
    emitter.instruction("ldr x11, [x10, #8]");                                  // load gl_pathc (match count)
    emitter.instruction("ldr x12, [x10, #16]");                                 // load current iteration index
    emitter.instruction("cmp x12, x11");                                        // exhausted the match list?
    emitter.instruction("b.hs __rt_readdir_end");                               // yes: report end of directory
    emitter.instruction("ldr x13, [x10, #0]");                                  // load gl_pathv (char**)
    emitter.instruction("ldr x1, [x13, x12, lsl #3]");                          // path pointer = pathv[index]
    emitter.instruction("add x12, x12, #1");                                    // advance the iterator
    emitter.instruction("str x12, [x10, #16]");                                 // persist the new index
    emitter.instruction("mov x2, #0");                                          // length counter for the strlen scan
    emitter.label("__rt_readdir_glob_strlen");
    emitter.instruction("ldrb w9, [x1, x2]");                                   // load the next path byte
    emitter.instruction("cbz w9, __rt_readdir_glob_ready");                     // stop at the NUL terminator
    emitter.instruction("add x2, x2, #1");                                      // advance the length counter
    emitter.instruction("b __rt_readdir_glob_strlen");                          // continue scanning
    emitter.label("__rt_readdir_glob_ready");
    emitter.instruction("bl __rt_str_persist");                                 // copy the path to the heap, x1 = ptr, x2 = len
    emitter.instruction("b __rt_readdir_ret");                                  // skip the libc path

    emitter.label("__rt_readdir_libc");
    // -- look up the DIR* recorded for this descriptor --
    abi::emit_symbol_address(emitter, "x9", "_dir_handles");
    emitter.instruction("ldr x10, [x9, x0, lsl #3]");                           // DIR* = _dir_handles[fd]
    emitter.instruction("cbz x10, __rt_readdir_end");                           // no handle recorded: report end of directory
    emitter.instruction("mov x0, x10");                                         // DIR* argument for readdir
    emitter.bl_c("readdir");
    emitter.instruction("cbz x0, __rt_readdir_end");                            // a NULL dirent means no more entries

    // -- point at d_name and measure it until the terminating NUL --
    emitter.instruction(&format!("add x1, x0, #{}", name_off));                 // x1 = pointer to dirent.d_name
    emitter.instruction("mov x2, #0");                                          // x2 = directory entry name length
    emitter.label("__rt_readdir_strlen");
    emitter.instruction("ldrb w9, [x1, x2]");                                   // load the next d_name byte
    emitter.instruction("cbz w9, __rt_readdir_ready");                          // stop at the terminating NUL byte
    emitter.instruction("add x2, x2, #1");                                      // count one more entry name byte
    emitter.instruction("b __rt_readdir_strlen");                               // continue scanning the entry name
    emitter.label("__rt_readdir_ready");

    // -- copy the name to the heap so it survives the next readdir/closedir --
    emitter.instruction("bl __rt_str_persist");                                 // copy the name to the heap, x1 = ptr, x2 = len
    emitter.instruction("b __rt_readdir_ret");                                  // skip the end-of-directory path

    emitter.label("__rt_readdir_end");
    emitter.instruction("mov x1, #0");                                          // a null pointer is boxed as PHP false
    emitter.instruction("mov x2, #0");                                          // the false marker carries no string length

    emitter.label("__rt_readdir_ret");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the frame
    emitter.instruction("ret");                                                 // return the entry name or the false marker
}

fn emit_readdir_linux_x86_64(emitter: &mut Emitter) {
    let name_off = emitter.platform.dirent_name_offset();

    emitter.blank();
    emitter.comment("--- runtime: readdir ---");
    emitter.label_global("__rt_readdir");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer

    // -- glob:// path takes precedence: probe _glob_handles[fd] first --
    emitter.instruction("lea r9, [rip + _glob_handles]");                       // base of the fd → glob_state pointer table
    emitter.instruction("mov r10, QWORD PTR [r9 + rdi * 8]");                   // glob_state* = _glob_handles[fd]
    emitter.instruction("test r10, r10");                                       // glob handle present?
    emitter.instruction("jz __rt_readdir_libc_x86");                            // no glob handle: fall through to libc readdir
    // glob_state layout: [0)=gl_pathv, [8)=gl_pathc, [16)=index.
    emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                        // load gl_pathc (match count)
    emitter.instruction("mov rdx, QWORD PTR [r10 + 16]");                       // load current iteration index
    emitter.instruction("cmp rdx, r11");                                        // exhausted the match list?
    emitter.instruction("jae __rt_readdir_end_x86");                            // yes: report end of directory
    emitter.instruction("mov r8, QWORD PTR [r10 + 0]");                         // load gl_pathv (char**)
    emitter.instruction("mov rsi, QWORD PTR [r8 + rdx * 8]");                   // path pointer = pathv[index]
    emitter.instruction("add rdx, 1");                                          // advance the iterator
    emitter.instruction("mov QWORD PTR [r10 + 16], rdx");                       // persist the new index
    emitter.instruction("xor rdx, rdx");                                        // length counter for the strlen scan
    emitter.label("__rt_readdir_glob_strlen_x86");
    emitter.instruction("mov r8b, BYTE PTR [rsi + rdx]");                       // load the next path byte
    emitter.instruction("test r8b, r8b");                                       // stop at the NUL terminator
    emitter.instruction("jz __rt_readdir_glob_ready_x86");                      // length now known
    emitter.instruction("add rdx, 1");                                          // advance the length counter
    emitter.instruction("jmp __rt_readdir_glob_strlen_x86");                    // continue scanning
    emitter.label("__rt_readdir_glob_ready_x86");
    emitter.instruction("mov rax, rsi");                                        // path pointer into __rt_str_persist's input register
    emitter.instruction("call __rt_str_persist");                               // copy the path to the heap, rax = ptr, rdx = len
    emitter.instruction("jmp __rt_readdir_ret_x86");                            // skip the libc path

    emitter.label("__rt_readdir_libc_x86");
    // -- look up the DIR* recorded for this descriptor --
    emitter.instruction("lea r9, [rip + _dir_handles]");                        // base of the fd->DIR* table
    emitter.instruction("mov r10, QWORD PTR [r9 + rdi * 8]");                   // DIR* = _dir_handles[fd]
    emitter.instruction("test r10, r10");                                       // was a DIR* recorded for this descriptor?
    emitter.instruction("jz __rt_readdir_end_x86");                             // no handle recorded: report end of directory
    emitter.instruction("mov rdi, r10");                                        // DIR* argument for readdir
    emitter.bl_c("readdir");
    emitter.instruction("test rax, rax");                                       // a NULL dirent means no more entries
    emitter.instruction("jz __rt_readdir_end_x86");                             // report end of directory once entries run out

    // -- point at d_name and measure it until the terminating NUL --
    emitter.instruction(&format!("lea rsi, [rax + {}]", name_off));             // rsi = pointer to dirent.d_name
    emitter.instruction("xor edx, edx");                                        // rdx = directory entry name length
    emitter.label("__rt_readdir_strlen_x86");
    emitter.instruction("mov r8b, BYTE PTR [rsi + rdx]");                       // load the next d_name byte
    emitter.instruction("test r8b, r8b");                                       // stop at the terminating NUL byte
    emitter.instruction("jz __rt_readdir_ready_x86");                           // the entry name length is now known
    emitter.instruction("add rdx, 1");                                          // count one more entry name byte
    emitter.instruction("jmp __rt_readdir_strlen_x86");                         // continue scanning the entry name
    emitter.label("__rt_readdir_ready_x86");

    // -- copy the name to the heap so it survives the next readdir/closedir --
    emitter.instruction("mov rax, rsi");                                        // d_name pointer into the str_persist input register
    emitter.instruction("call __rt_str_persist");                               // copy the name to the heap, rax = ptr, rdx = len
    emitter.instruction("jmp __rt_readdir_ret_x86");                            // skip the end-of-directory path

    emitter.label("__rt_readdir_end_x86");
    emitter.instruction("xor eax, eax");                                        // a null pointer is boxed as PHP false
    emitter.instruction("xor edx, edx");                                        // the false marker carries no string length

    emitter.label("__rt_readdir_ret_x86");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the entry name or the false marker
}
