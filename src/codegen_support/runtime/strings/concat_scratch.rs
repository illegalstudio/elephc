//! Purpose:
//! Emits the shared `__rt_concat_reserve`, `__rt_concat_publish`, and `__rt_alloc_overflow`
//! runtime helpers that bound every append into the fixed 64 KiB `_concat_buf` scratch buffer.
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
//! - `__rt_concat_publish` derives the storage class from the pointer itself: only pointers
//!   inside `[_concat_buf, _concat_buf + 65536)` move `_concat_off`, so heap-backed results
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
use crate::codegen_support::runtime::data::ALLOC_OVERFLOW_MSG;

/// Byte capacity of the shared `_concat_buf` scratch buffer declared in `runtime::data::fixed`.
pub(crate) const CONCAT_BUF_CAPACITY: usize = 65536;

/// Uniform heap-header kind stamped on a heap-backed `.` operator result.
///
/// The `.` operator is the only producer that stamps this kind, and codegen never releases a
/// `StrConcat` value (`value_is_scratch_string` classifies it as transient), so a live block
/// carrying this kind is by construction an unowned temporary with at most one consumer.
/// `__rt_str_persist` uses that to take the block over in place instead of copying it, which
/// is what keeps `$s .= ...` accumulation loops from leaking one oversized block per append.
pub(crate) const CONCAT_TEMP_HEAP_KIND: u32 = 7;

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
/// - Output: `x0`/`rax` = larger owned heap buffer holding the preserved prefix.
/// - Releases the superseded buffer through `__rt_heap_free_safe` (a no-op for scratch).
///
/// # `__rt_alloc_overflow`
/// - Recovers through an active cdylib boundary, otherwise writes PHP's
///   allocation-overflow fatal message and exits with status 1.
pub fn emit_concat_scratch(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_concat_scratch_linux_x86_64(emitter);
        return;
    }

    emit_concat_reserve_aarch64(emitter);
    emit_concat_publish_aarch64(emitter);
    emit_concat_grow_aarch64(emitter);
    emit_alloc_overflow_aarch64(emitter);
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
    emitter.instruction(&format!("mov x13, #{}", CONCAT_BUF_CAPACITY));         // load the concat scratch capacity in bytes
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
/// inside `[_concat_buf, _concat_buf + 65536)` advances `_concat_off`. Heap-backed results
/// own their own storage and must not move the shared scratch cursor.
fn emit_concat_publish_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: concat_publish ---");
    emitter.label_global("__rt_concat_publish");

    abi::emit_symbol_address(emitter, "x9", "_concat_buf");
    emitter.instruction("sub x10, x1, x9");                                     // compute the candidate scratch offset of the finished result
    emitter.instruction(&format!("mov x11, #{}", CONCAT_BUF_CAPACITY));         // load the concat scratch capacity in bytes
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
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = owned elephc string
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the heap allocation as a string payload
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

    emit_cdylib_allocation_escape(emitter);

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
    emitter.instruction(&format!("cmp r10, {}", CONCAT_BUF_CAPACITY));          // does the reservation still fit inside the shared scratch buffer?
    emitter.instruction("ja __rt_concat_reserve_heap_x86");                     // use the owned heap fallback when the scratch buffer would overflow
    abi::emit_symbol_address(emitter, "r11", "_concat_buf");
    emitter.instruction("lea rax, [r11 + r9]");                                 // return the scratch destination pointer at the current write offset
    emitter.instruction("ret");                                                 // return the scratch-backed reservation to the caller

    // -- heap fallback: oversized results get their own owned string allocation --
    emitter.label("__rt_concat_reserve_heap_x86");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer and realign the stack for the allocator call
    emitter.instruction("mov rbp, rsp");                                        // establish the reservation helper frame pointer
    emitter.instruction("call __rt_heap_alloc");                                // allocate owned storage large enough for the requested result
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(1))); // materialize the owned-string heap kind word with the x86_64 heap marker
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
    emitter.instruction(&format!("cmp r9, {}", CONCAT_BUF_CAPACITY));           // is the result outside the shared scratch window (unsigned, so heap pointers wrap high)?
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
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(1))); // materialize the owned-string heap kind word with the x86_64 heap marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the heap allocation as a string payload
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

    emit_cdylib_allocation_escape(emitter);

    emitter.instruction("mov edi, 2");                                          // fd = stderr for the allocation-overflow diagnostic
    abi::emit_symbol_address(emitter, "rsi", "_alloc_overflow_msg");
    emitter.instruction(&format!("mov edx, {}", ALLOC_OVERFLOW_MSG.len()));     // pass the exact allocation-overflow diagnostic byte count
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // emit the fatal allocation-overflow message before terminating
    emitter.instruction("mov edi, 1");                                          // exit code 1 for the allocation-overflow abort path
    emitter.instruction("mov eax, 60");                                         // Linux x86_64 syscall 60 = exit
    emitter.instruction("syscall");                                             // terminate the process after reporting the impossible allocation
}

/// Converts an impossible allocation into a recoverable cdylib boundary escape.
fn emit_cdylib_allocation_escape(emitter: &mut Emitter) {
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
            emitter.instruction("cbz x9, __rt_alloc_overflow_fatal");           // retain executable fatal behavior without a boundary
            abi::emit_store_imm_to_symbol(
                emitter,
                crate::codegen_support::cdylib::BOUNDARY_STATUS,
                0,
                crate::codegen_support::cdylib::STATUS_ALLOCATION_FAILURE as i64,
            );
            abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
            emitter.instruction("b __rt_throw_current");                        // unwind to the cdylib handler with allocation status recorded
            emitter.label("__rt_alloc_overflow_fatal");
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(
                emitter,
                "r8",
                crate::codegen_support::cdylib::BOUNDARY_ACTIVE,
            );
            emitter.instruction("mov r8, QWORD PTR [r8]");                      // read whether a native recovery boundary is active
            emitter.instruction("test r8, r8");                                 // distinguish cdylib recovery from executable fatal handling
            emitter.instruction("jz __rt_alloc_overflow_fatal");                // retain executable fatal behavior without a boundary
            abi::emit_store_imm_to_symbol(
                emitter,
                crate::codegen_support::cdylib::BOUNDARY_STATUS,
                0,
                crate::codegen_support::cdylib::STATUS_ALLOCATION_FAILURE as i64,
            );
            abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
            emitter.instruction("jmp __rt_throw_current");                      // unwind to the cdylib handler with allocation status recorded
            emitter.label("__rt_alloc_overflow_fatal");
        }
    }
}
