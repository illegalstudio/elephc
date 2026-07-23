//! Win32 shims for the filesystem/fd family: open/read/write/close/seek/
//! stat/rename/chmod/access/mkdir/rmdir/getcwd/chdir/statfs/getdents and the
//! temp-dir/dir-rewrite-stub helpers.

use crate::codegen::emit::Emitter;
use crate::codegen_support::sentinels;

/// Emits a shim that converts SysV `write(fd, buf, len)` to Win32 `WriteFile`.
///
/// SysV: rdi=fd, rsi=buf, rdx=len → MSx64: rcx=handle, rdx=buf, r8=len, r9=&written
/// Native failures are translated into the shared POSIX errno slot and return
/// -1; the bytes-written out parameter is consumed only after `WriteFile`
/// reports success.
pub(super) fn emit_shim_write(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_write");
    emitter.instruction("sub rsp, 56");                                         // allocate shadow(32) + written(4) + padding
    emitter.instruction("mov QWORD PTR [rsp + 48], rdx");                       // spill len (rdx is volatile, clobbered by the call) to a safe slot
    emitter.instruction("mov rcx, rdi");                                        // fd for handle conversion
    emitter.instruction("call __rt_fd_to_handle");                              // convert fd to Win32 HANDLE
    emitter.instruction("cmp rax, -1");                                         // not a CRT file descriptor (socket resource)?
    emitter.instruction("je .Lsys_write_socket");                               // Winsock streams use send(), not WriteFile
    emitter.instruction("mov rcx, rax");                                        // handle
    emitter.instruction("mov rdx, rsi");                                        // buffer
    emitter.instruction("mov r8, QWORD PTR [rsp + 48]");                        // reload len (arg3) after the handle-conversion call
    emitter.instruction("mov QWORD PTR [rsp + 32], 0");                         // lpOverlapped = NULL (arg5 slot)
    emitter.instruction("lea r9, [rsp + 40]");                                  // &bytesWritten (arg4 -> [rsp+40])
    emitter.instruction("call WriteFile");                                      // WriteFile(handle, buf, len, &written, NULL)
    emitter.instruction("test eax, eax");                                       // BOOL result: zero means the write failed
    emitter.instruction("jz .Lsys_write_file_fail");                            // translate the native error instead of reading an undefined out-param
    emitter.instruction("mov eax, DWORD PTR [rsp + 40]");                       // success: return the DWORD byte count
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return to caller
    emitter.label(".Lsys_write_file_fail");
    emitter.instruction("call GetLastError");                                   // obtain the failed WriteFile status
    emitter.instruction("mov DWORD PTR [rip + __rt_win32_last_error], eax");    // preserve the native diagnostic independently of errno
    emitter.instruction("call __rt_win32_errno_from_code");                     // translate the Win32 failure to POSIX errno
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], eax");               // publish errno for fwrite and direct write callers
    emitter.instruction("mov rax, -1");                                         // POSIX write failure sentinel
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return -1 to the caller
    emitter.label(".Lsys_write_socket");
    emitter.instruction("mov rcx, rdi");                                        // SOCKET
    emitter.instruction("mov rdx, rsi");                                        // buffer
    emitter.instruction("mov r8, QWORD PTR [rsp + 48]");                        // byte count
    emitter.instruction("xor r9d, r9d");                                        // flags = 0
    emitter.instruction("call send");                                           // socket-stream write
    emitter.instruction("cmp eax, -1");                                         // SOCKET_ERROR?
    emitter.instruction("je .Lsys_write_socket_fail");                          // publish Winsock errno
    emitter.instruction("cdqe");                                                // signed byte count
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return byte count
    emitter.label(".Lsys_write_socket_fail");
    emitter.instruction("call __rt_wsa_capture_errno");                         // translate WSAGetLastError
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return -1
    emitter.blank();
}

/// Emits a shim that converts SysV `read(fd, buf, len)` to Win32 `ReadFile`.
///
/// On failure (`ReadFile` returns `FALSE`), translates `GetLastError()` to a
/// POSIX errno via `__rt_win32_errno_from_code` (`shims_c_symbols.rs`),
/// stores it into `__rt_errno`, and returns -1 like POSIX `read()` — this is
/// what lets the `fgets`/`fread` EAGAIN-vs-EOF check (`__errno_location`,
/// compared against 11) tell a nonblocking would-block apart from a real
/// EOF/error. CRT descriptors marked `O_NONBLOCK` in the status cache first
/// probe named-pipe availability with `PeekNamedPipe`. PHP's Windows plain
/// wrapper polls a default nonblocking process pipe briefly before reporting
/// an empty read, so this shim waits in bounded one-millisecond intervals for
/// child output instead of racing the process start. The final `ReadFile`
/// request is capped to the availability result just as php-src caps `read()`.
/// On success errno is left untouched (POSIX does not clear it).
pub(super) fn emit_shim_read(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_read");
    emitter.instruction("sub rsp, 104");                                        // allocate shadow space, API out-params, arguments, and a pipe poll counter
    emitter.instruction("mov QWORD PTR [rsp + 72], rdx");                       // spill len (rdx is volatile, clobbered by the call) to a safe slot
    emitter.instruction("mov QWORD PTR [rsp + 80], rdi");                       // retain the CRT descriptor for cached-status lookup
    emitter.instruction("mov QWORD PTR [rsp + 88], rsi");                       // retain the destination buffer across status and Win32 calls
    emitter.instruction("test rdx, rdx");                                       // POSIX read(fd, buf, 0) must not inspect pipe readiness
    emitter.instruction("jnz .Lsys_read_nonempty");                             // nonzero requests may need native I/O
    emitter.instruction("xor eax, eax");                                        // zero-length reads succeed immediately, including O_NONBLOCK pipes
    emitter.instruction("add rsp, 104");                                        // restore the caller stack before the fast return
    emitter.instruction("ret");                                                 // return zero without calling PeekNamedPipe or ReadFile
    emitter.label(".Lsys_read_nonempty");
    emitter.instruction("mov rcx, rdi");                                        // fd for handle conversion
    emitter.instruction("call __rt_fd_to_handle");                              // convert fd to Win32 HANDLE
    emitter.instruction("cmp rax, -1");                                         // not a CRT file descriptor (socket resource)?
    emitter.instruction("je .Lsys_read_socket");                                // Winsock streams use recv(), not ReadFile
    emitter.instruction("mov QWORD PTR [rsp + 64], rax");                       // preserve the native HANDLE across the status lookup
    emitter.instruction("mov rdi, QWORD PTR [rsp + 80]");                       // cached status is keyed by the CRT descriptor
    emitter.instruction("call __rt_win_fd_status_find");                        // inspect F_GETFL-visible flags without changing ownership
    emitter.instruction("test rax, rax");                                       // did this descriptor receive explicit status flags?
    emitter.instruction("jz .Lsys_read_file");                                  // no cache entry: retain normal blocking ReadFile behavior
    emitter.instruction("test QWORD PTR [rax + 16], 0x800");                    // is O_NONBLOCK set in the cached Linux status flags?
    emitter.instruction("jz .Lsys_read_file");                                  // blocking descriptors may proceed directly to ReadFile
    // -- Windows anonymous pipes do not support overlapped I/O: poll like php-src before ReadFile --
    emitter.instruction("mov QWORD PTR [rsp + 96], 0");                         // no nonblocking pipe polls have elapsed yet
    emitter.label(".Lsys_read_nonblocking_pipe_probe");
    emitter.instruction("mov rcx, QWORD PTR [rsp + 64]");                       // hNamedPipe = converted parent pipe HANDLE
    emitter.instruction("xor edx, edx");                                        // lpBuffer = NULL (availability-only probe)
    emitter.instruction("xor r8d, r8d");                                        // nBufferSize = 0
    emitter.instruction("xor r9d, r9d");                                        // lpBytesRead = NULL
    emitter.instruction("lea rax, [rsp + 56]");                                 // address of total available-byte out-param
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // lpTotalBytesAvail (arg5)
    emitter.instruction("mov QWORD PTR [rsp + 40], 0");                         // lpBytesLeftThisMessage = NULL (arg6)
    emitter.instruction("call PeekNamedPipe");                                  // probe anonymous-pipe data without waiting
    emitter.instruction("test eax, eax");                                       // is this a readable named/anonymous pipe?
    emitter.instruction("jz .Lsys_read_file");                                  // non-pipe/error falls back to ReadFile's existing error behavior
    emitter.instruction("cmp DWORD PTR [rsp + 56], 0");                         // did the probe report immediately readable bytes?
    emitter.instruction("jne .Lsys_read_nonblocking_pipe_ready");               // cap the read to ready bytes before entering synchronous ReadFile
    emitter.instruction("inc QWORD PTR [rsp + 96]");                            // account for this empty nonblocking pipe probe
    emitter.instruction("cmp QWORD PTR [rsp + 96], 32000");                     // preserve php-src's roughly 32-second bounded wait
    emitter.instruction("jae .Lsys_read_nonblocking_pipe_ready");               // timeout issues PHP's zero-length read instead of blocking forever
    emitter.instruction("mov ecx, 1");                                          // sleep one millisecond before polling child output again
    emitter.instruction("call Sleep");                                          // yield while a Windows child initializes or flushes
    emitter.instruction("jmp .Lsys_read_nonblocking_pipe_probe");               // repeat until bytes, EOF, a non-pipe failure, or timeout
    emitter.label(".Lsys_read_nonblocking_pipe_ready");
    emitter.instruction("mov eax, DWORD PTR [rsp + 56]");                       // load PeekNamedPipe's currently available byte count
    emitter.instruction("mov rdx, QWORD PTR [rsp + 72]");                       // reload the caller's requested byte count
    emitter.instruction("cmp rdx, rax");                                        // is the PHP read request larger than the ready data?
    emitter.instruction("cmova rdx, rax");                                      // never let synchronous ReadFile wait for unavailable bytes
    emitter.instruction("mov QWORD PTR [rsp + 72], rdx");                       // carry the capped request into the shared ReadFile path
    emitter.label(".Lsys_read_file");
    emitter.instruction("mov rcx, QWORD PTR [rsp + 64]");                       // restore HANDLE after optional availability probe
    emitter.instruction("mov rdx, QWORD PTR [rsp + 88]");                       // restore caller destination buffer
    emitter.instruction("mov r8, QWORD PTR [rsp + 72]");                        // reload len (arg3) after helper calls
    emitter.instruction("mov QWORD PTR [rsp + 32], 0");                         // lpOverlapped = NULL (arg5 slot)
    emitter.instruction("lea r9, [rsp + 48]");                                  // &bytesRead (arg4)
    emitter.instruction("call ReadFile");                                       // ReadFile(handle, buf, len, &read, NULL)
    emitter.instruction("test eax, eax");                                       // BOOL result: 0 means the ReadFile call failed
    emitter.instruction("jz .Lsys_read_fail");                                  // failure: translate GetLastError and return -1
    emitter.instruction("mov eax, DWORD PTR [rsp + 48]");                       // success: return bytes read (DWORD out-param; zero-extend)
    emitter.instruction("add rsp, 104");                                        // restore stack
    emitter.instruction("ret");                                                 // return to caller
    emitter.label(".Lsys_read_socket");
    emitter.instruction("mov rcx, QWORD PTR [rsp + 80]");                       // SOCKET
    emitter.instruction("mov rdx, QWORD PTR [rsp + 88]");                       // destination buffer
    emitter.instruction("mov r8, QWORD PTR [rsp + 72]");                        // byte count
    emitter.instruction("xor r9d, r9d");                                        // flags = 0
    emitter.instruction("call recv");                                           // socket-stream read
    emitter.instruction("cmp eax, -1");                                         // SOCKET_ERROR?
    emitter.instruction("je .Lsys_read_socket_fail");                           // publish Winsock errno
    emitter.instruction("cdqe");                                                // signed byte count
    emitter.instruction("add rsp, 104");                                        // restore stack
    emitter.instruction("ret");                                                 // return byte count
    emitter.label(".Lsys_read_socket_fail");
    emitter.instruction("call __rt_wsa_capture_errno");                         // translate WSAGetLastError
    emitter.instruction("add rsp, 104");                                        // restore stack
    emitter.instruction("ret");                                                 // return -1
    emitter.label(".Lsys_read_fail");
    emitter.instruction("call GetLastError");                                   // fetch the Win32 code left by the failed ReadFile
    emitter.instruction("mov DWORD PTR [rip + __rt_win32_last_error], eax");    // preserve the native Win32 error separately from errno
    emitter.instruction("call __rt_win32_errno_from_code");                     // translate Win32 code (eax) to POSIX errno (eax)
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], eax");               // publish errno for __errno_location readers
    emitter.instruction("mov rax, -1");                                         // ReadFile failure: return -1 like POSIX read()
    emitter.instruction("add rsp, 104");                                        // restore stack
    emitter.instruction("ret");                                                 // return to caller
    emitter.blank();
}

/// Emits the bounded opaque-handle-to-stream-slot registry used by Windows I/O state.
///
/// `SOCKET` is a pointer-sized opaque value and can exceed every fixed runtime
/// table bound. The lookup returns a stable slot in `0..256` for each live
/// stream handle; a full registry deliberately falls back to slot zero so no
/// generated path can turn a native handle into an out-of-bounds address.
pub(super) fn emit_win_stream_slot_registry(emitter: &mut Emitter) {
    emitter.label_global("__rt_win_stream_slot");
    emitter.instruction("mov r8, rdi");                                         // retain the opaque descriptor while scanning registry state
    crate::codegen_support::abi::emit_symbol_address(emitter, "r9", "_win_stream_slot_handles"); // handle table base
    crate::codegen_support::abi::emit_symbol_address(emitter, "r10", "_win_stream_slot_used"); // occupancy table base
    emitter.instruction("xor ecx, ecx");                                        // scan slot index = 0
    emitter.instruction("mov rdx, -1");                                         // first-free slot sentinel
    emitter.label(".Lwin_stream_slot_scan");
    emitter.instruction("cmp ecx, 256");                                        // exhausted all bounded slots?
    emitter.instruction("je .Lwin_stream_slot_allocate");                       // allocate the remembered free slot or use fallback
    emitter.instruction("cmp BYTE PTR [r10 + rcx], 0");                         // occupied slot?
    emitter.instruction("je .Lwin_stream_slot_free");                           // remember the first available slot
    emitter.instruction("cmp QWORD PTR [r9 + rcx * 8], r8");                    // does this slot belong to the requested opaque handle?
    emitter.instruction("je .Lwin_stream_slot_found");                          // return the existing stable slot
    emitter.instruction("inc rcx");                                             // inspect the next entry
    emitter.instruction("jmp .Lwin_stream_slot_scan");                          // continue registry lookup
    emitter.label(".Lwin_stream_slot_free");
    emitter.instruction("cmp rdx, -1");                                         // already remembered a free slot?
    emitter.instruction("jne .Lwin_stream_slot_next");                          // retain the earliest free slot for deterministic reuse
    emitter.instruction("mov rdx, rcx");                                        // remember this free slot
    emitter.label(".Lwin_stream_slot_next");
    emitter.instruction("inc rcx");                                             // inspect remaining entries for an existing mapping
    emitter.instruction("jmp .Lwin_stream_slot_scan");                          // continue registry lookup
    emitter.label(".Lwin_stream_slot_found");
    emitter.instruction("mov rax, rcx");                                        // return the existing opaque-handle slot
    emitter.instruction("ret");                                                 // return to the stream-state caller
    emitter.label(".Lwin_stream_slot_allocate");
    emitter.instruction("cmp rdx, -1");                                         // a free slot was found?
    emitter.instruction("je .Lwin_stream_slot_full");                           // bounded registry is saturated
    emitter.instruction("mov QWORD PTR [r9 + rdx * 8], r8");                    // bind the requested handle to the free slot
    emitter.instruction("mov BYTE PTR [r10 + rdx], 1");                         // mark the slot live before any table consumer observes it
    emitter.instruction("mov rax, rdx");                                        // return the newly allocated stable slot
    emitter.instruction("ret");                                                 // return to the stream-state caller
    emitter.label(".Lwin_stream_slot_full");
    emitter.instruction("xor eax, eax");                                        // safe bounded fallback avoids opaque-handle table indexing
    emitter.instruction("ret");                                                 // return fallback slot zero
    emitter.blank();
}

/// Emits release of an opaque Windows stream slot when its descriptor closes.
///
/// The registry owns no OS handle; this helper only clears the handle mapping
/// and all state keyed by its compact slot so a recycled SOCKET cannot inherit
/// EOF or filter state from its predecessor.
pub(super) fn emit_win_stream_slot_clear(emitter: &mut Emitter) {
    emitter.label_global("__rt_win_stream_slot_clear");
    emitter.instruction("mov r8, rdi");                                         // preserve the closing opaque descriptor while scanning
    crate::codegen_support::abi::emit_symbol_address(emitter, "r9", "_win_stream_slot_handles"); // handle table base
    crate::codegen_support::abi::emit_symbol_address(emitter, "r10", "_win_stream_slot_used"); // occupancy table base
    emitter.instruction("xor ecx, ecx");                                        // begin at slot zero
    emitter.label(".Lwin_stream_slot_clear_scan");
    emitter.instruction("cmp ecx, 256");                                        // mapping was absent?
    emitter.instruction("je .Lwin_stream_slot_clear_done");                     // nothing to release
    emitter.instruction("cmp BYTE PTR [r10 + rcx], 0");                         // live registry entry?
    emitter.instruction("je .Lwin_stream_slot_clear_next");                     // skip unused entries
    emitter.instruction("cmp QWORD PTR [r9 + rcx * 8], r8");                    // entry belongs to this descriptor?
    emitter.instruction("je .Lwin_stream_slot_clear_found");                    // clear its compact stream state
    emitter.label(".Lwin_stream_slot_clear_next");
    emitter.instruction("inc rcx");                                             // inspect the next entry
    emitter.instruction("jmp .Lwin_stream_slot_clear_scan");                    // continue bounded lookup
    emitter.label(".Lwin_stream_slot_clear_found");
    emitter.instruction("mov BYTE PTR [r10 + rcx], 0");                         // release occupancy before descriptor reuse
    emitter.instruction("mov QWORD PTR [r9 + rcx * 8], 0");                     // discard stale opaque handle value
    crate::codegen_support::abi::emit_symbol_address(emitter, "r11", "_eof_flags"); // EOF state table base
    emitter.instruction("mov BYTE PTR [r11 + rcx], 0");                         // clear inherited EOF state
    crate::codegen_support::abi::emit_symbol_address(emitter, "r11", "_win_stream_timed_out"); // timeout state table base
    emitter.instruction("mov BYTE PTR [r11 + rcx], 0");                         // clear inherited timeout state
    crate::codegen_support::abi::emit_symbol_address(emitter, "r11", "_stream_read_filters"); // read filter table base
    emitter.instruction("mov BYTE PTR [r11 + rcx], 0");                         // clear inherited read filter state
    crate::codegen_support::abi::emit_symbol_address(emitter, "r11", "_stream_write_filters"); // write filter table base
    emitter.instruction("mov BYTE PTR [r11 + rcx], 0");                         // clear inherited write filter state
    emitter.label(".Lwin_stream_slot_clear_done");
    emitter.instruction("ret");                                                 // return without closing the descriptor itself
    emitter.blank();
}

