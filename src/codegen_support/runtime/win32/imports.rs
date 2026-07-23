//! Purpose:
//! Declares Win32 API imports and maps imported C symbols to runtime ABI shims.
//!
//! Called from:
//! - `crate::codegen_support::runtime::win32::emit_win32_shims()`.
//!
//! Key details:
//! - Every Win32 API referenced by emitted assembly is declared explicitly.
//! - C-library calls that need SysV-to-MSx64 adaptation resolve through dedicated shim labels.

use crate::codegen::emit::Emitter;

/// Emits `.extern` declarations for all Win32 API functions used by the shims.
pub(super) fn emit_win32_imports(emitter: &mut Emitter) {
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
    "GetLastError",
    "FormatMessageW",
    "MultiByteToWideChar",
    "WideCharToMultiByte",
    "CompareStringOrdinal",
    "CloseHandle",
    "ExitProcess",
    "GetCommandLineW",
    "CommandLineToArgvW",
    "LocalFree",
    "GetProcessHeap",
    "HeapAlloc",
    "HeapFree",
    "VirtualAlloc",
    "VirtualFree",
    "VirtualProtect",
    "GetCurrentProcessId",
    "GetProcessId",
    "GetSystemTimeAsFileTime",
    "QueryPerformanceCounter",
    "QueryPerformanceFrequency",
    "BCryptGenRandom",
    "CreateFileW",
    "SetFilePointer",
    "GetFileType",
    "GetConsoleMode",
    "DeleteFileW",
    "GetCurrentDirectoryW",
    "SetCurrentDirectoryW",
    "CreateDirectoryW",
    "RemoveDirectoryW",
    "GetFileAttributesW",
    "GetFileAttributesExW",
    "GetFileSizeEx",
    "MoveFileExW",
    "SetFileAttributesW",
    "GetDiskFreeSpaceExW",
    "GetTempPathW",
    "GetTempFileNameW",
    "GetComputerNameW",
    "GetNativeSystemInfo",
    "GetFileInformationByHandle",
    "DeviceIoControl",
    "gethostname",
    "gethostbyname",
    "getprotobyname",
    "getprotobynumber",
    "getservbyname",
    "getservbyport",
    "socket",
    "WSASocketW",
    "connect",
    "bind",
    "listen",
    "accept",
    "send",
    "recv",
    "sendto",
    "recvfrom",
    "WSASend",
    "WSARecv",
    "shutdown",
    "closesocket",
    "getsockname",
    "getpeername",
    "setsockopt",
    "getsockopt",
    "ioctlsocket",
    "select",
    "WSAGetLastError",
    "getpid",
    "_putenv",
    "uname",
    "sysinfo",
    "execve",
    "kill",
    "futex",
    "FlushFileBuffers",
    "LockFileEx",
    "UnlockFileEx",
    "CreateSymbolicLinkW",
    "CreateHardLinkW",
    "FindFirstFileExW",
    "FindNextFileW",
    "FindClose",
    "GetFullPathNameW",
    "GetFinalPathNameByHandleW",
    "GetBinaryTypeW",
    "SetFileTime",
    "_mkgmtime",
    "_execvp",
    "_popen",
    "_pclose",
    "_fileno",
    "fgetc",
    "system",
    "strtod",
    "snprintf",
    "strtol",
    "OpenProcess",
    "TerminateProcess",
    "GlobalMemoryStatusEx",
    "PathMatchSpecW",
    "_dup",
    "_dup2",
    "_open_osfhandle",
    "_get_osfhandle",
    "_close",
    "WSAStartup",
    "WSACleanup",
    "SetFilePointerEx",
    "SetEndOfFile",
    "Sleep",
    "GetProcessTimes",
    "getenv",
    "_tzset",
    "time",
    "localtime",
    "gmtime",
    "mktime",
    "pow",
    "sin",
    "cos",
    "tan",
    "asin",
    "acos",
    "atan",
    "sinh",
    "cosh",
    "tanh",
    "exp",
    "log",
    "log2",
    "log10",
    "atan2",
    "hypot",
    "fmod",
    "round",
    "compressBound",
    "deflateEnd",
    "inflateEnd",
    "deflate",
    "inflate",
    "uncompress",
    "inflateInit2_",
    "compress2",
    "deflateInit2_",
    // W3g bzip2 family: statically linked from the MinGW-sysroot `libbz2.a`
    // (via `ELEPHC_MINGW_SYSROOT`, `src/linker.rs:406`, `-lbz2`), MSx64 ABI —
    // same pattern as the W3d zlib family above. See `emit_shim_bzip2`.
    "BZ2_bzCompress",
    "BZ2_bzCompressInit",
    "BZ2_bzCompressEnd",
    "BZ2_bzBuffToBuffDecompress",
    "pcre2_regcomp",
    "pcre2_regexec",
    "pcre2_regfree",
    "malloc",
    "free",
    // W3e-2 net/dns/inet (ws2_32) + misc (msvcrt) family.
    "getaddrinfo",
    "freeaddrinfo",
    "inet_pton",
    "inet_ntop",
    "gethostbyaddr",
    "_strtoi64",
    "atof",
    "setlocale",
    // W3f-A iconv family: statically linked from the MinGW-sysroot `libiconv.a`
    // (via `ELEPHC_MINGW_SYSROOT`, `src/linker.rs:256/406`, `-liconv`), MSx64
    // ABI — same pattern as the W3d zlib / W3e-1 PCRE2-POSIX families above.
    "iconv_open",
    "iconv",
    "iconv_close",
    // W3f-B rewrites: standard msvcrt symbols (real ABI shims — see
    // `windows_c_shim_name` doc for the per-symbol msvcrt-existence verdict).
    "fopen",
    "fgets",
    "fclose",
    "strncmp",
    "strchr",
    "strtoul",
    // W6/C1c proc_open/proc_close family: real process spawning via
    // `CreatePipe`/`CreateProcessW`/`WaitForSingleObject`/`GetExitCodeProcess`
    // (`emit_proc_open_win32_x86_64` in `runtime/io/proc_open.rs`, and the
    // Windows arm of `emit_proc_close` in `runtime/io/proc_close.rs`).
    "CreatePipe",
    "DuplicateHandle",
    "PeekNamedPipe",
    "CreateProcessW",
    "WaitForSingleObject",
    "GetExitCodeProcess",
    "SetHandleInformation",
    "SetErrorMode",
];

