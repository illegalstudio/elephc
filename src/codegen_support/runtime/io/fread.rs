//! Purpose:
//! Emits the `__rt_fread`, `__rt_fread_done` runtime helper assembly for fread.
//! Keeps PHP filesystem/resource behavior, libc calls, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - I/O helpers bridge PHP strings, resources, descriptors, and libc calls while returning runtime arrays or pointer/length strings.
//! - `__rt_fread` returns a *borrowed* pointer/length pair. Reads that fit take a slice
//!   of the 64 KiB concat arena, whose storage is recycled by the caller frame's
//!   save/restore of `_concat_off`; reads that do not fit take an owned heap block
//!   instead, so as not to run off the end of the arena.
//! - That block is reclaimed by the ordinary release path, which needs two things that
//!   were both missing and are easy to mistake for one. The EIR already emits a
//!   `release` for these values; the backend was discarding it, because
//!   `value_is_scratch_string` treats any non-`Fresh` runtime call as arena scratch (see
//!   `codegen/lower_inst/ownership.rs`). And the block carried heap kind 0, so every
//!   `__rt_decref_any` in the runtime's own read paths skipped it as a raw block. Fixing
//!   either alone changes nothing measurable, which is what made this look like a
//!   missing lifetime mechanism rather than two independent omissions.
//! - Arena slices stay safe under that release: `__rt_heap_free_safe` validates against
//!   the managed heap window, and `_concat_buf` is a separate `.comm` object that can
//!   never fall inside it, so the arena case is an exact no-op rather than a heuristic.
//! - REMAINING GAP: a `stream_get_contents()` result that is boxed into a Mixed cell
//!   still strands its grown accumulation buffer. The boxing path copies the bytes out
//!   (`__rt_mixed_from_value`), so the original block is orphaned before any release
//!   refers to it; only the Mixed cell is tracked. Closing that belongs to the boxing
//!   path, not here.

use crate::codegen_support::{emit::Emitter, platform::{Arch, Platform}};
use crate::codegen_support::abi;

