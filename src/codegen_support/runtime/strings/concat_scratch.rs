//! Purpose:
//! Emits the shared `__rt_concat_reserve`, `__rt_concat_publish`, and `__rt_alloc_overflow`
//! runtime helpers that bound every append into the heap-sized `_concat_buf` result arena.
//! Runtime string/IO producers reserve their exact result size here instead of writing past
//! the scratch end into the adjacent BSS globals.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - `__rt_concat_reserve` returns scratch storage when the request still fits inside
//!   `_concat_buf`, and an owned heap block (kind word 1 stamped at `[ptr-8]`) otherwise.
//!   It never advances `_concat_off`; the caller publishes the *written* length afterwards.
//! - `__rt_concat_grow` marks its in-progress result with `CONCAT_TEMP_HEAP_KIND`, allowing
//!   `__rt_str_persist` to take over the buffer without allocating a leaking duplicate.
//! - `__rt_concat_publish` derives the storage class from the pointer itself: only pointers
//!   inside `[_concat_buf, _concat_buf + capacity)` move `_concat_off`, so heap-backed results
//!   leave the shared scratch offset untouched with no extra flag register.
//! - Requests larger than the configured heap capacity (`_heap_max`), including the wrapped
//!   or negative sizes produced by an overflowing size computation, reach
//!   `__rt_alloc_overflow`; an active cdylib boundary recovers with allocation
//!   status, while executables retain PHP's fatal behavior.
//! - `__rt_alloc_overflow` is a `.globl` fatal trampoline: callers must reach it with an
//!   UNCONDITIONAL branch from a local label so macOS atom splitting can never put a
//!   conditional branch out of range.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::sentinels::CONCAT_TEMP_HEAP_KIND;
use crate::codegen_support::runtime::data::{ALLOC_OVERFLOW_MSG, CONCAT_OVERFLOW_MSG};

/// Emits `__rt_concat_reserve`, `__rt_concat_publish`, `__rt_concat_grow`, and
/// `__rt_alloc_overflow`.
///
/// These helpers form the bounds-checked allocation front end for every runtime
/// producer that used to append blindly at `_concat_buf + _concat_off`.
///
/// # `__rt_concat_reserve`
/// - Input: `x0` (AArch64) / `rax` (x86_64) = required payload bytes, interpreted as unsigned.
/// - Output: `x0` / `rax` = destination pointer with room for at least that many bytes.
/// - Clobbers every caller-saved register: callers must spill their live state first.
/// - Fatals through `__rt_alloc_overflow` when the request exceeds `_heap_max`.
///
/// # `__rt_concat_publish`
/// - Input/output: `x1`/`x2` (AArch64) or `rax`/`rdx` (x86_64) = result pointer and length,
///   both preserved so the helper can be dropped straight into a string-returning epilogue.
/// - Advances `_concat_off` only for scratch-backed results; heap-backed results are a no-op.
///
/// # `__rt_concat_grow`
/// - Input: `x0`/`rax` = current buffer, `x1`/`rdi` = bytes to preserve, `x2`/`rsi` = new capacity.
/// - Output: `x0`/`rax` = transient heap string holding the preserved prefix; `__rt_str_persist`
///   can adopt it.
/// - Releases the superseded buffer through `__rt_heap_free_safe` (a no-op for scratch).
///
/// # `__rt_alloc_overflow`
/// - Recovers through an active cdylib boundary, otherwise writes PHP's
///   allocation-overflow fatal message and exits with status 1.
pub fn emit_concat_scratch(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_concat_scratch_linux_x86_64(emitter);
        emit_concat_headroom_linux_x86_64(emitter);
        emit_preg_cb_ensure_x86_64(emitter);
        return;
    }

    emit_concat_reserve_aarch64(emitter);
    emit_concat_publish_aarch64(emitter);
    emit_concat_grow_aarch64(emitter);
    emit_alloc_overflow_aarch64(emitter);
    emit_concat_headroom_aarch64(emitter);
    emit_preg_cb_ensure_aarch64(emitter);
}