/// Emits a bounded timeout-state update for the Windows stream descriptor in `rdi`.
pub(super) fn emit_win_stream_mark_timed_out(emitter: &mut Emitter) {
    emitter.label_global("__rt_win_stream_mark_timed_out");
    emitter.instruction("call __rt_win_stream_slot");                           // resolve the opaque descriptor to a bounded stream-state slot
    crate::codegen_support::abi::emit_symbol_address(emitter, "r10", "_win_stream_timed_out"); // timeout state table base
    emitter.instruction("mov BYTE PTR [r10 + rax], 1");                         // expose the most recent ETIMEDOUT through stream metadata
    emitter.instruction("ret");                                                 // return after recording the retryable timeout state
    emitter.blank();
}

/// Emits a bounded timeout-state reset for the Windows stream descriptor in `rdi`.
pub(super) fn emit_win_stream_clear_timed_out(emitter: &mut Emitter) {
    emitter.label_global("__rt_win_stream_clear_timed_out");
    emitter.instruction("call __rt_win_stream_slot");                           // resolve the opaque descriptor to a bounded stream-state slot
    crate::codegen_support::abi::emit_symbol_address(emitter, "r10", "_win_stream_timed_out"); // timeout state table base
    emitter.instruction("mov BYTE PTR [r10 + rax], 0");                         // a new stream operation clears the previous timeout outcome
    emitter.instruction("ret");                                                 // return after resetting timeout metadata
    emitter.blank();
}

/// Emits a shim that closes either a CRT file descriptor or a raw Winsock
/// SOCKET resource published by Windows `proc_open(["socket"])`.
pub(super) fn emit_shim_close(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_close");
    emitter.instruction("sub rsp, 56");                                         // shadow space and saved opaque descriptor
    emitter.instruction("mov QWORD PTR [rsp + 48], rdi");                       // retain descriptor across CRT probing
    emitter.instruction("mov ecx, edi");                                        // candidate CRT descriptor
    emitter.instruction("call _get_osfhandle");                                 // raw SOCKET values are not in the CRT fd table
    emitter.instruction("cmp rax, -1");                                         // did CRT recognize the descriptor?
    emitter.instruction("je .Lsys_close_socket");                               // raw SOCKET must use closesocket
    emitter.instruction("mov rdi, QWORD PTR [rsp + 48]");                       // restore CRT descriptor for metadata release
    emitter.instruction("call __rt_win_fd_status_clear");                       // discard cached fcntl flags before descriptor reuse
    emitter.instruction("mov rdi, QWORD PTR [rsp + 48]");                       // restore descriptor for compact stream-state release
    emitter.instruction("call __rt_user_filter_release_fd");                    // run user-filter onClose hooks while the opaque-handle mapping is live
    emitter.instruction("mov rdi, QWORD PTR [rsp + 48]");                       // restore descriptor after user-filter lifecycle callbacks
    emitter.instruction("call __rt_win_stream_slot_clear");                     // release bounded EOF/filter slot before CRT descriptor reuse
    emitter.instruction("mov ecx, DWORD PTR [rsp + 48]");                       // CRT file descriptor
    emitter.instruction("call _close");                                         // close descriptor and its underlying HANDLE
    emitter.instruction("cdqe");                                                // preserve msvcrt's signed close result
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return CRT close status
    emitter.label(".Lsys_close_socket");
    emitter.instruction("mov rdi, QWORD PTR [rsp + 48]");                       // raw SOCKET metadata key
    emitter.instruction("call __rt_win_fd_status_clear");                       // discard any cached socket status without table indexing
    emitter.instruction("mov rdi, QWORD PTR [rsp + 48]");                       // restore raw SOCKET for compact stream-state release
    emitter.instruction("call __rt_user_filter_release_fd");                    // run user-filter onClose hooks while the opaque-handle mapping is live
    emitter.instruction("mov rdi, QWORD PTR [rsp + 48]");                       // restore raw SOCKET after user-filter lifecycle callbacks
    emitter.instruction("call __rt_win_stream_slot_clear");                     // release bounded EOF/filter slot before SOCKET reuse
    emitter.instruction("mov rcx, QWORD PTR [rsp + 48]");                       // SOCKET argument for Winsock
    emitter.instruction("call closesocket");                                    // release the raw parent socket exactly once
    emitter.instruction("cmp eax, -1");                                         // SOCKET_ERROR?
    emitter.instruction("je .Lsys_close_socket_fail");                          // translate Winsock error
    emitter.instruction("cdqe");                                                // return zero on successful close
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return success
    emitter.label(".Lsys_close_socket_fail");
    emitter.instruction("call __rt_wsa_capture_errno");                         // publish WSAGetLastError as POSIX errno
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return -1 from errno helper
    emitter.blank();
}

/// Emits a shim that fills a Linux-layout `struct stat` buffer for an open fd.
///
/// SysV: rdi=fd (a CRT descriptor in this runtime), rsi=stat buffer.
/// Converts the descriptor to a HANDLE and reads `BY_HANDLE_FILE_INFORMATION`,
/// which supplies the size, attributes, volume, file index, link count, and
/// timestamps for ordinary files and directories. Pipe and character handles
/// do not universally support that query, so `GetFileType` provides a
/// type-correct zero-metadata fallback. Uses Win32 directly instead of msvcrt
/// `fstat`: msvcrt expects a CRT layout that differs from the runtime Linux
/// layout, and the `fstat` C-symbol name is a local shim stub.
pub(super) fn emit_shim_fstat(emitter: &mut Emitter) {
    let mode_off = emitter.platform.stat_mode_offset();
    let size_off = emitter.platform.stat_size_offset();
    let nlink_off = emitter.platform.stat_nlink_offset();
    let ino_off = emitter.platform.stat_ino_offset();
    let dev_off = emitter.platform.stat_dev_offset();
    let atime_off = emitter.platform.stat_atime_offset();
    let mtime_off = emitter.platform.stat_mtime_offset();
    let ctime_off = emitter.platform.stat_ctime_offset();
    emitter.label_global("__rt_sys_fstat");
    emitter.instruction("sub rsp, 120");                                        // shadow(32), BY_HANDLE_FILE_INFORMATION(52), and aligned local slots
    emitter.instruction("mov QWORD PTR [rsp + 88], rsi");                       // preserve the SysV stat destination across helper calls
    emitter.instruction("call __rt_fd_to_handle");                              // convert the CRT descriptor to its Win32 HANDLE
    emitter.instruction("cmp rax, -1");                                         // invalid CRT descriptor?
    emitter.instruction("je .Lfstat_bad_fd");                                   // publish EBADF without querying Win32
    emitter.instruction("mov QWORD PTR [rsp + 96], rax");                       // retain the handle across metadata calls
    emitter.instruction("cld");                                                 // forward direction for rep stosb
    emitter.instruction("lea rdi, [rsp + 32]");                                 // destination = BY_HANDLE_FILE_INFORMATION buffer
    emitter.instruction("xor eax, eax");                                        // zero fill byte
    emitter.instruction("mov ecx, 52");                                         // sizeof(BY_HANDLE_FILE_INFORMATION)
    emitter.instruction("rep stosb");                                           // clear the metadata buffer before Win32 fills it
    emitter.instruction("mov rcx, QWORD PTR [rsp + 96]");                       // hFile = converted descriptor handle
    emitter.instruction("lea rdx, [rsp + 32]");                                 // lpFileInformation = metadata buffer
    emitter.instruction("call GetFileInformationByHandle");                     // query real file metadata from the open handle
    emitter.instruction("test eax, eax");                                       // did the metadata query succeed?
    emitter.instruction("jnz .Lfstat_metadata");                                // populate every available stat field
    emitter.instruction("mov rcx, QWORD PTR [rsp + 96]");                       // hFile for the pipe/console fallback query
    emitter.instruction("call GetFileType");                                    // identify handles without file-information support
    emitter.instruction("test eax, eax");                                       // FILE_TYPE_UNKNOWN (0)?
    emitter.instruction("jz .Lfstat_type_fail");                                // preserve and translate the native failure
    emitter.instruction("mov DWORD PTR [rsp + 104], eax");                      // retain FILE_TYPE_* while the stat buffer is cleared
    // -- zero the Linux-layout stat buffer before filling fields --
    emitter.instruction("cld");                                                 // forward direction for rep stosb
    emitter.instruction("mov rsi, QWORD PTR [rsp + 88]");                       // restore the caller's stat destination
    emitter.instruction("mov rdi, rsi");                                        // dest = stat buffer base
    emitter.instruction("xor eax, eax");                                        // zero fill byte
    emitter.instruction("mov ecx, 128");                                        // Linux struct stat size in bytes
    emitter.instruction("rep stosb");                                           // zero the whole stat buffer
    // -- type-correct fallback for pipe and character handles --
    emitter.instruction("mov eax, DWORD PTR [rsp + 104]");                      // FILE_TYPE_* result from GetFileType
    emitter.instruction("cmp eax, 3");                                          // FILE_TYPE_PIPE?
    emitter.instruction("je .Lfstat_pipe_mode");                                // synthesize S_IFIFO
    emitter.instruction("cmp eax, 2");                                          // FILE_TYPE_CHAR?
    emitter.instruction("je .Lfstat_char_mode");                                // synthesize S_IFCHR
    emitter.instruction("mov eax, 0x81B6");                                     // FILE_TYPE_DISK fallback = S_IFREG | 0666
    emitter.instruction("jmp .Lfstat_fallback_mode_done");                      // store the disk fallback mode
    emitter.label(".Lfstat_pipe_mode");
    emitter.instruction("mov eax, 0x11B6");                                     // st_mode = S_IFIFO | 0666
    emitter.instruction("jmp .Lfstat_fallback_mode_done");                      // avoid the character fallback
    emitter.label(".Lfstat_char_mode");
    emitter.instruction("mov eax, 0x21B6");                                     // st_mode = S_IFCHR | 0666
    emitter.label(".Lfstat_fallback_mode_done");
    emitter.instruction(&format!("mov DWORD PTR [rsi + {}], eax", mode_off));   // store the fallback object type and permissions
    emitter.instruction(&format!("mov DWORD PTR [rsi + {}], 1", nlink_off));    // synthetic handles have one observable link
    emitter.instruction(&format!("mov QWORD PTR [rsi + {}], 1", dev_off));      // stable nonzero synthetic device id
    emitter.instruction("xor eax, eax");                                        // return success with the best available metadata
    emitter.instruction("add rsp, 120");                                        // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lfstat_metadata");
    emitter.instruction("mov rcx, QWORD PTR [rsp + 96]");                       // hFile for type classification alongside real metadata
    emitter.instruction("call GetFileType");                                    // distinguish disk files from pipes and character devices
    emitter.instruction("mov DWORD PTR [rsp + 104], eax");                      // retain FILE_TYPE_* across the stat-buffer zero fill
    // -- zero the Linux-layout stat buffer before filling real metadata --
    emitter.instruction("cld");                                                 // forward direction for rep stosb
    emitter.instruction("mov rsi, QWORD PTR [rsp + 88]");                       // restore the caller's stat destination
    emitter.instruction("mov rdi, rsi");                                        // dest = stat buffer base
    emitter.instruction("xor eax, eax");                                        // zero fill byte
    emitter.instruction("mov ecx, 128");                                        // Linux struct stat size in bytes
    emitter.instruction("rep stosb");                                           // zero the whole stat buffer
    // -- st_size, st_dev, st_ino, and st_nlink from BY_HANDLE_FILE_INFORMATION --
    emitter.instruction("mov eax, DWORD PTR [rsp + 68]");                       // nFileSizeLow
    emitter.instruction("mov edx, DWORD PTR [rsp + 64]");                       // nFileSizeHigh
    emitter.instruction("shl rdx, 32");                                         // position the size high dword
    emitter.instruction("or rax, rdx");                                         // combine into the full 64-bit size
    emitter.instruction(&format!("mov QWORD PTR [rsi + {}], rax", size_off));   // store st_size
    emitter.instruction("mov eax, DWORD PTR [rsp + 60]");                       // dwVolumeSerialNumber
    emitter.instruction("test eax, eax");                                       // Wine can expose zero for synthetic volumes
    emitter.instruction("jnz .Lfstat_dev_ready");                               // preserve a real volume serial
    emitter.instruction("mov eax, 1");                                          // stable nonzero device fallback
    emitter.label(".Lfstat_dev_ready");
    emitter.instruction(&format!("mov QWORD PTR [rsi + {}], rax", dev_off));    // store st_dev
    emitter.instruction("mov eax, DWORD PTR [rsp + 76]");                       // nFileIndexHigh
    emitter.instruction("shl rax, 32");                                         // position the inode high dword
    emitter.instruction("mov ecx, DWORD PTR [rsp + 80]");                       // nFileIndexLow
    emitter.instruction("or rax, rcx");                                         // combine into a 64-bit inode number
    emitter.instruction(&format!("mov QWORD PTR [rsi + {}], rax", ino_off));    // store st_ino
    emitter.instruction("mov eax, DWORD PTR [rsp + 72]");                       // nNumberOfLinks
    emitter.instruction("test eax, eax");                                       // zero is not a useful POSIX link count
    emitter.instruction("jnz .Lfstat_nlink_ready");                             // retain a native nonzero count
    emitter.instruction("mov eax, 1");                                          // synthesize a minimum valid link count
    emitter.label(".Lfstat_nlink_ready");
    emitter.instruction(&format!("mov DWORD PTR [rsi + {}], eax", nlink_off));  // store st_nlink
    // -- st_mode: file/directory/pipe/character type and readonly permission bits --
    emitter.instruction("mov edx, DWORD PTR [rsp + 32]");                       // dwFileAttributes
    emitter.instruction("mov ecx, DWORD PTR [rsp + 104]");                      // FILE_TYPE_* from the open handle
    emitter.instruction("cmp ecx, 3");                                          // FILE_TYPE_PIPE?
    emitter.instruction("je .Lfstat_metadata_pipe_mode");                       // pipes are FIFO objects, not regular files
    emitter.instruction("cmp ecx, 2");                                          // FILE_TYPE_CHAR?
    emitter.instruction("je .Lfstat_metadata_char_mode");                       // consoles and devices are character objects
    emitter.instruction("mov eax, 0x81B6");                                     // default st_mode = S_IFREG | 0666
    emitter.instruction("test edx, 0x10");                                      // FILE_ATTRIBUTE_DIRECTORY set?
    emitter.instruction("jz .Lfstat_mode_readonly");                            // non-directory keeps regular-file type
    emitter.instruction("mov eax, 0x41FF");                                     // st_mode = S_IFDIR | 0777
    emitter.instruction("jmp .Lfstat_mode_readonly");                           // apply readonly bits uniformly after type selection
    emitter.label(".Lfstat_metadata_pipe_mode");
    emitter.instruction("mov eax, 0x11B6");                                     // st_mode = S_IFIFO | 0666
    emitter.instruction("jmp .Lfstat_mode_readonly");                           // apply readonly bits uniformly after type selection
    emitter.label(".Lfstat_metadata_char_mode");
    emitter.instruction("mov eax, 0x21B6");                                     // st_mode = S_IFCHR | 0666
    emitter.label(".Lfstat_mode_readonly");
    emitter.instruction("test edx, 1");                                         // FILE_ATTRIBUTE_READONLY set?
    emitter.instruction("jz .Lfstat_mode_done");                                // writable objects retain synthesized write bits
    emitter.instruction("and eax, 0xFF6D");                                     // readonly objects clear 0222 while retaining type/read bits
    emitter.label(".Lfstat_mode_done");
    emitter.instruction(&format!("mov DWORD PTR [rsi + {}], eax", mode_off));   // store st_mode
    // -- timestamps: convert FILETIME values to Unix epoch seconds --
    emit_filetime_to_stat_seconds(emitter, 44, atime_off);
    emit_filetime_to_stat_seconds(emitter, 52, mtime_off);
    emit_filetime_to_stat_seconds(emitter, 36, ctime_off);
    emitter.instruction("xor eax, eax");                                        // return 0 (success)
    emitter.instruction("add rsp, 120");                                        // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lfstat_type_fail");
    emitter.instruction("call GetLastError");                                   // capture the fallback query failure
    emitter.instruction("mov DWORD PTR [rip + __rt_win32_last_error], eax");    // retain the native diagnostic
    emitter.instruction("call __rt_win32_errno_from_code");                     // translate the native error to POSIX errno
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], eax");               // publish observable errno
    emitter.instruction("mov rax, -1");                                         // return -1 on failure
    emitter.instruction("add rsp, 120");                                        // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lfstat_bad_fd");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 9");                 // EBADF: descriptor has no Win32 HANDLE
    emitter.instruction("mov rax, -1");                                         // return -1 for an invalid descriptor
    emitter.instruction("add rsp, 120");                                        // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits a shim that converts `open(path, flags, mode)` to `CreateFileW`.
