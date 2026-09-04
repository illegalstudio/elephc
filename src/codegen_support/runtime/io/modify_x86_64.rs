//! Purpose:
//! Emits the `__rt_chmod`, `__rt_cstr` runtime helper assembly for modify Linux x86 64.
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
use crate::codegen_support::{abi, emit::Emitter};

/// Emits x86_64 Linux runtime helpers for filesystem modify operations:
/// `__rt_chmod`, `__rt_chown`, `__rt_chown_user`, `__rt_chgrp_group`, `__rt_umask`,
/// `__rt_ftruncate`, `__rt_fsync`, `__rt_fflush`, `__rt_fdatasync`, `__rt_touch`.
///
/// Each helper converts a PHP string path to a C string, calls the corresponding
/// libc function, and returns a boolean predicate (1 on success, 0 on failure).
/// The `__rt_touch` helper additionally handles timestamp via `utimensat` using
/// platform-specific flags and OpenBSD-style atime/mtime masks passed via registers.
///
/// # Arguments
/// * `emitter` - The assembly emitter used to write x86_64 instructions.
///
/// # ABI details
/// * Path and string length arrive via `rdi`/`rsi` registers; `__rt_cstr` converts
///   to a C string pointer returned in `rax`.
/// * Scalar secondary arguments (mode, uid, gid, size) arrive via stack spill or
///   secondary registers (`rdx`, `rcx`).
/// * Boolean results are zero-extended to a full integer in `rax` before returning.
pub(super) fn emit_modify_linux_x86_64(emitter: &mut Emitter) {
    // -- chmod --
    emitter.blank();
    emitter.comment("--- runtime: chmod ---");
    emitter.label_global("__rt_chmod");
    // See the AArch64 counterpart: unguarded, this answered TRUE where php answers false.
    super::fopen::emit_refuse_when_file_wrapper_disabled_saying(
        emitter,
        super::fopen::DisabledWrapperAnswer::Predicate(0),
        super::fopen::DisabledWrapperNotice::Fixed {
            symbol: "_diag_chmod_non_standard",
            len: super::fopen::CHMOD_NON_STANDARD_STREAM.len(),
        },
    );
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish frame
    emitter.instruction("sub rsp, 32");                                         // align stack, plus the path the warning names
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve mode (came in via the secondary string-argument register)
    emitter.instruction("call __rt_path_cstr");                                 // path → C string in rax
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // the path the URL shape names
    emitter.instruction("mov rdi, rax");                                        // first libc chmod arg = C path
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // second libc chmod arg = mode
    emitter.instruction("call chmod");                                          // libc chmod(path, mode)
    // See the AArch64 counterpart: php names no path for a direct call and BOTH a path and a
    // different wording when the argument was a `file://` URL.
    emitter.instruction("cmp eax, 0");                                          // did libc chmod() return success as a C int?
    emitter.instruction("je __rt_chmod_ok_x86");
    abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_path_url", 0);
    emitter.instruction("test r10, r10");
    emitter.instruction("jnz __rt_chmod_url_warn_x86");
    super::path_op_warning::emit_libc_call_x86_64(
        emitter,
        "_warn_chmod_head",
        CHMOD_WARNING_HEAD.len(),
        None,
        "_warn_chmod_head",
        0,
    );
    emitter.instruction("jmp __rt_chmod_warned_x86");
    emitter.label("__rt_chmod_url_warn_x86");
    super::path_op_warning::emit_libc_call_x86_64(
        emitter,
        "_warn_chmod_url_head",
        CHMOD_URL_WARNING_HEAD.len(),
        Some("[rbp - 16]"),
        "_warn_chmod_url_mid",
        CHMOD_URL_WARNING_MIDDLE.len(),
    );
    emitter.label("__rt_chmod_warned_x86");
    emitter.instruction("xor eax, eax");                                        // php answers false for the failure it just named
    emitter.instruction("jmp __rt_chmod_done_x86");
    emitter.label("__rt_chmod_ok_x86");
    emitter.instruction("mov eax, 1");
    emitter.label("__rt_chmod_done_x86");
    emitter.instruction("add rsp, 32");                                         // release stack
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return predicate

    // -- the six ownership helpers --
    // See the AArch64 counterpart: FOUR entry points over two syscalls, because php names the
    // CALLER in the diagnostic and a shared body cannot know which one it is.
    for (symbol, libc, head, head_len) in [
        ("__rt_chown", "chown", "_warn_chown_head", CHOWN_WARNING_HEAD.len()),
        ("__rt_chgrp", "chown", "_warn_chgrp_head", CHGRP_WARNING_HEAD.len()),
        ("__rt_lchown", "lchown", "_warn_lchown_head", LCHOWN_WARNING_HEAD.len()),
        ("__rt_lchgrp", "lchown", "_warn_lchgrp_head", LCHGRP_WARNING_HEAD.len()),
    ] {
        emit_ownership_syscall_x86_64(emitter, symbol, libc, head, head_len);
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
        emit_ownership_name_lookup_x86_64(
            emitter, symbol, libc, owner, head, head_len, absent, absent_len,
        );
    }


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
    emitter.instruction("mov rdi, rax");                                        // fd → SysV first arg (size already in rsi from the caller)
    emitter.instruction("call ftruncate");                                      // libc ftruncate(fd, size)
    emitter.instruction("cmp eax, 0");                                          // did libc ftruncate() return success as a C int?
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
    emitter.instruction("cmp eax, 0");                                          // did libc fsync() return success as a C int?
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
    if emitter.platform == crate::codegen_support::platform::Platform::Linux {
        emitter.instruction("call fdatasync");                                  // libc fdatasync(fd) on Linux
    } else {
        emitter.instruction("call fsync");                                      // Darwin fallback
    }
    emitter.instruction("cmp eax, 0");                                          // did libc sync helper return success as a C int?
    emitter.instruction("sete al");                                             // boolean byte
    emitter.instruction("movzx rax, al");                                       // widen
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return predicate

    // -- touch --
    emitter.blank();
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
    emitter.instruction("call __rt_path_cstr");                                 // path → C string in rax
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save C path pointer

    emitter.instruction("mov rdi, rax");                                        // first arg = path
    emitter.instruction(&format!("mov rsi, 0x{:X}", open_flags));               // open flags
    emitter.instruction("mov rdx, 0x1B6");                                      // mode 0666 before umask
    emitter.instruction("call open");                                           // libc open()
    emitter.instruction("cmp eax, 0");                                          // did libc open() return a negative C int?
    emitter.instruction("jl __rt_touch_set_times_x86");                         // skip close on failure
    emitter.instruction("cdqe");                                                // normalize the successful C int fd before closing it
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
    // See the AArch64 counterpart: the stamp is what decides, and a URL takes the wrapper hook's
    // shape, which names the path in the parentheses AND again in the sentence.
    emitter.instruction("cmp eax, 0");                                          // did libc utimensat() return success as a C int?
    emitter.instruction("je __rt_touch_ok_x86");
    abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_path_url", 0);
    emitter.instruction("test r10, r10");
    emitter.instruction("jnz __rt_touch_url_warn_x86");
    super::path_op_warning::emit_libc_call_x86_64(
        emitter,
        "_warn_touch_head",
        TOUCH_WARNING_HEAD.len(),
        Some("[rbp - 8]"),
        "_warn_touch_mid",
        TOUCH_WARNING_MIDDLE.len(),
    );
    emitter.instruction("jmp __rt_touch_warned_x86");
    emitter.label("__rt_touch_url_warn_x86");
    abi::emit_symbol_address(emitter, "rdi", "_warn_touch_url_head");
    emitter.instruction(&format!("mov rsi, {}", TOUCH_URL_WARNING_HEAD.len()));
    emitter.instruction("mov rdx, QWORD PTR [rbp - 8]");
    abi::emit_symbol_address(emitter, "rcx", "_warn_touch_url_mid");
    emitter.instruction(&format!("mov r8, {}", TOUCH_URL_WARNING_MIDDLE.len()));
    emitter.instruction("call __rt_path_op_fragment");                          // head, path, the sentence's opening
    super::path_op_warning::emit_libc_call_x86_64(
        emitter,
        "_warn_touch_url_head",
        0,
        Some("[rbp - 8]"),
        "_warn_touch_mid",
        TOUCH_WARNING_MIDDLE.len(),
    );
    emitter.label("__rt_touch_warned_x86");
    emitter.instruction("xor eax, eax");                                        // php answers false for the failure it just named
    emitter.instruction("jmp __rt_touch_done_x86");
    emitter.label("__rt_touch_ok_x86");
    emitter.instruction("mov eax, 1");
    emitter.label("__rt_touch_done_x86");
    emitter.instruction("add rsp, 80");                                         // release frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return predicate
}

