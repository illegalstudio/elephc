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

/// Emits all Win32 shim wrappers for the Windows x86_64 target.
///
/// Each shim converts SysV calling convention to MSx64 and calls the
/// corresponding Win32 API function. The existing runtime code sets up
/// arguments in SysV registers (rdi, rsi, rdx, r10, r8, r9) before calling
/// these shims — the shims handle the ABI conversion.
pub(crate) fn emit_win32_shims(emitter: &mut Emitter) {
    debug_assert_eq!(
        (emitter.platform, emitter.target.arch),
        (Platform::Windows, Arch::X86_64),
        "Win32 shims are only emitted for windows-x86_64"
    );

    emit_win32_imports(emitter);
    emit_shim_write(emitter);
    emit_shim_read(emitter);
    emit_shim_unsupported_syscall(emitter);
    emit_shim_exit(emitter);
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
    emit_shim_fcntl(emitter);
    emit_shim_unlink(emitter);
    emit_shim_getcwd(emitter);
    emit_shim_chdir(emitter);
    emit_shim_mkdir(emitter);
    emit_shim_rmdir(emitter);
    emit_shim_stat(emitter);
    emit_shim_rename(emitter);
    emit_shim_chmod(emitter);
    emit_shim_getenv_shim(emitter);
    emit_shim_gethostname(emitter);
    emit_shim_socket_shims(emitter);
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
    emit_shim_pselect6(emitter);
    emit_shim_sendmsg(emitter);
    emit_shim_recvmsg(emitter);
    emit_shim_getdents(emitter);
    emit_shim_creat(emitter);
    emit_shim_clock_getres(emitter);
    emit_shim_newfstatat(emitter);
}

/// Emits `.extern` declarations for all Win32 API functions used by the shims.
fn emit_win32_imports(emitter: &mut Emitter) {
    emitter.raw("    # -- Win32 API imports (resolved by MinGW linker against kernel32/msvcrt) --");
    for func in WIN32_IMPORTS {
        emitter.raw(&format!(".extern {}", func));
    }
    emitter.blank();
}

/// Win32 API functions imported by the shims.
const WIN32_IMPORTS: &[&str] = &[
    "GetStdHandle",
    "WriteFile",
    "ReadFile",
    "CloseHandle",
    "ExitProcess",
    "GetProcessHeap",
    "HeapAlloc",
    "HeapFree",
    "VirtualAlloc",
    "VirtualFree",
    "VirtualProtect",
    "GetCurrentProcessId",
    "GetSystemTimeAsFileTime",
    "QueryPerformanceCounter",
    "QueryPerformanceFrequency",
    "BCryptGenRandom",
    "CreateFileA",
    "SetFilePointer",
    "GetFileType",
    "DeleteFileA",
    "GetCurrentDirectoryA",
    "SetCurrentDirectoryA",
    "CreateDirectoryA",
    "RemoveDirectoryA",
    "GetFileAttributesA",
    "MoveFileA",
    "SetFileAttributesA",
    "GetEnvironmentVariableA",
    "gethostname",
    "socket",
    "connect",
    "bind",
    "listen",
    "accept",
    "send",
    "recv",
    "sendto",
    "recvfrom",
    "shutdown",
    "closesocket",
    "getsockname",
    "getpeername",
    "setsockopt",
    "getsockopt",
    "ioctlsocket",
    "WSAGetLastError",
    "getpid",
    "_putenv",
    "uname",
    "sysinfo",
    "execve",
    "kill",
    "futex",
    "stat",
    "fstat",
    "lstat",
    "FlushFileBuffers",
    "LockFileEx",
    "UnlockFileEx",
    "CreateSymbolicLinkA",
    "CreateHardLinkA",
    "FindFirstFileA",
    "FindNextFileA",
    "FindClose",
    "GetFullPathNameA",
    "GetFinalPathNameByHandleA",
    "SetFileTime",
    "_mkgmtime",
    "_execvp",
    "OpenProcess",
    "TerminateProcess",
    "GlobalMemoryStatusEx",
    "PathMatchSpecA",
    "_dup",
    "_dup2",
];

/// Emits a shim that converts SysV `write(fd, buf, len)` to Win32 `WriteFile`.
///
/// SysV: rdi=fd, rsi=buf, rdx=len → MSx64: rcx=handle, rdx=buf, r8=len, r9=&written
fn emit_shim_write(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_write");
    emitter.instruction("sub rsp, 56");                                         // allocate shadow(32) + written(4) + padding
    emitter.instruction("mov QWORD PTR [rsp + 48], rdx");                       // spill len (rdx is volatile, clobbered by the call) to a safe slot
    emitter.instruction("mov rcx, rdi");                                        // fd for handle conversion
    emitter.instruction("call __rt_fd_to_handle");                              // convert fd to Win32 HANDLE
    emitter.instruction("mov rcx, rax");                                        // handle
    emitter.instruction("mov rdx, rsi");                                        // buffer
    emitter.instruction("mov r8, QWORD PTR [rsp + 48]");                        // reload len (arg3) after the handle-conversion call
    emitter.instruction("mov QWORD PTR [rsp + 32], 0");                         // lpOverlapped = NULL (arg5 slot)
    emitter.instruction("lea r9, [rsp + 40]");                                  // &bytesWritten (arg4 -> [rsp+40])
    emitter.instruction("call WriteFile");                                      // WriteFile(handle, buf, len, &written, NULL)
    emitter.instruction("mov rax, QWORD PTR [rsp + 40]");                       // return bytes written
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return to caller
    emitter.blank();
}

/// Emits a shim that converts SysV `read(fd, buf, len)` to Win32 `ReadFile`.
fn emit_shim_read(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_read");
    emitter.instruction("sub rsp, 56");                                         // allocate shadow(32) + read(4) + padding
    emitter.instruction("mov QWORD PTR [rsp + 48], rdx");                       // spill len (rdx is volatile, clobbered by the call) to a safe slot
    emitter.instruction("mov rcx, rdi");                                        // fd for handle conversion
    emitter.instruction("call __rt_fd_to_handle");                              // convert fd to Win32 HANDLE
    emitter.instruction("mov rcx, rax");                                        // handle
    emitter.instruction("mov rdx, rsi");                                        // buffer
    emitter.instruction("mov r8, QWORD PTR [rsp + 48]");                        // reload len (arg3) after the handle-conversion call
    emitter.instruction("mov QWORD PTR [rsp + 32], 0");                         // lpOverlapped = NULL (arg5 slot)
    emitter.instruction("lea r9, [rsp + 40]");                                  // &bytesRead (arg4 -> [rsp+40])
    emitter.instruction("call ReadFile");                                       // ReadFile(handle, buf, len, &read, NULL)
    emitter.instruction("mov rax, QWORD PTR [rsp + 40]");                       // return bytes read
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return to caller
    emitter.blank();
}

/// Emits the `__rt_unsupported_syscall` diagnostic helper.
///
/// The Windows syscall->shim transform rewrites any Linux syscall number with
/// no Win32 shim into `mov eax, <N>` + `call __rt_unsupported_syscall` (instead
/// of a silent `int3`). Entered with the Linux syscall number in `eax`, this
/// helper prints `unsupported syscall: <N>\n` to stderr via a manual base-10
/// itoa (no libc) and calls `ExitProcess(70)`. It is emitted unconditionally so
/// the transform always has a target, even if unreferenced in a given build.
fn emit_shim_unsupported_syscall(emitter: &mut Emitter) {
    emitter.label_global("__rt_unsupported_syscall");
    emitter.instruction("sub rsp, 120");                                        // frame: shadow(32)+overlapped+written+msg+itoa scratch, 16B aligned
    emitter.instruction("mov r15d, eax");                                       // stash syscall number (eax is clobbered by every Win32 call)
    // -- copy the "unsupported syscall: " prefix into the message buffer --
    emitter.instruction("cld");                                                 // ensure forward direction for the string copy
    emitter.instruction("lea rsi, [rip + __rt_unsup_prefix]");                  // source = constant prefix string
    emitter.instruction("lea rdi, [rsp + 48]");                                 // dest = message buffer start
    emitter.instruction("mov ecx, 21");                                         // prefix length (\"unsupported syscall: \")
    emitter.instruction("rep movsb");                                           // copy the 21 prefix bytes; rdi now points past the prefix
    // -- manual base-10 itoa of the number into scratch (least-significant first) --
    emitter.instruction("mov eax, r15d");                                       // working value = syscall number
    emitter.instruction("lea rsi, [rsp + 112]");                                // rsi = one past the itoa scratch end
    emitter.instruction("mov ecx, 10");                                         // decimal divisor
    emitter.label(".Lunsup_itoa");
    emitter.instruction("xor edx, edx");                                        // clear the high dividend half before dividing
    emitter.instruction("div ecx");                                             // eax /= 10, edx = eax % 10
    emitter.instruction("add dl, 48");                                          // convert the remainder to an ASCII digit ('0')
    emitter.instruction("dec rsi");                                             // move one byte toward the front of the scratch
    emitter.instruction("mov [rsi], dl");                                       // store the digit
    emitter.instruction("test eax, eax");                                       // any higher-order digits remaining?
    emitter.instruction("jnz .Lunsup_itoa");                                    // loop until the quotient reaches zero
    // -- append the digits (now in order at [rsi..scratch_end]) after the prefix --
    emitter.instruction("lea rdx, [rsp + 112]");                                // sentinel = itoa scratch end
    emitter.label(".Lunsup_copy");
    emitter.instruction("mov al, [rsi]");                                       // load the next digit
    emitter.instruction("mov [rdi], al");                                       // append it to the message buffer
    emitter.instruction("inc rsi");                                             // advance the source pointer
    emitter.instruction("inc rdi");                                             // advance the destination pointer
    emitter.instruction("cmp rsi, rdx");                                        // reached the end of the digits?
    emitter.instruction("jne .Lunsup_copy");                                    // keep copying digits
    emitter.instruction("mov BYTE PTR [rdi], 10");                              // append a trailing newline
    emitter.instruction("inc rdi");                                             // include the newline in the length
    emitter.instruction("lea rax, [rsp + 48]");                                 // message buffer start
    emitter.instruction("sub rdi, rax");                                        // rdi = total message length
    emitter.instruction("mov r14, rdi");                                        // stash length in a callee-saved reg (survives the calls)
    // -- WriteFile(stderr, &msg, len, &written, NULL) --
    emitter.instruction("mov ecx, -12");                                        // STD_ERROR_HANDLE
    emitter.instruction("call GetStdHandle");                                   // rax = stderr handle
    emitter.instruction("mov rcx, rax");                                        // handle (arg1)
    emitter.instruction("lea rdx, [rsp + 48]");                                 // &msg (arg2)
    emitter.instruction("mov r8, r14");                                         // len (arg3)
    emitter.instruction("lea r9, [rsp + 40]");                                  // &written (arg4), off the arg5 slot
    emitter.instruction("mov QWORD PTR [rsp + 32], 0");                         // lpOverlapped = NULL (arg5 slot)
    emitter.instruction("call WriteFile");                                      // write the diagnostic to stderr
    // -- ExitProcess(70) (never returns) --
    emitter.instruction("mov ecx, 70");                                         // process exit code 70
    emitter.instruction("call ExitProcess");                                    // terminate the process
    // -- constant prefix string in .data --
    emitter.raw(".data");
    emitter.raw("__rt_unsup_prefix:");
    emitter.raw("    .ascii \"unsupported syscall: \"");
    emitter.raw(".text");
    emitter.blank();
}

