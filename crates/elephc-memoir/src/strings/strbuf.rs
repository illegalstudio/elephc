//! Purpose:
//! Emits growable output buffers with checked capacity and explicit storage ownership.
//!
//! Called from:
//! - Compiler runtime orchestration for JSON, serialization and other string producers.
//!
//! Key details:
//! - A 32-byte caller-owned descriptor tracks start, cursor, capacity end and opening scratch offset.
//! - Buffers start in configured string scratch storage and relocate into transient heap blocks when needed.
//! - Helpers republish cursors after growth and release only owned transient child results.
//! - Nested producers synchronize scratch reservations; abandoned heap-backed buffers can be released.

use crate::abi;
use crate::Emitter;
use crate::Arch;
use crate::layout::{x86_64_heap_kind_word, CONCAT_TEMP_HEAP_KIND};


/// Byte offset of the buffer start inside a strbuf descriptor.
pub const STRBUF_START: usize = 0;
/// Byte offset of the write cursor inside a strbuf descriptor.
pub const STRBUF_WRITE: usize = 8;
/// Byte offset of the capacity end inside a strbuf descriptor.
pub const STRBUF_END: usize = 16;
/// Byte offset of the opening `_concat_off` inside a strbuf descriptor.
pub const STRBUF_OFF0: usize = 24;
/// Total byte size of a strbuf descriptor.
pub const STRBUF_SIZE: usize = 32;

/// Emits `__rt_strbuf_init`, `__rt_strbuf_ensure`, `__rt_strbuf_sync`, `__rt_strbuf_finish`,
/// `__rt_strbuf_finish_owned`, `__rt_strbuf_append`, `__rt_strbuf_release`, and `__rt_strbuf_reserve_temp`.
///
/// Register contract (AArch64 / x86_64):
/// - `init(desc)`: `x0`/`rdi` = descriptor. Opens the buffer at the scratch tail. Returns the
///   write cursor in `x11`/`r11`. Leaf.
/// - `ensure(desc, need)`: `x0`/`rdi` = descriptor, `x1`/`rsi` = bytes about to be written.
///   Grows the buffer when fewer than `need` bytes remain. Returns the (possibly relocated)
///   cursor in `x11`/`r11` and the descriptor in `x0`/`rdi`. Clobbers caller-saved registers.
/// - `sync(desc)`: points `_concat_off` where nested scratch writers may format safely.
///   Returns the cursor in `x11`/`r11`. Leaf.
/// - `finish(desc)`: syncs `_concat_off` and returns the result in `x1`/`x2` (`rax`/`rdx`). Leaf.
/// - `finish_owned(desc)`: like `finish`, but a grown heap result is restamped as an owned string
///   (kind 1) so a caller that treats the result as owned frees or copies it exactly once.
/// - `release(desc)`: `x0`/`rdi` = descriptor. Frees the grown heap block of a buffer that will
///   never be finished (its producer was unwound by a throw); a scratch-backed buffer is a no-op.
/// - `append(desc, src, len, extra)`: `x0..x3` / `rdi`, `rsi`, `rdx`, `rcx`. Copies `len` bytes,
///   leaving at least `extra` more bytes of room, and releases `src` when it is a transient
///   (`CONCAT_TEMP_HEAP_KIND`) heap block. Returns the cursor in `x11`/`r11`.
/// - `reserve_temp(need)`: `x0`/`rax` = bytes. Like `__rt_concat_reserve`, but an oversized
///   request gets a transient heap block a parent can release or `__rt_str_persist` can adopt.
pub fn emit_strbuf(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => emit_strbuf_aarch64(emitter),
        Arch::X86_64 => emit_strbuf_x86_64(emitter),
    }
}

/// Emits the AArch64 `_concat_off` publication shared by `sync` and `finish`.
///
/// Expects the descriptor in `x0` and leaves the cursor in `x11`. A buffer whose start lies in
/// the scratch window reserves everything up to its cursor; a heap buffer hands the scratch back
/// from the offset it was opened at.
fn emit_publish_offset_aarch64(emitter: &mut impl Emitter, suffix: &str) {
    let heap = format!("__rt_strbuf_{}_heap", suffix);
    let store = format!("__rt_strbuf_{}_store", suffix);
    emitter.instruction(&format!("ldr x11, [x0, #{}]", STRBUF_WRITE));          // load the live write cursor
    emitter.instruction(&format!("ldr x12, [x0, #{}]", STRBUF_START));          // load the buffer start to classify its storage
    abi::emit_symbol_address(emitter, "x10", "_concat_buf");
    emitter.instruction("sub x12, x12, x10");                                   // candidate scratch offset of the buffer start
    abi::emit_load_int_immediate(emitter, "x13", emitter.concat_capacity() as i64); // load the concat scratch capacity in bytes
    emitter.instruction("cmp x12, x13");                                        // does the buffer live outside the scratch window (unsigned: heap wraps high)?
    emitter.instruction(&format!("b.hs {}", heap));                             // a heap buffer holds no scratch
    emitter.instruction("sub x12, x11, x10");                                   // reserve the scratch up to the live cursor
    emitter.instruction(&format!("b {}", store));                               // publish that reservation
    emitter.label(&heap);
    emitter.instruction(&format!("ldr x12, [x0, #{}]", STRBUF_OFF0));           // hand the scratch back from the opening offset
    emitter.label(&store);
    abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("str x12, [x9]");                                       // publish the scratch offset nested writers start from
}

