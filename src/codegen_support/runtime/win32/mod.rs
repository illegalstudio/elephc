//! Purpose:
//! Emits Win32 API shim wrappers that convert SysV calling convention arguments
//! to MSx64 ABI and call the corresponding Win32 API functions. These shims are
//! the bridge between the existing Linux x86_64 runtime (which sets up arguments
//! in rdi/rsi/rdx/r10/rcx/r8/r9) and Windows kernel32/msvcrt functions.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` when target is Windows x86_64.
//!
//! Key details:
//! - Each shim takes arguments in SysV registers and shuffles them to MSx64 (rcx/rdx/r8/r9).
//! - 32-byte shadow space is allocated before each Win32 call and freed after.
//! - Stack is aligned to 16 bytes before each call.
//! - Win32 imports are declared via `.extern` — the MinGW linker resolves them.
//! - For imported functions, we use `call [rip+__imp_<name>]` (IAT indirection)
//!   to be immune to import-distance relocation issues.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::{Arch, Platform};
use crate::codegen_support::RuntimeFeatures;

mod imports;
mod shims_c_symbols;
mod shims_compress;
mod shims_encoding;
mod shims_errors;
mod shims_fs;
mod shims_misc;
mod shims_net;
mod shims_pcre;
mod shims_time;
#[cfg(test)]
mod tests;

use imports::*;
use shims_c_symbols::*;
use shims_compress::*;
use shims_encoding::*;
use shims_errors::*;
use shims_fs::*;
use shims_misc::*;
use shims_net::*;
use shims_pcre::*;
use shims_time::*;

pub(crate) use imports::windows_c_shim_name;
pub(crate) use shims_compress::emit_shim_iconv;

/// Emits all Win32 shim wrappers for the Windows x86_64 target.
///
/// Each shim converts SysV calling convention to MSx64 and calls the
/// corresponding Win32 API function. The existing runtime code sets up
/// arguments in SysV registers (rdi, rsi, rdx, r10, r8, r9) before calling
/// these shims — the shims handle the ABI conversion. The zlib/bzip2/pcre2/
/// iconv third-party shim families are gated on `features` so programs that
/// do not link those libraries never reference their symbols (avoiding an
/// undefined-reference link failure against the base MinGW link set).
pub(crate) fn emit_win32_shims(emitter: &mut Emitter, features: RuntimeFeatures) {
    debug_assert_eq!(
        (emitter.platform, emitter.target.arch),
        (Platform::Windows, Arch::X86_64),
        "Win32 shims are only emitted for windows-x86_64"
    );

    emit_win32_imports(emitter);
    emit_win32_encoding_helpers(emitter);
    emit_win32_error_message(emitter);
    emit_native_errno_capture_helpers(emitter);
    emit_shim_write(emitter);
    emit_shim_read(emitter);
    emit_shim_unsupported_syscall(emitter);
    emit_shim_exit(emitter);
    emit_fiber_stack_overflow_abort(emitter);
    emit_shim_sys_init_argv(emitter);
    emit_shim_sys_free_argv(emitter);
    emit_win_stream_slot_registry(emitter);
    emit_win_stream_slot_clear(emitter);
    emit_win_stream_mark_timed_out(emitter);
    emit_win_stream_clear_timed_out(emitter);
    emit_shim_close(emitter);
    emit_shim_mmap(emitter);
    emit_shim_munmap(emitter);
    emit_shim_brk(emitter);
    emit_shim_getpid(emitter);
    emit_shim_clock_gettime(emitter);
    emit_shim_getrandom(emitter);
    emit_shim_fstat(emitter);
    emit_shim_open(emitter);
    emit_shim_lseek(emitter);
    emit_win_fd_status_registry(emitter);
    emit_is_crt_fd(emitter);
    emit_shim_fcntl(emitter);
    emit_shim_unlink(emitter);
    emit_shim_getcwd(emitter);
    emit_shim_chdir(emitter);
    emit_shim_mkdir(emitter);
    emit_shim_rmdir(emitter);
    emit_shim_stat(emitter);
    emit_shim_rename(emitter);
    emit_shim_chmod(emitter);
    emit_shim_getenv(emitter);
    emit_shim_putenv(emitter);
    emit_shim_tzset(emitter);
    emit_shim_time(emitter);
    emit_shim_win_safe_gmtime(emitter);
    emit_shim_localtime(emitter);
    emit_shim_gmtime(emitter);
    emit_shim_mktime(emitter);
    emit_shim_gettimeofday(emitter);
    emit_shim_gethostname(emitter);
    emit_shim_strtod(emitter);
    emit_shim_strtol(emitter);
    emit_shim_gethostbyname(emitter);
    emit_shim_snprintf(emitter);
    emit_shim_snprintf_double(emitter);
    emit_shim_math_fp(emitter);
    if features.zlib {
        emit_shim_zlib(emitter);
    }
    if features.bzip2 {
        emit_shim_bzip2(emitter);
    }
    if features.regex {
        emit_shim_pcre2_posix(emitter);
    }
    emit_shim_malloc(emitter);
    emit_shim_free(emitter);
    emit_unix_loopback_registry(emitter);
    emit_shim_socket_shims(emitter);
    emit_winsock_init(emitter);
    emit_winsock_cleanup(emitter);
    emit_shim_access(emitter);
    emit_shim_ftruncate(emitter);
    emit_shim_getrusage(emitter);
    emit_shim_ioctl(emitter);
    emit_shim_dup_shims(emitter);
    emit_shim_getuid_shims(emitter);
    emit_shim_kill(emitter);
    emit_shim_uname(emitter);
    emit_shim_accept4(emitter);
    emit_shim_writev(emitter);
    emit_shim_sysinfo(emitter);
    emit_shim_execve(emitter);
    emit_shim_futex(emitter);
    emit_shim_mprotect(emitter);
    emit_shim_setsockopt(emitter);
    emit_shim_getsockopt(emitter);
    emit_shim_c_symbols(emitter);
    emit_shim_c_symbol_delegates(emitter);
    emit_shim_socketpair(emitter);
    emit_shim_statfs(emitter);
    emit_shim_sys_get_temp_dir(emitter);
    emit_shim_pselect6(emitter);
    emit_shim_sendmsg(emitter);
    emit_shim_recvmsg(emitter);
    emit_shim_getdents(emitter);
    emit_shim_creat(emitter);
    emit_shim_clock_getres(emitter);
    emit_shim_newfstatat(emitter);
    emit_shim_net_dns(emitter);
    if features.iconv {
        emit_shim_iconv(emitter);
    }
    emit_shim_msvcrt_passwd_lookup(emitter);
    emit_shim_opendir(emitter);
    emit_shim_readdir(emitter);
    emit_shim_closedir(emitter);
    emit_shim_rewinddir(emitter);
    emit_shim_mkstemp(emitter);
}