/// Emits the AArch64 `__rt_concat_reserve` helper.
///
/// Picks scratch storage while `_concat_off + required` still fits the 64 KiB buffer and
/// falls back to an owned heap allocation otherwise. `_concat_off` is deliberately left
/// unchanged so a caller that writes fewer bytes than it reserved does not waste scratch.
fn emit_concat_reserve_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: concat_reserve ---");
    emitter.label_global("__rt_concat_reserve");

    // -- reject requests the allocator could never satisfy, including wrapped sizes --
    abi::emit_symbol_address(emitter, "x9", "_heap_max");
    emitter.instruction("ldr x9, [x9]");                                        // load the configured heap capacity as the upper bound for any single result
    emitter.instruction("cmp x0, x9");                                          // is the requested byte count impossible to satisfy (unsigned, so wrapped sizes are huge)?
    emitter.instruction("b.hi __rt_concat_reserve_too_large");                  // report a PHP-style allocation overflow instead of writing past any buffer

    // -- prefer the shared 64 KiB scratch buffer while the result still fits --
    abi::emit_symbol_address(emitter, "x10", "_concat_off");
    emitter.instruction("ldr x11, [x10]");                                      // load the current concat scratch write offset
    emitter.instruction("add x12, x11, x0");                                    // compute the scratch tail this request would reach
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "x13", emitter.concat_capacity as i64); // load the concat scratch capacity in bytes
    emitter.instruction("cmp x12, x13");                                        // does the reservation still fit inside the shared scratch buffer?
    emitter.instruction("b.hi __rt_concat_reserve_heap");                       // use the owned heap fallback when the scratch buffer would overflow
    abi::emit_symbol_address(emitter, "x14", "_concat_buf");
    emitter.instruction("add x0, x14, x11");                                    // return the scratch destination pointer at the current write offset
    emitter.instruction("ret");                                                 // return the scratch-backed reservation to the caller

    // -- heap fallback: oversized results get their own owned string allocation --
    emitter.label("__rt_concat_reserve_heap");
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve the frame pointer and return address across the allocator call
    emitter.instruction("mov x29, sp");                                         // establish the reservation helper frame pointer
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate owned storage large enough for the requested result
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = owned elephc string
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the heap allocation as a string payload
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the frame pointer and return address after the allocator call
    emitter.instruction("ret");                                                 // return the heap-backed reservation to the caller

    // -- impossible request: report PHP's allocation-overflow fatal error --
    emitter.label("__rt_concat_reserve_too_large");
    emitter.instruction("b __rt_alloc_overflow");                               // unconditional branch keeps the fatal trampoline cross-atom safe
}

/// Emits the AArch64 `__rt_concat_publish` helper.
///
/// Classifies the result by address instead of by a caller-supplied flag: only a pointer
/// inside `[_concat_buf, _concat_buf + capacity)` advances `_concat_off`. Heap-backed results
/// own their own storage and must not move the shared scratch cursor.
fn emit_concat_publish_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: concat_publish ---");
    emitter.label_global("__rt_concat_publish");

    abi::emit_symbol_address(emitter, "x9", "_concat_buf");
    emitter.instruction("sub x10, x1, x9");                                     // compute the candidate scratch offset of the finished result
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "x11", emitter.concat_capacity as i64); // load the concat scratch capacity in bytes
    emitter.instruction("cmp x10, x11");                                        // is the result outside the shared scratch window (unsigned, so heap pointers wrap high)?
    emitter.instruction("b.hs __rt_concat_publish_done");                       // heap-backed results leave the shared scratch offset untouched
    emitter.instruction("add x10, x10, x2");                                    // advance the scratch offset past the bytes this result actually wrote
    abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("str x10, [x9]");                                       // publish the updated concat scratch write offset

    emitter.label("__rt_concat_publish_done");
    emitter.instruction("ret");                                                 // return with the result pointer/length pair untouched
}

