//! Win32 shims for the C-symbol delegate family: the 762-LOC `c_symbols`
//! cohesive emitter, its stub delegates, and the msvcrt-real
//! passwd/group-lookup shims (fopen/fgets/fclose/strncmp/strchr/strtoul).

use crate::codegen::emit::Emitter;

/// Emits shim wrappers for syscalls that have C symbol stubs but no `__rt_sys_*` label.
///
/// `windows_transform.rs` maps Linux syscalls 86/88/89/92/93/94 to `__rt_sys_*` names,
/// but the actual implementations live as C symbol stubs (link, symlink, readlink, etc.).
/// These thin wrappers shuffle SysV args to match the C stub calling convention and
/// delegate to the stubs.
pub(super) fn emit_shim_c_symbol_delegates(emitter: &mut Emitter) {
    // __rt_sys_link: delegate to C symbol `link` (CreateHardLinkW)
    // SysV: rdi=oldpath, rsi=newpath → C stub expects same SysV args
    emitter.label_global("__rt_sys_link");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call link");                                           // delegate to C symbol stub
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // __rt_sys_symlink: delegate to C symbol `symlink` (CreateSymbolicLinkW)
    // SysV: rdi=target, rsi=linkpath → C stub expects same SysV args
    emitter.label_global("__rt_sys_symlink");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call symlink");                                        // delegate to C symbol stub
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // __rt_sys_readlink: delegate to C symbol `readlink` (GetFinalPathNameByHandleW)
    // SysV: rdi=path, rsi=buf, rdx=bufsize → C stub expects same SysV args
    emitter.label_global("__rt_sys_readlink");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call readlink");                                       // delegate to C symbol stub
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // __rt_sys_chown: return -1 (ENOSYS — Windows uses ACLs)
    emitter.label_global("__rt_sys_chown");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 38");                // ENOSYS: Windows ownership is ACL-based
    emitter.instruction("mov rax, -1");                                         // return -1 (not supported)
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // __rt_sys_fchown: return -1 (ENOSYS)
    emitter.label_global("__rt_sys_fchown");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 38");                // ENOSYS: Windows ownership is ACL-based
    emitter.instruction("mov rax, -1");                                         // return -1 (not supported)
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // __rt_sys_lchown: return -1 (ENOSYS)
    emitter.label_global("__rt_sys_lchown");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 38");                // ENOSYS: Windows ownership is ACL-based
    emitter.instruction("mov rax, -1");                                         // return -1 (not supported)
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits stubs for C library symbols that are called directly by the runtime
/// but do not exist on Windows. Each stub either delegates to a Win32 equivalent
/// or returns a safe default value.
pub(super) fn emit_shim_c_symbols(emitter: &mut Emitter) {
    emitter.raw("    # -- C symbol stubs for functions not available on Windows --");

    // flock: use a zeroed OVERLAPPED at offset zero and lock the whole remaining file.
    // PHP's LOCK_UN is bit 3; LOCK_NB is bit 2 and maps to FAIL_IMMEDIATELY.
    emitter.label_global("flock");
    emitter.instruction("sub rsp, 104");                                        // shadow, stack args, OVERLAPPED, saved handle/op, and error
    emitter.instruction("mov QWORD PTR [rsp + 88], rsi");                       // preserve operation across Win32 calls
    emitter.instruction("mov rcx, rdi");                                        // fd
    emitter.instruction("call __rt_fd_to_handle");                              // convert fd to HANDLE
    emitter.instruction("cmp rax, -1");                                         // invalid CRT descriptor?
    emitter.instruction("je .Lflock_bad_fd");                                   // return EBADF
    emitter.instruction("mov QWORD PTR [rsp + 80], rax");                       // preserve HANDLE across unlock-before-lock
    emitter.instruction("pxor xmm0, xmm0");                                     // zero the complete OVERLAPPED state
    emitter.instruction("movdqu XMMWORD PTR [rsp + 48], xmm0");                 // OVERLAPPED bytes 0..15
    emitter.instruction("movdqu XMMWORD PTR [rsp + 64], xmm0");                 // OVERLAPPED bytes 16..31
    emitter.instruction("test QWORD PTR [rsp + 88], 8");                        // LOCK_UN?
    emitter.instruction("jnz .Lflock_unlock");                                  // perform only the explicit unlock
    // -- PHP compatibility: discard any prior lock owned by this descriptor before relocking --
    emitter.instruction("mov rcx, QWORD PTR [rsp + 80]");                       // handle
    emitter.instruction("xor edx, edx");                                        // dwReserved = 0
    emitter.instruction("mov r8d, 0xFFFFFFFF");                                 // nNumberOfBytesToUnlockLow = MAXDWORD
    emitter.instruction("mov r9d, 0xFFFFFFFF");                                 // nNumberOfBytesToUnlockHigh = MAXDWORD
    emitter.instruction("lea rax, [rsp + 48]");                                 // stable zero-offset OVERLAPPED
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // fifth MSx64 argument
    emitter.instruction("call UnlockFileEx");                                   // ignore ERROR_NOT_LOCKED before acquiring a new mode
    emitter.instruction("mov rcx, QWORD PTR [rsp + 80]");                       // handle for LockFileEx
    emitter.instruction("xor edx, edx");                                        // dwFlags = 0
    emitter.instruction("xor r8d, r8d");                                        // dwReserved = 0
    emitter.instruction("test QWORD PTR [rsp + 88], 2");                        // LOCK_EX (bit 1)?
    emitter.instruction("jz .Lflock_shared");                                   // shared lock keeps flags clear
    emitter.instruction("or edx, 2");                                           // LOCKFILE_EXCLUSIVE_LOCK
    emitter.label(".Lflock_shared");
    emitter.instruction("test QWORD PTR [rsp + 88], 4");                        // LOCK_NB (bit 2)?
    emitter.instruction("jz .Lflock_no_nb");                                    // skip if not LOCK_NB
    emitter.instruction("or edx, 1");                                           // LOCKFILE_FAIL_IMMEDIATELY
    emitter.label(".Lflock_no_nb");
    emitter.instruction("mov r9d, 0xFFFFFFFF");                                 // nNumberOfBytesToLockLow = MAXDWORD (zero-extends to r9)
    emitter.instruction("mov DWORD PTR [rsp + 32], 0xFFFFFFFF");                // nNumberOfBytesToLockHigh = MAXDWORD
    emitter.instruction("lea rax, [rsp + 48]");                                 // zero-offset OVERLAPPED
    emitter.instruction("mov QWORD PTR [rsp + 40], rax");                       // sixth MSx64 argument
    emitter.instruction("call LockFileEx");                                     // lock file
    emitter.instruction("jmp .Lflock_finish");                                  // normalize BOOL result to POSIX convention
    emitter.label(".Lflock_unlock");
    emitter.instruction("mov rcx, QWORD PTR [rsp + 80]");                       // handle
    emitter.instruction("xor rdx, rdx");                                        // dwReserved = 0
    emitter.instruction("mov r8d, 0xFFFFFFFF");                                 // nNumberOfBytesToUnlockLow = MAXDWORD (zero-extends)
    emitter.instruction("mov r9d, 0xFFFFFFFF");                                 // nNumberOfBytesToUnlockHigh = MAXDWORD (zero-extends)
    emitter.instruction("lea rax, [rsp + 48]");                                 // zero-offset OVERLAPPED
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // fifth MSx64 argument
    emitter.instruction("call UnlockFileEx");                                   // unlock file
    emitter.label(".Lflock_finish");
    emitter.instruction("test eax, eax");                                       // Win32 BOOL success?
    emitter.instruction("jz .Lflock_fail");                                     // publish errno and return -1
    emitter.instruction("xor rax, rax");                                        // return 0
    emitter.instruction("add rsp, 104");                                        // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lflock_fail");
    emitter.instruction("call GetLastError");                                   // capture lock failure
    emitter.instruction("mov DWORD PTR [rip + __rt_win32_last_error], eax");    // retain native diagnostic
    emitter.instruction("cmp eax, 32");                                         // ERROR_SHARING_VIOLATION?
    emitter.instruction("je .Lflock_would_block");                              // PHP reports nonblocking contention via would_block
    emitter.instruction("cmp eax, 33");                                         // ERROR_LOCK_VIOLATION?
    emitter.instruction("je .Lflock_would_block");                              // same contention result
    emitter.instruction("call __rt_win32_errno_from_code");                     // translate other failures
    emitter.instruction("jmp .Lflock_store_errno");                             // publish translated errno
    emitter.label(".Lflock_would_block");
    emitter.instruction("mov eax, 11");                                         // EAGAIN / EWOULDBLOCK
    emitter.label(".Lflock_store_errno");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], eax");               // publish POSIX errno
    emitter.instruction("mov rax, -1");                                         // POSIX flock failure
    emitter.instruction("add rsp, 104");                                        // restore stack
    emitter.instruction("ret");                                                 // return failure
    emitter.label(".Lflock_bad_fd");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 9");                 // EBADF
    emitter.instruction("mov rax, -1");                                         // invalid descriptor
    emitter.instruction("add rsp, 104");                                        // restore stack
    emitter.instruction("ret");                                                 // return failure
    emitter.blank();

    // __errno_location: return pointer to thread-local errno
    // On Windows, msvcrt provides _errno() which returns the same thing.
    // We alias it to a static errno variable. Written on failure by the
    // __rt_sys_read (ReadFile) shim (shims_fs.rs) and the recvfrom/recvmsg
    // (Winsock) shims (shims_net.rs) via __rt_win32_errno_from_code below;
    // left unchanged on success, matching POSIX errno semantics.
    emitter.label_global("__errno_location");
    emitter.instruction("lea rax, [rip + __rt_errno]");                         // return pointer to static errno
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // symlink: strict wide conversion with unprivileged-create retry
    // SysV: rdi=target, rsi=linkpath → Win32: rcx=symlinkPath, rdx=targetPath
    emitter.label_global("symlink");
    emitter.instruction("sub rsp, 72");                                         // shadow space and two owned wide paths
    emitter.instruction("mov QWORD PTR [rsp + 48], rsi");                       // preserve UTF-8 link path
    emitter.instruction("call __rt_win_utf8_to_utf16");                         // convert target
    emitter.instruction("test rax, rax");                                       // target conversion succeeded?
    emitter.instruction("jz .Lsymlink_conversion_fail");                        // invalid UTF-8
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // owned wide target
    emitter.instruction("mov rdi, QWORD PTR [rsp + 48]");                       // UTF-8 link path
    emitter.instruction("call __rt_win_utf8_to_utf16");                         // convert link path
    emitter.instruction("test rax, rax");                                       // link conversion succeeded?
    emitter.instruction("jz .Lsymlink_link_conversion_fail");                   // cleanup target
    emitter.instruction("mov QWORD PTR [rsp + 40], rax");                       // owned wide link path
    emitter.instruction("mov rcx, QWORD PTR [rsp + 32]");                       // wide target for directory detection
    emitter.instruction("call GetFileAttributesW");                             // detect directory reparse target when it exists
    emitter.instruction("mov r8, 2");                                           // ALLOW_UNPRIVILEGED_CREATE
    emitter.instruction("cmp eax, -1");                                         // target attributes available?
    emitter.instruction("je .Lsymlink_flags_ready");                            // dangling target: keep file-link semantics
    emitter.instruction("test eax, 0x10");                                      // FILE_ATTRIBUTE_DIRECTORY?
    emitter.instruction("jz .Lsymlink_flags_ready");                            // regular-file target
    emitter.instruction("or r8, 1");                                            // SYMBOLIC_LINK_FLAG_DIRECTORY
    emitter.label(".Lsymlink_flags_ready");
    emitter.instruction("mov QWORD PTR [rsp + 64], r8");                        // preserve directory flag for retry
    emitter.instruction("mov rcx, QWORD PTR [rsp + 40]");                       // lpSymlinkPath
    emitter.instruction("mov rdx, QWORD PTR [rsp + 32]");                       // lpTargetPath
    emitter.instruction("call CreateSymbolicLinkW");                            // create Unicode symbolic link
    emitter.instruction("test rax, rax");                                       // success?
    emitter.instruction("jnz .Lsymlink_ok");                                    // → success
    // -- retry without unprivileged flag (requires admin) --
    emitter.instruction("mov rcx, QWORD PTR [rsp + 40]");                       // lpSymlinkPath
    emitter.instruction("mov rdx, QWORD PTR [rsp + 32]");                       // lpTargetPath
    emitter.instruction("mov r8, QWORD PTR [rsp + 64]");                        // preserve directory-link classification
    emitter.instruction("and r8, 1");                                           // retry without unprivileged bit
    emitter.instruction("call CreateSymbolicLinkW");                            // retry without unprivileged flag
    emitter.instruction("test rax, rax");                                       // success?
    emitter.instruction("jz .Lsymlink_fail");                                   // → failure
    emitter.label(".Lsymlink_ok");
    emitter.instruction("mov rax, QWORD PTR [rsp + 40]");                       // owned wide link path
    emitter.instruction("call __rt_heap_free");                                 // release link conversion
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // owned wide target
    emitter.instruction("call __rt_heap_free");                                 // release target conversion
    emitter.instruction("xor rax, rax");                                        // return 0 (success)
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lsymlink_fail");
    emitter.instruction("call GetLastError");                                   // capture final native failure
    emitter.instruction("mov DWORD PTR [rsp + 56], eax");                       // preserve error across cleanup
    emitter.instruction("mov rax, QWORD PTR [rsp + 40]");                       // owned wide link path
    emitter.instruction("call __rt_heap_free");                                 // release link conversion
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // owned wide target
    emitter.instruction("call __rt_heap_free");                                 // release target conversion
    emitter.instruction("mov eax, DWORD PTR [rsp + 56]");                       // restore native error
    emitter.instruction("mov DWORD PTR [rip + __rt_win32_last_error], eax");    // retain native state
    emitter.instruction("call __rt_win32_errno_from_code");                     // translate errno
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], eax");               // publish errno
    emitter.instruction("mov rax, -1");                                         // return -1 on failure
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lsymlink_link_conversion_fail");
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // owned wide target
    emitter.instruction("call __rt_heap_free");                                 // release target conversion
    emitter.label(".Lsymlink_conversion_fail");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 84");                // EILSEQ
    emitter.instruction("mov rax, -1");                                         // return failure
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // link: delegate to CreateHardLinkW (args reversed from POSIX), then translate
    // the Win32 BOOL result (nonzero = success) to the POSIX convention __rt_link
    // expects (0 = success, -1 = failure) — mirrors the symlink shim immediately
    // above. Without this translation, CreateHardLinkW success (nonzero) compared
    // against 0 reports failure, and vice versa.
    emitter.label_global("link");
    emitter.instruction("sub rsp, 72");                                         // shadow space and two owned wide paths
    emitter.instruction("mov QWORD PTR [rsp + 48], rsi");                       // preserve UTF-8 new path
    emitter.instruction("call __rt_win_utf8_to_utf16");                         // convert existing path
    emitter.instruction("test rax, rax");                                       // conversion succeeded?
    emitter.instruction("jz .Llink_conversion_fail");                           // invalid UTF-8
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // owned wide existing path
    emitter.instruction("mov rdi, QWORD PTR [rsp + 48]");                       // UTF-8 new path
    emitter.instruction("call __rt_win_utf8_to_utf16");                         // convert new path
    emitter.instruction("test rax, rax");                                       // conversion succeeded?
    emitter.instruction("jz .Llink_new_conversion_fail");                       // cleanup existing path
    emitter.instruction("mov QWORD PTR [rsp + 40], rax");                       // owned wide new path
    emitter.instruction("mov rcx, rax");                                        // lpFileName = new path
    emitter.instruction("mov rdx, QWORD PTR [rsp + 32]");                       // lpExistingFileName
    emitter.instruction("xor r8, r8");                                          // lpSecurityAttributes = NULL
    emitter.instruction("call CreateHardLinkW");                                // create Unicode hard link
    emitter.instruction("test eax, eax");                                       // Win32 BOOL: nonzero = success
    emitter.instruction("jz .Llink_fail");                                      // zero = failure
    emitter.instruction("mov rax, QWORD PTR [rsp + 40]");                       // owned wide new path
    emitter.instruction("call __rt_heap_free");                                 // release new path
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // owned wide existing path
    emitter.instruction("call __rt_heap_free");                                 // release existing path
    emitter.instruction("xor rax, rax");                                        // translate success to POSIX 0
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Llink_fail");
    emitter.instruction("call GetLastError");                                   // capture native failure
    emitter.instruction("mov DWORD PTR [rsp + 56], eax");                       // preserve native error
    emitter.instruction("mov rax, QWORD PTR [rsp + 40]");                       // owned wide new path
    emitter.instruction("call __rt_heap_free");                                 // release new path
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // owned wide existing path
    emitter.instruction("call __rt_heap_free");                                 // release existing path
    emitter.instruction("mov eax, DWORD PTR [rsp + 56]");                       // restore native error
    emitter.instruction("mov DWORD PTR [rip + __rt_win32_last_error], eax");    // retain native state
    emitter.instruction("call __rt_win32_errno_from_code");                     // translate errno
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], eax");               // publish errno
    emitter.instruction("mov rax, -1");                                         // translate failure to POSIX -1
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Llink_new_conversion_fail");
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // owned wide existing path
    emitter.instruction("call __rt_heap_free");                                 // release existing path
    emitter.label(".Llink_conversion_fail");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 84");                // EILSEQ
    emitter.instruction("mov rax, -1");                                         // return failure
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    emit_shim_readlink_reparse(emitter);
    emit_shim_lstat_symlink_probe(emitter);

    // Legacy final-path implementation retained as a non-routed diagnostic reference.
    if false {
    emitter.label_global("__rt_readlink_final_path_legacy");
    emitter.instruction("sub rsp, 104");                                        // CreateFile stack args and owned path/output locals
    emitter.instruction("mov QWORD PTR [rsp + 80], rsi");                       // preserve caller UTF-8 buffer
    emitter.instruction("mov QWORD PTR [rsp + 88], rdx");                       // preserve caller byte capacity
    emitter.instruction("call __rt_win_utf8_to_utf16");                         // convert link path strictly
    emitter.instruction("test rax, rax");                                       // path conversion succeeded?
    emitter.instruction("jz .Lreadlink_conversion_fail");                       // invalid UTF-8
    emitter.instruction("mov QWORD PTR [rsp + 56], rax");                       // owned wide input path
    emitter.instruction("mov rax, 65536");                                      // 32,768 WCHAR output buffer
    emitter.instruction("call __rt_heap_alloc");                                // allocate long-path wide output
    emitter.instruction("test rax, rax");                                       // allocation succeeded?
    emitter.instruction("jz .Lreadlink_alloc_fail");                            // cleanup input and return ENOMEM
    emitter.instruction("mov QWORD PTR [rsp + 64], rax");                       // owned wide output
    emitter.instruction("mov rcx, QWORD PTR [rsp + 56]");                       // Unicode link path
    emitter.instruction("xor edx, edx");                                        // metadata-only access
    emitter.instruction("mov r8, 7");                                           // share read/write/delete
    emitter.instruction("xor r9, r9");                                          // security attributes = NULL
    emitter.instruction("mov QWORD PTR [rsp + 32], 3");                         // OPEN_EXISTING
    emitter.instruction("mov QWORD PTR [rsp + 40], 0x2200000");                 // BACKUP_SEMANTICS | OPEN_REPARSE_POINT
    emitter.instruction("mov QWORD PTR [rsp + 48], 0");                         // template = NULL
    emitter.instruction("call CreateFileW");                                    // open the reparse point without following it
    emitter.instruction("cmp rax, -1");                                         // INVALID_HANDLE_VALUE?
    emitter.instruction("je .Lreadlink_native_fail");                           // capture open failure
    emitter.instruction("mov QWORD PTR [rsp + 72], rax");                       // retain handle
    emitter.instruction("mov rcx, rax");                                        // reparse-point handle
    emitter.instruction("mov rdx, QWORD PTR [rsp + 64]");                       // wide output buffer
    emitter.instruction("mov r8, 32768");                                       // WCHAR capacity
    emitter.instruction("mov r9, 8");                                           // FILE_NAME_OPENED preserves reparse-point naming
    emitter.instruction("call GetFinalPathNameByHandleW");                      // retrieve long final/opened path
    emitter.instruction("mov DWORD PTR [rsp + 96], eax");                       // preserve WCHAR length/status
    emitter.instruction("test eax, eax");                                       // final-path query succeeded?
    emitter.instruction("jnz .Lreadlink_query_ok");                             // close and process successful result
    emitter.instruction("call GetLastError");                                   // capture query failure before CloseHandle
    emitter.instruction("mov DWORD PTR [rsp + 96], eax");                       // preserve native error across close
    emitter.instruction("mov rcx, QWORD PTR [rsp + 72]");                       // opened handle
    emitter.instruction("call CloseHandle");                                    // close on every query result
    emitter.instruction("jmp .Lreadlink_native_fail_saved");                    // cleanup with saved query error
    emitter.label(".Lreadlink_query_ok");
    emitter.instruction("mov rcx, QWORD PTR [rsp + 72]");                       // opened handle
    emitter.instruction("call CloseHandle");                                    // close successful query handle
    emitter.instruction("cmp DWORD PTR [rsp + 96], 32768");                     // result exceeded WCHAR capacity?
    emitter.instruction("jae .Lreadlink_range_fail");                           // reject truncated long path
    emitter.instruction("mov rdi, QWORD PTR [rsp + 64]");                       // wide result base
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // first four UTF-16 code units
    emitter.instruction("mov r10, 0x005c003f005c005c");                         // UTF-16 `\\?\` prefix
    emitter.instruction("cmp rax, r10");                                        // extended path prefix?
    emitter.instruction("jne .Lreadlink_prefix_done");                          // keep ordinary path
    emitter.instruction("mov rax, QWORD PTR [rdi + 8]");                        // code units four through seven
    emitter.instruction("mov r10, 0x005c0043004e0055");                         // UTF-16 `UNC\`
    emitter.instruction("cmp rax, r10");                                        // extended UNC prefix?
    emitter.instruction("jne .Lreadlink_drive_prefix");                         // ordinary extended drive path
    emitter.instruction("mov WORD PTR [rdi + 12], 92");                         // synthesize first UNC slash at old C position
    emitter.instruction("mov WORD PTR [rdi + 14], 92");                         // synthesize second UNC slash
    emitter.instruction("add rdi, 12");                                         // source now begins `\\server`
    emitter.instruction("jmp .Lreadlink_prefix_done");                          // normalized UNC source
    emitter.label(".Lreadlink_drive_prefix");
    emitter.instruction("add rdi, 8");                                          // strip four-WCHAR `\\?\` prefix
    emitter.label(".Lreadlink_prefix_done");
    emitter.instruction("mov rsi, QWORD PTR [rsp + 80]");                       // caller UTF-8 output
    emitter.instruction("mov rdx, QWORD PTR [rsp + 88]");                       // output byte capacity
    emitter.instruction("call __rt_win_utf16_to_utf8");                         // strict conversion including NUL
    emitter.instruction("test eax, eax");                                       // conversion succeeded and fit?
    emitter.instruction("jz .Lreadlink_native_fail");                           // capture conversion/range failure
    emitter.instruction("dec eax");                                             // readlink returns payload bytes excluding NUL
    emitter.instruction("mov DWORD PTR [rsp + 96], eax");                       // preserve result length across cleanup
    emitter.instruction("jmp .Lreadlink_cleanup_success");                      // free both owned buffers
    emitter.label(".Lreadlink_cleanup_success");
    emitter.instruction("mov rax, QWORD PTR [rsp + 64]");                       // owned wide output
    emitter.instruction("call __rt_heap_free");                                 // release output buffer
    emitter.instruction("mov rax, QWORD PTR [rsp + 56]");                       // owned wide input
    emitter.instruction("call __rt_heap_free");                                 // release input conversion
    emitter.instruction("mov eax, DWORD PTR [rsp + 96]");                       // return UTF-8 payload length
    emitter.instruction("add rsp, 104");                                        // restore stack
    emitter.instruction("ret");                                                 // return bytes written
    emitter.label(".Lreadlink_native_fail");
    emitter.instruction("call GetLastError");                                   // capture native/conversion failure
    emitter.instruction("mov DWORD PTR [rsp + 96], eax");                       // preserve error across cleanup
    emitter.label(".Lreadlink_native_fail_saved");
    emitter.instruction("mov rax, QWORD PTR [rsp + 64]");                       // owned wide output
    emitter.instruction("call __rt_heap_free");                                 // release output buffer
    emitter.instruction("mov rax, QWORD PTR [rsp + 56]");                       // owned wide input
    emitter.instruction("call __rt_heap_free");                                 // release input path
    emitter.instruction("mov eax, DWORD PTR [rsp + 96]");                       // restore native error
    emitter.instruction("mov DWORD PTR [rip + __rt_win32_last_error], eax");    // retain native state
    emitter.instruction("call __rt_win32_errno_from_code");                     // translate errno
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], eax");               // publish errno
    emitter.instruction("mov rax, -1");                                         // return failure
    emitter.instruction("add rsp, 104");                                        // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lreadlink_range_fail");
    emitter.instruction("mov rax, QWORD PTR [rsp + 64]");                       // owned wide output
    emitter.instruction("call __rt_heap_free");                                 // release output buffer
    emitter.instruction("mov rax, QWORD PTR [rsp + 56]");                       // owned wide input
    emitter.instruction("call __rt_heap_free");                                 // release input path
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 36");                // ENAMETOOLONG
    emitter.instruction("mov rax, -1");                                         // return failure
    emitter.instruction("add rsp, 104");                                        // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lreadlink_alloc_fail");
    emitter.instruction("mov rax, QWORD PTR [rsp + 56]");                       // owned wide input
    emitter.instruction("call __rt_heap_free");                                 // release input conversion
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 12");                // ENOMEM
    emitter.instruction("mov rax, -1");                                         // return failure
    emitter.instruction("add rsp, 104");                                        // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lreadlink_conversion_fail");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 84");                // EILSEQ
    emitter.instruction("mov rax, -1");                                         // return failure
    emitter.instruction("add rsp, 104");                                        // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
    }

    // lstat: probe the reparse point before stat() so dangling symbolic links succeed.
    let lstat_mode_off = emitter.platform.stat_mode_offset();
    let lstat_size_off = emitter.platform.stat_size_offset();
    let lstat_nlink_off = emitter.platform.stat_nlink_offset();
    let lstat_ino_off = emitter.platform.stat_ino_offset();
    let lstat_dev_off = emitter.platform.stat_dev_offset();
    let lstat_atime_off = emitter.platform.stat_atime_offset();
    let lstat_mtime_off = emitter.platform.stat_mtime_offset();
    let lstat_ctime_off = emitter.platform.stat_ctime_offset();
    emitter.label_global("lstat");
    emitter.instruction("sub rsp, 168");                                        // shadow, CreateFile args, metadata, and owned path/error locals aligned for MSx64
    emitter.instruction("mov QWORD PTR [rsp + 144], rdi");                      // preserve the UTF-8 path across the tag probe
    emitter.instruction("mov QWORD PTR [rsp + 64], rsi");                       // preserve the Linux-layout stat destination
    emitter.instruction("call __rt_lstat_is_symlink");                          // inspect the exact reparse tag without following its target
    emitter.instruction("test eax, eax");                                       // tri-state: 1 = symlink, 0 = normal/junction, -1 = reparse query failure
    emitter.instruction("js .Llstat_probe_fail");                               // php-src propagates FSCTL_GET_REPARSE_POINT failure instead of falling back to stat
    emitter.instruction("jz .Llstat_followed_fallback");                        // junctions and ordinary paths retain normal stat semantics
    // -- symbolic link: reopen the reparse point itself, including a dangling target --
    emitter.instruction("mov rdi, QWORD PTR [rsp + 144]");                      // restore the original UTF-8 link path
    emitter.instruction("call __rt_win_utf8_to_utf16");                         // convert the link name for CreateFileW
    emitter.instruction("test rax, rax");                                       // conversion succeeded?
    emitter.instruction("jz .Llstat_conversion_fail");                          // helper already published EILSEQ
    emitter.instruction("mov QWORD PTR [rsp + 56], rax");                       // retain owned UTF-16 path through metadata collection
    emitter.instruction("mov rcx, rax");                                        // lpFileName = wide link path
    emitter.instruction("xor edx, edx");                                        // dwDesiredAccess = metadata-only
    emitter.instruction("mov r8, 7");                                           // dwShareMode = FILE_SHARE_READ|WRITE|DELETE
    emitter.instruction("xor r9, r9");                                          // lpSecurityAttributes = NULL
    emitter.instruction("mov QWORD PTR [rsp + 32], 3");                         // dwCreationDisposition = OPEN_EXISTING
    emitter.instruction("mov QWORD PTR [rsp + 40], 0x2200000");                 // BACKUP_SEMANTICS | OPEN_REPARSE_POINT: do not follow the link
    emitter.instruction("mov QWORD PTR [rsp + 48], 0");                         // hTemplateFile = NULL
    emitter.instruction("call CreateFileW");                                    // open the symbolic-link object even when its target is missing
    emitter.instruction("cmp rax, -1");                                         // INVALID_HANDLE_VALUE?
    emitter.instruction("je .Llstat_native_fail");                              // preserve the open error through cleanup
    emitter.instruction("mov QWORD PTR [rsp + 72], rax");                       // retain the reparse-point handle
    emitter.instruction("cld");                                                 // forward direction for rep stosb
    emitter.instruction("lea rdi, [rsp + 80]");                                 // destination = BY_HANDLE_FILE_INFORMATION buffer
    emitter.instruction("xor eax, eax");                                        // zero fill byte
    emitter.instruction("mov ecx, 52");                                         // sizeof(BY_HANDLE_FILE_INFORMATION)
    emitter.instruction("rep stosb");                                           // clear the complete metadata buffer
    emitter.instruction("mov rcx, QWORD PTR [rsp + 72]");                       // hFile = opened reparse point
    emitter.instruction("lea rdx, [rsp + 80]");                                 // lpFileInformation = stack metadata buffer
    emitter.instruction("call GetFileInformationByHandle");                     // read link-object size, file index, times, and attributes
    emitter.instruction("test eax, eax");                                       // metadata query succeeded?
    emitter.instruction("jnz .Llstat_metadata_ready");                          // close then populate the stat buffer
    emitter.instruction("call GetLastError");                                   // capture metadata failure before CloseHandle
    emitter.instruction("mov DWORD PTR [rsp + 136], eax");                      // preserve native error across cleanup
    emitter.instruction("mov rcx, QWORD PTR [rsp + 72]");                       // opened reparse-point handle
    emitter.instruction("call CloseHandle");                                    // close even when metadata collection failed
    emitter.instruction("jmp .Llstat_native_fail_saved");                       // release UTF-16 path and translate saved error
    emitter.label(".Llstat_metadata_ready");
    emitter.instruction("mov rcx, QWORD PTR [rsp + 72]");                       // opened reparse-point handle
    emitter.instruction("call CloseHandle");                                    // close before constructing the caller-visible stat buffer
    // -- fill the runtime's Linux-layout stat buffer from the link object's metadata --
    emitter.instruction("cld");                                                 // forward direction for rep stosb
    emitter.instruction("mov rsi, QWORD PTR [rsp + 64]");                       // restore caller stat destination
    emitter.instruction("mov rdi, rsi");                                        // destination = stat buffer base
    emitter.instruction("xor eax, eax");                                        // zero fill byte
    emitter.instruction("mov ecx, 128");                                        // Windows runtime uses the Linux-sized stat layout
    emitter.instruction("rep stosb");                                           // clear unspecified POSIX fields deterministically
    emitter.instruction("mov eax, DWORD PTR [rsp + 116]");                      // nFileSizeLow
    emitter.instruction("mov edx, DWORD PTR [rsp + 112]");                      // nFileSizeHigh
    emitter.instruction("shl rdx, 32");                                         // position the high size dword
    emitter.instruction("or rax, rdx");                                         // combine the 64-bit link-object size
    emitter.instruction(&format!("mov QWORD PTR [rsi + {}], rax", lstat_size_off)); // store st_size without following the target
    emitter.instruction("mov eax, DWORD PTR [rsp + 108]");                      // dwVolumeSerialNumber
    emitter.instruction("test eax, eax");                                       // synthetic Wine volumes may report zero
    emitter.instruction("jnz .Llstat_dev_ready");                               // retain a real native volume id
    emitter.instruction("mov eax, 1");                                          // stable nonzero device fallback
    emitter.label(".Llstat_dev_ready");
    emitter.instruction(&format!("mov QWORD PTR [rsi + {}], rax", lstat_dev_off)); // store st_dev
    emitter.instruction("mov eax, DWORD PTR [rsp + 124]");                      // nFileIndexHigh
    emitter.instruction("shl rax, 32");                                         // position the inode high dword
    emitter.instruction("mov ecx, DWORD PTR [rsp + 128]");                      // nFileIndexLow
    emitter.instruction("or rax, rcx");                                         // combine the stable link-object inode number
    emitter.instruction(&format!("mov QWORD PTR [rsi + {}], rax", lstat_ino_off)); // store st_ino
    emitter.instruction("mov eax, DWORD PTR [rsp + 120]");                      // nNumberOfLinks
    emitter.instruction("test eax, eax");                                       // native link count is present?
    emitter.instruction("jnz .Llstat_nlink_ready");                             // retain a real nonzero count
    emitter.instruction("mov eax, 1");                                          // POSIX-compatible minimum link count
    emitter.label(".Llstat_nlink_ready");
    emitter.instruction(&format!("mov DWORD PTR [rsi + {}], eax", lstat_nlink_off)); // store st_nlink
    emitter.instruction("mov eax, DWORD PTR [rsp + 80]");                       // link-object dwFileAttributes
    emitter.instruction("test eax, 1");                                         // FILE_ATTRIBUTE_READONLY?
    emitter.instruction("jnz .Llstat_mode_readonly");                           // omit write bits for read-only links
    emitter.instruction("mov eax, 0xA1B6");                                     // S_IFLNK | 0666
    emitter.instruction("jmp .Llstat_mode_ready");                              // store the writable-link mode
    emitter.label(".Llstat_mode_readonly");
    emitter.instruction("mov eax, 0xA124");                                     // S_IFLNK | 0444
    emitter.label(".Llstat_mode_ready");
    emitter.instruction(&format!("mov DWORD PTR [rsi + {}], eax", lstat_mode_off)); // store php-src's Windows symlink permissions
    // -- convert the link object's FILETIME values to Unix epoch seconds --
    emit_lstat_filetime_seconds(emitter, 92, lstat_atime_off);
    emit_lstat_filetime_seconds(emitter, 100, lstat_mtime_off);
    emit_lstat_filetime_seconds(emitter, 84, lstat_ctime_off);
    emitter.instruction("mov rax, QWORD PTR [rsp + 56]");                       // owned UTF-16 link path
    emitter.instruction("call __rt_heap_free");                                 // release conversion after all metadata is consumed
    emitter.instruction("xor eax, eax");                                        // lstat succeeded for the symbolic-link object
    emitter.instruction("add rsp, 168");                                        // restore the SysV caller stack
    emitter.instruction("ret");                                                 // return success
    emitter.label(".Llstat_followed_fallback");
    emitter.instruction("mov rdi, QWORD PTR [rsp + 144]");                      // original UTF-8 path for normal stat
    emitter.instruction("mov rsi, QWORD PTR [rsp + 64]");                       // original stat destination
    emitter.instruction("call __rt_sys_stat");                                  // follow junctions/non-reparse paths exactly as stat does
    emitter.instruction("add rsp, 168");                                        // restore the SysV caller stack
    emitter.instruction("ret");                                                 // propagate ordinary stat status
    emitter.label(".Llstat_probe_fail");
    emitter.instruction("mov rax, -1");                                         // probe already published the corresponding POSIX errno
    emitter.instruction("add rsp, 168");                                        // restore the SysV caller stack without invoking followed stat
    emitter.instruction("ret");                                                 // propagate the exact reparse-control failure
    emitter.label(".Llstat_native_fail");
    emitter.instruction("call GetLastError");                                   // capture CreateFileW failure before freeing the path
    emitter.instruction("mov DWORD PTR [rsp + 136], eax");                      // preserve native error across cleanup
    emitter.label(".Llstat_native_fail_saved");
    emitter.instruction("mov rax, QWORD PTR [rsp + 56]");                       // owned UTF-16 link path
    emitter.instruction("call __rt_heap_free");                                 // release conversion on native failure
    emitter.instruction("mov eax, DWORD PTR [rsp + 136]");                      // restore native error code
    emitter.instruction("mov DWORD PTR [rip + __rt_win32_last_error], eax");    // retain diagnostic state
    emitter.instruction("call __rt_win32_errno_from_code");                     // translate native status to POSIX errno
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], eax");               // publish observable errno
    emitter.instruction("mov rax, -1");                                         // report lstat failure
    emitter.instruction("add rsp, 168");                                        // restore the SysV caller stack
    emitter.instruction("ret");                                                 // return failure
    emitter.label(".Llstat_conversion_fail");
    emitter.instruction("mov rax, -1");                                         // UTF conversion helper already set EILSEQ
    emitter.instruction("add rsp, 168");                                        // restore the SysV caller stack
    emitter.instruction("ret");                                                 // return status
    emitter.blank();

    // mmap: delegate to VirtualAlloc via __rt_sys_mmap
    emitter.label_global("mmap");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_mmap");                                  // call VirtualAlloc shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // munmap: delegate to VirtualFree via __rt_sys_munmap
    emitter.label_global("munmap");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_munmap");                                // call VirtualFree shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // mprotect: delegate to VirtualProtect via __rt_sys_mprotect
    emitter.label_global("mprotect");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_mprotect");                              // call VirtualProtect shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // brk: delegate to HeapAlloc via __rt_sys_brk
    emitter.label_global("brk");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_brk");                                   // call HeapAlloc shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // getrandom: delegate to BCryptGenRandom
    emitter.label_global("getrandom");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_getrandom");                             // call BCryptGenRandom shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // write: delegate to __rt_sys_write
    emitter.label_global("write");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_write");                                 // call WriteFile shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // read: delegate to __rt_sys_read
    emitter.label_global("read");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_read");                                  // call ReadFile shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // close: delegate to __rt_sys_close
    emitter.label_global("close");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_close");                                 // call CloseHandle shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // exit: delegate to __rt_sys_exit
    emitter.label_global("exit");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_exit");                                  // call ExitProcess shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // unreachable
    emitter.blank();

    // open: delegate to __rt_sys_open (CreateFileW)
    emitter.label_global("open");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_open");                                  // call CreateFileW shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // fstat: delegate to msvcrt fstat
    emitter.label_global("fstat");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_fstat");                                 // call msvcrt fstat
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // lseek: delegate to SetFilePointerEx
    emitter.label_global("lseek");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_lseek");                                 // call SetFilePointerEx shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // fcntl: delegate to ioctlsocket for socket operations
    emitter.label_global("fcntl");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_ioctl");                                 // call ioctlsocket shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // ioctl: delegate to ioctlsocket
    emitter.label_global("ioctl");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_ioctl");                                 // call ioctlsocket shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // getpid: delegate to GetCurrentProcessId
    emitter.label_global("getpid");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_getpid");                                // call GetCurrentProcessId shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // getuid/getgid: return 0 on Windows (PHP behavior — no Unix UID/GID)
    for sym in &["getuid", "getgid"] {
        emitter.label_global(sym);
        emitter.instruction("xor rax, rax");                                    // return 0 (PHP behavior on Windows)
        emitter.instruction("ret");                                             // return
        emitter.blank();
    }

    // getppid/setuid/setgid: return -1 (ENOSYS) — POSIX-only functions
    for sym in &["getppid", "setuid", "setgid"] {
        emitter.label_global(sym);
        emitter.instruction("mov rax, -1");                                     // return -1 (not supported on Windows)
        emitter.instruction("ret");                                             // return
        emitter.blank();
    }

    // kill: share the strict Win32 implementation used by the syscall surface
    emitter.label_global("kill");
    emitter.instruction("sub rsp, 8");                                          // align before entering the SysV runtime helper
    emitter.instruction("call __rt_sys_kill");                                  // preserve strict success/failure semantics
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // clock_gettime: delegate to GetSystemTimeAsFileTime
    emitter.label_global("clock_gettime");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_clock_gettime");                         // call clock_gettime shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // accept4: delegate to accept (Windows doesn't have accept4)
    emitter.label_global("accept4");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_accept4");                               // call accept shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // writev: loop over iovec array calling __rt_sys_write for each entry
    emitter.label_global("writev");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_writev");                                // call writev stub
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // sysinfo: stub
    emitter.label_global("sysinfo");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_sysinfo");                               // call sysinfo stub
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // uname: delegate to msvcrt
    emitter.label_global("uname");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_uname");                                 // call uname shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // execve: delegate to msvcrt _execvp (replaces current process)
    emitter.label_global("execve");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rdi");                                        // path
    emitter.instruction("mov rdx, rsi");                                        // argv
    emitter.instruction("call _execvp");                                        // execute program (replaces process)
    emitter.instruction("add rsp, 40");                                         // restore stack (only reached on failure)
    emitter.instruction("ret");                                                 // return -1 on failure (rax from _execvp)
    emitter.blank();

    // futex: stub
    emitter.label_global("futex");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_futex");                                 // call futex stub
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // utimensat: open the file with FILE_WRITE_ATTRIBUTES and apply the requested
    // atime/mtime via SetFileTime. Entry (SysV, matching the Linux utimensat ABI
    // `__rt_touch` in modify_x86_64.rs uses): rdi=AT_FDCWD (ignored — Windows has
    // no dirfd), rsi=path, rdx=timespec[2]* (ts[0]=atime{tv_sec@0,tv_nsec@8},
    // ts[1]=mtime{tv_sec@16,tv_nsec@24}), rcx=flags (ignored). rdx is MSx64
    // volatile and gets reused as CreateFileW's dwDesiredAccess, so the
    // timespec[2] pointer is saved to a stack slot before that call; rsi (path)
    // needs no saving since it is only read once, before any call, and is itself
    // MSx64 non-volatile.
    emitter.label_global("utimensat");
    emitter.instruction("sub rsp, 120");                                        // CreateFile stack args, times, owned wide path, and error locals
    emitter.instruction("mov QWORD PTR [rsp + 56], rdx");                       // save timespec pointer before rdx becomes CreateFileW access
    emitter.instruction("mov rdi, rsi");                                        // path is SysV argument two
    emitter.instruction("call __rt_win_utf8_to_utf16");                         // strictly convert target path
    emitter.instruction("test rax, rax");                                       // conversion succeeded?
    emitter.instruction("jz .Lutimensat_conversion_fail");                      // invalid UTF-8 path
    emitter.instruction("mov QWORD PTR [rsp + 88], rax");                       // retain owned wide path
    emitter.instruction("mov rcx, rax");                                        // lpFileName = UTF-16 path
    emitter.instruction("mov rdx, 0x100");                                      // dwDesiredAccess = FILE_WRITE_ATTRIBUTES
    emitter.instruction("mov r8, 7");                                           // dwShareMode = FILE_SHARE_READ|WRITE|DELETE
    emitter.instruction("xor r9, r9");                                          // lpSecurityAttributes = NULL
    emitter.instruction("mov QWORD PTR [rsp + 32], 3");                         // dwCreationDisposition = OPEN_EXISTING
    emitter.instruction("mov QWORD PTR [rsp + 40], 0x2000000");                 // dwFlagsAndAttributes = FILE_FLAG_BACKUP_SEMANTICS (lets directories open)
    emitter.instruction("mov QWORD PTR [rsp + 48], 0");                         // hTemplateFile = NULL
    emitter.instruction("call CreateFileW");                                    // open Unicode target for timestamp update
    emitter.instruction("cmp rax, -1");                                         // INVALID_HANDLE_VALUE?
    emitter.instruction("je .Lutimensat_fail");                                 // jump if the file could not be opened
    emitter.instruction("mov QWORD PTR [rsp + 64], rax");                       // save the handle across the FILETIME setup and SetFileTime call
    // -- atime: timespec[0] = {tv_sec@+0, tv_nsec@+8} --
    emitter.instruction("mov rax, QWORD PTR [rsp + 56]");                       // reload the timespec[2] pointer
    emitter.instruction("mov rdx, QWORD PTR [rax + 8]");                        // atime tv_nsec
    emitter.instruction("cmp rdx, 0x3FFFFFFF");                                 // UTIME_NOW sentinel?
    emitter.instruction("je .Lutimensat_atime_now");                            // → query the current time
    emitter.instruction("mov rcx, QWORD PTR [rax]");                            // atime tv_sec
    emitter.instruction("mov r8, 10000000");                                    // 100ns intervals per second
    emitter.instruction("imul rcx, r8");                                        // seconds -> 100ns ticks
    emitter.instruction("mov r8, 116444736000000000");                          // 1970->1601 epoch offset in 100ns units
    emitter.instruction("add rcx, r8");                                         // FILETIME ticks since 1601
    emitter.instruction("mov QWORD PTR [rsp + 72], rcx");                       // store the atime FILETIME
    emitter.instruction("jmp .Lutimensat_mtime");                               // continue with mtime
    emitter.label(".Lutimensat_atime_now");
    emitter.instruction("lea rcx, [rsp + 72]");                                 // lpSystemTimeAsFileTime out-param
    emitter.instruction("call GetSystemTimeAsFileTime");                        // atime = current time
    emitter.label(".Lutimensat_mtime");
    // -- mtime: timespec[1] = {tv_sec@+16, tv_nsec@+24} --
    emitter.instruction("mov rax, QWORD PTR [rsp + 56]");                       // reload the timespec[2] pointer
    emitter.instruction("mov rdx, QWORD PTR [rax + 24]");                       // mtime tv_nsec
    emitter.instruction("cmp rdx, 0x3FFFFFFF");                                 // UTIME_NOW sentinel?
    emitter.instruction("je .Lutimensat_mtime_now");                            // → query the current time
    emitter.instruction("mov rcx, QWORD PTR [rax + 16]");                       // mtime tv_sec
    emitter.instruction("mov r8, 10000000");                                    // 100ns intervals per second
    emitter.instruction("imul rcx, r8");                                        // seconds -> 100ns ticks
    emitter.instruction("mov r8, 116444736000000000");                          // 1970->1601 epoch offset in 100ns units
    emitter.instruction("add rcx, r8");                                         // FILETIME ticks since 1601
    emitter.instruction("mov QWORD PTR [rsp + 80], rcx");                       // store the mtime FILETIME
    emitter.instruction("jmp .Lutimensat_set");                                 // proceed to SetFileTime
    emitter.label(".Lutimensat_mtime_now");
    emitter.instruction("lea rcx, [rsp + 80]");                                 // lpSystemTimeAsFileTime out-param
    emitter.instruction("call GetSystemTimeAsFileTime");                        // mtime = current time
    emitter.label(".Lutimensat_set");
    emitter.instruction("mov rcx, QWORD PTR [rsp + 64]");                       // hFile = the opened handle
    emitter.instruction("xor rdx, rdx");                                        // lpCreationTime = NULL (leave creation time untouched)
    emitter.instruction("lea r8, [rsp + 72]");                                  // lpLastAccessTime = &atime FILETIME
    emitter.instruction("lea r9, [rsp + 80]");                                  // lpLastWriteTime = &mtime FILETIME
    emitter.instruction("call SetFileTime");                                    // apply the requested access/modify timestamps
    emitter.instruction("test eax, eax");                                       // timestamp update succeeded?
    emitter.instruction("jz .Lutimensat_set_fail");                             // capture error before closing handle
    emitter.instruction("mov rcx, QWORD PTR [rsp + 64]");                       // reload the handle
    emitter.instruction("call CloseHandle");                                    // release the handle
    emitter.instruction("mov rax, QWORD PTR [rsp + 88]");                       // owned wide path
    emitter.instruction("call __rt_heap_free");                                 // release path conversion
    emitter.instruction("add rsp, 120");                                        // restore stack
    emitter.instruction("xor rax, rax");                                        // return 0
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lutimensat_set_fail");
    emitter.instruction("call GetLastError");                                   // capture SetFileTime failure
    emitter.instruction("mov DWORD PTR [rsp + 96], eax");                       // preserve error across close
    emitter.instruction("mov rcx, QWORD PTR [rsp + 64]");                       // opened handle
    emitter.instruction("call CloseHandle");                                    // close failed update handle
    emitter.instruction("jmp .Lutimensat_fail_saved");                          // cleanup with saved error
    emitter.label(".Lutimensat_fail");
    emitter.instruction("call GetLastError");                                   // capture CreateFileW failure
    emitter.instruction("mov DWORD PTR [rsp + 96], eax");                       // preserve native error
    emitter.label(".Lutimensat_fail_saved");
    emitter.instruction("mov rax, QWORD PTR [rsp + 88]");                       // owned wide path
    emitter.instruction("call __rt_heap_free");                                 // release path conversion
    emitter.instruction("mov eax, DWORD PTR [rsp + 96]");                       // restore native error
    emitter.instruction("mov DWORD PTR [rip + __rt_win32_last_error], eax");    // retain native state
    emitter.instruction("call __rt_win32_errno_from_code");                     // translate errno
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], eax");               // publish errno
    emitter.instruction("mov rax, -1");                                         // return -1 on failure
    emitter.instruction("add rsp, 120");                                        // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lutimensat_conversion_fail");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 84");                // EILSEQ
    emitter.instruction("mov rax, -1");                                         // return failure
    emitter.instruction("add rsp, 120");                                        // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // fsync: delegate to FlushFileBuffers
    emitter.label_global("fsync");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rdi");                                        // fd
    emitter.instruction("call __rt_fd_to_handle");                              // convert fd to HANDLE
    emitter.instruction("mov rcx, rax");                                        // handle
    emitter.instruction("call FlushFileBuffers");                               // flush file buffers to disk
    emitter.instruction("test eax, eax");                                       // Win32 BOOL success?
    emitter.instruction("jz .Lfsync_fail");                                     // translate a native flush failure
    emitter.instruction("xor eax, eax");                                        // POSIX fsync success is zero
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return success
    emitter.label(".Lfsync_fail");
    emitter.instruction("call GetLastError");                                   // capture the failed flush error
    emitter.instruction("mov DWORD PTR [rip + __rt_win32_last_error], eax");    // preserve native error state
    emitter.instruction("call __rt_win32_errno_from_code");                     // translate to POSIX errno
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], eax");               // publish observable errno
    emitter.instruction("mov eax, -1");                                         // POSIX fsync failure
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return failure
    emitter.blank();

    // __rt_sys_fdatasync: msvcrt has no `fdatasync` export. FlushFileBuffers
    // (the `fsync` stub-delegate above) already flushes both data AND
    // metadata, satisfying fdatasync's (weaker) contract — the same
    // fallback `modify_x86_64.rs` already takes on Darwin (which also lacks
    // `fdatasync`). Entered with fd in rdi (SysV — the emit_call_c call site
    // is unchanged, only the symbol name routes here), so a bare tail-call
    // preserves rdi for `fsync` unmodified; no frame of its own is needed.
    emitter.label_global("__rt_sys_fdatasync");
    emitter.instruction("jmp fsync");                                           // tail-call: fsync's FlushFileBuffers already satisfies fdatasync
    emitter.blank();

    // chown/lchown/fchown: return -1 (ENOSYS) — Windows uses ACLs not Unix ownership
    for sym in &["chown", "lchown", "fchown"] {
        emitter.label_global(sym);
        emitter.instruction("mov DWORD PTR [rip + __rt_errno], 38");            // ENOSYS: never report an unsupported ownership change as success
        emitter.instruction("mov rax, -1");                                     // return -1 (not supported on Windows)
        emitter.instruction("ret");                                             // return
        emitter.blank();
    }

    // glob: real FindFirstFileExW/FindNextFileW enumeration. Builds a
    // glibc-layout `glob_t` (gl_pathc @ offset 0, gl_pathv @ offset 8 — see
    // `Platform::glob_pathv_offset`) into the caller's `glob_t*` (SysV arg4,
    // rcx), which is exactly what `__rt_glob`/`opendir_glob.rs` already read
    // on every other target. Win32 wildcard matching has no escape-character
    // concept at all (backslash is only ever a path separator), so this is
    // inherently GLOB_NOESCAPE — matching php-src's win32/glob.c — with no
    // extra code needed. `.` and `..` are always filtered (POSIX/php-src
    // glob() semantics; contrast with `opendir`/`readdir`, which DO return
    // dot entries). Each matched leaf name from `WIN32_FIND_DATAW.cFileName`
    // is converted back to UTF-8 and reassembled with the pattern's own
    // directory prefix into a freshly `__rt_heap_alloc`'d C string. The
    // `gl_pathv` array starts small and doubles as needed; growth preserves
    // every owned path, checks the allocator's 32-bit payload-size contract,
    // and cleans up the complete partial result on ENOMEM. `globfree` below
    // frees every block this allocates.
    emitter.label_global("glob");
    emitter.instruction("sub rsp, 760");                                        // shadow, WIN32_FIND_DATAW, and aligned glob locals
    emitter.instruction("mov QWORD PTR [rsp + 656], rdi");                      // save the pattern C-string pointer (SysV arg1)
    emitter.instruction("mov QWORD PTR [rsp + 680], rcx");                      // save the caller's glob_t* (SysV arg4) before it becomes MSx64 scratch
    // -- compute dirpart length: index just past the LAST '/' or '\' in the pattern (0 = none) --
    emitter.instruction("xor r8, r8");                                          // scan index
    emitter.instruction("xor r9, r9");                                          // dirpart length found so far
    emitter.label(".Lglob_dirscan");
    emitter.instruction("movzx eax, BYTE PTR [rdi + r8]");                      // load the next pattern byte
    emitter.instruction("test al, al");                                         // stop at the terminating NUL
    emitter.instruction("jz .Lglob_dirscan_done");                              // dirpart length is now final
    emitter.instruction("cmp al, 47");                                          // '/'?
    emitter.instruction("je .Lglob_dirscan_sep");                               // → record this separator
    emitter.instruction("cmp al, 92");                                          // '\'?
    emitter.instruction("jne .Lglob_dirscan_next");                             // neither separator: keep scanning
    emitter.label(".Lglob_dirscan_sep");
    emitter.instruction("lea r9, [r8 + 1]");                                    // dirpart length = index + 1 (keep the separator itself)
    emitter.label(".Lglob_dirscan_next");
    emitter.instruction("inc r8");                                              // advance the scan index
    emitter.instruction("jmp .Lglob_dirscan");                                  // continue scanning for the LAST separator
    emitter.label(".Lglob_dirscan_done");
    emitter.instruction("mov QWORD PTR [rsp + 664], r9");                       // save the dirpart length
    emitter.instruction("mov QWORD PTR [rsp + 688], 0");                        // initialize match count before any fallible allocation
    emitter.instruction("mov QWORD PTR [rsp + 704], 16");                       // initial gl_pathv capacity in pointer slots
    emitter.instruction("mov QWORD PTR [rsp + 744], 0");                        // no owned wide pattern yet
    // -- allocate a small gl_pathv array and grow it geometrically on demand --
    emitter.instruction("mov rax, 128");                                        // initial 16 pointer slots * 8 bytes
    emitter.instruction("call __rt_heap_alloc");                                // rax = gl_pathv array storage
    emitter.instruction("mov QWORD PTR [rsp + 696], rax");                      // save the array pointer
    emitter.instruction("test rax, rax");                                       // allocation succeeded?
    emitter.instruction("jz .Lglob_nospace_no_handle");                         // publish ENOMEM without entering enumeration
    emitter.instruction("mov rdi, QWORD PTR [rsp + 656]");                      // UTF-8 wildcard pattern
    emitter.instruction("call __rt_win_utf8_to_utf16");                         // allocate native Unicode wildcard pattern
    emitter.instruction("mov QWORD PTR [rsp + 744], rax");                      // preserve wide pattern through enumeration
    emitter.instruction("test rax, rax");                                       // strict conversion succeeded?
    emitter.instruction("jz .Lglob_nomatch");                                   // invalid UTF-8 has no Windows match
    // -- FindFirstFileExW(pattern, FindExInfoBasic, &findData, NameMatch, NULL, LARGE_FETCH) --
    emitter.instruction("mov rcx, rax");                                        // lpFileName = wide pattern
    emitter.instruction("mov edx, 1");                                          // FindExInfoBasic
    emitter.instruction("lea r8, [rsp + 64]");                                  // lpFindFileData = &WIN32_FIND_DATAW
    emitter.instruction("xor r9d, r9d");                                        // FindExSearchNameMatch
    emitter.instruction("mov QWORD PTR [rsp + 32], 0");                         // no search filter
    emitter.instruction("mov QWORD PTR [rsp + 40], 2");                         // FIND_FIRST_EX_LARGE_FETCH
    emitter.instruction("call FindFirstFileExW");                               // find the first Unicode matching entry
    emitter.instruction("mov QWORD PTR [rsp + 672], rax");                      // save hFind
    emitter.instruction("cmp rax, -1");                                         // INVALID_HANDLE_VALUE?
    emitter.instruction("je .Lglob_nomatch");                                   // no match at all: report GLOB_NOMATCH
    emitter.label(".Lglob_loop");
    // -- skip "." and ".." (glob() never returns them, unlike opendir/readdir) --
    emitter.instruction("lea rax, [rsp + 64 + 44]");                            // wide cFileName
    emitter.instruction("movzx ecx, WORD PTR [rax]");                           // cFileName[0]
    emitter.instruction("cmp cx, 46");                                          // '.'?
    emitter.instruction("jne .Lglob_not_dot");                                  // not a dot entry
    emitter.instruction("movzx ecx, WORD PTR [rax + 2]");                       // cFileName[1]
    emitter.instruction("test cx, cx");                                         // NUL right after the first '.'?
    emitter.instruction("jz .Lglob_skip_entry");                                // "." — drop it
    emitter.instruction("cmp cx, 46");                                          // second '.'?
    emitter.instruction("jne .Lglob_not_dot");                                  // some other ".x" name: keep it
    emitter.instruction("movzx ecx, WORD PTR [rax + 4]");                       // cFileName[2]
    emitter.instruction("test cx, cx");                                         // NUL right after ".."?
    emitter.instruction("jz .Lglob_skip_entry");                                // ".." — drop it
    emitter.label(".Lglob_not_dot");
    // -- double gl_pathv when count reaches capacity, preserving owned paths --
    emitter.instruction("mov rax, QWORD PTR [rsp + 688]");                      // reload the current match count
    emitter.instruction("cmp rax, QWORD PTR [rsp + 704]");                      // has the array reached its current capacity?
    emitter.instruction("jb .Lglob_capacity_ready");                            // there is already room for this match
    emitter.instruction("mov rcx, QWORD PTR [rsp + 704]");                      // old capacity in pointer slots
    emitter.instruction("cmp rcx, 0x10000000");                                 // would doubling exceed the heap allocator's 32-bit payload size?
    emitter.instruction("jae .Lglob_nospace_open");                             // reject arithmetic overflow as ENOMEM
    emitter.instruction("lea rax, [rcx + rcx]");                                // new capacity = old capacity * 2
    emitter.instruction("mov QWORD PTR [rsp + 704], rax");                      // retain the new capacity across allocation
    emitter.instruction("shl rax, 3");                                          // convert pointer slots to allocation bytes
    emitter.instruction("call __rt_heap_alloc");                                // allocate the larger pointer array
    emitter.instruction("test rax, rax");                                       // allocation succeeded?
    emitter.instruction("jz .Lglob_nospace_open");                              // keep the old array intact for complete cleanup
    emitter.instruction("mov QWORD PTR [rsp + 752], rax");                      // preserve the new array while copying and freeing
    emitter.instruction("xor rcx, rcx");                                        // copy index = 0
    emitter.label(".Lglob_grow_copy");
    emitter.instruction("cmp rcx, QWORD PTR [rsp + 688]");                      // copied every populated pointer?
    emitter.instruction("jae .Lglob_grow_copy_done");                           // switch ownership to the new array
    emitter.instruction("mov r10, QWORD PTR [rsp + 696]");                      // old array base
    emitter.instruction("mov r11, QWORD PTR [r10 + rcx * 8]");                  // load one owned path pointer
    emitter.instruction("mov r10, QWORD PTR [rsp + 752]");                      // new array base
    emitter.instruction("mov QWORD PTR [r10 + rcx * 8], r11");                  // preserve the path in the corresponding slot
    emitter.instruction("inc rcx");                                             // advance to the next populated pointer
    emitter.instruction("jmp .Lglob_grow_copy");                                // continue copying the partial result
    emitter.label(".Lglob_grow_copy_done");
    emitter.instruction("mov rax, QWORD PTR [rsp + 696]");                      // old gl_pathv allocation
    emitter.instruction("call __rt_heap_free");                                 // release obsolete pointer storage only
    emitter.instruction("mov rax, QWORD PTR [rsp + 752]");                      // reload the replacement array
    emitter.instruction("mov QWORD PTR [rsp + 696], rax");                      // publish the grown gl_pathv base
    emitter.label(".Lglob_capacity_ready");
    // -- allocate prefix plus the worst-case UTF-8 expansion of MAX_PATH WCHARs --
    emitter.instruction("mov rax, QWORD PTR [rsp + 664]");                      // dirpart length
    emitter.instruction("add rax, 1040");                                       // four UTF-8 bytes per WCHAR including terminator capacity
    emitter.instruction("jc .Lglob_nospace_open");                              // reject prefix-size arithmetic overflow as ENOMEM
    emitter.instruction("mov r10, rax");                                        // preserve the request while checking its high 32 bits
    emitter.instruction("shr r10, 32");                                         // isolate bytes outside the allocator's payload-size field
    emitter.instruction("jnz .Lglob_nospace_open");                             // reject size truncation as ENOMEM
    emitter.instruction("call __rt_heap_alloc");                                // rax = new path buffer
    emitter.instruction("mov QWORD PTR [rsp + 712], rax");                      // save the buffer pointer
    emitter.instruction("test rax, rax");                                       // path allocation succeeded?
    emitter.instruction("jz .Lglob_nospace_open");                              // clean the complete partial result on ENOMEM
    // -- copy the dirpart prefix from the original pattern --
    emitter.instruction("mov r10, QWORD PTR [rsp + 712]");                      // dest buffer
    emitter.instruction("mov r11, QWORD PTR [rsp + 656]");                      // src = the original pattern
    emitter.instruction("xor r9, r9");                                          // copy index
    emitter.instruction("mov r8, QWORD PTR [rsp + 664]");                       // dirpart length
    emitter.label(".Lglob_copy_dir");
    emitter.instruction("cmp r9, r8");                                          // copied the whole dirpart prefix?
    emitter.instruction("jae .Lglob_copy_dir_done");                            // → append the leaf name next
    emitter.instruction("movzx eax, BYTE PTR [r11 + r9]");                      // load the next dirpart byte
    emitter.instruction("mov BYTE PTR [r10 + r9], al");                         // store it into the destination buffer
    emitter.instruction("inc r9");                                              // advance the copy index
    emitter.instruction("jmp .Lglob_copy_dir");                                 // continue copying the dirpart prefix
    emitter.label(".Lglob_copy_dir_done");
    // -- append the matched wide leaf name as strict UTF-8 --
    emitter.instruction("lea rsi, [r10 + r8]");                                 // destination cursor after the UTF-8 prefix
    emitter.instruction("lea rdi, [rsp + 64 + 44]");                            // source WIN32_FIND_DATAW.cFileName
    emitter.instruction("mov rdx, 1040");                                       // destination capacity
    emitter.instruction("call __rt_win_utf16_to_utf8");                         // convert the native leaf name
    emitter.instruction("test eax, eax");                                       // conversion succeeded?
    emitter.instruction("jz .Lglob_conversion_fail");                           // drop unrepresentable native entries safely
    // -- store the new path pointer into gl_pathv[count] and advance count --
    emitter.instruction("mov rax, QWORD PTR [rsp + 688]");                      // reload the current match count
    emitter.instruction("mov r10, QWORD PTR [rsp + 696]");                      // gl_pathv array base
    emitter.instruction("mov r11, QWORD PTR [rsp + 712]");                      // the reassembled full-path buffer
    emitter.instruction("mov QWORD PTR [r10 + rax * 8], r11");                  // gl_pathv[count] = full path
    emitter.instruction("inc rax");                                             // count++
    emitter.instruction("mov QWORD PTR [rsp + 688], rax");                      // save the updated match count
    emitter.instruction("jmp .Lglob_skip_entry");                               // continue enumeration
    emitter.label(".Lglob_conversion_fail");
    emitter.instruction("mov rax, QWORD PTR [rsp + 712]");                      // path allocation for the failed conversion
    emitter.instruction("call __rt_heap_free");                                 // avoid leaking a dropped entry
    emitter.label(".Lglob_skip_entry");
    emitter.instruction("mov rcx, QWORD PTR [rsp + 672]");                      // hFind
    emitter.instruction("lea rdx, [rsp + 64]");                                 // &WIN32_FIND_DATAW reused for the next entry
    emitter.instruction("call FindNextFileW");                                  // advance to the next Unicode directory entry
    emitter.instruction("test eax, eax");                                       // more entries?
    emitter.instruction("jnz .Lglob_loop");                                     // continue the enumeration loop
    emitter.instruction("mov rcx, QWORD PTR [rsp + 672]");                      // hFind
    emitter.instruction("call FindClose");                                      // release the Win32 search handle
    emitter.instruction("mov rax, QWORD PTR [rsp + 744]");                      // retained UTF-16 wildcard pattern
    emitter.instruction("call __rt_heap_free");                                 // release native pattern after enumeration
    // -- sort gl_pathv[0..count) lexicographically (selection sort): Win32
    //    directory enumeration order is not guaranteed, but every other
    //    target's libc glob() returns sorted matches and callers (e.g. the
    //    `glob://` stream wrapper's ordered readdir() iteration) rely on it —
    //    matches php-src's own win32/glob.c, which also explicitly sorts. --
    emitter.instruction("mov QWORD PTR [rsp + 720], 0");                        // i = 0
    emitter.label(".Lglob_sort_outer");
    emitter.instruction("mov rax, QWORD PTR [rsp + 720]");                      // i
    emitter.instruction("mov rcx, QWORD PTR [rsp + 688]");                      // count
    emitter.instruction("cmp rcx, 1");                                          // zero or one result is already sorted
    emitter.instruction("jbe .Lglob_sort_done");                                // avoid count-1 underflow for an empty filtered result
    emitter.instruction("dec rcx");                                             // count - 1
    emitter.instruction("cmp rax, rcx");                                        // i < count - 1?
    emitter.instruction("jae .Lglob_sort_done");                                // fewer than 2 remaining: sorted
    emitter.instruction("mov QWORD PTR [rsp + 728], rax");                      // min_idx = i
    emitter.instruction("lea rax, [rax + 1]");                                  // j = i + 1
    emitter.instruction("mov QWORD PTR [rsp + 736], rax");                      // save j
    emitter.label(".Lglob_sort_inner");
    emitter.instruction("mov rax, QWORD PTR [rsp + 736]");                      // j
    emitter.instruction("cmp rax, QWORD PTR [rsp + 688]");                      // j < count?
    emitter.instruction("jae .Lglob_sort_inner_done");                          // inner scan finished
    emitter.instruction("mov r10, QWORD PTR [rsp + 696]");                      // gl_pathv array base
    emitter.instruction("mov r11, QWORD PTR [r10 + rax * 8]");                  // pathv[j]
    emitter.instruction("mov rcx, QWORD PTR [rsp + 728]");                      // min_idx
    emitter.instruction("mov r9, QWORD PTR [r10 + rcx * 8]");                   // pathv[min_idx]
    emitter.instruction("xor rcx, rcx");                                        // strcmp byte index
    emitter.label(".Lglob_strcmp_loop");
    emitter.instruction("movzx eax, BYTE PTR [r11 + rcx]");                     // pathv[j][idx]
    emitter.instruction("movzx edx, BYTE PTR [r9 + rcx]");                      // pathv[min_idx][idx]
    emitter.instruction("cmp al, dl");                                          // compare this byte pair
    emitter.instruction("jl .Lglob_strcmp_less");                               // pathv[j] sorts before pathv[min_idx]
    emitter.instruction("jg .Lglob_strcmp_next_j");                             // pathv[j] sorts after: min_idx stays
    emitter.instruction("test al, al");                                         // equal bytes — both strings ended (NUL)?
    emitter.instruction("jz .Lglob_strcmp_next_j");                             // identical strings: min_idx stays (stable)
    emitter.instruction("inc rcx");                                             // advance to the next byte pair
    emitter.instruction("jmp .Lglob_strcmp_loop");                              // keep comparing
    emitter.label(".Lglob_strcmp_less");
    emitter.instruction("mov rax, QWORD PTR [rsp + 736]");                      // j
    emitter.instruction("mov QWORD PTR [rsp + 728], rax");                      // min_idx = j
    emitter.label(".Lglob_strcmp_next_j");
    emitter.instruction("mov rax, QWORD PTR [rsp + 736]");                      // reload j
    emitter.instruction("inc rax");                                             // j++
    emitter.instruction("mov QWORD PTR [rsp + 736], rax");                      // save the advanced j
    emitter.instruction("jmp .Lglob_sort_inner");                               // continue the inner scan
    emitter.label(".Lglob_sort_inner_done");
    emitter.instruction("mov rax, QWORD PTR [rsp + 720]");                      // i
    emitter.instruction("mov rcx, QWORD PTR [rsp + 728]");                      // min_idx
    emitter.instruction("cmp rax, rcx");                                        // already in place?
    emitter.instruction("je .Lglob_sort_no_swap");                              // nothing to swap
    emitter.instruction("mov r10, QWORD PTR [rsp + 696]");                      // gl_pathv array base
    emitter.instruction("mov r11, QWORD PTR [r10 + rax * 8]");                  // pathv[i]
    emitter.instruction("mov r9, QWORD PTR [r10 + rcx * 8]");                   // pathv[min_idx]
    emitter.instruction("mov QWORD PTR [r10 + rax * 8], r9");                   // pathv[i] = old pathv[min_idx]
    emitter.instruction("mov QWORD PTR [r10 + rcx * 8], r11");                  // pathv[min_idx] = old pathv[i]
    emitter.label(".Lglob_sort_no_swap");
    emitter.instruction("mov rax, QWORD PTR [rsp + 720]");                      // reload i
    emitter.instruction("inc rax");                                             // i++
    emitter.instruction("mov QWORD PTR [rsp + 720], rax");                      // save the advanced i
    emitter.instruction("jmp .Lglob_sort_outer");                               // select the next minimum
    emitter.label(".Lglob_sort_done");
    emitter.instruction("mov rcx, QWORD PTR [rsp + 680]");                      // the caller's glob_t*
    emitter.instruction("mov rax, QWORD PTR [rsp + 688]");                      // final match count
    emitter.instruction("mov QWORD PTR [rcx], rax");                            // gl_pathc = match count
    emitter.instruction("mov rax, QWORD PTR [rsp + 696]");                      // gl_pathv array pointer
    emitter.instruction("mov QWORD PTR [rcx + 8], rax");                        // gl_pathv = array pointer
    emitter.instruction("xor rax, rax");                                        // return 0 (success)
    emitter.instruction("add rsp, 760");                                        // restore stack
    emitter.instruction("ret");                                                 // return
    // -- allocation/overflow failure: close enumeration and free the partial result --
    emitter.label(".Lglob_nospace_open");
    emitter.instruction("mov rcx, QWORD PTR [rsp + 672]");                      // active Win32 enumeration handle
    emitter.instruction("call FindClose");                                      // release the search handle before heap cleanup
    emitter.instruction("mov rax, QWORD PTR [rsp + 744]");                      // owned UTF-16 wildcard pattern
    emitter.instruction("test rax, rax");                                       // was the conversion allocated?
    emitter.instruction("jz .Lglob_nospace_paths");                             // skip a null pattern allocation
    emitter.instruction("call __rt_heap_free");                                 // release the native wildcard pattern
    emitter.label(".Lglob_nospace_paths");
    emitter.instruction("mov QWORD PTR [rsp + 720], 0");                        // cleanup index = 0
    emitter.label(".Lglob_nospace_path_loop");
    emitter.instruction("mov rcx, QWORD PTR [rsp + 720]");                      // reload cleanup index
    emitter.instruction("cmp rcx, QWORD PTR [rsp + 688]");                      // released every completed path?
    emitter.instruction("jae .Lglob_nospace_array");                            // continue with pointer-array cleanup
    emitter.instruction("mov r10, QWORD PTR [rsp + 696]");                      // current gl_pathv array base
    emitter.instruction("mov rax, QWORD PTR [r10 + rcx * 8]");                  // one completed owned path
    emitter.instruction("call __rt_heap_free");                                 // release the partial result path
    emitter.instruction("mov rcx, QWORD PTR [rsp + 720]");                      // restore cleanup index after the call
    emitter.instruction("inc rcx");                                             // advance to the next completed path
    emitter.instruction("mov QWORD PTR [rsp + 720], rcx");                      // persist the cleanup index
    emitter.instruction("jmp .Lglob_nospace_path_loop");                        // release the rest of the partial result
    emitter.label(".Lglob_nospace_array");
    emitter.instruction("mov rax, QWORD PTR [rsp + 696]");                      // current gl_pathv allocation
    emitter.instruction("test rax, rax");                                       // was pointer storage allocated?
    emitter.instruction("jz .Lglob_nospace_no_handle");                         // initial allocation failure has nothing to free
    emitter.instruction("call __rt_heap_free");                                 // release pointer-array storage
    emitter.label(".Lglob_nospace_no_handle");
    emitter.instruction("mov rcx, QWORD PTR [rsp + 680]");                      // caller's glob_t output
    emitter.instruction("mov QWORD PTR [rcx], 0");                              // leave gl_pathc empty on failure
    emitter.instruction("mov QWORD PTR [rcx + 8], 0");                          // leave gl_pathv null on failure
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 12");                // ENOMEM for allocation or representable-size exhaustion
    emitter.instruction("mov rax, 1");                                          // return GLOB_NOSPACE
    emitter.instruction("add rsp, 760");                                        // restore stack
    emitter.instruction("ret");                                                 // return failure after complete cleanup
    emitter.label(".Lglob_nomatch");
    emitter.instruction("mov rax, QWORD PTR [rsp + 744]");                      // converted wildcard pattern, possibly NULL
    emitter.instruction("test rax, rax");                                       // was conversion allocated?
    emitter.instruction("jz .Lglob_nomatch_free_pathv");                        // skip NULL cleanup
    emitter.instruction("call __rt_heap_free");                                 // release wide pattern after failed enumeration
    emitter.label(".Lglob_nomatch_free_pathv");
    emitter.instruction("mov rax, QWORD PTR [rsp + 696]");                      // the unused gl_pathv array allocation
    emitter.instruction("call __rt_heap_free");                                 // release it before reporting failure
    emitter.instruction("mov rax, 1");                                          // return GLOB_NOMATCH (any nonzero — callers only test != 0)
    emitter.instruction("add rsp, 760");                                        // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // globfree: releases every block `glob` (above) allocated — each
    // matched path string plus the gl_pathv array itself. SysV: rdi = &glob_t
    // (gl_pathc @ offset 0, gl_pathv @ offset 8), matching every call site
    // (`__rt_glob`, `opendir_glob.rs`, `closedir.rs`).
    emitter.label_global("globfree");
    emitter.instruction("sub rsp, 40");                                         // locals: count(8) + array ptr(8) + index(8) + pad(16), 16B aligned
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // gl_pathc
    emitter.instruction("mov QWORD PTR [rsp], rax");                            // save the match count
    emitter.instruction("mov rax, QWORD PTR [rdi + 8]");                        // gl_pathv
    emitter.instruction("mov QWORD PTR [rsp + 8], rax");                        // save the array pointer
    emitter.instruction("test rax, rax");                                       // was an array ever allocated?
    emitter.instruction("jz .Lglobfree_done");                                  // nothing to free (a never-populated glob_t)
    emitter.instruction("mov QWORD PTR [rsp + 16], 0");                         // index = 0
    emitter.label(".Lglobfree_loop");
    emitter.instruction("mov rcx, QWORD PTR [rsp + 16]");                       // reload the current index
    emitter.instruction("cmp rcx, QWORD PTR [rsp]");                            // freed every matched path string?
    emitter.instruction("jae .Lglobfree_free_array");                           // → free the array storage itself
    emitter.instruction("mov r10, QWORD PTR [rsp + 8]");                        // array base
    emitter.instruction("mov rax, QWORD PTR [r10 + rcx * 8]");                  // array[index] = one matched path string
    emitter.instruction("call __rt_heap_free");                                 // release that path string
    emitter.instruction("mov rcx, QWORD PTR [rsp + 16]");                       // reload the index (clobbered by the call)
    emitter.instruction("inc rcx");                                             // advance to the next slot
    emitter.instruction("mov QWORD PTR [rsp + 16], rcx");                       // save the updated index
    emitter.instruction("jmp .Lglobfree_loop");                                 // continue freeing matched path strings
    emitter.label(".Lglobfree_free_array");
    emitter.instruction("mov rax, QWORD PTR [rsp + 8]");                        // the gl_pathv array itself
    emitter.instruction("call __rt_heap_free");                                 // release the array storage
    emitter.label(".Lglobfree_done");
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // fnmatch: PHP/POSIX byte matcher. PathMatchSpecW is deliberately not used:
    // its shell wildcard grammar ignores FNM_PATHNAME/FNM_PERIOD/FNM_NOESCAPE
    // and is always case-insensitive, unlike php-src's Windows fnmatch port.
    emitter.label_global("fnmatch");
    emitter.instruction("sub rsp, 40");                                         // reserve flags, star-backtrack cursors, and original string
    emitter.instruction("mov QWORD PTR [rsp], rdx");                            // preserve FNM_* flags
    emitter.instruction("mov QWORD PTR [rsp + 8], 0");                          // no remembered pattern position after a star
    emitter.instruction("mov QWORD PTR [rsp + 16], 0");                         // no remembered candidate position for a star
    emitter.instruction("mov QWORD PTR [rsp + 24], rsi");                       // preserve candidate start for FNM_PERIOD
    emitter.label(".Lfnmatch_loop");
    emitter.instruction("movzx eax, BYTE PTR [rdi]");                           // load the next pattern byte
    emitter.instruction("test al, al");                                         // end of pattern?
    emitter.instruction("jz .Lfnmatch_pattern_end");                            // only a simultaneous candidate end matches
    emitter.instruction("cmp al, 42");                                          // '*' wildcard?
    emitter.instruction("je .Lfnmatch_star");                                   // remember a bounded backtracking point
    emitter.instruction("cmp al, 63");                                          // '?' wildcard?
    emitter.instruction("je .Lfnmatch_question");                               // match exactly one permitted byte
    emitter.instruction("cmp al, 91");                                          // '[' character class?
    emitter.instruction("je .Lfnmatch_class");                                  // parse and match the bracket expression
    emitter.instruction("cmp al, 92");                                          // backslash escape?
    emitter.instruction("jne .Lfnmatch_literal");                               // ordinary byte comparison
    emitter.instruction("test QWORD PTR [rsp], 2");                             // FNM_NOESCAPE leaves backslash literal
    emitter.instruction("jnz .Lfnmatch_literal");                               // do not consume the escaped byte
    emitter.instruction("cmp BYTE PTR [rdi + 1], 0");                           // dangling backslash?
    emitter.instruction("je .Lfnmatch_literal");                                // treat a terminal backslash literally
    emitter.instruction("inc rdi");                                             // skip the escape marker
    emitter.instruction("movzx eax, BYTE PTR [rdi]");                           // compare the escaped byte literally
    emitter.label(".Lfnmatch_literal");
    emitter.instruction("movzx ecx, BYTE PTR [rsi]");                           // load the candidate byte
    emitter.instruction("test cl, cl");                                         // candidate exhausted?
    emitter.instruction("jz .Lfnmatch_backtrack");                              // a literal cannot match the terminator
    emitter.instruction("mov r8d, eax");                                        // preserve the pattern byte for optional folding
    emitter.instruction("mov r9d, ecx");                                        // preserve the candidate byte for optional folding
    emitter.instruction("test QWORD PTR [rsp], 16");                            // FNM_CASEFOLD enabled?
    emitter.instruction("jz .Lfnmatch_literal_compare");                        // compare bytes exactly otherwise
    emitter.instruction("cmp r8b, 65");                                         // pattern byte below ASCII 'A'?
    emitter.instruction("jb .Lfnmatch_fold_candidate");                         // skip pattern folding
    emitter.instruction("cmp r8b, 90");                                         // pattern byte above ASCII 'Z'?
    emitter.instruction("ja .Lfnmatch_fold_candidate");                         // skip pattern folding
    emitter.instruction("or r8b, 32");                                          // fold pattern ASCII uppercase to lowercase
    emitter.label(".Lfnmatch_fold_candidate");
    emitter.instruction("cmp r9b, 65");                                         // candidate byte below ASCII 'A'?
    emitter.instruction("jb .Lfnmatch_literal_compare");                        // skip candidate folding
    emitter.instruction("cmp r9b, 90");                                         // candidate byte above ASCII 'Z'?
    emitter.instruction("ja .Lfnmatch_literal_compare");                        // skip candidate folding
    emitter.instruction("or r9b, 32");                                          // fold candidate ASCII uppercase to lowercase
    emitter.label(".Lfnmatch_literal_compare");
    emitter.instruction("cmp r8b, r9b");                                        // do the literal bytes match?
    emitter.instruction("jne .Lfnmatch_backtrack");                             // retry through the most recent star on mismatch
    emitter.instruction("inc rdi");                                             // consume one pattern byte
    emitter.instruction("inc rsi");                                             // consume one candidate byte
    emitter.instruction("jmp .Lfnmatch_loop");                                  // continue matching
    emitter.label(".Lfnmatch_question");
    emitter.instruction("movzx ecx, BYTE PTR [rsi]");                           // load the wildcard candidate byte
    emitter.instruction("test cl, cl");                                         // '?' cannot match the terminator
    emitter.instruction("jz .Lfnmatch_backtrack");                              // retry through a preceding star
    emitter.instruction("test QWORD PTR [rsp], 1");                             // FNM_PATHNAME enabled?
    emitter.instruction("jz .Lfnmatch_question_period");                        // slash is ordinary without the flag
    emitter.instruction("cmp cl, 47");                                          // candidate is '/'?
    emitter.instruction("je .Lfnmatch_backtrack");                              // wildcard cannot cross a path separator
    emitter.label(".Lfnmatch_question_period");
    emitter.instruction("cmp cl, 46");                                          // candidate is '.'?
    emitter.instruction("jne .Lfnmatch_question_consume");                      // no leading-period restriction
    emitter.instruction("call .Lfnmatch_is_leading_period");                    // rax = whether wildcard sees a leading period
    emitter.instruction("test eax, eax");                                       // is FNM_PERIOD blocking this wildcard?
    emitter.instruction("jnz .Lfnmatch_backtrack");                             // leading period must be matched literally
    emitter.label(".Lfnmatch_question_consume");
    emitter.instruction("inc rdi");                                             // consume '?'
    emitter.instruction("inc rsi");                                             // consume its candidate byte
    emitter.instruction("jmp .Lfnmatch_loop");                                  // continue matching
    emitter.label(".Lfnmatch_star");
    emitter.instruction("inc rdi");                                             // consume the first '*'
    emitter.label(".Lfnmatch_star_collapse");
    emitter.instruction("cmp BYTE PTR [rdi], 42");                              // adjacent '*' wildcard?
    emitter.instruction("jne .Lfnmatch_star_ready");                            // keep one canonical star
    emitter.instruction("inc rdi");                                             // collapse the redundant wildcard
    emitter.instruction("jmp .Lfnmatch_star_collapse");                         // continue collapsing
    emitter.label(".Lfnmatch_star_ready");
    emitter.instruction("mov QWORD PTR [rsp + 8], rdi");                        // retry pattern immediately after the star
    emitter.instruction("mov QWORD PTR [rsp + 16], rsi");                       // initially let '*' consume zero bytes
    emitter.instruction("jmp .Lfnmatch_loop");                                  // attempt the zero-length match first
    emitter.label(".Lfnmatch_class");
    emitter.instruction("movzx ecx, BYTE PTR [rsi]");                           // load the class candidate byte
    emitter.instruction("test cl, cl");                                         // a class cannot match the terminator
    emitter.instruction("jz .Lfnmatch_backtrack");                              // retry through a preceding star
    emitter.instruction("test QWORD PTR [rsp], 1");                             // FNM_PATHNAME enabled?
    emitter.instruction("jz .Lfnmatch_class_period");                           // slash is class-matchable without the flag
    emitter.instruction("cmp cl, 47");                                          // candidate is '/'?
    emitter.instruction("je .Lfnmatch_backtrack");                              // classes cannot cross a path separator
    emitter.label(".Lfnmatch_class_period");
    emitter.instruction("cmp cl, 46");                                          // candidate is '.'?
    emitter.instruction("jne .Lfnmatch_class_parse");                           // no leading-period restriction
    emitter.instruction("call .Lfnmatch_is_leading_period");                    // rax = whether wildcard sees a leading period
    emitter.instruction("test eax, eax");                                       // is FNM_PERIOD blocking this class?
    emitter.instruction("jnz .Lfnmatch_backtrack");                             // leading period must be literal
    emitter.label(".Lfnmatch_class_parse");
    emitter.instruction("lea r8, [rdi + 1]");                                   // class cursor after '['
    emitter.instruction("xor r9d, r9d");                                        // class is positive by default
    emitter.instruction("cmp BYTE PTR [r8], 33");                               // leading '!' negates the class
    emitter.instruction("je .Lfnmatch_class_negate");                           // record class negation
    emitter.instruction("cmp BYTE PTR [r8], 94");                               // accept '^' as the alternate negator
    emitter.instruction("jne .Lfnmatch_class_begin");                           // begin scanning members
    emitter.label(".Lfnmatch_class_negate");
    emitter.instruction("mov r9d, 1");                                          // remember negated class semantics
    emitter.instruction("inc r8");                                              // skip the negation marker
    emitter.label(".Lfnmatch_class_begin");
    emitter.instruction("xor r10d, r10d");                                      // no class member has matched yet
    emitter.label(".Lfnmatch_class_scan");
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load the next class byte
    emitter.instruction("test al, al");                                         // unterminated class?
    emitter.instruction("jz .Lfnmatch_class_invalid");                          // treat '[' as a literal
    emitter.instruction("cmp al, 93");                                          // closing ']'?
    emitter.instruction("je .Lfnmatch_class_done");                             // finish the class decision
    emitter.instruction("mov r11d, eax");                                       // lower endpoint defaults to this member
    emitter.instruction("cmp BYTE PTR [r8 + 1], 45");                           // range marker follows?
    emitter.instruction("jne .Lfnmatch_class_single");                          // compare one member
    emitter.instruction("cmp BYTE PTR [r8 + 2], 0");                            // missing range upper endpoint?
    emitter.instruction("je .Lfnmatch_class_single");                           // '-' is literal at the end
    emitter.instruction("cmp BYTE PTR [r8 + 2], 93");                           // '-' immediately before closing bracket?
    emitter.instruction("je .Lfnmatch_class_single");                           // keep '-' literal
    emitter.instruction("movzx eax, BYTE PTR [r8 + 2]");                        // load the range upper endpoint
    emitter.instruction("mov rdx, QWORD PTR [rsp]");                            // reload flags for the shared comparison
    emitter.instruction("jmp .Lfnmatch_class_compare_range");                   // test candidate within the range
    emitter.label(".Lfnmatch_class_single");
    emitter.instruction("mov eax, r11d");                                       // single member has equal lower/upper endpoints
    emitter.instruction("mov rdx, QWORD PTR [rsp]");                            // reload flags for the shared comparison
    emitter.label(".Lfnmatch_class_compare_range");
    emitter.instruction("movzx ecx, BYTE PTR [rsi]");                           // reload candidate byte
    emitter.instruction("test rdx, 16");                                        // FNM_CASEFOLD enabled?
    emitter.instruction("jz .Lfnmatch_class_compare");                          // retain exact byte order otherwise
    emitter.instruction("cmp r11b, 65");                                        // lower endpoint below ASCII uppercase?
    emitter.instruction("jb .Lfnmatch_class_fold_upper");                       // skip lower folding
    emitter.instruction("cmp r11b, 90");                                        // lower endpoint above ASCII uppercase?
    emitter.instruction("ja .Lfnmatch_class_fold_upper");                       // skip lower folding
    emitter.instruction("or r11b, 32");                                         // fold lower endpoint
    emitter.label(".Lfnmatch_class_fold_upper");
    emitter.instruction("cmp al, 65");                                          // upper endpoint below ASCII uppercase?
    emitter.instruction("jb .Lfnmatch_class_fold_candidate");                   // skip upper folding
    emitter.instruction("cmp al, 90");                                          // upper endpoint above ASCII uppercase?
    emitter.instruction("ja .Lfnmatch_class_fold_candidate");                   // skip upper folding
    emitter.instruction("or al, 32");                                           // fold upper endpoint
    emitter.label(".Lfnmatch_class_fold_candidate");
    emitter.instruction("cmp cl, 65");                                          // candidate below ASCII uppercase?
    emitter.instruction("jb .Lfnmatch_class_compare");                          // skip candidate folding
    emitter.instruction("cmp cl, 90");                                          // candidate above ASCII uppercase?
    emitter.instruction("ja .Lfnmatch_class_compare");                          // skip candidate folding
    emitter.instruction("or cl, 32");                                           // fold candidate
    emitter.label(".Lfnmatch_class_compare");
    emitter.instruction("cmp cl, r11b");                                        // candidate below lower endpoint?
    emitter.instruction("jb .Lfnmatch_class_advance");                          // this member does not match
    emitter.instruction("cmp cl, al");                                          // candidate above upper endpoint?
    emitter.instruction("ja .Lfnmatch_class_advance");                          // this member does not match
    emitter.instruction("mov r10d, 1");                                         // remember that one member matched
    emitter.label(".Lfnmatch_class_advance");
    emitter.instruction("cmp BYTE PTR [r8 + 1], 45");                           // was this syntactically a range?
    emitter.instruction("jne .Lfnmatch_class_advance_one");                     // consume one member
    emitter.instruction("cmp BYTE PTR [r8 + 2], 0");                            // malformed range endpoint?
    emitter.instruction("je .Lfnmatch_class_advance_one");                      // consume one member
    emitter.instruction("cmp BYTE PTR [r8 + 2], 93");                           // '-' was literal before closing bracket?
    emitter.instruction("je .Lfnmatch_class_advance_one");                      // consume one member
    emitter.instruction("add r8, 3");                                           // consume the complete range
    emitter.instruction("jmp .Lfnmatch_class_scan");                            // scan remaining members
    emitter.label(".Lfnmatch_class_advance_one");
    emitter.instruction("inc r8");                                              // consume one class member
    emitter.instruction("jmp .Lfnmatch_class_scan");                            // scan remaining members
    emitter.label(".Lfnmatch_class_done");
    emitter.instruction("cmp r10d, r9d");                                       // positive wants matched=1; negated wants matched=0
    emitter.instruction("je .Lfnmatch_backtrack");                              // class decision rejected the candidate
    emitter.instruction("lea rdi, [r8 + 1]");                                   // consume the whole bracket expression
    emitter.instruction("inc rsi");                                             // consume its candidate byte
    emitter.instruction("mov rdx, QWORD PTR [rsp]");                            // restore flags after class scratch use
    emitter.instruction("jmp .Lfnmatch_loop");                                  // continue matching
    emitter.label(".Lfnmatch_class_invalid");
    emitter.instruction("mov rdx, QWORD PTR [rsp]");                            // restore flags after class scratch use
    emitter.instruction("mov eax, 91");                                         // malformed '[' is matched literally
    emitter.instruction("jmp .Lfnmatch_literal");                               // use ordinary literal comparison
    emitter.label(".Lfnmatch_pattern_end");
    emitter.instruction("cmp BYTE PTR [rsi], 0");                               // candidate also exhausted?
    emitter.instruction("jne .Lfnmatch_backtrack");                             // let a preceding star absorb more bytes
    emitter.instruction("xor eax, eax");                                        // libc fnmatch success is zero
    emitter.instruction("add rsp, 40");                                         // release matcher locals
    emitter.instruction("ret");                                                 // return match
    emitter.label(".Lfnmatch_backtrack");
    emitter.instruction("mov rdi, QWORD PTR [rsp + 8]");                        // reload the pattern position after the latest star
    emitter.instruction("test rdi, rdi");                                       // was any star seen?
    emitter.instruction("jz .Lfnmatch_nomatch");                                // no remaining matching alternative
    emitter.instruction("mov rsi, QWORD PTR [rsp + 16]");                       // reload how far that star currently consumes
    emitter.instruction("movzx ecx, BYTE PTR [rsi]");                           // inspect the next byte the star might absorb
    emitter.instruction("test cl, cl");                                         // candidate exhausted?
    emitter.instruction("jz .Lfnmatch_nomatch");                                // star has no larger alternative
    emitter.instruction("test QWORD PTR [rsp], 1");                             // FNM_PATHNAME enabled?
    emitter.instruction("jz .Lfnmatch_backtrack_period");                       // slash may be absorbed otherwise
    emitter.instruction("cmp cl, 47");                                          // next byte is '/'?
    emitter.instruction("je .Lfnmatch_nomatch");                                // star cannot cross a separator
    emitter.label(".Lfnmatch_backtrack_period");
    emitter.instruction("cmp cl, 46");                                          // next byte is '.'?
    emitter.instruction("jne .Lfnmatch_backtrack_consume");                     // no leading-period restriction
    emitter.instruction("call .Lfnmatch_is_leading_period");                    // rax = whether wildcard sees a leading period
    emitter.instruction("test eax, eax");                                       // is FNM_PERIOD blocking the star?
    emitter.instruction("jnz .Lfnmatch_nomatch");                               // star cannot consume that leading period
    emitter.label(".Lfnmatch_backtrack_consume");
    emitter.instruction("inc rsi");                                             // let the star consume one additional byte
    emitter.instruction("mov QWORD PTR [rsp + 16], rsi");                       // remember the expanded candidate position
    emitter.instruction("jmp .Lfnmatch_loop");                                  // retry the suffix
    emitter.label(".Lfnmatch_is_leading_period");
    emitter.instruction("xor eax, eax");                                        // default: wildcard may consume this period
    emitter.instruction("test QWORD PTR [rsp + 8], 4");                         // FNM_PERIOD enabled? (return address precedes matcher locals)
    emitter.instruction("jz .Lfnmatch_period_return");                          // no restriction without the flag
    emitter.instruction("cmp rsi, QWORD PTR [rsp + 32]");                       // beginning of the whole candidate? (account for return address)
    emitter.instruction("je .Lfnmatch_period_yes");                             // leading period is protected
    emitter.instruction("test QWORD PTR [rsp + 8], 1");                         // only pathname mode starts segments after '/' (account for return address)
    emitter.instruction("jz .Lfnmatch_period_return");                          // interior periods are ordinary
    emitter.instruction("cmp BYTE PTR [rsi - 1], 47");                          // immediately follows a separator?
    emitter.instruction("jne .Lfnmatch_period_return");                         // not a segment-leading period
    emitter.label(".Lfnmatch_period_yes");
    emitter.instruction("mov eax, 1");                                          // report a protected leading period
    emitter.label(".Lfnmatch_period_return");
    emitter.instruction("ret");                                                 // return to wildcard decision
    emitter.label(".Lfnmatch_nomatch");
    emitter.instruction("mov rax, 1");                                          // return FNM_NOMATCH (1)
    emitter.instruction("add rsp, 40");                                         // release matcher locals
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // realpath: resolve the opened object through GetFinalPathNameByHandleW and return strict UTF-8.
    emitter.label_global("realpath");
    emitter.instruction("sub rsp, 104");                                        // shadow space, outgoing Win32 arguments, and owned input/output paths
    emitter.instruction("mov QWORD PTR [rsp + 96], rsi");                       // preserve caller UTF-8 destination outside outgoing argument slots
    emitter.instruction("call __rt_win_utf8_to_utf16");                         // convert input path strictly
    emitter.instruction("test rax, rax");                                       // conversion succeeded?
    emitter.instruction("jz .Lrealpath_conversion_fail");                       // invalid UTF-8
    emitter.instruction("mov QWORD PTR [rsp + 88], rax");                       // retain owned wide input outside outgoing argument slots
    emitter.instruction("mov rax, 65536");                                      // maximum extended wide path buffer
    emitter.instruction("call __rt_heap_alloc");                                // allocate UTF-16 resolved path
    emitter.instruction("test rax, rax");                                       // allocation succeeded?
    emitter.instruction("jz .Lrealpath_alloc_fail");                            // cleanup input and return ENOMEM
    emitter.instruction("mov QWORD PTR [rsp + 80], rax");                       // retain owned wide result outside Win32 outgoing argument slots
    emitter.instruction("mov rcx, QWORD PTR [rsp + 88]");                       // lpFileName = wide input
    emitter.instruction("xor edx, edx");                                        // dwDesiredAccess = metadata-only
    emitter.instruction("mov r8d, 7");                                          // share read/write/delete so the current directory and live files resolve
    emitter.instruction("xor r9, r9");                                          // lpSecurityAttributes = NULL
    emitter.instruction("mov QWORD PTR [rsp + 32], 3");                         // dwCreationDisposition = OPEN_EXISTING
    emitter.instruction("mov QWORD PTR [rsp + 40], 0x2000080");                 // dwFlagsAndAttributes = FILE_ATTRIBUTE_NORMAL | FILE_FLAG_BACKUP_SEMANTICS
    emitter.instruction("mov QWORD PTR [rsp + 48], 0");                         // hTemplateFile = NULL
    emitter.instruction("call CreateFileW");                                    // open the object so final-path resolution follows symlinks and junctions
    emitter.instruction("cmp rax, -1");                                         // INVALID_HANDLE_VALUE?
    emitter.instruction("je .Lrealpath_open_fail");                             // native open failure maps to PHP false
    emitter.instruction("mov QWORD PTR [rsp + 64], rax");                       // preserve handle until final path retrieval completes
    emitter.instruction("mov rcx, rax");                                        // hFile = opened target object
    emitter.instruction("mov rdx, QWORD PTR [rsp + 80]");                       // lpszFilePath = wide output buffer
    emitter.instruction("mov r8d, 32768");                                      // cchFilePath = WCHAR output capacity
    emitter.instruction("xor r9d, r9d");                                        // FILE_NAME_NORMALIZED | VOLUME_NAME_DOS
    emitter.instruction("call GetFinalPathNameByHandleW");                      // resolve the physical final path through the opened handle
    emitter.instruction("mov DWORD PTR [rsp + 72], eax");                       // preserve WCHAR result length across CloseHandle
    emitter.instruction("test eax, eax");                                       // final-path query succeeded?
    emitter.instruction("jnz .Lrealpath_query_ok");                             // close then process the result
    emitter.instruction("call GetLastError");                                   // capture query failure before closing the handle
    emitter.instruction("mov DWORD PTR [rsp + 56], eax");                       // retain native error across CloseHandle
    emitter.instruction("mov rcx, QWORD PTR [rsp + 64]");                       // opened target handle
    emitter.instruction("call CloseHandle");                                    // close on query failure
    emitter.instruction("jmp .Lrealpath_native_fail_saved");                    // release owned path buffers with the saved error
    emitter.label(".Lrealpath_query_ok");
    emitter.instruction("mov rcx, QWORD PTR [rsp + 64]");                       // opened target handle
    emitter.instruction("call CloseHandle");                                    // close after successful final-path lookup
    emitter.instruction("mov eax, DWORD PTR [rsp + 72]");                       // restore WCHAR result length
    emitter.instruction("cmp rax, 32768");                                      // output exceeded capacity?
    emitter.instruction("jae .Lrealpath_range_fail");                           // reject truncated result
    emitter.instruction("mov rdi, QWORD PTR [rsp + 80]");                       // wide resolved path
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // first four UTF-16 code units
    emitter.instruction("mov r10, 0x005c003f005c005c");                         // UTF-16 `\\?\`
    emitter.instruction("cmp rax, r10");                                        // extended prefix present?
    emitter.instruction("jne .Lrealpath_prefix_done");                          // ordinary path
    emitter.instruction("mov rax, QWORD PTR [rdi + 8]");                        // possible `UNC\`
    emitter.instruction("mov r10, 0x005c0043004e0055");                         // UTF-16 UNC marker
    emitter.instruction("cmp rax, r10");                                        // extended UNC path?
    emitter.instruction("jne .Lrealpath_drive_prefix");                         // extended drive path
    emitter.instruction("mov WORD PTR [rdi + 12], 92");                         // first UNC slash
    emitter.instruction("mov WORD PTR [rdi + 14], 92");                         // second UNC slash
    emitter.instruction("add rdi, 12");                                         // source now starts with `\\server`
    emitter.instruction("jmp .Lrealpath_prefix_done");                          // prefix normalized
    emitter.label(".Lrealpath_drive_prefix");
    emitter.instruction("add rdi, 8");                                          // strip `\\?\`
    emitter.label(".Lrealpath_prefix_done");
    emitter.instruction("mov rsi, QWORD PTR [rsp + 96]");                       // caller UTF-8 destination
    emitter.instruction("mov rdx, 4096");                                       // caller allocation contract
    emitter.instruction("call __rt_win_utf16_to_utf8");                         // strict UTF-8 result conversion
    emitter.instruction("test eax, eax");                                       // conversion succeeded and fit?
    emitter.instruction("jz .Lrealpath_native_fail");                           // capture conversion failure
    emitter.instruction("mov rax, QWORD PTR [rsp + 80]");                       // owned wide result
    emitter.instruction("call __rt_heap_free");                                 // release result conversion
    emitter.instruction("mov rax, QWORD PTR [rsp + 88]");                       // owned wide input
    emitter.instruction("call __rt_heap_free");                                 // release input conversion
    emitter.instruction("mov rax, QWORD PTR [rsp + 96]");                       // return resolved UTF-8 pointer
    emitter.instruction("add rsp, 104");                                        // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lrealpath_open_fail");
    emitter.instruction("call GetLastError");                                   // capture the open failure before releasing owned buffers
    emitter.instruction("mov DWORD PTR [rsp + 56], eax");                       // preserve native error across cleanup
    emitter.instruction("jmp .Lrealpath_native_fail_saved");                    // release both path buffers and publish errno
    emitter.label(".Lrealpath_native_fail");
    emitter.instruction("call GetLastError");                                   // capture native/conversion failure
    emitter.instruction("mov DWORD PTR [rsp + 56], eax");                       // preserve native error
    emitter.label(".Lrealpath_native_fail_saved");
    emitter.instruction("mov rax, QWORD PTR [rsp + 80]");                       // owned wide result
    emitter.instruction("call __rt_heap_free");                                 // release result allocation
    emitter.instruction("mov rax, QWORD PTR [rsp + 88]");                       // owned wide input
    emitter.instruction("call __rt_heap_free");                                 // release input conversion
    emitter.instruction("mov eax, DWORD PTR [rsp + 56]");                       // restore native error
    emitter.instruction("mov DWORD PTR [rip + __rt_win32_last_error], eax");    // retain native state
    emitter.instruction("call __rt_win32_errno_from_code");                     // translate errno
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], eax");               // publish errno
    emitter.instruction("xor rax, rax");                                        // return NULL on failure
    emitter.instruction("add rsp, 104");                                        // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lrealpath_range_fail");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 36");                // ENAMETOOLONG
    emitter.instruction("jmp .Lrealpath_cleanup_fail");                         // release both allocations
    emitter.label(".Lrealpath_alloc_fail");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 12");                // ENOMEM
    emitter.instruction("mov rax, QWORD PTR [rsp + 88]");                       // owned wide input
    emitter.instruction("call __rt_heap_free");                                 // release input conversion
    emitter.instruction("xor eax, eax");                                        // return NULL
    emitter.instruction("add rsp, 104");                                        // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lrealpath_cleanup_fail");
    emitter.instruction("mov rax, QWORD PTR [rsp + 80]");                       // owned wide result
    emitter.instruction("call __rt_heap_free");                                 // release result allocation
    emitter.instruction("mov rax, QWORD PTR [rsp + 88]");                       // owned wide input
    emitter.instruction("call __rt_heap_free");                                 // release input conversion
    emitter.instruction("xor eax, eax");                                        // return NULL
    emitter.instruction("add rsp, 104");                                        // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lrealpath_conversion_fail");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 84");                // EILSEQ
    emitter.instruction("xor eax, eax");                                        // return NULL
    emitter.instruction("add rsp, 104");                                        // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // chmod: delegate to SetFileAttributesW
    emitter.label_global("chmod");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_chmod");                                 // call SetFileAttributesW shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // unlink: delegate to DeleteFileW
    emitter.label_global("unlink");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_unlink");                                // call DeleteFileW shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // access: delegate to __rt_sys_access (GetFileAttributesW)
    emitter.label_global("access");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_access");                                // call GetFileAttributesW shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // ftruncate: delegate to __rt_sys_ftruncate (SetFilePointerEx + SetEndOfFile)
    emitter.label_global("ftruncate");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_ftruncate");                             // call SetFilePointerEx+SetEndOfFile shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // umask: php-src keeps process-local mask state on Windows even though the
    // filesystem can only reflect its write-bit subset through READONLY.
    emitter.label_global("umask");
    emitter.instruction("mov eax, DWORD PTR [rip + __rt_win_umask]");           // return the previous process-local mask
    emitter.instruction("mov DWORD PTR [rip + __rt_win_umask], edi");           // publish the caller's new mask
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // sleep: convert SysV `sleep(unsigned seconds)` to Win32 `Sleep(DWORD ms)`.
    // libc `sleep` returns 0 when not interrupted by a signal; Win32 `Sleep` has no
    // early-wakeup contract here, so we always return 0.
    emitter.label_global("sleep");
    // -- frame: shadow(32) + pad(8), 40 ≡ 8 mod 16 keeps rsp ≡ 0 at the call --
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8) for Sleep call
    emitter.instruction("imul rcx, rdi, 1000");                                 // seconds (SysV arg1, rdi) → milliseconds for Win32 Sleep
    emitter.instruction("call Sleep");                                          // Sleep(ms) — blocks the current thread, no return value used
    emitter.instruction("xor eax, eax");                                        // libc sleep returns 0 (no signal interruption on Windows)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return 0
    emitter.blank();

    // usleep: convert SysV `usleep(useconds_t usec)` to Win32 `Sleep(DWORD ms)`.
    // libc `usleep` returns 0 on success; Win32 `Sleep` has no early-wakeup contract
    // here, so we always return 0. `usleep(0)` → `Sleep(0)` yields the timeslice,
    // matching POSIX semantics.
    emitter.label_global("usleep");
    // -- frame: shadow(32) + pad(8), 40 ≡ 8 mod 16 keeps rsp ≡ 0 at the call --
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8) for Sleep call
    emitter.instruction("mov rax, rdi");                                        // microseconds (SysV arg1, rdi) → rax for division
    emitter.instruction("xor rdx, rdx");                                        // clear high half of dividend before unsigned div
    emitter.instruction("mov ecx, 1000");                                       // divisor: 1000 (usec → ms)
    emitter.instruction("div rcx");                                             // rax = usec / 1000 = milliseconds, rdx = remainder
    emitter.instruction("mov rcx, rax");                                        // milliseconds for Win32 Sleep
    emitter.instruction("call Sleep");                                          // Sleep(ms) — blocks the current thread, no return value used
    emitter.instruction("xor eax, eax");                                        // libc usleep returns 0 on success
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return 0
    emitter.blank();

    // popen: convert SysV `popen(command, mode)` to msvcrt `_popen`.
    // SysV: rdi=command, rsi=mode → MSx64: rcx=command, rdx=mode. Returns FILE* in rax.
    emitter.label_global("popen");
    // -- popen: SysV→MSx64 for msvcrt _popen --
    emitter.instruction("mov rcx, rdi");                                        // command (SysV arg1) → MSx64 arg1
    emitter.instruction("mov rdx, rsi");                                        // mode (SysV arg2) → MSx64 arg2
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8) for _popen call
    emitter.instruction("call _popen");                                         // _popen(command, mode) → FILE* in rax
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return FILE*
    emitter.blank();

    // pclose: convert SysV `pclose(FILE*)` to msvcrt `_pclose`.
    // SysV: rdi=stream → MSx64: rcx=stream. Returns int in eax.
    emitter.label_global("pclose");
    // -- pclose: SysV→MSx64 for msvcrt _pclose --
    emitter.instruction("mov rcx, rdi");                                        // stream (SysV arg1) → MSx64 arg1
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8) for _pclose call
    emitter.instruction("call _pclose");                                        // _pclose(stream) → int in eax
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return int
    emitter.blank();

    // fileno: convert SysV `fileno(FILE*)` to msvcrt `_fileno`.
    // SysV: rdi=stream → MSx64: rcx=stream. Returns int in eax.
    emitter.label_global("fileno");
    // -- fileno: SysV→MSx64 for msvcrt _fileno --
    emitter.instruction("mov rcx, rdi");                                        // stream (SysV arg1) → MSx64 arg1
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8) for _fileno call
    emitter.instruction("call _fileno");                                        // _fileno(stream) → int in eax
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return int
    emitter.blank();

    // fgetc: convert SysV `fgetc(FILE*)` to msvcrt `fgetc`.
    // SysV: rdi=stream → MSx64: rcx=stream. Returns int in eax (char or EOF).
    emitter.label_global("__rt_sys_fgetc");
    // -- fgetc: SysV→MSx64 for msvcrt fgetc --
    emitter.instruction("mov rcx, rdi");                                        // stream (SysV arg1) → MSx64 arg1
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8) for fgetc call
    emitter.instruction("call QWORD PTR [rip + __imp_fgetc]");                  // imported msvcrt fgetc(stream) → int in eax
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return int
    emitter.blank();

    // system: convert SysV `system(command)` to msvcrt `system`.
    // SysV: rdi=command → MSx64: rcx=command. Returns int in eax.
    emitter.label_global("__rt_sys_system");
    // -- system: SysV→MSx64 for msvcrt system --
    emitter.instruction("mov rcx, rdi");                                        // command (SysV arg1) → MSx64 arg1
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8) for system call
    emitter.instruction("call QWORD PTR [rip + __imp_system]");                 // imported msvcrt system(command) → int in eax
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return int
    emitter.blank();

    // mkdir: delegate to CreateDirectoryW
    emitter.label_global("mkdir");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_mkdir");                                 // call CreateDirectoryW shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // rmdir: delegate to RemoveDirectoryW
    emitter.label_global("rmdir");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_rmdir");                                 // call RemoveDirectoryW shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // rename: delegate to __rt_sys_rename (MoveFileExW, returns POSIX status)
    emitter.label_global("rename");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_rename");                                // call MoveFileExW shim (POSIX status)
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // getcwd: delegate to dynamic GetCurrentDirectoryW + strict UTF-8 conversion
    emitter.label_global("getcwd");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_getcwd");                                // call Unicode getcwd shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // chdir: delegate to SetCurrentDirectoryW
    emitter.label_global("chdir");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_chdir");                                 // call SetCurrentDirectoryW shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // stat: delegate to msvcrt stat
    emitter.label_global("stat");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_stat");                                  // call msvcrt stat
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // dirfd: Windows has no kernel concept of "the fd behind this DIR*" (the
    // DIR* minted by `__rt_sys_opendir` is a heap pointer, far too large to
    // index the 256-slot `_dir_handles` table `opendir.rs` keys by fd). Mint
    // a small, distinct table index the same way `opendir_glob.rs` already
    // does for `_glob_handles`: `dup(2)` msvcrt's CRT-level fd for stderr via
    // `__rt_sys_dup` (backed by `_dup`, which hands out small increasing CRT
    // fds independent of any Win32 HANDLE value). The DIR* itself is ignored
    // here — `opendir.rs` stores it in `_dir_handles[fd]` right after this
    // call returns, so the fd only needs to be a fresh, in-range table key.
    // hstrerror: return NULL — use WSAGetLastError instead
    // h_errno: return 0 — Windows uses WSAGetLastError
    emitter.label_global("dirfd");
    emitter.instruction("sub rsp, 8");                                          // align to 16 bytes before calling __rt_sys_dup
    emitter.instruction("mov rdi, 2");                                          // dup stderr's CRT fd — mints a fresh small table-index fd
    emitter.instruction("call __rt_sys_dup");                                   // rax = new small fd (or -1 on exhaustion, matching libc dirfd's rare failure mode)
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return the minted fd
    emitter.blank();
    emitter.label_global("hstrerror");
    emitter.instruction("xor rax, rax");                                        // return NULL
    emitter.instruction("ret");                                                 // return
    emitter.blank();
    emitter.label_global("h_errno");
    emitter.instruction("xor rax, rax");                                        // return 0
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // timegm: delegate to msvcrt _mkgmtime
    emitter.label_global("timegm");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rdi");                                        // struct tm pointer
    emitter.instruction("call _mkgmtime");                                      // convert UTC tm to time_t
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return time_t in rax
    emitter.blank();

    // __errno static variable — see __rt_win32_errno_from_code below for who
    // writes it and the docblock on __errno_location above for who reads it.
    emitter.raw(".data");
    emitter.raw("__rt_errno:");
    emitter.raw("    .zero 8");
    emitter.raw("__rt_win_umask:");
    emitter.raw("    .zero 4");
    emitter.raw("__rt_win32_last_error:");
    emitter.raw("    .zero 4");
    emitter.raw("__rt_wsa_last_error:");
    emitter.raw("    .zero 4");
    emitter.raw(".text");
    emitter.blank();

    emit_win32_errno_translate(emitter);
}