/// C-library symbols that have a dedicated `__rt_sys_<symbol>` Windows shim
/// emitted below (`emit_shim_strtod`, `emit_shim_strtol`, `emit_shim_snprintf`,
/// `emit_shim_gethostbyname`, the W3b datetime family — `emit_shim_getenv`,
/// `emit_shim_putenv`, `emit_shim_tzset`, `emit_shim_time`, `emit_shim_localtime`,
/// `emit_shim_gmtime`, `emit_shim_mktime`, `emit_shim_gettimeofday` — and the
/// W3c math/FP family emitted uniformly by [`emit_shim_math_fp`] /
/// [`emit_fp_shadow_shim`] over [`MATH_FP_SHIM_SYMBOLS`], and the W3d zlib
/// family — `compressBound`, `deflateEnd`, `inflateEnd`, `deflate`,
/// `inflate`, `uncompress`, `inflateInit2_`, `compress2`, `deflateInit2_`,
/// emitted by [`emit_shim_zlib`] — statically linked from the MinGW-sysroot
/// `libz.a`, which is MSx64-ABI (built by MinGW gcc), NOT SysV, and the W3e-1
/// PCRE2-POSIX/alloc family — `pcre2_regcomp`, `pcre2_regexec`,
/// `pcre2_regfree` (statically linked from the MinGW-sysroot
/// `libpcre2-posix.a`/`libpcre2-8.a`, also MSx64-ABI) and `malloc`/`free`
/// (standard msvcrt symbols), emitted by [`emit_shim_pcre2_posix`],
/// [`emit_shim_malloc`], and [`emit_shim_free`] — each backed by a Win32 API
/// import declared in
/// [`WIN32_IMPORTS`]. This is the SINGLE SOURCE OF TRUTH consulted by
/// `Emitter::emit_call_c` (`codegen_support::emit`): registering a new symbol
/// here, adding its `emit_shim_*` wrapper, adding its Win32 import name to
/// `WIN32_IMPORTS`, and calling the new `emit_shim_*` from `emit_win32_shims`
/// is the complete, one-place change needed to route a new msvcrt/ws2_32 call
/// correctly on windows-x86_64. Returns `None` for a symbol with no shim —
/// callers fall back to the SysV stub-delegate list or panic (see
/// `Emitter::emit_call_c`).
pub(crate) fn windows_c_shim_name(symbol: &str) -> Option<&'static str> {
    match symbol {
        "strtod" => Some("__rt_sys_strtod"),
        "strtol" => Some("__rt_sys_strtol"),
        "snprintf" => Some("__rt_sys_snprintf"),
        "gethostbyname" => Some("__rt_sys_gethostbyname"),
        "getenv" => Some("__rt_sys_getenv"),
        "putenv" => Some("__rt_sys_putenv"),
        "tzset" => Some("__rt_sys_tzset"),
        "time" => Some("__rt_sys_time"),
        "localtime" => Some("__rt_sys_localtime"),
        "gmtime" => Some("__rt_sys_gmtime"),
        "mktime" => Some("__rt_sys_mktime"),
        "gettimeofday" => Some("__rt_sys_gettimeofday"),
        "pow" => Some("__rt_sys_pow"),
        "sin" => Some("__rt_sys_sin"),
        "cos" => Some("__rt_sys_cos"),
        "tan" => Some("__rt_sys_tan"),
        "asin" => Some("__rt_sys_asin"),
        "acos" => Some("__rt_sys_acos"),
        "atan" => Some("__rt_sys_atan"),
        "sinh" => Some("__rt_sys_sinh"),
        "cosh" => Some("__rt_sys_cosh"),
        "tanh" => Some("__rt_sys_tanh"),
        "exp" => Some("__rt_sys_exp"),
        "log" => Some("__rt_sys_log"),
        "log2" => Some("__rt_sys_log2"),
        "log10" => Some("__rt_sys_log10"),
        "atan2" => Some("__rt_sys_atan2"),
        "hypot" => Some("__rt_sys_hypot"),
        "fmod" => Some("__rt_sys_fmod"),
        "round" => Some("__rt_sys_round"),
        "compressBound" => Some("__rt_sys_compressBound"),
        "deflateEnd" => Some("__rt_sys_deflateEnd"),
        "inflateEnd" => Some("__rt_sys_inflateEnd"),
        "deflate" => Some("__rt_sys_deflate"),
        "inflate" => Some("__rt_sys_inflate"),
        "uncompress" => Some("__rt_sys_uncompress"),
        "inflateInit2_" => Some("__rt_sys_inflateInit2_"),
        "compress2" => Some("__rt_sys_compress2"),
        "deflateInit2_" => Some("__rt_sys_deflateInit2_"),
        // W3g bzip2 family — real ABI shims (libbz2 statically linked on
        // Windows, same sysroot mechanism as the zlib family above). See
        // `emit_shim_bzip2`.
        "BZ2_bzCompress" => Some("__rt_sys_BZ2_bzCompress"),
        "BZ2_bzCompressInit" => Some("__rt_sys_BZ2_bzCompressInit"),
        "BZ2_bzCompressEnd" => Some("__rt_sys_BZ2_bzCompressEnd"),
        "BZ2_bzBuffToBuffDecompress" => Some("__rt_sys_BZ2_bzBuffToBuffDecompress"),
        "pcre2_regcomp" => Some("__rt_sys_pcre2_regcomp"),
        "pcre2_regexec" => Some("__rt_sys_pcre2_regexec"),
        "pcre2_regfree" => Some("__rt_sys_pcre2_regfree"),
        "malloc" => Some("__rt_sys_malloc"),
        "free" => Some("__rt_sys_free"),
        // W3e-2 net/dns/inet (ws2_32) family — see `emit_shim_net_dns`.
        "getaddrinfo" => Some("__rt_sys_getaddrinfo"),
        "freeaddrinfo" => Some("__rt_sys_freeaddrinfo"),
        "inet_pton" => Some("__rt_sys_inet_pton"),
        "inet_ntop" => Some("__rt_sys_inet_ntop"),
        "gethostbyaddr" => Some("__rt_sys_gethostbyaddr"),
        // W3e-2 misc msvcrt family — see `emit_shim_net_dns`.
        "strtoll" => Some("__rt_sys_strtoll"),
        "atof" => Some("__rt_sys_atof"),
        // `dup` already has an `__rt_sys_dup` shim (`emit_shim_dup_shims`,
        // W3c) that calls msvcrt `_dup` with the required `cdqe`
        // sign-extension — reused here rather than duplicated.
        "dup" => Some("__rt_sys_dup"),
        "setlocale" => Some("__rt_sys_setlocale"),
        // `chown`/`lchown` are DELIBERATELY NOT routed to the pre-existing
        // `__rt_sys_chown`/`__rt_sys_lchown` labels (`emit_shim_c_symbol_delegates`,
        // used by the Linux-syscall-number 92/94 transform path — see
        // `windows_transform.rs`), which return -1 (ENOSYS). php-src makes
        // PHP-level `chown`/`lchown` fail silently on Windows; this distinct
        // contract from the Linux syscall surface means that this
        // W3e-2 libc-call-site family gets its own `__rt_sys_libc_chown`/
        // `__rt_sys_libc_lchown` shims instead of overloading the existing
        // (unrelated call path's) labels. See `emit_shim_net_dns`.
        "chown" => Some("__rt_sys_libc_chown"),
        "lchown" => Some("__rt_sys_libc_lchown"),
        // `dup2` already has an `__rt_sys_dup2` shim (`emit_shim_dup_shims`,
        // W3c) that calls msvcrt `_dup2` with the required `cdqe`
        // sign-extension — reused rather than duplicated. Consumers:
        // `stream_filters/iconv.rs` (W3f-A) plus `stream_filters/inflate.rs`
        // and `stream_filters/compress_bzip2_stream.rs` (W3g).
        "dup2" => Some("__rt_sys_dup2"),
        // W3f-A iconv family — real ABI shims (libiconv statically linked on
        // Windows, see the `WIN32_IMPORTS` comment above). See
        // `emit_shim_iconv`.
        "iconv_open" => Some("__rt_sys_iconv_open"),
        "iconv" => Some("__rt_sys_iconv"),
        "iconv_close" => Some("__rt_sys_iconv_close"),
        // W3f-B rewrites — msvcrt-real shims: fopen/fgets/fclose/strncmp/
        // strchr/strtoul all EXIST on msvcrt, so `principal_lookup.rs`'s
        // passwd/group lookup gets real ABI shims rather than a bespoke
        // stub; the "no /etc/passwd on Windows" behavior emerges naturally
        // (fopen("/etc/passwd") -> NULL -> the lookup's existing fail path).
        // See `emit_shim_msvcrt_passwd_lookup`.
        "fopen" => Some("__rt_sys_fopen"),
        "fgets" => Some("__rt_sys_fgets"),
        "fclose" => Some("__rt_sys_fclose"),
        "fgetc" => Some("__rt_sys_fgetc"),
        "system" => Some("__rt_sys_system"),
        "strncmp" => Some("__rt_sys_strncmp"),
        "strchr" => Some("__rt_sys_strchr"),
        "strtoul" => Some("__rt_sys_strtoul"),
        // Directory and temporary-file operations have no direct msvcrt
        // equivalents. These real ABI shims implement them with the Unicode
        // Win32 APIs (`FindFirstFileExW`/`FindNextFileW`/`FindClose` and
        // exclusive `CreateFileW` creation) while preserving the POSIX
        // contracts expected by the shared runtime emitters.
        "opendir" => Some("__rt_sys_opendir"),
        "readdir" => Some("__rt_sys_readdir"),
        "closedir" => Some("__rt_sys_closedir"),
        "rewinddir" => Some("__rt_sys_rewinddir"),
        "mkstemp" => Some("__rt_sys_mkstemp"),
        // W3g: msvcrt has no `fdatasync` export. `fsync` (the bare
        // stub-delegate label emitted by `emit_shim_c_symbol_delegates`,
        // FlushFileBuffers-backed) already satisfies fdatasync's contract
        // (flush data AND metadata), so `__rt_sys_fdatasync` tail-calls it
        // rather than duplicating the body — same fallback the non-Windows
        // Darwin path already takes (`modify_x86_64.rs`).
        "fdatasync" => Some("__rt_sys_fdatasync"),
        _ => None,
    }
}