///
/// SysV: rdi=path, rsi=flags, rdx=mode.
/// Maps Linux open flags to Win32 `CreateFileW` parameters after strict UTF-8 conversion:
/// - O_RDONLY(0) → GENERIC_READ, OPEN_EXISTING
/// - O_WRONLY(1) → GENERIC_WRITE, OPEN_EXISTING
/// - O_RDWR(2) → GENERIC_READ|GENERIC_WRITE, OPEN_EXISTING
/// - O_CREAT(0x40) → OPEN_ALWAYS (or CREATE_ALWAYS with O_TRUNC)
/// - O_CREAT|O_EXCL(0xC0) → CREATE_NEW
/// - O_TRUNC(0x200) → TRUNCATE_EXISTING (or CREATE_ALWAYS with O_CREAT)
/// - O_APPEND(0x400) → FILE_APPEND_DATA
pub(super) fn emit_shim_open(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_open");
    emitter.instruction("sub rsp, 88");                                         // shadow, CreateFile stack args, and aligned owned-path locals
    emitter.instruction("mov QWORD PTR [rsp + 64], rsi");                       // preserve flags across strict conversion
    emitter.instruction("call __rt_win_utf8_to_utf16");                         // convert the PHP UTF-8 path to an owned wide path
    emitter.instruction("test rax, rax");                                       // conversion succeeded?
    emitter.instruction("jz .Lopen_conversion_fail");                           // invalid UTF-8 path
    emitter.instruction("mov QWORD PTR [rsp + 56], rax");                       // retain the owned wide path through CreateFileW
    emitter.instruction("mov rsi, QWORD PTR [rsp + 64]");                       // restore Linux open flags
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
    emitter.instruction("test rsi, 0x80");                                      // O_EXCL with O_CREAT?
    emitter.instruction("jnz .Lopen_create_new");                               // → CREATE_NEW, never replace an existing file
    emitter.instruction("test rsi, 0x200");                                     // O_CREAT + O_TRUNC?
    emitter.instruction("jnz .Lopen_create_always");                            // → CREATE_ALWAYS
    emitter.instruction("mov r10, 4");                                          // OPEN_ALWAYS (create or open)
    emitter.instruction("jmp .Lopen_disp_done");                                // → proceed
    emitter.label(".Lopen_create_always");
    emitter.instruction("mov r10, 2");                                          // CREATE_ALWAYS
    emitter.instruction("jmp .Lopen_disp_done");                                // → proceed
    emitter.label(".Lopen_create_new");
    emitter.instruction("mov r10, 1");                                          // CREATE_NEW: PHP fopen('x') must fail when the path exists
    emitter.instruction("jmp .Lopen_disp_done");                                // → proceed
    emitter.label(".Lopen_no_creat");
    emitter.instruction("test rsi, 0x200");                                     // O_TRUNC without O_CREAT?
    emitter.instruction("jnz .Lopen_trunc_existing");                           // → TRUNCATE_EXISTING
    emitter.instruction("mov r10, 3");                                          // OPEN_EXISTING
    emitter.instruction("jmp .Lopen_disp_done");                                // → proceed
    emitter.label(".Lopen_trunc_existing");
    emitter.instruction("mov r10, 5");                                          // TRUNCATE_EXISTING
    emitter.label(".Lopen_disp_done");
    // -- call CreateFileW(rcx=path, rdx=access, r8=share, r9=NULL, [rsp+32]=disp, [rsp+40]=0, [rsp+48]=0) --
    emitter.instruction("mov rcx, QWORD PTR [rsp + 56]");                       // lpFileName = owned UTF-16 path
    emitter.instruction("mov rdx, rax");                                        // dwDesiredAccess
    emitter.instruction("mov r8, 3");                                           // FILE_SHARE_READ | FILE_SHARE_WRITE
    emitter.instruction("xor r9, r9");                                          // lpSecurityAttributes = NULL
    emitter.instruction("mov QWORD PTR [rsp + 32], r10");                       // dwCreationDisposition
    emitter.instruction("mov QWORD PTR [rsp + 40], 0");                         // dwFlagsAndAttributes = 0
    emitter.instruction("mov QWORD PTR [rsp + 48], 0");                         // hTemplateFile = NULL
    emitter.instruction("call CreateFileW");                                    // open the Unicode path
    emitter.instruction("cmp rax, -1");                                         // INVALID_HANDLE_VALUE?
    emitter.instruction("je .Lopen_fail");                                      // capture native failure before cleanup
    emitter.instruction("mov QWORD PTR [rsp + 72], rax");                       // preserve the file handle across cleanup
    emitter.instruction("mov rax, QWORD PTR [rsp + 56]");                       // owned wide path
    emitter.instruction("call __rt_heap_free");                                 // release the converted path
    emitter.instruction("mov rcx, QWORD PTR [rsp + 72]");                       // Win32 handle transferred into the CRT table
    emitter.instruction("mov edx, 0x8000");                                     // default PHP fopen mode is CRT _O_BINARY
    emitter.instruction("test DWORD PTR [rsp + 64], 0x4000");                   // did PHP's `t` modifier request CRT text translation?
    emitter.instruction("jz .Lopen_crt_text_mode_done");                        // retain binary mode unless text was explicit
    emitter.instruction("mov edx, 0x4000");                                     // _O_TEXT enables the Windows CRT text-mode behavior
    emitter.label(".Lopen_crt_text_mode_done");
    emitter.instruction("mov eax, DWORD PTR [rsp + 64]");                       // Linux access mode flags
    emitter.instruction("and eax, 3");                                          // O_RDONLY/O_WRONLY/O_RDWR match the CRT values
    emitter.instruction("or edx, eax");                                         // preserve the caller's access mode
    emitter.instruction("test DWORD PTR [rsp + 64], 0x400");                    // Linux O_APPEND?
    emitter.instruction("jz .Lopen_crt_flags_done");                            // no CRT append flag needed
    emitter.instruction("or edx, 8");                                           // _O_APPEND
    emitter.label(".Lopen_crt_flags_done");
    emitter.instruction("call _open_osfhandle");                                // adopt HANDLE as a CRT file descriptor
    emitter.instruction("cmp eax, -1");                                         // CRT descriptor allocation failed?
    emitter.instruction("je .Lopen_crt_fail");                                  // close the still-owned raw handle
    emitter.instruction("mov QWORD PTR [rsp + 80], rax");                       // preserve the new descriptor while caching status flags
    emitter.instruction("mov rdi, rax");                                        // registry key = CRT descriptor
    emitter.instruction("mov rsi, QWORD PTR [rsp + 64]");                       // registry value = original Linux open flags
    emitter.instruction("call __rt_win_fd_status_upsert");                      // retain access mode for F_GETFL metadata
    emitter.instruction("mov rax, QWORD PTR [rsp + 80]");                       // restore the descriptor result
    emitter.instruction("add rsp, 88");                                         // restore stack
    emitter.instruction("ret");                                                 // return CRT file descriptor
    emitter.label(".Lopen_crt_fail");
    emitter.instruction("mov rcx, QWORD PTR [rsp + 72]");                       // raw HANDLE was not adopted on failure
    emitter.instruction("call CloseHandle");                                    // avoid leaking the failed descriptor's handle
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 24");                // EMFILE: CRT descriptor table allocation failed
    emitter.instruction("mov rax, -1");                                         // POSIX open failure
    emitter.instruction("add rsp, 88");                                         // restore stack
    emitter.instruction("ret");                                                 // return failure
    emitter.label(".Lopen_fail");
    emitter.instruction("call GetLastError");                                   // fetch native failure before cleanup can clobber it
    emitter.instruction("mov DWORD PTR [rsp + 80], eax");                       // preserve the native error code
    emitter.instruction("mov rax, QWORD PTR [rsp + 56]");                       // owned wide path
    emitter.instruction("call __rt_heap_free");                                 // release the converted path on failure
    emitter.instruction("mov eax, DWORD PTR [rsp + 80]");                       // restore native error code
    emitter.instruction("mov DWORD PTR [rip + __rt_win32_last_error], eax");    // retain native Win32 state
    emitter.instruction("call __rt_win32_errno_from_code");                     // translate to POSIX errno
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], eax");               // publish observable errno
    emitter.instruction("mov rax, -1");                                         // POSIX open failure
    emitter.instruction("add rsp, 88");                                         // restore stack
    emitter.instruction("ret");                                                 // return failure
    emitter.label(".Lopen_conversion_fail");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 84");                // EILSEQ: invalid UTF-8 path
    emitter.instruction("mov rax, -1");                                         // POSIX open failure
    emitter.instruction("add rsp, 88");                                         // restore stack
    emitter.instruction("ret");                                                 // return failure
    emitter.blank();
}

/// Emits a shim that converts `lseek` to the 64-bit `SetFilePointerEx` API.
///
/// SysV: rdi=fd, rsi=offset, rdx=whence.
/// Maps SEEK_SET(0)→FILE_BEGIN(0), SEEK_CUR(1)→FILE_CURRENT(1), SEEK_END(2)→FILE_END(2).
/// The signed 64-bit distance and resulting absolute position are preserved
/// without the 2 GiB truncation of the legacy `SetFilePointer` shim. Native
/// failures are translated into the runtime errno channel and return `-1`.
pub(super) fn emit_shim_lseek(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_lseek");
    emitter.instruction("sub rsp, 72");                                         // shadow space plus aligned offset/whence/result/error locals
    emitter.instruction("mov QWORD PTR [rsp + 32], rsi");                       // preserve signed 64-bit distance across handle conversion
    emitter.instruction("mov QWORD PTR [rsp + 40], rdx");                       // preserve whence across handle conversion
    emitter.instruction("mov rcx, rdi");                                        // fd
    emitter.instruction("call __rt_fd_to_handle");                              // convert to HANDLE
    emitter.instruction("mov rcx, rax");                                        // handle
    emitter.instruction("mov rdx, QWORD PTR [rsp + 32]");                       // signed LARGE_INTEGER distance
    emitter.instruction("lea r8, [rsp + 48]");                                  // receive the full-width new file position
    emitter.instruction("mov r9d, DWORD PTR [rsp + 40]");                       // FILE_BEGIN/FILE_CURRENT/FILE_END
    emitter.instruction("call SetFilePointerEx");                               // seek without truncating offsets above 2 GiB
    emitter.instruction("test eax, eax");                                       // Win32 BOOL success?
    emitter.instruction("jz .Llseek_fail");                                     // translate native failure
    emitter.instruction("mov rax, QWORD PTR [rsp + 48]");                       // return the 64-bit absolute position
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return new position
    emitter.label(".Llseek_fail");
    emitter.instruction("call GetLastError");                                   // capture the native seek failure
    emitter.instruction("mov DWORD PTR [rip + __rt_win32_last_error], eax");    // retain native Win32 state
    emitter.instruction("call __rt_win32_errno_from_code");                     // translate failure to POSIX errno
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], eax");               // publish observable errno
    emitter.instruction("mov rax, -1");                                         // POSIX lseek failure
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return failure
    emitter.blank();
}

/// Emits the bounded descriptor-status registry used by the Win32 fcntl shim.
pub(super) fn emit_win_fd_status_registry(emitter: &mut Emitter) {
    emitter.label_global("__rt_win_fd_status_find");
    emitter.instruction("lea rax, [rip + _win_fd_status_records]");             // first cached descriptor-status record
    emitter.instruction("mov ecx, 64");                                         // bounded registry capacity
    emitter.label(".Lwin_fd_status_find_loop");
    emitter.instruction("cmp QWORD PTR [rax], 0");                              // active record?
    emitter.instruction("je .Lwin_fd_status_find_next");                        // skip unused records
    emitter.instruction("cmp QWORD PTR [rax + 8], rdi");                        // same descriptor or opaque SOCKET?
    emitter.instruction("je .Lwin_fd_status_find_done");                        // return the matching record
    emitter.label(".Lwin_fd_status_find_next");
    emitter.instruction("add rax, 24");                                         // advance to the next record
    emitter.instruction("dec ecx");                                             // one fewer record remains
    emitter.instruction("jnz .Lwin_fd_status_find_loop");                       // scan the bounded table
    emitter.instruction("xor eax, eax");                                        // no cached status exists
    emitter.label(".Lwin_fd_status_find_done");
    emitter.instruction("ret");                                                 // return record or NULL
    emitter.blank();

    emitter.label_global("__rt_win_fd_status_upsert");
    emitter.instruction("lea rax, [rip + _win_fd_status_records]");             // first cached descriptor-status record
    emitter.instruction("xor r8d, r8d");                                        // no free record selected yet
    emitter.instruction("mov ecx, 64");                                         // bounded registry capacity
    emitter.label(".Lwin_fd_status_upsert_loop");
    emitter.instruction("cmp QWORD PTR [rax], 0");                              // unused record?
    emitter.instruction("jne .Lwin_fd_status_upsert_match");                    // occupied records may match the key
    emitter.instruction("test r8, r8");                                         // already retained an earlier free record?
    emitter.instruction("cmovz r8, rax");                                       // remember the first available slot
    emitter.instruction("jmp .Lwin_fd_status_upsert_next");                     // continue looking for an existing key
    emitter.label(".Lwin_fd_status_upsert_match");
    emitter.instruction("cmp QWORD PTR [rax + 8], rdi");                        // same descriptor or opaque SOCKET?
    emitter.instruction("je .Lwin_fd_status_upsert_store");                     // overwrite its status flags
    emitter.label(".Lwin_fd_status_upsert_next");
    emitter.instruction("add rax, 24");                                         // advance to the next record
    emitter.instruction("dec ecx");                                             // one fewer record remains
    emitter.instruction("jnz .Lwin_fd_status_upsert_loop");                     // scan the bounded table
    emitter.instruction("mov rax, r8");                                         // use the first free record, if any
    emitter.instruction("test rax, rax");                                       // registry exhausted?
    emitter.instruction("jz .Lwin_fd_status_upsert_done");                      // silently leave status uncached
    emitter.label(".Lwin_fd_status_upsert_store");
    emitter.instruction("mov QWORD PTR [rax], 1");                              // mark the record active
    emitter.instruction("mov QWORD PTR [rax + 8], rdi");                        // cache descriptor/SOCKET key
    emitter.instruction("mov QWORD PTR [rax + 16], rsi");                       // cache Linux status flags
    emitter.label(".Lwin_fd_status_upsert_done");
    emitter.instruction("ret");                                                 // return record or NULL on exhaustion
    emitter.blank();

    emitter.label_global("__rt_win_fd_status_clear");
    emitter.instruction("sub rsp, 8");                                          // align the internal lookup call
    emitter.instruction("call __rt_win_fd_status_find");                        // find cached state for the closing descriptor
    emitter.instruction("add rsp, 8");                                          // restore the caller stack
    emitter.instruction("test rax, rax");                                       // matching record found?
    emitter.instruction("jz .Lwin_fd_status_clear_done");                       // nothing was cached
    emitter.instruction("mov QWORD PTR [rax], 0");                              // release the registry record
    emitter.label(".Lwin_fd_status_clear_done");
    emitter.instruction("ret");                                                 // return after cache invalidation
    emitter.blank();
}

/// Emits a Win32 implementation of Linux `fcntl(F_GETFL/F_SETFL)`.
///
/// File access flags are retained from `open`; socket nonblocking changes are
/// applied with `ioctlsocket(FIONBIO)` and cached for subsequent F_GETFL calls.
pub(super) fn emit_shim_fcntl(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_fcntl");
    emitter.instruction("sub rsp, 72");                                         // shadow space plus descriptor, command, flags, and ioctl value
    emitter.instruction("mov QWORD PTR [rsp + 32], rdi");                       // preserve descriptor/SOCKET
    emitter.instruction("mov QWORD PTR [rsp + 40], rsi");                       // preserve fcntl command
    emitter.instruction("mov QWORD PTR [rsp + 48], rdx");                       // preserve proposed status flags
    emitter.instruction("cmp esi, 3");                                          // F_GETFL?
    emitter.instruction("je .Lfcntl_getfl");                                    // return cached status flags
    emitter.instruction("cmp esi, 4");                                          // F_SETFL?
    emitter.instruction("je .Lfcntl_setfl");                                    // update file/socket status flags
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 22");                // EINVAL for unsupported fcntl commands
    emitter.instruction("mov rax, -1");                                         // unsupported operation
    emitter.instruction("jmp .Lfcntl_done");                                    // return failure
    emitter.label(".Lfcntl_getfl");
    emitter.instruction("call __rt_win_fd_status_find");                        // query cached flags for this descriptor
    emitter.instruction("test rax, rax");                                       // cached record available?
    emitter.instruction("jnz .Lfcntl_getfl_cached");                            // return its exact Linux flags
    emitter.instruction("mov rdi, QWORD PTR [rsp + 32]");                       // restore descriptor for HANDLE detection
    emitter.instruction("mov rcx, rdi");                                        // CRT descriptor argument
    emitter.instruction("call _get_osfhandle");                                 // distinguish CRT descriptors from Winsock SOCKETs
    emitter.instruction("cmp rax, -1");                                         // socket/invalid descriptor has no CRT HANDLE
    emitter.instruction("je .Lfcntl_getfl_socket");                             // sockets are read-write and initially blocking
    emitter.instruction("xor esi, esi");                                        // uncached CRT descriptor defaults to O_RDONLY
    emitter.instruction("jmp .Lfcntl_getfl_seed");                              // cache the inferred flags
    emitter.label(".Lfcntl_getfl_socket");
    emitter.instruction("mov esi, 2");                                          // Winsock sockets have O_RDWR semantics
    emitter.label(".Lfcntl_getfl_seed");
    emitter.instruction("mov rdi, QWORD PTR [rsp + 32]");                       // registry key
    emitter.instruction("call __rt_win_fd_status_upsert");                      // cache inferred initial state
    emitter.instruction("mov rax, rsi");                                        // return inferred Linux status flags
    emitter.instruction("jmp .Lfcntl_done");                                    // finish F_GETFL
    emitter.label(".Lfcntl_getfl_cached");
    emitter.instruction("mov rax, QWORD PTR [rax + 16]");                       // return cached Linux status flags
    emitter.instruction("jmp .Lfcntl_done");                                    // finish F_GETFL
    emitter.label(".Lfcntl_setfl");
    emitter.instruction("mov rcx, QWORD PTR [rsp + 32]");                       // descriptor for CRT HANDLE detection
    emitter.instruction("call _get_osfhandle");                                 // files need cache-only changes; sockets need FIONBIO
    emitter.instruction("cmp rax, -1");                                         // no CRT HANDLE means a Winsock SOCKET
    emitter.instruction("jne .Lfcntl_setfl_cache");                             // regular files do not need a native nonblocking operation
    emitter.instruction("mov rax, QWORD PTR [rsp + 48]");                       // proposed Linux status flags
    emitter.instruction("shr rax, 11");                                         // move O_NONBLOCK (0x800) to bit zero
    emitter.instruction("and eax, 1");                                          // ULONG mode = 0/1
    emitter.instruction("mov DWORD PTR [rsp + 56], eax");                       // stable FIONBIO value storage
    emitter.instruction("mov rcx, QWORD PTR [rsp + 32]");                       // Winsock SOCKET
    emitter.instruction("mov edx, 0x8004667e");                                 // FIONBIO
    emitter.instruction("lea r8, [rsp + 56]");                                  // pointer to ULONG nonblocking mode
    emitter.instruction("call ioctlsocket");                                    // update native socket blocking state
    emitter.instruction("cmp eax, -1");                                         // SOCKET_ERROR?
    emitter.instruction("je .Lfcntl_setfl_fail");                               // publish Winsock failure
    emitter.label(".Lfcntl_setfl_cache");
    emitter.instruction("mov rdi, QWORD PTR [rsp + 32]");                       // registry key
    emitter.instruction("mov rsi, QWORD PTR [rsp + 48]");                       // updated Linux status flags
    emitter.instruction("call __rt_win_fd_status_upsert");                      // preserve F_GETFL-visible state
    emitter.instruction("xor eax, eax");                                        // F_SETFL success
    emitter.instruction("jmp .Lfcntl_done");                                    // return success
    emitter.label(".Lfcntl_setfl_fail");
    emitter.instruction("call __rt_wsa_capture_errno");                         // translate WSAGetLastError and return -1
    emitter.label(".Lfcntl_done");
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return fcntl result
    emitter.blank();
}

