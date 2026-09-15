//! Purpose:
//! Emits the `__rt_implode`, `__rt_implode_loop` runtime helper assembly for implode.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - String helpers use PHP pointer/length pairs and target ABI return registers; heap-backed results must remain refcount-compatible.
//! - The destination is BOUNDED. `__rt_implode` used to compute `_concat_buf + _concat_off`
//!   once and stream every byte into it with no check, so a joined result one byte past the
//!   64 KiB scratch ran into the adjacent BSS globals: silent data corruption on the CLI
//!   (`strlen()` and the head/tail bytes still right, the middle wrong) and SIGSEGV in a
//!   `--web` worker, at exactly 65537 bytes and on any `--heap-size`, because `_concat_buf` is
//!   a fixed BSS array (issue #515). It now grows through `__rt_concat_grow`, the shape
//!   `__rt_stream_get_contents` already uses, so an oversized join moves into an owned heap
//!   block instead.
//! - A join cannot size its destination up front the way `__rt_str_replace` does: an element's
//!   rendered length is only known once `__rt_mixed_cast_string` has run. So the room check
//!   sits before each copy, against the length actually about to be written — which needs no
//!   prediction to be correct.
//! - TWO independent invariants live in the boxed-Mixed element path, and both are pinned by
//!   tests. Do not drop either when reworking this emitter:
//!   1. OWNERSHIP (#601): `__rt_mixed_cast_string` persists string payloads into a fresh heap
//!      block. Implode owns that block and releases it once the bytes are copied, via the
//!      per-iteration "owned mixed-cast slot" (`[sp, #48]` on AArch64, `[rbp - 72]` on x86_64).
//!      Pinned by `tests/codegen/runtime_gc/regressions.rs` (the `#601` implode tests) and by
//!      `test_implode_{arm64,x86_64}_releases_mixed_cast_string` below.
//!   2. CURSOR: the LIVE destination cursor is published as `_concat_off` before each nested
//!      cast, and the ABSOLUTE end offset is stamped on completion. Pinned by
//!      `callback_return_type_matrix_joins_like_php` in `tests/array_result_type_tests.rs` and
//!      by `test_implode_{arm64,x86_64}_publishes_live_cursor_across_mixed_cast` below.
//!      Both now branch on where the destination lives: a SCRATCH destination reserves what it
//!      has written against the cast's own scratch exactly as before, while a GROWN one holds
//!      no scratch at all and hands the whole buffer back to the cast by restoring the entry
//!      offset.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use super::concat_scratch::CONCAT_BUF_CAPACITY;

/// Destination headroom guaranteed before a nested `__rt_mixed_cast_string`.
///
/// An int/float element is FORMATTED into `_concat_buf` at `_concat_off` by `__rt_itoa` /
/// `__rt_ftoa` and only then copied, so the cast writes before implode gets to measure what it
/// produced. Growing first when this much room is missing keeps that write inside the
/// destination implode has already reserved (scratch case) or leaves the cast the whole
/// scratch buffer (grown case).
///
/// 64 bounds both producers with room to spare: `__rt_ftoa` formats through
/// `snprintf(dst, 48, "%.14G", …)`, so 47 bytes is its ceiling, and a 64-bit integer with a
/// sign is 20.
const MIXED_CAST_HEADROOM: usize = 64;

/// Emits the AArch64 "make room for `x15` more destination bytes" sequence.
///
/// Compares the bytes already written plus the pending write against the destination's
/// capacity and, when they no longer fit, moves the whole accumulation into a larger owned
/// block through `__rt_concat_grow`. The doubling keeps a long join from reallocating per
/// element; taking the exact requirement instead when it is larger keeps one oversized element
/// from needing a second growth of its own.
///
/// Reads `x15` (pending byte count) and `x9` (live cursor). Preserves `x1`, `x2`, `x9`, `x10`
/// and `x11` across the growth call, and relocates `x9` into the grown buffer. Every other
/// caller-saved register is clobbered, so callers reload what they still need — `x3` in
/// particular.
fn emit_implode_ensure_room_aarch64(emitter: &mut Emitter, site: &str) {
    let skip = format!("__rt_implode_room_ok_{}", site);
    emitter.instruction("ldr x13, [sp, #24]");                                  // destination base
    emitter.instruction("sub x14, x9, x13");                                    // bytes already written into it
    emitter.instruction("add x15, x14, x15");                                   // capacity this write would need
    emitter.instruction("ldr x12, [sp, #80]");                                  // current destination capacity
    emitter.instruction("cmp x15, x12");                                        // does the pending write still fit?
    emitter.instruction(&format!("b.ls {}", skip));                             // room already reserved: copy straight in
    emitter.instruction("lsl x12, x12, #1");                                    // double the destination capacity
    emitter.instruction("cmp x12, x15");                                        // is the doubled capacity already enough?
    emitter.instruction("csel x12, x12, x15, hi");                              // otherwise take exactly what this write needs
    emitter.instruction("str x12, [sp, #80]");                                  // save the grown capacity
    emitter.instruction("str x14, [sp, #56]");                                  // bytes to preserve, and the cursor offset to restore
    emitter.instruction("str x10, [sp, #64]");                                  // preserve array length across the growth call
    emitter.instruction("str x11, [sp, #72]");                                  // preserve loop index across the growth call
    emitter.instruction("str x1, [sp, #96]");                                   // preserve the pending source pointer
    emitter.instruction("str x2, [sp, #104]");                                  // preserve the pending source length
    emitter.instruction("mov x0, x13");                                         // current buffer
    emitter.instruction("mov x1, x14");                                         // bytes to preserve
    emitter.instruction("mov x2, x12");                                         // new capacity
    emitter.instruction("bl __rt_concat_grow");                                 // move the join into a larger owned block
    emitter.instruction("str x0, [sp, #24]");                                   // record the grown destination base
    emitter.instruction("ldr x14, [sp, #56]");                                  // bytes preserved
    emitter.instruction("add x9, x0, x14");                                     // cursor at the same offset inside the grown buffer
    emitter.instruction("ldr x10, [sp, #64]");                                  // restore array length
    emitter.instruction("ldr x11, [sp, #72]");                                  // restore loop index
    emitter.instruction("ldr x1, [sp, #96]");                                   // restore the pending source pointer
    emitter.instruction("ldr x2, [sp, #104]");                                  // restore the pending source length
    emitter.label(&skip);
}