/// Emits the AArch64 `__rt_concat_grow` helper.
///
/// Moves an in-progress reservation into a larger owned heap block, preserving the bytes
/// written so far. Incremental producers (`stream_get_contents`) call it when the next chunk
/// no longer fits the current reservation. The old block is released through
/// `__rt_heap_free_safe`, which skips concat-scratch and other non-heap pointers.
///
/// - Input: `x0` = current buffer, `x1` = bytes to preserve, `x2` = new capacity.
/// - Output: `x0` = new buffer.
/// - Clobbers every caller-saved register.
fn emit_concat_grow_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: concat_grow ---");
    emitter.label_global("__rt_concat_grow");

    emitter.instruction("sub sp, sp, #48");                                     // allocate spill space for the old buffer, preserved length, and new buffer
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the grow helper frame pointer
    emitter.instruction("stp x0, x1, [sp]");                                    // save the current buffer and the number of bytes to preserve

    // -- allocate the larger owned block and stamp it as an elephc string --
    emitter.instruction("mov x0, x2");                                          // pass the requested new capacity to the reservation front end
    abi::emit_symbol_address(emitter, "x9", "_heap_max");
    emitter.instruction("ldr x9, [x9]");                                        // load the configured heap capacity as the upper bound for any single result
    emitter.instruction("cmp x0, x9");                                          // is the grown capacity impossible to satisfy?
    emitter.instruction("b.hi __rt_concat_grow_too_large");                     // report a PHP-style allocation overflow instead of writing past any buffer
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the larger owned accumulation buffer
    emitter.instruction(&format!("mov x9, #{}", CONCAT_TEMP_HEAP_KIND));        // stamp a transient string until a consumer persists it
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the heap allocation as a transient string payload
    emitter.instruction("str x0, [sp, #16]");                                   // save the grown buffer for the return value

    // -- copy the bytes written so far into the grown buffer --
    emitter.instruction("ldp x10, x11, [sp]");                                  // reload the old buffer pointer and the preserved byte count
    emitter.instruction("mov x12, #0");                                         // byte-copy index
    emitter.label("__rt_concat_grow_copy");
    emitter.instruction("cmp x12, x11");                                        // have all preserved bytes been copied into the grown buffer?
    emitter.instruction("b.hs __rt_concat_grow_copy_done");                     // leave the copy loop once the preserved prefix is duplicated
    emitter.instruction("ldrb w13, [x10, x12]");                                // load the next preserved byte from the old buffer
    emitter.instruction("strb w13, [x0, x12]");                                 // store it at the same offset inside the grown buffer
    emitter.instruction("add x12, x12, #1");                                    // advance the copy index
    emitter.instruction("b __rt_concat_grow_copy");                             // copy the next preserved byte
    emitter.label("__rt_concat_grow_copy_done");

    // -- release the old block when it was heap-backed; scratch pointers are skipped --
    emitter.instruction("ldr x0, [sp]");                                        // reload the old buffer pointer for release
    emitter.instruction("bl __rt_heap_free_safe");                              // free the superseded owned block, ignoring concat-scratch pointers
    emitter.instruction("ldr x0, [sp, #16]");                                   // return the grown buffer pointer
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the grow helper frame
    emitter.instruction("ret");                                                 // return the grown accumulation buffer

    // -- impossible capacity: report PHP's allocation-overflow fatal error --
    emitter.label("__rt_concat_grow_too_large");
    emitter.instruction("b __rt_alloc_overflow");                               // unconditional branch keeps the fatal trampoline cross-atom safe
}

/// Emits the AArch64 `__rt_alloc_overflow` fatal trampoline.
///
/// Mirrors PHP's "Possible integer overflow in memory allocation" fatal: it writes the
/// diagnostic to stderr and exits with a non-zero status rather than faulting.
fn emit_alloc_overflow_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: alloc_overflow (fatal) ---");
    emitter.label_global("__rt_alloc_overflow");

    emit_cdylib_allocation_escape(emitter, "__rt_alloc_overflow_fatal");

    emitter.instruction("mov x0, #2");                                          // fd = stderr for the allocation-overflow diagnostic
    abi::emit_symbol_address(emitter, "x1", "_alloc_overflow_msg");
    emitter.instruction(&format!("mov x2, #{}", ALLOC_OVERFLOW_MSG.len()));     // pass the exact allocation-overflow diagnostic byte count
    emitter.syscall(4);
    emitter.instruction("mov x0, #1");                                          // exit code 1 for the allocation-overflow abort path
    emitter.syscall(1);
}