/// Emits a shim that converts `unlink` to `DeleteFileW`.
pub(super) fn emit_shim_unlink(emitter: &mut Emitter) {
    emit_wide_path_bool_shim(emitter, "__rt_sys_unlink", "DeleteFileW", false);
}

/// Emits a POSIX `getcwd` shim backed by a dynamically sized `GetCurrentDirectoryW` buffer.
pub(super) fn emit_shim_getcwd(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_getcwd");
    emitter.instruction("sub rsp, 88");                                         // shadow space and aligned destination/wide/error locals
    emitter.instruction("mov QWORD PTR [rsp + 32], rdi");                       // preserve caller UTF-8 destination
    emitter.instruction("mov QWORD PTR [rsp + 40], rsi");                       // preserve destination byte capacity
    emitter.instruction("xor ecx, ecx");                                        // query the required WCHAR capacity
    emitter.instruction("xor edx, edx");                                        // no destination for the size query
    emitter.instruction("call GetCurrentDirectoryW");                           // required WCHAR count including NUL
    emitter.instruction("test eax, eax");                                       // size query succeeded?
    emitter.instruction("jz .Lgetcwd_query_fail");                              // capture native query failure
    emitter.instruction("mov DWORD PTR [rsp + 56], eax");                       // retain WCHAR capacity
    emitter.instruction("movsxd rax, eax");                                     // widen WCHAR count for allocation
    emitter.instruction("shl rax, 1");                                          // WCHAR uses two bytes
    emitter.instruction("call __rt_heap_alloc");                                // allocate dynamic wide buffer
    emitter.instruction("test rax, rax");                                       // allocation succeeded?
    emitter.instruction("jz .Lgetcwd_alloc_fail");                              // ENOMEM
    emitter.instruction("mov QWORD PTR [rsp + 48], rax");                       // retain owned wide buffer
    emitter.instruction("mov ecx, DWORD PTR [rsp + 56]");                       // WCHAR capacity
    emitter.instruction("mov rdx, rax");                                        // wide destination
    emitter.instruction("call GetCurrentDirectoryW");                           // retrieve the current directory as UTF-16
    emitter.instruction("test eax, eax");                                       // native retrieval succeeded?
    emitter.instruction("jz .Lgetcwd_wide_fail");                               // capture native failure
    emitter.instruction("cmp eax, DWORD PTR [rsp + 56]");                       // did the directory grow after the size query?
    emitter.instruction("jae .Lgetcwd_range_fail");                             // caller-visible range failure
    emitter.instruction("mov rdi, QWORD PTR [rsp + 48]");                       // UTF-16 source
    emitter.instruction("mov rsi, QWORD PTR [rsp + 32]");                       // caller UTF-8 destination
    emitter.instruction("mov rdx, QWORD PTR [rsp + 40]");                       // caller destination byte capacity
    emitter.instruction("call __rt_win_utf16_to_utf8");                         // strict UTF-16 to UTF-8 conversion including NUL
    emitter.instruction("test eax, eax");                                       // conversion fit and succeeded?
    emitter.instruction("jz .Lgetcwd_wide_fail");                               // capture conversion/range failure
    emitter.instruction("mov rax, QWORD PTR [rsp + 48]");                       // owned wide buffer
    emitter.instruction("call __rt_heap_free");                                 // release temporary UTF-16 storage
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // POSIX getcwd returns the destination pointer
    emitter.instruction("add rsp, 88");                                         // restore stack
    emitter.instruction("ret");                                                 // return UTF-8 destination
    emitter.label(".Lgetcwd_wide_fail");
    emitter.instruction("call GetLastError");                                   // capture failure before cleanup
    emitter.instruction("mov DWORD PTR [rsp + 64], eax");                       // retain native error code
    emitter.instruction("mov rax, QWORD PTR [rsp + 48]");                       // owned wide buffer
    emitter.instruction("call __rt_heap_free");                                 // release temporary UTF-16 storage
    emitter.instruction("mov eax, DWORD PTR [rsp + 64]");                       // restore native error
    emitter.instruction("jmp .Lgetcwd_publish_native_error");                   // map error and return NULL
    emitter.label(".Lgetcwd_query_fail");
    emitter.instruction("call GetLastError");                                   // capture size-query failure
    emitter.label(".Lgetcwd_publish_native_error");
    emitter.instruction("mov DWORD PTR [rip + __rt_win32_last_error], eax");    // retain native Win32 state
    emitter.instruction("call __rt_win32_errno_from_code");                     // translate to POSIX errno
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], eax");               // publish observable errno
    emitter.instruction("xor eax, eax");                                        // POSIX getcwd failure is NULL
    emitter.instruction("add rsp, 88");                                         // restore stack
    emitter.instruction("ret");                                                 // return failure
    emitter.label(".Lgetcwd_range_fail");
    emitter.instruction("mov rax, QWORD PTR [rsp + 48]");                       // owned wide buffer
    emitter.instruction("call __rt_heap_free");                                 // release buffer after a size race
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 34");                // ERANGE
    emitter.instruction("xor eax, eax");                                        // POSIX getcwd failure is NULL
    emitter.instruction("add rsp, 88");                                         // restore stack
    emitter.instruction("ret");                                                 // return failure
    emitter.label(".Lgetcwd_alloc_fail");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 12");                // ENOMEM
    emitter.instruction("xor eax, eax");                                        // POSIX getcwd failure is NULL
    emitter.instruction("add rsp, 88");                                         // restore stack
    emitter.instruction("ret");                                                 // return failure
    emitter.blank();
}

/// Emits a shim that wraps `SetCurrentDirectoryW`.
pub(super) fn emit_shim_chdir(emitter: &mut Emitter) {
    emit_wide_path_bool_shim(
        emitter,
        "__rt_sys_chdir",
        "SetCurrentDirectoryW",
        false,
    );
}

/// Emits a shim that converts `mkdir` to `CreateDirectoryW`.
pub(super) fn emit_shim_mkdir(emitter: &mut Emitter) {
    emit_wide_path_bool_shim(emitter, "__rt_sys_mkdir", "CreateDirectoryW", true);
}

/// Emits a shim that converts `rmdir` to `RemoveDirectoryW`.
pub(super) fn emit_shim_rmdir(emitter: &mut Emitter) {
    emit_wide_path_bool_shim(emitter, "__rt_sys_rmdir", "RemoveDirectoryW", false);
}

/// Emits a one-path Win32 filesystem shim with POSIX status translation and errno capture.
fn emit_wide_path_bool_shim(
    emitter: &mut Emitter,
    label: &str,
    api: &str,
    null_second_arg: bool,
) {
    let fail_label = format!(".L{}_wide_fail", label.trim_start_matches("__rt_sys_"));
    let conversion_label = format!(
        ".L{}_wide_conversion_fail",
        label.trim_start_matches("__rt_sys_")
    );
    emitter.label_global(label);
    emitter.instruction("sub rsp, 56");                                         // shadow space and aligned owned-path locals
    emitter.instruction("call __rt_win_utf8_to_utf16");                         // convert SysV rdi UTF-8 path strictly
    emitter.instruction("test rax, rax");                                       // conversion succeeded?
    emitter.instruction(&format!("jz {conversion_label}"));                     // invalid UTF-8 path
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // retain owned wide path
    emitter.instruction("mov rcx, rax");                                        // native path argument
    if null_second_arg {
        emitter.instruction("xor edx, edx");                                    // optional security attributes = NULL
    }
    emitter.instruction(&format!("call {api}"));                                // invoke native Unicode filesystem API
    emitter.instruction("test eax, eax");                                       // Win32 BOOL success?
    emitter.instruction(&format!("jz {fail_label}"));                           // capture the native failure
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // owned wide path
    emitter.instruction("call __rt_heap_free");                                 // release converted path
    emitter.instruction("xor eax, eax");                                        // translate Win32 TRUE to POSIX success
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return zero for POSIX success
    emitter.label(&fail_label);
    emitter.instruction("call GetLastError");                                   // fetch error before cleanup can clobber it
    emitter.instruction("mov DWORD PTR [rsp + 40], eax");                       // preserve native error
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // owned wide path
    emitter.instruction("call __rt_heap_free");                                 // release converted path on failure
    emitter.instruction("mov eax, DWORD PTR [rsp + 40]");                       // reload native error
    emitter.instruction("mov DWORD PTR [rip + __rt_win32_last_error], eax");    // retain native Win32 state
    emitter.instruction("call __rt_win32_errno_from_code");                     // translate to POSIX errno
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], eax");               // publish observable errno
    emitter.instruction("mov eax, -1");                                         // translate Win32 FALSE to POSIX failure
    emitter.instruction("cdqe");                                                // sign-extend the POSIX int result for 64-bit runtime consumers
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return -1 for POSIX failure
    emitter.label(&conversion_label);
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 84");                // EILSEQ: invalid UTF-8 path
    emitter.instruction("mov eax, -1");                                         // invalid paths use the POSIX failure sentinel
    emitter.instruction("cdqe");                                                // sign-extend the POSIX int result for 64-bit runtime consumers
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return -1 for POSIX failure
    emitter.blank();
}

