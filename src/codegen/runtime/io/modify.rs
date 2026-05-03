use crate::codegen::{emit::Emitter, platform::Arch};

/// File-modification helpers: touch / chmod / chown / chgrp / umask /
/// ftruncate / fflush / fsync / fdatasync.
///
/// All of these go through libc rather than the raw-syscall path used by
/// `fs.rs`, because:
/// - libc gives us a single ABI on both Darwin arm64 and Linux arm64 without
///   needing additional `linux_transform` syscall remappings;
/// - macOS lacks a `fdatasync` syscall, so we transparently fall back to
///   `fsync` there;
/// - `utimensat` (used by `__rt_touch`) is the modern portable API and avoids
///   the legacy `utimes`/`utime` zoo.
pub fn emit_modify(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_modify_linux_x86_64(emitter);
        return;
    }

    // ================================================================
    // __rt_chmod: chmod(path, mode)
    // Input:  x1/x2 = path, x3 = mode
    // Output: x0 = 1 on success, 0 on failure
    // ================================================================
    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment after preceding runtime literals
    emitter.comment("--- runtime: chmod ---");
    emitter.label_global("__rt_chmod");
    emitter.instruction("sub sp, sp, #32");                                     // allocate frame + spill slot for mode
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish new frame pointer
    emitter.instruction("str x3, [sp, #0]");                                    // preserve the mode value across the cstr call
    emitter.instruction("bl __rt_cstr");                                        // path → null-terminated C string in x0
    emitter.instruction("ldr x1, [sp, #0]");                                    // restore mode into the second libc argument
    emitter.bl_c("chmod");                                                      // libc chmod(path, mode)
    emitter.instruction("cmp x0, #0");                                          // success?
    emitter.instruction("cset x0, eq");                                         // x0 = 1 if chmod succeeded
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate frame
    emitter.instruction("ret");                                                 // return predicate

    // ================================================================
    // __rt_chown: chown(path, uid, gid=-1)  (chgrp uses -1 for uid)
    // Input:  x1/x2 = path, x3 = uid, x4 = gid (use -1 for "leave alone")
    // Output: x0 = 1 on success, 0 on failure
    // ================================================================
    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment for the next runtime helper
    emitter.comment("--- runtime: chown ---");
    emitter.label_global("__rt_chown");
    emitter.instruction("sub sp, sp, #32");                                     // allocate frame + spill slots for uid/gid
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish new frame pointer
    emitter.instruction("stp x3, x4, [sp, #0]");                                // preserve uid/gid across the cstr call
    emitter.instruction("bl __rt_cstr");                                        // path → C string in x0
    emitter.instruction("ldp x1, x2, [sp, #0]");                                // restore uid/gid into the libc argument registers
    emitter.bl_c("chown");                                                      // libc chown(path, uid, gid)
    emitter.instruction("cmp x0, #0");                                          // success?
    emitter.instruction("cset x0, eq");                                         // x0 = 1 if chown succeeded
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate frame
    emitter.instruction("ret");                                                 // return predicate

    // ================================================================
    // __rt_chown_user: resolve a user name via getpwnam(), then chown(path, uid, -1)
    // Input:  x1/x2 = path, x3/x4 = user name
    // Output: x0 = 1 on success, 0 on failure
    // ================================================================
    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment for the next runtime helper
    emitter.comment("--- runtime: chown user name ---");
    emitter.label_global("__rt_chown_user");
    emitter.instruction("sub sp, sp, #48");                                     // allocate frame + spill slots for path and user strings
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish new frame pointer
    emitter.instruction("stp x3, x4, [sp, #16]");                               // preserve user-name ptr/len across path conversion
    emitter.instruction("bl __rt_cstr");                                        // path → C string in x0
    emitter.instruction("str x0, [sp, #0]");                                    // save C path pointer
    emitter.instruction("ldp x1, x2, [sp, #16]");                               // reload user-name ptr/len
    emitter.instruction("bl __rt_cstr2");                                       // user name → secondary C string in x0
    emitter.bl_c("getpwnam");                                                   // libc getpwnam(name)
    emitter.instruction("cbz x0, __rt_chown_user_fail");                        // unknown user name → false
    emitter.instruction("ldr w1, [x0, #16]");                                   // load passwd.pw_uid
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload C path pointer
    emitter.instruction("mov x2, #-1");                                         // gid = -1 (leave group unchanged)
    emitter.bl_c("chown");                                                      // libc chown(path, uid, -1)
    emitter.instruction("cmp x0, #0");                                          // success?
    emitter.instruction("cset x0, eq");                                         // x0 = 1 if chown succeeded
    emitter.instruction("b __rt_chown_user_done");                              // skip failure return
    emitter.label("__rt_chown_user_fail");
    emitter.instruction("mov x0, #0");                                          // unknown name returns false
    emitter.label("__rt_chown_user_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate frame
    emitter.instruction("ret");                                                 // return predicate

    // ================================================================
    // __rt_chgrp_group: resolve a group name via getgrnam(), then chown(path, -1, gid)
    // Input:  x1/x2 = path, x3/x4 = group name
    // Output: x0 = 1 on success, 0 on failure
    // ================================================================
    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment for the next runtime helper
    emitter.comment("--- runtime: chgrp group name ---");
    emitter.label_global("__rt_chgrp_group");
    emitter.instruction("sub sp, sp, #48");                                     // allocate frame + spill slots for path and group strings
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish new frame pointer
    emitter.instruction("stp x3, x4, [sp, #16]");                               // preserve group-name ptr/len across path conversion
    emitter.instruction("bl __rt_cstr");                                        // path → C string in x0
    emitter.instruction("str x0, [sp, #0]");                                    // save C path pointer
    emitter.instruction("ldp x1, x2, [sp, #16]");                               // reload group-name ptr/len
    emitter.instruction("bl __rt_cstr2");                                       // group name → secondary C string in x0
    emitter.bl_c("getgrnam");                                                   // libc getgrnam(name)
    emitter.instruction("cbz x0, __rt_chgrp_group_fail");                       // unknown group name → false
    emitter.instruction("ldr w2, [x0, #16]");                                   // load group.gr_gid
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload C path pointer
    emitter.instruction("mov x1, #-1");                                         // uid = -1 (leave owner unchanged)
    emitter.bl_c("chown");                                                      // libc chown(path, -1, gid)
    emitter.instruction("cmp x0, #0");                                          // success?
    emitter.instruction("cset x0, eq");                                         // x0 = 1 if chown succeeded
    emitter.instruction("b __rt_chgrp_group_done");                             // skip failure return
    emitter.label("__rt_chgrp_group_fail");
    emitter.instruction("mov x0, #0");                                          // unknown name returns false
    emitter.label("__rt_chgrp_group_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate frame
    emitter.instruction("ret");                                                 // return predicate

    // ================================================================
    // __rt_umask: umask(mask) — sets new umask, returns previous
    // Input:  x0 = new mask value
    // Output: x0 = previous umask
    // ================================================================
    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment for the next runtime helper
    emitter.comment("--- runtime: umask ---");
    emitter.label_global("__rt_umask");
    emitter.instruction("sub sp, sp, #16");                                     // allocate minimal frame
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish new frame pointer
    emitter.bl_c("umask");                                                      // libc umask(mask) — returns previous mask in x0
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // deallocate frame
    emitter.instruction("ret");                                                 // return previous umask

    // ================================================================
    // __rt_ftruncate: ftruncate(fd, size)
    // Input:  x0 = fd, x1 = new size
    // Output: x0 = 1 on success, 0 on failure
    // ================================================================
    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment for the next runtime helper
    emitter.comment("--- runtime: ftruncate ---");
    emitter.label_global("__rt_ftruncate");
    emitter.instruction("sub sp, sp, #16");                                     // allocate minimal frame
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish new frame pointer
    emitter.bl_c("ftruncate");                                                  // libc ftruncate(fd, size)
    emitter.instruction("cmp x0, #0");                                          // success?
    emitter.instruction("cset x0, eq");                                         // x0 = 1 if ftruncate succeeded
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // deallocate frame
    emitter.instruction("ret");                                                 // return predicate

    // ================================================================
    // __rt_fsync: fsync(fd)  (also used by __rt_fflush)
    // Input:  x0 = fd
    // Output: x0 = 1 on success, 0 on failure
    // ================================================================
    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment for the next runtime helper
    emitter.comment("--- runtime: fsync ---");
    emitter.label_global("__rt_fsync");
    emitter.label_global("__rt_fflush");
    emitter.instruction("sub sp, sp, #16");                                     // allocate minimal frame
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish new frame pointer
    emitter.bl_c("fsync");                                                      // libc fsync(fd)
    emitter.instruction("cmp x0, #0");                                          // success?
    emitter.instruction("cset x0, eq");                                         // x0 = 1 if fsync succeeded
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // deallocate frame
    emitter.instruction("ret");                                                 // return predicate

    // ================================================================
    // __rt_fdatasync: fdatasync(fd) — Darwin lacks the function, so we
    // fall back to fsync there. On Linux libc fdatasync exists.
    // Input:  x0 = fd
    // Output: x0 = 1 on success, 0 on failure
    // ================================================================
    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment for the next runtime helper
    emitter.comment("--- runtime: fdatasync ---");
    emitter.label_global("__rt_fdatasync");
    emitter.instruction("sub sp, sp, #16");                                     // allocate minimal frame
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish new frame pointer
    if emitter.platform == crate::codegen::platform::Platform::Linux {
        emitter.bl_c("fdatasync");                                              // libc fdatasync(fd) on Linux
    } else {
        emitter.bl_c("fsync");                                                  // Darwin fallback: fsync flushes data and metadata, satisfying the fdatasync contract
    }
    emitter.instruction("cmp x0, #0");                                          // success?
    emitter.instruction("cset x0, eq");                                         // x0 = 1 if sync succeeded
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // deallocate frame
    emitter.instruction("ret");                                                 // return predicate

    // ================================================================
    // __rt_touch: touch(path, mtime, atime, current_mask)
    // Input:  x1/x2 = path, x3 = mtime, x4 = atime, x5 bit0/bit1 = atime/mtime current
    // Output: x0 = 1 on success, 0 on failure
    //
    // Implementation: opens the file with O_WRONLY|O_CREAT to create it if
    // missing, closes the descriptor, then sets the access/modification
    // timestamps via libc utimensat with AT_FDCWD.
    //
    // Frame layout (64 bytes):
    //   sp+ 0  : path cstr pointer
    //   sp+ 8  : mtime
    //   sp+16  : atime
    //   sp+24  : current-time mask
    //   sp+32  : timespec[0] = atime  (.tv_sec=8, .tv_nsec=8)
    //   sp+48  : timespec[1] = mtime
    //   sp+? saved frame regs at end
    // ================================================================
    let frame = 80usize;
    let save_off = frame - 16;
    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment for the next runtime helper
    emitter.comment("--- runtime: touch ---");
    emitter.label_global("__rt_touch");
    emitter.instruction(&format!("sub sp, sp, #{}", frame));                    // allocate frame + timespec[2] + spill slots
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", save_off));         // save frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{}", save_off));                // establish new frame pointer
    emitter.instruction("str x3, [sp, #8]");                                    // save mtime arg
    emitter.instruction("str x4, [sp, #16]");                                   // save atime arg
    emitter.instruction("str x5, [sp, #24]");                                   // save current-time mask
    emitter.instruction("bl __rt_cstr");                                        // path → C string in x0
    emitter.instruction("str x0, [sp, #0]");                                    // save C path pointer

    // -- create the file if missing via open(path, O_WRONLY|O_CREAT, 0666) --
    // Use the raw syscall (#5) rather than libc open() because Darwin's
    // ARM64 ABI passes variadic libc args on the stack: open()'s third
    // mode argument would be ignored when set in x2, leaving the kernel
    // to read garbage and create the file with bogus permissions.
    let plat = emitter.platform;
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload C path pointer for the open syscall
    emitter.instruction(&format!("mov x1, #0x{:X}", plat.o_wronly_creat()));    // O_WRONLY|O_CREAT without truncating existing files
    emitter.instruction("mov x2, #0x1B6");                                      // mode 0666 before umask
    emitter.syscall(5);                                                         // sys_open: returns fd in x0 (errno on failure)
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: negative return = error
    }
    emitter.instruction(&plat.branch_on_syscall_success("__rt_touch_close_fd")); // success: go close the fresh fd
    emitter.instruction("b __rt_touch_set_times");                              // failure: skip close, still try to stamp existing file
    emitter.label("__rt_touch_close_fd");
    emitter.syscall(6);                                                         // sys_close: release the freshly created fd

    emitter.label("__rt_touch_set_times");
    // Build timespec[2]: atime at sp+32, mtime at sp+48
    let utime_now = plat.utime_now_nsec();
    emitter.instruction("ldr x10, [sp, #24]");                                  // load current-time mask
    emitter.instruction("tbnz x10, #0, __rt_touch_atime_now");                  // use current time for atime?
    emitter.instruction("ldr x9, [sp, #16]");                                   // load explicit atime seconds
    emitter.instruction("str x9, [sp, #32]");                                   // tv_sec = atime
    emitter.instruction("str xzr, [sp, #40]");                                  // tv_nsec = 0
    emitter.instruction("b __rt_touch_handle_mtime");                           // proceed to mtime
    emitter.label("__rt_touch_atime_now");
    emitter.instruction("str xzr, [sp, #32]");                                  // tv_sec = 0 (ignored when nsec is UTIME_NOW)
    emitter.instruction(&format!("mov x9, #{}", utime_now));                    // platform UTIME_NOW sentinel
    emitter.instruction("str x9, [sp, #40]");                                   // tv_nsec = UTIME_NOW
    emitter.instruction("b __rt_touch_handle_mtime");                           // proceed to mtime

    emitter.label("__rt_touch_handle_mtime");
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload current-time mask
    emitter.instruction("tbnz x10, #1, __rt_touch_mtime_now");                  // use current time for mtime?
    emitter.instruction("ldr x9, [sp, #8]");                                    // load explicit mtime seconds
    emitter.instruction("str x9, [sp, #48]");                                   // tv_sec = mtime
    emitter.instruction("str xzr, [sp, #56]");                                  // tv_nsec = 0
    emitter.instruction("b __rt_touch_call_utimensat");                         // proceed to syscall
    emitter.label("__rt_touch_mtime_now");
    emitter.instruction("str xzr, [sp, #48]");                                  // tv_sec = 0
    emitter.instruction(&format!("mov x9, #{}", utime_now));                    // platform UTIME_NOW sentinel
    emitter.instruction("str x9, [sp, #56]");                                   // tv_nsec = UTIME_NOW
    emitter.instruction("b __rt_touch_call_utimensat");                         // proceed to syscall

    emitter.label("__rt_touch_call_utimensat");
    emitter.instruction(&format!("mov x0, #{}", plat.at_fdcwd()));              // AT_FDCWD (platform-dependent: -2 Darwin, -100 Linux)
    emitter.instruction("ldr x1, [sp, #0]");                                    // C path pointer
    emitter.instruction("add x2, sp, #32");                                     // pointer to timespec[2]
    emitter.instruction("mov x3, #0");                                          // flags = 0 (follow symlinks)
    emitter.bl_c("utimensat");                                                  // libc utimensat(AT_FDCWD, path, times, 0)
    emitter.instruction("cmp x0, #0");                                          // success?
    emitter.instruction("cset x0, eq");                                         // x0 = 1 if utimensat succeeded
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_off));         // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", frame));                    // deallocate frame
    emitter.instruction("ret");                                                 // return predicate
}

