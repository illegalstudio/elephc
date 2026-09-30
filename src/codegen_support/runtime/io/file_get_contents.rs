//! Purpose:
//! Emits the `__rt_file_get_contents`, `__rt_cstr` runtime helper assembly for file get contents.
//! Keeps PHP filesystem/resource behavior, libc calls, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - I/O helpers bridge PHP strings, resources, descriptors, and libc calls while returning runtime arrays or pointer/length strings.

use crate::codegen_support::{emit::Emitter, platform::Arch};
use crate::codegen_support::abi;
use crate::codegen_support::runtime::data::{FGC_READ_FAILED_HEAD, FGC_READ_FAILED_MID};

/// The extra bytes php asks for on top of the size `stat` reported.
///
/// php's `_php_stream_copy_to_mem` allocates `st_size + CHUNK` so ONE read can also see the end
/// of the file. The number is user-visible: a failed read names it, and an empty directory whose
/// `st_size` is 64 reports `Read of 8256 bytes failed`.
const FGC_READ_CHUNK: i64 = 8192;

/// Emits `__rt_file_get_contents`, the runtime helper that reads an entire file into an owned heap buffer.
/// Dispatches to the x86_64 or ARM64 implementation based on `emitter.target`.
/// Input: x1=filename pointer, x2=filename length (PHP string encoding)
/// Output: x1=heap buffer pointer, x2=bytes read (caller owns the buffer)
/// Failure: returns x1=0, x2=0 and emits a "Failed to open stream" warning via `__rt_diag_warning`.
/// Uses stat64 to determine file size, then open+read+close for the actual I/O.
/// Calls `__rt_cstr` to null-terminate the filename before passing to stat/open.
pub fn emit_file_get_contents(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_file_get_contents_linux_x86_64(emitter);
        return;
    }

    let plat = emitter.platform;
    let stat_buf = plat.stat_buf_size(emitter.target.arch);
    // Layout: [0..8) path ptr, [8..16) file size, [16..16+stat_buf) stat buffer,
    //         then fd, heap ptr, bytes read, + saved frame regs
    let stat_base = 16;
    let st_size_abs = stat_base + plat.stat_size_offset();
    let locals_start = stat_base + stat_buf;
    let fd_off = locals_start;                // +0: fd
    let heap_off = locals_start + 8;          // +8: heap buffer ptr
    let bread_off = locals_start + 16;        // +16: bytes read
    let frame_size = (locals_start + 24 + 16 + 15) & !15; // +24 locals + 16 saved regs, aligned
    let save_offset = frame_size - 16;

    emitter.blank();
    emitter.comment("--- runtime: file_get_contents ---");
    emitter.label_global("__rt_file_get_contents");

    // -- set up stack frame --
    emitter.instruction(&format!("sub sp, sp, #{}", frame_size));               // allocate stack for stat buf + locals + frame
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", save_offset));      // save frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{}", save_offset));             // establish new frame pointer

    // -- null-terminate the filename --
    emitter.instruction("bl __rt_path_cstr");                                   // convert filename to C string, x0=cstr path
    emitter.instruction("str x0, [sp, #0]");                                    // save null-terminated path pointer

    // -- call stat64 to get file size --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload path for stat64
    emitter.instruction(&format!("add x1, sp, #{}", stat_base));                // pointer to stat buffer on stack
    emitter.syscall(338);
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // compare the Linux stat result against the success sentinel
    }
    emitter.instruction(&plat.branch_on_syscall_success("__rt_file_get_contents_stat_ok")); // continue only when stat succeeded
    emitter.instruction("b __rt_file_get_contents_fail");                       // return an empty string and warn when stat fails
    emitter.label("__rt_file_get_contents_stat_ok");

    // -- extract file size from stat struct --
    emitter.instruction(&format!("ldr x9, [sp, #{}]", st_size_abs));            // load st_size from stat struct
    emitter.instruction("str x9, [sp, #8]");                                    // save file size on stack

    // -- open the file for reading --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload null-terminated path
    emitter.instruction("mov x1, #0");                                          // O_RDONLY = 0
    emitter.instruction("mov x2, #0");                                          // mode not needed for O_RDONLY
    emitter.syscall(5);
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // compare the Linux open result against the success sentinel
    }
    emitter.instruction(&plat.branch_on_syscall_success("__rt_file_get_contents_open_ok")); // continue only when open succeeded
    emitter.instruction("b __rt_file_get_contents_fail");                       // return an empty string and warn when open fails
    emitter.label("__rt_file_get_contents_open_ok");
    emitter.instruction(&format!("str x0, [sp, #{}]", fd_off));                 // save fd on stack

    // -- allocate heap buffer for file contents --
    //
    // php sizes this read `st_size + CHUNK`, not `st_size`: `_php_stream_copy_to_mem` adds one
    // chunk so a single read can also SEE the end of the file. The number is visible — a failed
    // read names it, `Read of 8256 bytes failed` for an empty directory — so asking for exactly
    // `st_size` reported a count php never asks for.
    emitter.instruction("ldr x0, [sp, #8]");                                    // the size stat reported
    emitter.instruction(&format!("mov x9, #{}", FGC_READ_CHUNK));               // php's own read chunk
    emitter.instruction("add x0, x0, x9");                                      // ask for one chunk more
    emitter.instruction("str x0, [sp, #8]");                                    // the read and the message both use it
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate buffer, x0=pointer
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = persisted elephc string
    emitter.instruction("str x9, [x0, #-8]");                                   // store string kind in the uniform heap header
    emitter.instruction(&format!("str x0, [sp, #{}]", heap_off));               // save heap buffer pointer

    // -- read entire file into buffer --
    emitter.instruction(&format!("ldr x0, [sp, #{}]", fd_off));                 // reload fd
    emitter.instruction(&format!("ldr x1, [sp, #{}]", heap_off));               // buffer pointer for read
    emitter.instruction("ldr x2, [sp, #8]");                                    // file size plus one chunk
    emitter.syscall(3);
    // A FAILED read is not a byte count. php answers `""` for one — the open succeeded, so
    // there is a string, and it is empty — and elephc stored the syscall result as the length:
    // on macOS a failed `read(2)` answers the errno itself, so reading a DIRECTORY produced a
    // 21-byte string of uninitialised heap (`EISDIR`), which `file_get_contents()` handed back
    // and `copy()` wrote out.
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux answers -errno
    }
    emitter.instruction(&plat.branch_on_syscall_success("__rt_fgc_read_ok"));
    // php says so out loud, naming the byte count it ASKED for — the size `stat` reported — the
    // errno, and the system's own text for it. The pieces go out one call at a time, which is
    // how every other composed diagnostic in this runtime is written.
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction("neg x0, x0");                                      // Linux answers -errno
    }
    emitter.instruction(&format!("str x0, [sp, #{}]", bread_off));              // park the errno; the slot is written again below
    abi::emit_symbol_address(emitter, "x1", "_fgc_read_failed_head");
    emitter.instruction(&format!("mov x2, #{}", FGC_READ_FAILED_HEAD.len()));
    emitter.instruction("bl __rt_diag_warning");                                // honours @ and the filter scope
    emitter.instruction("ldr x0, [sp, #8]");                                    // the size the read asked for
    emitter.instruction("bl __rt_itoa");                                        // decimal digits into x1/x2
    emitter.instruction("bl __rt_diag_warning");
    abi::emit_symbol_address(emitter, "x1", "_fgc_read_failed_mid");
    emitter.instruction(&format!("mov x2, #{}", FGC_READ_FAILED_MID.len()));
    emitter.instruction("bl __rt_diag_warning");
    emitter.instruction(&format!("ldr x0, [sp, #{}]", bread_off));              // the errno
    emitter.instruction("bl __rt_itoa");
    emitter.instruction("bl __rt_diag_warning");
    abi::emit_symbol_address(emitter, "x1", "_diag_space");
    emitter.instruction("mov x2, #1");
    emitter.instruction("bl __rt_diag_warning");
    emitter.instruction(&format!("ldr x0, [sp, #{}]", bread_off));              // the errno again, for its text
    emitter.instruction("bl __rt_socket_strerror");                             // x0 = message pointer, x1 = its length
    emitter.instruction("mov x2, x1");                                          // the diagnostic sink reads x1/x2
    emitter.instruction("mov x1, x0");
    emitter.instruction("bl __rt_diag_warning");
    abi::emit_symbol_address(emitter, "x1", "_diag_newline");
    emitter.instruction("mov x2, #1");
    emitter.instruction("bl __rt_diag_warning");
    emitter.instruction("mov x0, #0");                                          // php's answer is the empty string
    emitter.label("__rt_fgc_read_ok");
    emitter.instruction(&format!("str x0, [sp, #{}]", bread_off));              // save actual bytes read

    // -- close the file --
    emitter.instruction(&format!("ldr x0, [sp, #{}]", fd_off));                 // reload fd
    emitter.syscall(6);

    // -- return buffer pointer and bytes read --
    emitter.instruction(&format!("ldr x1, [sp, #{}]", heap_off));               // return heap buffer pointer
    emitter.instruction(&format!("ldr x2, [sp, #{}]", bread_off));              // return actual bytes read

    // -- restore frame and return --
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_offset));      // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", frame_size));               // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller

    emitter.label("__rt_file_get_contents_fail");
    // Both branches into here still carry the failing syscall result, and the
    // null-terminated path is at [sp, #0]. php-src names both in the message.
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction("neg x3, x0");                                      // Linux answers -errno
    } else {
        emitter.instruction("mov x3, x0");                                      // macOS answers the errno itself
    }
    // php-src warns TWICE when the scheme names no wrapper, the missing-wrapper line first
    // because it is the one that says WHY. The helper is silent for any path a wrapper claims.
    emitter.instruction("str x3, [sp, #-16]!");                                 // the errno survives the extra warning
    emitter.instruction("ldr x2, [sp, #16]");                                   // the null-terminated path
    abi::emit_symbol_address(emitter, "x0", "_uww_name_fgc");
    emitter.instruction(&format!("mov x1, #{}", "file_get_contents".len()));    // bare callee name
    // Prefer a name a delegating builtin published for the duration of its call. The
    // two values are loaded SEPARATELY: materializing a symbol borrows the scratch
    // register, so a length held there would not survive the pointer load.
    abi::emit_load_symbol_to_reg(emitter, "x9", "_rt_open_diag_name_len", 0);
    emitter.instruction("cbz x9, __rt_fgc_uww_named");
    abi::emit_load_symbol_to_reg(emitter, "x0", "_rt_open_diag_name", 0);
    abi::emit_load_symbol_to_reg(emitter, "x1", "_rt_open_diag_name_len", 0);
    emitter.label("__rt_fgc_uww_named");
    emitter.instruction("bl __rt_unknown_wrapper_warning");
    emitter.instruction("ldr x3, [sp], #16");                                   // restore the errno
    emitter.instruction("ldr x2, [sp, #0]");                                    // the null-terminated path
    abi::emit_symbol_address(emitter, "x0", "_diag_open_failed_fgc_prefix");
    emitter.instruction("mov x1, #27");                                         // prefix length
    // Prefer a name a delegating builtin published for the duration of its call. The
    // two values are loaded SEPARATELY: materializing a symbol borrows the scratch
    // register, so a length held there would not survive the pointer load.
    abi::emit_load_symbol_to_reg(emitter, "x9", "_rt_open_diag_prefix_len", 0);
    emitter.instruction("cbz x9, __rt_fgc_open_named");
    abi::emit_load_symbol_to_reg(emitter, "x0", "_rt_open_diag_prefix", 0);
    abi::emit_load_symbol_to_reg(emitter, "x1", "_rt_open_diag_prefix_len", 0);
    emitter.label("__rt_fgc_open_named");
    emitter.instruction("bl __rt_open_failed_warning");
    emitter.instruction("mov x1, #0");                                          // return an empty string pointer on read-path failure
    emitter.instruction("mov x2, #0");                                          // return an empty string length on read-path failure
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_offset));      // restore frame pointer and return address on the failure path
    emitter.instruction(&format!("add sp, sp, #{}", frame_size));               // deallocate stack frame on the failure path
    emitter.instruction("ret");                                                 // return the empty string result for the failed read path
}