/// Emits a shim that fills a Linux-layout `struct stat` buffer for a path.
///
/// SysV: rdi=path, rsi=stat buffer. Returns 0 on success, -1 on failure.
/// Queries the file with Win32 `GetFileAttributesExW` and writes st_mode,
/// st_size, st_nlink, st_ino, and the atime/mtime/ctime seconds at the
/// Windows-target `struct stat` offsets the runtime reads. Uses Win32 directly
/// instead of msvcrt `stat`: the msvcrt struct layout differs from the runtime
/// Linux layout, and the `stat` C-symbol name is a local shim stub, so calling
/// it would recurse.
///
/// `st_ino` is synthesized from `GetFileInformationByHandle`'s
/// `nFileIndexHigh:nFileIndexLow` (the NTFS/ReFS file-index pair, stable and
/// unique per volume — Windows' closest equivalent to a POSIX inode number),
/// which requires a second Win32 round trip: `CreateFileW` (metadata-only, with
/// `FILE_FLAG_BACKUP_SEMANTICS` so directories open too) to get a handle, then
/// `GetFileInformationByHandle`, then `CloseHandle`. The owned UTF-16 path is
/// retained in a stack slot because the zero-fill loops reuse `rdi` as their
/// `rep stosb` destination; `rsi` (the stat buffer base) is MSx64 non-volatile
/// and survives the native calls untouched.
pub(super) fn emit_shim_stat(emitter: &mut Emitter) {
    let mode_off = emitter.platform.stat_mode_offset();
    let size_off = emitter.platform.stat_size_offset();
    let nlink_off = emitter.platform.stat_nlink_offset();
    let ino_off = emitter.platform.stat_ino_offset();
    let dev_off = emitter.platform.stat_dev_offset();
    let atime_off = emitter.platform.stat_atime_offset();
    let mtime_off = emitter.platform.stat_mtime_offset();
    let ctime_off = emitter.platform.stat_ctime_offset();
    emitter.label_global("__rt_sys_stat");
    emitter.instruction("sub rsp, 168");                                        // shadow(32), path metadata, handle information, and executable/mode locals, 16B aligned
    emitter.instruction("mov QWORD PTR [rsp + 144], rsi");                      // preserve the SysV stat destination across UTF conversion
    emitter.instruction("call __rt_win_utf8_to_utf16");                         // strictly convert the PHP UTF-8 path
    emitter.instruction("test rax, rax");                                       // conversion succeeded?
    emitter.instruction("jz .Lstat_conversion_fail");                           // invalid UTF-8 path
    emitter.instruction("mov QWORD PTR [rsp + 72], rax");                       // retain the owned wide path for both metadata queries
    emitter.instruction("mov rcx, rax");                                        // lpFileName = UTF-16 path
    emitter.instruction("xor edx, edx");                                        // fInfoLevelId = GetFileExInfoStandard (0)
    emitter.instruction("lea r8, [rsp + 32]");                                  // lpFileInformation = &WIN32_FILE_ATTRIBUTE_DATA
    emitter.instruction("call GetFileAttributesExW");                           // query Unicode path attributes, size, and timestamps
    emitter.instruction("test eax, eax");                                       // zero return means the query failed
    emitter.instruction("jz .Lstat_fail");                                      // return -1 when the path cannot be queried
    // -- zero the Linux-layout stat buffer before filling fields --
    emitter.instruction("cld");                                                 // forward direction for rep stosb
    emitter.instruction("mov rsi, QWORD PTR [rsp + 144]");                      // restore the caller's stat destination
    emitter.instruction("mov rdi, rsi");                                        // dest = stat buffer base
    emitter.instruction("xor eax, eax");                                        // zero fill byte
    emitter.instruction("mov ecx, 128");                                        // Linux struct stat size in bytes
    emitter.instruction("rep stosb");                                           // zero the whole stat buffer
    // -- st_size = (nFileSizeHigh << 32) | nFileSizeLow --
    emitter.instruction("mov eax, DWORD PTR [rsp + 64]");                       // nFileSizeLow (zero-extends into rax)
    emitter.instruction("mov edx, DWORD PTR [rsp + 60]");                       // nFileSizeHigh
    emitter.instruction("shl rdx, 32");                                         // shift the high dword into the upper 32 bits
    emitter.instruction("or rax, rdx");                                         // combine into the full 64-bit size
    emitter.instruction(&format!("mov QWORD PTR [rsi + {}], rax", size_off));   // store st_size
    // -- st_mode: PHP/Windows synthesizes permissions from attributes and executable suffixes --
    emitter.instruction("mov edx, DWORD PTR [rsp + 32]");                       // dwFileAttributes
    emitter.instruction("mov eax, 0x81B6");                                     // default st_mode = S_IFREG | 0666
    emitter.instruction("test edx, 0x10");                                      // FILE_ATTRIBUTE_DIRECTORY set?
    emitter.instruction("jz .Lstat_mode_readonly");                             // non-directory keeps regular-file type
    emitter.instruction("mov eax, 0x41FF");                                     // st_mode = S_IFDIR | 0777
    emitter.label(".Lstat_mode_readonly");
    emitter.instruction("test edx, 1");                                         // FILE_ATTRIBUTE_READONLY set?
    emitter.instruction("jz .Lstat_mode_base_ready");                           // writable object retains synthesized write bits
    emitter.instruction("and eax, 0xFF6D");                                     // readonly object clears 0222, including directories (0555)
    emitter.label(".Lstat_mode_base_ready");
    emitter.instruction("mov DWORD PTR [rsp + 156], eax");                      // retain the base mode across GetBinaryTypeW
    emitter.instruction("test edx, 0x10");                                      // directory?
    emitter.instruction("jnz .Lstat_mode_done");                                // directories are executable by type, not file suffix
    emitter.instruction("mov rdi, QWORD PTR [rsp + 72]");                       // UTF-16 path for executable-suffix inspection
    emitter.instruction("xor rcx, rcx");                                        // path length in UTF-16 code units
    emitter.label(".Lstat_exec_len");
    emitter.instruction("cmp WORD PTR [rdi + rcx * 2], 0");                     // trailing NUL reached?
    emitter.instruction("je .Lstat_exec_len_done");                             // suffix can now be inspected
    emitter.instruction("inc rcx");                                             // count one UTF-16 code unit
    emitter.instruction("jmp .Lstat_exec_len");                                 // continue scanning the owned path
    emitter.label(".Lstat_exec_len_done");
    emitter.instruction("cmp rcx, 4");                                          // extension needs dot plus three characters
    emitter.instruction("jb .Lstat_mode_done");                                 // no executable suffix possible
    emitter.instruction("cmp WORD PTR [rdi + rcx * 2 - 8], 46");                // final extension begins with ASCII dot?
    emitter.instruction("jne .Lstat_mode_done");                                // no recognized extension
    emitter.instruction("movzx eax, WORD PTR [rdi + rcx * 2 - 6]");             // first extension letter
    emitter.instruction("or eax, 0x20");                                        // ASCII case-fold e/E, c/C, b/B
    emitter.instruction("cmp eax, 101");                                        // .e?? (exe)?
    emitter.instruction("je .Lstat_exec_e");                                    // inspect exe
    emitter.instruction("cmp eax, 99");                                         // .c?? (com/cmd)?
    emitter.instruction("je .Lstat_exec_c");                                    // inspect com/cmd
    emitter.instruction("cmp eax, 98");                                         // .b?? (bat)?
    emitter.instruction("jne .Lstat_mode_done");                                // unsupported extension
    emitter.instruction("movzx eax, WORD PTR [rdi + rcx * 2 - 4]");             // second extension letter
    emitter.instruction("or eax, 0x20");                                        // ASCII case-fold a/A
    emitter.instruction("cmp eax, 97");                                         // .ba?
    emitter.instruction("jne .Lstat_mode_done");                                // not .bat
    emitter.instruction("movzx eax, WORD PTR [rdi + rcx * 2 - 2]");             // third extension letter
    emitter.instruction("or eax, 0x20");                                        // ASCII case-fold t/T
    emitter.instruction("cmp eax, 116");                                        // .bat?
    emitter.instruction("jne .Lstat_mode_done");                                // not .bat
    emitter.instruction("or DWORD PTR [rsp + 156], 0x49");                      // .bat keeps php-src's compatibility execute bits 0111
    emitter.instruction("jmp .Lstat_mode_done");                                // mode is complete
    emitter.label(".Lstat_exec_e");
    emitter.instruction("movzx eax, WORD PTR [rdi + rcx * 2 - 4]");             // second extension letter
    emitter.instruction("or eax, 0x20");                                        // ASCII case-fold x/X
    emitter.instruction("cmp eax, 120");                                        // .ex?
    emitter.instruction("jne .Lstat_mode_done");                                // not .exe
    emitter.instruction("movzx eax, WORD PTR [rdi + rcx * 2 - 2]");             // third extension letter
    emitter.instruction("or eax, 0x20");                                        // ASCII case-fold e/E
    emitter.instruction("cmp eax, 101");                                        // .exe?
    emitter.instruction("jne .Lstat_mode_done");                                // not .exe
    emitter.instruction("jmp .Lstat_exec_binary");                              // validate PE/DOS executable through Win32
    emitter.label(".Lstat_exec_c");
    emitter.instruction("movzx eax, WORD PTR [rdi + rcx * 2 - 4]");             // second extension letter
    emitter.instruction("or eax, 0x20");                                        // ASCII case-fold o/O or m/M
    emitter.instruction("cmp eax, 111");                                        // .co? (com)?
    emitter.instruction("je .Lstat_exec_com");                                  // inspect .com
    emitter.instruction("cmp eax, 109");                                        // .cm? (cmd)?
    emitter.instruction("jne .Lstat_mode_done");                                // neither .com nor .cmd
    emitter.instruction("movzx eax, WORD PTR [rdi + rcx * 2 - 2]");             // third extension letter
    emitter.instruction("or eax, 0x20");                                        // ASCII case-fold d/D
    emitter.instruction("cmp eax, 100");                                        // .cmd?
    emitter.instruction("jne .Lstat_mode_done");                                // not .cmd
    emitter.instruction("or DWORD PTR [rsp + 156], 0x49");                      // .cmd keeps php-src's compatibility execute bits 0111
    emitter.instruction("jmp .Lstat_mode_done");                                // mode is complete
    emitter.label(".Lstat_exec_com");
    emitter.instruction("movzx eax, WORD PTR [rdi + rcx * 2 - 2]");             // third extension letter
    emitter.instruction("or eax, 0x20");                                        // ASCII case-fold m/M
    emitter.instruction("cmp eax, 109");                                        // .com?
    emitter.instruction("jne .Lstat_mode_done");                                // not .com
    emitter.label(".Lstat_exec_binary");
    emitter.instruction("mov rcx, QWORD PTR [rsp + 72]");                       // lpApplicationName = original UTF-16 path
    emitter.instruction("lea rdx, [rsp + 152]");                                // lpBinaryType = stack DWORD outside shadow space
    emitter.instruction("call GetBinaryTypeW");                                 // validate .exe/.com as executable images
    emitter.instruction("test eax, eax");                                       // accepted executable image?
    emitter.instruction("jz .Lstat_mode_done");                                 // retain non-executable base mode
    emitter.instruction("or DWORD PTR [rsp + 156], 0x49");                      // add user/group/other execute bits
    emitter.label(".Lstat_mode_done");
    emitter.instruction("mov eax, DWORD PTR [rsp + 156]");                      // reload synthesized mode after optional executable probe
    emitter.instruction(&format!("mov DWORD PTR [rsi + {}], eax", mode_off));   // store st_mode
    emitter.instruction(&format!("mov DWORD PTR [rsi + {}], 1", nlink_off));    // temporary fallback until handle metadata supplies the real link count
    // -- timestamps: convert each FILETIME to Unix epoch seconds --
    emit_filetime_to_stat_seconds(emitter, 44, atime_off);
    emit_filetime_to_stat_seconds(emitter, 52, mtime_off);
    emit_filetime_to_stat_seconds(emitter, 36, ctime_off);
    // -- st_ino: CreateFileW + GetFileInformationByHandle synthesize a stable file id --
    emitter.instruction("mov rcx, QWORD PTR [rsp + 72]");                       // lpFileName = the retained UTF-16 path
    emitter.instruction("xor edx, edx");                                        // dwDesiredAccess = 0 (metadata-only open)
    emitter.instruction("mov r8, 7");                                           // dwShareMode = FILE_SHARE_READ|WRITE|DELETE
    emitter.instruction("xor r9, r9");                                          // lpSecurityAttributes = NULL
    emitter.instruction("mov QWORD PTR [rsp + 32], 3");                         // dwCreationDisposition = OPEN_EXISTING
    emitter.instruction("mov QWORD PTR [rsp + 40], 0x2000000");                 // dwFlagsAndAttributes = FILE_FLAG_BACKUP_SEMANTICS (lets directories open)
    emitter.instruction("mov QWORD PTR [rsp + 48], 0");                         // hTemplateFile = NULL
    emitter.instruction("call CreateFileW");                                    // open a metadata-only handle to the Unicode path
    emitter.instruction("cmp rax, -1");                                         // INVALID_HANDLE_VALUE?
    emitter.instruction("je .Lstat_ino_skip");                                  // leave st_ino at 0 when the handle cannot be opened
    emitter.instruction("mov QWORD PTR [rsp + 80], rax");                       // save the handle across GetFileInformationByHandle
    emitter.instruction("cld");                                                 // forward direction for rep stosb
    emitter.instruction("lea rdi, [rsp + 88]");                                 // dest = BY_HANDLE_FILE_INFORMATION buffer
    emitter.instruction("xor eax, eax");                                        // zero fill byte
    emitter.instruction("mov ecx, 52");                                         // sizeof(BY_HANDLE_FILE_INFORMATION)
    emitter.instruction("rep stosb");                                           // zero the buffer before the query
    emitter.instruction("mov rcx, QWORD PTR [rsp + 80]");                       // hFile = the opened handle
    emitter.instruction("lea rdx, [rsp + 88]");                                 // lpFileInformation = &BY_HANDLE_FILE_INFORMATION
    emitter.instruction("call GetFileInformationByHandle");                     // query followed target metadata and link count
    emitter.instruction("test eax, eax");                                       // metadata query succeeded?
    emitter.instruction("jz .Lstat_metadata_skip");                             // retain attribute-query fallback when the handle rejects metadata
    // The path query above reports the reparse point's own zero length under
    // Wine.  The ordinary handle follows a symlink, so prefer its target
    // metadata for stat() just as PHP/POSIX do (lstat() corrects the type later).
    emitter.instruction("mov rsi, QWORD PTR [rsp + 144]");                      // restore caller stat destination
    emitter.instruction("mov eax, DWORD PTR [rsp + 124]");                      // followed target nFileSizeLow
    emitter.instruction("mov edx, DWORD PTR [rsp + 120]");                      // followed target nFileSizeHigh
    emitter.instruction("shl rdx, 32");                                         // position target size high dword
    emitter.instruction("or rax, rdx");                                         // combine followed target size
    emitter.instruction(&format!("mov QWORD PTR [rsi + {}], rax", size_off));   // publish followed target size
    emitter.instruction("mov eax, DWORD PTR [rsp + 116]");                      // dwVolumeSerialNumber identifies the backing volume
    emitter.instruction("test eax, eax");                                       // Wine may expose a zero serial for synthetic volumes
    emitter.instruction("jnz .Lstat_dev_ready");                                // retain a real nonzero serial
    emitter.instruction("mov eax, 1");                                          // stable nonzero synthetic device id
    emitter.label(".Lstat_dev_ready");
    emitter.instruction(&format!("mov QWORD PTR [rsi + {}], rax", dev_off));    // publish st_dev for linkinfo/stat arrays
    emitter.instruction("mov eax, DWORD PTR [rsp + 132]");                      // nFileIndexHigh (base+44, base = rsp+88)
    emitter.instruction("shl rax, 32");                                         // shift into the upper 32 bits of the 64-bit id
    emitter.instruction("mov ecx, DWORD PTR [rsp + 136]");                      // nFileIndexLow (base+48, base = rsp+88)
    emitter.instruction("or rax, rcx");                                         // combine into the full 64-bit inode number
    emitter.instruction(&format!("mov QWORD PTR [rsi + {}], rax", ino_off));    // store st_ino
    emitter.instruction("mov eax, DWORD PTR [rsp + 128]");                      // nNumberOfLinks (base+40, base = rsp+88)
    emitter.instruction(&format!("mov DWORD PTR [rsi + {}], eax", nlink_off));  // publish the native hard-link count
    // Prefer the followed handle's attributes and timestamps over the path
    // query. This matters for symlinks, whose link attributes can differ from
    // the target metadata returned by PHP's ordinary stat().
    emitter.instruction("mov edx, DWORD PTR [rsp + 88]");                       // followed target dwFileAttributes
    emitter.instruction("mov ecx, DWORD PTR [rsp + 156]");                      // retain executable bits derived from the original path suffix
    emitter.instruction("and ecx, 0x49");                                       // isolate user/group/other execute bits
    emitter.instruction("mov eax, 0x81B6");                                     // followed regular-file mode = S_IFREG | 0666
    emitter.instruction("test edx, 0x10");                                      // followed target is a directory?
    emitter.instruction("jz .Lstat_handle_mode_readonly");                      // regular files retain the suffix-derived execute bits
    emitter.instruction("mov eax, 0x41FF");                                     // followed directory mode = S_IFDIR | 0777
    emitter.label(".Lstat_handle_mode_readonly");
    emitter.instruction("test edx, 1");                                         // followed target is readonly?
    emitter.instruction("jz .Lstat_handle_mode_ready");                         // writable target retains write bits
    emitter.instruction("and eax, 0xFF6D");                                     // readonly target clears 0222
    emitter.label(".Lstat_handle_mode_ready");
    emitter.instruction("or eax, ecx");                                         // restore valid .exe/.com/.bat/.cmd execute bits
    emitter.instruction(&format!("mov DWORD PTR [rsi + {}], eax", mode_off));   // publish followed target type and permissions
    emit_filetime_to_stat_seconds(emitter, 100, atime_off);
    emit_filetime_to_stat_seconds(emitter, 108, mtime_off);
    emit_filetime_to_stat_seconds(emitter, 92, ctime_off);
    emitter.instruction("mov rcx, QWORD PTR [rsp + 80]");                       // hFile for Win32 object-type classification
    emitter.instruction("call GetFileType");                                    // distinguish named pipes and character devices from files
    emitter.instruction("cmp eax, 3");                                          // FILE_TYPE_PIPE?
    emitter.instruction("je .Lstat_pipe_mode");                                 // replace the regular-file type with S_IFIFO
    emitter.instruction("cmp eax, 2");                                          // FILE_TYPE_CHAR?
    emitter.instruction("jne .Lstat_metadata_skip");                            // files/directories retain their path-derived mode
    emitter.instruction("mov eax, 0x21B6");                                     // st_mode = S_IFCHR | 0666
    emitter.instruction("mov edx, DWORD PTR [rsp + 88]");                       // dwFileAttributes from BY_HANDLE_FILE_INFORMATION
    emitter.instruction("test edx, 1");                                         // FILE_ATTRIBUTE_READONLY?
    emitter.instruction("jz .Lstat_special_mode_ready");                        // writable character device
    emitter.instruction("and eax, 0xFF6D");                                     // readonly character device clears 0222
    emitter.instruction("jmp .Lstat_special_mode_ready");                       // store the selected character mode
    emitter.label(".Lstat_pipe_mode");
    emitter.instruction("mov eax, 0x11B6");                                     // st_mode = S_IFIFO | 0666
    emitter.instruction("mov edx, DWORD PTR [rsp + 88]");                       // dwFileAttributes from BY_HANDLE_FILE_INFORMATION
    emitter.instruction("test edx, 1");                                         // FILE_ATTRIBUTE_READONLY?
    emitter.instruction("jz .Lstat_special_mode_ready");                        // writable pipe
    emitter.instruction("and eax, 0xFF6D");                                     // readonly pipe clears 0222
    emitter.label(".Lstat_special_mode_ready");
    emitter.instruction(&format!("mov DWORD PTR [rsi + {}], eax", mode_off));   // publish pipe/character object type and permissions
    emitter.label(".Lstat_metadata_skip");
    emitter.instruction("mov rcx, QWORD PTR [rsp + 80]");                       // reload the handle
    emitter.instruction("call CloseHandle");                                    // release the metadata-only handle
    emitter.label(".Lstat_ino_skip");
    emitter.instruction("mov rax, QWORD PTR [rsp + 72]");                       // owned wide path
    emitter.instruction("call __rt_heap_free");                                 // release the converted path after both queries
    emitter.instruction("xor eax, eax");                                        // return 0 (success)
    emitter.instruction("add rsp, 168");                                        // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lstat_fail");
    emitter.instruction("call GetLastError");                                   // capture query failure before cleanup
    emitter.instruction("mov DWORD PTR [rsp + 80], eax");                       // preserve native error code
    emitter.instruction("mov rax, QWORD PTR [rsp + 72]");                       // owned wide path
    emitter.instruction("call __rt_heap_free");                                 // release the converted path on failure
    emitter.instruction("mov eax, DWORD PTR [rsp + 80]");                       // restore native error code
    emitter.instruction("mov DWORD PTR [rip + __rt_win32_last_error], eax");    // retain native Win32 state
    emitter.instruction("call __rt_win32_errno_from_code");                     // translate to POSIX errno
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], eax");               // publish observable errno
    emitter.instruction("mov rax, -1");                                         // return -1 on failure
    emitter.instruction("add rsp, 168");                                        // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lstat_conversion_fail");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 84");                // EILSEQ: invalid UTF-8 path
    emitter.instruction("mov rax, -1");                                         // return -1 on failure
    emitter.instruction("add rsp, 168");                                        // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits the conversion of a Win32 `FILETIME` (100ns ticks since 1601) held at
/// `[rsp + src_off]` into Unix epoch seconds stored at `[rsi + dst_off]` of the
/// Linux-layout stat buffer. Clobbers rax/rdx/r8/r9 (all MSx64 volatile) and
/// leaves rsi (the stat buffer base) intact.
fn emit_filetime_to_stat_seconds(emitter: &mut Emitter, src_off: usize, dst_off: usize) {
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", src_off));    // load the 64-bit FILETIME
    emitter.instruction("mov r8, 116444736000000000");                          // 1601->1970 offset in 100ns units
    emitter.instruction("sub rax, r8");                                         // rebase the tick count onto the Unix epoch
    emitter.instruction("cqo");                                                 // sign-extend negative pre-1970 FILETIME deltas
    emitter.instruction("mov r9, 10000000");                                    // 100ns intervals per second
    emitter.instruction("idiv r9");                                             // rax = signed whole seconds since 1970
    emitter.instruction(&format!("mov QWORD PTR [rsi + {}], rax", dst_off));    // store the timestamp seconds
}

/// Emits a shim that wraps `MoveFileExW` for `rename`, translating the Win32
/// `BOOL` result (nonzero = success) to the POSIX convention `__rt_rename`
/// expects (0 = success, -1 = failure). Without the translation a successful
/// rename (`BOOL` nonzero, compared against 0 by `__rt_rename`) would be
/// reported as failure and vice versa — mirrors the `link` shim.
/// `MOVEFILE_REPLACE_EXISTING` matches php-src's write-then-rename overwrite.
pub(super) fn emit_shim_rename(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_rename");
    emitter.instruction("sub rsp, 72");                                         // shadow space and aligned two-path ownership locals
    emitter.instruction("mov QWORD PTR [rsp + 32], rsi");                       // preserve UTF-8 destination across conversion
    emitter.instruction("call __rt_win_utf8_to_utf16");                         // convert UTF-8 source path
    emitter.instruction("test rax, rax");                                       // source conversion succeeded?
    emitter.instruction("jz .Lrename_conversion_fail");                         // invalid UTF-8 source
    emitter.instruction("mov QWORD PTR [rsp + 40], rax");                       // retain owned wide source
    emitter.instruction("mov rdi, QWORD PTR [rsp + 32]");                       // UTF-8 destination path
    emitter.instruction("call __rt_win_utf8_to_utf16");                         // convert UTF-8 destination path
    emitter.instruction("test rax, rax");                                       // destination conversion succeeded?
    emitter.instruction("jz .Lrename_destination_conversion_fail");             // release source before returning
    emitter.instruction("mov QWORD PTR [rsp + 48], rax");                       // retain owned wide destination
    emitter.instruction("mov rcx, QWORD PTR [rsp + 40]");                       // lpExistingFileName = wide source
    emitter.instruction("mov rdx, rax");                                        // lpNewFileName = wide destination
    emitter.instruction("mov r8d, 3");                                          // MOVEFILE_REPLACE_EXISTING | MOVEFILE_COPY_ALLOWED
    emitter.instruction("call MoveFileExW");                                    // rename Unicode path, replacing existing target
    emitter.instruction("test eax, eax");                                       // Win32 BOOL: nonzero = success
    emitter.instruction("jz .Lrename_fail");                                    // zero = failure
    emitter.instruction("mov DWORD PTR [rsp + 56], 0");                         // preserve POSIX success across cleanup
    emitter.instruction("jmp .Lrename_cleanup");                                // release both converted paths
    emitter.label(".Lrename_fail");
    emitter.instruction("call GetLastError");                                   // capture native rename failure
    emitter.instruction("mov DWORD PTR [rsp + 56], eax");                       // preserve native error across cleanup
    emitter.label(".Lrename_cleanup");
    emitter.instruction("mov rax, QWORD PTR [rsp + 48]");                       // owned wide destination
    emitter.instruction("call __rt_heap_free");                                 // release destination
    emitter.instruction("mov rax, QWORD PTR [rsp + 40]");                       // owned wide source
    emitter.instruction("call __rt_heap_free");                                 // release source
    emitter.instruction("mov eax, DWORD PTR [rsp + 56]");                       // zero success or native error
    emitter.instruction("test eax, eax");                                       // was the operation successful?
    emitter.instruction("jz .Lrename_success");                                 // return POSIX zero
    emitter.instruction("mov DWORD PTR [rip + __rt_win32_last_error], eax");    // retain native Win32 state
    emitter.instruction("call __rt_win32_errno_from_code");                     // translate failure
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], eax");               // publish errno
    emitter.instruction("mov rax, -1");                                         // POSIX rename failure
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lrename_success");
    emitter.instruction("xor eax, eax");                                        // POSIX rename success
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lrename_destination_conversion_fail");
    emitter.instruction("mov rax, QWORD PTR [rsp + 40]");                       // owned wide source
    emitter.instruction("call __rt_heap_free");                                 // release source after destination conversion failure
    emitter.label(".Lrename_conversion_fail");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 84");                // EILSEQ: invalid UTF-8 path
    emitter.instruction("mov rax, -1");                                         // POSIX rename failure
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits a shim that wraps `SetFileAttributesW` for `chmod`.
///
/// SysV: rdi=path, rsi=mode.
/// Maps Unix mode to Win32 file attributes:
/// - If mode has no write bits (mode & 0222 == 0) → FILE_ATTRIBUTE_READONLY (1)
/// - Otherwise → FILE_ATTRIBUTE_NORMAL (128)
pub(super) fn emit_shim_chmod(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_chmod");
    emitter.instruction("sub rsp, 56");                                         // shadow space and aligned owned-path locals
    emitter.instruction("mov QWORD PTR [rsp + 40], rsi");                       // preserve mode across conversion
    emitter.instruction("call __rt_win_utf8_to_utf16");                         // convert path strictly to UTF-16
    emitter.instruction("test rax, rax");                                       // conversion succeeded?
    emitter.instruction("jz .Lchmod_conversion_fail");                          // invalid UTF-8 path
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // retain owned wide path
    emitter.instruction("mov rcx, rax");                                        // native path
    emitter.instruction("mov rsi, QWORD PTR [rsp + 40]");                       // restore Unix mode
    emitter.instruction("test rsi, 0222");                                      // any write bit set?
    emitter.instruction("jnz .Lchmod_writable");                                // → FILE_ATTRIBUTE_NORMAL
    emitter.instruction("mov rdx, 1");                                          // FILE_ATTRIBUTE_READONLY
    emitter.instruction("jmp .Lchmod_call");                                    // → call SetFileAttributesW
    emitter.label(".Lchmod_writable");
    emitter.instruction("mov rdx, 128");                                        // FILE_ATTRIBUTE_NORMAL
    emitter.label(".Lchmod_call");
    emitter.instruction("call SetFileAttributesW");                             // set Unicode file attributes
    emitter.instruction("test eax, eax");                                       // Win32 BOOL success?
    emitter.instruction("jz .Lchmod_fail");                                     // capture native failure
    emitter.instruction("mov DWORD PTR [rsp + 40], 0");                         // preserve POSIX success across cleanup
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // owned wide path
    emitter.instruction("call __rt_heap_free");                                 // release path
    emitter.instruction("mov eax, DWORD PTR [rsp + 40]");                       // restore POSIX success
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return success
    emitter.label(".Lchmod_fail");
    emitter.instruction("call GetLastError");                                   // capture native failure
    emitter.instruction("mov DWORD PTR [rsp + 40], eax");                       // preserve error across cleanup
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // owned wide path
    emitter.instruction("call __rt_heap_free");                                 // release path
    emitter.instruction("mov eax, DWORD PTR [rsp + 40]");                       // reload native error
    emitter.instruction("mov DWORD PTR [rip + __rt_win32_last_error], eax");    // retain native state
    emitter.instruction("call __rt_win32_errno_from_code");                     // translate failure
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], eax");               // publish errno
    emitter.instruction("mov eax, -1");                                         // POSIX chmod failure
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return failure
    emitter.label(".Lchmod_conversion_fail");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 84");                // EILSEQ: invalid UTF-8 path
    emitter.instruction("mov eax, -1");                                         // POSIX chmod failure
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return failure
    emitter.blank();
}