/// Emits the Linux x86_64 variants of the concat scratch reservation helpers.
///
/// Same contract as the AArch64 path with System V registers: `rax` carries the requested
/// size into `__rt_concat_reserve` and the destination pointer out, while
/// `__rt_concat_publish` takes and preserves the `rax`/`rdx` string result pair.
fn emit_concat_scratch_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: concat_reserve ---");
    emitter.label_global("__rt_concat_reserve");

    // -- reject requests the allocator could never satisfy, including wrapped sizes --
    abi::emit_symbol_address(emitter, "r8", "_heap_max");
    emitter.instruction("mov r8, QWORD PTR [r8]");                              // load the configured heap capacity as the upper bound for any single result
    emitter.instruction("cmp rax, r8");                                         // is the requested byte count impossible to satisfy (unsigned, so wrapped sizes are huge)?
    emitter.instruction("ja __rt_concat_reserve_too_large_x86");                // report a PHP-style allocation overflow instead of writing past any buffer

    // -- prefer the shared 64 KiB scratch buffer while the result still fits --
    abi::emit_symbol_address(emitter, "r9", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r9]");                              // load the current concat scratch write offset
    emitter.instruction("mov r10, r9");                                         // copy the write offset before deriving the tail this request would reach
    emitter.instruction("add r10, rax");                                        // compute the scratch tail this request would reach
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "r8", emitter.concat_capacity as i64); // load the shared scratch capacity
    emitter.instruction("cmp r10, r8");                                         // does the reservation still fit inside the shared scratch buffer?
    emitter.instruction("ja __rt_concat_reserve_heap_x86");                     // use the owned heap fallback when the scratch buffer would overflow
    abi::emit_symbol_address(emitter, "r11", "_concat_buf");
    emitter.instruction("lea rax, [r11 + r9]");                                 // return the scratch destination pointer at the current write offset
    emitter.instruction("ret");                                                 // return the scratch-backed reservation to the caller

    // -- heap fallback: oversized results get their own owned string allocation --
    emitter.label("__rt_concat_reserve_heap_x86");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer and realign the stack for the allocator call
    emitter.instruction("mov rbp, rsp");                                        // establish the reservation helper frame pointer
    emitter.instruction("call __rt_heap_alloc");                                // allocate owned storage large enough for the requested result
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(1)));// materialize the owned-string heap kind word with the x86_64 heap marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the heap allocation as a string payload
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the allocator call
    emitter.instruction("ret");                                                 // return the heap-backed reservation to the caller

    // -- impossible request: report PHP's allocation-overflow fatal error --
    emitter.label("__rt_concat_reserve_too_large_x86");
    emitter.instruction("jmp __rt_alloc_overflow");                             // unconditional branch keeps the fatal trampoline reachable from every caller

    emitter.blank();
    emitter.comment("--- runtime: concat_publish ---");
    emitter.label_global("__rt_concat_publish");

    abi::emit_symbol_address(emitter, "r8", "_concat_buf");
    emitter.instruction("mov r9, rax");                                         // copy the result pointer before deriving its candidate scratch offset
    emitter.instruction("sub r9, r8");                                          // compute the candidate scratch offset of the finished result
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "r8", emitter.concat_capacity as i64); // load the shared scratch capacity
    emitter.instruction("cmp r9, r8");                                          // is the result outside the shared scratch window (unsigned, so heap pointers wrap high)?
    emitter.instruction("jae __rt_concat_publish_done_x86");                    // heap-backed results leave the shared scratch offset untouched
    emitter.instruction("add r9, rdx");                                         // advance the scratch offset past the bytes this result actually wrote
    abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov QWORD PTR [r8], r9");                              // publish the updated concat scratch write offset

    emitter.label("__rt_concat_publish_done_x86");
    emitter.instruction("ret");                                                 // return with the result pointer/length pair untouched

    emitter.blank();
    emitter.comment("--- runtime: concat_grow ---");
    emitter.label_global("__rt_concat_grow");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer across the allocator and release calls
    emitter.instruction("mov rbp, rsp");                                        // establish the grow helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill slots for the old buffer, preserved length, and grown buffer
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the current buffer pointer across the allocator call
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // save the number of bytes to preserve across the allocator call

    // -- allocate the larger owned block and stamp it as an elephc string --
    emitter.instruction("mov rax, rsi");                                        // pass the requested new capacity to the allocator
    abi::emit_symbol_address(emitter, "r8", "_heap_max");
    emitter.instruction("mov r8, QWORD PTR [r8]");                              // load the configured heap capacity as the upper bound for any single result
    emitter.instruction("cmp rax, r8");                                         // is the grown capacity impossible to satisfy?
    emitter.instruction("ja __rt_concat_grow_too_large_x86");                   // report a PHP-style allocation overflow instead of writing past any buffer
    emitter.instruction("call __rt_heap_alloc");                                // allocate the larger owned accumulation buffer
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(CONCAT_TEMP_HEAP_KIND)));// materialize the transient-string heap kind word with the x86_64 heap marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the heap allocation as a transient string payload
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the grown buffer for the return value

    // -- copy the bytes written so far into the grown buffer --
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the old buffer pointer for the preserved-prefix copy
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the preserved byte count for the copy loop
    emitter.instruction("xor rcx, rcx");                                        // byte-copy index
    emitter.label("__rt_concat_grow_copy_x86");
    emitter.instruction("cmp rcx, r11");                                        // have all preserved bytes been copied into the grown buffer?
    emitter.instruction("jae __rt_concat_grow_copy_done_x86");                  // leave the copy loop once the preserved prefix is duplicated
    emitter.instruction("mov r9b, BYTE PTR [r10 + rcx]");                       // load the next preserved byte from the old buffer
    emitter.instruction("mov BYTE PTR [rax + rcx], r9b");                       // store it at the same offset inside the grown buffer
    emitter.instruction("inc rcx");                                             // advance the copy index
    emitter.instruction("jmp __rt_concat_grow_copy_x86");                       // copy the next preserved byte
    emitter.label("__rt_concat_grow_copy_done_x86");

    // -- release the old block when it was heap-backed; scratch pointers are skipped --
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the old buffer pointer for release
    emitter.instruction("call __rt_heap_free_safe");                            // free the superseded owned block, ignoring concat-scratch pointers
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the grown buffer pointer
    emitter.instruction("add rsp, 32");                                         // release the grow helper spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the grown accumulation buffer

    // -- impossible capacity: report PHP's allocation-overflow fatal error --
    emitter.label("__rt_concat_grow_too_large_x86");
    emitter.instruction("jmp __rt_alloc_overflow");                             // unconditional branch keeps the fatal trampoline reachable from every caller

    emitter.blank();
    emitter.comment("--- runtime: alloc_overflow (fatal) ---");
    emitter.label_global("__rt_alloc_overflow");

    emit_cdylib_allocation_escape(emitter, "__rt_alloc_overflow_fatal");

    emitter.instruction("mov edi, 2");                                          // fd = stderr for the allocation-overflow diagnostic
    abi::emit_symbol_address(emitter, "rsi", "_alloc_overflow_msg");
    emitter.instruction(&format!("mov edx, {}", ALLOC_OVERFLOW_MSG.len()));     // pass the exact allocation-overflow diagnostic byte count
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // emit the fatal allocation-overflow message before terminating
    emitter.instruction("mov edi, 1");                                          // exit code 1 for the allocation-overflow abort path
    emitter.instruction("mov eax, 60");                                         // Linux x86_64 syscall 60 = exit
    emitter.instruction("syscall");                                             // terminate the process after reporting the impossible allocation
}