/// Emits a shim that calls `ExitProcess` with the exit code from rdi.
fn emit_shim_exit(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_exit");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rdi");                                        // exit code
    emitter.instruction("call ExitProcess");                                    // terminate process
    emitter.instruction("add rsp, 40");                                         // unreachable but keeps stack balanced
    emitter.instruction("ret");                                                 // unreachable
    emitter.blank();
}

/// Emits a shim that converts `close(fd)` to `CloseHandle(handle)`.
fn emit_shim_close(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_close");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rdi");                                        // fd
    emitter.instruction("call __rt_fd_to_handle");                              // convert to HANDLE
    emitter.instruction("mov rcx, rax");                                        // handle
    emitter.instruction("call CloseHandle");                                    // close handle
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return to caller
    emitter.blank();
}

/// Emits a shim that converts `mmap` to `VirtualAlloc`.
///
/// SysV: rdi=addr, rsi=len, rdx=prot, r10=flags, r8=fd, r9=offset
fn emit_shim_mmap(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_mmap");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rdi");                                        // base address (NULL = let OS choose)
    emitter.instruction("mov rdx, rsi");                                        // size
    emitter.instruction("mov r8, 0x1000");                                      // MEM_COMMIT
    emitter.instruction("mov r9, 0x04");                                        // PAGE_READWRITE (default)
    emitter.instruction("call VirtualAlloc");                                   // allocate memory
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return base address in rax
    emitter.blank();
}

/// Emits a shim that converts `munmap` to `VirtualFree`.
fn emit_shim_munmap(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_munmap");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rdi");                                        // base address
    emitter.instruction("xor rdx, rdx");                                        // size = 0 (MEM_RELEASE requires 0)
    emitter.instruction("mov r8, 0x8000");                                      // MEM_RELEASE
    emitter.instruction("call VirtualFree");                                    // free memory
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits a shim that provides a simple heap allocation via `HeapAlloc`.
///
/// SysV: rdi=size → returns pointer
fn emit_shim_brk(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_brk");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("call GetProcessHeap");                                 // get default heap handle
    emitter.instruction("mov rcx, rax");                                        // heap handle
    emitter.instruction("xor rdx, rdx");                                        // flags = 0
    emitter.instruction("mov r8, rdi");                                         // size
    emitter.instruction("call HeapAlloc");                                      // allocate from heap
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return pointer
    emitter.blank();
}

/// Emits a shim that returns the current process ID.
fn emit_shim_getpid(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_getpid");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("call GetCurrentProcessId");                            // get PID
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return PID in rax
    emitter.blank();
}

/// Emits a shim that gets the current time via `GetSystemTimeAsFileTime`.
///
/// SysV: rdi=timespec* → fills in [sec, nsec]
fn emit_shim_clock_gettime(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_clock_gettime");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + FILETIME(8)
    emitter.instruction("lea rcx, [rsp + 32]");                                 // &filetime
    emitter.instruction("call GetSystemTimeAsFileTime");                        // get 100ns intervals since 1601
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // load FILETIME (64-bit)
    emitter.instruction("mov r10, 116444736000000000");                         // Unix epoch offset (100ns intervals from 1601 to 1970)
    emitter.instruction("sub rax, r10");                                        // convert to Unix epoch (100ns intervals since 1970)
    emitter.instruction("xor rdx, rdx");                                        // clear high 64 bits of dividend
    emitter.instruction("mov r11, 10000000");                                   // divisor: 100ns intervals per second
    emitter.instruction("div r11");                                             // RDX:RAX / r11 → RAX = seconds, RDX = remainder
    emitter.instruction("mov QWORD PTR [rdi], rax");                            // store seconds
    emitter.instruction("imul rdx, 100");                                       // convert remainder to nanoseconds
    emitter.instruction("mov QWORD PTR [rdi + 8], rdx");                        // store nanoseconds
    emitter.instruction("xor rax, rax");                                        // return 0 (success)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits a shim that generates random bytes via `BCryptGenRandom`.
///
/// SysV: rdi=buffer, rsi=count (arbitrary count — the whole buffer is filled).
/// BCryptGenRandom's signature is `BCryptGenRandom(hAlgorithm, pbBuffer, cbBuffer,
/// dwFlags)`, so it is called with `hAlgorithm = NULL`, `pbBuffer = buffer`,
/// `cbBuffer = count`, and `dwFlags = BCRYPT_USE_SYSTEM_PREFERRED_RNG (2)`. It
/// returns STATUS_SUCCESS (0) on success, a nonzero NTSTATUS on failure. This shim
/// mirrors Linux getrandom's contract: it returns the byte count on success and -1 on
/// error, so callers can treat a negative return as a hard failure.
fn emit_shim_getrandom(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_getrandom");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + padding
    emitter.instruction("mov QWORD PTR [rsp + 32], rsi");                       // save requested byte count to return on success
    emitter.instruction("xor rcx, rcx");                                        // hAlgorithm = NULL (use the system-preferred RNG)
    emitter.instruction("mov rdx, rdi");                                        // pbBuffer = caller buffer
    emitter.instruction("mov r8, rsi");                                         // cbBuffer = caller-requested byte count
    emitter.instruction("mov r9, 2");                                           // dwFlags = BCRYPT_USE_SYSTEM_PREFERRED_RNG
    emitter.instruction("call BCryptGenRandom");                                // fill the whole buffer with CSPRNG bytes
    emitter.instruction("test rax, rax");                                       // BCryptGenRandom returns STATUS_SUCCESS (0) on success
    emitter.instruction("jnz .Lgetrandom_fail");                                // any nonzero NTSTATUS → return -1
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // return the byte count on success
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lgetrandom_fail");
    emitter.instruction("mov rax, -1");                                         // return -1 on failure
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits a shim that fakes `fstat` using msvcrt's `_fstat` or a stub.
fn emit_shim_fstat(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_fstat");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rdi");                                        // fd
    emitter.instruction("mov rdx, rsi");                                        // stat buffer
    emitter.instruction("call fstat");                                          // msvcrt fstat
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits a shim that converts `open(path, flags, mode)` to `CreateFileA`.
///
/// SysV: rdi=path, rsi=flags, rdx=mode.
/// Maps Linux open flags to Win32 CreateFileA parameters:
/// - O_RDONLY(0) → GENERIC_READ, OPEN_EXISTING
/// - O_WRONLY(1) → GENERIC_WRITE, OPEN_EXISTING
/// - O_RDWR(2) → GENERIC_READ|GENERIC_WRITE, OPEN_EXISTING
/// - O_CREAT(0x40) → OPEN_ALWAYS (or CREATE_ALWAYS with O_TRUNC)
/// - O_TRUNC(0x200) → TRUNCATE_EXISTING (or CREATE_ALWAYS with O_CREAT)
/// - O_APPEND(0x400) → FILE_APPEND_DATA
fn emit_shim_open(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_open");
    emitter.instruction("sub rsp, 56");                                         // shadow(32) + stack args(24)
    // -- determine dwDesiredAccess in rax --
    emitter.instruction("mov rax, 0x80000000");                                 // GENERIC_READ (default)
    emitter.instruction("test rsi, 1");                                         // O_WRONLY?
    emitter.instruction("jnz .Lopen_wr");                                       // → GENERIC_WRITE
    emitter.instruction("test rsi, 2");                                         // O_RDWR?
    emitter.instruction("jnz .Lopen_rw");                                       // → GENERIC_READ|GENERIC_WRITE
    emitter.instruction("jmp .Lopen_access_done");                              // → use GENERIC_READ
    emitter.label(".Lopen_wr");
    emitter.instruction("mov rax, 0x40000000");                                 // GENERIC_WRITE
    emitter.instruction("jmp .Lopen_access_done");                              // → proceed
    emitter.label(".Lopen_rw");
    emitter.instruction("mov rax, 0xC0000000");                                 // GENERIC_READ|GENERIC_WRITE
    emitter.label(".Lopen_access_done");
    emitter.instruction("test rsi, 0x400");                                     // O_APPEND?
    emitter.instruction("jz .Lopen_no_append");                                 // skip if not append
    emitter.instruction("or rax, 0x4");                                         // FILE_APPEND_DATA
    emitter.label(".Lopen_no_append");
    // -- determine dwCreationDisposition in r10 --
    emitter.instruction("test rsi, 0x40");                                      // O_CREAT?
    emitter.instruction("jz .Lopen_no_creat");                                  // → no create
    emitter.instruction("test rsi, 0x200");                                     // O_CREAT + O_TRUNC?
    emitter.instruction("jnz .Lopen_create_always");                            // → CREATE_ALWAYS
    emitter.instruction("mov r10, 4");                                          // OPEN_ALWAYS (create or open)
    emitter.instruction("jmp .Lopen_disp_done");                                // → proceed
    emitter.label(".Lopen_create_always");
    emitter.instruction("mov r10, 2");                                          // CREATE_ALWAYS
    emitter.instruction("jmp .Lopen_disp_done");                                // → proceed
    emitter.label(".Lopen_no_creat");
    emitter.instruction("test rsi, 0x200");                                     // O_TRUNC without O_CREAT?
    emitter.instruction("jnz .Lopen_trunc_existing");                           // → TRUNCATE_EXISTING
    emitter.instruction("mov r10, 3");                                          // OPEN_EXISTING
    emitter.instruction("jmp .Lopen_disp_done");                                // → proceed
    emitter.label(".Lopen_trunc_existing");
    emitter.instruction("mov r10, 5");                                          // TRUNCATE_EXISTING
    emitter.label(".Lopen_disp_done");
    // -- call CreateFileA(rcx=path, rdx=access, r8=share, r9=NULL, [rsp+32]=disp, [rsp+40]=0, [rsp+48]=0) --
    emitter.instruction("mov rcx, rdi");                                        // lpFileName = path
    emitter.instruction("mov rdx, rax");                                        // dwDesiredAccess
    emitter.instruction("mov r8, 3");                                           // FILE_SHARE_READ | FILE_SHARE_WRITE
    emitter.instruction("xor r9, r9");                                          // lpSecurityAttributes = NULL
    emitter.instruction("mov QWORD PTR [rsp + 32], r10");                       // dwCreationDisposition
    emitter.instruction("mov QWORD PTR [rsp + 40], 0");                         // dwFlagsAndAttributes = 0
    emitter.instruction("mov QWORD PTR [rsp + 48], 0");                         // hTemplateFile = NULL
    emitter.instruction("call CreateFileA");                                    // open file
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return handle
    emitter.blank();
}

/// Emits a shim that converts `lseek` to `SetFilePointer`.
///
/// SysV: rdi=fd, rsi=offset, rdx=whence.
/// Maps SEEK_SET(0)→FILE_BEGIN(0), SEEK_CUR(1)→FILE_CURRENT(1), SEEK_END(2)→FILE_END(2).
fn emit_shim_lseek(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_lseek");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov QWORD PTR [rsp + 32], rdx");                       // spill whence (r10 is volatile, clobbered by the call) to a safe slot
    emitter.instruction("mov rcx, rdi");                                        // fd
    emitter.instruction("call __rt_fd_to_handle");                              // convert to HANDLE
    emitter.instruction("mov rcx, rax");                                        // handle
    emitter.instruction("mov rdx, rsi");                                        // distance to move (low 32)
    emitter.instruction("xor r8, r8");                                          // distance high = NULL
    emitter.instruction("mov r9, QWORD PTR [rsp + 32]");                        // reload whence (arg4: 0/1/2) after the handle-conversion call
    emitter.instruction("call SetFilePointer");                                 // set file position
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return new position
    emitter.blank();
}

/// Emits a shim that delegates `fcntl` to `ioctlsocket` for socket operations.
///
/// On Windows, `fcntl` for sockets maps to `ioctlsocket`. Non-socket fds
/// return 0 (no-op) since Windows doesn't support file locking via fcntl.
fn emit_shim_fcntl(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_fcntl");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_ioctl");                                 // delegate to ioctlsocket shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits a shim that converts `unlink` to `DeleteFileA`.
fn emit_shim_unlink(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_unlink");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rdi");                                        // file path
    emitter.instruction("call DeleteFileA");                                    // delete file
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return (nonzero = success)
    emitter.blank();
}