/// Emits a shim that converts `access(path, mode)` to `GetFileAttributesW`.
///
/// SysV: rdi=path, rsi=mode. `GetFileAttributesW` performs the common existence
/// check; PHP's Windows rules then reject `W_OK` for a readonly path and use
/// `GetBinaryTypeW` to decide `X_OK`. `R_OK` succeeds whenever the path exists,
/// matching php-src's Windows compatibility behavior. `GetFileAttributesW`
/// returns `INVALID_FILE_ATTRIBUTES` (0xFFFFFFFF) when the path does not resolve.
pub(super) fn emit_shim_access(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_access");
    emitter.instruction("sub rsp, 72");                                         // shadow space, mode/error locals, and aligned owned path storage
    emitter.instruction("mov QWORD PTR [rsp + 40], rsi");                       // preserve requested access bits across conversion
    emitter.instruction("call __rt_win_utf8_to_utf16");                         // convert path strictly to UTF-16
    emitter.instruction("test rax, rax");                                       // conversion succeeded?
    emitter.instruction("jz .Laccess_conversion_fail");                         // invalid UTF-8 path
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // retain owned wide path
    emitter.instruction("mov edx, DWORD PTR [rsp + 40]");                       // reload F_OK/R_OK/W_OK/X_OK request bits
    emitter.instruction("test edx, 1");                                         // X_OK requested?
    emitter.instruction("jnz .Laccess_check_execute");                          // php-src tests executable images before every attribute permission check
    emitter.instruction("mov rcx, rax");                                        // lpFileName = native path
    emitter.instruction("call GetFileAttributesW");                             // query Unicode file attributes
    emitter.instruction("cmp eax, 0xFFFFFFFF");                                 // INVALID_FILE_ATTRIBUTES?
    emitter.instruction("je .Laccess_fail");                                    // → file does not exist
    emitter.instruction("mov edx, DWORD PTR [rsp + 40]");                       // native call clobbered mode register; reload requested access bits
    emitter.instruction("test edx, 2");                                         // W_OK requested?
    emitter.instruction("jz .Laccess_success");                                 // F_OK/R_OK succeeded after the existence probe
    emitter.instruction("test eax, 1");                                         // FILE_ATTRIBUTE_READONLY set?
    emitter.instruction("jnz .Laccess_eacces");                                 // PHP exposes readonly files as not writable
    emitter.instruction("jmp .Laccess_success");                                // writable attributes satisfy W_OK without requiring an executable image
    emitter.label(".Laccess_check_execute");
    emitter.instruction("mov rcx, QWORD PTR [rsp + 32]");                       // lpApplicationName = retained UTF-16 path
    emitter.instruction("lea rdx, [rsp + 52]");                                 // lpBinaryType = caller-owned DWORD local
    emitter.instruction("call GetBinaryTypeW");                                 // PHP's Windows executable-file check
    emitter.instruction("test eax, eax");                                       // recognized executable image?
    emitter.instruction("jz .Laccess_fail");                                    // preserve GetBinaryTypeW failure
    emitter.label(".Laccess_success");
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // owned wide path
    emitter.instruction("call __rt_heap_free");                                 // release converted path
    emitter.instruction("xor eax, eax");                                        // return 0 for the requested supported access mode
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Laccess_eacces");
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // owned wide path
    emitter.instruction("call __rt_heap_free");                                 // release converted path before the synthetic failure
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 13");                // EACCES: readonly path rejects W_OK
    emitter.instruction("mov eax, -1");                                         // report POSIX access failure
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Laccess_fail");
    emitter.instruction("call GetLastError");                                   // fetch native failure before cleanup
    emitter.instruction("mov DWORD PTR [rsp + 56], eax");                       // preserve native error across cleanup
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // owned wide path
    emitter.instruction("call __rt_heap_free");                                 // release converted path
    emitter.instruction("mov eax, DWORD PTR [rsp + 56]");                       // reload native error
    emitter.instruction("mov DWORD PTR [rip + __rt_win32_last_error], eax");    // retain Win32 error
    emitter.instruction("call __rt_win32_errno_from_code");                     // translate native error
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], eax");               // publish errno
    emitter.instruction("mov eax, -1");                                         // return -1 (path not found)
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Laccess_conversion_fail");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 84");                // EILSEQ: invalid UTF-8 path
    emitter.instruction("mov eax, -1");                                         // access failure
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits a shim that converts `ftruncate(fd, length)` to `SetFilePointerEx` + `SetEndOfFile`.
///
/// SysV: rdi=fd, rsi=length. Converts the CRT descriptor to its Win32 handle,
/// saves the current file position, seeks to `length`, calls `SetEndOfFile`, and
/// restores the original position. Returns 0 on success, or publishes errno and
/// returns -1 on failure (matching libc `ftruncate` and PHP's Windows stream path).
pub(super) fn emit_shim_ftruncate(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_ftruncate");
    // -- stack frame: shadow(32) + length/handle/old-position locals, aligned --
    emitter.instruction("sub rsp, 72");                                         // reserve shadow space and aligned locals
    emitter.instruction("test rsi, rsi");                                       // reject negative file sizes before calling Win32
    emitter.instruction("js .Lftruncate_invalid_size");                         // negative lengths map to EINVAL
    emitter.instruction("mov QWORD PTR [rsp + 32], rsi");                       // preserve the requested length across calls
    emitter.instruction("call __rt_fd_to_handle");                              // convert the CRT descriptor in rdi to a Win32 HANDLE
    emitter.instruction("cmp rax, -1");                                         // INVALID_HANDLE_VALUE?
    emitter.instruction("je .Lftruncate_bad_fd");                               // invalid CRT descriptor maps to EBADF
    emitter.instruction("cmp rax, -2");                                         // detached standard descriptor sentinel?
    emitter.instruction("je .Lftruncate_bad_fd");                               // descriptors without a HANDLE map to EBADF
    emitter.instruction("mov QWORD PTR [rsp + 40], rax");                       // retain the HANDLE across Win32 calls
    // -- save the current file pointer before moving to the new end --
    emitter.instruction("mov rcx, rax");                                        // hFile = converted HANDLE
    emitter.instruction("xor edx, edx");                                        // liDistanceToMove = 0
    emitter.instruction("lea r8, [rsp + 48]");                                  // lpNewFilePointer = saved current position
    emitter.instruction("mov r9d, 1");                                          // dwMoveMethod = FILE_CURRENT
    emitter.instruction("call SetFilePointerEx");                               // capture the PHP-visible stream position
    emitter.instruction("test eax, eax");                                       // position query succeeded?
    emitter.instruction("jz .Lftruncate_fail");                                 // translate the Win32 failure
    // -- SetFilePointerEx(handle, length, NULL, FILE_BEGIN) --
    emitter.instruction("mov rcx, QWORD PTR [rsp + 40]");                       // hFile = converted HANDLE
    emitter.instruction("mov rdx, QWORD PTR [rsp + 32]");                       // liDistanceToMove = requested length
    emitter.instruction("xor r8, r8");                                          // lpNewFilePointer = NULL
    emitter.instruction("xor r9d, r9d");                                        // dwMoveMethod = FILE_BEGIN
    emitter.instruction("call SetFilePointerEx");                               // seek to the target offset from the start of the file
    emitter.instruction("test eax, eax");                                       // seek succeeded?
    emitter.instruction("jz .Lftruncate_fail");                                 // translate the Win32 failure
    // -- SetEndOfFile(handle) --
    emitter.instruction("mov rcx, QWORD PTR [rsp + 40]");                       // hFile = converted HANDLE
    emitter.instruction("call SetEndOfFile");                                   // truncate the file at the current pointer
    emitter.instruction("test eax, eax");                                       // truncate succeeded?
    emitter.instruction("jz .Lftruncate_fail");                                 // translate the Win32 failure
    // -- restore the original stream position after resizing --
    emitter.instruction("mov rcx, QWORD PTR [rsp + 40]");                       // hFile = converted HANDLE
    emitter.instruction("mov rdx, QWORD PTR [rsp + 48]");                       // liDistanceToMove = saved position
    emitter.instruction("xor r8, r8");                                          // lpNewFilePointer = NULL
    emitter.instruction("xor r9d, r9d");                                        // dwMoveMethod = FILE_BEGIN
    emitter.instruction("call SetFilePointerEx");                               // restore PHP's stream cursor
    emitter.instruction("test eax, eax");                                       // restore succeeded?
    emitter.instruction("jz .Lftruncate_fail");                                 // report restoration failure like php-src
    emitter.instruction("xor eax, eax");                                        // return 0 (success)
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lftruncate_fail");
    emitter.instruction("call GetLastError");                                   // capture the seek/truncate failure
    emitter.instruction("mov DWORD PTR [rip + __rt_win32_last_error], eax");    // retain native Win32 state
    emitter.instruction("call __rt_win32_errno_from_code");                     // translate failure to POSIX errno
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], eax");               // publish observable errno
    emitter.instruction("mov rax, -1");                                         // return -1 (failure)
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lftruncate_bad_fd");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 9");                 // EBADF for an invalid CRT descriptor
    emitter.instruction("mov rax, -1");                                         // return -1 (failure)
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lftruncate_invalid_size");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 22");                // EINVAL for a negative target size
    emitter.instruction("mov rax, -1");                                         // return -1 (failure)
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits a writev shim — iterates over iovec array, calling __rt_sys_write for each.
///
/// SysV: rdi=fd, rsi=iov, rdx=iovcnt.
/// Each iovec is 16 bytes: [iov_base:8, iov_len:8].
pub(super) fn emit_shim_writev(emitter: &mut Emitter) {
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

/// Emits a statfs shim — delegates to `GetDiskFreeSpaceExW` and fills the three
/// fields `__rt_disk_space` reads out of the Linux-layout `struct statfs` buffer:
/// `f_bsize` (offset 8), `f_blocks` (offset 16), `f_bavail` (offset 32) — see
/// `Platform::statfs_bsize_offset`/`statfs_blocks_offset`/`statfs_bavail_offset`
/// in `platform/target.rs`, which this shim's literal offsets must track.
///
/// SysV: rdi=path, rsi=statfs buffer base (both MSx64 non-volatile, survive the
/// call). `f_bsize` is reported as 1 so `f_blocks`/`f_bavail` can hold raw byte
/// counts directly (`__rt_disk_space` computes `bytes = f_bsize * block_count`).
/// Returns 0 on success, -1 on failure (leaving the caller's buffer untouched).
pub(super) fn emit_shim_statfs(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_statfs");
    emitter.instruction("sub rsp, 88");                                         // shadow, two out-qwords, owned path, and native error
    emitter.instruction("mov QWORD PTR [rsp + 64], rsi");                       // preserve statfs destination across runtime cleanup helpers
    emitter.instruction("call __rt_win_utf8_to_utf16");                         // strictly convert the filesystem path
    emitter.instruction("test rax, rax");                                       // conversion succeeded?
    emitter.instruction("jz .Lstatfs_conversion_fail");                         // invalid UTF-8 path
    emitter.instruction("mov QWORD PTR [rsp + 48], rax");                       // retain owned wide path
    emitter.instruction("mov rcx, rax");                                        // lpDirectoryName = UTF-16 path
    emitter.instruction("lea rdx, [rsp + 32]");                                 // &FreeBytesAvailableToCaller
    emitter.instruction("lea r8, [rsp + 40]");                                  // &TotalNumberOfBytes
    emitter.instruction("xor r9, r9");                                          // lpTotalNumberOfFreeBytes = NULL (unused)
    emitter.instruction("call GetDiskFreeSpaceExW");                            // query available/total bytes for the Unicode volume path
    emitter.instruction("test eax, eax");                                       // zero return means the query failed
    emitter.instruction("jz .Lstatfs_fail");                                    // return -1 when the path cannot be queried
    emitter.instruction("mov rax, QWORD PTR [rsp + 48]");                       // owned wide path
    emitter.instruction("call __rt_heap_free");                                 // release converted path after the native query
    emitter.instruction("mov rsi, QWORD PTR [rsp + 64]");                       // restore statfs destination after cleanup
    emitter.instruction("mov DWORD PTR [rsi + 8], 1");                          // f_bsize = 1 (so f_blocks/f_bavail already hold raw bytes)
    emitter.instruction("mov rax, QWORD PTR [rsp + 40]");                       // TotalNumberOfBytes
    emitter.instruction("mov QWORD PTR [rsi + 16], rax");                       // f_blocks = total bytes (f_bsize == 1)
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // FreeBytesAvailableToCaller
    emitter.instruction("mov QWORD PTR [rsi + 32], rax");                       // f_bavail = available bytes (f_bsize == 1)
    emitter.instruction("xor rax, rax");                                        // return 0 (success)
    emitter.instruction("add rsp, 88");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lstatfs_fail");
    emitter.instruction("call GetLastError");                                   // capture native query failure before cleanup
    emitter.instruction("mov DWORD PTR [rsp + 56], eax");                       // preserve native error
    emitter.instruction("mov rax, QWORD PTR [rsp + 48]");                       // owned wide path
    emitter.instruction("call __rt_heap_free");                                 // release converted path on failure
    emitter.instruction("mov eax, DWORD PTR [rsp + 56]");                       // restore native error
    emitter.instruction("mov DWORD PTR [rip + __rt_win32_last_error], eax");    // retain native Win32 state
    emitter.instruction("call __rt_win32_errno_from_code");                     // translate to POSIX errno
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], eax");               // publish observable errno
    emitter.instruction("mov rax, -1");                                         // return -1 on failure
    emitter.instruction("add rsp, 88");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lstatfs_conversion_fail");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 84");                // EILSEQ: invalid UTF-8 path
    emitter.instruction("mov rax, -1");                                         // return -1 on failure
    emitter.instruction("add rsp, 88");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits the `__rt_sys_get_temp_dir` shim: retrieves the Windows temp directory