/// Emits the AArch64 `__rt_concat_headroom` guard and its fatal overflow exit.
///
/// `__rt_concat_headroom` takes the absolute write pointer in `x0` and the number of bytes
/// the caller is about to write in `x1`. It returns silently while the write stays inside
/// `[_concat_buf, _concat_buf + capacity)` and reaches `__rt_concat_overflow` otherwise. Incremental
/// encoders whose result size is not known up front (`json_encode()`, `serialize()`) call it
/// before every append so an oversized result fails closed instead of overwriting `_concat_off`
/// and the adjacent heap.
fn emit_concat_headroom_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: concat_headroom (guard) ---");
    emitter.label_global("__rt_concat_headroom");
    abi::emit_symbol_address(emitter, "x9", "_concat_buf");
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "x10", emitter.concat_capacity as i64); // load the scratch capacity
    emitter.instruction("add x9, x9, x10");                                     // compute the one-past-end scratch address
    emitter.instruction("cmp x0, x9");                                          // is the write pointer already outside the arena?
    emitter.instruction("b.hs __rt_concat_headroom_full");                      // fail closed before any byte leaves the buffer
    emitter.instruction("sub x11, x9, x0");                                     // remaining bytes between the cursor and the arena end
    emitter.instruction("cmp x1, x11");                                         // does the pending write still fit?
    emitter.instruction("b.hi __rt_concat_headroom_full");                      // fail closed when the append would cross the end
    emitter.instruction("ret");                                                 // the pending write fits inside the shared scratch arena

    emitter.label("__rt_concat_headroom_full");
    emitter.instruction("b __rt_concat_overflow");                              // unconditional branch keeps the fatal trampoline cross-atom safe

    emitter.blank();
    emitter.comment("--- runtime: concat_overflow (fatal) ---");
    emitter.label_global("__rt_concat_overflow");
    emit_cdylib_allocation_escape(emitter, "__rt_concat_overflow_fatal");
    emitter.instruction("mov x0, #2");                                          // write the capacity diagnostic to stderr
    abi::emit_symbol_address(emitter, "x1", "_concat_overflow_msg");
    emitter.instruction(&format!("mov x2, #{}", CONCAT_OVERFLOW_MSG.len()));    // exact diagnostic byte length
    emitter.syscall(4);
    emitter.instruction("mov x0, #255");                                        // PHP exits with 255 on an uncaught fatal error
    emitter.syscall(1);
}