/// Translates a Win32 `GetLastError`/`WSAGetLastError` code into the POSIX
/// errno value the runtime's read/recv consumers understand. Windows unifies
/// the two: `WSAGetLastError` is a thin wrapper over the same per-thread
/// `GetLastError` value, so one table serves both the ReadFile (kernel32)
/// caller (`shims_fs.rs`'s `__rt_sys_read`) and the Winsock recv callers
/// (`shims_net.rs`'s `__rt_sys_recvfrom`/`__rt_sys_recvmsg`).
///
/// Contract: EAX in = Win32/WSA error code, EAX out = POSIX errno. Leaf
/// helper (issues no further calls); only EAX/ECX are live across it.
///
/// Critical — must map to EAGAIN(11), the `fgets`/`fread` would-block check
/// (`io/fgets.rs`, `io/fread.rs`, `cmp r10d, 11`): `WSAEWOULDBLOCK`(10035),
/// `ERROR_NO_DATA`(232), `ERROR_IO_PENDING`(997).
///
/// Common: `ERROR_FILE_NOT_FOUND`(2)/`ERROR_PATH_NOT_FOUND`(3) -> ENOENT(2);
/// `ERROR_ACCESS_DENIED`(5) -> EACCES(13); `ERROR_INVALID_HANDLE`(6) ->
/// EBADF(9); `ERROR_BROKEN_PIPE`(109) -> EPIPE(32); `ERROR_HANDLE_EOF`(38)
/// -> 0 (EOF is not an error); `WSAEINTR`(10004) -> EINTR(4);
/// `ERROR_INSUFFICIENT_BUFFER`(122) -> ERANGE(34);
/// `ERROR_NO_UNICODE_TRANSLATION`(1113) -> EILSEQ(84);
/// `WSAECONNRESET`(10054) -> ECONNRESET(104); `WSAETIMEDOUT`(10060) ->
/// ETIMEDOUT(110). Any other code -> EIO(5).
/// Emits exact Win32 `readlink` lowering through `FSCTL_GET_REPARSE_POINT`.
fn emit_shim_readlink_reparse(emitter: &mut Emitter) {
    emitter.label_global("readlink");
    emitter.instruction("sub rsp, 152");                                        // DeviceIoControl stack args and owned conversion locals
    emitter.instruction("mov QWORD PTR [rsp + 88], rsi");                       // caller byte destination
    emitter.instruction("mov QWORD PTR [rsp + 96], rdx");                       // caller byte capacity
    emitter.instruction("call __rt_win_utf8_to_utf16");                         // convert reparse-point path strictly
    emitter.instruction("test rax, rax");                                       // conversion succeeded?
    emitter.instruction("jz .Lreadlink_exact_conversion_fail");                 // invalid UTF-8
    emitter.instruction("mov QWORD PTR [rsp + 64], rax");                       // owned wide input path
    emitter.instruction("mov rax, 16384");                                      // MAXIMUM_REPARSE_DATA_BUFFER_SIZE
    emitter.instruction("call __rt_heap_alloc");                                // allocate REPARSE_DATA_BUFFER
    emitter.instruction("test rax, rax");                                       // allocation succeeded?
    emitter.instruction("jz .Lreadlink_exact_alloc_reparse_fail");              // cleanup input
    emitter.instruction("mov QWORD PTR [rsp + 72], rax");                       // owned reparse buffer
    emitter.instruction("mov rcx, QWORD PTR [rsp + 64]");                       // wide link path
    emitter.instruction("xor edx, edx");                                        // metadata-only access
    emitter.instruction("mov r8, 7");                                           // share read/write/delete
    emitter.instruction("xor r9, r9");                                          // security attributes = NULL
    emitter.instruction("mov QWORD PTR [rsp + 32], 3");                         // OPEN_EXISTING
    emitter.instruction("mov QWORD PTR [rsp + 40], 0x2200000");                 // BACKUP_SEMANTICS | OPEN_REPARSE_POINT
    emitter.instruction("mov QWORD PTR [rsp + 48], 0");                         // template = NULL
    emitter.instruction("call CreateFileW");                                    // open reparse point without following it
    emitter.instruction("cmp rax, -1");                                         // INVALID_HANDLE_VALUE?
    emitter.instruction("je .Lreadlink_exact_native_fail");                     // capture open failure
    emitter.instruction("mov QWORD PTR [rsp + 80], rax");                       // retain handle
    emitter.instruction("mov rcx, rax");                                        // DeviceIoControl handle
    emitter.instruction("mov edx, 0x900A8");                                    // FSCTL_GET_REPARSE_POINT
    emitter.instruction("xor r8, r8");                                          // no input buffer
    emitter.instruction("xor r9d, r9d");                                        // input size = 0
    emitter.instruction("mov rax, QWORD PTR [rsp + 72]");                       // reparse output buffer
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // lpOutBuffer (arg5)
    emitter.instruction("mov QWORD PTR [rsp + 40], 16384");                     // nOutBufferSize (arg6)
    emitter.instruction("lea rax, [rsp + 104]");                                // bytes-returned slot
    emitter.instruction("mov QWORD PTR [rsp + 48], rax");                       // lpBytesReturned (arg7)
    emitter.instruction("mov QWORD PTR [rsp + 56], 0");                         // lpOverlapped = NULL (arg8)
    emitter.instruction("call DeviceIoControl");                                // fetch REPARSE_DATA_BUFFER
    emitter.instruction("test eax, eax");                                       // ioctl succeeded?
    emitter.instruction("jnz .Lreadlink_exact_ioctl_ok");                       // parse returned buffer
    emitter.instruction("call GetLastError");                                   // capture ioctl failure before close
    emitter.instruction("mov DWORD PTR [rsp + 136], eax");                      // preserve native error
    emitter.instruction("mov rcx, QWORD PTR [rsp + 80]");                       // opened handle
    emitter.instruction("call CloseHandle");                                    // close failed ioctl handle
    emitter.instruction("jmp .Lreadlink_exact_native_fail_saved");              // cleanup with saved error
    emitter.label(".Lreadlink_exact_ioctl_ok");
    emitter.instruction("mov rcx, QWORD PTR [rsp + 80]");                       // opened handle
    emitter.instruction("call CloseHandle");                                    // close after successful ioctl
    emitter.instruction("cmp DWORD PTR [rsp + 104], 8");                        // fixed reparse header present?
    emitter.instruction("jb .Lreadlink_exact_invalid_reparse");                 // truncated header
    emitter.instruction("mov r10, QWORD PTR [rsp + 72]");                       // REPARSE_DATA_BUFFER base
    emitter.instruction("mov eax, DWORD PTR [r10]");                            // ReparseTag
    emitter.instruction("cmp eax, 0xA000000C");                                 // IO_REPARSE_TAG_SYMLINK
    emitter.instruction("je .Lreadlink_exact_symlink");                         // symlink layout has PathBuffer at +20
    emitter.instruction("cmp eax, 0xA0000003");                                 // IO_REPARSE_TAG_MOUNT_POINT
    emitter.instruction("je .Lreadlink_exact_mount");                           // junction layout has PathBuffer at +16
    emitter.instruction("jmp .Lreadlink_exact_invalid_reparse");                // unsupported reparse tag
    emitter.label(".Lreadlink_exact_symlink");
    emitter.instruction("cmp DWORD PTR [rsp + 104], 20");                       // symbolic-link fixed fields present?
    emitter.instruction("jb .Lreadlink_exact_invalid_reparse");                 // truncated symbolic-link record
    emitter.instruction("lea r11, [r10 + 20]");                                 // symbolic-link PathBuffer
    emitter.instruction("jmp .Lreadlink_exact_select_name");                    // shared name selection
    emitter.label(".Lreadlink_exact_mount");
    emitter.instruction("cmp DWORD PTR [rsp + 104], 16");                       // mount-point fixed fields present?
    emitter.instruction("jb .Lreadlink_exact_invalid_reparse");                 // truncated mount-point record
    emitter.instruction("lea r11, [r10 + 16]");                                 // junction PathBuffer
    emitter.label(".Lreadlink_exact_select_name");
    emitter.instruction("movzx eax, WORD PTR [r10 + 14]");                      // PrintNameLength in bytes
    emitter.instruction("movzx ecx, WORD PTR [r10 + 12]");                      // PrintNameOffset in bytes
    emitter.instruction("test eax, eax");                                       // printable name available?
    emitter.instruction("jnz .Lreadlink_exact_name_selected");                  // prefer print name
    emitter.instruction("movzx eax, WORD PTR [r10 + 10]");                      // SubstituteNameLength in bytes
    emitter.instruction("movzx ecx, WORD PTR [r10 + 8]");                       // SubstituteNameOffset in bytes
    emitter.label(".Lreadlink_exact_name_selected");
    emitter.instruction("test eax, eax");                                       // selected name non-empty?
    emitter.instruction("jz .Lreadlink_exact_invalid_reparse");                 // malformed reparse record
    emitter.instruction("test eax, 1");                                         // UTF-16 byte length must be even
    emitter.instruction("jnz .Lreadlink_exact_invalid_reparse");                // reject half code unit
    emitter.instruction("test ecx, 1");                                         // UTF-16 byte offset must be even
    emitter.instruction("jnz .Lreadlink_exact_invalid_reparse");                // reject misaligned slice
    emitter.instruction("lea rdx, [r11 + rcx]");                                // selected slice start
    emitter.instruction("lea r8, [rdx + rax]");                                 // selected slice end
    emitter.instruction("mov r9d, DWORD PTR [rsp + 104]");                      // bytes returned by DeviceIoControl
    emitter.instruction("lea r9, [r10 + r9]");                                  // end of initialized reparse bytes
    emitter.instruction("cmp r8, r9");                                          // selected name lies inside returned data?
    emitter.instruction("ja .Lreadlink_exact_invalid_reparse");                 // reject out-of-bounds offset/length
    emitter.instruction("lea r9, [r10 + 16382]");                               // last safe location for a WCHAR terminator
    emitter.instruction("cmp r8, r9");                                          // terminator fits allocation?
    emitter.instruction("ja .Lreadlink_exact_invalid_reparse");                 // reject allocation overflow
    emitter.instruction("lea rdi, [r11 + rcx]");                                // selected UTF-16 slice
    emitter.instruction("mov WORD PTR [rdi + rax], 0");                         // terminate selected byte-length slice
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // inspect first four UTF-16 code units
    emitter.instruction("mov r10, 0x005c003f005c005c");                         // `\\?\` prefix
    emitter.instruction("cmp rax, r10");                                        // Win32 extended prefix?
    emitter.instruction("je .Lreadlink_exact_extended_prefix");                 // normalize extended form
    emitter.instruction("mov r10, 0x005c003f003f005c");                         // NT `\??\` prefix
    emitter.instruction("cmp rax, r10");                                        // NT substitute prefix?
    emitter.instruction("jne .Lreadlink_exact_prefix_done");                    // relative/ordinary print name
    emitter.label(".Lreadlink_exact_extended_prefix");
    emitter.instruction("mov rax, QWORD PTR [rdi + 8]");                        // possible `UNC\` marker
    emitter.instruction("mov r10, 0x005c0043004e0055");                         // UTF-16 `UNC\`
    emitter.instruction("cmp rax, r10");                                        // UNC substitution?
    emitter.instruction("jne .Lreadlink_exact_drive_prefix");                   // drive path: strip four units
    emitter.instruction("mov WORD PTR [rdi + 12], 92");                         // synthesize first UNC slash
    emitter.instruction("mov WORD PTR [rdi + 14], 92");                         // synthesize second UNC slash
    emitter.instruction("add rdi, 12");                                         // source begins `\\server`
    emitter.instruction("jmp .Lreadlink_exact_prefix_done");                    // normalized UNC
    emitter.label(".Lreadlink_exact_drive_prefix");
    emitter.instruction("add rdi, 8");                                          // strip four UTF-16 prefix units
    emitter.label(".Lreadlink_exact_prefix_done");
    emitter.instruction("mov QWORD PTR [rsp + 112], rdi");                      // preserve selected wide source
    emitter.instruction("xor esi, esi");                                        // query UTF-8 size
    emitter.instruction("xor edx, edx");                                        // no destination buffer
    emitter.instruction("call __rt_win_utf16_to_utf8");                         // required bytes including NUL
    emitter.instruction("test eax, eax");                                       // conversion size query succeeded?
    emitter.instruction("jz .Lreadlink_exact_native_fail");                     // capture conversion failure
    emitter.instruction("mov DWORD PTR [rsp + 128], eax");                      // exact UTF-8 allocation size
    emitter.instruction("movsxd rax, eax");                                     // widen byte count
    emitter.instruction("call __rt_heap_alloc");                                // allocate temporary UTF-8 result
    emitter.instruction("test rax, rax");                                       // allocation succeeded?
    emitter.instruction("jz .Lreadlink_exact_alloc_utf8_fail");                 // cleanup wide/reparse buffers
    emitter.instruction("mov QWORD PTR [rsp + 120], rax");                      // owned UTF-8 temporary
    emitter.instruction("mov rdi, QWORD PTR [rsp + 112]");                      // selected wide source
    emitter.instruction("mov rsi, rax");                                        // UTF-8 temporary destination
    emitter.instruction("mov edx, DWORD PTR [rsp + 128]");                      // exact byte capacity
    emitter.instruction("call __rt_win_utf16_to_utf8");                         // strict conversion
    emitter.instruction("test eax, eax");                                       // conversion succeeded?
    emitter.instruction("jz .Lreadlink_exact_native_fail_utf8");                // cleanup all allocations
    emitter.instruction("dec eax");                                             // payload bytes exclude NUL
    emitter.instruction("mov rcx, QWORD PTR [rsp + 96]");                       // caller capacity
    emitter.instruction("cmp rax, rcx");                                        // payload fits caller buffer?
    emitter.instruction("cmova rax, rcx");                                      // POSIX readlink truncates to capacity
    emitter.instruction("mov QWORD PTR [rsp + 136], rax");                      // preserve return byte count
    emitter.instruction("mov rcx, rax");                                        // copy count
    emitter.instruction("mov rsi, QWORD PTR [rsp + 120]");                      // temporary UTF-8 source
    emitter.instruction("mov rdi, QWORD PTR [rsp + 88]");                       // caller destination
    emitter.instruction("cld");                                                 // copy forward regardless of prior string operations
    emitter.instruction("rep movsb");                                           // copy exactly returned bytes, no forced NUL
    emitter.instruction("jmp .Lreadlink_exact_cleanup_success");                // balanced cleanup
    emitter.label(".Lreadlink_exact_cleanup_success");
    emitter.instruction("mov rax, QWORD PTR [rsp + 120]");                      // UTF-8 temporary
    emitter.instruction("call __rt_heap_free");                                 // release temporary result
    emitter.instruction("mov rax, QWORD PTR [rsp + 72]");                       // reparse buffer
    emitter.instruction("call __rt_heap_free");                                 // release reparse data
    emitter.instruction("mov rax, QWORD PTR [rsp + 64]");                       // wide input path
    emitter.instruction("call __rt_heap_free");                                 // release input conversion
    emitter.instruction("mov rax, QWORD PTR [rsp + 136]");                      // return bytes copied
    emitter.instruction("add rsp, 152");                                        // restore stack
    emitter.instruction("ret");                                                 // return exact POSIX readlink count
    emitter.label(".Lreadlink_exact_native_fail_utf8");
    emitter.instruction("call GetLastError");                                   // capture final conversion failure
    emitter.instruction("mov DWORD PTR [rsp + 136], eax");                      // preserve native error
    emitter.instruction("mov rax, QWORD PTR [rsp + 120]");                      // UTF-8 temporary
    emitter.instruction("call __rt_heap_free");                                 // release failed conversion buffer
    emitter.instruction("jmp .Lreadlink_exact_native_fail_saved");              // shared cleanup
    emitter.label(".Lreadlink_exact_native_fail");
    emitter.instruction("call GetLastError");                                   // capture native failure
    emitter.instruction("mov DWORD PTR [rsp + 136], eax");                      // preserve native error
    emitter.label(".Lreadlink_exact_native_fail_saved");
    emitter.instruction("mov rax, QWORD PTR [rsp + 72]");                       // reparse buffer
    emitter.instruction("call __rt_heap_free");                                 // release reparse data
    emitter.instruction("mov rax, QWORD PTR [rsp + 64]");                       // wide input path
    emitter.instruction("call __rt_heap_free");                                 // release input conversion
    emitter.instruction("mov eax, DWORD PTR [rsp + 136]");                      // restore native error
    emitter.instruction("mov DWORD PTR [rip + __rt_win32_last_error], eax");    // retain native state
    emitter.instruction("call __rt_win32_errno_from_code");                     // translate errno
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], eax");               // publish errno
    emitter.instruction("mov rax, -1");                                         // return failure
    emitter.instruction("add rsp, 152");                                        // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lreadlink_exact_invalid_reparse");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 22");                // EINVAL for unsupported/malformed tag
    emitter.instruction("jmp .Lreadlink_exact_cleanup_errno");                  // cleanup owned buffers
    emitter.label(".Lreadlink_exact_alloc_utf8_fail");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 12");                // ENOMEM
    emitter.label(".Lreadlink_exact_cleanup_errno");
    emitter.instruction("mov rax, QWORD PTR [rsp + 72]");                       // reparse buffer
    emitter.instruction("call __rt_heap_free");                                 // release reparse data
    emitter.instruction("mov rax, QWORD PTR [rsp + 64]");                       // wide input path
    emitter.instruction("call __rt_heap_free");                                 // release input conversion
    emitter.instruction("mov rax, -1");                                         // return failure
    emitter.instruction("add rsp, 152");                                        // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lreadlink_exact_alloc_reparse_fail");
    emitter.instruction("mov rax, QWORD PTR [rsp + 64]");                       // wide input path
    emitter.instruction("call __rt_heap_free");                                 // release input conversion
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 12");                // ENOMEM
    emitter.instruction("mov rax, -1");                                         // return failure
    emitter.instruction("add rsp, 152");                                        // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lreadlink_exact_conversion_fail");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 84");                // EILSEQ
    emitter.instruction("mov rax, -1");                                         // return failure
    emitter.instruction("add rsp, 152");                                        // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits a tag-specific `lstat()` reparse-point probe.
