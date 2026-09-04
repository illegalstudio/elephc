//! Purpose:
//! Emits the `__rt_touch`, `__rt_chmod` runtime helper assembly for modify.
//! Keeps PHP filesystem/resource behavior, libc calls, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - I/O helpers bridge PHP strings, resources, descriptors, and libc calls while returning runtime arrays or pointer/length strings.

use crate::codegen_support::runtime::data::{
    CHGRP_UNKNOWN_PRINCIPAL_HEAD, CHGRP_WARNING_HEAD, CHMOD_URL_WARNING_HEAD,
    CHMOD_URL_WARNING_MIDDLE, CHMOD_WARNING_HEAD, CHOWN_UNKNOWN_PRINCIPAL_HEAD,
    CHOWN_WARNING_HEAD, LCHGRP_UNKNOWN_PRINCIPAL_HEAD, LCHGRP_WARNING_HEAD,
    LCHOWN_UNKNOWN_PRINCIPAL_HEAD, LCHOWN_WARNING_HEAD, TOUCH_URL_WARNING_HEAD,
    TOUCH_URL_WARNING_MIDDLE, TOUCH_WARNING_HEAD, TOUCH_WARNING_MIDDLE,
};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

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
    // php locates a wrapper for every path, and `chmod()` words the refusal its own way — it is
    // the ONLY guarded operation that does. Unguarded, elephc reached the syscall and answered
    // TRUE while php answers false, which is a wrong VALUE and not just a missing line.
    super::fopen::emit_refuse_when_file_wrapper_disabled_saying(
        emitter,
        super::fopen::DisabledWrapperAnswer::Predicate(0),
        super::fopen::DisabledWrapperNotice::Fixed {
            symbol: "_diag_chmod_non_standard",
            len: super::fopen::CHMOD_NON_STANDARD_STREAM.len(),
        },
    );
    emitter.instruction("sub sp, sp, #32");                                     // allocate frame + spill slot for mode
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish new frame pointer
    emitter.instruction("str x3, [sp, #0]");                                    // preserve the mode value across the cstr call
    emitter.instruction("bl __rt_path_cstr");                                   // path → null-terminated C string in x0
    emitter.instruction("str x0, [sp, #8]");                                    // the path the URL shape names
    emitter.instruction("ldr x1, [sp, #0]");                                    // restore mode into the second libc argument
    emitter.bl_c("chmod");                                                      // libc chmod(path, mode)

    // -- php names no path in this one, and elephc named nothing at all --
    // MEASURED: `Warning: chmod(): No such file or directory`, parentheses EMPTY.
    emitter.instruction("cmp x0, #0");                                          // libc answers 0 on success
    emitter.instruction("b.eq __rt_chmod_ok");
    // A `file://` URL reaches php through the plain-files wrapper's METADATA hook, whose
    // diagnostic names the path and words the failure differently — see the two wordings.
    abi::emit_load_symbol_to_reg(emitter, "x10", "_rt_path_url", 0);
    emitter.instruction("cbnz x10, __rt_chmod_url_warn");
    super::path_op_warning::emit_libc_call_aarch64(
        emitter,
        "_warn_chmod_head",
        CHMOD_WARNING_HEAD.len(),
        None,
        "_warn_chmod_head",
        0,
    );
    emitter.instruction("b __rt_chmod_warned");
    emitter.label("__rt_chmod_url_warn");
    super::path_op_warning::emit_libc_call_aarch64(
        emitter,
        "_warn_chmod_url_head",
        CHMOD_URL_WARNING_HEAD.len(),
        Some("[sp, #8]"),
        "_warn_chmod_url_mid",
        CHMOD_URL_WARNING_MIDDLE.len(),
    );
    emitter.label("__rt_chmod_warned");
    emitter.instruction("mov x0, #0");                                          // php answers false for the failure it just named
    emitter.instruction("b __rt_chmod_done");
    emitter.label("__rt_chmod_ok");
    emitter.instruction("mov x0, #1");
    emitter.label("__rt_chmod_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate frame
    emitter.instruction("ret");                                                 // return predicate

    // ================================================================
    // The six ownership helpers
    // ================================================================
    // php names the CALLER in every one of these diagnostics — `Warning: chgrp(): No such file
    // or directory`, not the `chown()` syscall it is built on — so the four callers cannot share
    // one entry point. Handing the name in as an argument would let a call site forget it and
    // pass garbage; a wrong ENTRY POINT does not compile. They are emitted from one Rust function
    // each so the bodies cannot drift apart, which is the trade `__rt_path_op_fragment` and
    // `__rt_path_op_warning` already make in this crate.
    for (symbol, libc, head, head_len) in [
        ("__rt_chown", "chown", "_warn_chown_head", CHOWN_WARNING_HEAD.len()),
        ("__rt_chgrp", "chown", "_warn_chgrp_head", CHGRP_WARNING_HEAD.len()),
        ("__rt_lchown", "lchown", "_warn_lchown_head", LCHOWN_WARNING_HEAD.len()),
        ("__rt_lchgrp", "lchown", "_warn_lchgrp_head", LCHGRP_WARNING_HEAD.len()),
    ] {
        emit_ownership_syscall_aarch64(emitter, symbol, libc, head, head_len);
    }

    for (symbol, libc, owner, head, head_len, absent, absent_len) in [
        (
            "__rt_chown_user", "chown", true,
            "_warn_chown_head", CHOWN_WARNING_HEAD.len(),
            "_warn_chown_noprincipal", CHOWN_UNKNOWN_PRINCIPAL_HEAD.len(),
        ),
        (
            "__rt_lchown_user", "lchown", true,
            "_warn_lchown_head", LCHOWN_WARNING_HEAD.len(),
            "_warn_lchown_noprincipal", LCHOWN_UNKNOWN_PRINCIPAL_HEAD.len(),
        ),
        (
            "__rt_chgrp_group", "chown", false,
            "_warn_chgrp_head", CHGRP_WARNING_HEAD.len(),
            "_warn_chgrp_noprincipal", CHGRP_UNKNOWN_PRINCIPAL_HEAD.len(),
        ),
        (
            "__rt_lchgrp_group", "lchown", false,
            "_warn_lchgrp_head", LCHGRP_WARNING_HEAD.len(),
            "_warn_lchgrp_noprincipal", LCHGRP_UNKNOWN_PRINCIPAL_HEAD.len(),
        ),
    ] {
        emit_ownership_name_lookup_aarch64(
            emitter, symbol, libc, owner, head, head_len, absent, absent_len,
        );
    }

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
    if emitter.platform == crate::codegen_support::platform::Platform::Linux {
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
    // php locates a wrapper for every path; a bare one is the plain-files wrapper.
    super::fopen::emit_refuse_when_file_wrapper_disabled_saying(
        emitter,
        super::fopen::DisabledWrapperAnswer::Predicate(0),
        super::fopen::DisabledWrapperNotice::FailedToOpen {
            name_symbol: "_uww_name_touch",
            name_len: 5,
            directory: false,
        },
    );
    emitter.instruction(&format!("sub sp, sp, #{}", frame));                    // allocate frame + timespec[2] + spill slots
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", save_off));         // save frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{}", save_off));                // establish new frame pointer
    emitter.instruction("str x3, [sp, #8]");                                    // save mtime arg
    emitter.instruction("str x4, [sp, #16]");                                   // save atime arg
    emitter.instruction("str x5, [sp, #24]");                                   // save current-time mask
    emitter.instruction("bl __rt_path_cstr");                                   // path → C string in x0
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

    // -- php puts the path INSIDE the sentence for this one --
    // MEASURED: `Warning: touch(): Unable to create file /no/such/x.txt because No such file or
    // directory`. The stamp is what decides: a file that exists but could not be OPENED is still
    // touched successfully by php, and warning on the open would have reported those as failures.
    emitter.instruction("cmp x0, #0");                                          // libc answers 0 on success
    emitter.instruction("b.eq __rt_touch_ok");
    // See `chmod()`: a URL takes the wrapper hook's shape, which names the path in the
    // parentheses AND again in the sentence.
    abi::emit_load_symbol_to_reg(emitter, "x10", "_rt_path_url", 0);
    emitter.instruction("cbnz x10, __rt_touch_url_warn");
    super::path_op_warning::emit_libc_call_aarch64(
        emitter,
        "_warn_touch_head",
        TOUCH_WARNING_HEAD.len(),
        Some("[sp, #0]"),
        "_warn_touch_mid",
        TOUCH_WARNING_MIDDLE.len(),
    );
    emitter.instruction("b __rt_touch_warned");
    emitter.label("__rt_touch_url_warn");
    abi::emit_symbol_address(emitter, "x0", "_warn_touch_url_head");
    emitter.instruction(&format!("mov x1, #{}", TOUCH_URL_WARNING_HEAD.len()));
    emitter.instruction("ldr x2, [sp, #0]");
    abi::emit_symbol_address(emitter, "x3", "_warn_touch_url_mid");
    emitter.instruction(&format!("mov x4, #{}", TOUCH_URL_WARNING_MIDDLE.len()));
    emitter.instruction("bl __rt_path_op_fragment");                            // head, path, the sentence's opening
    super::path_op_warning::emit_libc_call_aarch64(
        emitter,
        "_warn_touch_url_head",
        0,
        Some("[sp, #0]"),
        "_warn_touch_mid",
        TOUCH_WARNING_MIDDLE.len(),
    );
    emitter.label("__rt_touch_warned");
    emitter.instruction("mov x0, #0");                                          // php answers false for the failure it just named
    emitter.instruction("b __rt_touch_done");
    emitter.label("__rt_touch_ok");
    emitter.instruction("mov x0, #1");
    emitter.label("__rt_touch_done");
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_off));         // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", frame));                    // deallocate frame
    emitter.instruction("ret");                                                 // return predicate
}