/// Emits the x86_64 Linux `__rt_concat_headroom` guard and its fatal overflow exit.
///
/// Inputs use the System V registers: `rdi` = absolute write pointer, `rsi` = pending byte
/// count. The contract matches the AArch64 helper.
fn emit_concat_headroom_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: concat_headroom (guard) ---");
    emitter.label_global("__rt_concat_headroom");
    abi::emit_symbol_address(emitter, "r8", "_concat_buf");
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "r9", emitter.concat_capacity as i64); // load the scratch capacity
    emitter.instruction("add r9, r8");                                          // compute the one-past-end scratch address
    emitter.instruction("cmp rdi, r9");                                         // is the write pointer already outside the arena?
    emitter.instruction("jae __rt_concat_headroom_full_x86");                   // fail closed before any byte leaves the buffer
    emitter.instruction("mov r10, r9");                                         // seed the remaining-capacity computation
    emitter.instruction("sub r10, rdi");                                        // remaining bytes between the cursor and the arena end
    emitter.instruction("cmp rsi, r10");                                        // does the pending write still fit?
    emitter.instruction("ja __rt_concat_headroom_full_x86");                    // fail closed when the append would cross the end
    emitter.instruction("ret");                                                 // the pending write fits inside the shared scratch arena

    emitter.label("__rt_concat_headroom_full_x86");
    emitter.instruction("jmp __rt_concat_overflow");                            // unconditional branch keeps the fatal trampoline reachable

    emitter.blank();
    emitter.comment("--- runtime: concat_overflow (fatal) ---");
    emitter.label_global("__rt_concat_overflow");
    emit_cdylib_allocation_escape(emitter, "__rt_concat_overflow_fatal");
    emitter.instruction("mov edi, 2");                                          // write the capacity diagnostic to stderr
    abi::emit_symbol_address(emitter, "rsi", "_concat_overflow_msg");
    emitter.instruction(&format!("mov edx, {}", CONCAT_OVERFLOW_MSG.len()));    // exact diagnostic byte length
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // emit the fatal capacity diagnostic
    emitter.instruction("mov edi, 255");                                        // PHP exits with 255 on an uncaught fatal error
    emitter.instruction("mov eax, 60");                                         // Linux x86_64 syscall 60 = exit
    emitter.instruction("syscall");                                             // terminate after reporting the oversized result
}