///
/// The probe first excludes ordinary paths with `GetFileAttributesW`, then
/// opens a reparse point with `FILE_FLAG_OPEN_REPARSE_POINT` and reads its
/// `REPARSE_DATA_BUFFER`. It returns one only for `IO_REPARSE_TAG_SYMLINK`.
/// A junction (`IO_REPARSE_TAG_MOUNT_POINT`) and every other reparse point
/// return zero. A failed `DeviceIoControl` on an opened reparse point returns
/// -1 after publishing errno, matching php-src's `lstat` error propagation
/// rather than silently falling back to `stat`.
fn emit_shim_lstat_symlink_probe(emitter: &mut Emitter) {
    emitter.label_global("__rt_lstat_is_symlink");
    emitter.instruction("sub rsp, 152");                                        // shadow space, Win32 stack arguments, and owned wide/reparse buffers
    emitter.instruction("call __rt_win_utf8_to_utf16");                         // convert the SysV UTF-8 path without lossy ANSI fallback
    emitter.instruction("test rax, rax");                                       // conversion succeeded?
    emitter.instruction("jz .Llstat_probe_false");                              // invalid UTF-8 cannot be a symbolic link
    emitter.instruction("mov QWORD PTR [rsp + 64], rax");                       // retain the owned wide path across probing calls
    emitter.instruction("mov rcx, rax");                                        // inspect the converted path's native attributes first
    emitter.instruction("call GetFileAttributesW");                             // ordinary files must not enter FSCTL_GET_REPARSE_POINT
    emitter.instruction("cmp eax, -1");                                         // inaccessible paths fall through to the normal lstat() failure path
    emitter.instruction("je .Llstat_probe_cleanup_wide_false");                 // let stat() report the original path error
    emitter.instruction("test eax, 0x400");                                     // FILE_ATTRIBUTE_REPARSE_POINT set?
    emitter.instruction("jz .Llstat_probe_cleanup_wide_false");                 // regular files and directories use lstat's normal stat fallback
    emitter.instruction("mov rax, 16384");                                      // MAXIMUM_REPARSE_DATA_BUFFER_SIZE
    emitter.instruction("call __rt_heap_alloc");                                // allocate a complete REPARSE_DATA_BUFFER
    emitter.instruction("test rax, rax");                                       // allocation succeeded?
    emitter.instruction("jz .Llstat_probe_cleanup_wide_false");                 // conservative non-link result on allocation failure
    emitter.instruction("mov QWORD PTR [rsp + 72], rax");                       // retain the owned reparse buffer
    emitter.instruction("mov rcx, QWORD PTR [rsp + 64]");                       // lpFileName = wide source path
    emitter.instruction("xor edx, edx");                                        // metadata-only access is sufficient for the control query
    emitter.instruction("mov r8, 7");                                           // permit concurrent readers, writers, and deleters
    emitter.instruction("xor r9, r9");                                          // lpSecurityAttributes = NULL
    emitter.instruction("mov QWORD PTR [rsp + 32], 3");                         // dwCreationDisposition = OPEN_EXISTING
    emitter.instruction("mov QWORD PTR [rsp + 40], 0x2200000");                 // FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT
    emitter.instruction("mov QWORD PTR [rsp + 48], 0");                         // hTemplateFile = NULL
    emitter.instruction("call CreateFileW");                                    // open the reparse point itself rather than its target
    emitter.instruction("cmp rax, -1");                                         // INVALID_HANDLE_VALUE?
    emitter.instruction("je .Llstat_probe_cleanup_false");                      // inaccessible/non-reparse paths are not marked as links
    emitter.instruction("mov QWORD PTR [rsp + 80], rax");                       // preserve the opened handle through DeviceIoControl
    emitter.instruction("mov rcx, rax");                                        // hDevice = opened reparse point
    emitter.instruction("mov edx, 0x900A8");                                    // dwIoControlCode = FSCTL_GET_REPARSE_POINT
    emitter.instruction("xor r8, r8");                                          // lpInBuffer = NULL
    emitter.instruction("xor r9d, r9d");                                        // nInBufferSize = 0
    emitter.instruction("mov rax, QWORD PTR [rsp + 72]");                       // allocated REPARSE_DATA_BUFFER
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // lpOutBuffer (fifth MSx64 argument)
    emitter.instruction("mov QWORD PTR [rsp + 40], 16384");                     // nOutBufferSize = MAXIMUM_REPARSE_DATA_BUFFER_SIZE
    emitter.instruction("lea rax, [rsp + 104]");                                // bytes-returned storage
    emitter.instruction("mov QWORD PTR [rsp + 48], rax");                       // lpBytesReturned (seventh MSx64 argument)
    emitter.instruction("mov QWORD PTR [rsp + 56], 0");                         // lpOverlapped = NULL
    emitter.instruction("call DeviceIoControl");                                // fetch the reparse tag from the opened object
    emitter.instruction("mov DWORD PTR [rsp + 136], eax");                      // retain control success across CloseHandle
    emitter.instruction("test eax, eax");                                       // DeviceIoControl succeeded?
    emitter.instruction("jnz .Llstat_probe_ioctl_ok");                          // inspect the returned reparse tag
    emitter.instruction("call GetLastError");                                   // capture reparse-control failure before closing its handle
    emitter.instruction("mov DWORD PTR [rsp + 136], eax");                      // preserve native error through cleanup
    emitter.instruction("mov rcx, QWORD PTR [rsp + 80]");                       // opened reparse-point handle
    emitter.instruction("call CloseHandle");                                    // close failed control-query handle before returning errno
    emitter.instruction("jmp .Llstat_probe_cleanup_error");                     // free probe buffers and propagate failure to lstat
    emitter.label(".Llstat_probe_ioctl_ok");
    emitter.instruction("mov rcx, QWORD PTR [rsp + 80]");                       // opened reparse-point handle
    emitter.instruction("call CloseHandle");                                    // close successful control-query handle
    emitter.instruction("cmp DWORD PTR [rsp + 104], 8");                        // REPARSE_DATA_BUFFER header is complete?
    emitter.instruction("jb .Llstat_probe_cleanup_false");                      // malformed replies are not symbolic links
    emitter.instruction("mov rax, QWORD PTR [rsp + 72]");                       // REPARSE_DATA_BUFFER base
    emitter.instruction("cmp DWORD PTR [rax], 0xA000000C");                     // IO_REPARSE_TAG_SYMLINK only
    emitter.instruction("sete al");                                             // return one for symlinks and zero for junctions/other tags
    emitter.instruction("movzx eax, al");                                       // normalize the Boolean return value
    emitter.instruction("mov DWORD PTR [rsp + 136], eax");                      // preserve the classification through heap cleanup
    emitter.instruction("jmp .Llstat_probe_cleanup_result");                    // release owned probe storage
    emitter.label(".Llstat_probe_cleanup_false");
    emitter.instruction("xor eax, eax");                                        // default to non-link for failed/unsupported probes
    emitter.instruction("mov DWORD PTR [rsp + 136], eax");                      // preserve false through heap cleanup
    emitter.label(".Llstat_probe_cleanup_result");
    emitter.instruction("mov rax, QWORD PTR [rsp + 72]");                       // owned reparse buffer
    emitter.instruction("call __rt_heap_free");                                 // release REPARSE_DATA_BUFFER
    emitter.instruction("mov rax, QWORD PTR [rsp + 64]");                       // owned wide input path
    emitter.instruction("call __rt_heap_free");                                 // release UTF-16 conversion
    emitter.instruction("mov eax, DWORD PTR [rsp + 136]");                      // restore Boolean classification
    emitter.instruction("add rsp, 152");                                        // restore the SysV caller stack
    emitter.instruction("ret");                                                 // return one only for IO_REPARSE_TAG_SYMLINK
    emitter.label(".Llstat_probe_cleanup_error");
    emitter.instruction("mov rax, QWORD PTR [rsp + 72]");                       // owned reparse buffer after failed control query
    emitter.instruction("call __rt_heap_free");                                 // release REPARSE_DATA_BUFFER
    emitter.instruction("mov rax, QWORD PTR [rsp + 64]");                       // owned wide input path
    emitter.instruction("call __rt_heap_free");                                 // release UTF-16 conversion
    emitter.instruction("mov eax, DWORD PTR [rsp + 136]");                      // restore native DeviceIoControl error
    emitter.instruction("mov DWORD PTR [rip + __rt_win32_last_error], eax");    // retain diagnostic state for callers
    emitter.instruction("call __rt_win32_errno_from_code");                     // translate native reparse-control error to POSIX errno
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], eax");               // publish errno for lstat()
    emitter.instruction("mov rax, -1");                                         // tri-state failure result
    emitter.instruction("add rsp, 152");                                        // restore the SysV caller stack
    emitter.instruction("ret");                                                 // return failed reparse classification
    emitter.label(".Llstat_probe_cleanup_wide_false");
    emitter.instruction("mov rax, QWORD PTR [rsp + 64]");                       // owned wide input after allocation failure
    emitter.instruction("call __rt_heap_free");                                 // release UTF-16 conversion
    emitter.label(".Llstat_probe_false");
    emitter.instruction("xor eax, eax");                                        // failed conversion/allocation is conservatively not a link
    emitter.instruction("add rsp, 152");                                        // restore the SysV caller stack
    emitter.instruction("ret");                                                 // return false
    emitter.blank();
}

