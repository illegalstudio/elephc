//! Purpose:
//! Emits the `__rt_stream_select` runtime helper, which waits for readability,
//! writability, or exceptional conditions across descriptor sets using
//! `poll(2)` instead of `select(2)` to remove the 64-fd ceiling.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - Descriptors are collected into a stack-allocated `struct pollfd` array
//!   (capacity 256, 2048 bytes). Read entries use `POLLIN` (0x001), write
//!   entries `POLLOUT` (0x004), except entries `POLLPRI` (0x002).
//! - `poll(2)` is reached through libc on ARM64 (both platforms) because ARM64 Linux
//!   has no `poll` syscall — 73 there is `ppoll`, which takes a `struct timespec *`
//!   instead of a millisecond count. x86_64 Linux keeps the native syscall (7), where
//!   `poll` does exist and does take milliseconds.
//! - After `poll` returns, each resource array is compacted in place to the
//!   ready subset by checking the corresponding `revents` field.
//! - Synthetic user-wrapper descriptors (`fd & 0x40000000`) are resolved to a
//!   real selectable fd via `__rt_user_wrapper_stream_cast(fd, 3)` before
//!   being added to the pollfd array.
//! - Timeout: `seconds` and `microseconds` are combined into a millisecond
//!   count. A non-positive sentinel (null/infinite) maps to `-1` (block
//!   forever); `seconds == 0 && microseconds == 0` maps to `0` (non-blocking).

use crate::codegen_support::{emit::Emitter, platform::{Arch, Platform}};

const POLLIN: i64 = 0x001;
const POLLOUT: i64 = 0x004;
const POLLPRI: i64 = 0x002;

/// Emits the `__rt_stream_select` runtime helper, dispatching to the target variant.
pub fn emit_stream_select(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_select_linux_x86_64(emitter);
        return;
    }
    emit_stream_select_aarch64(emitter);
}

/// ARM64 variant of `__rt_stream_select` (macOS via `_poll`, Linux via syscall 73).
///
/// Frame layout (2304 bytes): `[0..2048]` pollfd array, `[2048]` read_arr,
/// `[2056]` write_arr, `[2064]` except_arr, `[2072]` read_len, `[2080]` write_len,
/// `[2088]` except_len, `[2096]` nfds, `[2104]` ready_count, `[2112]` timeout_ms,
/// `[2120..2168]` spill slots for stream_cast, `[2168]` timeout seconds,
/// `[2176]` timeout microseconds, `[2184]` castable tally, `[2192]` the synthetic fd a refused
/// cast has to name in its diagnostics, `[2256]` x29, `[2264]` x30.
fn emit_stream_select_aarch64(emitter: &mut Emitter) {
    let linux = emitter.target.platform == Platform::Linux;
    emitter.blank();
    emitter.comment("--- runtime: stream_select ---");
    emitter.label_global("__rt_stream_select");

    emitter.instruction("sub sp, sp, #2304");                                    // allocate the poll-based select frame
    emitter.instruction("add x9, sp, #2256");                                     // compute the fp/lr save address (offset > 504 needs add)
    emitter.instruction("stp x29, x30, [x9]");                                  // save frame pointer and return address
    emitter.instruction("add x29, sp, #2256");                                   // establish the helper frame pointer

    // -- save the three resource arrays --
    emitter.instruction("str x0, [sp, #2048]");                                  // save the read resource array
    emitter.instruction("str x1, [sp, #2056]");                                  // save the write resource array
    emitter.instruction("str x2, [sp, #2064]");                                  // save the except resource array
    // The timeout arrives in x3/x4, which are caller-saved: building the pollfd array
    // calls __rt_stream_fd for every entry (and __rt_user_wrapper_stream_cast for a
    // wrapper), so both are garbage by the time the timeout is computed. Spilling them
    // here is what keeps `stream_select($r, $w, $e, 0, 200000)` from returning instantly.
    emitter.instruction("str x3, [sp, #2168]");                                  // save the timeout seconds
    emitter.instruction("str x4, [sp, #2176]");                                  // save the timeout microseconds

    // -- count total fds = read_len + write_len + except_len --
    emitter.instruction("mov x9, #0");                                           // a null set is an EMPTY set, not a fault
    crate::codegen_support::sentinels::emit_branch_if_null_container(
        emitter, "x0", "x12", "__rt_stream_select_len_r_done");
    emitter.instruction("ldr x9, [x0]");                                         // read array length
    emitter.label("__rt_stream_select_len_r_done");
    emitter.instruction("mov x10, #0");
    crate::codegen_support::sentinels::emit_branch_if_null_container(
        emitter, "x1", "x12", "__rt_stream_select_len_w_done");
    emitter.instruction("ldr x10, [x1]");                                        // write array length
    emitter.label("__rt_stream_select_len_w_done");
    emitter.instruction("mov x11, #0");
    crate::codegen_support::sentinels::emit_branch_if_null_container(
        emitter, "x2", "x12", "__rt_stream_select_len_e_done");
    emitter.instruction("ldr x11, [x2]");                                        // except array length
    emitter.label("__rt_stream_select_len_e_done");
    emitter.instruction("str x9, [sp, #2072]");                                   // save original read length
    emitter.instruction("str x10, [sp, #2080]");                                  // save original write length
    emitter.instruction("str x11, [sp, #2088]");                                  // save original except length
    emitter.instruction("add x12, x9, x10");                                      // sum read + write lengths
    emitter.instruction("add x12, x12, x11");                                     // add except length
    emitter.instruction("str x12, [sp, #2096]");                                   // save total nfds
    emitter.instruction("cbz x12, __rt_stream_select_no_castable");               // no entries at all → php's ValueError
    emitter.instruction("mov x13, #256");                                         // pollfd capacity ceiling
    emitter.instruction("cmp x12, x13");                                         // exceeds the stack-allocated capacity?
    emitter.instruction("b.gt __rt_stream_select_error");                          // too many fds → return -1

    // -- build the pollfd array: read (POLLIN), write (POLLOUT), except (POLLPRI) --
    emitter.instruction("mov x14, #0");                                          // running pollfd index
    emitter.instruction("str xzr, [sp, #2184]");                                  // clear the castable-descriptor tally
    emit_build_pollfd_aarch64(emitter, 2048, 2072, POLLIN, "r");
    emit_build_pollfd_aarch64(emitter, 2056, 2080, POLLOUT, "w");
    emit_build_pollfd_aarch64(emitter, 2064, 2088, POLLPRI, "e");
    // php counts the streams it could cast to a descriptor and raises `ValueError: No stream
    // arrays were passed` when that count is zero — the same error an empty array gets, because
    // php cannot tell "you passed nothing" from "nothing you passed is selectable".
    emitter.instruction("ldr x9, [sp, #2184]");                                   // how many entries yielded a real descriptor?
    emitter.instruction("cbz x9, __rt_stream_select_no_castable");                // none → php's ValueError

    // -- compute the poll timeout in milliseconds (reloaded from the frame) --
    emit_compute_timeout_aarch64(emitter, linux);

    // -- invoke poll(2) --
    emitter.instruction("mov x0, sp");                                          // pollfds pointer = frame base
    emitter.instruction("ldr x1, [sp, #2096]");                                   // nfds
    emitter.instruction("ldr x2, [sp, #2112]");                                   // timeout in ms
    // ARM64 Linux has NO `poll` syscall: number 73 in the generic table is `ppoll`, whose
    // third argument is a `struct timespec *`, not a millisecond count. A real millisecond
    // value was therefore read as a pointer and faulted (EFAULT, so stream_select answered
    // -1), while a zero was read as a NULL timespec, i.e. block forever — which is what hung
    // the linux-aarch64 stream_select tests for 60s. libc's poll() takes exactly the
    // millisecond contract this helper already computes, on both platforms.
    emitter.bl_c("poll");
    emitter.instruction("str x0, [sp, #2104]");                                   // save the ready descriptor count
    emitter.instruction("cmp x0, #0");                                          // poll returned an error?
    emitter.instruction("b.lt __rt_stream_select_error");                        // negative → return -1

    // -- compact each array to the ready subset --
    emit_compact_pollfd_aarch64(emitter, 2048, 2072, "r");
    emit_compact_pollfd_aarch64(emitter, 2056, 2080, "w");
    emit_compact_pollfd_aarch64(emitter, 2064, 2088, "e");

    emitter.instruction("ldr x0, [sp, #2104]");                                   // return the ready descriptor count
    emitter.instruction("b __rt_stream_select_epilogue");                         // jump to the common epilogue

    emitter.label("__rt_stream_select_error");
    emitter.instruction("mov x0, #-1");                                         // return -1 on poll failure or overflow
    emitter.instruction("b __rt_stream_select_epilogue");                         // jump to the common epilogue

    emitter.label("__rt_stream_select_no_castable");
    // -2 is the lowering's cue to raise php's `ValueError: No stream arrays were passed`. It has
    // to be distinct from the -1 that means "poll failed", which php answers as `false`.
    emitter.instruction("mov x0, #-2");                                         // nothing selectable was passed

    emitter.label("__rt_stream_select_epilogue");
    emitter.instruction("add x9, sp, #2256");                                     // compute the fp/lr restore address (offset > 504 needs add)
    emitter.instruction("ldp x29, x30, [x9]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #2304");                                    // release the poll-based select frame
    emitter.instruction("ret");                                                  // return to the caller
}