/// Emits the AArch64 sequence that publishes `_concat_off` for whichever storage the join
/// currently occupies.
///
/// A SCRATCH destination reserves everything written so far against the shared buffer, which
/// is what stops a nested `__rt_ftoa` from formatting over bytes already joined. A GROWN
/// destination occupies no scratch at all, so it restores the offset the call started from and
/// leaves the whole buffer to the cast.
///
/// `x9` must hold the live cursor. Clobbers `x12`-`x15`.
fn emit_implode_publish_offset_aarch64(emitter: &mut Emitter, site: &str) {
    let grown = format!("__rt_implode_off_grown_{}", site);
    let store = format!("__rt_implode_off_store_{}", site);
    emitter.instruction("ldr x13, [sp, #24]");                                  // destination base
    crate::codegen_support::abi::emit_symbol_address(emitter, "x14", "_concat_buf");
    emitter.instruction("sub x15, x13, x14");                                   // candidate scratch offset of the destination
    emitter.instruction(&format!("mov x12, #{}", CONCAT_BUF_CAPACITY));         // concat scratch capacity in bytes
    emitter.instruction("cmp x15, x12");                                        // is the destination outside the scratch window (unsigned: heap wraps high)?
    emitter.instruction(&format!("b.hs {}", grown));                            // a grown join holds no scratch
    emitter.instruction("sub x14, x9, x14");                                    // absolute offset of the live destination cursor
    emitter.instruction(&format!("b {}", store));                               // publish the reservation the nested cast must not cross
    emitter.label(&grown);
    emitter.instruction("ldr x14, [sp, #88]");                                  // restore the offset this call started from
    emitter.label(&store);
    emitter.instruction("ldr x13, [sp, #32]");                                  // concat offset variable address
    emitter.instruction("str x14, [x13]");                                      // publish it
}

/// Emits the x86_64 "make room for `r9` more destination bytes" sequence.
///
/// The x86_64 mirror of [`emit_implode_ensure_room_aarch64`], with the same contract: reads the
/// pending byte count from `r9` and the live cursor from `r10`, preserves `r8`/`r9`/`r10`
/// across the growth call, and relocates `r10` into the grown buffer. The array length and loop
/// cursor already live in spill slots on this target, so they need no saving.
fn emit_implode_ensure_room_x86_64(emitter: &mut Emitter, site: &str) {
    let skip = format!("__rt_implode_room_ok_{}", site);
    let have_cap = format!("__rt_implode_room_cap_{}", site);
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // destination base
    emitter.instruction("mov rdx, r10");                                        // live cursor
    emitter.instruction("sub rdx, rcx");                                        // bytes already written into it
    emitter.instruction("mov r11, rdx");                                        // start from the bytes already written
    emitter.instruction("add r11, r9");                                         // capacity this write would need
    emitter.instruction("cmp r11, QWORD PTR [rbp - 80]");                       // does the pending write still fit?
    emitter.instruction(&format!("jbe {}", skip));                              // room already reserved: copy straight in
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // current destination capacity
    emitter.instruction("shl rax, 1");                                          // double it
    emitter.instruction("cmp rax, r11");                                        // is the doubled capacity already enough?
    emitter.instruction(&format!("ja {}", have_cap));                           // keep the doubled capacity
    emitter.instruction("mov rax, r11");                                        // otherwise take exactly what this write needs
    emitter.label(&have_cap);
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // save the grown capacity
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // stash the preserved byte count in the cursor slot
    emitter.instruction("mov QWORD PTR [rbp - 96], r8");                        // preserve the pending source pointer
    emitter.instruction("mov QWORD PTR [rbp - 104], r9");                       // preserve the pending source length
    emitter.instruction("mov rsi, rax");                                        // new capacity
    emitter.instruction("mov rdi, rdx");                                        // bytes to preserve
    emitter.instruction("mov rax, rcx");                                        // current buffer
    emitter.instruction("call __rt_concat_grow");                               // move the join into a larger owned block
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // record the grown destination base
    emitter.instruction("mov r10, rax");                                        // rebuild the cursor from the grown base
    emitter.instruction("add r10, QWORD PTR [rbp - 40]");                       // ... at the same offset the preserved bytes reach
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // restore the cursor slot
    emitter.instruction("mov r8, QWORD PTR [rbp - 96]");                        // restore the pending source pointer
    emitter.instruction("mov r9, QWORD PTR [rbp - 104]");                       // restore the pending source length
    emitter.label(&skip);
}

