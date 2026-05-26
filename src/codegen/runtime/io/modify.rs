//! Purpose:
//! Emits the `__rt_touch`, `__rt_chmod` runtime helper assembly for modify.
//! Keeps PHP filesystem/resource behavior, libc calls, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//!
//! Key details:
//! - I/O helpers bridge PHP strings, resources, descriptors, and libc calls while returning runtime arrays or pointer/length strings.

use crate::codegen::{emit::Emitter, platform::Arch};

use super::modify_x86_64::emit_modify_linux_x86_64;

/// Emits file-modification runtime helpers for ARM64 targets.
///
/// Dispatches to `emit_modify_linux_x86_64` on x86_64 Linux.
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