/// Builds one pollfd section from a resource array stored at `arr_off`, using
/// the length at `len_off` and the poll event mask `events`. The running pollfd
/// index lives in `x14` across calls so the three sections are contiguous.
fn emit_build_pollfd_aarch64(emitter: &mut Emitter, arr_off: i64, len_off: i64, events: i64, suffix: &str) {
    let loop_l = format!("__rt_stream_select_build_{}_loop", suffix);
    let next_l = format!("__rt_stream_select_build_{}_next", suffix);
    let unbox_l = format!("__rt_stream_select_build_{}_unbox", suffix);
    let after_unbox_l = format!("__rt_stream_select_build_{}_after_unbox", suffix);
    let cast_l = format!("__rt_stream_select_build_{}_cast", suffix);
    let cast_done_l = format!("__rt_stream_select_build_{}_cast_done", suffix);
    let cast_ok_l = format!("__rt_stream_select_build_{}_cast_ok", suffix);
    let memory_refused_l = format!("__rt_stream_select_build_{}_mem_refused", suffix);
    let done_l = format!("__rt_stream_select_build_{}_done", suffix);

    emitter.instruction(&format!("ldr x9, [sp, #{}]", arr_off));                // load the resource array pointer
    emitter.instruction(&format!("ldr x10, [sp, #{}]", len_off));               // load the section length
    crate::codegen_support::sentinels::emit_branch_if_null_container(
        emitter, "x9", "x4", &done_l);                                          // a null set has no header to read
    emitter.instruction("ldr x4, [x9, #-8]");                                    // load the packed indexed-array kind word
    emitter.instruction("lsr x4, x4, #8");                                       // shift the value_type tag into the low byte
    emitter.instruction("and x4, x4, #0x7f");                                     // isolate the value_type tag
    emitter.instruction("add x12, x9, #24");                                      // skip the array header to the data region
    emitter.instruction("mov x11, #0");                                          // element index
    emitter.label(&loop_l);
    emitter.instruction("cmp x11, x10");                                         // scanned every element?
    emitter.instruction(&format!("b.ge {}", done_l));                            // section is fully built
    emitter.instruction("ldr x13, [x12, x11, lsl #3]");                         // load the slot value (raw fd or Mixed* cell)
    emitter.instruction("cmp x4, #7");                                          // is this a Mixed-boxed indexed array?
    emitter.instruction(&format!("b.eq {}", unbox_l));                           // unbox the Mixed cell to get the underlying fd
    emitter.instruction(&format!("b {}", after_unbox_l));                        // raw-int array: x13 already holds the fd
    emitter.label(&unbox_l);
    emitter.instruction(&format!("cbz x13, {}", next_l));                        // null Mixed cell → skip the descriptor
    emitter.instruction("ldr x13, [x13, #8]");                                   // payload_lo of the Mixed cell is the fd
    emitter.label(&after_unbox_l);
    // -- resolve the opaque registry handle to its backend descriptor --
    // Array slots hold generation-packed stream handles, not raw fds, so the
    // wrapper probe below must run on the resolved descriptor. __rt_stream_fd
    // passes raw descriptors through unchanged, so this stays correct for
    // transitional int arrays.
    emitter.instruction("str x9, [sp, #2120]");                                  // spill the array pointer across the resolve call
    emitter.instruction("str x10, [sp, #2128]");                                 // spill the section length
    emitter.instruction("str x11, [sp, #2136]");                                 // spill the element index
    emitter.instruction("str x4, [sp, #2144]");                                  // spill the value_type tag
    emitter.instruction("str x12, [sp, #2152]");                                 // spill the data-region pointer
    emitter.instruction("str x14, [sp, #2160]");                                 // spill the running pollfd index
    emitter.instruction("mov x0, x13");                                          // opaque stream handle → descriptor lookup
    // A `php://memory` stream is bytes in the heap, so there is no descriptor to poll: php names
    // the type and drops the entry. The guard answers -1 for one, which is the same "unusable
    // descriptor" the resolve below produces for a closed handle, so the store, the tally and
    // the compact pass all treat it the way they already treat those.
    emitter.instruction(&format!("bl {}", "__rt_stream_select_memory_guard"));   // -1 once it has warned
    emitter.instruction("cmn x0, #1");                                          // did the guard refuse it?
    emitter.instruction(&format!("b.eq {}", memory_refused_l));                 // skip the resolve; -1 is already the answer
    emitter.instruction("bl __rt_stream_fd");                                    // resolve the backend descriptor through StreamState
    emitter.label(&memory_refused_l);
    emitter.instruction("mov x13, x0");                                          // adopt the resolved descriptor
    emitter.instruction("ldr x9, [sp, #2120]");                                  // reload the array pointer
    emitter.instruction("ldr x10, [sp, #2128]");                                 // reload the section length
    emitter.instruction("ldr x11, [sp, #2136]");                                 // reload the element index
    emitter.instruction("ldr x4, [sp, #2144]");                                  // reload the value_type tag
    emitter.instruction("ldr x12, [sp, #2152]");                                 // reload the data-region pointer
    emitter.instruction("ldr x14, [sp, #2160]");                                 // reload the running pollfd index
    // -- resolve synthetic user-wrapper fds to a real selectable fd via stream_cast --
    // __rt_stream_fd answers -1 for a handle it cannot resolve, which is what a CLOSED stream
    // gives. -1 has every bit set, so the synthetic-handle test below matched it and handed -1
    // to the wrapper cast, which dereferenced it: `fclose($f); stream_select([$f], ...)`
    // SEGFAULTED. php drops such an entry silently (`php_stream_from_zval_no_verify` yields
    // NULL and the loop continues), so -1 goes straight to the store, where it is recorded as
    // an unusable descriptor.
    emitter.instruction("cmn x13, #1");                                          // is the resolved descriptor -1?
    emitter.instruction(&format!("b.eq {}", cast_done_l));                       // unresolvable handle: never cast it
    emitter.instruction("tst x13, #0x40000000");                                 // is this a synthetic user-wrapper descriptor?
    emitter.instruction(&format!("b.eq {}", cast_done_l));                       // ordinary OS fd → use it directly
    emitter.label(&cast_l);
    emitter.instruction("str x9, [sp, #2120]");                                  // spill the array pointer across the cast call
    emitter.instruction("str x10, [sp, #2128]");                                 // spill the section length
    emitter.instruction("str x11, [sp, #2136]");                                 // spill the element index
    emitter.instruction("str x4, [sp, #2144]");                                  // spill the value_type tag
    emitter.instruction("str x12, [sp, #2152]");                                 // spill the data-region pointer
    emitter.instruction("str x14, [sp, #2160]");                                 // spill the running pollfd index
    emitter.instruction("mov x0, x13");                                          // synthetic fd → stream_cast argument
    emitter.instruction("mov x1, #3");                                          // STREAM_CAST_FOR_SELECT
    // stream_cast() returns a PHP resource, i.e. an opaque registry handle, so
    // it needs the same descriptor resolution before poll() sees it. Without
    // this poll() got a handle, reported POLLNVAL, counted it in the ready
    // total, and then the revents & 0x7 keep-mask dropped the slot.
    emitter.instruction("str x13, [sp, #2192]");                                 // the synthetic fd, for the diagnostics below
    emitter.instruction("bl __rt_user_wrapper_stream_cast");                      // resolve to the wrapper's underlying stream (or -1)
    emitter.instruction("bl __rt_stream_fd");                                    // cast result is a resource handle: resolve to its descriptor
    emitter.instruction("mov x13, x0");                                         // adopt the resolved descriptor
    // php EXPLAINS a stream it cannot select on: `W::stream_cast is not implemented!` when the
    // class defines no such method, then `Cannot represent a stream of type user-space as a
    // select()able descriptor` either way. elephc raised neither, so the `ValueError` that follows
    // arrived with nothing to say which stream caused it. Measured on `php -n` 8.5.6.
    emitter.instruction("cmn x13, #1");                                          // did the cast refuse?
    emitter.instruction(&format!("b.ne {}", cast_ok_l));
    emitter.instruction("ldr x0, [sp, #2192]");                                  // the synthetic fd it refused
    emitter.instruction("bl __rt_stream_select_cast_warning");
    emitter.instruction("mov x13, #-1");                                         // the warning helper clobbers the result register
    emitter.label(&cast_ok_l);
    emitter.instruction("ldr x9, [sp, #2120]");                                  // reload the array pointer
    emitter.instruction("ldr x10, [sp, #2128]");                                 // reload the section length
    emitter.instruction("ldr x11, [sp, #2136]");                                 // reload the element index
    emitter.instruction("ldr x4, [sp, #2144]");                                  // reload the value_type tag
    emitter.instruction("ldr x12, [sp, #2152]");                                 // reload the data-region pointer
    emitter.instruction("ldr x14, [sp, #2160]");                                 // reload the running pollfd index
    emitter.label(&cast_done_l);
    // -- store the pollfd entry at pollfds[x14] (even for fd=-1 so indices stay aligned) --
    emitter.instruction("lsl x15, x14, #3");                                     // byte offset = pollfd_index * 8
    emitter.instruction("add x15, sp, x15");                                     // pollfd address = frame base + offset
    emitter.instruction("str w13, [x15]");                                       // store the fd (low 32 bits; -1 is fine, poll reports POLLNVAL)
    emitter.instruction(&format!("mov w16, #{}", events));                       // event mask (POLLIN/POLLOUT/POLLPRI)
    emitter.instruction("strh w16, [x15, #4]");                                   // store the events field (16-bit)
    emitter.instruction("strh wzr, [x15, #6]");                                  // clear the revents field
    emitter.instruction("add x14, x14, #1");                                     // advance the running pollfd index
    // php counts only the entries it could CAST; the tally decides between running the poll and
    // raising `No stream arrays were passed`. The pollfd slot is still written above, because the
    // compact pass indexes by array position and the two loops must stay aligned.
    emitter.instruction("cmn x13, #1");                                          // did this entry yield a real descriptor?
    emitter.instruction(&format!("b.eq {}", next_l));                             // -1 contributes nothing to the tally
    emitter.instruction("ldr x17, [sp, #2184]");                                 // load the castable-descriptor tally
    emitter.instruction("add x17, x17, #1");                                     // one more selectable entry
    emitter.instruction("str x17, [sp, #2184]");                                 // store it back
    emitter.label(&next_l);
    emitter.instruction("add x11, x11, #1");                                     // advance to the next element
    emitter.instruction(&format!("b {}", loop_l));                              // continue scanning the array
    emitter.label(&done_l);
}