/// Converts an impossible allocation into a recoverable cdylib boundary escape.
fn emit_cdylib_allocation_escape(emitter: &mut Emitter, fatal_label: &str) {
    if !emitter.cdylib_boundary {
        return;
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(
                emitter,
                "x9",
                crate::codegen_support::cdylib::BOUNDARY_ACTIVE,
            );
            emitter.instruction("ldr x9, [x9]");                                // read whether a native recovery boundary is active
            emitter.instruction(&format!("cbz x9, {}", fatal_label));           // retain executable fatal behavior without a boundary
            abi::emit_store_imm_to_symbol(
                emitter,
                crate::codegen_support::cdylib::BOUNDARY_STATUS,
                0,
                crate::codegen_support::cdylib::STATUS_ALLOCATION_FAILURE as i64,
            );
            abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
            emitter.instruction("b __rt_throw_current");                        // unwind to the cdylib handler with allocation status recorded
            emitter.label(fatal_label);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(
                emitter,
                "r8",
                crate::codegen_support::cdylib::BOUNDARY_ACTIVE,
            );
            emitter.instruction("mov r8, QWORD PTR [r8]");                      // read whether a native recovery boundary is active
            emitter.instruction("test r8, r8");                                 // distinguish cdylib recovery from executable fatal handling
            emitter.instruction(&format!("jz {}", fatal_label));                // retain executable fatal behavior without a boundary
            abi::emit_store_imm_to_symbol(
                emitter,
                crate::codegen_support::cdylib::BOUNDARY_STATUS,
                0,
                crate::codegen_support::cdylib::STATUS_ALLOCATION_FAILURE as i64,
            );
            abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
            emitter.instruction("jmp __rt_throw_current");                      // unwind to the cdylib handler with allocation status recorded
            emitter.label(fatal_label);
        }
    }
}


/// Emits the AArch64 `__rt_preg_cb_ensure` guard for the callback replacement buffer.
///
/// Inputs: `x0` = address of the base slot, `x1` = address of the write slot,
/// `x2` = address of the capacity slot, `x3` = bytes about to be appended.
/// The helper grows the owned output block through `__rt_concat_grow` when
/// `write - base + needed` exceeds the capacity, then updates the three slots.
/// It preserves every callee-saved register and clobbers only `x0`-`x13`.
fn emit_preg_cb_ensure_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: preg_cb_ensure (grow callback output) ---");
    emitter.label_global("__rt_preg_cb_ensure");
    emitter.instruction("ldr x9, [x0]");                                        // output base
    emitter.instruction("ldr x10, [x1]");                                       // output write cursor
    emitter.instruction("ldr x11, [x2]");                                       // output capacity
    emitter.instruction("sub x12, x10, x9");                                    // bytes already written
    emitter.instruction("add x13, x12, x3");                                    // bytes needed after this append
    emitter.instruction("cmp x13, x11");                                        // does the append still fit?
    emitter.instruction("b.hi __rt_preg_cb_ensure_grow");                       // grow the owned block when it does not
    emitter.instruction("ret");                                                 // the pending append fits the current block

    emitter.label("__rt_preg_cb_ensure_grow");
    emitter.instruction("sub sp, sp, #96");                                     // reserve the grow frame
    emitter.instruction("stp x29, x30, [sp, #80]");                             // preserve the frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish the ensure frame pointer
    emitter.instruction("stp x0, x1, [sp]");                                    // save the base and write slot addresses
    emitter.instruction("str x2, [sp, #16]");                                   // save the capacity slot address
    emitter.instruction("stp x12, x13, [sp, #24]");                             // save the used and total byte counts
    emitter.instruction("mov x0, x9");                                          // pass the current base
    emitter.instruction("mov x1, x12");                                         // pass the bytes to preserve
    emitter.instruction("lsl x2, x11, #1");                                     // start with double the current capacity
    emitter.instruction("cmp x2, x13");                                         // is the doubled capacity enough for this append?
    emitter.instruction("csel x2, x13, x2, lo");                                // otherwise grow straight to the required total
    emitter.instruction("str x2, [sp, #40]");                                   // save the new capacity across the grow call
    emitter.instruction("bl __rt_concat_grow");                                 // allocate, copy, and release the old block
    emitter.instruction("mov x9, #1");                                          // callback output is an owned call result, not a concat operand
    emitter.instruction("str x9, [x0, #-8]");                                   // retag the grown block so persistence copies instead of adopting it
    emitter.instruction("mov x9, x0");                                          // new base
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the base slot address
    emitter.instruction("str x9, [x10]");                                       // publish the new base
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload the bytes already written
    emitter.instruction("add x11, x9, x11");                                    // write cursor = new base + used
    emitter.instruction("ldr x12, [sp, #8]");                                   // reload the write slot address
    emitter.instruction("str x11, [x12]");                                      // publish the new write cursor
    emitter.instruction("ldr x13, [sp, #16]");                                  // reload the capacity slot address
    emitter.instruction("ldr x14, [sp, #40]");                                  // reload the new capacity
    emitter.instruction("str x14, [x13]");                                      // publish the new capacity
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore the frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the ensure frame
    emitter.instruction("ret");                                                 // return to the append site
}