/// Emits one ownership syscall entry point: `<symbol>(x1/x2 = path, x3 = uid, x4 = gid)`.
///
/// `-1` in either principal means "leave that one alone", which is how `chgrp` reaches `chown`.
/// A failure names ITS OWN caller and the reason libc recorded, then answers php's `false`.
fn emit_ownership_syscall_aarch64(
    emitter: &mut Emitter,
    symbol: &str,
    libc: &str,
    head: &str,
    head_len: usize,
) {
    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment for the next runtime helper
    emitter.comment(&format!("--- runtime: {} ---", &symbol[5..]));
    emitter.label_global(symbol);
    emitter.instruction("sub sp, sp, #32");                                     // allocate frame + spill slots for uid/gid
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish new frame pointer
    emitter.instruction("stp x3, x4, [sp, #0]");                                // preserve uid/gid across the cstr call
    emitter.instruction("bl __rt_path_cstr");                                   // path → C string in x0
    emitter.instruction("ldp x1, x2, [sp, #0]");                                // restore uid/gid into the libc argument registers
    emitter.bl_c(libc);                                                         // libc chown/lchown(path, uid, gid)
    emitter.instruction("cmp x0, #0");                                          // success?
    emitter.instruction(&format!("b.eq {symbol}_ok"));                          // php warns about nothing it managed
    super::path_op_warning::emit_libc_call_aarch64(emitter, head, head_len, None, head, 0);
    emitter.instruction("mov x0, #0");                                          // php answers false for the failure it just named
    emitter.instruction(&format!("b {symbol}_done"));
    emitter.label(&format!("{symbol}_ok"));
    emitter.instruction("mov x0, #1");
    emitter.label(&format!("{symbol}_done"));
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate frame
    emitter.instruction("ret");                                                 // return predicate
}