/// Emits conversion of a stack-resident Win32 `FILETIME` to the Unix epoch
/// seconds field of the Linux-layout `struct stat` currently based in `rsi`.
///
/// `BY_HANDLE_FILE_INFORMATION` stores 100-nanosecond ticks since 1601. The
/// subtraction and signed division preserve valid pre-1970 link timestamps;
/// all clobbered registers are volatile under the MSx64 calls already complete
/// on the surrounding `lstat` path.
fn emit_lstat_filetime_seconds(emitter: &mut Emitter, src_off: usize, dst_off: usize) {
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", src_off));    // load the 64-bit FILETIME tick count
    emitter.instruction("mov r8, 116444736000000000");                          // 1601-to-1970 offset in 100-nanosecond ticks
    emitter.instruction("sub rax, r8");                                         // rebase onto the Unix epoch
    emitter.instruction("cqo");                                                 // sign-extend pre-1970 deltas for signed division
    emitter.instruction("mov r9, 10000000");                                    // FILETIME ticks per whole second
    emitter.instruction("idiv r9");                                             // rax = Unix epoch seconds
    emitter.instruction(&format!("mov QWORD PTR [rsi + {}], rax", dst_off));    // store the stat timestamp seconds
}

/// Emits the shared Win32 and Winsock error-code to POSIX `errno` translator.
pub(super) fn emit_win32_errno_translate(emitter: &mut Emitter) {
    emitter.label_global("__rt_win32_errno_from_code");
    emitter.instruction("mov ecx, eax");                                        // save the raw Win32/WSA code for the compare chain
    emitter.instruction("cmp ecx, 10035");                                      // WSAEWOULDBLOCK
    emitter.instruction("je .Lw32err_eagain");                                  // -> EAGAIN
    emitter.instruction("cmp ecx, 232");                                        // ERROR_NO_DATA (nonblocking pipe read, no data yet)
    emitter.instruction("je .Lw32err_eagain");                                  // -> EAGAIN
    emitter.instruction("cmp ecx, 997");                                        // ERROR_IO_PENDING (overlapped I/O still in flight)
    emitter.instruction("je .Lw32err_eagain");                                  // -> EAGAIN
    emitter.instruction("cmp ecx, 2");                                          // ERROR_FILE_NOT_FOUND
    emitter.instruction("je .Lw32err_enoent");                                  // -> ENOENT
    emitter.instruction("cmp ecx, 3");                                          // ERROR_PATH_NOT_FOUND
    emitter.instruction("je .Lw32err_enoent");                                  // -> ENOENT
    emitter.instruction("cmp ecx, 5");                                          // ERROR_ACCESS_DENIED
    emitter.instruction("je .Lw32err_eacces");                                  // -> EACCES
    emitter.instruction("cmp ecx, 6");                                          // ERROR_INVALID_HANDLE
    emitter.instruction("je .Lw32err_ebadf");                                   // -> EBADF
    emitter.instruction("cmp ecx, 109");                                        // ERROR_BROKEN_PIPE
    emitter.instruction("je .Lw32err_epipe");                                   // -> EPIPE
    emitter.instruction("cmp ecx, 122");                                        // ERROR_INSUFFICIENT_BUFFER
    emitter.instruction("je .Lw32err_erange");                                  // -> ERANGE
    emitter.instruction("cmp ecx, 1113");                                       // ERROR_NO_UNICODE_TRANSLATION
    emitter.instruction("je .Lw32err_eilseq");                                  // -> EILSEQ
    emitter.instruction("cmp ecx, 38");                                         // ERROR_HANDLE_EOF
    emitter.instruction("je .Lw32err_eof");                                     // -> 0 (EOF is not an error)
    emitter.instruction("cmp ecx, 10004");                                      // WSAEINTR
    emitter.instruction("je .Lw32err_eintr");                                   // -> EINTR
    emitter.instruction("cmp ecx, 10054");                                      // WSAECONNRESET
    emitter.instruction("je .Lw32err_econnreset");                              // -> ECONNRESET
    emitter.instruction("cmp ecx, 10060");                                      // WSAETIMEDOUT
    emitter.instruction("je .Lw32err_etimedout");                               // -> ETIMEDOUT
    emitter.instruction("cmp ecx, 10061");                                      // WSAECONNREFUSED
    emitter.instruction("je .Lw32err_econnrefused");                            // -> ECONNREFUSED
    emitter.instruction("cmp ecx, 10036");                                      // WSAEINPROGRESS
    emitter.instruction("je .Lw32err_einprogress");                             // -> EINPROGRESS
    emitter.instruction("cmp ecx, 10037");                                      // WSAEALREADY
    emitter.instruction("je .Lw32err_ealready");                                // -> EALREADY
    emitter.instruction("cmp ecx, 10038");                                      // WSAENOTSOCK
    emitter.instruction("je .Lw32err_enotsock");                                // -> ENOTSOCK
    emitter.instruction("cmp ecx, 10048");                                      // WSAEADDRINUSE
    emitter.instruction("je .Lw32err_eaddrinuse");                              // -> EADDRINUSE
    emitter.instruction("cmp ecx, 10049");                                      // WSAEADDRNOTAVAIL
    emitter.instruction("je .Lw32err_eaddrnotavail");                           // -> EADDRNOTAVAIL
    emitter.instruction("cmp ecx, 10050");                                      // WSAENETDOWN
    emitter.instruction("je .Lw32err_enetdown");                                // -> ENETDOWN
    emitter.instruction("cmp ecx, 10051");                                      // WSAENETUNREACH
    emitter.instruction("je .Lw32err_enetunreach");                             // -> ENETUNREACH
    emitter.instruction("cmp ecx, 10053");                                      // WSAECONNABORTED
    emitter.instruction("je .Lw32err_econnaborted");                            // -> ECONNABORTED
    emitter.instruction("cmp ecx, 10055");                                      // WSAENOBUFS
    emitter.instruction("je .Lw32err_enobufs");                                 // -> ENOBUFS
    emitter.instruction("cmp ecx, 10057");                                      // WSAENOTCONN
    emitter.instruction("je .Lw32err_enotconn");                                // -> ENOTCONN
    emitter.instruction("cmp ecx, 112");                                        // ERROR_DISK_FULL
    emitter.instruction("je .Lw32err_enospc");                                  // -> ENOSPC
    emitter.instruction("cmp ecx, 145");                                        // ERROR_DIR_NOT_EMPTY
    emitter.instruction("je .Lw32err_enotempty");                               // -> ENOTEMPTY
    emitter.instruction("cmp ecx, 183");                                        // ERROR_ALREADY_EXISTS
    emitter.instruction("je .Lw32err_eexist");                                  // -> EEXIST
    emitter.instruction("cmp ecx, 206");                                        // ERROR_FILENAME_EXCED_RANGE
    emitter.instruction("je .Lw32err_enametoolong");                            // -> ENAMETOOLONG
    emitter.instruction("cmp ecx, 267");                                        // ERROR_DIRECTORY
    emitter.instruction("je .Lw32err_enotdir");                                 // -> ENOTDIR
    emitter.instruction("mov eax, 5");                                          // unknown code -> EIO
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lw32err_eagain");
    emitter.instruction("mov eax, 11");                                         // EAGAIN
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lw32err_enoent");
    emitter.instruction("mov eax, 2");                                          // ENOENT
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lw32err_eacces");
    emitter.instruction("mov eax, 13");                                         // EACCES
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lw32err_ebadf");
    emitter.instruction("mov eax, 9");                                          // EBADF
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lw32err_epipe");
    emitter.instruction("mov eax, 32");                                         // EPIPE
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lw32err_erange");
    emitter.instruction("mov eax, 34");                                         // ERANGE
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lw32err_eilseq");
    emitter.instruction("mov eax, 84");                                         // EILSEQ
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lw32err_eof");
    emitter.instruction("xor eax, eax");                                        // EOF is not an error
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lw32err_eintr");
    emitter.instruction("mov eax, 4");                                          // EINTR
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lw32err_econnreset");
    emitter.instruction("mov eax, 104");                                        // ECONNRESET
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lw32err_etimedout");
    emitter.instruction("mov eax, 110");                                        // ETIMEDOUT
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lw32err_econnrefused");
    emitter.instruction("mov eax, 111");                                        // ECONNREFUSED
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lw32err_einprogress");
    emitter.instruction("mov eax, 115");                                        // EINPROGRESS
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lw32err_ealready");
    emitter.instruction("mov eax, 114");                                        // EALREADY
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lw32err_enotsock");
    emitter.instruction("mov eax, 88");                                         // ENOTSOCK
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lw32err_eaddrinuse");
    emitter.instruction("mov eax, 98");                                         // EADDRINUSE
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lw32err_eaddrnotavail");
    emitter.instruction("mov eax, 99");                                         // EADDRNOTAVAIL
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lw32err_enetdown");
    emitter.instruction("mov eax, 100");                                        // ENETDOWN
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lw32err_enetunreach");
    emitter.instruction("mov eax, 101");                                        // ENETUNREACH
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lw32err_econnaborted");
    emitter.instruction("mov eax, 103");                                        // ECONNABORTED
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lw32err_enobufs");
    emitter.instruction("mov eax, 105");                                        // ENOBUFS
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lw32err_enotconn");
    emitter.instruction("mov eax, 107");                                        // ENOTCONN
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lw32err_enospc");
    emitter.instruction("mov eax, 28");                                         // ENOSPC
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lw32err_enotempty");
    emitter.instruction("mov eax, 39");                                         // ENOTEMPTY
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lw32err_eexist");
    emitter.instruction("mov eax, 17");                                         // EEXIST
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lw32err_enametoolong");
    emitter.instruction("mov eax, 36");                                         // ENAMETOOLONG
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lw32err_enotdir");
    emitter.instruction("mov eax, 20");                                         // ENOTDIR
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits the W3f-B msvcrt-real family for `runtime/io/principal_lookup.rs`'s
/// passwd/group lookup: `fopen`/`fgets`/`fclose`/`strncmp`/`strchr`/
/// `strtoul` all EXIST as standard msvcrt symbols, so each gets a real
/// ABI arg-shuffle shim rather than a stub. `/etc/passwd`/`/etc/group` do
/// not exist on Windows, so `fopen(_etc_passwd_path, "r")` returns `NULL`
/// naturally and the lookup's pre-existing `je <fail_label>` path handles
/// it — no bespoke "unsupported" behavior is needed for this family.
pub(super) fn emit_shim_msvcrt_passwd_lookup(emitter: &mut Emitter) {
    emit_shim_fopen(emitter);
    emit_shim_fgets(emitter);
    emit_shim_fclose(emitter);
    emit_shim_strncmp(emitter);
    emit_shim_strchr(emitter);
    emit_shim_strtoul(emitter);
}

/// Emits the `__rt_sys_fopen` shim: converts SysV `fopen(const char* path,
/// const char* mode)` (rdi, rsi) to MSx64 `fopen` (rcx=path, rdx=mode). No
/// register collision, so the two moves may run in either order. Returns a
/// `FILE*` in `rax`; the sole consumer (`principal_lookup.rs:47-48`) does
/// `test rax, rax; je <fail>` — a zero/nonzero pointer test, not a sign
/// test — so no cdqe.
fn emit_shim_fopen(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_fopen");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8)
    emitter.instruction("mov rdx, rsi");                                        // mode → arg2 (rdx)
    emitter.instruction("mov rcx, rdi");                                        // path → arg1 (rcx)
    emitter.instruction("call fopen");                                          // msvcrt fopen (MSx64 ABI, returns FILE* in rax)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return (no cdqe — pointer test only)
    emitter.blank();
}

/// Emits the `__rt_sys_fgets` shim: converts SysV `fgets(char* buf, int n,
/// FILE* stream)` (rdi, esi, rdx) to MSx64 `fgets` (rcx=buf, edx=n, r8=stream).
/// Register-shuffle hazard: SysV arg3 (stream) is in `rdx`, which is ALSO the
/// MSx64 arg2 (n) target, so it is saved to `r8` BEFORE `rdx` is overwritten
/// by the arg2 shuffle; `edx`←`esi` (n) and `rcx`←`rdi` (buf) move last.
/// Returns a `char*` in `rax`; the sole consumer (`principal_lookup.rs:56-57`)
/// does `test rax, rax; je <close_fail>` — a zero/nonzero pointer test — so
/// no cdqe.
fn emit_shim_fgets(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_fgets");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8)
    emitter.instruction("mov r8, rdx");                                         // SAVE stream (SysV arg3) before rdx is overwritten
    emitter.instruction("mov edx, esi");                                        // n → arg2 (edx)
    emitter.instruction("mov rcx, rdi");                                        // buf → arg1 (rcx)
    emitter.instruction("call fgets");                                          // msvcrt fgets (MSx64 ABI, returns char* in rax)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return (no cdqe — pointer test only)
    emitter.blank();
}

/// Emits the `__rt_sys_fclose` shim: converts SysV `fclose(FILE* stream)`
/// (rdi) to MSx64 `fclose` (rcx=stream). Mirrors `emit_shim_zlib_trivial_1arg`
/// (1-arg case). Neither call site (`principal_lookup.rs:79,85`) reads the
/// return value, so no cdqe verdict is reachable.
fn emit_shim_fclose(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_fclose");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8)
    emitter.instruction("mov rcx, rdi");                                        // stream → arg1 (rcx)
    emitter.instruction("call fclose");                                         // msvcrt fclose (MSx64 ABI, returns int in eax)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return (no cdqe — return value unread)
    emitter.blank();
}

/// Emits the `__rt_sys_strncmp` shim: converts SysV `strncmp(const char* a,
/// const char* b, size_t n)` (rdi, rsi, rdx) to MSx64 `strncmp` (rcx=a,
/// rdx=b, r8=n). Register-shuffle hazard: SysV arg3 (n) is in `rdx`, which is
/// ALSO the MSx64 arg2 (b) target, so it is saved to `r8` BEFORE `rdx` is
/// overwritten by the arg2 shuffle; `rdx`←`rsi` (b) and `rcx`←`rdi` (a) move
/// last. Returns an `int` in `eax`; the sole consumer
/// (`principal_lookup.rs:62-63`) does `test eax, eax; jne <loop>` — a
/// zero/nonzero test, never a sign test — so no cdqe.
fn emit_shim_strncmp(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_strncmp");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8)
    emitter.instruction("mov r8, rdx");                                         // SAVE n (SysV arg3) before rdx is overwritten
    emitter.instruction("mov rdx, rsi");                                        // b → arg2 (rdx)
    emitter.instruction("mov rcx, rdi");                                        // a → arg1 (rcx)
    emitter.instruction("call strncmp");                                        // msvcrt strncmp (MSx64 ABI, returns int in eax)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return (no cdqe — zero/nonzero test only)
    emitter.blank();
}

/// Emits the `__rt_sys_strchr` shim: converts SysV `strchr(const char* s,
/// int c)` (rdi, esi) to MSx64 `strchr` (rcx=s, edx=c). No register
/// collision, so the two moves may run in either order. Returns a `char*` in
/// `rax`; the sole consumer (`principal_lookup.rs:71-72`) does `test rax,
/// rax; je <loop>` — a zero/nonzero pointer test — so no cdqe.
fn emit_shim_strchr(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_strchr");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8)
    emitter.instruction("mov edx, esi");                                        // c → arg2 (edx)
    emitter.instruction("mov rcx, rdi");                                        // s → arg1 (rcx)
    emitter.instruction("call strchr");                                         // msvcrt strchr (MSx64 ABI, returns char* in rax)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return (no cdqe — pointer test only)
    emitter.blank();
}

/// Emits the `__rt_sys_strtoul` shim: converts SysV `strtoul(const char* s,
/// char** endptr, int base)` (rdi, rsi, edx) to MSx64 `strtoul` (rcx=s,
/// rdx=endptr, r8d=base). Register-shuffle hazard: SysV arg3 (base) is in
/// `edx`, which is ALSO the MSx64 arg2 (endptr) target, so it is saved to
/// `r8d` BEFORE `rdx` is overwritten by the arg2 shuffle; `rdx`←`rsi`
/// (endptr) and `rcx`←`rdi` (s) move last.
///
/// No cdqe: the sole consumer (`principal_lookup.rs:76-77`) stores the full
/// `rax` directly with no test at all. `unsigned long` is 32-bit under
/// Windows LLP64 (vs. 64-bit LP64 on Linux/macOS) — but a plain MSx64 write
/// to `eax` (msvcrt `strtoul`'s return register) implicitly zero-extends
/// into `rax` per the x86-64 architecture's register-write rule, and the
/// parsed uid/gid values are always small non-negative numbers, so the
/// zero-extension is exactly the correct widening — no cdqe (sign-extension)
/// would be wrong here even if a consumer did sign-test, since the value is
/// unsigned.
fn emit_shim_strtoul(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_strtoul");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8)
    emitter.instruction("mov r8d, edx");                                        // SAVE base (SysV arg3) before rdx is overwritten
    emitter.instruction("mov rdx, rsi");                                        // endptr → arg2 (rdx)
    emitter.instruction("mov rcx, rdi");                                        // s → arg1 (rcx)
    emitter.instruction("call strtoul");                                        // msvcrt strtoul (returns unsigned long in eax; zero-extends into rax)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return (no cdqe — unsigned, zero-extension is correct)
    emitter.blank();
}