/// Emits the AArch64 strbuf helpers.
fn emit_strbuf_aarch64(emitter: &mut impl Emitter) {
    // -- init: open the buffer at the free scratch tail --
    emitter.blank();
    emitter.comment("--- runtime: strbuf_init ---");
    emitter.label_global("__rt_strbuf_init");
    abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load the current scratch offset
    emitter.instruction(&format!("str x10, [x0, #{}]", STRBUF_OFF0));           // remember where the buffer was opened
    abi::emit_symbol_address(emitter, "x12", "_concat_buf");
    abi::emit_load_int_immediate(emitter, "x13", emitter.concat_capacity() as i64); // load the concat scratch capacity in bytes
    emitter.instruction("add x13, x12, x13");                                   // the scratch window ends here
    emitter.instruction("add x11, x12, x10");                                   // the buffer starts at the scratch tail
    emitter.instruction(&format!("str x11, [x0, #{}]", STRBUF_START));          // record the buffer start
    emitter.instruction(&format!("str x11, [x0, #{}]", STRBUF_WRITE));          // the cursor starts at the buffer start
    emitter.instruction("cmp x11, x13");                                        // is the scratch already past its end?
    emitter.instruction("csel x13, x13, x11, ls");                              // an over-full scratch opens with zero capacity
    emitter.instruction(&format!("str x13, [x0, #{}]", STRBUF_END));            // record the capacity end
    emitter.instruction("ret");                                                 // return the cursor in x11

    // -- ensure: grow the buffer when the next write would cross its capacity --
    emitter.blank();
    emitter.comment("--- runtime: strbuf_ensure ---");
    emitter.label_global("__rt_strbuf_ensure");
    emitter.instruction(&format!("ldr x11, [x0, #{}]", STRBUF_WRITE));          // load the live write cursor
    emitter.instruction(&format!("ldr x10, [x0, #{}]", STRBUF_END));            // load the capacity end
    emitter.instruction("sub x10, x10, x11");                                   // bytes still free in the buffer
    emitter.instruction("cmp x1, x10");                                         // does the pending write still fit?
    emitter.instruction("b.hi __rt_strbuf_ensure_grow");                        // no: move the buffer into a larger heap block
    emitter.instruction("ret");                                                 // yes: the cursor is ready in x11
    emitter.label("__rt_strbuf_ensure_grow");
    emitter.instruction("sub sp, sp, #48");                                     // reserve slots for the descriptor, used bytes, and new capacity
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the grow frame
    emitter.instruction("str x0, [sp, #0]");                                    // keep the descriptor across the allocator
    emitter.instruction(&format!("ldr x9, [x0, #{}]", STRBUF_START));           // load the current buffer start
    emitter.instruction("sub x10, x11, x9");                                    // bytes already written
    emitter.instruction("str x10, [sp, #8]");                                   // keep the used byte count
    emitter.instruction("add x2, x10, x1");                                     // the capacity this write needs
    abi::emit_load_symbol_to_reg(emitter, "x12", "_heap_max", 0);
    emitter.instruction("cmp x2, x12");                                         // can the configured heap hold the result at all?
    emitter.instruction("b.hi __rt_strbuf_ensure_too_large");                   // no: PHP's fatal string-capacity error, exit 255
    emitter.instruction("lsl x2, x2, #1");                                      // double it so a long output grows a logarithmic number of times
    emitter.instruction("cmp x2, x12");                                         // did doubling overshoot the heap?
    emitter.instruction("csel x2, x12, x2, hi");                                // then ask for the whole heap, which still holds the result
    emitter.instruction("str x2, [sp, #16]");                                   // keep the new capacity
    emitter.instruction("mov x0, x9");                                          // current buffer
    emitter.instruction("mov x1, x10");                                         // bytes to preserve
    emitter.instruction("bl __rt_concat_grow");                                 // move the output into a larger transient heap block
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the descriptor
    emitter.instruction(&format!("str x0, [x9, #{}]", STRBUF_START));           // record the relocated buffer start
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the used byte count
    emitter.instruction("add x11, x0, x10");                                    // the cursor keeps its offset in the grown buffer
    emitter.instruction(&format!("str x11, [x9, #{}]", STRBUF_WRITE));          // record the relocated cursor
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the new capacity
    emitter.instruction("add x10, x0, x10");                                    // the grown buffer ends here
    emitter.instruction(&format!("str x10, [x9, #{}]", STRBUF_END));            // record the new capacity end
    emitter.instruction("mov x0, x9");                                          // return the descriptor in x0
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the grow frame
    emitter.instruction("ret");                                                 // return the relocated cursor in x11
    emitter.label("__rt_strbuf_ensure_too_large");
    emitter.instruction("b __rt_concat_overflow");                              // unconditional branch keeps the fatal trampoline cross-atom safe

    // -- sync: publish the scratch offset nested writers may use --
    emitter.blank();
    emitter.comment("--- runtime: strbuf_sync ---");
    emitter.label_global("__rt_strbuf_sync");
    emit_publish_offset_aarch64(emitter, "sync");
    emitter.instruction("ret");                                                 // return the cursor in x11

    // -- finish: publish the scratch offset and return the result slice --
    emitter.blank();
    emitter.comment("--- runtime: strbuf_finish ---");
    emitter.label_global("__rt_strbuf_finish");
    emit_publish_offset_aarch64(emitter, "finish");
    emitter.instruction(&format!("ldr x1, [x0, #{}]", STRBUF_START));           // result pointer = buffer start
    emitter.instruction("sub x2, x11, x1");                                     // result length = cursor - start
    emitter.instruction("ret");                                                 // return the finished slice in x1/x2

    // -- finish_owned: finish, and stamp a grown heap result as an owned string --
    emitter.blank();
    emitter.comment("--- runtime: strbuf_finish_owned ---");
    emitter.label_global("__rt_strbuf_finish_owned");
    emitter.instruction("mov x15, x30");                                        // keep the return address across the leaf finish call
    emitter.instruction("bl __rt_strbuf_finish");                               // publish the scratch offset and load the result slice
    emitter.instruction("mov x30, x15");                                        // restore the return address
    abi::emit_symbol_address(emitter, "x10", "_concat_buf");
    emitter.instruction("sub x10, x1, x10");                                    // candidate scratch offset of the result
    abi::emit_load_int_immediate(emitter, "x11", emitter.concat_capacity() as i64); // load the concat scratch capacity in bytes
    emitter.instruction("cmp x10, x11");                                        // does the result live in the scratch window?
    emitter.instruction("b.lo __rt_strbuf_finish_owned_done");                  // scratch results need no ownership stamp
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = owned elephc string
    emitter.instruction("str x9, [x1, #-8]");                                   // the caller now owns the block like any heap string result
    emitter.label("__rt_strbuf_finish_owned_done");
    emitter.instruction("ret");                                                 // return the slice in x1/x2

    // -- release: drop a buffer that will never be finished (a producer unwound by a throw) --
    emitter.blank();
    emitter.comment("--- runtime: strbuf_release ---");
    emitter.label_global("__rt_strbuf_release");
    emitter.instruction(&format!("ldr x9, [x0, #{}]", STRBUF_START));           // load the buffer start to classify its storage
    abi::emit_symbol_address(emitter, "x10", "_concat_buf");
    emitter.instruction("sub x10, x9, x10");                                    // candidate scratch offset of the buffer start
    abi::emit_load_int_immediate(emitter, "x11", emitter.concat_capacity() as i64); // load the concat scratch capacity in bytes
    emitter.instruction("cmp x10, x11");                                        // does the buffer live outside the scratch window?
    emitter.instruction("b.lo __rt_strbuf_release_done");                       // scratch bytes hold no owner to free
    emitter.instruction("mov x0, x9");                                          // the grown heap block is the only owner
    emitter.instruction("b __rt_heap_free");                                    // free it and return to the producer
    emitter.label("__rt_strbuf_release_done");
    emitter.instruction("ret");                                                 // nothing to release

    // -- append: copy a produced slice to the cursor, then release a transient source --
    emitter.blank();
    emitter.comment("--- runtime: strbuf_append ---");
    emitter.label_global("__rt_strbuf_append");
    emitter.instruction("sub sp, sp, #48");                                     // reserve slots for the descriptor and the source slice
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the append frame
    emitter.instruction("stp x0, x1, [sp, #0]");                                // keep the descriptor and source pointer
    emitter.instruction("str x2, [sp, #16]");                                   // keep the source length
    // -- a slice produced in place at the cursor is committed BEFORE any growth: growing copies
    //    only the bytes below the cursor, so it would drop (or free) bytes still above it --
    emitter.instruction(&format!("ldr x9, [x0, #{}]", STRBUF_WRITE));           // current cursor
    emitter.instruction("cmp x1, x9");                                          // was the slice written in place at the cursor?
    emitter.instruction("b.ne __rt_strbuf_append_separate");                    // no: grow for the copy, then copy it
    emitter.instruction("add x9, x9, x2");                                      // the in-place bytes become part of the output
    emitter.instruction(&format!("str x9, [x0, #{}]", STRBUF_WRITE));           // record the committed cursor
    emitter.instruction("mov x1, x3");                                          // only the caller's headroom is still needed
    emitter.instruction("bl __rt_strbuf_ensure");                               // grow for the headroom; the committed bytes move with the buffer
    emitter.instruction("b __rt_strbuf_append_done");                           // nothing to copy or release
    emitter.label("__rt_strbuf_append_separate");
    emitter.instruction("add x1, x2, x3");                                      // room for the copy plus the caller's headroom
    emitter.instruction("bl __rt_strbuf_ensure");                               // grow first so the copy stays inside the buffer
    emitter.instruction("ldp x0, x1, [sp, #0]");                                // reload the descriptor and source pointer
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the source length
    emitter.instruction("cmp x1, x11");                                         // was the slice produced in place at the cursor?
    emitter.instruction("b.eq __rt_strbuf_append_copied");                      // then its bytes are already where they belong
    emitter.instruction("mov x10, #0");                                         // byte-copy index
    emitter.label("__rt_strbuf_append_copy");
    emitter.instruction("cmp x10, x2");                                         // copied every source byte?
    emitter.instruction("b.hs __rt_strbuf_append_copied");                      // stop once the slice is in place
    emitter.instruction("ldrb w12, [x1, x10]");                                 // load the next source byte
    emitter.instruction("strb w12, [x11, x10]");                                // store it at the cursor (forward copy: the source never sits below it)
    emitter.instruction("add x10, x10, #1");                                    // advance the copy index
    emitter.instruction("b __rt_strbuf_append_copy");                           // copy the next byte
    emitter.label("__rt_strbuf_append_copied");
    emitter.instruction("add x11, x11, x2");                                    // advance the cursor past the slice
    emitter.instruction(&format!("str x11, [x0, #{}]", STRBUF_WRITE));          // record the advanced cursor
    // -- a grown child output is a transient block nobody else owns: release it now --
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the source pointer for the heap-kind probe
    emitter.instruction("bl __rt_heap_kind");                                   // classify the source storage
    emitter.instruction(&format!("cmp x0, #{}", CONCAT_TEMP_HEAP_KIND));        // is it a transient grown buffer?
    emitter.instruction("b.ne __rt_strbuf_append_done");                        // scratch, literals, and owned strings stay untouched
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the transient source block
    emitter.instruction("bl __rt_heap_free");                                   // release it now that its bytes are copied
    emitter.label("__rt_strbuf_append_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // return the descriptor in x0
    emitter.instruction(&format!("ldr x11, [x0, #{}]", STRBUF_WRITE));          // return the cursor in x11
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the append frame
    emitter.instruction("ret");                                                 // return to the producer

    // -- reserve_temp: exact reservation whose heap fallback is a transient block --
    emitter.blank();
    emitter.comment("--- runtime: strbuf_reserve_temp ---");
    emitter.label_global("__rt_strbuf_reserve_temp");
    abi::emit_load_symbol_to_reg(emitter, "x9", "_heap_max", 0);
    emitter.instruction("cmp x0, x9");                                          // can the configured heap hold the result at all?
    emitter.instruction("b.hi __rt_strbuf_reserve_temp_too_large");             // no: PHP's fatal string-capacity error, exit 255
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve the frame pointer and return address across the reservation
    emitter.instruction("mov x29, sp");                                         // establish the reservation frame
    emitter.instruction("bl __rt_concat_reserve");                              // scratch when it fits, an owned heap block otherwise
    abi::emit_symbol_address(emitter, "x9", "_concat_buf");
    emitter.instruction("sub x10, x0, x9");                                     // candidate scratch offset of the reservation
    abi::emit_load_int_immediate(emitter, "x11", emitter.concat_capacity() as i64); // load the concat scratch capacity in bytes
    emitter.instruction("cmp x10, x11");                                        // did the reservation land in the scratch window?
    emitter.instruction("b.lo __rt_strbuf_reserve_temp_done");                  // scratch reservations need no stamp
    emitter.instruction(&format!("mov x9, #{}", CONCAT_TEMP_HEAP_KIND));        // transient string kind
    emitter.instruction("str x9, [x0, #-8]");                                   // let a parent release it or str_persist adopt it
    emitter.label("__rt_strbuf_reserve_temp_done");
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the frame pointer and return address
    emitter.instruction("ret");                                                 // return the reservation in x0
    emitter.label("__rt_strbuf_reserve_temp_too_large");
    emitter.instruction("b __rt_concat_overflow");                              // unconditional branch keeps the fatal trampoline cross-atom safe
}

/// Emits the x86_64 `_concat_off` publication shared by `sync` and `finish`.
///
/// Expects the descriptor in `rdi` and leaves the cursor in `r11`. Clobbers `r8`-`r10`.
fn emit_publish_offset_x86_64(emitter: &mut impl Emitter, suffix: &str) {
    let heap = format!("__rt_strbuf_{}_heap_x86", suffix);
    let store = format!("__rt_strbuf_{}_store_x86", suffix);
    emitter.instruction(&format!("mov r11, QWORD PTR [rdi + {}]", STRBUF_WRITE)); // load the live write cursor
    emitter.instruction(&format!("mov r10, QWORD PTR [rdi + {}]", STRBUF_START)); // load the buffer start to classify its storage
    abi::emit_symbol_address(emitter, "r9", "_concat_buf");
    emitter.instruction("sub r10, r9");                                         // candidate scratch offset of the buffer start
    emitter.instruction(&format!("cmp r10, {}", emitter.concat_capacity()));    // does the buffer live outside the scratch window (unsigned: heap wraps high)?
    emitter.instruction(&format!("jae {}", heap));                              // a heap buffer holds no scratch
    emitter.instruction("mov r10, r11");                                        // copy the cursor before turning it into an offset
    emitter.instruction("sub r10, r9");                                         // reserve the scratch up to the live cursor
    emitter.instruction(&format!("jmp {}", store));                             // publish that reservation
    emitter.label(&heap);
    emitter.instruction(&format!("mov r10, QWORD PTR [rdi + {}]", STRBUF_OFF0)); // hand the scratch back from the opening offset
    emitter.label(&store);
    abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov QWORD PTR [r8], r10");                             // publish the scratch offset nested writers start from
}

/// Emits the x86_64 strbuf helpers.
fn emit_strbuf_x86_64(emitter: &mut impl Emitter) {
    // -- init: open the buffer at the free scratch tail --
    emitter.blank();
    emitter.comment("--- runtime: strbuf_init ---");
    emitter.label_global("__rt_strbuf_init");
    abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r8, QWORD PTR [r8]");                              // load the current scratch offset
    emitter.instruction(&format!("mov QWORD PTR [rdi + {}], r8", STRBUF_OFF0)); // remember where the buffer was opened
    abi::emit_symbol_address(emitter, "r9", "_concat_buf");
    emitter.instruction(&format!("lea r10, [r9 + {}]", emitter.concat_capacity())); // the scratch window ends here
    emitter.instruction("lea r11, [r9 + r8]");                                  // the buffer starts at the scratch tail
    emitter.instruction(&format!("mov QWORD PTR [rdi + {}], r11", STRBUF_START)); // record the buffer start
    emitter.instruction(&format!("mov QWORD PTR [rdi + {}], r11", STRBUF_WRITE)); // the cursor starts at the buffer start
    emitter.instruction("cmp r11, r10");                                        // is the scratch already past its end?
    emitter.instruction("cmova r10, r11");                                      // an over-full scratch opens with zero capacity
    emitter.instruction(&format!("mov QWORD PTR [rdi + {}], r10", STRBUF_END)); // record the capacity end
    emitter.instruction("ret");                                                 // return the cursor in r11

    // -- ensure: grow the buffer when the next write would cross its capacity --
    emitter.blank();
    emitter.comment("--- runtime: strbuf_ensure ---");
    emitter.label_global("__rt_strbuf_ensure");
    emitter.instruction(&format!("mov r11, QWORD PTR [rdi + {}]", STRBUF_WRITE)); // load the live write cursor
    emitter.instruction(&format!("mov r10, QWORD PTR [rdi + {}]", STRBUF_END)); // load the capacity end
    emitter.instruction("sub r10, r11");                                        // bytes still free in the buffer
    emitter.instruction("cmp rsi, r10");                                        // does the pending write still fit?
    emitter.instruction("ja __rt_strbuf_ensure_grow_x86");                      // no: move the buffer into a larger heap block
    emitter.instruction("ret");                                                 // yes: the cursor is ready in r11
    emitter.label("__rt_strbuf_ensure_grow_x86");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer and realign the stack
    emitter.instruction("mov rbp, rsp");                                        // establish the grow frame
    emitter.instruction("sub rsp, 32");                                         // reserve aligned slots for the descriptor, used bytes, and new capacity
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // keep the descriptor across the allocator
    emitter.instruction(&format!("mov rax, QWORD PTR [rdi + {}]", STRBUF_START)); // current buffer start
    emitter.instruction("mov r10, r11");                                        // copy the cursor before turning it into a byte count
    emitter.instruction("sub r10, rax");                                        // bytes already written
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // keep the used byte count
    emitter.instruction("add rsi, r10");                                        // the capacity this write needs
    abi::emit_load_symbol_to_reg(emitter, "r8", "_heap_max", 0);
    emitter.instruction("cmp rsi, r8");                                         // can the configured heap hold the result at all?
    emitter.instruction("ja __rt_concat_overflow");                             // no: PHP's fatal string-capacity error, exit 255
    emitter.instruction("add rsi, rsi");                                        // double it so a long output grows a logarithmic number of times
    emitter.instruction("cmp rsi, r8");                                         // did doubling overshoot the heap?
    emitter.instruction("cmova rsi, r8");                                       // then ask for the whole heap, which still holds the result
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // keep the new capacity
    emitter.instruction("mov rdi, r10");                                        // bytes to preserve (rax = current buffer, rsi = new capacity)
    emitter.instruction("call __rt_concat_grow");                               // move the output into a larger transient heap block
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the descriptor
    emitter.instruction(&format!("mov QWORD PTR [rdi + {}], rax", STRBUF_START)); // record the relocated buffer start
    emitter.instruction("mov r11, rax");                                        // start from the grown buffer
    emitter.instruction("add r11, QWORD PTR [rbp - 16]");                       // the cursor keeps its offset in the grown buffer
    emitter.instruction(&format!("mov QWORD PTR [rdi + {}], r11", STRBUF_WRITE)); // record the relocated cursor
    emitter.instruction("mov r10, rax");                                        // start from the grown buffer
    emitter.instruction("add r10, QWORD PTR [rbp - 24]");                       // the grown buffer ends here
    emitter.instruction(&format!("mov QWORD PTR [rdi + {}], r10", STRBUF_END)); // record the new capacity end
    emitter.instruction("add rsp, 32");                                         // release the grow slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the relocated cursor in r11

    // -- sync: publish the scratch offset nested writers may use --
    emitter.blank();
    emitter.comment("--- runtime: strbuf_sync ---");
    emitter.label_global("__rt_strbuf_sync");
    emit_publish_offset_x86_64(emitter, "sync");
    emitter.instruction("ret");                                                 // return the cursor in r11

    // -- finish: publish the scratch offset and return the result slice --
    emitter.blank();
    emitter.comment("--- runtime: strbuf_finish ---");
    emitter.label_global("__rt_strbuf_finish");
    emit_publish_offset_x86_64(emitter, "finish");
    emitter.instruction(&format!("mov rax, QWORD PTR [rdi + {}]", STRBUF_START)); // result pointer = buffer start
    emitter.instruction("mov rdx, r11");                                        // copy the cursor before turning it into a length
    emitter.instruction("sub rdx, rax");                                        // result length = cursor - start
    emitter.instruction("ret");                                                 // return the finished slice in rax/rdx

    // -- finish_owned: finish, and stamp a grown heap result as an owned string --
    emitter.blank();
    emitter.comment("--- runtime: strbuf_finish_owned ---");
    emitter.label_global("__rt_strbuf_finish_owned");
    emitter.instruction("call __rt_strbuf_finish");                             // publish the scratch offset and load the result slice
    abi::emit_symbol_address(emitter, "r9", "_concat_buf");
    emitter.instruction("mov r10, rax");                                        // copy the result pointer before turning it into an offset
    emitter.instruction("sub r10, r9");                                         // candidate scratch offset of the result
    emitter.instruction(&format!("cmp r10, {}", emitter.concat_capacity()));    // does the result live in the scratch window?
    emitter.instruction("jb __rt_strbuf_finish_owned_done_x86");                // scratch results need no ownership stamp
    emitter.instruction(&format!("mov r10, 0x{:x}", x86_64_heap_kind_word(1))); // owned-string heap kind with the x86_64 heap marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // the caller now owns the block like any heap string result
    emitter.label("__rt_strbuf_finish_owned_done_x86");
    emitter.instruction("ret");                                                 // return the slice in rax/rdx

    // -- release: drop a buffer that will never be finished (a producer unwound by a throw) --
    emitter.blank();
    emitter.comment("--- runtime: strbuf_release ---");
    emitter.label_global("__rt_strbuf_release");
    emitter.instruction(&format!("mov rax, QWORD PTR [rdi + {}]", STRBUF_START)); // load the buffer start to classify its storage
    abi::emit_symbol_address(emitter, "r9", "_concat_buf");
    emitter.instruction("mov r10, rax");                                        // copy the start before turning it into an offset
    emitter.instruction("sub r10, r9");                                         // candidate scratch offset of the buffer start
    emitter.instruction(&format!("cmp r10, {}", emitter.concat_capacity()));    // does the buffer live outside the scratch window?
    emitter.instruction("jb __rt_strbuf_release_done_x86");                     // scratch bytes hold no owner to free
    emitter.instruction("jmp __rt_heap_free");                                  // free the grown heap block (rax) and return to the producer
    emitter.label("__rt_strbuf_release_done_x86");
    emitter.instruction("ret");                                                 // nothing to release

    // -- append: copy a produced slice to the cursor, then release a transient source --
    emitter.blank();
    emitter.comment("--- runtime: strbuf_append ---");
    emitter.label_global("__rt_strbuf_append");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer and realign the stack
    emitter.instruction("mov rbp, rsp");                                        // establish the append frame
    emitter.instruction("sub rsp, 32");                                         // reserve aligned slots for the descriptor and the source slice
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // keep the descriptor
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // keep the source pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // keep the source length
    // -- see the AArch64 arm: commit an in-place slice before any growth --
    emitter.instruction(&format!("mov r9, QWORD PTR [rdi + {}]", STRBUF_WRITE)); // current cursor
    emitter.instruction("cmp rsi, r9");                                         // was the slice written in place at the cursor?
    emitter.instruction("jne __rt_strbuf_append_separate_x86");                 // no: grow for the copy, then copy it
    emitter.instruction("add r9, rdx");                                         // the in-place bytes become part of the output
    emitter.instruction(&format!("mov QWORD PTR [rdi + {}], r9", STRBUF_WRITE)); // record the committed cursor
    emitter.instruction("mov rsi, rcx");                                        // only the caller's headroom is still needed
    emitter.instruction("call __rt_strbuf_ensure");                             // grow for the headroom; the committed bytes move with the buffer
    emitter.instruction("jmp __rt_strbuf_append_done_x86");                     // nothing to copy or release
    emitter.label("__rt_strbuf_append_separate_x86");
    emitter.instruction("lea rsi, [rdx + rcx]");                                // room for the copy plus the caller's headroom
    emitter.instruction("call __rt_strbuf_ensure");                             // grow first so the copy stays inside the buffer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the source pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload the source length
    emitter.instruction("cmp rsi, r11");                                        // was the slice produced in place at the cursor?
    emitter.instruction("je __rt_strbuf_append_copied_x86");                    // then its bytes are already where they belong
    emitter.instruction("xor ecx, ecx");                                        // byte-copy index
    emitter.label("__rt_strbuf_append_copy_x86");
    emitter.instruction("cmp rcx, rdx");                                        // copied every source byte?
    emitter.instruction("jae __rt_strbuf_append_copied_x86");                   // stop once the slice is in place
    emitter.instruction("mov r8b, BYTE PTR [rsi + rcx]");                       // load the next source byte
    emitter.instruction("mov BYTE PTR [r11 + rcx], r8b");                       // store it at the cursor (forward copy: the source never sits below it)
    emitter.instruction("add rcx, 1");                                          // advance the copy index
    emitter.instruction("jmp __rt_strbuf_append_copy_x86");                     // copy the next byte
    emitter.label("__rt_strbuf_append_copied_x86");
    emitter.instruction("add r11, rdx");                                        // advance the cursor past the slice
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the descriptor
    emitter.instruction(&format!("mov QWORD PTR [rdi + {}], r11", STRBUF_WRITE)); // record the advanced cursor
    // -- a grown child output is a transient block nobody else owns: release it now --
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the source pointer for the heap-kind probe
    emitter.instruction("call __rt_heap_kind");                                 // classify the source storage
    emitter.instruction(&format!("cmp eax, {}", CONCAT_TEMP_HEAP_KIND));        // is it a transient grown buffer?
    emitter.instruction("jne __rt_strbuf_append_done_x86");                     // scratch, literals, and owned strings stay untouched
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the transient source block
    emitter.instruction("call __rt_heap_free");                                 // release it now that its bytes are copied
    emitter.label("__rt_strbuf_append_done_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // return the descriptor in rdi
    emitter.instruction(&format!("mov r11, QWORD PTR [rdi + {}]", STRBUF_WRITE)); // return the cursor in r11
    emitter.instruction("add rsp, 32");                                         // release the append slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the producer

    // -- reserve_temp: exact reservation whose heap fallback is a transient block --
    emitter.blank();
    emitter.comment("--- runtime: strbuf_reserve_temp ---");
    emitter.label_global("__rt_strbuf_reserve_temp");
    abi::emit_load_symbol_to_reg(emitter, "r8", "_heap_max", 0);
    emitter.instruction("cmp rax, r8");                                         // can the configured heap hold the result at all?
    emitter.instruction("ja __rt_concat_overflow");                             // no: PHP's fatal string-capacity error, exit 255
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer and realign the stack
    emitter.instruction("mov rbp, rsp");                                        // establish the reservation frame
    emitter.instruction("call __rt_concat_reserve");                            // scratch when it fits, an owned heap block otherwise
    abi::emit_symbol_address(emitter, "r8", "_concat_buf");
    emitter.instruction("mov r9, rax");                                         // copy the reservation before turning it into an offset
    emitter.instruction("sub r9, r8");                                          // candidate scratch offset of the reservation
    emitter.instruction(&format!("cmp r9, {}", emitter.concat_capacity()));     // did the reservation land in the scratch window?
    emitter.instruction("jb __rt_strbuf_reserve_temp_done_x86");                // scratch reservations need no stamp
    emitter.instruction(&format!("mov r10, 0x{:x}", x86_64_heap_kind_word(CONCAT_TEMP_HEAP_KIND))); // transient string kind with the x86_64 heap marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // let a parent release it or str_persist adopt it
    emitter.label("__rt_strbuf_reserve_temp_done_x86");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the reservation in rax
}

/// Bridges a producer's frame-local cursor to the growable string descriptor.
///
/// Slots are measured from sp on AArch64 and below rbp on x86-64.
#[derive(Clone, Copy)]
pub struct FramedStringBuffer {
    /// Base of the buffer descriptor.
    pub descriptor: usize,
    /// Existing cursor slot used by the producer's formatters.
    pub cursor: usize,
}

impl FramedStringBuffer {
    /// Opens an empty buffer and supplies its write pointer to the producer.
    pub fn emit_init(self, emitter: &mut impl Emitter) {
        self.emit_descriptor_address(emitter);
        abi::emit_call_label(emitter, "__rt_strbuf_init");
        self.emit_load_cursor(emitter);
    }

    /// Makes room for a fixed number of bytes and reloads the possibly relocated write pointer.
    pub fn emit_ensure_fixed(self, emitter: &mut impl Emitter, bytes: usize) {
        let need = abi::int_arg_reg_name(emitter, 1);
        abi::emit_load_int_immediate(emitter, need, bytes as i64);
        self.emit_ensure(emitter);
    }

    /// Makes room for the byte count in the second native argument register.
    pub fn emit_ensure(self, emitter: &mut impl Emitter) {
        self.emit_sync_cursor(emitter);
        abi::emit_call_label(emitter, "__rt_strbuf_ensure");
        self.emit_load_cursor(emitter);
    }

    /// Copies a source pair backed by immutable native data in argument registers two and three.
    pub fn emit_append_external(self, emitter: &mut impl Emitter) {
        self.emit_sync_cursor(emitter);
        let extra = abi::int_arg_reg_name(emitter, 3);
        abi::emit_load_int_immediate(emitter, extra, 0);
        abi::emit_call_label(emitter, "__rt_strbuf_append");
        self.emit_load_cursor(emitter);
    }

    /// Publishes the accumulated string through the requested ownership entry.
    pub fn emit_finish(self, emitter: &mut impl Emitter, owned: bool) {
        self.emit_sync_cursor(emitter);
        let finish = if owned { "__rt_strbuf_finish_owned" } else { "__rt_strbuf_finish" };
        abi::emit_call_label(emitter, finish);
    }

    /// Materializes the descriptor address in the first native argument register.
    fn emit_descriptor_address(self, emitter: &mut impl Emitter) {
        let Self { descriptor, .. } = self;
        match emitter.arch() {
            Arch::AArch64 => emitter.instruction(&format!("add x0, sp, #{descriptor}")), // pass the physical buffer descriptor
            Arch::X86_64 => emitter.instruction(&format!("lea rdi, [rbp - {descriptor}]")), // pass the physical buffer descriptor
        }
    }

    /// Copies the producer's current write pointer into the descriptor.
    fn emit_sync_cursor(self, emitter: &mut impl Emitter) {
        let Self { cursor, .. } = self;
        self.emit_descriptor_address(emitter);
        match emitter.arch() {
            Arch::AArch64 => {
                emitter.instruction(&format!("ldr x10, [sp, #{cursor}]"));      // reload the cursor advanced by the producer
                emitter.instruction(&format!("str x10, [x0, #{STRBUF_WRITE}]")); // publish it before the buffer operation
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {cursor}]")); // reload the cursor advanced by the producer
                emitter.instruction(&format!("mov QWORD PTR [rdi + {STRBUF_WRITE}], r10")); // publish it before the buffer operation
            }
        }
    }

    /// Updates the producer frame with the cursor returned by the buffer operation.
    fn emit_load_cursor(self, emitter: &mut impl Emitter) {
        let Self { cursor, .. } = self;
        match emitter.arch() {
            Arch::AArch64 => emitter.instruction(&format!("str x11, [sp, #{cursor}]")), // preserve the current write pointer
            Arch::X86_64 => emitter.instruction(&format!("mov QWORD PTR [rbp - {cursor}], r11")), // preserve the current write pointer
        }
    }
}