/// via `GetTempPathW` and returns it as an owned UTF-8 elephc string (heap-allocated,
/// tagged as a persisted string in the uniform heap header, matching the
/// `tempnam.rs` stamping convention).
///
/// No SysV input arguments. Returns rax = string pointer, rdx = string length
/// (matches `abi::string_result_regs` for x86_64, so the codegen caller needs
/// no register shuffle after `call __rt_sys_get_temp_dir`). Returns an empty
/// string (rax=0, rdx=0) if the native query, allocation, or strict conversion fails.
pub(super) fn emit_shim_sys_get_temp_dir(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_get_temp_dir");
    emitter.instruction("sub rsp, 72");                                         // shadow space and aligned wide/UTF-8 ownership locals
    emitter.instruction("xor ecx, ecx");                                        // query required WCHAR capacity
    emitter.instruction("xor edx, edx");                                        // no output buffer for the size query
    emitter.instruction("call GetTempPathW");                                   // required WCHAR count including NUL
    emitter.instruction("test eax, eax");                                       // size query succeeded?
    emitter.instruction("jz .Lsys_get_temp_dir_query_fail");                    // capture native failure
    emitter.instruction("mov DWORD PTR [rsp + 40], eax");                       // preserve WCHAR capacity
    emitter.instruction("movsxd rax, eax");                                     // widen capacity for allocation
    emitter.instruction("shl rax, 1");                                          // WCHAR uses two bytes
    emitter.instruction("call __rt_heap_alloc");                                // allocate dynamic UTF-16 buffer
    emitter.instruction("test rax, rax");                                       // wide allocation succeeded?
    emitter.instruction("jz .Lsys_get_temp_dir_alloc_fail");                    // ENOMEM
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // retain owned UTF-16 buffer
    emitter.instruction("mov ecx, DWORD PTR [rsp + 40]");                       // WCHAR capacity
    emitter.instruction("mov rdx, rax");                                        // UTF-16 destination
    emitter.instruction("call GetTempPathW");                                   // retrieve temp directory as UTF-16
    emitter.instruction("test eax, eax");                                       // native retrieval succeeded?
    emitter.instruction("jz .Lsys_get_temp_dir_wide_fail");                     // capture native failure
    emitter.instruction("cmp eax, DWORD PTR [rsp + 40]");                       // did the path grow after the size query?
    emitter.instruction("jae .Lsys_get_temp_dir_range_fail");                   // reject truncated/raced result
    emitter.instruction("mov rdi, QWORD PTR [rsp + 32]");                       // UTF-16 source
    emitter.instruction("xor esi, esi");                                        // query UTF-8 size without a destination
    emitter.instruction("xor edx, edx");                                        // destination byte capacity = zero
    emitter.instruction("call __rt_win_utf16_to_utf8");                         // required UTF-8 bytes including NUL
    emitter.instruction("test eax, eax");                                       // strict size query succeeded?
    emitter.instruction("jz .Lsys_get_temp_dir_wide_fail");                     // capture conversion failure
    emitter.instruction("mov DWORD PTR [rsp + 48], eax");                       // preserve UTF-8 capacity
    emitter.instruction("movsxd rax, eax");                                     // allocation size includes NUL
    emitter.instruction("call __rt_heap_alloc");                                // allocate owned UTF-8 string
    emitter.instruction("test rax, rax");                                       // UTF-8 allocation succeeded?
    emitter.instruction("jz .Lsys_get_temp_dir_utf8_alloc_fail");               // cleanup wide buffer and return ENOMEM
    emitter.instruction(&format!(                                               // owned-string heap kind word
        "mov r10, 0x{:x}",
        sentinels::x86_64_heap_kind_word(1)
    ));
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the buffer as a persisted elephc string
    emitter.instruction("mov QWORD PTR [rsp + 56], rax");                       // retain owned UTF-8 result
    emitter.instruction("mov rdi, QWORD PTR [rsp + 32]");                       // UTF-16 source
    emitter.instruction("mov rsi, rax");                                        // owned UTF-8 destination
    emitter.instruction("mov edx, DWORD PTR [rsp + 48]");                       // destination byte capacity
    emitter.instruction("call __rt_win_utf16_to_utf8");                         // perform strict conversion including NUL
    emitter.instruction("test eax, eax");                                       // final conversion succeeded?
    emitter.instruction("jz .Lsys_get_temp_dir_conversion_fail");               // release both allocations on failure
    emitter.instruction("dec eax");                                             // exclude terminating NUL from elephc length
    emitter.instruction("mov r10, QWORD PTR [rsp + 56]");                       // UTF-8 result base
    emitter.instruction("test eax, eax");                                       // path has at least one byte?
    emitter.instruction("jz .Lsys_get_temp_dir_trim_done");                     // nothing to trim
    emitter.instruction("cmp BYTE PTR [r10 + rax - 1], 92");                    // trailing Windows backslash?
    emitter.instruction("jne .Lsys_get_temp_dir_trim_done");                    // preserve non-separator final byte
    emitter.instruction("dec eax");                                             // strip the trailing backslash
    emitter.label(".Lsys_get_temp_dir_trim_done");
    emitter.instruction("mov BYTE PTR [r10 + rax], 0");                         // terminate after optional trim
    emitter.instruction("mov DWORD PTR [rsp + 48], eax");                       // preserve result length across wide cleanup
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // owned UTF-16 buffer
    emitter.instruction("call __rt_heap_free");                                 // release temporary wide storage
    emitter.instruction("mov rax, QWORD PTR [rsp + 56]");                       // return owned UTF-8 pointer
    emitter.instruction("mov edx, DWORD PTR [rsp + 48]");                       // return UTF-8 byte length
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return owned pointer/length
    emitter.label(".Lsys_get_temp_dir_conversion_fail");
    emitter.instruction("call GetLastError");                                   // capture conversion failure before cleanup
    emitter.instruction("mov DWORD PTR [rsp + 64], eax");                       // preserve native error code
    emitter.instruction("mov rax, QWORD PTR [rsp + 56]");                       // owned UTF-8 allocation
    emitter.instruction("call __rt_heap_free");                                 // release failed result
    emitter.instruction("jmp .Lsys_get_temp_dir_wide_fail_saved");              // cleanup wide allocation and publish error
    emitter.label(".Lsys_get_temp_dir_wide_fail");
    emitter.instruction("call GetLastError");                                   // capture native/conversion failure
    emitter.instruction("mov DWORD PTR [rsp + 64], eax");                       // preserve native error code
    emitter.label(".Lsys_get_temp_dir_wide_fail_saved");
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // owned UTF-16 buffer
    emitter.instruction("call __rt_heap_free");                                 // release temporary wide storage
    emitter.instruction("mov eax, DWORD PTR [rsp + 64]");                       // restore native error code
    emitter.instruction("jmp .Lsys_get_temp_dir_publish_native_error");         // map error and return empty string
    emitter.label(".Lsys_get_temp_dir_query_fail");
    emitter.instruction("call GetLastError");                                   // capture initial query failure
    emitter.label(".Lsys_get_temp_dir_publish_native_error");
    emitter.instruction("mov DWORD PTR [rip + __rt_win32_last_error], eax");    // retain native Win32 state
    emitter.instruction("call __rt_win32_errno_from_code");                     // translate to POSIX errno
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], eax");               // publish observable errno
    emitter.instruction("jmp .Lsys_get_temp_dir_fail");                         // return empty string
    emitter.label(".Lsys_get_temp_dir_range_fail");
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // owned UTF-16 buffer
    emitter.instruction("call __rt_heap_free");                                 // release buffer after a size race
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 34");                // ERANGE
    emitter.instruction("jmp .Lsys_get_temp_dir_fail");                         // return empty string
    emitter.label(".Lsys_get_temp_dir_utf8_alloc_fail");
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // owned UTF-16 buffer
    emitter.instruction("call __rt_heap_free");                                 // release wide storage after allocation failure
    emitter.label(".Lsys_get_temp_dir_alloc_fail");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 12");                // ENOMEM
    emitter.label(".Lsys_get_temp_dir_fail");
    emitter.instruction("xor rax, rax");                                        // empty string pointer
    emitter.instruction("xor rdx, rdx");                                        // empty string length
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits a getdents shim — Windows doesn't have getdents, return -1 (ENOSYS).
/// PHP uses FindFirstFileExW/FindNextFileW for directory iteration on Windows.
pub(super) fn emit_shim_getdents(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_getdents");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 38");                // ENOSYS: callers must use the FindFirstFileExW DIR layer
    emitter.instruction("mov rax, -1");                                         // return -1 (not supported on Windows)
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits a creat shim — maps to __rt_sys_open with O_WRONLY|O_CREAT|O_TRUNC.
pub(super) fn emit_shim_creat(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_creat");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("mov rsi, 0x240");                                      // O_WRONLY | O_CREAT | O_TRUNC
    emitter.instruction("call __rt_sys_open");                                  // delegate to open shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits a newfstatat shim — delegates to msvcrt stat on Windows.
pub(super) fn emit_shim_newfstatat(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_newfstatat");
    emitter.instruction("sub rsp, 8");                                          // align stack
    emitter.instruction("mov rdi, rsi");                                        // path (skip dirfd arg1)
    emitter.instruction("mov rsi, rdx");                                        // stat buffer
    emitter.instruction("call __rt_sys_stat");                                  // delegate to stat shim
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits the `__rt_sys_opendir` shim: opens a directory stream via
/// `FindFirstFileExW` over `<path>\*`, the real Windows port of POSIX
/// `opendir()` (the same primitive php-src's own win32 dirent layer uses).
///
/// Allocates a persistent DIR state block through `__rt_heap_alloc` (624
/// bytes total), shared by `__rt_sys_readdir`/`__rt_sys_closedir`/
/// `__rt_sys_rewinddir` below:
///   +0    HANDLE hFind
///   +8    i64 first_pending (1 = the WIN32_FIND_DATAW at +24 is
///         FindFirstFileExW's still-unconsumed first entry; 0 = the next
///         `readdir()` call must fetch a fresh entry via `FindNextFileW`)
///   +16   WCHAR* pattern — the heap-allocated "<path>\*" search pattern,
///         kept alive (not freed until `closedir`) so `rewinddir` can
///         `FindClose` + reopen it
///   +24   WIN32_FIND_DATAW (592 bytes; `cFileName` at +24+44 = +68)
///   +616  a Linux-`dirent`-layout scratch buffer that `__rt_sys_readdir`
///         fills per call — `d_name` lands at `Platform::dirent_name_offset()`
///         (19 on Windows, mirroring the Linux layout `opendir.rs`/
///         `scandir.rs`/`readdir.rs` already read on every other target)
///
/// Returns the DIR state pointer in rax, or NULL (rax=0) on failure — the
/// exact sentinel `opendir.rs`'s `test rax,rax; jz` already handles.
/// SysV: rdi = a NUL-terminated path C string (already produced by
/// `__rt_cstr` at the call site).
pub(super) fn emit_shim_opendir(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_opendir");
    emitter.instruction("sub rsp, 104");                                        // shadow(32), FindFirstFileExW stack args, and aligned locals
    emitter.instruction("mov QWORD PTR [rsp + 32], rdi");                       // save the path C-string pointer
    // -- measure the path length --
    emitter.instruction("xor rax, rax");                                        // scan index
    emitter.label(".Lopendir_strlen");
    emitter.instruction("cmp BYTE PTR [rdi + rax], 0");                         // terminating NUL?
    emitter.instruction("je .Lopendir_strlen_done");                            // path length is now known
    emitter.instruction("inc rax");                                             // count one more path byte
    emitter.instruction("jmp .Lopendir_strlen");                                // continue scanning the path
    emitter.label(".Lopendir_strlen_done");
    emitter.instruction("mov QWORD PTR [rsp + 40], rax");                       // save the path length
    // -- build the "<path>\*" search pattern --
    emitter.instruction("add rax, 3");                                          // + '\' + '*' + NUL
    emitter.instruction("call __rt_heap_alloc");                                // rax = the search-pattern buffer
    emitter.instruction("mov QWORD PTR [rsp + 48], rax");                       // save the pattern buffer pointer
    emitter.instruction("mov r10, rax");                                        // dest cursor
    emitter.instruction("mov r11, QWORD PTR [rsp + 32]");                       // src = the original path
    emitter.instruction("xor r9, r9");                                          // copy index
    emitter.instruction("mov r8, QWORD PTR [rsp + 40]");                        // path length
    emitter.label(".Lopendir_copy_path");
    emitter.instruction("cmp r9, r8");                                          // copied the whole path?
    emitter.instruction("jae .Lopendir_copy_path_done");                        // → append the "\*" suffix
    emitter.instruction("movzx eax, BYTE PTR [r11 + r9]");                      // load the next path byte
    emitter.instruction("mov BYTE PTR [r10 + r9], al");                         // store it into the pattern buffer
    emitter.instruction("inc r9");                                              // advance the copy index
    emitter.instruction("jmp .Lopendir_copy_path");                             // continue copying the path
    emitter.label(".Lopendir_copy_path_done");
    emitter.instruction("mov BYTE PTR [r10 + r8], 92");                         // append '\'
    emitter.instruction("lea rax, [r8 + 1]");                                   // index of the wildcard byte
    emitter.instruction("mov BYTE PTR [r10 + rax], 42");                        // append '*'
    emitter.instruction("lea rax, [r8 + 2]");                                   // index of the terminator
    emitter.instruction("mov BYTE PTR [r10 + rax], 0");                         // NUL-terminate the search pattern
    // -- convert the UTF-8 pattern once and retain the UTF-16 form for rewinddir --
    emitter.instruction("mov rdi, QWORD PTR [rsp + 48]");                       // UTF-8 search pattern
    emitter.instruction("call __rt_win_utf8_to_utf16");                         // allocate a strict UTF-16 pattern
    emitter.instruction("mov QWORD PTR [rsp + 64], rax");                       // preserve the converted pattern
    emitter.instruction("mov rax, QWORD PTR [rsp + 48]");                       // temporary UTF-8 pattern
    emitter.instruction("call __rt_heap_free");                                 // only the wide pattern remains owned by DIR
    emitter.instruction("mov rax, QWORD PTR [rsp + 64]");                       // converted search pattern
    emitter.instruction("test rax, rax");                                       // conversion succeeded?
    emitter.instruction("jz .Lopendir_conversion_fail");                        // reject invalid UTF-8 paths cleanly
    emitter.instruction("mov QWORD PTR [rsp + 48], rax");                       // retain UTF-16 pattern in the normal cleanup slot
    // -- allocate the persistent DIR state block --
    emitter.instruction("mov rax, 1680");                                       // header + WIN32_FIND_DATAW + UTF-8 dirent scratch
    emitter.instruction("call __rt_heap_alloc");                                // rax = the DIR state block
    emitter.instruction("mov QWORD PTR [rsp + 56], rax");                       // save the DIR state pointer
    emitter.instruction("test rax, rax");                                       // state allocation succeeded?
    emitter.instruction("jz .Lopendir_fail");                                   // release the converted pattern on allocation failure
    // -- FindFirstFileExW(pattern, FindExInfoBasic, &data, FindExSearchNameMatch, NULL, LARGE_FETCH) --
    emitter.instruction("mov rcx, QWORD PTR [rsp + 48]");                       // lpFileName = the search pattern
    emitter.instruction("mov edx, 1");                                          // fInfoLevelId = FindExInfoBasic
    emitter.instruction("mov r8, QWORD PTR [rsp + 56]");                        // DIR state
    emitter.instruction("add r8, 24");                                          // &DIRstate.data (WIN32_FIND_DATAW)
    emitter.instruction("xor r9d, r9d");                                        // fSearchOp = FindExSearchNameMatch
    emitter.instruction("mov QWORD PTR [rsp + 32], 0");                         // lpSearchFilter = NULL
    emitter.instruction("mov QWORD PTR [rsp + 40], 2");                         // FIND_FIRST_EX_LARGE_FETCH
    emitter.instruction("call FindFirstFileExW");                               // open Unicode enumeration over the first entry
    emitter.instruction("cmp rax, -1");                                         // INVALID_HANDLE_VALUE?
    emitter.instruction("je .Lopendir_fail");                                   // opendir failed
    emitter.instruction("mov r10, QWORD PTR [rsp + 56]");                       // DIR state
    emitter.instruction("mov QWORD PTR [r10], rax");                            // DIRstate.hFind = the search handle
    emitter.instruction("mov QWORD PTR [r10 + 8], 1");                          // DIRstate.first_pending = 1
    emitter.instruction("mov rax, QWORD PTR [rsp + 48]");                       // the search pattern buffer
    emitter.instruction("mov QWORD PTR [r10 + 16], rax");                       // DIRstate.pattern = the pattern (kept alive for rewinddir)
    emitter.instruction("mov rax, r10");                                        // return the DIR state pointer
    emitter.instruction("add rsp, 104");                                        // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lopendir_fail");
    emitter.instruction("mov rax, QWORD PTR [rsp + 48]");                       // the unused search pattern buffer
    emitter.instruction("call __rt_heap_free");                                 // release it
    emitter.instruction("mov rax, QWORD PTR [rsp + 56]");                       // the unused DIR state block
    emitter.instruction("test rax, rax");                                       // was the state block allocated?
    emitter.instruction("jz .Lopendir_fail_done");                              // skip freeing a NULL allocation
    emitter.instruction("call __rt_heap_free");                                 // release it
    emitter.label(".Lopendir_fail_done");
    emitter.instruction("xor eax, eax");                                        // sentinel: NULL DIR* (opendir failed)
    emitter.instruction("add rsp, 104");                                        // restore stack
    emitter.instruction("ret");                                                 // return the failure sentinel
    emitter.label(".Lopendir_conversion_fail");
    emitter.instruction("xor eax, eax");                                        // invalid UTF-8 cannot name a Windows path
    emitter.instruction("add rsp, 104");                                        // restore stack
    emitter.instruction("ret");                                                 // return NULL
    emitter.blank();
}

/// Emits the `__rt_sys_readdir` shim: advances the DIR state opened by
/// `__rt_sys_opendir` above (`FindNextFileW`, or the `FindFirstFileExW` entry
/// already pending from `opendir`) and copies the matched entry name into
/// the Linux-`dirent`-layout scratch buffer embedded in the DIR state —
/// `d_name` lands at `Platform::dirent_name_offset()`, exactly what every
/// shared consumer (`opendir.rs`/`scandir.rs`/`readdir.rs`) already reads on
/// every other target. php-src semantics: NO sorting (raw OS enumeration
/// order) and `.`/`..` ARE returned — contrast with the `glob` shim in
/// `shims_c_symbols.rs`, which always filters them.
///
/// Returns a pointer to the scratch dirent, or NULL (rax=0) at
/// end-of-directory — the sentinel every consumer already treats as "no
/// more entries". SysV: rdi = DIR state pointer.
pub(super) fn emit_shim_readdir(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_readdir");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + pad(8), 16B aligned
    emitter.instruction("cmp QWORD PTR [rdi + 8], 0");                          // DIRstate.first_pending?
    emitter.instruction("je .Lreaddir_next");                                   // 0: fetch a fresh entry via FindNextFileW
    emitter.instruction("mov QWORD PTR [rdi + 8], 0");                          // consume the pending FindFirstFileExW entry
    emitter.instruction("jmp .Lreaddir_build");                                 // the data at DIRstate+24 already holds this entry
    emitter.label(".Lreaddir_next");
    emitter.instruction("mov rcx, QWORD PTR [rdi]");                            // DIRstate.hFind
    emitter.instruction("lea rdx, [rdi + 24]");                                 // &DIRstate.data
    emitter.instruction("call FindNextFileW");                                  // advance to the next Unicode directory entry
    emitter.instruction("test eax, eax");                                       // more entries?
    emitter.instruction("jnz .Lreaddir_build");                                 // yes: convert the returned entry
    emitter.instruction("call GetLastError");                                   // distinguish EOF from a native enumeration failure
    emitter.instruction("cmp eax, 18");                                         // ERROR_NO_MORE_FILES?
    emitter.instruction("je .Lreaddir_end");                                    // clean end of directory leaves errno unchanged
    emitter.instruction("jmp .Lreaddir_native_error");                          // publish the enumeration failure
    emitter.label(".Lreaddir_build");
    // -- zero the Linux dirent header fields; only d_name (below) is read by any consumer --
    emitter.instruction("lea rax, [rdi + 616]");                                // scratch dirent after WIN32_FIND_DATAW
    emitter.instruction("mov QWORD PTR [rax], 0");                              // d_ino = 0
    emitter.instruction("mov QWORD PTR [rax + 8], 0");                          // d_off = 0
    emitter.instruction("mov DWORD PTR [rax + 16], 0");                         // d_reclen = 0, d_type = 0
    // -- convert cFileName from UTF-16 into PHP's UTF-8 dirent name --
    emitter.instruction("mov QWORD PTR [rsp + 32], rdi");                       // preserve DIR state across the helper call
    emitter.instruction("lea rsi, [rax + 19]");                                 // destination = dirent.d_name
    emitter.instruction("mov rdx, 1040");                                       // worst-case UTF-8 capacity for MAX_PATH WCHARs
    emitter.instruction("lea rdi, [rdi + 68]");                                 // source = WIN32_FIND_DATAW.cFileName
    emitter.instruction("call __rt_win_utf16_to_utf8");                         // convert the native entry name
    emitter.instruction("test eax, eax");                                       // conversion succeeded?
    emitter.instruction("jnz .Lreaddir_converted");                             // return the converted entry
    emitter.instruction("call GetLastError");                                   // capture strict UTF conversion failure
    emitter.instruction("jmp .Lreaddir_native_error");                          // publish rather than pretending it was EOF
    emitter.label(".Lreaddir_converted");
    emitter.instruction("mov rdi, QWORD PTR [rsp + 32]");                       // restore DIR state
    emitter.instruction("lea rax, [rdi + 616]");                                // return the scratch dirent pointer
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return the scratch dirent pointer (still in rax)
    emitter.label(".Lreaddir_native_error");
    emitter.instruction("mov DWORD PTR [rip + __rt_win32_last_error], eax");    // retain native Win32 state
    emitter.instruction("call __rt_win32_errno_from_code");                     // translate failure to POSIX errno
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], eax");               // publish observable errno
    emitter.label(".Lreaddir_end");
    emitter.instruction("xor eax, eax");                                        // sentinel: NULL dirent (end-of-directory)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return the failure sentinel
    emitter.blank();
}

/// Emits the `__rt_sys_closedir` shim: closes the Win32 search handle
/// (`FindClose`) and releases the DIR state block `__rt_sys_opendir`
/// allocated, including the kept-alive search pattern string. Returns zero on
/// success, or publishes the translated `FindClose` error and returns -1.
/// SysV: rdi = DIR state pointer.
pub(super) fn emit_shim_closedir(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_closedir");
    emitter.instruction("sub rsp, 56");                                         // shadow plus saved state and close result, 16B aligned
    emitter.instruction("mov QWORD PTR [rsp + 32], rdi");                       // save the DIR state pointer
    emitter.instruction("mov rcx, QWORD PTR [rdi]");                            // DIRstate.hFind
    emitter.instruction("call FindClose");                                      // release the Win32 search handle
    emitter.instruction("test eax, eax");                                       // close succeeded?
    emitter.instruction("jnz .Lclosedir_close_ok");                             // preserve zero success marker
    emitter.instruction("call GetLastError");                                   // capture close failure before heap cleanup
    emitter.instruction("mov DWORD PTR [rsp + 40], eax");                       // retain native error across cleanup
    emitter.instruction("jmp .Lclosedir_cleanup");                              // release owned allocations on every path
    emitter.label(".Lclosedir_close_ok");
    emitter.instruction("mov DWORD PTR [rsp + 40], 0");                         // success marker
    emitter.label(".Lclosedir_cleanup");
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // DIR state pointer
    emitter.instruction("mov rax, QWORD PTR [rax + 16]");                       // DIRstate.pattern
    emitter.instruction("call __rt_heap_free");                                 // release the kept-alive search pattern
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // DIR state pointer
    emitter.instruction("call __rt_heap_free");                                 // release the DIR state block itself
    emitter.instruction("mov eax, DWORD PTR [rsp + 40]");                       // restore close result/error
    emitter.instruction("test eax, eax");                                       // was FindClose successful?
    emitter.instruction("jnz .Lclosedir_error");                                // publish close failure after cleanup
    emitter.instruction("xor eax, eax");                                        // closedir success
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lclosedir_error");
    emitter.instruction("mov DWORD PTR [rip + __rt_win32_last_error], eax");    // retain native Win32 state
    emitter.instruction("call __rt_win32_errno_from_code");                     // translate failure to POSIX errno
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], eax");               // publish observable errno
    emitter.instruction("mov rax, -1");                                         // POSIX closedir failure
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return failure
    emitter.blank();
}