/// Emits the fd-to-HANDLE conversion helper.
///
/// All file descriptors exposed by the runtime, including the CRT-provided
/// standard descriptors, belong to the Microsoft CRT descriptor table.
/// `_get_osfhandle` recovers the underlying Win32 handle for direct API calls.
pub(crate) fn emit_fd_to_handle(emitter: &mut Emitter) {
    emitter.label_global("__rt_fd_to_handle");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov ecx, edi");                                        // CRT file descriptor
    emitter.instruction("call _get_osfhandle");                                 // recover the owned Win32 HANDLE
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return handle
    emitter.blank();
}

/// Emits a bounded CRT-descriptor predicate for runtime paths that index
/// fixed per-fd tables. Raw Winsock SOCKET values are opaque pointer-sized
/// handles and must never be used as table offsets.
pub(crate) fn emit_is_crt_fd(emitter: &mut Emitter) {
    emitter.label_global("__rt_win_is_crt_fd");
    emitter.instruction("sub rsp, 40");                                         // reserve MSx64 shadow space
    emitter.instruction("mov ecx, edi");                                        // pass the candidate descriptor to the CRT
    emitter.instruction("call _get_osfhandle");                                 // recover a HANDLE only for CRT descriptors
    emitter.instruction("cmp rax, -1");                                         // CRT rejects raw Winsock SOCKET values
    emitter.instruction("setne al");                                            // return whether the descriptor belongs to the CRT table
    emitter.instruction("movzx eax, al");                                       // widen the boolean result
    emitter.instruction("add rsp, 40");                                         // release shadow space
    emitter.instruction("ret");                                                 // return 1 for CRT fd, 0 for raw SOCKET
    emitter.blank();
}

/// Emits the Windows entry point wrapper.
///
/// MinGW's CRT startup calls `main(argc, argv, envp)` with MSx64 ABI:
/// rcx=argc, rdx=argv, r8=envp. Our codegen expects SysV ABI:
/// rdi=argc, rsi=argv. This wrapper shuffles the arguments.
pub(crate) fn emit_main_wrapper(emitter: &mut Emitter) {
    emitter.label_global("main");
    emitter.instruction("sub rsp, 24");                                         // align stack to 16 bytes + spill slots for argc/argv
    emitter.instruction("mov QWORD PTR [rsp + 0], rcx");                        // spill argc (rcx is volatile on MSx64) across the init call
    emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                        // spill argv (rdx is volatile on MSx64) across the init call
    // -- initialize Winsock before any socket use --
    emitter.instruction("call __rt_winsock_init");                              // WSAStartup(MAKEWORD(2,2), &wsadata) — idempotent across re-entry
    emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");                        // SysV arg1 = argc (reloaded after the init call)
    emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");                        // SysV arg2 = argv (reloaded after the init call)
    emitter.instruction("call __elephc_main");                                  // call the real program entry
    emitter.instruction("mov QWORD PTR [rsp + 16], rax");                       // preserve program status across Winsock cleanup
    emitter.instruction("call __rt_winsock_cleanup");                           // balance WSAStartup on normal program return
    emitter.instruction("mov rax, QWORD PTR [rsp + 16]");                       // restore program status for the CRT
    emitter.instruction("add rsp, 24");                                         // restore stack
    emitter.instruction("ret");                                                 // return to CRT
    emitter.blank();
}