/// Emits the `__rt_fread` runtime helper for reading bytes from a file descriptor.
///
/// On ARM64: reads into the concat buffer, updates `_concat_off`, sets `_eof_flags[fd]` on EOF,
/// and returns (pointer, byte_count) in x1:x2.
///
/// On x86_64: same semantics but uses libc `read()` and returns (pointer, byte_count) in rax:rdx.
///
/// # Inputs
/// - x0/rdi: file descriptor
/// - x1/rsi: number of bytes to read
///
/// # Outputs
/// - x1/x86_64 rax: pointer to bytes in concat buffer (borrowed, not owned)
/// - x2/rdx: actual bytes read (0 on EOF/error)
///
/// # Side effects
/// - Advances `_concat_off` by actual bytes read.
/// - Sets `_eof_flags[fd] = 1` when the stream is exhausted.
pub fn emit_fread(emitter: &mut Emitter) {
    emit_fread_readable_bytes(emitter);

    if emitter.target.arch == Arch::X86_64 {
        emit_fread_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: fread ---");
    emitter.label_global("__rt_fread");

    // -- user-wrapper synthetic fd path (Phase 10 step 4) --
    emitter.instruction("mov w9, #0x4000");                                     // load the high half of USER_WRAPPER_FD_BASE = 0x40000000
    emitter.instruction("lsl w9, w9, #16");                                     // shift into bits 30..16 to form 0x40000000
    emitter.instruction("cmp x0, x9");                                          // is this a synthetic user-wrapper fd?
    emitter.instruction("b.lt __rt_fread_real_fd");                             // not a wrapper fd → issue the real read syscall path
    emitter.instruction("b __rt_user_wrapper_fread");                           // wrapper fd: tail-call stream_read (uncond → cross-atom safe)
    emitter.label("__rt_fread_real_fd");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish new frame pointer

    // -- save fd and requested length --
    emitter.instruction("str x0, [sp, #0]");                                    // save file descriptor
    emitter.instruction("str x1, [sp, #8]");                                    // save requested read length

    // -- pick the destination: the concat arena, or the heap when it will not fit --
    // The arena is a flat 64 KiB buffer with an offset and no capacity of its own, so
    // a request larger than what is left of it used to be written straight past the
    // end. Nothing reported that: the returned bytes were correct, because they were
    // read back from the same overflowed region, while whatever followed the buffer
    // was destroyed -- observed as a stream resource that fclose() then rejected as
    // "unknown given", with the victim changing from program to program.
    //
    // Oversized reads go to their own heap block instead. Callers read it exactly as
    // they read an arena slice -- same pointer/length pair -- which is what makes the
    // substitution invisible to them.
    //
    // It is also why the block is never reclaimed: this helper's result is borrowed by
    // contract, so no caller releases it, and the arena's own recycling (the frame
    // save/restore of _concat_off) does not describe heap storage. Every read past the
    // arena therefore strands its block for the life of the process. That is a bounded
    // leak where the unbounded write it replaced was silent corruption, so it stands
    // until the lifetime is modelled properly -- see the note in the module docs.
    //
    // That block is sized from the stream, never from the request: fread() reads *up
    // to* the length it is given, so `fread($f, 100000000)` on a small file is
    // ordinary PHP that php answers in full, and reserving the asked-for size instead
    // emptied the 8 MiB heap in one call. The clamped length is stored back, because
    // both the TLS path and the read syscall below take their count from it.
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current write offset
    emitter.instruction("mov x13, #65536");                                     // the arena's fixed capacity
    emitter.instruction("sub x13, x13, x10");                                   // bytes still free in the arena
    emitter.instruction("ldr x14, [sp, #8]");                                   // the requested read length
    emitter.instruction("cmp x14, x13");                                        // does the whole request still fit?
    emitter.instruction("b.ls __rt_fread_dest_choose");                         // it does: no reason to probe the stream
    emitter.instruction("ldr x0, [sp, #0]");                                    // the descriptor to probe
    emitter.instruction("bl __rt_fread_readable_bytes");                        // x0 = what this stream can still supply
    emitter.instruction("ldr x14, [sp, #8]");                                   // reload the request across the probe
    emitter.instruction("cmp x14, x0");                                         // is the request beyond that supply?
    emitter.instruction("csel x14, x0, x14, hi");                               // read only what the stream actually holds
    emitter.instruction("str x14, [sp, #8]");                                   // the read and the TLS path share this length
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // reload the offset the probe clobbered
    emitter.instruction("mov x13, #65536");                                     // the arena's fixed capacity
    emitter.instruction("sub x13, x13, x10");                                   // bytes still free in the arena
    emitter.label("__rt_fread_dest_choose");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("cmp x14, x13");                                        // does the clamped read still fit?
    emitter.instruction("b.hi __rt_fread_dest_heap");                           // it does not: take an owned block
    emitter.instruction("add x12, x11, x10");                                   // compute write pointer: buf + offset
    emitter.instruction("b __rt_fread_dest_ready");                             // the arena is the destination
    emitter.label("__rt_fread_dest_heap");
    emitter.instruction("mov x0, x14");                                         // allocate what the stream can supply
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = owned payload pointer
    emitter.instruction("mov x13, #1");                                         // heap kind 1 = owned elephc string
    emitter.instruction("str x13, [x0, #-8]");                                  // stamp it: raw blocks are skipped by every decref
    emitter.instruction("mov x12, x0");                                         // the heap block is the destination
    emitter.label("__rt_fread_dest_ready");
    emitter.instruction("str x12, [sp, #16]");                                  // save start pointer for return value

    // -- TLS dispatch: route through elephc_tls_read when fd has an
    //    attached session (Phase 11 B3). --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload fd for the TLS check
    emitter.instruction("bl __rt_tls_session_get");                             // resolve the full-width descriptor through the bounded TLS map
    emitter.instruction("cbz x0, __rt_fread_do_syscall");                       // no TLS attached → fall through to read syscall
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload the concat-buffer destination pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // len
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_elephc_tls_read_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load elephc_tls_read entry pointer
    emitter.emit_published_bridge_call("x9");                                   // x0 = bytes, EOF, or one documented TLS v2 sentinel
    emitter.instruction("cmp x0, #-1");                                         // terminal TLS failures alone retain the existing EOF behavior
    emitter.instruction("b.eq __rt_fread_mark_eof");                            // terminal TLS failure: report an empty exhausted stream
    emitter.instruction("cmp x0, #0");                                          // distinguish EOF/bytes from retryable TLS sentinels
    emitter.instruction("b.ge __rt_fread_read_ok");                             // bytes and TLS EOF use the normal read-result path
    emitter.instruction("b __rt_fread_would_block");                            // WouldBlock/TimedOut return empty without marking EOF

    emitter.label("__rt_fread_do_syscall");
    // -- perform read syscall --
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd for read syscall
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload the buffer pointer after the TLS lookup helper
    emitter.instruction("ldr x2, [sp, #8]");                                    // number of bytes to read
    emitter.syscall(3);
    if emitter.platform.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: negative read result means failure
    }
    emitter.instruction(&emitter.platform.branch_on_syscall_success("__rt_fread_read_ok")); // continue only when the read syscall succeeded
    if emitter.platform.needs_cmp_before_error_branch() {
        emitter.instruction(&format!("cmn x0, #{}", emitter.platform.would_block_errno())); // Linux: is this -EAGAIN/-EWOULDBLOCK from a nonblocking fd?
    } else {
        emitter.instruction(&format!("cmp x0, #{}", emitter.platform.would_block_errno())); // macOS: is this EAGAIN/EWOULDBLOCK from a nonblocking fd?
    }
    emitter.instruction("b.eq __rt_fread_would_block");                         // a transient nonblocking miss is not EOF
    emitter.instruction("str xzr, [sp, #24]");                                  // failed reads return an empty result
    emitter.instruction("b __rt_fread_mark_eof");                               // mark the stream as exhausted after a read failure
    emitter.label("__rt_fread_read_ok");

    // -- update concat_off by actual bytes read, but only for an arena destination --
    // An oversized read landed in its own heap block, which the arena offset does not
    // describe; advancing it there would hand the next writer a cursor into bytes
    // nobody wrote. The destination pointer says which case this is: an unsigned
    // distance below the arena's capacity means it points inside it.
    emitter.instruction("str x0, [sp, #24]");                                   // save actual bytes read
    emitter.instruction("ldr x12, [sp, #16]");                                  // reload the destination pointer
    crate::codegen_support::abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("sub x13, x12, x11");                                   // distance from the arena base
    emitter.instruction("mov x14, #65536");                                     // the arena's fixed capacity
    emitter.instruction("cmp x13, x14");                                        // did the bytes land inside the arena?
    emitter.instruction("b.hs __rt_fread_off_kept");                            // heap destination: the arena cursor is unrelated
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    emitter.instruction("add x10, x10, x0");                                    // advance offset by bytes read
    emitter.instruction("str x10, [x9]");                                       // store updated offset
    emitter.label("__rt_fread_off_kept");

    // -- set eof flag if read returned 0 --
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload bytes read
    emitter.instruction("cbnz x0, __rt_fread_done");                            // if bytes > 0, skip eof flag
    emitter.label("__rt_fread_mark_eof");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload fd
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_eof_flags");
    emitter.instruction("mov w10, #1");                                         // eof marker value
    emitter.instruction("strb w10, [x9, x0]");                                  // set _eof_flags[fd] = 1

    emitter.label("__rt_fread_would_block");
    emitter.instruction("str xzr, [sp, #24]");                                  // return an empty read without setting EOF for EAGAIN/EWOULDBLOCK

    // -- return pointer and length --
    emitter.label("__rt_fread_done");
    emitter.instruction("ldr x1, [sp, #16]");                                   // return string start pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // return actual bytes read as length

    // -- apply an attached read filter to the bytes just read --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the file descriptor
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_stream_read_filters");
    emitter.instruction("ldrb w3, [x9, x0]");                                   // read filter id for this descriptor
    emitter.instruction("cbz w3, __rt_fread_ret");                              // skip when no read filter is attached
    emitter.instruction("cmp w3, #128");                                        // user-filter id range (>= USER_FILTER_ID_BASE)?
    emitter.instruction("b.lt __rt_fread_builtin_filter");                      // built-in filter: in-place transform
    emitter.instruction("mov x3, #0");                                          // direction = 0 (read) for the user-filter dispatch
    emitter.instruction("bl __rt_apply_user_stream_filter");                    // x1/x2 ← user filter's transformed string
    emitter.instruction("b __rt_fread_ret");                                    // common epilogue
    emitter.label("__rt_fread_builtin_filter");
    emitter.instruction("bl __rt_apply_stream_filter");                         // transform the read bytes in place; x2 = (possibly compacted) length on return

    // -- restore frame and return --
    emitter.label("__rt_fread_ret");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the x86_64 Linux variant of `__rt_fread` using libc `read()`.
fn emit_fread_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fread ---");
    emitter.label_global("__rt_fread");

    // -- user-wrapper synthetic fd path (Phase 10 step 4) --
    emitter.instruction("mov r9d, 0x40000000");                                 // USER_WRAPPER_FD_BASE
    emitter.instruction("cmp rdi, r9");                                         // is this a synthetic user-wrapper fd?
    emitter.instruction("jge __rt_user_wrapper_fread");                         // dispatch into the wrapper's stream_read instead of issuing a read syscall

    emitter.instruction("cmp rdi, 0");                                          // does fread() have a valid non-negative file descriptor to read from?
    emitter.instruction("jge __rt_fread_fd_ok_x86");                            // continue to the normal read path when the file descriptor is valid
    emitter.instruction("xor eax, eax");                                        // return an empty string pointer immediately when fopen() failed
    emitter.instruction("xor edx, edx");                                        // return an empty string length immediately when fopen() failed
    emitter.instruction("ret");                                                 // skip the stream read path entirely for invalid file descriptors

    emitter.label("__rt_fread_fd_ok_x86");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while fread() uses local spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved file descriptor, length, and concat-buffer start pointer
    emitter.instruction("sub rsp, 32");                                         // reserve aligned stack space for the fread() read-path temporaries

    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the file descriptor across the concat-buffer address computation and libc read() call
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the requested byte count across the concat-buffer address computation and libc read() call
    // Same arena-overflow guard as the AArch64 arm above: a request larger than what
    // is left of the flat 64 KiB arena gets its own heap block instead of running off
    // the end of it. Callers cannot tell the difference -- a heap-backed string is the
    // same pointer/length pair, and __rt_decref_any validates against the managed heap
    // window, so it releases this block and keeps ignoring arena slices.
    //
    // The block is sized from the stream and not from the request, for the reason the
    // AArch64 arm spells out: fread() reads *up to* its length, so an oversized
    // request is ordinary PHP and must not reserve what was merely asked for.
    abi::emit_load_symbol_to_reg(emitter, "r10", "_concat_off", 0);             // load the current concat-buffer absolute offset before appending the fread() result
    emitter.instruction("mov rax, 65536");                                      // the arena's fixed capacity
    emitter.instruction("sub rax, r10");                                        // bytes still free in the arena
    emitter.instruction("cmp QWORD PTR [rbp - 16], rax");                       // does the whole request still fit?
    emitter.instruction("jbe __rt_fread_dest_choose_x86");                      // it does: no reason to probe the stream
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the descriptor to probe
    emitter.instruction("call __rt_fread_readable_bytes");                      // rax = what this stream can still supply
    emitter.instruction("cmp QWORD PTR [rbp - 16], rax");                       // is the request beyond that supply?
    emitter.instruction("jbe __rt_fread_len_kept_x86");                         // the request already fits inside it
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // read only what the stream actually holds
    emitter.label("__rt_fread_len_kept_x86");
    abi::emit_load_symbol_to_reg(emitter, "r10", "_concat_off", 0);             // reload the offset the probe clobbered
    emitter.instruction("mov rax, 65536");                                      // the arena's fixed capacity
    emitter.instruction("sub rax, r10");                                        // bytes still free in the arena
    emitter.label("__rt_fread_dest_choose_x86");
    abi::emit_symbol_address(emitter, "r11", "_concat_buf");                    // materialize the concat-buffer base address once for the x86_64 fread() helper
    emitter.instruction("cmp QWORD PTR [rbp - 16], rax");                       // does the clamped read still fit?
    emitter.instruction("ja __rt_fread_dest_heap_x86");                         // it does not: take an owned block
    emitter.instruction("lea rax, [r11 + r10]");                                // compute the start pointer for the bytes that libc read() will append
    emitter.instruction("jmp __rt_fread_dest_ready_x86");                       // the arena is the destination
    emitter.label("__rt_fread_dest_heap_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // allocate what the stream can supply
    emitter.instruction("call __rt_heap_alloc");                                // rax = owned payload pointer
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(1))); // owned-string kind word carrying the x86_64 heap magic
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp it: raw blocks are skipped by every decref
    emitter.label("__rt_fread_dest_ready_x86");
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the concat-buffer start pointer for the final elephc string result

    if emitter.target.platform == Platform::Windows {
        emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                    // restore the opaque descriptor before starting a new read operation
        emitter.instruction("call __rt_win_stream_clear_timed_out");            // every read begins with timed_out=false, including TLS-dispatched reads
    }

    // -- TLS dispatch: route through elephc_tls_read when fd has an
    //    attached session (Phase 11 B3). --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the full-width descriptor for the TLS map lookup
    emitter.instruction("call __rt_tls_session_get");                           // resolve the descriptor without using it as a raw array index
    emitter.instruction("test rax, rax");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_fread_do_syscall_x86");                        // no TLS attached → use libc read
    emitter.instruction("mov rdi, rax");                                        // handle as first arg
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // buf ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // len
    abi::emit_load_symbol_to_reg(emitter, "r9", "_elephc_tls_read_fn", 0);      // prepare SysV call argument
    emitter.emit_published_bridge_call("r9");                                   // rax = bytes, EOF, or one documented TLS v2 sentinel
    emitter.instruction("cmp rax, -1");                                         // terminal TLS failure alone retains the existing EOF behavior
    emitter.instruction("je __rt_fread_eof_x86");                               // terminal TLS failure: report an empty exhausted stream
    emitter.instruction("cmp rax, 0");                                          // distinguish EOF/bytes from retryable TLS sentinels
    emitter.instruction("jg __rt_fread_read_ok_x86");                           // publish a positive TLS read
    emitter.instruction("je __rt_fread_eof_x86");                               // a zero-byte TLS read is genuine EOF
    if emitter.target.platform == Platform::Windows {
        emitter.instruction("cmp rax, -3");                                     // TLS v2 TimedOut needs PHP-visible Windows stream metadata
        emitter.instruction("jne __rt_fread_would_block_x86");                  // TLS WouldBlock remains an empty non-EOF read
        emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                    // restore the opaque stream descriptor for timeout metadata
        emitter.instruction("call __rt_win_stream_mark_timed_out");             // record TimedOut without mutating EOF state
    }
    emitter.instruction("jmp __rt_fread_would_block_x86");                      // retryable TLS sentinels return an empty non-EOF read
    emitter.label("__rt_fread_do_syscall_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the file descriptor as the first libc read() argument
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // pass the concat-buffer write pointer as the second libc read() argument
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // pass the requested byte count as the third libc read() argument
    emitter.instruction("call read");                                           // read the requested bytes into the concat-buffer append window through libc read()
    emitter.instruction("cmp rax, 0");                                          // classify libc read() as bytes, EOF, or failure
    emitter.instruction("jg __rt_fread_read_ok_x86");                           // positive byte count: publish the successful read
    emitter.instruction("jl __rt_fread_read_failed_x86");                       // negative result: inspect errno before treating it as EOF
    emitter.instruction("jmp __rt_fread_eof_x86");                              // zero-byte read means real EOF

    emitter.label("__rt_fread_read_ok_x86");
    // Only an arena destination moves the arena cursor: an oversized read landed in
    // its own heap block, which that cursor does not describe, and advancing it there
    // would hand the next writer an offset into bytes nobody wrote. The destination
    // pointer says which case this is -- an unsigned distance below the capacity means
    // it points inside the arena.
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the destination pointer
    abi::emit_symbol_address(emitter, "r10", "_concat_buf");                    // the arena base
    emitter.instruction("sub r11, r10");                                        // distance from the arena base
    emitter.instruction("cmp r11, 65536");                                      // did the bytes land inside the arena?
    emitter.instruction("jae __rt_fread_off_kept_x86");                         // heap destination: the arena cursor is unrelated
    abi::emit_load_symbol_to_reg(emitter, "r10", "_concat_off", 0);             // reload the previous concat-buffer absolute offset before publishing the fread() append
    emitter.instruction("add r10, rax");                                        // advance the concat-buffer offset by the number of bytes libc read() returned
    abi::emit_store_reg_to_symbol(emitter, "r10", "_concat_off", 0);            // publish the updated concat-buffer offset for later string appenders
    emitter.label("__rt_fread_off_kept_x86");
    emitter.instruction("mov rdx, rax");                                        // return the successful byte count in the x86_64 elephc string-length result register
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the concat-buffer start pointer in the x86_64 elephc string-pointer result register
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the file descriptor for the read-filter lookup
    if emitter.target.platform == Platform::Windows {
        emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                   // preserve the successful byte count across the slot-registry call
        emitter.instruction("mov rdi, r10");                                    // pass the opaque Windows descriptor to the slot registry
        emitter.instruction("call __rt_win_stream_slot");                       // obtain a bounded filter-table slot
        emitter.instruction("mov r10, rax");                                    // table indexing uses the compact slot, never a raw SOCKET
        emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                   // restore the fread result pointer clobbered by the slot lookup
        emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                   // restore the fread result length clobbered by the slot lookup
    }
    abi::emit_symbol_address(emitter, "r11", "_stream_read_filters");           // materialize the read-filter table base
    emitter.instruction("movzx ecx, BYTE PTR [r11 + r10]");                     // read filter id for this bounded stream slot
    emitter.instruction("test rcx, rcx");                                       // is a read filter attached to this stream?
    emitter.instruction("jz __rt_fread_ret_x86");                               // skip when no read filter is attached
    emitter.instruction("cmp rcx, 128");                                        // user-filter id range (>= USER_FILTER_ID_BASE)?
    emitter.instruction("jl __rt_fread_builtin_filter_x86");                    // built-in filter: in-place transform
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // retain the real descriptor for user-filter dispatch
    emitter.instruction("mov rsi, rax");                                        // buf ptr into the dispatcher's second arg
    // rdx already holds the byte count
    emitter.instruction("xor ecx, ecx");                                        // direction = 0 (read) for the user-filter dispatch
    emitter.instruction("call __rt_apply_user_stream_filter");                  // rax/rdx ← user filter's transformed string
    emitter.instruction("jmp __rt_fread_ret_x86");                              // common epilogue
    emitter.label("__rt_fread_builtin_filter_x86");
    emitter.instruction("call __rt_apply_stream_filter");                       // transform the read bytes in place; rdx = (possibly compacted) length on return
    emitter.label("__rt_fread_ret_x86");
    emitter.instruction("add rsp, 32");                                         // release the fread() spill slots before returning the successful string slice
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the successful fread() path
    emitter.instruction("ret");                                                 // return the borrowed concat-buffer string slice to the caller

    emitter.label("__rt_fread_read_failed_x86");
    emitter.instruction("call __errno_location");                               // fetch errno after libc read() failed
    emitter.instruction("mov r10d, DWORD PTR [rax]");                           // load the thread-local errno value
    emitter.instruction("cmp r10d, 11");                                        // is this EAGAIN/EWOULDBLOCK from a nonblocking fd?
    emitter.instruction("je __rt_fread_would_block_x86");                       // transient nonblocking miss returns empty without EOF
    if emitter.target.platform == Platform::Windows {
        emitter.instruction("cmp r10d, 110");                                   // ETIMEDOUT is retryable stream timeout, not EOF
        emitter.instruction("jne __rt_fread_eof_x86");                          // only genuine non-timeout failures preserve the old EOF behavior
        emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                    // restore the opaque stream descriptor for timeout metadata
        emitter.instruction("call __rt_win_stream_mark_timed_out");             // record timed_out without touching EOF state
        emitter.instruction("jmp __rt_fread_would_block_x86");                  // return an empty non-EOF read after timeout
    }
    emitter.instruction("jmp __rt_fread_eof_x86");                              // other read failures behave like an exhausted stream

    emitter.label("__rt_fread_would_block_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the concat-buffer start pointer for an empty transient read
    emitter.instruction("xor edx, edx");                                        // return a zero-length read result without setting EOF
    emitter.instruction("add rsp, 32");                                         // release the fread() spill slots before returning the empty string
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the would-block fread() path
    emitter.instruction("ret");                                                 // return the empty non-EOF read result

    emitter.label("__rt_fread_eof_x86");
    // The destination is chosen before the read, so an oversized read that turns out
    // to be at end of stream has already taken its block -- and this arm returns a
    // null pointer, so nothing downstream can release it. A drain loop reading large
    // chunks ends on exactly this path, once per stream. __rt_heap_free ignores
    // pointers without the heap marker, so an arena slice passes through untouched
    // and needs no test here. The AArch64 arm returns the pointer with length 0
    // instead of dropping it, so its caller still owns the block and it must not be
    // released there.
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // the destination chosen before the read
    emitter.instruction("call __rt_heap_free");                                 // release it when the read never filled it
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the file descriptor so the eof-flag table can mark this stream as exhausted
    if emitter.target.platform == Platform::Windows {
        emitter.instruction("mov rdi, r10");                                    // pass the opaque Windows descriptor to the slot registry
        emitter.instruction("call __rt_win_stream_slot");                       // obtain a bounded EOF-table slot
        emitter.instruction("mov r10, rax");                                    // table indexing uses the compact slot, never a raw SOCKET
    }
    abi::emit_symbol_address(emitter, "r11", "_eof_flags");                     // materialize the eof-flag table base address for the current stream descriptor
    emitter.instruction("mov BYTE PTR [r11 + r10], 1");                         // mark the compact stream slot as EOF-reached after the zero-byte or failed read
    emitter.instruction("xor eax, eax");                                        // return an empty string pointer when libc read() reports EOF or failure
    emitter.instruction("xor edx, edx");                                        // return an empty string length when libc read() reports EOF or failure
    emitter.instruction("add rsp, 32");                                         // release the fread() spill slots before returning the empty-string result
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the EOF/error fread() path
    emitter.instruction("ret");                                                 // return the empty string result for the exhausted or failed stream read
}

/// Emits `__rt_fread_readable_bytes`, the bound used to size an oversized read.
///
/// `fread($f, $n)` reads *up to* `$n` bytes, so a request may legitimately dwarf the
/// stream behind it: `fread($f, 100000000)` on a 13-byte file is 13 bytes in PHP, not
/// an error. Sizing an owned block from the request alone therefore emptied elephc's
/// 8 MiB default heap on a read php answers in full, which is what this bound exists
/// to prevent.
///
/// The probe is `lseek` rather than `fstat` because it has to tell a file from a
/// socket, and only `lseek` *fails* on one (ESPIPE). `fstat` succeeds on a socket and
/// reports `st_size` 0, which would clamp every socket read to nothing. A stream that
/// cannot be asked falls back to one arena's worth: php stops such a read at the first
/// packet regardless, so a larger bound would buy nothing.
///
/// # Inputs
/// - x0/rdi: file descriptor
///
/// # Outputs
/// - x0/rax: bytes the stream can still supply, or 65536 when it cannot be asked
///
/// # Side effects
/// - None. The seek position is restored before returning.
fn emit_fread_readable_bytes(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_fread_readable_bytes_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: fread readable bytes ---");
    emitter.label_global("__rt_fread_readable_bytes");
    emitter.instruction("sub sp, sp, #48");                                     // allocate the probe frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the probe frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // keep the descriptor across the seek probes

    // -- where the stream is positioned: lseek(fd, 0, SEEK_CUR) --
    emitter.instruction("mov x1, #0");                                          // offset 0
    emitter.instruction("mov x2, #1");                                          // SEEK_CUR
    emitter.syscall(199);
    if emitter.platform.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative result means lseek failed
    }
    emitter.instruction(&emitter.platform.branch_on_syscall_success("__rt_frb_seekable")); // the size is knowable
    emitter.instruction("mov x0, #65536");                                      // a socket or pipe cannot be asked: one arena's worth
    emitter.instruction("b __rt_frb_ret");                                      // nothing moved, so nothing to restore

    emitter.label("__rt_frb_seekable");
    emitter.instruction("str x0, [sp, #8]");                                    // remember where the read starts from

    // -- where the stream ends: lseek(fd, 0, SEEK_END) --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the descriptor
    emitter.instruction("mov x1, #0");                                          // offset 0
    emitter.instruction("mov x2, #2");                                          // SEEK_END
    emitter.syscall(199);
    if emitter.platform.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative result means lseek failed
    }
    emitter.instruction(&emitter.platform.branch_on_syscall_success("__rt_frb_have_end")); // the end offset is usable
    emitter.instruction("mov x0, #65536");                                      // the end is unknown: fall back to one arena's worth
    emitter.instruction("b __rt_frb_ret");                                      // the position never moved, so nothing to restore

    emitter.label("__rt_frb_have_end");
    emitter.instruction("str x0, [sp, #16]");                                   // hold the end across the restoring seek
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the descriptor
    emitter.instruction("ldr x1, [sp, #8]");                                    // seek back to where the stream was
    emitter.instruction("mov x2, #0");                                          // SEEK_SET
    emitter.syscall(199);
    emitter.instruction("ldr x9, [sp, #16]");                                   // the end offset
    emitter.instruction("ldr x10, [sp, #8]");                                   // the offset the read starts from
    emitter.instruction("subs x0, x9, x10");                                    // bytes between the position and the end
    emitter.instruction("b.gt __rt_frb_ret");                                   // a positive remainder is the bound
    emitter.instruction("mov x0, #0");                                          // at or past the end: nothing left to read

    emitter.label("__rt_frb_ret");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate the probe frame
    emitter.instruction("ret");                                                 // return the supply bound
}