/// Compacts one resource array in place, keeping only the slots whose
/// corresponding pollfd entry has a non-zero `revents`. The pollfd section
/// for this array starts at `pollfd_base + section_offset*8`.
fn emit_compact_pollfd_aarch64(emitter: &mut Emitter, arr_off: i64, len_off: i64, suffix: &str) {
    let loop_l = format!("__rt_stream_select_keep_{}_loop", suffix);
    let next_l = format!("__rt_stream_select_keep_{}_next", suffix);
    let unbox_l = format!("__rt_stream_select_keep_{}_unbox", suffix);
    let after_unbox_l = format!("__rt_stream_select_keep_{}_after_unbox", suffix);
    let cast_done_l = format!("__rt_stream_select_keep_{}_cast_done", suffix);
    let done_l = format!("__rt_stream_select_keep_{}_done", suffix);
    let skip_l = format!("__rt_stream_select_keep_{}_skip", suffix);
    let memory_refused_l = format!("__rt_stream_select_keep_{}_mem_refused", suffix);

    emitter.instruction(&format!("ldr x9, [sp, #{}]", arr_off));                // load the resource array pointer
    emitter.instruction(&format!("ldr x10, [sp, #{}]", len_off));               // load the original section length
    crate::codegen_support::sentinels::emit_branch_if_null_container(
        emitter, "x9", "x4", &skip_l);                                          // a null set has no header to read
    emitter.instruction("ldr x4, [x9, #-8]");                                    // load the packed indexed-array kind word
    emitter.instruction("lsr x4, x4, #8");                                       // shift the value_type tag into the low byte
    emitter.instruction("and x4, x4, #0x7f");                                     // isolate the value_type tag
    emitter.instruction("add x12, x9, #24");                                      // skip the array header to the data region
    emitter.instruction("mov x11, #0");                                          // source element index
    emitter.instruction("mov x13, #0");                                          // destination (kept) element index
    // -- compute the pollfd section base: this array's pollfds start at index section_offset --
    // section_offset = read_len (for read), read_len+write_len (for write), read_len+write_len+except_len (for except)
    // We recompute it from the saved lengths to avoid carrying another register.
    // -- compute the pollfd section offset: read=0, write=read_len, except=read_len+write_len --
    emitter.instruction("mov x14, #0");                                          // section_offset = 0 (read is the first section)
    if suffix == "w" {
        emitter.instruction("ldr x14, [sp, #2072]");                             // section_offset = read_len (write starts after read)
    } else if suffix == "e" {
        emitter.instruction("ldr x14, [sp, #2072]");                             // read_len
        emitter.instruction("ldr x15, [sp, #2080]");                            // write_len
        emitter.instruction("add x14, x14, x15");                                // section_offset = read_len + write_len
    }
    emitter.label(&loop_l);
    emitter.instruction("cmp x11, x10");                                         // scanned every element?
    emitter.instruction(&format!("b.ge {}", done_l));                            // compaction is complete
    emitter.instruction("ldr x15, [x12, x11, lsl #3]");                         // load the raw slot value (preserved for the kept-array store)
    // -- extract the fd for the pollfd lookup (mirror of the build path) --
    emitter.instruction("mov x16, x15");                                         // copy for fd extraction; x15 stays the slot's stored value
    emitter.instruction("cmp x4, #7");                                          // is this a Mixed-boxed indexed array?
    emitter.instruction(&format!("b.eq {}", unbox_l));                           // unbox the Mixed cell to get the underlying fd
    emitter.instruction(&format!("b {}", after_unbox_l));                        // raw-int array: x16 already holds the fd
    emitter.label(&unbox_l);
    emitter.instruction(&format!("cbz x16, {}", next_l));                         // null Mixed cell → drop the slot
    emitter.instruction("ldr x16, [x16, #8]");                                   // payload_lo of the Mixed cell is the fd
    emitter.label(&after_unbox_l);
    // -- resolve the opaque registry handle to its backend descriptor --
    // Array slots hold generation-packed stream handles, not raw fds, so the
    // wrapper probe below must run on the resolved descriptor. __rt_stream_fd
    // passes raw descriptors through unchanged, so this stays correct for
    // transitional int arrays.
    emitter.instruction("str x9, [sp, #2120]");                                  // spill the array pointer across the resolve call
    emitter.instruction("str x10, [sp, #2128]");                                 // spill the section length
    emitter.instruction("str x11, [sp, #2136]");                                 // spill the source index
    emitter.instruction("str x13, [sp, #2144]");                                 // spill the destination index
    emitter.instruction("str x14, [sp, #2152]");                                 // spill the section offset
    emitter.instruction("str x4, [sp, #2160]");                                  // spill the value_type tag
    emitter.instruction("str x12, [sp, #2168]");                                 // spill the data-region pointer
    emitter.instruction("str x15, [sp, #2176]");                                 // spill the original slot value
    emitter.instruction("mov x0, x16");                                          // opaque stream handle → descriptor lookup
    // php walks the arrays TWICE — once to build the descriptor sets and once to translate the
    // result back — and names an unrepresentable stream on both passes, so a memory stream sitting
    // beside a real one warns twice. This pass only runs when `poll` did, which is why the
    // memory-ONLY call still warns once: its `ValueError` is raised before the poll.
    emitter.instruction("bl __rt_stream_select_memory_guard");                   // -1 once it has warned
    emitter.instruction("cmn x0, #1");                                          // did the guard refuse it?
    emitter.instruction(&format!("b.eq {}", memory_refused_l));                 // skip the resolve; -1 is already the answer
    emitter.instruction("bl __rt_stream_fd");                                    // resolve the backend descriptor through StreamState
    emitter.label(&memory_refused_l);
    emitter.instruction("mov x16, x0");                                          // adopt the resolved descriptor
    emitter.instruction("ldr x9, [sp, #2120]");                                  // reload the array pointer
    emitter.instruction("ldr x10, [sp, #2128]");                                 // reload the section length
    emitter.instruction("ldr x11, [sp, #2136]");                                 // reload the source index
    emitter.instruction("ldr x13, [sp, #2144]");                                 // reload the destination index
    emitter.instruction("ldr x14, [sp, #2152]");                                 // reload the section offset
    emitter.instruction("ldr x4, [sp, #2160]");                                  // reload the value_type tag
    emitter.instruction("ldr x12, [sp, #2168]");                                 // reload the data-region pointer
    emitter.instruction("ldr x15, [sp, #2176]");                                 // reload the original slot value
    // -- resolve synthetic user-wrapper fds (idempotent with build) --
    // The same -1 guard the build pass carries: this loop must reach `cast_done` on exactly the
    // entries the build pass did, or the two disagree about which pollfd belongs to which slot.
    emitter.instruction("cmn x16, #1");                                          // is the resolved descriptor -1?
    emitter.instruction(&format!("b.eq {}", cast_done_l));                       // unresolvable handle: never cast it
    emitter.instruction("tst x16, #0x40000000");                                 // is this a synthetic user-wrapper descriptor?
    emitter.instruction(&format!("b.eq {}", cast_done_l));                       // ordinary OS fd → use it directly
    emitter.instruction("str x9, [sp, #2120]");                                  // spill the array pointer across the cast call
    emitter.instruction("str x10, [sp, #2128]");                                 // spill the section length
    emitter.instruction("str x11, [sp, #2136]");                                 // spill the source index
    emitter.instruction("str x13, [sp, #2144]");                                 // spill the destination index
    emitter.instruction("str x14, [sp, #2152]");                                 // spill the section offset
    emitter.instruction("str x4, [sp, #2160]");                                  // spill the value_type tag
    emitter.instruction("str x12, [sp, #2168]");                                 // spill the data-region pointer
    emitter.instruction("str x15, [sp, #2176]");                                 // spill the original slot value
    emitter.instruction("mov x0, x16");                                          // synthetic fd → stream_cast argument
    emitter.instruction("mov x1, #3");                                          // STREAM_CAST_FOR_SELECT
    // stream_cast() returns a PHP resource, i.e. an opaque registry handle, so
    // it needs the same descriptor resolution before poll() sees it. Without
    // this poll() got a handle, reported POLLNVAL, counted it in the ready
    // total, and then the revents & 0x7 keep-mask dropped the slot.
    emitter.instruction("bl __rt_user_wrapper_stream_cast");                     // resolve to the wrapper's underlying stream
    emitter.instruction("bl __rt_stream_fd");                                    // cast result is a resource handle: resolve to its descriptor
    emitter.instruction("mov x16, x0");                                         // adopt the resolved descriptor
    emitter.instruction("ldr x9, [sp, #2120]");                                  // reload the array pointer
    emitter.instruction("ldr x10, [sp, #2128]");                                 // reload the section length
    emitter.instruction("ldr x11, [sp, #2136]");                                 // reload the source index
    emitter.instruction("ldr x13, [sp, #2144]");                                 // reload the destination index
    emitter.instruction("ldr x14, [sp, #2152]");                                 // reload the section offset
    emitter.instruction("ldr x4, [sp, #2160]");                                  // reload the value_type tag
    emitter.instruction("ldr x12, [sp, #2168]");                                 // reload the data-region pointer
    emitter.instruction("ldr x15, [sp, #2176]");                                 // reload the original slot value
    emitter.label(&cast_done_l);
    // -- look up the pollfd entry at index (section_offset + source_index) --
    emitter.instruction("add x17, x14, x11");                                    // pollfd index = section_offset + source_index
    emitter.instruction("lsl x17, x17, #3");                                    // byte offset = pollfd_index * 8
    emitter.instruction("add x17, sp, x17");                                     // pollfd address = frame base + offset
    emitter.instruction("ldrh w8, [x17, #6]");                                   // load revents (w8: x18 is reserved on Apple, and x9 holds the array)
    // -- keep only if revents has a requested event bit (POLLIN|POLLOUT|POLLPRI = 0x007) --
    emitter.instruction("and w8, w8, #0x7");                                      // mask out POLLNVAL/POLLERR/POLLHUP
    emitter.instruction(&format!("cbz w8, {}", next_l));                         // revents & 0x7 == 0 → drop the slot
    // -- keep the slot: store the original value at the kept prefix --
    emitter.instruction("str x15, [x12, x13, lsl #3]");                          // keep the original slot value at the front
    emitter.instruction("add x13, x13, #1");                                     // advance the kept-descriptor index
    emitter.label(&next_l);
    emitter.instruction("add x11, x11, #1");                                     // advance to the next source element
    emitter.instruction(&format!("b {}", loop_l));                              // continue compacting the array
    emitter.label(&done_l);
    emitter.instruction("str x13, [x9]");                                        // store the compacted array length
    emitter.label(&skip_l);                                                      // a null set has no length to store back
}