/// Emits the `__rt_sys_rewinddir` shim: rewinds a directory stream back to
/// its first entry via `FindClose` + `FindFirstFileExW` against the DIR
/// state's kept-alive search pattern — Windows has no native "seek to
/// start" primitive for a search handle, so php-src's own win32 rewinddir
/// also closes and reopens the search. SysV: rdi = DIR state pointer. Void
/// return, matching the shared `rewinddir.rs` consumer, which discards the
/// result.
pub(super) fn emit_shim_rewinddir(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_rewinddir");
    emitter.instruction("sub rsp, 56");                                         // shadow space, two stack args, and aligned state spill
    emitter.instruction("mov QWORD PTR [rsp + 48], rdi");                       // save the DIR state pointer
    emitter.instruction("mov rcx, QWORD PTR [rdi]");                            // DIRstate.hFind
    emitter.instruction("call FindClose");                                      // close the current search handle
    emitter.instruction("mov rdi, QWORD PTR [rsp + 48]");                       // reload the DIR state pointer
    emitter.instruction("mov rcx, QWORD PTR [rdi + 16]");                       // DIRstate.pattern (the kept-alive search pattern)
    emitter.instruction("mov edx, 1");                                          // fInfoLevelId = FindExInfoBasic
    emitter.instruction("lea r8, [rdi + 24]");                                  // &DIRstate.data
    emitter.instruction("xor r9d, r9d");                                        // fSearchOp = FindExSearchNameMatch
    emitter.instruction("mov QWORD PTR [rsp + 32], 0");                         // lpSearchFilter = NULL
    emitter.instruction("mov QWORD PTR [rsp + 40], 2");                         // FIND_FIRST_EX_LARGE_FETCH
    emitter.instruction("call FindFirstFileExW");                               // reopen Unicode enumeration at the first entry
    emitter.instruction("mov rdi, QWORD PTR [rsp + 48]");                       // reload the DIR state pointer
    emitter.instruction("mov QWORD PTR [rdi], rax");                            // DIRstate.hFind = the reopened handle (or INVALID_HANDLE_VALUE)
    emitter.instruction("mov QWORD PTR [rdi + 8], 0");                          // clear first_pending by default
    emitter.instruction("cmp rax, -1");                                         // did the reopen fail?
    emitter.instruction("je .Lrewinddir_done");                                 // failure: leave first_pending cleared — readdir sees the invalid handle fail cleanly
    emitter.instruction("mov QWORD PTR [rdi + 8], 1");                          // success: the just-fetched entry is unconsumed
    emitter.label(".Lrewinddir_done");
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits the `__rt_sys_mkstemp` shim: creates a unique temp file by
/// rewriting the mutable template's trailing "XXXXXX" placeholder in place
/// — matching POSIX `mkstemp()`'s exact contract of mutating the SAME
/// buffer at the SAME length, which `tempnam.rs`/`streams_ext.rs` both rely
/// on (they compute the returned string's length arithmetically from the
/// original directory/prefix lengths rather than re-scanning for NUL, so
/// the rewritten suffix must stay exactly 6 bytes) — with pseudo-random
/// alphanumeric characters, then exclusively creating that path via
/// `CreateFileW(..., CREATE_NEW, ...)`. `CREATE_NEW` fails with
/// `ERROR_FILE_EXISTS`/`ERROR_ALREADY_EXISTS` on a collision, giving the
/// same atomic-uniqueness guarantee real `mkstemp()` provides; a collision
/// retries with a freshly reseeded suffix (bounded at 8 attempts).
///
/// On success, explicitly clears the read-only attribute some Windows
/// configurations default new files to (php-src's own temp-file creation
/// does the equivalent `chmod(path, 0600)` for the same reason) and returns
/// the new file as a Microsoft CRT descriptor via `_open_osfhandle`, matching
/// `__rt_sys_open` and making the result valid for `_dup`/`_dup2` as well as
/// direct Win32 I/O after `__rt_fd_to_handle` conversion.
///
/// SysV: rdi = the mutable template buffer (NUL-terminated, ending in
/// "XXXXXX"). Returns the open fd in eax, or -1 on failure.
pub(super) fn emit_shim_mkstemp(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_mkstemp");
    emitter.instruction("sub rsp, 104");                                        // shadow(32)+CreateFileW stack args(24)+locals(48), 16B aligned
    emitter.instruction("mov QWORD PTR [rsp + 56], rdi");                       // save the template buffer pointer
    // -- measure the template length; the last 6 bytes before the NUL are always "XXXXXX" --
    emitter.instruction("xor rax, rax");                                        // scan index
    emitter.label(".Lmkstemp_strlen");
    emitter.instruction("cmp BYTE PTR [rdi + rax], 0");                         // terminating NUL?
    emitter.instruction("je .Lmkstemp_strlen_done");                            // template length is now known
    emitter.instruction("inc rax");                                             // count one more template byte
    emitter.instruction("jmp .Lmkstemp_strlen");                                // continue scanning the template
    emitter.label(".Lmkstemp_strlen_done");
    emitter.instruction("lea rax, [rdi + rax - 6]");                            // xpos = the start of the "XXXXXX" placeholder
    emitter.instruction("mov QWORD PTR [rsp + 64], rax");                       // save xpos
    emitter.instruction("mov QWORD PTR [rsp + 88], 0");                         // attempt = 0
    emitter.label(".Lmkstemp_retry");
    // -- reseed from QueryPerformanceCounter, mixed with the buffer address and the attempt index --
    emitter.instruction("lea rcx, [rsp + 72]");                                 // &LARGE_INTEGER out-param
    emitter.instruction("call QueryPerformanceCounter");                        // high-resolution counter (changes every retry)
    emitter.instruction("mov rax, QWORD PTR [rsp + 72]");                       // the 64-bit counter value
    emitter.instruction("mov rcx, QWORD PTR [rsp + 56]");                       // the template buffer address
    emitter.instruction("xor rax, rcx");                                        // mix in the buffer address
    emitter.instruction("mov rcx, QWORD PTR [rsp + 88]");                       // the attempt index
    emitter.instruction("imul rcx, rcx, 40503");                                // spread the attempt index across the seed (Knuth multiplicative constant)
    emitter.instruction("xor rax, rcx");                                        // mix in the attempt index
    emitter.instruction("mov QWORD PTR [rsp + 80], rax");                       // save the seed
    for i in 0..6u32 {
        emitter.instruction("mov rax, QWORD PTR [rsp + 80]");                   // reload the seed
        emitter.instruction("xor rdx, rdx");                                    // clear the high dividend half
        emitter.instruction("mov rcx, 36");                                     // 36 alphanumeric characters
        emitter.instruction("div rcx");                                         // rdx = seed % 36
        emitter.instruction("cmp rdx, 10");                                     // digit or letter?
        emitter.instruction(&format!("jae .Lmkstemp_alpha_{}", i));             // → letter
        emitter.instruction("add rdx, 48");                                     // '0' + n
        emitter.instruction(&format!("jmp .Lmkstemp_store_{}", i));             // → store the character
        emitter.label(&format!(".Lmkstemp_alpha_{}", i));
        emitter.instruction("add rdx, 87");                                     // 'a' + (n - 10)
        emitter.label(&format!(".Lmkstemp_store_{}", i));
        emitter.instruction("mov r8, QWORD PTR [rsp + 64]");                    // xpos
        emitter.instruction(&format!("mov BYTE PTR [r8 + {}], dl", i));         // write the generated character
        emitter.instruction("mov rax, QWORD PTR [rsp + 80]");                   // reload the seed
        emitter.instruction("mov rcx, 1103515245");                             // classic LCG multiplier
        emitter.instruction("mul rcx");                                         // rax = seed * multiplier
        emitter.instruction("add rax, 12345");                                  // classic LCG increment
        emitter.instruction("mov QWORD PTR [rsp + 80], rax");                   // save the advanced seed
    }
    // -- convert each candidate to UTF-16, then create it atomically with CreateFileW --
    emitter.instruction("mov rdi, QWORD PTR [rsp + 56]");                       // UTF-8 template holding the fresh suffix
    emitter.instruction("call __rt_win_utf8_to_utf16");                         // allocate the native Unicode path
    emitter.instruction("mov QWORD PTR [rsp + 72], rax");                       // preserve the candidate wide path
    emitter.instruction("test rax, rax");                                       // conversion succeeded?
    emitter.instruction("jz .Lmkstemp_hard_fail");                              // invalid UTF-8 cannot name a Windows temp file
    emitter.instruction("mov rcx, rax");                                        // lpFileName = converted candidate
    emitter.instruction("mov rdx, 0xC0000000");                                 // GENERIC_READ | GENERIC_WRITE
    emitter.instruction("xor r8, r8");                                          // dwShareMode = 0 (exclusive, matching mkstemp()'s own guarantee)
    emitter.instruction("xor r9, r9");                                          // lpSecurityAttributes = NULL
    emitter.instruction("mov QWORD PTR [rsp + 32], 1");                         // dwCreationDisposition = CREATE_NEW (fails if the path exists)
    emitter.instruction("mov QWORD PTR [rsp + 40], 128");                       // dwFlagsAndAttributes = FILE_ATTRIBUTE_NORMAL
    emitter.instruction("mov QWORD PTR [rsp + 48], 0");                         // hTemplateFile = NULL
    emitter.instruction("call CreateFileW");                                    // exclusively create the Unicode temp file
    emitter.instruction("mov QWORD PTR [rsp + 96], rax");                       // preserve HANDLE across cleanup and error lookup
    emitter.instruction("cmp rax, -1");                                         // INVALID_HANDLE_VALUE?
    emitter.instruction("jne .Lmkstemp_success");                               // created: return this handle as the fd
    emitter.instruction("call GetLastError");                                   // inspect why CreateFileW failed
    emitter.instruction("mov DWORD PTR [rsp + 80], eax");                       // preserve the raw failure code across heap cleanup
    emitter.instruction("mov rax, QWORD PTR [rsp + 72]");                       // wide candidate allocation
    emitter.instruction("call __rt_heap_free");                                 // release failed candidate path
    emitter.instruction("mov eax, DWORD PTR [rsp + 80]");                       // reload the original CreateFileW error
    emitter.instruction("cmp eax, 80");                                         // ERROR_FILE_EXISTS?
    emitter.instruction("je .Lmkstemp_next_attempt");                           // → retry with a fresh suffix
    emitter.instruction("cmp eax, 183");                                        // ERROR_ALREADY_EXISTS?
    emitter.instruction("je .Lmkstemp_next_attempt");                           // → retry with a fresh suffix
    emitter.instruction("mov DWORD PTR [rip + __rt_win32_last_error], eax");    // preserve the native hard-failure code
    emitter.instruction("call __rt_win32_errno_from_code");                     // translate the hard failure to POSIX errno
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], eax");               // publish the translated temp-file failure
    emitter.instruction("jmp .Lmkstemp_hard_fail");                             // any other failure: give up immediately
    emitter.label(".Lmkstemp_next_attempt");
    emitter.instruction("mov rax, QWORD PTR [rsp + 88]");                       // reload the attempt index
    emitter.instruction("inc rax");                                             // advance to the next attempt
    emitter.instruction("mov QWORD PTR [rsp + 88], rax");                       // save the updated attempt index
    emitter.instruction("cmp rax, 8");                                          // exhausted the retry budget?
    emitter.instruction("jl .Lmkstemp_retry");                                  // retry with a fresh suffix
    emitter.instruction("mov DWORD PTR [rip + __rt_win32_last_error], 183");    // collision budget exhausted with ERROR_ALREADY_EXISTS
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 17");                // publish EEXIST for the exhausted collision budget
    emitter.label(".Lmkstemp_hard_fail");
    emitter.instruction("mov rax, -1");                                         // sentinel: mkstemp failed
    emitter.instruction("add rsp, 104");                                        // restore stack
    emitter.instruction("ret");                                                 // return the failure sentinel
    emitter.label(".Lmkstemp_success");
    emitter.instruction("mov rcx, QWORD PTR [rsp + 72]");                       // final unique Unicode path
    emitter.instruction("mov rdx, 128");                                        // FILE_ATTRIBUTE_NORMAL: guards against a read-only default on some configs
    emitter.instruction("call SetFileAttributesW");                             // ensure the Unicode temp file is writable
    emitter.instruction("mov rax, QWORD PTR [rsp + 72]");                       // wide candidate allocation
    emitter.instruction("call __rt_heap_free");                                 // release path after all Win32 path calls
    emitter.instruction("mov rcx, QWORD PTR [rsp + 96]");                       // raw Win32 handle to adopt
    emitter.instruction("mov edx, 0x8002");                                     // _O_BINARY | _O_RDWR
    emitter.instruction("call _open_osfhandle");                                // create a CRT descriptor for dup/read/write/close
    emitter.instruction("cmp eax, -1");                                         // descriptor allocation failed?
    emitter.instruction("je .Lmkstemp_crt_fail");                               // close the unowned raw handle
    emitter.instruction("add rsp, 104");                                        // restore stack
    emitter.instruction("ret");                                                 // return the CRT file descriptor
    emitter.label(".Lmkstemp_crt_fail");
    emitter.instruction("mov rcx, QWORD PTR [rsp + 96]");                       // HANDLE remains caller-owned after conversion failure
    emitter.instruction("call CloseHandle");                                    // avoid leaking the temporary file handle
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 24");                // EMFILE: no CRT descriptor slot available
    emitter.instruction("mov rax, -1");                                         // mkstemp failure
    emitter.instruction("add rsp, 104");                                        // restore stack
    emitter.instruction("ret");                                                 // return failure
    emitter.blank();
}