/// Emits the x86_64 sequence that publishes `_concat_off` for whichever storage the join
/// currently occupies. The x86_64 mirror of [`emit_implode_publish_offset_aarch64`].
///
/// `r10` must hold the live cursor. Clobbers `rcx`, `rdx`, `r8` and `r9`.
fn emit_implode_publish_offset_x86_64(emitter: &mut Emitter, site: &str) {
    let grown = format!("__rt_implode_off_grown_{}", site);
    let store = format!("__rt_implode_off_store_{}", site);
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // destination base
    crate::codegen_support::abi::emit_symbol_address(emitter, "r8", "_concat_buf");
    emitter.instruction("mov rdx, rcx");                                        // copy the base before converting it to a scratch offset
    emitter.instruction("sub rdx, r8");                                         // candidate scratch offset of the destination
    emitter.instruction(&format!("cmp rdx, {}", CONCAT_BUF_CAPACITY));          // is the destination outside the scratch window (unsigned: heap wraps high)?
    emitter.instruction(&format!("jae {}", grown));                             // a grown join holds no scratch
    emitter.instruction("mov r9, r10");                                         // copy the live cursor before converting it to an absolute offset
    emitter.instruction("sub r9, r8");                                          // absolute offset of the live destination cursor
    emitter.instruction(&format!("jmp {}", store));                             // publish the reservation the nested cast must not cross
    emitter.label(&grown);
    emitter.instruction("mov r9, QWORD PTR [rbp - 88]");                        // restore the offset this call started from
    emitter.label(&store);
    crate::codegen_support::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov QWORD PTR [r8], r9");                              // publish it
}