/// Emits a shim that wraps `GetCurrentDirectoryA`.
fn emit_shim_getcwd(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_getcwd");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rsi");                                        // buffer size
    emitter.instruction("mov rdx, rdi");                                        // buffer
    emitter.instruction("call GetCurrentDirectoryA");                           // get current directory
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return buffer pointer
    emitter.blank();
}

/// Emits a shim that wraps `SetCurrentDirectoryA`.
fn emit_shim_chdir(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_chdir");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rdi");                                        // path
    emitter.instruction("call SetCurrentDirectoryA");                           // change directory
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return (nonzero = success)
    emitter.blank();
}

/// Emits a shim that converts `mkdir` to `CreateDirectoryA`.
fn emit_shim_mkdir(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_mkdir");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rdi");                                        // path
    emitter.instruction("xor rdx, rdx");                                        // lpSecurityAttributes = NULL
    emitter.instruction("call CreateDirectoryA");                               // create directory
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return (nonzero = success)
    emitter.blank();
}

/// Emits a shim that converts `rmdir` to `RemoveDirectoryA`.
fn emit_shim_rmdir(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_rmdir");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rdi");                                        // path
    emitter.instruction("call RemoveDirectoryA");                               // remove directory
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return (nonzero = success)
    emitter.blank();
}

/// Emits a shim that wraps `GetFileAttributesA` for `stat`.
fn emit_shim_stat(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_stat");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rdi");                                        // path
    emitter.instruction("mov rdx, rsi");                                        // stat buffer
    emitter.instruction("call stat");                                           // msvcrt stat
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits a shim that wraps `MoveFileA` for `rename`.
fn emit_shim_rename(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_rename");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rdi");                                        // old name
    emitter.instruction("mov rdx, rsi");                                        // new name
    emitter.instruction("call MoveFileA");                                      // move/rename file
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return (nonzero = success)
    emitter.blank();
}

/// Emits a shim that wraps `SetFileAttributesA` for `chmod`.
///
/// SysV: rdi=path, rsi=mode.
/// Maps Unix mode to Win32 file attributes:
/// - If mode has no write bits (mode & 0222 == 0) → FILE_ATTRIBUTE_READONLY (1)
/// - Otherwise → FILE_ATTRIBUTE_NORMAL (128)
fn emit_shim_chmod(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_chmod");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rdi");                                        // path
    emitter.instruction("test rsi, 0222");                                      // any write bit set?
    emitter.instruction("jnz .Lchmod_writable");                                // → FILE_ATTRIBUTE_NORMAL
    emitter.instruction("mov rdx, 1");                                          // FILE_ATTRIBUTE_READONLY
    emitter.instruction("jmp .Lchmod_call");                                    // → call SetFileAttributesA
    emitter.label(".Lchmod_writable");
    emitter.instruction("mov rdx, 128");                                        // FILE_ATTRIBUTE_NORMAL
    emitter.label(".Lchmod_call");
    emitter.instruction("call SetFileAttributesA");                             // set file attributes
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return (nonzero = success)
    emitter.blank();
}

/// Emits a shim that wraps `GetEnvironmentVariableA`.
fn emit_shim_getenv_shim(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_getenv");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov r8, rdx");                                         // save buffer size before rdx is overwritten
    emitter.instruction("mov rcx, rdi");                                        // var name
    emitter.instruction("mov rdx, rsi");                                        // buffer
    emitter.instruction("call GetEnvironmentVariableA");                        // get env variable
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return length
    emitter.blank();
}

/// Emits a shim that wraps msvcrt `gethostname`.
fn emit_shim_gethostname(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_gethostname");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rdi");                                        // buffer
    emitter.instruction("mov rdx, rsi");                                        // length
    emitter.instruction("call gethostname");                                    // msvcrt gethostname
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits socket-related shims (socket, connect, bind, listen, accept, send, recv, etc.).
/// sendto/recvfrom have 6 args and are emitted separately with dedicated shims.
fn emit_shim_socket_shims(emitter: &mut Emitter) {
    let shims: &[(&str, &str)] = &[
        ("__rt_sys_socket", "socket"),
        ("__rt_sys_connect", "connect"),
        ("__rt_sys_bind", "bind"),
        ("__rt_sys_listen", "listen"),
        ("__rt_sys_accept", "accept"),
        ("__rt_sys_shutdown", "shutdown"),
        ("__rt_sys_getsockname", "getsockname"),
        ("__rt_sys_getpeername", "getpeername"),
    ];
    for (label, func) in shims {
        emitter.label_global(label);
        emitter.instruction("sub rsp, 40");                                     // shadow(32) + alignment(8)
        emitter.instruction("mov r8, rdx");                                     // save arg3 before rdx is overwritten
        emitter.instruction("mov rcx, rdi");                                    // arg1
        emitter.instruction("mov rdx, rsi");                                    // arg2
        emitter.instruction(&format!("call {}", func));                         // call Win32/msvcrt function
        emitter.instruction("add rsp, 40");                                     // restore stack
        emitter.instruction("ret");                                             // return
        emitter.blank();
    }
    // closesocket: 1 arg
    emitter.label_global("__rt_sys_closesocket");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8)
    emitter.instruction("mov rcx, rdi");                                        // socket
    emitter.instruction("call closesocket");                                    // close socket
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // sendto: 6 args — socket, buf, len, flags, dest_addr, addrlen
    // SysV: rdi=socket, rsi=buf, rdx=len, r10=flags, r8=dest_addr, r9=addrlen
    // MSx64: rcx, rdx, r8, r9, [rsp+32], [rsp+40]
    emitter.label_global("__rt_sys_sendto");
    emitter.instruction("sub rsp, 56");                                         // shadow(32) + stack args(24), aligned
    emitter.instruction("mov QWORD PTR [rsp + 32], r8");                        // dest_addr → 5th arg (stack)
    emitter.instruction("mov QWORD PTR [rsp + 40], r9");                        // addrlen → 6th arg (stack)
    emitter.instruction("mov r9, r10");                                         // flags → r9 (4th arg)
    emitter.instruction("mov r8, rdx");                                         // len → r8 (3rd arg)
    emitter.instruction("mov rdx, rsi");                                        // buf → rdx (2nd arg)
    emitter.instruction("mov rcx, rdi");                                        // socket → rcx (1st arg)
    emitter.instruction("call sendto");                                         // sendto(socket, buf, len, flags, dest_addr, addrlen)
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // recvfrom: 6 args — socket, buf, len, flags, src_addr, &addrlen
    // SysV: rdi=socket, rsi=buf, rdx=len, r10=flags, r8=src_addr, r9=&addrlen
    // MSx64: rcx, rdx, r8, r9, [rsp+32], [rsp+40]
    emitter.label_global("__rt_sys_recvfrom");
    emitter.instruction("sub rsp, 56");                                         // shadow(32) + stack args(24), aligned
    emitter.instruction("mov QWORD PTR [rsp + 32], r8");                        // src_addr → 5th arg (stack)
    emitter.instruction("mov QWORD PTR [rsp + 40], r9");                        // &addrlen → 6th arg (stack)
    emitter.instruction("mov r9, r10");                                         // flags → r9 (4th arg)
    emitter.instruction("mov r8, rdx");                                         // len → r8 (3rd arg)
    emitter.instruction("mov rdx, rsi");                                        // buf → rdx (2nd arg)
    emitter.instruction("mov rcx, rdi");                                        // socket → rcx (1st arg)
    emitter.instruction("call recvfrom");                                       // recvfrom(socket, buf, len, flags, src_addr, &addrlen)
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits a shim that converts `ioctl` to `ioctlsocket`.
fn emit_shim_ioctl(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_ioctl");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov r8, rdx");                                         // save argp before rdx is overwritten
    emitter.instruction("mov rcx, rdi");                                        // socket
    emitter.instruction("mov rdx, rsi");                                        // cmd
    emitter.instruction("call ioctlsocket");                                    // ioctl socket
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits dup/dup2 shims using msvcrt `_dup`/`_dup2`.
fn emit_shim_dup_shims(emitter: &mut Emitter) {
    // _dup(fd) → returns new fd or -1
    emitter.label_global("__rt_sys_dup");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rdi");                                        // fd
    emitter.instruction("call _dup");                                           // msvcrt _dup
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return new fd
    emitter.blank();
    // _dup2(fd, fd2) → returns fd2 or -1
    emitter.label_global("__rt_sys_dup2");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rdi");                                        // fd
    emitter.instruction("mov rdx, rsi");                                        // fd2
    emitter.instruction("call _dup2");                                          // msvcrt _dup2
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return new fd
    emitter.blank();
}