/// Computes the poll timeout in milliseconds and stores it at `[sp, #2112]`.
///
/// The caller passes `seconds` in `x3` and `microseconds` in `x4` (ARM64).
/// A sentinel null/infinite value (negative or >= 0x7fffffff) maps to `-1`
/// (block forever); `seconds == 0 && microseconds == 0` maps to `0`.
fn emit_compute_timeout_aarch64(emitter: &mut Emitter, _linux: bool) {
    // The pollfd build clobbered x3/x4, so the timeout comes back off the frame.
    emitter.instruction("ldr x3, [sp, #2168]");                                 // reload the timeout seconds
    emitter.instruction("ldr x4, [sp, #2176]");                                 // reload the timeout microseconds
    emitter.instruction("cmp x3, #0");                                          // seconds == 0?
    emitter.instruction("b.ne __rt_stream_select_ts_pos");                        // positive seconds → compute timeout
    emitter.instruction("cmp x4, #0");                                          // microseconds == 0 too?
    emitter.instruction("b.ne __rt_stream_select_ts_us_only");                   // only microseconds → compute from us
    emitter.instruction("str xzr, [sp, #2112]");                                  // timeout = 0 (non-blocking poll)
    emitter.instruction("b __rt_stream_select_ts_done");                          // timeout computed
    emitter.label("__rt_stream_select_ts_us_only");
    emitter.instruction("mov x9, #1000");                                         // millisecond divisor
    emitter.instruction("udiv x0, x4, x9");                                       // microseconds / 1000 = ms
    emitter.instruction("str x0, [sp, #2112]");                                   // store the timeout
    emitter.instruction("b __rt_stream_select_ts_done");                          // timeout computed
    emitter.label("__rt_stream_select_ts_pos");
    emitter.instruction("tbnz x3, #63, __rt_stream_select_ts_inf");               // negative seconds → infinite
    emitter.instruction("mov x9, #0x7fffffff");                                   // sentinel threshold
    emitter.instruction("cmp x3, x9");                                          // seconds >= sentinel?
    emitter.instruction("b.ge __rt_stream_select_ts_inf");                        // treat as infinite
    emitter.instruction("mov x9, #1000");                                         // seconds → ms multiplier
    emitter.instruction("mul x0, x3, x9");                                       // seconds * 1000
    emitter.instruction("udiv x1, x4, x9");                                       // microseconds / 1000
    emitter.instruction("add x0, x0, x1");                                       // total timeout in ms
    emitter.instruction("str x0, [sp, #2112]");                                   // store the timeout
    emitter.instruction("b __rt_stream_select_ts_done");                          // timeout computed
    emitter.label("__rt_stream_select_ts_inf");
    emitter.instruction("mov x0, #-1");                                          // -1 = block forever
    emitter.instruction("str x0, [sp, #2112]");                                   // store the infinite timeout
    emitter.label("__rt_stream_select_ts_done");
}