/// The x86_64 counterpart of `emit_ownership_syscall_aarch64`.
fn emit_ownership_syscall_x86_64(
    emitter: &mut Emitter,
    symbol: &str,
    libc: &str,
    head: &str,
    head_len: usize,
) {
    emitter.blank();
    emitter.comment(&format!("--- runtime: {} ---", &symbol[5..]));
    emitter.label_global(symbol);
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish frame
    emitter.instruction("sub rsp, 32");                                         // align stack + spill uid/gid
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve uid
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve gid
    emitter.instruction("call __rt_path_cstr");                                 // path → C string in rax
    emitter.instruction("mov rdi, rax");                                        // first libc arg = path
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // second arg = uid
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // third arg = gid
    emitter.instruction(&format!("call {libc}"));                               // libc chown/lchown(path, uid, gid)
    emitter.instruction("cmp eax, 0");                                          // did libc return success as a C int?
    emitter.instruction(&format!("je {symbol}_ok_x86"));                        // php warns about nothing it managed
    super::path_op_warning::emit_libc_call_x86_64(emitter, head, head_len, None, head, 0);
    emitter.instruction("xor eax, eax");                                        // php answers false for the failure it just named
    emitter.instruction(&format!("jmp {symbol}_done_x86"));
    emitter.label(&format!("{symbol}_ok_x86"));
    emitter.instruction("mov eax, 1");
    emitter.label(&format!("{symbol}_done_x86"));
    emitter.instruction("add rsp, 32");                                         // release stack
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return predicate
}