fn emit_modify_linux_x86_64(emitter: &mut Emitter) {
    // -- chmod --
    emitter.blank();
    emitter.comment("--- runtime: chmod ---");
    emitter.label_global("__rt_chmod");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish frame
    emitter.instruction("sub rsp, 16");                                         // align stack
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve mode (came in via the secondary string-argument register)
    emitter.instruction("call __rt_cstr");                                      // path → C string in rax
    emitter.instruction("mov rdi, rax");                                        // first libc chmod arg = C path
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // second libc chmod arg = mode
    emitter.instruction("call chmod");                                          // libc chmod(path, mode)
    emitter.instruction("cmp rax, 0");                                          // success?
    emitter.instruction("sete al");                                             // boolean byte
    emitter.instruction("movzx rax, al");                                       // widen to canonical integer result
    emitter.instruction("add rsp, 16");                                         // release stack
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return predicate

    // -- chown --
    emitter.blank();
    emitter.comment("--- runtime: chown ---");
    emitter.label_global("__rt_chown");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish frame
    emitter.instruction("sub rsp, 32");                                         // align stack + spill uid/gid
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve uid
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve gid
    emitter.instruction("call __rt_cstr");                                      // path → C string in rax
    emitter.instruction("mov rdi, rax");                                        // first libc chown arg = path
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // second arg = uid
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // third arg = gid
    emitter.instruction("call chown");                                          // libc chown(path, uid, gid)
    emitter.instruction("cmp rax, 0");                                          // success?
    emitter.instruction("sete al");                                             // boolean byte
    emitter.instruction("movzx rax, al");                                       // widen
    emitter.instruction("add rsp, 32");                                         // release stack
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return predicate

    // -- chown by user name --
    emitter.blank();
    emitter.comment("--- runtime: chown user name ---");
    emitter.label_global("__rt_chown_user");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish frame
    emitter.instruction("sub rsp, 32");                                         // align stack + spill user string and path pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // preserve user-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // preserve user-name length
    emitter.instruction("call __rt_cstr");                                      // path → C string in rax
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save C path pointer
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload user-name pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload user-name length
    emitter.instruction("call __rt_cstr2");                                     // user name → secondary C string in rax
    emitter.instruction("mov rdi, rax");                                        // first getpwnam arg = C user name
    emitter.instruction("call getpwnam");                                       // libc getpwnam(name)
    emitter.instruction("test rax, rax");                                       // user found?
    emitter.instruction("jz __rt_chown_user_fail_x86");                         // unknown user name → false
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // first chown arg = C path
    emitter.instruction("mov esi, DWORD PTR [rax + 16]");                       // second chown arg = passwd.pw_uid
    emitter.instruction("mov rdx, -1");                                         // gid = -1 (leave group unchanged)
    emitter.instruction("call chown");                                          // libc chown(path, uid, -1)
    emitter.instruction("cmp rax, 0");                                          // success?
    emitter.instruction("sete al");                                             // boolean byte
    emitter.instruction("movzx rax, al");                                       // widen
    emitter.instruction("jmp __rt_chown_user_done_x86");                        // skip failure return
    emitter.label("__rt_chown_user_fail_x86");
    emitter.instruction("xor eax, eax");                                        // unknown name returns false
    emitter.label("__rt_chown_user_done_x86");
    emitter.instruction("add rsp, 32");                                         // release stack
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return predicate

    // -- chgrp by group name --
    emitter.blank();
    emitter.comment("--- runtime: chgrp group name ---");
    emitter.label_global("__rt_chgrp_group");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish frame
    emitter.instruction("sub rsp, 32");                                         // align stack + spill group string and path pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // preserve group-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // preserve group-name length
    emitter.instruction("call __rt_cstr");                                      // path → C string in rax
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save C path pointer
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload group-name pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload group-name length
    emitter.instruction("call __rt_cstr2");                                     // group name → secondary C string in rax
    emitter.instruction("mov rdi, rax");                                        // first getgrnam arg = C group name
    emitter.instruction("call getgrnam");                                       // libc getgrnam(name)
    emitter.instruction("test rax, rax");                                       // group found?
    emitter.instruction("jz __rt_chgrp_group_fail_x86");                        // unknown group name → false
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // first chown arg = C path
    emitter.instruction("mov rsi, -1");                                         // uid = -1 (leave owner unchanged)
    emitter.instruction("mov edx, DWORD PTR [rax + 16]");                       // third chown arg = group.gr_gid
    emitter.instruction("call chown");                                          // libc chown(path, -1, gid)
    emitter.instruction("cmp rax, 0");                                          // success?
    emitter.instruction("sete al");                                             // boolean byte
    emitter.instruction("movzx rax, al");                                       // widen
    emitter.instruction("jmp __rt_chgrp_group_done_x86");                       // skip failure return
    emitter.label("__rt_chgrp_group_fail_x86");
    emitter.instruction("xor eax, eax");                                        // unknown name returns false
    emitter.label("__rt_chgrp_group_done_x86");
    emitter.instruction("add rsp, 32");                                         // release stack
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return predicate

    // -- umask --
    emitter.blank();
    emitter.comment("--- runtime: umask ---");
    emitter.label_global("__rt_umask");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish frame
    emitter.instruction("mov rdi, rax");                                        // mask comes in via the int return register
    emitter.instruction("call umask");                                          // libc umask(mask) — returns previous mask
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return previous mask

    // -- ftruncate --
    emitter.blank();
    emitter.comment("--- runtime: ftruncate ---");
    emitter.label_global("__rt_ftruncate");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish frame
    emitter.instruction("mov rdi, rax");                                        // fd
    emitter.instruction("mov rsi, rdx");                                        // size (came via rdx as the secondary scalar)
    emitter.instruction("call ftruncate");                                      // libc ftruncate(fd, size)
    emitter.instruction("cmp rax, 0");                                          // success?
    emitter.instruction("sete al");                                             // boolean byte
    emitter.instruction("movzx rax, al");                                       // widen
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return predicate

    // -- fsync / fflush --
    emitter.blank();
    emitter.comment("--- runtime: fsync ---");
    emitter.label_global("__rt_fsync");
    emitter.label_global("__rt_fflush");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish frame
    emitter.instruction("mov rdi, rax");                                        // fd
    emitter.instruction("call fsync");                                          // libc fsync(fd)
    emitter.instruction("cmp rax, 0");                                          // success?
    emitter.instruction("sete al");                                             // boolean byte
    emitter.instruction("movzx rax, al");                                       // widen
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return predicate

    // -- fdatasync --
    emitter.blank();
    emitter.comment("--- runtime: fdatasync ---");
    emitter.label_global("__rt_fdatasync");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish frame
    emitter.instruction("mov rdi, rax");                                        // fd
    if emitter.platform == crate::codegen::platform::Platform::Linux {
        emitter.instruction("call fdatasync");                                  // libc fdatasync(fd) on Linux
    } else {
        emitter.instruction("call fsync");                                      // Darwin fallback
    }
    emitter.instruction("cmp rax, 0");                                          // success?
    emitter.instruction("sete al");                                             // boolean byte
    emitter.instruction("movzx rax, al");                                       // widen
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return predicate

    // -- touch --
    emitter.blank();
    emitter.comment("--- runtime: touch ---");
    emitter.label_global("__rt_touch");
    let plat = emitter.platform;
    let open_flags = plat.o_wronly_creat();
    let utime_now = plat.utime_now_nsec();
    // Frame layout (rbp-relative):
    //   [rbp -  8] : C path pointer
    //   [rbp - 16] : mtime arg
    //   [rbp - 24] : atime arg
    //   [rbp - 32] : current-time mask
    //   [rbp - 64] : timespec[0] (tv_sec=[rbp-64], tv_nsec=[rbp-56])
    //   [rbp - 48] : timespec[1] (tv_sec=[rbp-48], tv_nsec=[rbp-40])
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish frame
    emitter.instruction("sub rsp, 80");                                         // reserve aligned frame
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // save mtime arg
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // save atime arg
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save current-time mask
    emitter.instruction("call __rt_cstr");                                      // path → C string in rax
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save C path pointer

    emitter.instruction("mov rdi, rax");                                        // first arg = path
    emitter.instruction(&format!("mov rsi, 0x{:X}", open_flags));               // open flags
    emitter.instruction("mov rdx, 0x1B6");                                      // mode 0666 before umask
    emitter.instruction("call open");                                           // libc open()
    emitter.instruction("cmp rax, 0");                                          // success?
    emitter.instruction("jl __rt_touch_set_times_x86");                         // skip close on failure
    emitter.instruction("mov rdi, rax");                                        // fd
    emitter.instruction("call close");                                          // libc close(fd)

    emitter.label("__rt_touch_set_times_x86");
    // atime
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // load current-time mask
    emitter.instruction("test r8, 1");                                          // use current time for atime?
    emitter.instruction("jnz __rt_touch_atime_now_x86");                        // current atime path
    emitter.instruction("mov r8, QWORD PTR [rbp - 24]");                        // load explicit atime seconds
    emitter.instruction("mov QWORD PTR [rbp - 64], r8");                        // tv_sec = atime
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // tv_nsec = 0
    emitter.instruction("jmp __rt_touch_handle_mtime_x86");                     // continue with mtime selection
    emitter.label("__rt_touch_atime_now_x86");
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // tv_sec = 0
    emitter.instruction(&format!("mov QWORD PTR [rbp - 56], {}", utime_now));   // tv_nsec = platform UTIME_NOW sentinel

    emitter.label("__rt_touch_handle_mtime_x86");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload current-time mask
    emitter.instruction("test r8, 2");                                          // use current time for mtime?
    emitter.instruction("jnz __rt_touch_mtime_now_x86");                        // current mtime path
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // load explicit mtime seconds
    emitter.instruction("mov QWORD PTR [rbp - 48], r8");                        // tv_sec = mtime
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // tv_nsec = 0
    emitter.instruction("jmp __rt_touch_call_utimensat_x86");                   // call utimensat with prepared timestamps
    emitter.label("__rt_touch_mtime_now_x86");
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // tv_sec = 0
    emitter.instruction(&format!("mov QWORD PTR [rbp - 40], {}", utime_now));   // tv_nsec = platform UTIME_NOW sentinel

    emitter.label("__rt_touch_call_utimensat_x86");
    emitter.instruction(&format!("mov rdi, {}", plat.at_fdcwd()));              // AT_FDCWD (platform-dependent: -2 Darwin, -100 Linux)
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // C path pointer
    emitter.instruction("lea rdx, [rbp - 64]");                                 // pointer to timespec[0]
    emitter.instruction("mov rcx, 0");                                          // flags = 0
    emitter.instruction("call utimensat");                                      // libc utimensat()
    emitter.instruction("cmp rax, 0");                                          // success?
    emitter.instruction("sete al");                                             // boolean byte
    emitter.instruction("movzx rax, al");                                       // widen
    emitter.instruction("add rsp, 80");                                         // release frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return predicate
}