/// x86_64 Linux variant of `__rt_stream_select` using `poll` (syscall 7).
///
/// Frame layout (4352 bytes, rbp-relative): `[rbp-4352..rbp-2304)` pollfd array (256 × 8),
/// `[rbp-2184..rbp-2048]` state slots. The pollfd array used to start at `rbp-2048`, which
/// is also the except-array slot: writing the FIRST pollfd entry overwrote that pointer and
/// the except compaction then dereferenced it. AArch64 was immune — there the pollfds sit at
/// `sp+0` and the state slots at `sp+2048`, so the two never met.
fn emit_stream_select_linux_x86_64(emitter: &mut Emitter) {
    let linux = emitter.target.platform == Platform::Linux;
    emitter.blank();
    emitter.comment("--- runtime: stream_select ---");
    emitter.label_global("__rt_stream_select");

    emitter.instruction("push rbp");                                             // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                         // establish the helper frame pointer
    emitter.instruction("sub rsp, 4352");                                        // pollfd array plus state slots, kept disjoint

    // -- save the three resource arrays --
    emitter.instruction("mov QWORD PTR [rbp - 2064], rdi");                       // save the read resource array
    emitter.instruction("mov QWORD PTR [rbp - 2056], rsi");                       // save the write resource array
    emitter.instruction("mov QWORD PTR [rbp - 2048], rdx");                       // save the except resource array
    // The timeout arrives in rcx/r8, which are caller-saved: building the pollfd array
    // calls __rt_stream_fd for every entry (and __rt_user_wrapper_stream_cast for a
    // wrapper), so both are garbage by the time the timeout is computed. Spilling them
    // here is what keeps `stream_select($r, $w, $e, 0, 200000)` from returning instantly.
    emitter.instruction("mov QWORD PTR [rbp - 2176], rcx");                       // save the timeout seconds
    emitter.instruction("mov QWORD PTR [rbp - 2184], r8");                        // save the timeout microseconds

    // -- count total fds = read_len + write_len + except_len --
    emitter.instruction("xor r9, r9");                                           // a null set is an EMPTY set, not a fault
    crate::codegen_support::sentinels::emit_branch_if_null_container(
        emitter, "rdi", "rax", "__rt_stream_select_len_r_done_x");
    emitter.instruction("mov r9, QWORD PTR [rdi]");                              // read array length
    emitter.label("__rt_stream_select_len_r_done_x");
    emitter.instruction("xor r10, r10");
    crate::codegen_support::sentinels::emit_branch_if_null_container(
        emitter, "rsi", "rax", "__rt_stream_select_len_w_done_x");
    emitter.instruction("mov r10, QWORD PTR [rsi]");                             // write array length
    emitter.label("__rt_stream_select_len_w_done_x");
    emitter.instruction("xor r11, r11");
    crate::codegen_support::sentinels::emit_branch_if_null_container(
        emitter, "rdx", "rax", "__rt_stream_select_len_e_done_x");
    emitter.instruction("mov r11, QWORD PTR [rdx]");                             // except array length
    emitter.label("__rt_stream_select_len_e_done_x");
    emitter.instruction("mov QWORD PTR [rbp - 2072], r9");                        // save original read length
    emitter.instruction("mov QWORD PTR [rbp - 2080], r10");                      // save original write length
    emitter.instruction("mov QWORD PTR [rbp - 2088], r11");                      // save original except length
    emitter.instruction("add r9, r10");                                          // sum read + write lengths
    emitter.instruction("add r9, r11");                                          // add except length
    emitter.instruction("mov QWORD PTR [rbp - 2096], r9");                       // save total nfds
    emitter.instruction("test r9, r9");                                          // no entries at all?
    emitter.instruction("jz __rt_stream_select_no_castable_x");                  // → php's ValueError
    emitter.instruction("cmp r9, 256");                                          // exceeds the stack-allocated capacity?
    emitter.instruction("ja __rt_stream_select_error_x");                         // too many fds → return -1

    // -- build the pollfd array: read (POLLIN), write (POLLOUT), except (POLLPRI) --
    emitter.instruction("xor r14, r14");                                         // running pollfd index
    emitter.instruction("mov QWORD PTR [rbp - 2184], 0");                        // clear the castable-descriptor tally
    emit_build_pollfd_x86(emitter, 2064, 2072, POLLIN, "r");
    emit_build_pollfd_x86(emitter, 2056, 2080, POLLOUT, "w");
    emit_build_pollfd_x86(emitter, 2048, 2088, POLLPRI, "e");
    // php counts the streams it could cast and raises `ValueError: No stream arrays were passed`
    // when that count is zero — it cannot tell "you passed nothing" from "nothing is selectable".
    emitter.instruction("cmp QWORD PTR [rbp - 2184], 0");                        // did anything yield a real descriptor?
    emitter.instruction("je __rt_stream_select_no_castable_x");                  // none → php's ValueError

    // -- compute the poll timeout in milliseconds (reloaded from the frame) --
    emit_compute_timeout_x86(emitter, linux);

    // -- invoke poll(2) --
    emitter.instruction("lea rdi, [rbp - 4352]");                                 // pollfds pointer = frame base
    emitter.instruction("mov rsi, QWORD PTR [rbp - 2096]");                      // nfds
    emitter.instruction("mov rdx, QWORD PTR [rbp - 2112]");                       // timeout in ms
    if linux {
        emitter.instruction("mov eax, 7");                                       // Linux x86_64 syscall number for poll
        emitter.instruction("syscall");                                          // invoke poll via the kernel
    } else {
        emitter.instruction("call _poll");                                       // macOS: poll from libSystem
    }
    emitter.instruction("mov QWORD PTR [rbp - 2104], rax");                       // save the ready descriptor count
    emitter.instruction("test rax, rax");                                        // poll returned an error?
    emitter.instruction("js __rt_stream_select_error_x");                        // negative → return -1

    // -- compact each array to the ready subset --
    emit_compact_pollfd_x86(emitter, 2064, 2072, "r");
    emit_compact_pollfd_x86(emitter, 2056, 2080, "w");
    emit_compact_pollfd_x86(emitter, 2048, 2088, "e");

    emitter.instruction("mov rax, QWORD PTR [rbp - 2104]");                       // return the ready descriptor count
    emitter.instruction("jmp __rt_stream_select_epilogue_x");                     // jump to the common epilogue

    emitter.label("__rt_stream_select_error_x");
    emitter.instruction("mov rax, -1");                                          // return -1 on poll failure or overflow
    emitter.instruction("jmp __rt_stream_select_epilogue_x");                     // jump to the common epilogue

    emitter.label("__rt_stream_select_no_castable_x");
    // -2 is the lowering's cue to raise php's `ValueError: No stream arrays were passed`, kept
    // distinct from the -1 that means "poll failed" and becomes `false`.
    emitter.instruction("mov rax, -2");                                          // nothing selectable was passed

    emitter.label("__rt_stream_select_epilogue_x");
    emitter.instruction("leave");                                                // restore rbp + rsp
    emitter.instruction("ret");                                                 // return to the caller
}