/// Emits the x86_64 Linux implementation of `__rt_file_get_contents`.
/// Uses the System V AMD64 ABI: rdi=path, rsi=stat buffer for stat(); rdi=fd, rsi=buf, rdx=count for read(); returns rax=ptr, rdx=length.
/// Calls `__rt_cstr` to null-terminate the filename.
/// Calls `__rt_heap_alloc` to allocate the owned destination buffer.
/// Calls `__rt_diag_warning` on failure before returning (rax=0, rdx=0).
/// Frame: pushes rbp, allocates `frame_size` bytes on rsp, preserves r10 as a temporary.
fn emit_file_get_contents_linux_x86_64(emitter: &mut Emitter) {
    let stat_buf = emitter.platform.stat_buf_size(emitter.target.arch);
    let size_off = emitter.platform.stat_size_offset();
    let frame_size = ((stat_buf + 48) + 15) & !15;
    let path_off = 8usize;
    let size_slot_off = 16usize;
    let fd_off = 24usize;
    let heap_off = 32usize;
    let bread_off = 40usize;

    emitter.blank();
    emitter.comment("--- runtime: file_get_contents ---");
    emitter.label_global("__rt_file_get_contents");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while file_get_contents uses stat and I/O spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the temporary path, size, fd, and heap pointer slots
    emitter.instruction(&format!("sub rsp, {}", frame_size));                   // reserve an aligned Linux stat buffer plus local spill slots for the read-path helper

    emitter.instruction("call __rt_path_cstr");                                 // convert the elephc filename in rax/rdx into a null-terminated C path in rax
    emitter.instruction(&format!("mov QWORD PTR [rbp - {}], rax", path_off));   // preserve the C path pointer across the libc stat(), open(), read(), and close() calls

    emitter.instruction("mov rdi, rax");                                        // pass the C path pointer as the first libc stat() argument
    emitter.instruction("lea rsi, [rsp]");                                      // pass the temporary stack stat buffer as the second libc stat() argument
    emitter.instruction("call stat");                                           // populate the temporary Linux stat buffer so the file size can be read safely
    emitter.instruction("cmp eax, 0");                                          // test whether libc stat() succeeded before reading the file metadata
    emitter.instruction("jne __rt_file_get_contents_fail");                     // return the empty string when the input path cannot be stated

    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", size_off));   // load st_size from the temporary Linux stat buffer after libc stat() succeeds
    emitter.instruction(&format!("mov QWORD PTR [rbp - {}], r10", size_slot_off)); // preserve the file byte size across the later open(), heap_alloc(), and read() calls

    emitter.instruction(&format!("mov rdi, QWORD PTR [rbp - {}]", path_off));   // reload the C path pointer before opening the input file for reading
    emitter.instruction("xor esi, esi");                                        // pass O_RDONLY as the libc open() flags for the file_get_contents() read path
    emitter.instruction("call open");                                           // open the input file for reading through libc open()
    emitter.instruction("cmp eax, 0");                                          // test whether libc open() returned a negative C int descriptor
    emitter.instruction("jl __rt_file_get_contents_fail");                      // return the empty string when the file could not be opened for reading
    emitter.instruction("cdqe");                                                // normalize the successful C int fd into the runtime's 64-bit descriptor value
    emitter.instruction(&format!("mov QWORD PTR [rbp - {}], rax", fd_off));     // preserve the opened file descriptor across the later heap allocation and read() call

    // See the AArch64 arm: php sizes this read `st_size + CHUNK`, and the number is visible in
    // the Notice a failed read prints.
    emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {}]", size_slot_off)); // the size stat reported
    emitter.instruction(&format!("add rax, {}", FGC_READ_CHUNK));               // ask for one chunk more
    emitter.instruction(&format!("mov QWORD PTR [rbp - {}], rax", size_slot_off)); // the read and the message both use it
    emitter.instruction("call __rt_heap_alloc");                                // allocate owned heap storage for the file payload through the shared x86_64 heap wrapper
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(1))); // materialize the owned-string heap kind word with the x86_64 heap marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the allocated buffer as a persisted elephc string in the uniform heap header
    emitter.instruction(&format!("mov QWORD PTR [rbp - {}], rax", heap_off));   // preserve the owned destination buffer pointer across the libc read() and close() calls

    emitter.instruction(&format!("mov rdi, QWORD PTR [rbp - {}]", fd_off));     // pass the opened file descriptor as the first libc read() argument
    emitter.instruction(&format!("mov rsi, QWORD PTR [rbp - {}]", heap_off));   // pass the owned destination buffer as the second libc read() argument
    emitter.instruction(&format!("mov rdx, QWORD PTR [rbp - {}]", size_slot_off)); // the stat-derived size plus one chunk
    emitter.instruction("call read");                                           // read the entire file payload into the owned elephc string buffer through libc read()
    // See the AArch64 arm: a FAILED read is not a byte count, and php answers the empty string.
    emitter.instruction("cmp rax, 0");
    emitter.instruction("jge __rt_fgc_read_ok_x86");
    // See the AArch64 arm: php names the byte count, the errno and the system's text for it.
    emitter.instruction("call __errno_location");                               // libc read() reports through errno
    emitter.instruction("movsxd rax, DWORD PTR [rax]");
    emitter.instruction(&format!("mov QWORD PTR [rbp - {}], rax", bread_off));  // park the errno
    abi::emit_symbol_address(emitter, "rdi", "_fgc_read_failed_head");
    emitter.instruction(&format!("mov esi, {}", FGC_READ_FAILED_HEAD.len()));
    emitter.instruction("call __rt_diag_warning");                              // honours @ and the filter scope
    emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {}]", size_slot_off)); // the size the read asked for
    emitter.instruction("call __rt_itoa");                                      // decimal digits into rax/rdx
    emitter.instruction("mov rdi, rax");
    emitter.instruction("mov rsi, rdx");
    emitter.instruction("call __rt_diag_warning");
    abi::emit_symbol_address(emitter, "rdi", "_fgc_read_failed_mid");
    emitter.instruction(&format!("mov esi, {}", FGC_READ_FAILED_MID.len()));
    emitter.instruction("call __rt_diag_warning");
    emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {}]", bread_off));  // the errno
    emitter.instruction("call __rt_itoa");
    emitter.instruction("mov rdi, rax");
    emitter.instruction("mov rsi, rdx");
    emitter.instruction("call __rt_diag_warning");
    abi::emit_symbol_address(emitter, "rdi", "_diag_space");
    emitter.instruction("mov esi, 1");
    emitter.instruction("call __rt_diag_warning");
    emitter.instruction(&format!("mov rdi, QWORD PTR [rbp - {}]", bread_off));  // the errno again, for its text
    emitter.instruction("call __rt_socket_strerror");                           // rax = message pointer, rdx = its length
    emitter.instruction("mov rdi, rax");
    emitter.instruction("mov rsi, rdx");
    emitter.instruction("call __rt_diag_warning");
    abi::emit_symbol_address(emitter, "rdi", "_diag_newline");
    emitter.instruction("mov esi, 1");
    emitter.instruction("call __rt_diag_warning");
    emitter.instruction("xor eax, eax");                                        // php's answer is the empty string
    emitter.label("__rt_fgc_read_ok_x86");
    emitter.instruction(&format!("mov QWORD PTR [rbp - {}], rax", bread_off));  // preserve the actual read byte count for the final elephc string result pair

    emitter.instruction(&format!("mov rdi, QWORD PTR [rbp - {}]", fd_off));     // reload the file descriptor before closing the successfully opened source file
    emitter.instruction("call close");                                          // close the opened source file after the read() call completes

    emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {}]", heap_off));   // return the owned file payload pointer in the x86_64 string result register
    emitter.instruction(&format!("mov rdx, QWORD PTR [rbp - {}]", bread_off));  // return the actual read byte count in the x86_64 string length register
    emitter.instruction(&format!("add rsp, {}", frame_size));                   // release the temporary Linux stat buffer and local spill slots used by file_get_contents()
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the owned file payload
    emitter.instruction("ret");                                                 // return the owned file contents as an elephc string

    emitter.label("__rt_file_get_contents_fail");
    // The libc wrappers report failure through errno rather than the return value, and the
    // C path is held in the frame rather than a register.
    emitter.instruction("call __errno_location");
    emitter.instruction("movsxd rcx, DWORD PTR [rax]");                         // the errno to describe
    // php-src warns TWICE when the scheme names no wrapper, the missing-wrapper line first
    // because it is the one that says WHY. The helper is silent for any path a wrapper claims.
    emitter.instruction("push rcx");                                            // the errno survives the extra warning
    emitter.instruction("push rcx");                                            // keep rsp 16-byte aligned for the call
    emitter.instruction(&format!("mov rdx, QWORD PTR [rbp - {}]", path_off));   // the null-terminated path
    abi::emit_symbol_address(emitter, "rdi", "_uww_name_fgc");
    emitter.instruction(&format!("mov esi, {}", "file_get_contents".len()));    // bare callee name
    // See the AArch64 arm: load both values from their slots, never park one in scratch.
    abi::emit_load_symbol_to_reg(emitter, "r11", "_rt_open_diag_name_len", 0);
    emitter.instruction("test r11, r11");
    emitter.instruction("jz __rt_fgc_uww_named_x86");
    abi::emit_load_symbol_to_reg(emitter, "rdi", "_rt_open_diag_name", 0);
    abi::emit_load_symbol_to_reg(emitter, "rsi", "_rt_open_diag_name_len", 0);
    emitter.label("__rt_fgc_uww_named_x86");
    emitter.instruction("call __rt_unknown_wrapper_warning");
    emitter.instruction("pop rcx");                                             // discard the alignment copy
    emitter.instruction("pop rcx");                                             // restore the errno
    emitter.instruction(&format!("mov rdx, QWORD PTR [rbp - {}]", path_off));   // the null-terminated path
    abi::emit_symbol_address(emitter, "rdi", "_diag_open_failed_fgc_prefix");
    emitter.instruction("mov esi, 27");                                         // prefix length
    // See the AArch64 arm: load both values from their slots, never park one in scratch.
    abi::emit_load_symbol_to_reg(emitter, "r11", "_rt_open_diag_prefix_len", 0);
    emitter.instruction("test r11, r11");
    emitter.instruction("jz __rt_fgc_open_named_x86");
    abi::emit_load_symbol_to_reg(emitter, "rdi", "_rt_open_diag_prefix", 0);
    abi::emit_load_symbol_to_reg(emitter, "rsi", "_rt_open_diag_prefix_len", 0);
    emitter.label("__rt_fgc_open_named_x86");
    emitter.instruction("call __rt_open_failed_warning");
    emitter.instruction("xor eax, eax");                                        // return an empty string pointer when the file could not be stated or opened
    emitter.instruction("xor edx, edx");                                        // return an empty string length when the file could not be stated or opened
    emitter.instruction(&format!("add rsp, {}", frame_size));                   // release the temporary Linux stat buffer and local spill slots on the failure path
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the empty string
    emitter.instruction("ret");                                                 // return the empty string result for the failed read-path helper
}