/// Emits one NAME-resolving ownership entry point: `<symbol>(x1/x2 = path, x3/x4 = principal)`.
///
/// Two failures, worded differently. A name that does not resolve carries no `errno` and php
/// QUOTES it — `Warning: chown(): Unable to find uid for nosuchuser` — while a syscall failure
/// reports `strerror` and no name. elephc printed neither, and answered `false` in silence.
fn emit_ownership_name_lookup_aarch64(
    emitter: &mut Emitter,
    symbol: &str,
    libc: &str,
    owner: bool,
    head: &str,
    head_len: usize,
    absent: &str,
    absent_len: usize,
) {
    let (lookup, subject) = if owner {
        ("__rt_lookup_passwd_uid", "user")
    } else {
        ("__rt_lookup_group_gid", "group")
    };
    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment for the next runtime helper
    emitter.comment(&format!("--- runtime: {} ---", &symbol[5..]));
    emitter.label_global(symbol);
    emitter.instruction("sub sp, sp, #48");                                     // allocate frame + spill slots for path and principal strings
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish new frame pointer
    emitter.instruction("stp x3, x4, [sp, #16]");                               // preserve principal ptr/len across path conversion
    emitter.instruction("bl __rt_path_cstr");                                   // path → C string in x0
    emitter.instruction("str x0, [sp, #0]");                                    // save C path pointer
    emitter.instruction("ldp x1, x2, [sp, #16]");                               // reload principal ptr/len
    emitter.instruction("bl __rt_cstr2");                                       // principal → secondary C string in x0
    emitter.instruction("str x0, [sp, #8]");                                    // the name php quotes when it does not resolve
    emitter.instruction("ldr x1, [sp, #24]");                                   // second lookup arg = principal length
    emitter.instruction(&format!("bl {lookup}"));                               // resolve id from the local database without NSS
    emitter.instruction("cmn x0, #1");                                          // was the name absent?
    emitter.instruction(&format!("b.eq {symbol}_absent"));                      // unknown name is its own diagnostic
    if owner {
        emitter.instruction("mov x1, x0");                                      // second argument = resolved uid
        emitter.instruction("ldr x0, [sp, #0]");                                // reload C path pointer
        emitter.instruction("mov x2, #-1");                                     // gid = -1 (leave group unchanged)
    } else {
        emitter.instruction("mov x2, x0");                                      // third argument = resolved gid
        emitter.instruction("ldr x0, [sp, #0]");                                // reload C path pointer
        emitter.instruction("mov x1, #-1");                                     // uid = -1 (leave owner unchanged)
    }
    emitter.bl_c(libc);                                                         // libc chown/lchown with the resolved principal
    emitter.instruction("cmp x0, #0");                                          // success?
    emitter.instruction(&format!("b.eq {symbol}_ok"));                          // php warns about nothing it managed
    super::path_op_warning::emit_libc_call_aarch64(emitter, head, head_len, None, head, 0);
    emitter.instruction("mov x0, #0");                                          // php answers false for the failure it just named
    emitter.instruction(&format!("b {symbol}_done"));

    emitter.label(&format!("{symbol}_absent"));
    // The shared composer spells `head` + this string + newline when the middle is empty and the
    // reason is zero, which is exactly php's wording for an unresolvable principal.
    abi::emit_symbol_address(emitter, "x0", absent);
    emitter.instruction(&format!("mov x1, #{absent_len}"));
    emitter.instruction("ldr x2, [sp, #8]");                                    // the principal name php quotes
    abi::emit_symbol_address(emitter, "x3", absent);
    emitter.instruction("mov x4, #0");                                          // no middle: the name ends the sentence
    emitter.instruction("mov x5, #0");                                          // and no errno describes it
    emitter.instruction("bl __rt_path_op_warning");
    let _ = subject;
    emitter.instruction("mov x0, #0");                                          // unknown name returns false

    emitter.instruction(&format!("b {symbol}_done"));
    emitter.label(&format!("{symbol}_ok"));
    emitter.instruction("mov x0, #1");
    emitter.label(&format!("{symbol}_done"));
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate frame
    emitter.instruction("ret");                                                 // return predicate
}