/// Builds one pollfd section (x86_64). The running pollfd index lives in `r14`.
fn emit_build_pollfd_x86(emitter: &mut Emitter, arr_off: i64, len_off: i64, events: i64, suffix: &str) {
    let loop_l = format!("__rt_stream_select_build_{}_loop_x", suffix);
    let next_l = format!("__rt_stream_select_build_{}_next_x", suffix);
    let unbox_l = format!("__rt_stream_select_build_{}_unbox_x", suffix);
    let after_unbox_l = format!("__rt_stream_select_build_{}_after_unbox_x", suffix);
    let cast_done_l = format!("__rt_stream_select_build_{}_cast_done_x", suffix);
    let cast_ok_l = format!("__rt_stream_select_build_{}_cast_ok_x", suffix);
    let memory_refused_l = format!("__rt_stream_select_build_{}_mem_refused_x", suffix);
    let done_l = format!("__rt_stream_select_build_{}_done_x", suffix);

    emitter.instruction(&format!("mov r11, QWORD PTR [rbp - {}]", arr_off));     // load the resource array pointer
    emitter.instruction(&format!("mov rdi, QWORD PTR [rbp - {}]", len_off));    // load the section length
    crate::codegen_support::sentinels::emit_branch_if_null_container(
        emitter, "r11", "r12", &done_l);                                        // a null set has no header to read
    emitter.instruction("mov r12, QWORD PTR [r11 - 8]");                         // load the packed indexed-array kind word
    emitter.instruction("shr r12, 8");                                          // shift the value_type tag into the low byte
    emitter.instruction("and r12, 0x7f");                                       // isolate the value_type tag
    emitter.instruction("xor rsi, rsi");                                        // element index
    emitter.label(&loop_l);
    emitter.instruction("cmp rsi, rdi");                                        // scanned every element?
    emitter.instruction(&format!("jae {}", done_l));                            // section is fully built
    emitter.instruction("mov rdx, QWORD PTR [r11 + 24 + rsi * 8]");              // load the slot value (raw fd or Mixed* cell)
    emitter.instruction("cmp r12, 7");                                          // is this a Mixed-boxed indexed array?
    emitter.instruction(&format!("je {}", unbox_l));                            // unbox the Mixed cell to get the underlying fd
    emitter.instruction(&format!("jmp {}", after_unbox_l));                      // raw-int array: rdx already holds the fd
    emitter.label(&unbox_l);
    emitter.instruction("test rdx, rdx");                                       // null Mixed cell?
    emitter.instruction(&format!("jz {}", next_l));                             // null Mixed cell → skip the descriptor
    emitter.instruction("mov rdx, QWORD PTR [rdx + 8]");                         // payload_lo of the Mixed cell is the fd
    emitter.label(&after_unbox_l);
    // -- resolve the opaque registry handle to its backend descriptor --
    // Array slots hold generation-packed stream handles, not raw fds, so the
    // wrapper probe below must run on the resolved descriptor. __rt_stream_fd
    // passes raw descriptors through unchanged, so this stays correct for
    // transitional int arrays.
    emitter.instruction("mov QWORD PTR [rbp - 2120], r11");                      // spill the array pointer across the resolve call
    emitter.instruction("mov QWORD PTR [rbp - 2128], rdi");                      // spill the section length
    emitter.instruction("mov QWORD PTR [rbp - 2136], rsi");                      // spill the element index
    emitter.instruction("mov QWORD PTR [rbp - 2144], r12");                      // spill the value_type tag
    emitter.instruction("mov QWORD PTR [rbp - 2152], r14");                      // spill the running pollfd index
    emitter.instruction("mov rdi, rdx");                                         // opaque stream handle → descriptor lookup
    // See the AArch64 counterpart: a `php://memory` stream has no descriptor to poll, and the
    // guard answers -1 once it has named the type the way php does.
    emitter.instruction("call __rt_stream_select_memory_guard");                 // -1 once it has warned
    emitter.instruction("cmp rax, -1");                                          // did the guard refuse it?
    emitter.instruction(&format!("je {}", memory_refused_l));                    // skip the resolve; -1 is already the answer
    emitter.instruction("mov rdi, rax");                                         // the handle the guard passed through
    emitter.instruction("call __rt_stream_fd");                                  // resolve the backend descriptor through StreamState
    emitter.label(&memory_refused_l);
    emitter.instruction("mov rdx, rax");                                         // adopt the resolved descriptor
    emitter.instruction("mov r11, QWORD PTR [rbp - 2120]");                      // reload the array pointer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 2128]");                      // reload the section length
    emitter.instruction("mov rsi, QWORD PTR [rbp - 2136]");                      // reload the element index
    emitter.instruction("mov r12, QWORD PTR [rbp - 2144]");                      // reload the value_type tag
    emitter.instruction("mov r14, QWORD PTR [rbp - 2152]");                      // reload the running pollfd index
    // -- resolve synthetic user-wrapper fds to a real selectable fd via stream_cast --
    // __rt_stream_fd answers -1 for a handle it cannot resolve, which is what a CLOSED stream
    // gives. -1 has every bit set, so the synthetic-handle test below matched it and handed -1
    // to the wrapper cast, which dereferenced it: `fclose($f); stream_select([$f], ...)`
    // SEGFAULTED. php drops such an entry silently, so -1 goes straight to the store.
    emitter.instruction("cmp rdx, -1");                                          // is the resolved descriptor -1?
    emitter.instruction(&format!("je {}", cast_done_l));                          // unresolvable handle: never cast it
    emitter.instruction("test rdx, 0x40000000");                                 // is this a synthetic user-wrapper descriptor?
    emitter.instruction(&format!("jz {}", cast_done_l));                          // ordinary OS fd → use it directly
    emitter.instruction("mov QWORD PTR [rbp - 2120], r11");                      // spill the array pointer across the cast call
    emitter.instruction("mov QWORD PTR [rbp - 2128], rdi");                      // spill the section length
    emitter.instruction("mov QWORD PTR [rbp - 2136], rsi");                      // spill the element index
    emitter.instruction("mov QWORD PTR [rbp - 2144], r12");                      // spill the value_type tag
    emitter.instruction("mov QWORD PTR [rbp - 2152], r14");                      // spill the running pollfd index
    emitter.instruction("mov rdi, rdx");                                         // synthetic fd → stream_cast argument
    emitter.instruction("mov esi, 3");                                          // STREAM_CAST_FOR_SELECT
    // stream_cast() returns a PHP resource, i.e. an opaque registry handle, so
    // it needs the same descriptor resolution before poll() sees it. Without
    // this poll() got a handle, reported POLLNVAL, counted it in the ready
    // total, and then the revents & 0x7 keep-mask dropped the slot.
    emitter.instruction("mov QWORD PTR [rbp - 2192], rdi");                      // the synthetic fd, for the diagnostics below
    emitter.instruction("call __rt_user_wrapper_stream_cast");                   // resolve to the wrapper's underlying stream (or -1)
    emitter.instruction("mov rdi, rax");                                         // cast result is a resource handle
    emitter.instruction("call __rt_stream_fd");                                  // resolve it to its backend descriptor
    emitter.instruction("mov rdx, rax");                                         // adopt the resolved descriptor
    // See the AArch64 counterpart: php EXPLAINS a stream it cannot select on.
    emitter.instruction("cmp rdx, -1");                                          // did the cast refuse?
    emitter.instruction(&format!("jne {}", cast_ok_l));
    emitter.instruction("mov rdi, QWORD PTR [rbp - 2192]");                      // the synthetic fd it refused
    emitter.instruction("call __rt_stream_select_cast_warning");
    emitter.instruction("mov rdx, -1");                                          // the warning helper clobbers the result register
    emitter.label(&cast_ok_l);
    emitter.instruction("mov r11, QWORD PTR [rbp - 2120]");                      // reload the array pointer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 2128]");                      // reload the section length
    emitter.instruction("mov rsi, QWORD PTR [rbp - 2136]");                      // reload the element index
    emitter.instruction("mov r12, QWORD PTR [rbp - 2144]");                      // reload the value_type tag
    emitter.instruction("mov r14, QWORD PTR [rbp - 2152]");                      // reload the running pollfd index
    emitter.label(&cast_done_l);
    // -- store the pollfd entry at pollfds[r14] (even for fd=-1 so indices stay aligned) --
    emitter.instruction("mov rax, r14");                                         // pollfd index
    emitter.instruction("shl rax, 3");                                          // byte offset = index * 8
    emitter.instruction("lea rax, [rbp - 4352 + rax]");                          // pollfd address = frame base + offset
    emitter.instruction("mov DWORD PTR [rax], edx");                             // store the fd (low 32 bits; -1 is fine, poll reports POLLNVAL)
    emitter.instruction(&format!("mov WORD PTR [rax + 4], {}", events));         // store the events field (16-bit)
    emitter.instruction("mov WORD PTR [rax + 6], 0");                            // clear the revents field
    emitter.instruction("inc r14");                                             // advance the running pollfd index
    // php counts only the entries it could CAST; see the AArch64 counterpart. The pollfd slot is
    // still written above so the compact pass stays aligned with this one.
    emitter.instruction("cmp rdx, -1");                                          // did this entry yield a real descriptor?
    emitter.instruction(&format!("je {}", next_l));                              // -1 contributes nothing to the tally
    emitter.instruction("inc QWORD PTR [rbp - 2184]");                           // one more selectable entry
    emitter.label(&next_l);
    emitter.instruction("inc rsi");                                             // advance to the next element
    emitter.instruction(&format!("jmp {}", loop_l));                             // continue scanning the array
    emitter.label(&done_l);
}