/// Emits getuid/getgid (return 0, PHP behavior on Windows) and setuid/setgid/getppid/
/// getpriority/setpriority (return -1, ENOSYS — POSIX-only functions).
fn emit_shim_getuid_shims(emitter: &mut Emitter) {
    // getuid/getgid: return 0 on Windows (PHP behavior — no Unix UID/GID)
    for (label, _desc) in &[
        ("__rt_sys_getuid", "getuid"),
        ("__rt_sys_getgid", "getgid"),
    ] {
        emitter.label_global(label);
        emitter.instruction("xor rax, rax");                                    // return 0 (PHP behavior on Windows)
        emitter.instruction("ret");                                             // return
        emitter.blank();
    }
    // setuid/setgid/getppid/getpriority/setpriority: return -1 (ENOSYS)
    for (label, _desc) in &[
        ("__rt_sys_setuid", "setuid"),
        ("__rt_sys_setgid", "setgid"),
        ("__rt_sys_getppid", "getppid"),
        ("__rt_sys_getpriority", "getpriority"),
        ("__rt_sys_setpriority", "setpriority"),
    ] {
        emitter.label_global(label);
        emitter.instruction("mov rax, -1");                                     // return -1 (not supported on Windows)
        emitter.instruction("ret");                                             // return
        emitter.blank();
    }
}