/// Emits the x86_64 `__rt_preg_cb_ensure` guard for the callback replacement buffer.
///
/// Inputs: `rdi` = address of the base slot, `rsi` = address of the write slot,
/// `rdx` = address of the capacity slot, `rcx` = bytes about to be appended.
fn emit_preg_cb_ensure_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: preg_cb_ensure (grow callback output) ---");
    emitter.label_global("__rt_preg_cb_ensure");
    emitter.instruction("mov r8, QWORD PTR [rdi]");                             // output base
    emitter.instruction("mov r9, QWORD PTR [rsi]");                             // output write cursor
    emitter.instruction("mov r10, QWORD PTR [rdx]");                            // output capacity
    emitter.instruction("mov r11, r9");                                         // bytes already written
    emitter.instruction("sub r11, r8");                                         // used = write - base
    emitter.instruction("mov rax, r11");                                        // seed the required total
    emitter.instruction("add rax, rcx");                                        // total = used + needed
    emitter.instruction("cmp rax, r10");                                        // does the append still fit?
    emitter.instruction("ja __rt_preg_cb_ensure_grow_x86");                     // grow the owned block when it does not
    emitter.instruction("ret");                                                 // the pending append fits the current block

    emitter.label("__rt_preg_cb_ensure_grow_x86");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the ensure frame base
    emitter.instruction("sub rsp, 64");                                         // reserve the grow frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the base slot address
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the write slot address
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the capacity slot address
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // save the bytes already written
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the required total
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // save the current capacity
    emitter.instruction("mov rax, r8");                                         // pass the current base
    emitter.instruction("mov rdi, r11");                                        // pass the bytes to preserve
    emitter.instruction("mov rsi, r10");                                        // start with double the current capacity
    emitter.instruction("shl rsi, 1");                                          // double it
    emitter.instruction("cmp rsi, QWORD PTR [rbp - 40]");                       // is the doubled capacity enough?
    emitter.instruction("jae __rt_preg_cb_ensure_cap_ready_x86");               // keep the doubled capacity when it is
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // otherwise grow straight to the required total
    emitter.label("__rt_preg_cb_ensure_cap_ready_x86");
    emitter.instruction("mov QWORD PTR [rbp - 56], rsi");                       // save the new capacity across the grow call
    emitter.instruction("call __rt_concat_grow");                               // allocate, copy, and release the old block
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(1)));// materialize the owned-string kind word
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // retag the grown block so persistence copies instead of adopting it
    emitter.instruction("mov r8, rax");                                         // new base
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the base slot address
    emitter.instruction("mov QWORD PTR [r9], r8");                              // publish the new base
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the bytes already written
    emitter.instruction("add r10, r8");                                         // write cursor = new base + used
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the write slot address
    emitter.instruction("mov QWORD PTR [r9], r10");                             // publish the new write cursor
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the capacity slot address
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the new capacity
    emitter.instruction("mov QWORD PTR [r10], r11");                            // publish the new capacity
    emitter.instruction("add rsp, 64");                                         // release the ensure frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the append site
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// The shared grow helper returns a transient string on both ABIs, while exact reservations
    /// remain ordinary owned strings. This lets `__rt_str_persist` adopt grown results in place.
    #[test]
    fn concat_grow_marks_results_transient_on_both_architectures() {
        let mut arm = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_concat_scratch(&mut arm);
        let arm_asm = arm.output();
        assert!(arm_asm.contains("mov x9, #1"));
        assert!(arm_asm.contains(&format!("mov x9, #{}", CONCAT_TEMP_HEAP_KIND)));

        let mut x86 = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_concat_scratch(&mut x86);
        let x86_asm = x86.output();
        assert!(x86_asm.contains(&format!(
            "mov r10, 0x{:x}",
            crate::codegen_support::sentinels::x86_64_heap_kind_word(1)
        )));
        assert!(x86_asm.contains(&format!(
            "mov r10, 0x{:x}",
            crate::codegen_support::sentinels::x86_64_heap_kind_word(CONCAT_TEMP_HEAP_KIND)
        )));
    }
}