/// Compacts one resource array in place (x86_64), keeping only ready slots.
fn emit_compact_pollfd_x86(emitter: &mut Emitter, arr_off: i64, len_off: i64, suffix: &str) {
    let loop_l = format!("__rt_stream_select_keep_{}_loop_x", suffix);
    let next_l = format!("__rt_stream_select_keep_{}_next_x", suffix);
    let unbox_l = format!("__rt_stream_select_keep_{}_unbox_x", suffix);
    let after_unbox_l = format!("__rt_stream_select_keep_{}_after_unbox_x", suffix);
    let cast_done_l = format!("__rt_stream_select_keep_{}_cast_done_x", suffix);
    let done_l = format!("__rt_stream_select_keep_{}_done_x", suffix);
    let skip_l = format!("__rt_stream_select_keep_{}_skip_x", suffix);
    let memory_refused_l = format!("__rt_stream_select_keep_{}_mem_refused_x", suffix);

    emitter.instruction(&format!("mov r11, QWORD PTR [rbp - {}]", arr_off));     // load the resource array pointer
    emitter.instruction(&format!("mov rdi, QWORD PTR [rbp - {}]", len_off));    // load the original section length
    crate::codegen_support::sentinels::emit_branch_if_null_container(
        emitter, "r11", "r12", &skip_l);                                        // a null set has no header to read
    emitter.instruction("mov r12, QWORD PTR [r11 - 8]");                         // load the packed indexed-array kind word
    emitter.instruction("shr r12, 8");                                          // shift the value_type tag into the low byte
    emitter.instruction("and r12, 0x7f");                                       // isolate the value_type tag
    emitter.instruction("xor rsi, rsi");                                        // source element index
    emitter.instruction("xor r9, r9");                                          // destination (kept) element index
    // -- compute the pollfd section offset: read=0, write=read_len, except=read_len+write_len --
    emitter.instruction("xor r14, r14");                                         // section_offset = 0 (read is the first section)
    if suffix == "w" {
        emitter.instruction("mov r14, QWORD PTR [rbp - 2072]");                  // section_offset = read_len (write starts after read)
    } else if suffix == "e" {
        emitter.instruction("mov r14, QWORD PTR [rbp - 2072]");                  // read_len
        emitter.instruction("mov r15, QWORD PTR [rbp - 2080]");                  // write_len
        emitter.instruction("add r14, r15");                                     // section_offset = read_len + write_len
    }
    emitter.label(&loop_l);
    emitter.instruction("cmp rsi, rdi");                                        // scanned every element?
    emitter.instruction(&format!("jae {}", done_l));                            // compaction is complete
    emitter.instruction("mov r13, QWORD PTR [r11 + 24 + rsi * 8]");              // load the raw slot value (preserved for the kept-array store)
    emitter.instruction("mov rdx, r13");                                         // copy for fd extraction; r13 stays the slot's stored value
    emitter.instruction("cmp r12, 7");                                          // is this a Mixed-boxed indexed array?
    emitter.instruction(&format!("je {}", unbox_l));                            // unbox the Mixed cell to get the underlying fd
    emitter.instruction(&format!("jmp {}", after_unbox_l));                      // raw-int array: rdx already holds the fd
    emitter.label(&unbox_l);
    emitter.instruction("test rdx, rdx");                                       // null Mixed cell?
    emitter.instruction(&format!("jz {}", next_l));                             // null Mixed cell → drop the slot
    emitter.instruction("mov rdx, QWORD PTR [rdx + 8]");                         // payload_lo of the Mixed cell is the fd
    emitter.label(&after_unbox_l);
    // -- resolve the opaque registry handle to its backend descriptor --
    // Array slots hold generation-packed stream handles, not raw fds, so the
    // wrapper probe below must run on the resolved descriptor. __rt_stream_fd
    // passes raw descriptors through unchanged, so this stays correct for
    // transitional int arrays.
    emitter.instruction("mov QWORD PTR [rbp - 2120], r11");                      // spill the array pointer across the resolve call
    emitter.instruction("mov QWORD PTR [rbp - 2128], rdi");                      // spill the section length
    emitter.instruction("mov QWORD PTR [rbp - 2136], rsi");                      // spill the source index
    emitter.instruction("mov QWORD PTR [rbp - 2144], r9");                       // spill the destination index
    emitter.instruction("mov QWORD PTR [rbp - 2152], r14");                      // spill the section offset
    emitter.instruction("mov QWORD PTR [rbp - 2160], r12");                      // spill the value_type tag
    emitter.instruction("mov QWORD PTR [rbp - 2168], r13");                      // spill the original slot value
    emitter.instruction("mov rdi, rdx");                                         // opaque stream handle → descriptor lookup
    // See the AArch64 counterpart: php names an unrepresentable stream on BOTH of its passes.
    emitter.instruction("call __rt_stream_select_memory_guard");                 // -1 once it has warned
    emitter.instruction("cmp rax, -1");                                          // did the guard refuse it?
    emitter.instruction(&format!("je {}", memory_refused_l));                    // skip the resolve; -1 is already the answer
    emitter.instruction("mov rdi, rax");                                         // the handle the guard passed through
    emitter.instruction("call __rt_stream_fd");                                  // resolve the backend descriptor through StreamState
    emitter.label(&memory_refused_l);
    emitter.instruction("mov rdx, rax");                                         // adopt the resolved descriptor
    emitter.instruction("mov r11, QWORD PTR [rbp - 2120]");                      // reload the array pointer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 2128]");                      // reload the section length
    emitter.instruction("mov rsi, QWORD PTR [rbp - 2136]");                      // reload the source index
    emitter.instruction("mov r9, QWORD PTR [rbp - 2144]");                       // reload the destination index
    emitter.instruction("mov r14, QWORD PTR [rbp - 2152]");                      // reload the section offset
    emitter.instruction("mov r12, QWORD PTR [rbp - 2160]");                      // reload the value_type tag
    emitter.instruction("mov r13, QWORD PTR [rbp - 2168]");                      // reload the original slot value
    // -- resolve synthetic user-wrapper fds (idempotent with build) --
    // The same -1 guard the build pass carries: this loop must reach `cast_done` on exactly the
    // entries the build pass did, or the two disagree about which pollfd belongs to which slot.
    emitter.instruction("cmp rdx, -1");                                          // is the resolved descriptor -1?
    emitter.instruction(&format!("je {}", cast_done_l));                         // unresolvable handle: never cast it
    emitter.instruction("test rdx, 0x40000000");                                 // is this a synthetic user-wrapper descriptor?
    emitter.instruction(&format!("jz {}", cast_done_l));                         // ordinary OS fd → use it directly
    emitter.instruction("mov QWORD PTR [rbp - 2120], r11");                      // spill the array pointer across the cast call
    emitter.instruction("mov QWORD PTR [rbp - 2128], rdi");                      // spill the section length
    emitter.instruction("mov QWORD PTR [rbp - 2136], rsi");                      // spill the source index
    emitter.instruction("mov QWORD PTR [rbp - 2144], r9");                       // spill the destination index
    emitter.instruction("mov QWORD PTR [rbp - 2152], r14");                      // spill the section offset
    emitter.instruction("mov QWORD PTR [rbp - 2160], r12");                      // spill the value_type tag
    emitter.instruction("mov QWORD PTR [rbp - 2168], r13");                      // spill the original slot value
    emitter.instruction("mov rdi, rdx");                                         // synthetic fd → stream_cast argument
    emitter.instruction("mov esi, 3");                                          // STREAM_CAST_FOR_SELECT
    // stream_cast() returns a PHP resource, i.e. an opaque registry handle, so
    // it needs the same descriptor resolution before poll() sees it. Without
    // this poll() got a handle, reported POLLNVAL, counted it in the ready
    // total, and then the revents & 0x7 keep-mask dropped the slot.
    emitter.instruction("call __rt_user_wrapper_stream_cast");                   // resolve to the wrapper's underlying stream
    emitter.instruction("mov rdi, rax");                                         // cast result is a resource handle
    emitter.instruction("call __rt_stream_fd");                                  // resolve it to its backend descriptor
    emitter.instruction("mov rdx, rax");                                         // adopt the resolved descriptor
    emitter.instruction("mov r11, QWORD PTR [rbp - 2120]");                      // reload the array pointer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 2128]");                      // reload the section length
    emitter.instruction("mov rsi, QWORD PTR [rbp - 2136]");                      // reload the source index
    emitter.instruction("mov r9, QWORD PTR [rbp - 2144]");                       // reload the destination index
    emitter.instruction("mov r14, QWORD PTR [rbp - 2152]");                      // reload the section offset
    emitter.instruction("mov r12, QWORD PTR [rbp - 2160]");                      // reload the value_type tag
    emitter.instruction("mov r13, QWORD PTR [rbp - 2168]");                      // reload the original slot value
    emitter.label(&cast_done_l);
    // -- look up the pollfd entry at index (section_offset + source_index) --
    emitter.instruction("mov rax, r14");                                         // section offset
    emitter.instruction("add rax, rsi");                                         // pollfd index = section_offset + source_index
    emitter.instruction("shl rax, 3");                                          // byte offset = pollfd_index * 8
    emitter.instruction("lea rax, [rbp - 4352 + rax]");                          // pollfd address = frame base + offset
    emitter.instruction("movzx eax, WORD PTR [rax + 6]");                         // load the revents field (16-bit)
    emitter.instruction("and eax, 7");                                          // mask out POLLNVAL/POLLERR/POLLHUP (keep POLLIN|POLLOUT|POLLPRI)
    emitter.instruction("test eax, eax");                                       // revents & 0x7 == 0?
    emitter.instruction(&format!("jz {}", next_l));                              // drop the slot if no requested event is ready
    // -- keep the slot: store the original value at the kept prefix --
    emitter.instruction("mov QWORD PTR [r11 + 24 + r9 * 8], r13");               // keep the original slot value at the front
    emitter.instruction("inc r9");                                              // advance the kept-descriptor index
    emitter.label(&next_l);
    emitter.instruction("inc rsi");                                             // advance to the next source element
    emitter.instruction(&format!("jmp {}", loop_l));                             // continue compacting the array
    emitter.label(&done_l);
    emitter.instruction("mov QWORD PTR [r11], r9");                             // store the compacted array length
    emitter.label(&skip_l);                                                     // a null set has no length to store back
}