/// The x86_64 counterpart of `emit_ownership_name_lookup_aarch64`.
fn emit_ownership_name_lookup_x86_64(
    emitter: &mut Emitter,
    symbol: &str,
    libc: &str,
    owner: bool,
    head: &str,
    head_len: usize,
    absent: &str,
    absent_len: usize,
) {
    let lookup = if owner {
        "__rt_lookup_passwd_uid"
    } else {
        "__rt_lookup_group_gid"
    };
    emitter.blank();
    emitter.comment(&format!("--- runtime: {} ---", &symbol[5..]));
    emitter.label_global(symbol);
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish frame
    emitter.instruction("sub rsp, 48");                                         // align stack + spill principal string and path pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // preserve principal pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // preserve principal length
    emitter.instruction("call __rt_path_cstr");                                 // path → C string in rax
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save C path pointer
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload principal pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload principal length
    emitter.instruction("call __rt_cstr2");                                     // principal → secondary C string in rax
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // the name php quotes when it does not resolve
    emitter.instruction("mov rdi, rax");                                        // first lookup arg = C principal name
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // second lookup arg = principal length
    emitter.instruction(&format!("call {lookup}"));                             // resolve id from the local database without NSS
    emitter.instruction("cmp eax, -1");                                         // was the name absent?
    emitter.instruction(&format!("je {symbol}_absent_x86"));                    // unknown name is its own diagnostic
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // first argument = C path
    if owner {
        emitter.instruction("mov esi, eax");                                    // second argument = resolved uid
        emitter.instruction("mov rdx, -1");                                     // gid = -1 (leave group unchanged)
    } else {
        emitter.instruction("mov edx, eax");                                    // third argument = resolved gid
        emitter.instruction("mov rsi, -1");                                     // uid = -1 (leave owner unchanged)
    }
    emitter.instruction(&format!("call {libc}"));                               // libc chown/lchown with the resolved principal
    emitter.instruction("cmp eax, 0");                                          // did libc return success as a C int?
    emitter.instruction(&format!("je {symbol}_ok_x86"));                        // php warns about nothing it managed
    super::path_op_warning::emit_libc_call_x86_64(emitter, head, head_len, None, head, 0);
    emitter.instruction("xor eax, eax");                                        // php answers false for the failure it just named
    emitter.instruction(&format!("jmp {symbol}_done_x86"));

    emitter.label(&format!("{symbol}_absent_x86"));
    // The shared composer spells `head` + this string + newline when the middle is empty and the
    // reason is zero, which is exactly php's wording for an unresolvable principal.
    abi::emit_symbol_address(emitter, "rdi", absent);
    emitter.instruction(&format!("mov rsi, {absent_len}"));
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // the principal name php quotes
    abi::emit_symbol_address(emitter, "rcx", absent);
    emitter.instruction("mov r8, 0");                                           // no middle: the name ends the sentence
    emitter.instruction("mov r9, 0");                                           // and no errno describes it
    emitter.instruction("call __rt_path_op_warning");
    emitter.instruction("xor eax, eax");                                        // unknown name returns false
    emitter.instruction(&format!("jmp {symbol}_done_x86"));

    emitter.label(&format!("{symbol}_ok_x86"));
    emitter.instruction("mov eax, 1");
    emitter.label(&format!("{symbol}_done_x86"));
    emitter.instruction("add rsp, 48");                                         // release stack
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return predicate
}