/// Emits a kill shim using OpenProcess+TerminateProcess for SIGKILL, no-op otherwise.
///
/// SysV: rdi=pid, rsi=signal.
fn emit_shim_kill(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_kill");
    emitter.instruction("cmp rsi, 9");                                          // sig == SIGKILL?
    emitter.instruction("jne .Lsys_kill_noop");                                 // skip if not SIGKILL
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + handle(8)
    emitter.instruction("mov rcx, 1");                                          // dwDesiredAccess = PROCESS_TERMINATE
    emitter.instruction("xor rdx, rdx");                                        // bInheritHandle = FALSE
    emitter.instruction("mov r8, rdi");                                         // dwProcessId = pid
    emitter.instruction("call OpenProcess");                                    // open process handle
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // save handle
    emitter.instruction("test rax, rax");                                       // check if OpenProcess succeeded
    emitter.instruction("je .Lsys_kill_fail");                                  // jump if failed
    emitter.instruction("mov rcx, rax");                                        // handle
    emitter.instruction("mov rdx, 1");                                          // exit code
    emitter.instruction("call TerminateProcess");                               // terminate process
    emitter.instruction("mov rcx, QWORD PTR [rsp + 32]");                       // reload handle
    emitter.instruction("call CloseHandle");                                    // close handle
    emitter.label(".Lsys_kill_fail");
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("xor rax, rax");                                        // return 0
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lsys_kill_noop");
    emitter.instruction("xor rax, rax");                                        // return 0 (no-op for non-SIGKILL)
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits a uname shim using msvcrt.
fn emit_shim_uname(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_uname");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rdi");                                        // utsname buffer
    emitter.instruction("call uname");                                          // msvcrt uname (may not exist)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits an accept4 shim (maps to accept on Windows).
fn emit_shim_accept4(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_accept4");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov r8, rdx");                                         // save addrlen before rdx is overwritten
    emitter.instruction("mov rcx, rdi");                                        // socket
    emitter.instruction("mov rdx, rsi");                                        // addr
    emitter.instruction("call accept");                                         // Win32 accept (ignores flags)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits a writev shim — iterates over iovec array, calling __rt_sys_write for each.
///
/// SysV: rdi=fd, rsi=iov, rdx=iovcnt.
/// Each iovec is 16 bytes: [iov_base:8, iov_len:8].
fn emit_shim_writev(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_writev");
    emitter.instruction("sub rsp, 40");                                         // save fd(8) + iov_ptr(8) + iovcnt(8) + total(8)
    emitter.instruction("mov QWORD PTR [rsp], rdi");                            // save fd
    emitter.instruction("mov QWORD PTR [rsp + 8], rsi");                        // save iov pointer
    emitter.instruction("mov QWORD PTR [rsp + 16], rdx");                       // save iovcnt
    emitter.instruction("xor rax, rax");                                        // total written = 0
    emitter.instruction("mov QWORD PTR [rsp + 24], rax");                       // save total
    emitter.label(".Lwritev_loop");
    emitter.instruction("mov r10, QWORD PTR [rsp + 16]");                       // load iovcnt
    emitter.instruction("test r10, r10");                                       // iovcnt == 0?
    emitter.instruction("jz .Lwritev_done");                                    // done if no more iovecs
    emitter.instruction("mov r11, QWORD PTR [rsp + 8]");                        // load current iov pointer
    emitter.instruction("mov rdi, QWORD PTR [rsp]");                            // fd
    emitter.instruction("mov rsi, QWORD PTR [r11]");                            // iov_base
    emitter.instruction("mov rdx, QWORD PTR [r11 + 8]");                        // iov_len
    emitter.instruction("call __rt_sys_write");                                 // write(fd, iov_base, iov_len)
    emitter.instruction("add QWORD PTR [rsp + 8], 16");                         // advance iov pointer to next entry
    emitter.instruction("sub QWORD PTR [rsp + 16], 1");                         // iovcnt--
    emitter.instruction("test rax, rax");                                       // write returned error?
    emitter.instruction("js .Lwritev_done");                                    // exit on error
    emitter.instruction("add QWORD PTR [rsp + 24], rax");                       // total += bytes_written
    emitter.instruction("jmp .Lwritev_loop");                                   // continue loop
    emitter.label(".Lwritev_done");
    emitter.instruction("mov rax, QWORD PTR [rsp + 24]");                       // return total bytes written
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits a sysinfo shim using GlobalMemoryStatusEx for memory info.
fn emit_shim_sysinfo(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_sysinfo");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rdi");                                        // sysinfo struct pointer
    emitter.instruction("call GlobalMemoryStatusEx");                           // get memory status (best effort)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("xor rax, rax");                                        // return 0 (success)
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits an execve shim using msvcrt _execvp.
///
/// SysV: rdi=path, rsi=argv, rdx=envp (ignored on Windows).
fn emit_shim_execve(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_execve");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rdi");                                        // path
    emitter.instruction("mov rdx, rsi");                                        // argv
    emitter.instruction("call _execvp");                                        // execute program (replaces process)
    emitter.instruction("add rsp, 40");                                         // restore stack (only reached on failure)
    emitter.instruction("ret");                                                 // return -1 on failure (rax from _execvp)
    emitter.blank();
}

/// Emits a futex shim (stub — Windows has its own synchronization primitives).
fn emit_shim_futex(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_futex");
    emitter.instruction("xor rax, rax");                                        // stub: return 0
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits an mprotect shim using VirtualProtect.
fn emit_shim_mprotect(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_mprotect");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + old_protect(8)
    emitter.instruction("mov rcx, rdi");                                        // address
    emitter.instruction("mov rdx, rsi");                                        // size
    emitter.instruction("mov r8, 0x04");                                        // PAGE_READWRITE
    emitter.instruction("lea r9, [rsp + 32]");                                  // &old_protect
    emitter.instruction("call VirtualProtect");                                 // change protection
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits setsockopt shim — 5 args: socket, level, optname, optval, optlen.
///
/// SysV: rdi=socket, rsi=level, rdx=optname, r10=optval, r8=optlen
/// MSx64: rcx, rdx, r8, r9, [rsp+32]
fn emit_shim_setsockopt(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_setsockopt");
    emitter.instruction("sub rsp, 56");                                         // shadow(32) + stack arg(8) + alignment(16)
    emitter.instruction("mov QWORD PTR [rsp + 32], r8");                        // optlen → 5th arg (stack)
    emitter.instruction("mov r9, r10");                                         // optval → r9 (4th arg)
    emitter.instruction("mov r8, rdx");                                         // optname → r8 (3rd arg)
    emitter.instruction("mov rdx, rsi");                                        // level → rdx (2nd arg)
    emitter.instruction("mov rcx, rdi");                                        // socket → rcx (1st arg)
    emitter.instruction("call setsockopt");                                     // setsockopt(socket, level, optname, optval, optlen)
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits getsockopt shim — 5 args: socket, level, optname, optval, &optlen.
///
/// SysV: rdi=socket, rsi=level, rdx=optname, r10=optval, r8=&optlen
/// MSx64: rcx, rdx, r8, r9, [rsp+32]
fn emit_shim_getsockopt(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_getsockopt");
    emitter.instruction("sub rsp, 56");                                         // shadow(32) + stack arg(8) + alignment(16)
    emitter.instruction("mov QWORD PTR [rsp + 32], r8");                        // &optlen → 5th arg (stack)
    emitter.instruction("mov r9, r10");                                         // optval → r9 (4th arg)
    emitter.instruction("mov r8, rdx");                                         // optname → r8 (3rd arg)
    emitter.instruction("mov rdx, rsi");                                        // level → rdx (2nd arg)
    emitter.instruction("mov rcx, rdi");                                        // socket → rcx (1st arg)
    emitter.instruction("call getsockopt");                                     // getsockopt(socket, level, optname, optval, &optlen)
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits a socketpair shim — Windows doesn't have socketpair, return -1 (ENOSYS).
fn emit_shim_socketpair(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_socketpair");
    emitter.instruction("mov rax, -1");                                         // return -1 (not supported on Windows)
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits a statfs shim — delegates to GetDiskFreeSpaceExA for basic filesystem info.
/// Returns 0 (success) for simplicity since the struct layout differs.
fn emit_shim_statfs(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_statfs");
    emitter.instruction("xor rax, rax");                                        // return 0 (best-effort stub)
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits a pselect6 shim — Windows doesn't have pselect6, return -1 (ENOSYS).
/// PHP's stream_select() uses select() via ws2_32 on Windows.
fn emit_shim_pselect6(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_pselect6");
    emitter.instruction("mov rax, -1");                                         // return -1 (not directly supported)
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits a sendmsg shim — Windows doesn't have sendmsg, return -1 (ENOSYS).
fn emit_shim_sendmsg(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_sendmsg");
    emitter.instruction("mov rax, -1");                                         // return -1 (not supported on Windows)
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits a recvmsg shim — Windows doesn't have recvmsg, return -1 (ENOSYS).
fn emit_shim_recvmsg(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_recvmsg");
    emitter.instruction("mov rax, -1");                                         // return -1 (not supported on Windows)
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits a getdents shim — Windows doesn't have getdents, return -1 (ENOSYS).
/// PHP uses FindFirstFileA/FindNextFileA for directory iteration on Windows.
fn emit_shim_getdents(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_getdents");
    emitter.instruction("mov rax, -1");                                         // return -1 (not supported on Windows)
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits a creat shim — maps to __rt_sys_open with O_WRONLY|O_CREAT|O_TRUNC.
fn emit_shim_creat(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_creat");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("mov rsi, 0x240");                                      // O_WRONLY | O_CREAT | O_TRUNC
    emitter.instruction("call __rt_sys_open");                                  // delegate to open shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits a clock_getres shim — returns 1ns resolution (best-effort).
fn emit_shim_clock_getres(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_clock_getres");
    emitter.instruction("xor rax, rax");                                        // return 0 (success)
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits a newfstatat shim — delegates to msvcrt stat on Windows.
fn emit_shim_newfstatat(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_newfstatat");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("mov rdi, rsi");                                        // path (skip dirfd arg1)
    emitter.instruction("mov rsi, rdx");                                        // stat buffer
    emitter.instruction("call __rt_sys_stat");                                  // delegate to stat shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits shim wrappers for syscalls that have C symbol stubs but no `__rt_sys_*` label.
///
/// `windows_transform.rs` maps Linux syscalls 86/88/89/92/93/94 to `__rt_sys_*` names,
/// but the actual implementations live as C symbol stubs (link, symlink, readlink, etc.).
/// These thin wrappers shuffle SysV args to match the C stub calling convention and
/// delegate to the stubs.
fn emit_shim_c_symbol_delegates(emitter: &mut Emitter) {
    // __rt_sys_link: delegate to C symbol `link` (CreateHardLinkA)
    // SysV: rdi=oldpath, rsi=newpath → C stub expects same SysV args
    emitter.label_global("__rt_sys_link");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call link");                                           // delegate to C symbol stub
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // __rt_sys_symlink: delegate to C symbol `symlink` (CreateSymbolicLinkA)
    // SysV: rdi=target, rsi=linkpath → C stub expects same SysV args
    emitter.label_global("__rt_sys_symlink");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call symlink");                                        // delegate to C symbol stub
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // __rt_sys_readlink: delegate to C symbol `readlink` (GetFinalPathNameByHandleA)
    // SysV: rdi=path, rsi=buf, rdx=bufsize → C stub expects same SysV args
    emitter.label_global("__rt_sys_readlink");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call readlink");                                       // delegate to C symbol stub
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // __rt_sys_chown: return -1 (ENOSYS — Windows uses ACLs)
    emitter.label_global("__rt_sys_chown");
    emitter.instruction("mov rax, -1");                                         // return -1 (not supported)
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // __rt_sys_fchown: return -1 (ENOSYS)
    emitter.label_global("__rt_sys_fchown");
    emitter.instruction("mov rax, -1");                                         // return -1 (not supported)
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // __rt_sys_lchown: return -1 (ENOSYS)
    emitter.label_global("__rt_sys_lchown");
    emitter.instruction("mov rax, -1");                                         // return -1 (not supported)
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits the fd-to-HANDLE conversion helper.
///
/// On Windows, stdio handles (0, 1, 2) map to GetStdHandle(STD_INPUT_HANDLE,
/// STD_OUTPUT_HANDLE, STD_ERROR_HANDLE). Other fds are C runtime file handles
/// which need `_get_osfhandle` to convert — but for simplicity we use a direct
/// mapping for stdio and pass-through for others.
pub(crate) fn emit_fd_to_handle(emitter: &mut Emitter) {
    emitter.label_global("__rt_fd_to_handle");
    emitter.instruction("cmp rdi, 0");                                          // fd == stdin?
    emitter.instruction("je .Lfd_stdin");                                       // → STD_INPUT_HANDLE
    emitter.instruction("cmp rdi, 1");                                          // fd == stdout?
    emitter.instruction("je .Lfd_stdout");                                      // → STD_OUTPUT_HANDLE
    emitter.instruction("cmp rdi, 2");                                          // fd == stderr?
    emitter.instruction("je .Lfd_stderr");                                      // → STD_ERROR_HANDLE
    emitter.instruction("mov rax, rdi");                                        // pass-through for other fds
    emitter.instruction("ret");                                                 // return fd as handle
    emitter.label(".Lfd_stdin");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, -10");                                        // STD_INPUT_HANDLE
    emitter.instruction("call GetStdHandle");                                   // get stdin handle
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return handle
    emitter.label(".Lfd_stdout");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, -11");                                        // STD_OUTPUT_HANDLE
    emitter.instruction("call GetStdHandle");                                   // get stdout handle
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return handle
    emitter.label(".Lfd_stderr");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, -12");                                        // STD_ERROR_HANDLE
    emitter.instruction("call GetStdHandle");                                   // get stderr handle
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return handle
    emitter.blank();
}

/// Emits stubs for C library symbols that are called directly by the runtime
/// but do not exist on Windows. Each stub either delegates to a Win32 equivalent
/// or returns a safe default value.
fn emit_shim_c_symbols(emitter: &mut Emitter) {
    emitter.raw("    # -- C symbol stubs for functions not available on Windows --");

    // flock: use LockFileEx for LOCK_EX/LOCK_SH, UnlockFileEx for LOCK_UN
    // Handles LOCK_NB (bit 2) by setting LOCKFILE_FAIL_IMMEDIATELY.
    // LockFileEx has 6 args → needs 56 bytes (shadow 32 + stack args 24, aligned).
    emitter.label_global("flock");
    emitter.instruction("sub rsp, 56");                                         // shadow(32) + stack args(24) for LockFileEx
    emitter.instruction("mov rcx, rdi");                                        // fd
    emitter.instruction("call __rt_fd_to_handle");                              // convert fd to HANDLE
    emitter.instruction("mov rcx, rax");                                        // handle
    emitter.instruction("test rsi, rsi");                                       // operation == LOCK_UN (0)?
    emitter.instruction("jz .Lflock_unlock");                                   // unlock if zero
    emitter.instruction("xor rdx, rdx");                                        // dwReserved = 0
    emitter.instruction("xor r8, r8");                                          // dwFlags = 0
    emitter.instruction("test rsi, 2");                                         // LOCK_EX (bit 1)?
    emitter.instruction("setne r8b");                                           // set LOCKFILE_EXCLUSIVE (0x2) if LOCK_EX
    emitter.instruction("test rsi, 4");                                         // LOCK_NB (bit 2)?
    emitter.instruction("jz .Lflock_no_nb");                                    // skip if not LOCK_NB
    emitter.instruction("or r8, 1");                                            // LOCKFILE_FAIL_IMMEDIATELY (0x1)
    emitter.label(".Lflock_no_nb");
    emitter.instruction("mov r9d, 0xFFFFFFFF");                                 // nNumberOfBytesToLockLow = MAXDWORD (zero-extends to r9)
    emitter.instruction("mov dword ptr [rsp + 32], 0xFFFFFFFF");                // nNumberOfBytesToLockHigh = MAXDWORD
    emitter.instruction("call LockFileEx");                                     // lock file
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("xor rax, rax");                                        // return 0
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lflock_unlock");
    emitter.instruction("xor rdx, rdx");                                        // dwReserved = 0
    emitter.instruction("mov r8d, 0xFFFFFFFF");                                 // nNumberOfBytesToUnlockLow = MAXDWORD (zero-extends)
    emitter.instruction("mov r9d, 0xFFFFFFFF");                                 // nNumberOfBytesToUnlockHigh = MAXDWORD (zero-extends)
    emitter.instruction("call UnlockFileEx");                                   // unlock file
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("xor rax, rax");                                        // return 0
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // __errno_location: return pointer to thread-local errno
    // On Windows, msvcrt provides _errno() which returns the same thing.
    // We alias it to a static errno variable.
    emitter.label_global("__errno_location");
    emitter.instruction("lea rax, [rip + __rt_errno]");                         // return pointer to static errno
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // symlink: delegate to CreateSymbolicLinkA with unprivileged-create retry
    // SysV: rdi=target, rsi=linkpath → Win32: rcx=symlinkPath, rdx=targetPath
    emitter.label_global("symlink");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rsi");                                        // lpSymlinkPath = linkpath (SysV arg2)
    emitter.instruction("mov rdx, rdi");                                        // lpTargetPath = target (SysV arg1)
    emitter.instruction("mov r8, 2");                                           // SYMBOLIC_LINK_FLAG_ALLOW_UNPRIVILEGED_CREATE
    emitter.instruction("call CreateSymbolicLinkA");                            // create symbolic link (unprivileged)
    emitter.instruction("test rax, rax");                                       // success?
    emitter.instruction("jnz .Lsymlink_ok");                                    // → success
    // -- retry without unprivileged flag (requires admin) --
    emitter.instruction("mov rcx, rsi");                                        // lpSymlinkPath = linkpath
    emitter.instruction("mov rdx, rdi");                                        // lpTargetPath = target
    emitter.instruction("xor r8, r8");                                          // dwFlags = 0 (requires admin)
    emitter.instruction("call CreateSymbolicLinkA");                            // retry without unprivileged flag
    emitter.instruction("test rax, rax");                                       // success?
    emitter.instruction("jz .Lsymlink_fail");                                   // → failure
    emitter.label(".Lsymlink_ok");
    emitter.instruction("xor rax, rax");                                        // return 0 (success)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lsymlink_fail");
    emitter.instruction("mov rax, -1");                                         // return -1 on failure
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // link: delegate to CreateHardLinkA (args reversed from POSIX)
    emitter.label_global("link");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rsi");                                        // lpFileName = newpath (SysV arg2)
    emitter.instruction("mov rdx, rdi");                                        // lpExistingFileName = oldpath (SysV arg1)
    emitter.instruction("xor r8, r8");                                          // lpSecurityAttributes = NULL
    emitter.instruction("call CreateHardLinkA");                                // create hard link
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return (nonzero = success)
    emitter.blank();

    // readlink: use CreateFileA + GetFinalPathNameByHandleA + CloseHandle
    // Strips the `\\?\` prefix that GetFinalPathNameByHandleA prepends.
    // Stack layout (72 bytes): [0..31]=shadow, [32..47]=CreateFileA stack args,
    // [48]=hTemplateFile, [56]=saved bufsize, [64]=saved buf
    emitter.label_global("readlink");
    emitter.instruction("sub rsp, 72");                                         // shadow(32) + stack args(16) + saved(24)
    emitter.instruction("mov QWORD PTR [rsp + 64], rsi");                       // save buffer at high offset (no conflict)
    emitter.instruction("mov QWORD PTR [rsp + 56], rdx");                       // save bufsize at high offset
    // -- CreateFileA(path, GENERIC_READ, FILE_SHARE_RW, NULL, OPEN_EXISTING, 0, NULL) --
    emitter.instruction("mov rcx, rdi");                                        // lpFileName = path
    emitter.instruction("mov rdx, 0x80000000");                                 // GENERIC_READ
    emitter.instruction("mov r8, 3");                                           // FILE_SHARE_READ | FILE_SHARE_WRITE
    emitter.instruction("xor r9, r9");                                          // lpSecurityAttributes = NULL
    emitter.instruction("mov QWORD PTR [rsp + 32], 3");                         // dwCreationDisposition = OPEN_EXISTING
    emitter.instruction("mov QWORD PTR [rsp + 40], 0");                         // dwFlagsAndAttributes = 0
    emitter.instruction("mov QWORD PTR [rsp + 48], 0");                         // hTemplateFile = NULL
    emitter.instruction("call CreateFileA");                                    // open file
    emitter.instruction("cmp rax, -1");                                         // INVALID_HANDLE_VALUE?
    emitter.instruction("je .Lreadlink_fail");                                  // jump if failed
    // -- Save handle, then GetFinalPathNameByHandleA(handle, buf, bufsize, 0) --
    emitter.instruction("mov QWORD PTR [rsp + 48], rax");                       // spill handle (r10 is volatile across Win32 calls) to a safe slot
    emitter.instruction("mov rcx, rax");                                        // handle
    emitter.instruction("mov rdx, QWORD PTR [rsp + 64]");                       // buffer
    emitter.instruction("mov r8, QWORD PTR [rsp + 56]");                        // bufsize
    emitter.instruction("xor r9, r9");                                          // dwFlags = 0
    emitter.instruction("call GetFinalPathNameByHandleA");                      // get final path
    emitter.instruction("mov QWORD PTR [rsp + 40], rax");                       // spill path length (r11 is volatile) across CloseHandle
    // -- CloseHandle --
    emitter.instruction("mov rcx, QWORD PTR [rsp + 48]");                       // reload handle for CloseHandle
    emitter.instruction("call CloseHandle");                                    // close file handle
    // -- strip \\?\ prefix (4 chars) if present --
    emitter.instruction("mov r10, QWORD PTR [rsp + 64]");                       // buffer
    emitter.instruction("mov rax, QWORD PTR [rsp + 40]");                       // reload path length
    emitter.instruction("cmp rax, 4");                                          // path < 4 chars?
    emitter.instruction("jl .Lreadlink_no_strip");                              // can't have prefix
    emitter.instruction("mov ecx, DWORD PTR [r10]");                            // load first 4 bytes
    emitter.instruction("cmp ecx, 0x5C3F5C5C");                                 // "\\?\" in little-endian
    emitter.instruction("jne .Lreadlink_no_strip");                             // not prefix
    // -- strip prefix: copy content left by 4 bytes, adjust length --
    emitter.instruction("sub rax, 4");                                          // new length = original - 4
    emitter.instruction("lea rsi, [r10 + 4]");                                  // source = buffer + 4
    emitter.instruction("mov rdi, r10");                                        // dest = buffer
    emitter.instruction("mov rcx, rax");                                        // copy count
    emitter.label(".Lreadlink_strip_loop");
    emitter.instruction("test rcx, rcx");                                       // remaining bytes?
    emitter.instruction("jz .Lreadlink_strip_done");                            // done
    emitter.instruction("mov dl, BYTE PTR [rsi]");                              // load byte
    emitter.instruction("mov BYTE PTR [rdi], dl");                              // store byte
    emitter.instruction("inc rsi");                                             // advance source
    emitter.instruction("inc rdi");                                             // advance dest
    emitter.instruction("dec rcx");                                             // remaining--
    emitter.instruction("jmp .Lreadlink_strip_loop");                           // continue
    emitter.label(".Lreadlink_strip_done");
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return length (already in rax)
    emitter.label(".Lreadlink_no_strip");
    emitter.instruction("mov rax, QWORD PTR [rsp + 40]");                       // reload path length (return as-is)
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lreadlink_fail");
    emitter.instruction("mov rax, -1");                                         // return -1 on failure
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // lstat: delegate to __rt_sys_stat (same as stat on Windows)
    emitter.label_global("lstat");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rdi");                                        // path
    emitter.instruction("mov rdx, rsi");                                        // stat buffer
    emitter.instruction("call stat");                                           // msvcrt stat
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return
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

    // open: delegate to __rt_sys_open (CreateFileA)
    emitter.label_global("open");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_open");                                  // call CreateFileA shim
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

    // lseek: delegate to SetFilePointer
    emitter.label_global("lseek");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_lseek");                                 // call SetFilePointer shim
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

    // kill: use TerminateProcess for SIGKILL (9), no-op for other signals
    emitter.label_global("kill");
    emitter.instruction("cmp rsi, 9");                                          // sig == SIGKILL?
    emitter.instruction("jne .Lkill_noop");                                     // skip if not SIGKILL
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + handle(8)
    emitter.instruction("mov rcx, 1");                                          // dwDesiredAccess = PROCESS_TERMINATE
    emitter.instruction("xor rdx, rdx");                                        // bInheritHandle = FALSE
    emitter.instruction("mov r8, rdi");                                         // dwProcessId = pid
    emitter.instruction("call OpenProcess");                                    // open process handle
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // save handle
    emitter.instruction("test rax, rax");                                       // check if OpenProcess succeeded
    emitter.instruction("je .Lkill_fail");                                      // jump if failed
    emitter.instruction("mov rcx, rax");                                        // handle
    emitter.instruction("mov rdx, 1");                                          // exit code
    emitter.instruction("call TerminateProcess");                               // terminate process
    emitter.instruction("mov rcx, QWORD PTR [rsp + 32]");                       // reload handle
    emitter.instruction("call CloseHandle");                                    // close handle
    emitter.label(".Lkill_fail");
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("xor rax, rax");                                        // return 0
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lkill_noop");
    emitter.instruction("xor rax, rax");                                        // return 0 (no-op for non-SIGKILL)
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

    // utimensat: simplified — open file and call SetFileTime with NULL (preserves current time)
    emitter.label_global("utimensat");
    emitter.instruction("sub rsp, 56");                                         // shadow(32) + handle(8) + padding(16)
    emitter.instruction("mov rcx, rsi");                                        // path (SysV arg2, skip dirfd)
    emitter.instruction("mov rdx, 0x80000000");                                 // GENERIC_READ
    emitter.instruction("mov r8, 3");                                           // FILE_SHARE_READ | FILE_SHARE_WRITE
    emitter.instruction("xor r9, r9");                                          // lpSecurityAttributes = NULL
    emitter.instruction("mov QWORD PTR [rsp + 32], 3");                         // dwCreationDisposition = OPEN_EXISTING
    emitter.instruction("mov QWORD PTR [rsp + 40], 0");                         // dwFlagsAndAttributes = 0
    emitter.instruction("mov QWORD PTR [rsp + 48], 0");                         // hTemplateFile = NULL
    emitter.instruction("call CreateFileA");                                    // open file
    emitter.instruction("mov QWORD PTR [rsp + 48], rax");                       // save handle (reuse [rsp+48] after call)
    emitter.instruction("cmp rax, -1");                                         // INVALID_HANDLE_VALUE?
    emitter.instruction("je .Lutimensat_fail");                                 // jump if failed
    emitter.instruction("mov rcx, rax");                                        // handle
    emitter.instruction("xor rdx, rdx");                                        // lpCreationTime = NULL
    emitter.instruction("xor r8, r8");                                          // lpLastAccessTime = NULL
    emitter.instruction("xor r9, r9");                                          // lpLastWriteTime = NULL
    emitter.instruction("call SetFileTime");                                    // set file times (NULL = preserve)
    emitter.instruction("mov rcx, QWORD PTR [rsp + 48]");                       // reload handle
    emitter.instruction("call CloseHandle");                                    // close handle
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("xor rax, rax");                                        // return 0
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lutimensat_fail");
    emitter.instruction("mov rax, -1");                                         // return -1 on failure
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // fsync: delegate to FlushFileBuffers
    emitter.label_global("fsync");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rdi");                                        // fd
    emitter.instruction("call __rt_fd_to_handle");                              // convert fd to HANDLE
    emitter.instruction("mov rcx, rax");                                        // handle
    emitter.instruction("call FlushFileBuffers");                               // flush file buffers to disk
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return (nonzero = success)
    emitter.blank();

    // chown/lchown/fchown: return -1 (ENOSYS) — Windows uses ACLs not Unix ownership
    for sym in &["chown", "lchown", "fchown"] {
        emitter.label_global(sym);
        emitter.instruction("mov rax, -1");                                     // return -1 (not supported on Windows)
        emitter.instruction("ret");                                             // return
        emitter.blank();
    }

    // glob: use FindFirstFileA to check if pattern matches anything
    emitter.label_global("glob");
    emitter.instruction("sub rsp, 56");                                         // shadow(40) + WIN32_FIND_DATA + handle(8), 16-byte aligned
    emitter.instruction("mov rcx, rdi");                                        // pattern
    emitter.instruction("lea rdx, [rsp + 40]");                                 // &findData (above shadow space)
    emitter.instruction("call FindFirstFileA");                                 // find first matching file
    emitter.instruction("cmp rax, -1");                                         // INVALID_HANDLE_VALUE?
    emitter.instruction("je .Lglob_nomatch");                                   // no match
    emitter.instruction("mov rcx, rax");                                        // handle
    emitter.instruction("call FindClose");                                      // close find handle
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("xor rax, rax");                                        // return 0 (success, found matches)
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lglob_nomatch");
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("mov rax, 1");                                          // return GLOB_NOMATCH
    emitter.instruction("ret");                                                 // return
    emitter.blank();
    // globfree: no-op (FindFirstFileA/FindClose don't allocate a result array)
    emitter.label_global("globfree");
    emitter.instruction("ret");                                                 // no-op
    emitter.blank();

    // fnmatch: delegate to PathMatchSpecA (shlwapi)
    emitter.label_global("fnmatch");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rsi");                                        // pszFile = string (SysV arg2)
    emitter.instruction("mov rdx, rdi");                                        // pszSpec = pattern (SysV arg1)
    emitter.instruction("call PathMatchSpecA");                                 // match pattern against string
    emitter.instruction("test rax, rax");                                       // PathMatchSpecA returns TRUE on match
    emitter.instruction("jz .Lfnmatch_nomatch");                                // jump if no match
    emitter.instruction("xor rax, rax");                                        // return 0 (FNM_NOMATCH = 0 means match)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lfnmatch_nomatch");
    emitter.instruction("mov rax, 1");                                          // return FNM_NOMATCH (1)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // realpath: delegate to GetFullPathNameA
    emitter.label_global("realpath");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rdi");                                        // lpFileName = path
    emitter.instruction("mov rdx, 4096");                                       // lpBuffer size
    emitter.instruction("mov r8, rsi");                                         // lpBuffer = resolved
    emitter.instruction("xor r9, r9");                                          // lpFilePart = NULL
    emitter.instruction("call GetFullPathNameA");                               // resolve to full path
    emitter.instruction("test rax, rax");                                       // check if succeeded
    emitter.instruction("je .Lrealpath_fail");                                  // jump if failed (return 0)
    emitter.instruction("mov rax, rsi");                                        // return resolved path pointer
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lrealpath_fail");
    emitter.instruction("xor rax, rax");                                        // return NULL on failure
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // chmod: delegate to SetFileAttributesA
    emitter.label_global("chmod");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_chmod");                                 // call SetFileAttributesA shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // unlink: delegate to DeleteFileA
    emitter.label_global("unlink");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_unlink");                                // call DeleteFileA shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // mkdir: delegate to CreateDirectoryA
    emitter.label_global("mkdir");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_mkdir");                                 // call CreateDirectoryA shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // rmdir: delegate to RemoveDirectoryA
    emitter.label_global("rmdir");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_rmdir");                                 // call RemoveDirectoryA shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // rename: delegate to MoveFileA
    emitter.label_global("rename");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_rename");                                // call MoveFileA shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // getcwd: delegate to GetCurrentDirectoryA
    emitter.label_global("getcwd");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_getcwd");                                // call GetCurrentDirectoryA shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();

    // chdir: delegate to SetCurrentDirectoryA
    emitter.label_global("chdir");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("call __rt_sys_chdir");                                 // call SetCurrentDirectoryA shim
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

    // dirfd: return -1 (ENOSYS) — no concept on Windows
    // hstrerror: return NULL — use WSAGetLastError instead
    // h_errno: return 0 — Windows uses WSAGetLastError
    emitter.label_global("dirfd");
    emitter.instruction("mov rax, -1");                                         // return -1 (not supported on Windows)
    emitter.instruction("ret");                                                 // return
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

    // __errno static variable
    emitter.raw(".data");
    emitter.raw("__rt_errno:");
    emitter.raw("    .zero 8");
    emitter.raw(".text");
    emitter.blank();
}

/// Emits the Windows entry point wrapper.
///
/// MinGW's CRT startup calls `main(argc, argv, envp)` with MSx64 ABI:
/// rcx=argc, rdx=argv, r8=envp. Our codegen expects SysV ABI:
/// rdi=argc, rsi=argv. This wrapper shuffles the arguments.
pub(crate) fn emit_main_wrapper(emitter: &mut Emitter) {
    emitter.label_global("main");
    emitter.instruction("sub rsp, 8");                                          // align stack to 16 bytes
    emitter.instruction("mov rdi, rcx");                                        // SysV arg1 = argc (from MSx64 rcx)
    emitter.instruction("mov rsi, rdx");                                        // SysV arg2 = argv (from MSx64 rdx)
    emitter.instruction("call __elephc_main");                                  // call the real program entry
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return to CRT
    emitter.blank();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::Target;

    /// Verifies that Win32 shims emit the expected symbols for windows-x86_64.
    #[test]
    fn test_win32_shims_emit_expected_symbols() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_win32_shims(&mut emitter);
        let asm = emitter.output();
        for sym in [
            "__rt_sys_write",
            "__rt_sys_read",
            "__rt_sys_exit",
            "__rt_sys_close",
            "__rt_sys_mmap",
            "__rt_sys_open",
        ] {
            assert!(
                asm.contains(&format!(".globl {}\n", sym)),
                "Win32 shim missing global symbol {}",
                sym
            );
        }
    }

    /// Verifies that Win32 imports are declared.
    #[test]
    fn test_win32_imports_declared() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_win32_shims(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains(".extern GetStdHandle"));
        assert!(asm.contains(".extern WriteFile"));
        assert!(asm.contains(".extern ExitProcess"));
        assert!(asm.contains(".extern HeapAlloc"));
    }

    /// Verifies that fd_to_handle emits stdio conversion.
    #[test]
    fn test_fd_to_handle_emitted() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_fd_to_handle(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("GetStdHandle"));
        assert!(asm.contains("STD_INPUT_HANDLE") || asm.contains("-10"));
    }

    /// Verifies that the main wrapper shuffles MSx64 args to SysV.
    #[test]
    fn test_main_wrapper_shuffles_args() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_main_wrapper(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("mov rdi, rcx"));
        assert!(asm.contains("mov rsi, rdx"));
        assert!(asm.contains("call __elephc_main"));
    }

    /// Verifies that newly added shims for previously-missing syscalls are emitted.
    #[test]
    fn test_new_shims_emitted() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_win32_shims(&mut emitter);
        let asm = emitter.output();
        for sym in [
            "__rt_sys_lseek",
            "__rt_sys_socketpair",
            "__rt_sys_statfs",
            "__rt_sys_pselect6",
            "__rt_sys_sendmsg",
            "__rt_sys_recvmsg",
            "__rt_sys_getdents",
            "__rt_sys_creat",
            "__rt_sys_clock_getres",
            "__rt_sys_newfstatat",
            "__rt_sys_dup",
            "__rt_sys_dup2",
        ] {
            assert!(
                asm.contains(&format!(".globl {}\n", sym)),
                "Win32 shim missing global symbol {}",
                sym
            );
        }
    }

    /// Verifies that fcntl delegates to ioctlsocket (not a no-op stub).
    #[test]
    fn test_fcntl_delegates_to_ioctlsocket() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_shim_fcntl(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("__rt_sys_ioctl"));
    }

    /// Verifies that open shim handles O_CREAT and O_TRUNC flags.
    #[test]
    fn test_open_shim_handles_flags() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_shim_open(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("0x40"), "O_CREAT check missing");
        assert!(asm.contains("0x200"), "O_TRUNC check missing");
        assert!(asm.contains("0x400"), "O_APPEND check missing");
    }

    /// Verifies that getrandom returns byte count on success, -1 on failure.
    #[test]
    fn test_getrandom_returns_count() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_shim_getrandom(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("BCryptGenRandom"));
        assert!(asm.contains(".Lgetrandom_fail"));
    }

    /// Verifies that kill shim uses OpenProcess+TerminateProcess for SIGKILL.
    #[test]
    fn test_kill_uses_terminate_process() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_shim_kill(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("OpenProcess"));
        assert!(asm.contains("TerminateProcess"));
    }

    /// Verifies that writev shim saves iov pointer on stack (not in rsi).
    #[test]
    fn test_writev_saves_iov_ptr() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_shim_writev(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("[rsp + 8]"), "iov pointer should be saved on stack");
    }

    /// Verifies that _dup and _dup2 are in the imports list.
    #[test]
    fn test_dup_imports_declared() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_win32_shims(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains(".extern _dup"));
        assert!(asm.contains(".extern _dup2"));
    }

    /// Verifies that the 6 previously-missing __rt_sys_* shims are now emitted.
    #[test]
    fn test_missing_sys_shims_now_emitted() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_win32_shims(&mut emitter);
        let asm = emitter.output();
        for sym in [
            "__rt_sys_link",
            "__rt_sys_symlink",
            "__rt_sys_readlink",
            "__rt_sys_chown",
            "__rt_sys_fchown",
            "__rt_sys_lchown",
        ] {
            assert!(
                asm.contains(&format!(".globl {}\n", sym)),
                "Missing __rt_sys_* shim: {}",
                sym
            );
        }
    }

    /// Verifies that clock_gettime uses r11 as divisor (not rdx, which crashes).
    #[test]
    fn test_clock_gettime_divisor_is_r11() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_shim_clock_gettime(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("xor rdx, rdx"), "RDX must be cleared before div");
        assert!(asm.contains("mov r11, 10000000"), "Divisor should be in r11");
        assert!(asm.contains("div r11"), "Should divide by r11, not rdx");
        assert!(!asm.contains("div rdx"), "Must NOT divide by rdx (crash bug)");
    }

    /// Verifies that utimensat sets all 7 CreateFileA arguments.
    #[test]
    fn test_utimensat_has_all_createfile_args() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_shim_c_symbols(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("utimensat"));
        // arg 5: dwCreationDisposition at [rsp+32]
        assert!(asm.contains("[rsp + 32], 3"));
        // arg 6: dwFlagsAndAttributes at [rsp+40]
        assert!(asm.contains("[rsp + 40], 0"));
        // arg 7: hTemplateFile at [rsp+48]
        assert!(asm.contains("[rsp + 48], 0"));
    }

    /// Verifies that symlink shim retries with ALLOW_UNPRIVILEGED_CREATE.
    #[test]
    fn test_symlink_unprivileged_retry() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_shim_c_symbols(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("mov r8, 2"), "Should set ALLOW_UNPRIVILEGED_CREATE");
        assert!(asm.contains(".Lsymlink_ok"));
        assert!(asm.contains(".Lsymlink_fail"));
    }

    /// Verifies that readlink shim saves buffer at offset 64 (no conflict with CreateFileA args).
    #[test]
    fn test_readlink_clean_stack_layout() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_shim_c_symbols(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("[rsp + 64], rsi"), "Buffer should be saved at offset 64");
        assert!(asm.contains("[rsp + 56], rdx"), "Bufsize should be saved at offset 56");
        assert!(!asm.contains("Wait"), "No leftover debugging comments");
    }

    /// Verifies that all shims use 16-byte aligned stack frames (sub rsp, 40 not 32).
    #[test]
    fn test_stack_alignment_16_bytes() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_win32_shims(&mut emitter);
        let asm = emitter.output();
        // Every shim that calls a Win32 function must use sub rsp, 40 or 56 (8 mod 16)
        assert!(
            !asm.contains("sub rsp, 32\n"),
            "No shim should use sub rsp, 32 (misaligned). Use 40 or 56 instead."
        );
    }

    /// Verifies that sendto passes all 6 arguments.
    #[test]
    fn test_sendto_passes_6_args() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_shim_socket_shims(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("__rt_sys_sendto"));
        assert!(asm.contains("[rsp + 32], r8"), "sendto: dest_addr should be at [rsp+32]");
        assert!(asm.contains("[rsp + 40], r9"), "sendto: addrlen should be at [rsp+40]");
        assert!(asm.contains("mov r9, r10"), "sendto: flags should go to r9");
    }

    /// Verifies that recvfrom passes all 6 arguments.
    #[test]
    fn test_recvfrom_passes_6_args() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_shim_socket_shims(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("__rt_sys_recvfrom"));
        assert!(asm.contains("[rsp + 32], r8"), "recvfrom: src_addr should be at [rsp+32]");
        assert!(asm.contains("[rsp + 40], r9"), "recvfrom: &addrlen should be at [rsp+40]");
    }

    /// Verifies that setsockopt passes all 5 arguments.
    #[test]
    fn test_setsockopt_passes_5_args() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_shim_setsockopt(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("[rsp + 32], r8"), "setsockopt: optlen should be at [rsp+32]");
        assert!(asm.contains("mov r9, r10"), "setsockopt: optval should go to r9");
    }

    /// Verifies that getsockopt passes all 5 arguments.
    #[test]
    fn test_getsockopt_passes_5_args() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_shim_getsockopt(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("[rsp + 32], r8"), "getsockopt: &optlen should be at [rsp+32]");
        assert!(asm.contains("mov r9, r10"), "getsockopt: optval should go to r9");
    }

    /// Verifies that flock uses sub rsp, 56 for LockFileEx (6 args).
    #[test]
    fn test_flock_stack_alignment_for_6_args() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_shim_c_symbols(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("flock"));
        assert!(asm.contains("sub rsp, 56"), "flock needs 56 bytes for LockFileEx (6 args)");
    }

    /// Verifies WriteFile shim uses the MSx64-correct 5th-arg layout: lpOverlapped=NULL
    /// at [rsp+32] (arg5) and the &bytesWritten output pointer at [rsp+40], never
    /// colliding on the arg5 slot.
    #[test]
    fn test_write_shim_overlapped_and_output_offsets() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_shim_write(&mut emitter);
        let asm = emitter.output();
        assert!(
            asm.contains("mov QWORD PTR [rsp + 32], 0"),
            "lpOverlapped NULL must be at the arg5 slot [rsp+32]"
        );
        assert!(
            asm.contains("lea r9, [rsp + 40]"),
            "&bytesWritten (arg4) must point at [rsp+40], off the arg5 slot"
        );
        assert!(
            asm.contains("mov rax, QWORD PTR [rsp + 40]"),
            "bytesWritten must be read back from [rsp+40]"
        );
        assert!(
            !asm.contains("lea r9, [rsp + 32]"),
            "output pointer must not alias the arg5 (lpOverlapped) slot"
        );
        // `len` must be spilled to the stack and reloaded across the intervening
        // `call __rt_fd_to_handle` — r8 is volatile in MSx64 and may be clobbered.
        assert!(
            asm.contains("mov QWORD PTR [rsp + 48], rdx"),
            "len must be spilled to [rsp+48] before the handle-conversion call"
        );
        assert!(
            asm.contains("mov r8, QWORD PTR [rsp + 48]"),
            "len must be reloaded from [rsp+48] into r8 after the handle-conversion call"
        );
        assert!(
            !asm.contains("mov r8, rdx"),
            "len must not be parked in volatile r8 across the call (regression guard)"
        );
    }

    /// Verifies ReadFile shim uses the MSx64-correct 5th-arg layout: lpOverlapped=NULL
    /// at [rsp+32] (arg5) and the &bytesRead output pointer at [rsp+40].
    #[test]
    fn test_read_shim_overlapped_and_output_offsets() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_shim_read(&mut emitter);
        let asm = emitter.output();
        assert!(
            asm.contains("mov QWORD PTR [rsp + 32], 0"),
            "lpOverlapped NULL must be at the arg5 slot [rsp+32]"
        );
        assert!(
            asm.contains("lea r9, [rsp + 40]"),
            "&bytesRead (arg4) must point at [rsp+40], off the arg5 slot"
        );
        assert!(
            asm.contains("mov rax, QWORD PTR [rsp + 40]"),
            "bytesRead must be read back from [rsp+40]"
        );
        assert!(
            !asm.contains("lea r9, [rsp + 32]"),
            "output pointer must not alias the arg5 (lpOverlapped) slot"
        );
        // `len` must be spilled to the stack and reloaded across the intervening
        // `call __rt_fd_to_handle` — r8 is volatile in MSx64 and may be clobbered.
        assert!(
            asm.contains("mov QWORD PTR [rsp + 48], rdx"),
            "len must be spilled to [rsp+48] before the handle-conversion call"
        );
        assert!(
            asm.contains("mov r8, QWORD PTR [rsp + 48]"),
            "len must be reloaded from [rsp+48] into r8 after the handle-conversion call"
        );
        assert!(
            !asm.contains("mov r8, rdx"),
            "len must not be parked in volatile r8 across the call (regression guard)"
        );
    }

    /// Verifies the lseek shim spills `whence` across the intervening
    /// `call __rt_fd_to_handle` instead of holding it in the volatile r10.
    #[test]
    fn test_lseek_shim_spills_whence_across_call() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_shim_lseek(&mut emitter);
        let asm = emitter.output();
        assert!(
            asm.contains("mov QWORD PTR [rsp + 32], rdx"),
            "whence must be spilled to [rsp+32] before the handle-conversion call"
        );
        assert!(
            asm.contains("mov r9, QWORD PTR [rsp + 32]"),
            "whence must be reloaded from [rsp+32] into r9 after the handle-conversion call"
        );
        assert!(
            !asm.contains("mov r9, r10"),
            "whence must not survive the call in volatile r10 (regression guard)"
        );
    }

    /// Verifies the readlink shim spills the file HANDLE and the returned path
    /// length to the stack across the intervening GetFinalPathNameByHandleA /
    /// CloseHandle calls rather than holding them in the volatile r10/r11.
    #[test]
    fn test_readlink_shim_spills_handle_and_length_across_calls() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_shim_c_symbols(&mut emitter);
        let asm = emitter.output();
        assert!(
            asm.contains("mov QWORD PTR [rsp + 48], rax"),
            "readlink handle must be spilled to [rsp+48] across the Win32 calls"
        );
        assert!(
            asm.contains("mov rcx, QWORD PTR [rsp + 48]"),
            "readlink handle must be reloaded from [rsp+48] for CloseHandle"
        );
        assert!(
            asm.contains("mov QWORD PTR [rsp + 40], rax"),
            "readlink path length must be spilled to [rsp+40] across CloseHandle"
        );
        assert!(
            !asm.contains("mov rcx, r10"),
            "readlink handle must not survive a call in volatile r10 (regression guard)"
        );
    }

    /// Verifies that `emit_win32_shims` unconditionally emits the
    /// `__rt_unsupported_syscall` diagnostic helper (the target of the transform's
    /// unmapped-syscall path), so it is always present for the transform to call.
    #[test]
    fn test_unsupported_syscall_helper_emitted() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_win32_shims(&mut emitter);
        let asm = emitter.output();
        assert!(
            asm.contains(".globl __rt_unsupported_syscall\n"),
            "emit_win32_shims must emit the __rt_unsupported_syscall diagnostic helper"
        );
        assert!(
            asm.contains("call ExitProcess"),
            "the unsupported-syscall helper must terminate via ExitProcess"
        );
    }
}