/// Computes the poll timeout in milliseconds (x86_64) and stores it at `[rbp-2112]`.
fn emit_compute_timeout_x86(emitter: &mut Emitter, _linux: bool) {
    // The pollfd build clobbered rcx/r8, so the timeout comes back off the frame.
    emitter.instruction("mov rcx, QWORD PTR [rbp - 2176]");                      // reload the timeout seconds
    emitter.instruction("mov r8, QWORD PTR [rbp - 2184]");                       // reload the timeout microseconds
    emitter.instruction("test rcx, rcx");                                        // seconds == 0?
    emitter.instruction("jnz __rt_stream_select_ts_pos_x");                      // positive seconds → compute timeout
    emitter.instruction("test r8, r8");                                          // microseconds == 0 too?
    emitter.instruction("jnz __rt_stream_select_ts_us_only_x");                  // only microseconds → compute from us
    emitter.instruction("mov QWORD PTR [rbp - 2112], 0");                        // timeout = 0 (non-blocking poll)
    emitter.instruction("jmp __rt_stream_select_ts_done_x");                     // timeout computed
    emitter.label("__rt_stream_select_ts_us_only_x");
    emitter.instruction("mov rax, r8");                                          // microseconds
    emitter.instruction("xor rdx, rdx");                                         // clear rdx for div
    emitter.instruction("mov r9, 1000");                                         // millisecond divisor
    emitter.instruction("div r9");                                              // microseconds / 1000 = ms
    emitter.instruction("mov QWORD PTR [rbp - 2112], rax");                      // store the timeout
    emitter.instruction("jmp __rt_stream_select_ts_done_x");                     // timeout computed
    emitter.label("__rt_stream_select_ts_pos_x");
    emitter.instruction("test rcx, rcx");                                        // negative seconds?
    emitter.instruction("js __rt_stream_select_ts_inf_x");                       // → infinite
    emitter.instruction("mov r9, 0x7fffffff");                                   // sentinel threshold
    emitter.instruction("cmp rcx, r9");                                          // seconds >= sentinel?
    emitter.instruction("jge __rt_stream_select_ts_inf_x");                      // treat as infinite
    emitter.instruction("imul r9, rcx, 1000");                                   // seconds * 1000 (in r9)
    emitter.instruction("mov rax, r8");                                          // microseconds
    emitter.instruction("xor rdx, rdx");                                         // clear rdx for div
    emitter.instruction("mov r10, 1000");                                        // divisor
    emitter.instruction("div r10");                                             // rax = microseconds / 1000
    emitter.instruction("add r9, rax");                                          // total timeout = seconds*1000 + us/1000
    emitter.instruction("mov QWORD PTR [rbp - 2112], r9");                       // store the timeout
    emitter.instruction("jmp __rt_stream_select_ts_done_x");                     // timeout computed
    emitter.label("__rt_stream_select_ts_inf_x");
    emitter.instruction("mov QWORD PTR [rbp - 2112], -1");                       // -1 = block forever
    emitter.label("__rt_stream_select_ts_done_x");
}