/// Emits the x86_64 variant of `__rt_fread_readable_bytes`.
///
/// Same three-step seek probe as the AArch64 arm, issued as raw `lseek` syscalls so
/// `windows_transform` rewrites them into `__rt_sys_lseek`, whose `SetFilePointerEx`
/// fails on a socket handle exactly as ESPIPE does on Linux. The descriptor is
/// reloaded before every probe because that rewrite turns each one into a call, which
/// keeps none of the argument registers.
fn emit_fread_readable_bytes_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fread readable bytes ---");
    emitter.label_global("__rt_fread_readable_bytes");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the probe
    emitter.instruction("sub rsp, 32");                                         // reserve the descriptor, position and end slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // keep the descriptor across the seek probes

    // -- where the stream is positioned: lseek(fd, 0, SEEK_CUR) --
    emitter.instruction("xor esi, esi");                                        // offset 0
    emitter.instruction("mov edx, 1");                                          // SEEK_CUR
    emitter.instruction("mov eax, 8");                                          // Linux x86_64 syscall 8 = lseek
    emitter.instruction("syscall");                                             // probe whether the descriptor is seekable
    emitter.instruction("test rax, rax");                                       // did lseek fail with a negative result?
    emitter.instruction("jns __rt_frb_seekable_x86");                           // seekable: the size is knowable
    emitter.instruction("mov rax, 65536");                                      // a socket or pipe cannot be asked: one arena's worth
    emitter.instruction("jmp __rt_frb_ret_x86");                                // nothing moved, so nothing to restore

    emitter.label("__rt_frb_seekable_x86");
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // remember where the read starts from

    // -- where the stream ends: lseek(fd, 0, SEEK_END) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the descriptor
    emitter.instruction("xor esi, esi");                                        // offset 0
    emitter.instruction("mov edx, 2");                                          // SEEK_END
    emitter.instruction("mov eax, 8");                                          // Linux x86_64 syscall 8 = lseek
    emitter.instruction("syscall");                                             // query the end of the stream
    emitter.instruction("test rax, rax");                                       // did lseek fail with a negative result?
    emitter.instruction("jns __rt_frb_have_end_x86");                           // the end offset is usable
    emitter.instruction("mov rax, 65536");                                      // the end is unknown: fall back to one arena's worth
    emitter.instruction("jmp __rt_frb_ret_x86");                                // the position never moved, so nothing to restore

    emitter.label("__rt_frb_have_end_x86");
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // hold the end across the restoring seek
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the descriptor
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // seek back to where the stream was
    emitter.instruction("xor edx, edx");                                        // SEEK_SET
    emitter.instruction("mov eax, 8");                                          // Linux x86_64 syscall 8 = lseek
    emitter.instruction("syscall");                                             // restore the position the probe consumed
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // the end offset
    emitter.instruction("sub rax, QWORD PTR [rbp - 16]");                       // bytes between the position and the end
    emitter.instruction("jg __rt_frb_ret_x86");                                 // a positive remainder is the bound
    emitter.instruction("xor eax, eax");                                        // at or past the end: nothing left to read

    emitter.label("__rt_frb_ret_x86");
    emitter.instruction("add rsp, 32");                                         // release the probe slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the supply bound
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::Target;

    /// Verifies both arms send an oversized read to the heap, not past the arena.
    ///
    /// The concat arena is a flat 64 KiB buffer with an offset and no capacity of its
    /// own, so a larger request used to be written straight off the end of it. The
    /// fixture that proves the behaviour needs a real filesystem and a linker, and the
    /// windows link set is not reproducible on every host, so the guard that both arms
    /// carry the check lives here.
    #[test]
    fn oversized_reads_take_an_owned_block_instead_of_overrunning_the_arena() {
        for (platform, arch, alloc, guard) in [
            (Platform::MacOS, Arch::AArch64, "bl __rt_heap_alloc\n", "__rt_fread_dest_heap:\n"),
            (Platform::Linux, Arch::X86_64, "call __rt_heap_alloc\n", "__rt_fread_dest_heap_x86:\n"),
        ] {
            let mut emitter = Emitter::new(Target::new(platform, arch));
            emit_fread(&mut emitter);
            let asm = emitter.output();

            assert!(asm.contains(guard), "{:?}/{:?}: missing the capacity guard", platform, arch);
            assert!(asm.contains(alloc), "{:?}/{:?}: the guard must allocate", platform, arch);
            assert!(
                asm.matches("65536").count() >= 2,
                "{:?}/{:?}: the capacity is needed both to choose the destination and to \
                 decide whether the arena cursor moves",
                platform,
                arch
            );
        }
    }

    /// Verifies an oversized read is sized from the stream, never from the request.
    ///
    /// `fread($f, $n)` reads up to `$n` bytes, so the request is an upper bound and
    /// not an amount to reserve: sizing the block from it turned `fread($f, 1e8)` on
    /// a small file -- which php answers in full -- into an exhausted 8 MiB heap. The
    /// probe has to run *before* the allocation for the clamp to mean anything, which
    /// is the ordering this pins.
    #[test]
    fn oversized_reads_are_sized_from_the_stream_and_not_from_the_request() {
        for (platform, arch, alloc) in [
            (Platform::MacOS, Arch::AArch64, "bl __rt_heap_alloc"),
            (Platform::Linux, Arch::X86_64, "call __rt_heap_alloc"),
            (Platform::Windows, Arch::X86_64, "call __rt_heap_alloc"),
        ] {
            let mut emitter = Emitter::new(Target::new(platform, arch));
            emit_fread(&mut emitter);
            let asm = emitter.output();

            let fread_body = asm
                .find("__rt_fread:")
                .expect("missing the fread entry point");
            let probe = asm[fread_body..]
                .find("__rt_fread_readable_bytes")
                .unwrap_or_else(|| panic!("{:?}/{:?}: the read is never bounded by the stream", platform, arch));
            let allocation = asm[fread_body..]
                .find(alloc)
                .unwrap_or_else(|| panic!("{:?}/{:?}: the oversized path must still take a block", platform, arch));
            assert!(
                probe < allocation,
                "{:?}/{:?}: the block is sized before the stream is asked what it holds",
                platform,
                arch
            );
        }
    }

    /// Verifies the probe leaves the stream position exactly where it found it.
    ///
    /// It reads the position, seeks to the end to measure, and must seek back: a
    /// probe that skipped the third seek would silently consume the whole stream and
    /// hand back nothing, which no fixture asserting only on lengths would catch.
    #[test]
    fn the_readable_bytes_probe_restores_the_stream_position() {
        for (platform, arch, seek, expected) in [
            (Platform::MacOS, Arch::AArch64, "mov x16, #199", 3),
            (Platform::Linux, Arch::X86_64, "mov eax, 8", 3),
        ] {
            let mut emitter = Emitter::new(Target::new(platform, arch));
            emit_fread_readable_bytes(&mut emitter);
            let asm = emitter.output();

            assert_eq!(
                asm.matches(seek).count(),
                expected,
                "{:?}/{:?}: the probe must read the position, measure the end, and seek back",
                platform,
                arch
            );
        }
    }

    /// Verifies the x86_64 EOF arm releases a block its own null return would strand.
    ///
    /// The destination is chosen before the read, so an oversized read that turns out
    /// to be at end of stream has already taken a block -- and this arm returns a null
    /// pointer, leaving nothing downstream able to release it. A drain loop reading
    /// large chunks ends on exactly this path. The AArch64 arm hands the pointer back
    /// with length 0 instead, so its caller still owns the block and freeing it there
    /// would be a use-after-free.
    #[test]
    fn an_oversized_read_at_end_of_stream_releases_its_block_on_x86_64() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_fread(&mut emitter);
        let asm = emitter.output();
        let eof = asm
            .find("__rt_fread_eof_x86:")
            .expect("missing the x86_64 EOF arm");
        let release = asm[eof..]
            .find("call __rt_heap_free")
            .expect("the EOF arm strands the block it allocated before the read");
        let returns = asm[eof..].find("ret").expect("missing the EOF return");
        assert!(release < returns, "the block must be released before returning");

        let mut arm = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_fread(&mut arm);
        let arm_asm = arm.output();
        let mark_eof = arm_asm
            .find("__rt_fread_mark_eof:")
            .expect("missing the AArch64 EOF arm");
        assert!(
            !arm_asm[mark_eof..].contains("bl __rt_heap_free"),
            "AArch64 returns the pointer to its caller, so releasing it here frees a live block"
        );
    }

    /// Verifies TLS v2 retry sentinels reach the non-EOF path on both active
    /// runtime architectures, while only the terminal sentinel reaches EOF.
    #[test]
    fn tls_retry_sentinels_do_not_emit_eof_branches() {
        let mut arm = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_fread(&mut arm);
        let arm_asm = arm.output();
        assert!(arm_asm.contains("cmp x0, #-1"));
        assert!(arm_asm.contains("b.eq __rt_fread_mark_eof"));
        assert!(arm_asm.contains("b __rt_fread_would_block"));

        let mut x86 = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_fread(&mut x86);
        let x86_asm = x86.output();
        assert!(x86_asm.contains("cmp rax, -1"));
        assert!(x86_asm.contains("je __rt_fread_eof_x86"));
        assert!(x86_asm.contains("jmp __rt_fread_would_block_x86"));
    }

    /// Verifies Windows keeps the successful fread string result intact while
    /// mapping an opaque descriptor to its bounded filter-table slot.
    #[test]
    fn windows_filter_slot_lookup_preserves_fread_result() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_fread(&mut emitter);
        let asm = emitter.output();
        let slot_call = asm.find("call __rt_win_stream_slot").expect("missing slot lookup");
        let restore_ptr = asm[slot_call..]
            .find("mov rax, QWORD PTR [rbp - 24]")
            .expect("missing fread pointer restore");
        let restore_len = asm[slot_call..]
            .find("mov rdx, QWORD PTR [rbp - 32]")
            .expect("missing fread length restore");
        assert!(restore_ptr < restore_len);
    }
}