/// Emits the `__rt_implode` runtime helper for joining array elements with a glue string.
/// Dispatches to platform-specific implementations (x86_64 Linux vs ARM64).
///
/// Input registers (ARM64): x1=glue_ptr, x2=glue_len, x3=array_ptr
/// Output registers (ARM64): x1=result_ptr, x2=result_len
/// Input registers (x86_64 Linux): rdi=glue_ptr, rsi=glue_len, rdx=array_ptr
/// Output registers (x86_64 Linux): rax=result_ptr, rdx=result_len
///
/// The result is written into the shared concat buffer and _concat_off is updated
/// to reflect the bytes consumed by this call.
pub fn emit_implode(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_implode_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: implode ---");
    emitter.label_global("__rt_implode");

    // -- set up stack frame (128 bytes) --
    emitter.instruction("sub sp, sp, #128");                                    // allocate 128 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #112]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #112");                                   // establish new frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                    // save glue string ptr and length
    emitter.instruction("str x3, [sp, #16]");                                   // save array pointer

    // -- get concat_buf write position --
    crate::codegen_support::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current write offset
    emitter.instruction("str x8, [sp, #88]");                                   // remember the entry offset: a grown join restores it, holding no scratch
    crate::codegen_support::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // compute destination pointer
    emitter.instruction("str x9, [sp, #24]");                                   // save result start pointer
    emitter.instruction("str x6, [sp, #32]");                                   // save offset variable address
    // The destination starts as the scratch tail and grows out of it on demand, so the
    // capacity it is measured against is what remains of the 64 KiB buffer, not all of it.
    emitter.instruction(&format!("mov x12, #{}", CONCAT_BUF_CAPACITY));         // total concat scratch capacity in bytes
    emitter.instruction("subs x12, x12, x8");                                   // room left in the scratch buffer from the entry offset
    emitter.instruction("csel x12, x12, xzr, pl");                              // a full (or over-full) scratch starts this join at zero capacity
    emitter.instruction("str x12, [sp, #80]");                                  // save the destination capacity

    // -- load array length and initialize index --
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload array pointer
    emitter.instruction("ldr x10, [x3]");                                       // load array element count
    emitter.instruction("ldr x12, [x3, #-8]");                                  // load packed indexed-array metadata for element layout dispatch
    emitter.instruction("lsr x12, x12, #8");                                    // move the indexed-array value_type tag into the low bits
    emitter.instruction("and x12, x12, #0x7f");                                 // isolate the indexed-array element value_type tag
    emitter.instruction("str x12, [sp, #40]");                                  // preserve the value_type tag across mixed element casts
    emitter.instruction("mov x11, #0");                                         // initialize element index = 0

    // -- main loop: join elements with glue --
    emitter.label("__rt_implode_loop");
    emitter.instruction("cmp x11, x10");                                        // check if all elements processed
    emitter.instruction("b.ge __rt_implode_done");                              // if done, finalize result

    // -- insert glue before element (skip for first element) --
    emitter.instruction("cbz x11, __rt_implode_elem");                          // skip glue before first element
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload glue ptr and length
    emitter.instruction("mov x15, x2");                                         // the glue bytes about to be written
    emit_implode_ensure_room_aarch64(emitter, "glue");
    emitter.instruction("mov x12, x2");                                         // copy glue length as counter
    emitter.label("__rt_implode_glue");
    emitter.instruction("cbz x12, __rt_implode_elem");                          // if no glue bytes remain, copy element
    emitter.instruction("ldrb w13, [x1], #1");                                  // load glue byte, advance glue ptr
    emitter.instruction("strb w13, [x9], #1");                                  // store to dest, advance dest ptr
    emitter.instruction("sub x12, x12, #1");                                    // decrement glue byte counter
    emitter.instruction("b __rt_implode_glue");                                 // continue copying glue

    // -- copy current array element --
    emitter.label("__rt_implode_elem");
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload array pointer
    emitter.instruction("ldr x13, [sp, #40]");                                  // reload the indexed-array value_type tag for this element
    emitter.instruction("str xzr, [sp, #48]");                                  // clear the owned mixed-cast slot; borrowed element slots release nothing
    emitter.instruction("cmp x13, #7");                                         // are elements boxed Mixed cells?
    emitter.instruction("b.eq __rt_implode_mixed_elem");                        // mixed slots must be cast to string before copying
    emitter.instruction("lsl x12, x11, #4");                                    // compute byte offset: index * 16
    emitter.instruction("add x12, x3, x12");                                    // add to array base
    emitter.instruction("add x12, x12, #24");                                   // skip 24-byte array header
    emitter.instruction("ldr x1, [x12]");                                       // load element string pointer
    emitter.instruction("ldr x2, [x12, #8]");                                   // load element string length
    emitter.instruction("b __rt_implode_copy_value");                           // copy the loaded string payload into the result buffer

    emitter.label("__rt_implode_mixed_elem");
    // The cast FORMATS an int/float element straight into the scratch before implode can
    // measure it, so the room has to exist beforehand. Growing here is what keeps that write
    // inside the destination this call has reserved, or — once grown — leaves the cast the
    // whole scratch buffer.
    emitter.instruction(&format!("mov x15, #{}", MIXED_CAST_HEADROOM));         // headroom a nested int/float format can need
    emit_implode_ensure_room_aarch64(emitter, "cast");
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload the array pointer the growth call clobbered
    emitter.instruction("lsl x12, x11, #3");                                    // compute byte offset: index * 8 for Mixed pointer slots
    emitter.instruction("add x12, x3, x12");                                    // add the Mixed slot offset to the array base
    emitter.instruction("add x12, x12, #24");                                   // skip 24-byte array header
    emitter.instruction("ldr x0, [x12]");                                       // load the boxed Mixed element pointer
    emitter.instruction("str x9, [sp, #56]");                                   // save destination cursor across the mixed string cast
    emitter.instruction("str x10, [sp, #64]");                                  // save array length across the mixed string cast
    emitter.instruction("str x11, [sp, #72]");                                  // save loop index across the mixed string cast
    emitter.instruction("str x0, [sp, #96]");                                   // save the boxed element across the offset publish
    // Publish the LIVE destination cursor as `_concat_off` before the nested cast. `__rt_ftoa`
    // (and `__rt_itoa`) format into `_concat_buf` at `_concat_off`; leaving the offset parked at
    // the implode result START made them write over the glue and element bytes already copied.
    emit_implode_publish_offset_aarch64(emitter, "cast");
    emitter.instruction("ldr x0, [sp, #96]");                                   // reload the boxed element pointer for the cast
    emitter.instruction("bl __rt_mixed_cast_string");                           // cast the boxed Mixed element to a string payload
    emitter.instruction("ldr x9, [sp, #56]");                                   // restore destination cursor after the mixed string cast
    emitter.instruction("ldr x10, [sp, #64]");                                  // restore array length after the mixed string cast
    emitter.instruction("ldr x11, [sp, #72]");                                  // restore loop index after the mixed string cast
    // Record the cast result as this frame's owned temporary (#601). `__rt_mixed_cast_string`
    // only ALLOCATES for tag 1 (string), where it routes through `__rt_str_persist` and always
    // returns a fresh `__rt_heap_alloc` block that no other owner aliases. Tag 0/2 (int/float)
    // and tag 3 true format into the shared `_concat_buf` scratch, tag 3 false and null/objects
    // return a null pointer: `__rt_heap_free` ignores all of those by contract (AArch64 rejects
    // pointers outside `_heap_buf.._heap_buf+_heap_off`; x86_64 rejects payloads without the
    // heap magic marker, explicitly to let callers pass concat-buffer storage). The pointer is
    // captured BEFORE the copy loop, which post-increments x1 off the allocation base.
    emitter.instruction("str x1, [sp, #48]");                                   // record the persisted mixed-cast string to release once its bytes are copied

    // -- copy element bytes to output --
    emitter.label("__rt_implode_copy_value");
    emitter.instruction("mov x15, x2");                                         // the element bytes about to be written
    emit_implode_ensure_room_aarch64(emitter, "elem");
    emitter.instruction("mov x12, x2");                                         // copy element length as counter
    emitter.label("__rt_implode_copy");
    emitter.instruction("cbz x12, __rt_implode_next");                          // if no bytes remain, move to next element
    emitter.instruction("ldrb w13, [x1], #1");                                  // load element byte, advance src ptr
    emitter.instruction("strb w13, [x9], #1");                                  // store to dest, advance dest ptr
    emitter.instruction("sub x12, x12, #1");                                    // decrement byte counter
    emitter.instruction("b __rt_implode_copy");                                 // continue copying element

    // -- advance to next element (releasing any persisted mixed-cast string first) --
    emitter.label("__rt_implode_next");
    emitter.instruction("ldr x0, [sp, #48]");                                   // load the owned mixed-cast string, or zero for borrowed element slots
    emitter.instruction("cbz x0, __rt_implode_next_advance");                   // borrowed element slots own no temporary to release
    emitter.instruction("str x9, [sp, #56]");                                   // preserve destination cursor across the heap release
    emitter.instruction("str x10, [sp, #64]");                                  // preserve array length across the heap release
    emitter.instruction("str x11, [sp, #72]");                                  // preserve loop index across the heap release
    emitter.instruction("bl __rt_heap_free");                                   // release the persisted string produced by __rt_mixed_cast_string
    emitter.instruction("ldr x9, [sp, #56]");                                   // restore destination cursor after the heap release
    emitter.instruction("ldr x10, [sp, #64]");                                  // restore array length after the heap release
    emitter.instruction("ldr x11, [sp, #72]");                                  // restore loop index after the heap release
    emitter.label("__rt_implode_next_advance");
    emitter.instruction("add x11, x11, #1");                                    // increment element index
    emitter.instruction("b __rt_implode_loop");                                 // process next element

    // -- finalize: compute result length and update concat_off --
    emitter.label("__rt_implode_done");
    emitter.instruction("ldr x1, [sp, #24]");                                   // load result start pointer
    emitter.instruction("sub x2, x9, x1");                                      // result length = dest_end - dest_start
    // Stamp the ABSOLUTE end offset rather than adding the length to whatever `_concat_off`
    // currently holds: a nested mixed cast may have advanced it past its own scratch, and that
    // scratch is inside the region this call just overwrote with the joined result. A GROWN
    // result is not in the scratch at all, so it restores the offset this call started from —
    // the same rule `__rt_concat_publish` applies, decided by the destination's own address.
    emit_implode_publish_offset_aarch64(emitter, "done");

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the x86_64 Linux variant of `__rt_implode`.
/// Implements the same join logic as the ARM64 path but uses x86_64 System V ABI
/// registers and x86_64 assembly conventions (r8-r11, rdx for length, rax for pointer).
///
/// Input registers: rdi=glue_ptr, rsi=glue_len, rdx=array_ptr
/// Output registers: rax=result_ptr, rdx=result_len
///
/// The result is written into the shared concat buffer and _concat_off is updated.
fn emit_implode_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: implode ---");
    emitter.label_global("__rt_implode");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving implode spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for glue, array, and concat-buffer bookkeeping
    emitter.instruction("sub rsp, 112");                                        // reserve aligned spill slots for glue, array, concat destination, array length, loop index, the owned mixed-cast string, and the destination bound
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the glue string pointer across the indexed-array copy loop and concat-buffer bookkeeping
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the glue string length across the indexed-array copy loop and concat-buffer bookkeeping
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the indexed-array pointer across the element copy loop and concat-buffer bookkeeping
    crate::codegen_support::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the current concat-buffer write offset before materializing the implode output start pointer
    emitter.instruction("mov QWORD PTR [rbp - 88], r9");                        // remember the entry offset: a grown join restores it, holding no scratch
    crate::codegen_support::abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("lea r10, [r10 + r9]");                                 // compute the current concat-buffer destination pointer for the implode output
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // preserve the implode result start pointer so the final string result can reference the copied bytes
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // preserve the current concat-buffer destination cursor across glue and element copy loops
    // The destination starts as the scratch tail and grows out of it on demand, so the
    // capacity it is measured against is what remains of the 64 KiB buffer, not all of it.
    emitter.instruction(&format!("mov rcx, {}", CONCAT_BUF_CAPACITY));          // total concat scratch capacity in bytes
    emitter.instruction("sub rcx, r9");                                         // room left in the scratch buffer from the entry offset
    emitter.instruction("jns __rt_implode_capacity_ready");                     // a full (or over-full) scratch starts this join at zero capacity
    emitter.instruction("xor ecx, ecx");                                        // clamp the remaining capacity at zero
    emitter.label("__rt_implode_capacity_ready");
    emitter.instruction("mov QWORD PTR [rbp - 80], rcx");                       // save the destination capacity
    emitter.instruction("mov r11, QWORD PTR [rdx]");                            // load the indexed-array logical length once before entering the implode loop
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // preserve the indexed-array logical length for the loop termination check
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // initialize the indexed-array loop cursor to the first element
    emitter.instruction("mov r11, QWORD PTR [rdx - 8]");                        // load packed indexed-array metadata for element layout dispatch
    emitter.instruction("shr r11, 8");                                          // move the indexed-array value_type tag into the low bits
    emitter.instruction("and r11, 0x7f");                                       // isolate the indexed-array element value_type tag
    emitter.instruction("mov QWORD PTR [rbp - 64], r11");                       // preserve the value_type tag across mixed element casts

    emitter.label("__rt_implode_loop");
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the current indexed-array loop cursor before deciding whether implode is complete
    emitter.instruction("cmp r11, QWORD PTR [rbp - 48]");                       // compare the current indexed-array loop cursor against the saved logical length
    emitter.instruction("jae __rt_implode_done");                               // stop once every indexed-array element has been copied into the concat buffer
    emitter.instruction("test r11, r11");                                       // check whether the current element is the first one in the indexed array
    emitter.instruction("jz __rt_implode_elem");                                // skip glue emission before copying the first indexed-array element
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the glue string pointer before copying the separator bytes
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the glue string length before copying the separator bytes
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the current concat-buffer destination cursor before copying the separator bytes
    emit_implode_ensure_room_x86_64(emitter, "glue");

    emitter.label("__rt_implode_glue");
    emitter.instruction("test r9, r9");                                         // check whether every glue byte has already been copied into the concat buffer
    emitter.instruction("jz __rt_implode_glue_done");                           // continue with the array element once the glue string has been fully copied
    emitter.instruction("mov r11b, BYTE PTR [r8]");                             // load one byte from the glue string before advancing the source pointer
    emitter.instruction("mov BYTE PTR [r10], r11b");                            // store one separator byte into the concat buffer before advancing the destination pointer
    emitter.instruction("add r8, 1");                                           // advance the glue string source pointer after copying one separator byte
    emitter.instruction("add r10, 1");                                          // advance the concat-buffer destination pointer after storing one separator byte
    emitter.instruction("sub r9, 1");                                           // decrement the remaining glue byte count after copying one separator byte
    emitter.instruction("jmp __rt_implode_glue");                               // continue copying separator bytes until the glue string is exhausted

    emitter.label("__rt_implode_glue_done");
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // preserve the concat-buffer destination cursor after copying the separator bytes

    emitter.label("__rt_implode_elem");
    emitter.instruction("mov QWORD PTR [rbp - 72], 0");                         // clear the owned mixed-cast slot; borrowed element slots release nothing
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the current indexed-array loop cursor before locating the next string element slot
    emitter.instruction("cmp QWORD PTR [rbp - 64], 7");                         // are elements boxed Mixed cells?
    emitter.instruction("je __rt_implode_mixed_elem");                          // mixed slots must be cast to string before copying
    emitter.instruction("mov rcx, r11");                                        // copy the indexed-array loop cursor before scaling it into a string-slot byte offset
    emitter.instruction("shl rcx, 4");                                          // convert the indexed-array loop cursor into the 16-byte offset of the current string slot
    emitter.instruction("mov r8, QWORD PTR [rbp - 24]");                        // reload the indexed-array pointer before addressing the current string slot
    emitter.instruction("lea rcx, [r8 + rcx + 24]");                            // compute the address of the current indexed-array string slot after the fixed array header
    emitter.instruction("mov r8, QWORD PTR [rcx]");                             // load the current indexed-array string pointer before copying the element bytes
    emitter.instruction("mov r9, QWORD PTR [rcx + 8]");                         // load the current indexed-array string length before copying the element bytes
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the current concat-buffer destination cursor before copying the element bytes
    emitter.instruction("jmp __rt_implode_copy_value");                         // copy the loaded string payload into the result buffer

    emitter.label("__rt_implode_mixed_elem");
    // The cast FORMATS an int/float element straight into the scratch before implode can
    // measure it, so the room has to exist beforehand. Growing here is what keeps that write
    // inside the destination this call has reserved, or — once grown — leaves the cast the
    // whole scratch buffer.
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // live destination cursor for the headroom check
    emitter.instruction("xor r8d, r8d");                                        // no pending source to preserve across the headroom growth
    emitter.instruction("mov r9, 64");                                          // headroom a nested int/float format can need
    emit_implode_ensure_room_x86_64(emitter, "cast");
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the loop cursor the growth call clobbered
    emitter.instruction("mov rcx, r11");                                        // copy the indexed-array loop cursor before scaling it to a Mixed slot offset
    emitter.instruction("shl rcx, 3");                                          // convert the indexed-array loop cursor into the 8-byte Mixed slot offset
    emitter.instruction("mov r8, QWORD PTR [rbp - 24]");                        // reload the indexed-array pointer before addressing the current Mixed slot
    emitter.instruction("lea rcx, [r8 + rcx + 24]");                            // compute the address of the current indexed-array Mixed slot
    emitter.instruction("mov rax, QWORD PTR [rcx]");                            // load the boxed Mixed element pointer for string casting
    // Publish the LIVE destination cursor as `_concat_off` before the nested cast. `__rt_ftoa`
    // (and `__rt_itoa`) format into `_concat_buf` at `_concat_off`; leaving the offset parked at
    // the implode result START made them write over the glue and element bytes already copied.
    emitter.instruction("mov QWORD PTR [rbp - 96], rax");                       // save the boxed element across the offset publish
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the live implode destination cursor
    emit_implode_publish_offset_x86_64(emitter, "cast");
    emitter.instruction("mov rax, QWORD PTR [rbp - 96]");                       // reload the boxed element pointer for the cast
    emitter.instruction("call __rt_mixed_cast_string");                         // cast the boxed Mixed element to a string payload
    // Record the cast result as this frame's owned temporary (#601): only tag 1 (string) routes
    // through `__rt_str_persist` and allocates. Int/float/bool scratch pointers into `_concat_buf`
    // are rejected by `__rt_heap_free`'s heap-magic check, which exists for exactly this caller.
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // record the persisted mixed-cast string to release once its bytes are copied
    emitter.instruction("mov r8, rax");                                         // move the cast string pointer into the copy-loop source register
    emitter.instruction("mov r9, rdx");                                         // move the cast string length into the copy-loop counter register
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the current concat-buffer destination cursor after casting

    emitter.label("__rt_implode_copy_value");
    emit_implode_ensure_room_x86_64(emitter, "elem");

    emitter.label("__rt_implode_copy");
    emitter.instruction("test r9, r9");                                         // check whether every element byte has already been copied into the concat buffer
    emitter.instruction("jz __rt_implode_next");                                // advance to the next indexed-array element once the current string is fully copied
    emitter.instruction("mov r11b, BYTE PTR [r8]");                             // load one byte from the current indexed-array string before advancing the source pointer
    emitter.instruction("mov BYTE PTR [r10], r11b");                            // store one byte from the current indexed-array string into the concat buffer
    emitter.instruction("add r8, 1");                                           // advance the current indexed-array string source pointer after copying one byte
    emitter.instruction("add r10, 1");                                          // advance the concat-buffer destination pointer after storing one byte
    emitter.instruction("sub r9, 1");                                           // decrement the remaining current string byte count after copying one byte
    emitter.instruction("jmp __rt_implode_copy");                               // continue copying bytes from the current indexed-array string until it is exhausted

    emitter.label("__rt_implode_next");
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // preserve the concat-buffer destination cursor after copying the current indexed-array element
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // load the owned mixed-cast string, or zero for borrowed element slots
    emitter.instruction("test rax, rax");                                       // borrowed element slots own no temporary to release
    emitter.instruction("jz __rt_implode_next_advance");                        // skip the release when the current element was copied from borrowed storage
    emitter.instruction("call __rt_heap_free");                                 // release the persisted string produced by __rt_mixed_cast_string
    emitter.label("__rt_implode_next_advance");
    emitter.instruction("add QWORD PTR [rbp - 56], 1");                         // advance the indexed-array loop cursor to the next element
    emitter.instruction("jmp __rt_implode_loop");                               // continue joining indexed-array elements into the concat buffer

    emitter.label("__rt_implode_done");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the final concat-buffer destination cursor to compute the joined string length
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the implode result start pointer before computing the joined string length
    emitter.instruction("mov rdx, r10");                                        // copy the final concat-buffer destination cursor before subtracting the result start pointer
    emitter.instruction("sub rdx, rax");                                        // compute the joined string length as dest_end - dest_start
    // Stamp the ABSOLUTE end offset rather than adding the length to whatever `_concat_off`
    // currently holds: a nested mixed cast may have advanced it past its own scratch, and that
    // scratch is inside the region this call just overwrote with the joined result. A GROWN
    // result is not in the scratch at all, so it restores the offset this call started from —
    // the same rule `__rt_concat_publish` applies, decided by the destination's own address.
    emitter.instruction("mov QWORD PTR [rbp - 96], rax");                       // save the result pointer across the offset publish
    emitter.instruction("mov QWORD PTR [rbp - 104], rdx");                      // save the result length across the offset publish
    emit_implode_publish_offset_x86_64(emitter, "done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 96]");                       // restore the result pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 104]");                      // restore the result length
    emitter.instruction("add rsp, 112");                                        // release the implode spill slots before returning the joined string
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the joined string
    emitter.instruction("ret");                                                 // return the joined string in the standard x86_64 string result registers
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Arch, Platform, Target};

    /// Verifies the ARM64 implode emitter releases the persisted mixed-cast string:
    /// it records the cast pointer, guards borrowed slots, and frees owned temporaries
    /// with a balanced 128-byte frame.
    #[test]
    fn test_implode_arm64_releases_mixed_cast_string() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_implode(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("__rt_implode:\n"));
        assert!(asm.contains("str x1, [sp, #48]"));
        assert!(asm.contains("cbz x0, __rt_implode_next_advance"));
        assert!(asm.contains("bl __rt_heap_free"));
        assert!(asm.contains("__rt_implode_next_advance:\n"));
        assert_eq!(asm.matches("sub sp, sp, #128").count(), 1);
        assert_eq!(asm.matches("add sp, sp, #128").count(), 1);
    }

    /// Verifies the x86_64 implode emitter releases the persisted mixed-cast string:
    /// it records the cast pointer in the owned slot, guards borrowed slots, and frees
    /// owned temporaries with a balanced 112-byte frame.
    #[test]
    fn test_implode_x86_64_releases_mixed_cast_string() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_implode(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("__rt_implode:\n"));
        assert!(asm.contains("mov QWORD PTR [rbp - 72], rax"));
        assert!(asm.contains("jz __rt_implode_next_advance"));
        assert!(asm.contains("call __rt_heap_free"));
        assert!(asm.contains("__rt_implode_next_advance:\n"));
        assert_eq!(asm.matches("sub rsp, 112").count(), 1);
        assert_eq!(asm.matches("add rsp, 112").count(), 1);
    }

    /// The owned mixed-cast slot must be cleared at the TOP of every element iteration, so a
    /// borrowed (typed-array) element can never inherit the previous iteration's owned pointer
    /// and free it a second time. This is the guard that keeps the release exactly-once.
    #[test]
    fn test_implode_clears_owned_slot_before_each_element() {
        let mut arm = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_implode(&mut arm);
        let arm_asm = arm.output();
        assert_eq!(arm_asm.matches("str xzr, [sp, #48]").count(), 1);
        assert_eq!(arm_asm.matches("str x1, [sp, #48]").count(), 1);

        let mut x86 = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_implode(&mut x86);
        let x86_asm = x86.output();
        assert_eq!(x86_asm.matches("mov QWORD PTR [rbp - 72], 0").count(), 1);
        assert_eq!(x86_asm.matches("mov QWORD PTR [rbp - 72], rax").count(), 1);
    }

    /// Pins the ARM64 cursor discipline: the LIVE destination cursor is published as
    /// `_concat_off` before the nested cast (so `__rt_ftoa`/`__rt_itoa` format past the bytes
    /// already joined), and the finalizer stamps the ABSOLUTE end offset instead of adding the
    /// result length to whatever `_concat_off` the nested cast left behind.
    ///
    /// Both sites now branch on where the destination lives. A scratch destination publishes
    /// the live cursor as before; a GROWN one occupies no scratch at all and restores the
    /// offset the call started from, which is also what leaves the nested cast the whole
    /// buffer to format into.
    #[test]
    fn test_implode_arm64_publishes_live_cursor_across_mixed_cast() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_implode(&mut emitter);
        let asm = emitter.output();
        assert_eq!(
            asm.matches("sub x14, x9, x14").count(),
            2,
            "the live cursor offset is published at the cast and at the finalizer"
        );
        assert_eq!(
            asm.matches("ldr x14, [sp, #88]").count(),
            2,
            "and both fall back to the entry offset once the destination has grown"
        );
        assert_eq!(asm.matches("str x14, [x13]").count(), 2);
        assert!(
            !asm.contains("add x8, x8, x2"),
            "implode must stamp the absolute concat offset, not accumulate a relative length"
        );
    }

    /// Issue #515: every destination write is preceded by a room check, so a join larger than
    /// the 64 KiB scratch grows into an owned heap block instead of running into the adjacent
    /// BSS globals.
    ///
    /// Three sites, and all three are load-bearing: the glue, the element, and the headroom a
    /// nested `__rt_mixed_cast_string` needs *before* it formats an int or float straight into
    /// the scratch — that one writes before implode can measure what it produced, so it cannot
    /// be checked afterwards like the other two.
    ///
    /// Asserted per target because the bug was invisible on one of them: the CLI corrupted the
    /// result silently while a `--web` worker took SIGSEGV for the same input.
    #[test]
    fn test_implode_bounds_every_destination_write() {
        for (target, grow, headroom, reload_base) in [
            (
                Target::new(Platform::MacOS, Arch::AArch64),
                "bl __rt_concat_grow",
                "mov x15, #64",
                "ldr x13, [sp, #24]",
            ),
            (
                Target::new(Platform::Linux, Arch::X86_64),
                "call __rt_concat_grow",
                "mov r9, 64",
                "mov rcx, QWORD PTR [rbp - 32]",
            ),
        ] {
            let mut emitter = Emitter::new(target);
            emit_implode(&mut emitter);
            let asm = emitter.output();
            assert_eq!(
                asm.matches(grow).count(),
                3,
                "glue, element and mixed-cast headroom must each be able to grow on {:?}",
                target.arch
            );
            assert!(
                asm.contains(headroom),
                "the mixed cast needs its headroom reserved on {:?}",
                target.arch
            );
            assert!(
                asm.matches(reload_base).count() >= 3,
                "each check must re-read the destination base, which growth relocates, on {:?}",
                target.arch
            );
        }
    }

    /// Pins the x86_64 cursor discipline, mirroring the ARM64 case: the publish reads the live
    /// cursor out of the destination spill slot, and the finalizer converts the final cursor to
    /// an absolute offset rather than adding the joined length to the stored offset.
    ///
    /// Both sites branch on where the destination lives, exactly as on ARM64: the live cursor
    /// for a scratch destination, the entry offset for a grown one.
    #[test]
    fn test_implode_x86_64_publishes_live_cursor_across_mixed_cast() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_implode(&mut emitter);
        let asm = emitter.output();
        assert_eq!(
            asm.matches("mov r9, r10").count(),
            2,
            "the live cursor is published at the cast and at the finalizer"
        );
        assert_eq!(
            asm.matches("sub r9, r8").count(),
            2,
            "and converted to an absolute offset at both"
        );
        assert_eq!(
            asm.matches("mov r9, QWORD PTR [rbp - 88]").count(),
            2,
            "both fall back to the entry offset once the destination has grown"
        );
        assert!(
            !asm.contains("add r9, rdx"),
            "implode must stamp the absolute concat offset, not accumulate a relative length"
        );
    }
}
