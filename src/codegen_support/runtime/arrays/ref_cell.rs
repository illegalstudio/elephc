//! Purpose:
//! Emits the refcounted reference-cell runtime helpers: `__rt_ref_cell_alloc`,
//! `__rt_ref_cell_incref`, `__rt_ref_cell_decref`, `__rt_ref_cell_free_deep`, and
//! `__rt_ref_cell_store`.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - A reference cell is a managed-heap block with heap kind `REFERENCE_CELL_HEAP_KIND` and payload
//!   `{ value@0, inner_tag@8 }`. The inner tag is ALSO kept in header bits 8-14, the payload
//!   descriptor every generic managed-cell walker (`__rt_decref_any`, clone, the collectors) reads,
//!   so these cells and the object-property cells share one release and tracing contract.
//!   Heap kind 7 is the transient concat buffer and must never be stamped here.
//!   It carries the uniform 16-byte header (`size@-16`, `refcnt@-12`, `kind@-8`) so it is refcountable
//!   and visible to the cycle collector, unlike the legacy raw 16-byte cell.
//! - `inner_tag` is a runtime value-tag (0 Int … 10 Callable), not a heap kind; the container slot that
//!   holds a reference stores the hardcoded value-tag 11 alongside this cell pointer.
//! - Multiple owners (array slot(s) + alias local(s)) share one cell; the refcount tracks them.

use crate::codegen_support::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen_support::sentinels::{
    emit_store_reference_cell_canary, x86_64_heap_kind_word, REFERENCE_CELL_HEAP_KIND,
    REFERENCE_CELL_PAYLOAD_BYTES,
};

/// Emits `__rt_ref_cell_alloc`: allocate a managed refcounted reference cell.
///
/// Allocates 16 payload bytes through the managed heap (header refcount = 1), stamps the managed reference-cell kind and payload descriptor,
/// and stores the initial inner value at `+0` and the inner value-tag at `+8`.
///
/// # ABI
/// - ARM64: `x0` = inner value word, `x1` = inner value-tag; result `x0` = cell pointer.
/// - x86_64: `rdi` = inner value word, `rsi` = inner value-tag; result `rax` = cell pointer.
pub fn emit_ref_cell_alloc(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ref_cell_alloc_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: ref_cell_alloc ---");
    emitter.label_global("__rt_ref_cell_alloc");

    // -- set up a frame and preserve the incoming payload across the heap allocation --
    emitter.instruction("sub sp, sp, #32");                                     // allocate a small frame for the payload spill
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // set up the new frame pointer
    emitter.instruction("stp x0, x1, [sp, #0]");                                // save the inner value word and inner value-tag across the alloc call

    // -- allocate the 16-byte cell payload through the managed heap --
    emitter.instruction(&format!("mov x0, #{REFERENCE_CELL_PAYLOAD_BYTES}"));    // reference cells store value@0, inner_tag@8 and their canary@16
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the cell storage; header refcount starts at 1
    emit_store_reference_cell_canary(emitter, "x0", "x9");
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the inner value-tag for the header payload descriptor
    emitter.instruction("lsl x9, x9, #8");                                      // the managed-cell payload descriptor lives in header bits 8-14
    emitter.instruction(&format!("orr x9, x9, #{}", REFERENCE_CELL_HEAP_KIND)); // identify the block as a managed reference cell
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the cell kind and payload descriptor in the uniform header

    // -- install the initial inner value and its value-tag --
    emitter.instruction("ldp x10, x11, [sp, #0]");                              // reload the inner value word and inner value-tag
    emitter.instruction("str x10, [x0]");                                       // store the inner value at cell+0
    emitter.instruction("str x11, [x0, #8]");                                   // store the inner value-tag at cell+8

    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate the frame
    emitter.instruction("ret");                                                 // return the reference-cell pointer in x0
}

/// Emits the x86_64 Linux variant of `__rt_ref_cell_alloc`.
///
/// Allocates the 16-byte cell through the shared heap wrapper, stamps the kind word with the x86_64 heap
/// marker plus the managed reference-cell kind and payload descriptor, and installs the inner value at `+0` and the inner value-tag at `+8`.
fn emit_ref_cell_alloc_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ref_cell_alloc ---");
    emitter.label_global("__rt_ref_cell_alloc");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before spilling the payload
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the payload spill
    emitter.instruction("sub rsp, 16");                                         // reserve slots for the inner value word and inner value-tag
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the inner value word across the heap allocation call
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the inner value-tag across the heap allocation call
    emitter.instruction(&format!("mov rax, {REFERENCE_CELL_PAYLOAD_BYTES}"));    // reference cells store value@0, inner_tag@8 and their canary@16
    emitter.instruction("call __rt_heap_alloc");                                // allocate the cell storage; header refcount starts at 1
    emit_store_reference_cell_canary(emitter, "rax", "r10");
    emitter.instruction(&format!("mov r10, 0x{:x}", x86_64_heap_kind_word(REFERENCE_CELL_HEAP_KIND))); // materialize the managed reference-cell kind word with the x86_64 heap marker
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the inner value-tag for the header payload descriptor
    emitter.instruction("shl r11, 8");                                          // the managed-cell payload descriptor lives in header bits 8-14
    emitter.instruction("or r10, r11");                                         // combine the cell kind with its payload descriptor
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the cell kind and payload descriptor in the uniform header
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the inner value word after the heap allocation
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store the inner value at cell+0
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the inner value-tag after the heap allocation
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // store the inner value-tag at cell+8
    emitter.instruction("add rsp, 16");                                         // release the payload spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the reference-cell pointer in rax
}

/// Emits `__rt_ref_cell_incref`: retain a shared reference cell.
///
/// A kind-7 cell is an ordinary managed heap block, so the generic `__rt_incref` (which range-checks the
/// pointer and bumps the 32-bit refcount at `[cell-12]`) is exactly the required behavior. Both variants
/// tail-call it, keeping the input register identical to the generic helper (`x0` / `rax`).
pub fn emit_ref_cell_incref(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emitter.blank();
        emitter.comment("--- runtime: ref_cell_incref ---");
        emitter.label_global("__rt_ref_cell_incref");
        emitter.instruction("jmp __rt_incref");                                 // a kind-7 cell is a managed heap block; reuse the generic increment
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: ref_cell_incref ---");
    emitter.label_global("__rt_ref_cell_incref");
    emitter.instruction("b __rt_incref");                                       // a kind-7 cell is a managed heap block; reuse the generic increment
}

/// Emits `__rt_ref_cell_release_claim`, decrementing a raw local cell's owner count.
///
/// Returns the unchanged cell pointer only when the caller released the final share; otherwise it
/// returns null. The caller remains responsible for type-aware payload destruction because raw
/// local cells store string lengths or tagged-scalar metadata in their second word rather than a
/// uniform runtime value tag.
pub fn emit_ref_cell_release_claim(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emitter.blank();
        emitter.comment("--- runtime: ref_cell_release_claim ---");
        emitter.label_global("__rt_ref_cell_release_claim");
        emitter.instruction("test rax, rax");                                   // null cells have no ownership share to release
        emitter.instruction("jz __rt_ref_cell_release_claim_done");             // return null unchanged for an absent cell
        emitter.instruction("mov r10d, DWORD PTR [rax - 12]");                  // load the raw local cell's uniform heap refcount
        emitter.instruction("sub r10d, 1");                                     // release the caller's ownership share
        emitter.instruction("mov DWORD PTR [rax - 12], r10d");                  // publish the decremented owner count
        emitter.instruction("jz __rt_ref_cell_release_claim_done");             // the final owner receives the cell pointer for typed destruction
        emitter.instruction("xor eax, eax");                                    // another owner keeps the raw cell and its payload alive
        emitter.label("__rt_ref_cell_release_claim_done");
        emitter.instruction("ret");                                             // return cell-or-null in rax
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: ref_cell_release_claim ---");
    emitter.label_global("__rt_ref_cell_release_claim");
    emitter.instruction("cbz x0, __rt_ref_cell_release_claim_done");            // null cells have no ownership share to release
    emitter.instruction("ldr w9, [x0, #-12]");                                  // load the raw local cell's uniform heap refcount
    emitter.instruction("subs w9, w9, #1");                                     // release the caller's ownership share
    emitter.instruction("str w9, [x0, #-12]");                                  // publish the decremented owner count
    emitter.instruction("b.eq __rt_ref_cell_release_claim_done");               // the final owner receives the cell pointer for typed destruction
    emitter.instruction("mov x0, #0");                                          // another owner keeps the raw cell and its payload alive
    emitter.label("__rt_ref_cell_release_claim_done");
    emitter.instruction("ret");                                                 // return cell-or-null in x0
}

/// Emits `__rt_ref_cell_decref`: release one owner of a reference cell.
///
/// Decrements the 32-bit refcount at `[cell-12]`; when it reaches zero, tail-calls
/// `__rt_ref_cell_free_deep` to release the inner value and free the cell block. Non-heap and null
/// pointers are skipped. Mirrors the structure of `__rt_decref_mixed`.
///
/// # ABI
/// - ARM64: `x0` = cell pointer. x86_64: `rax` = cell pointer.
pub fn emit_ref_cell_decref(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ref_cell_decref_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: ref_cell_decref ---");
    emitter.label_global("__rt_ref_cell_decref");

    emitter.instruction("cbz x0, __rt_ref_cell_decref_skip");                   // skip null reference-cell pointers immediately
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("cmp x0, x9");                                          // is the pointer below the heap start?
    emitter.instruction("b.lo __rt_ref_cell_decref_skip");                      // non-heap pointers need no reference-cell decref
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_heap_off");
    emitter.instruction("ldr x10, [x10]");                                      // load the current heap offset
    emitter.instruction("add x10, x9, x10");                                    // compute the current heap end
    emitter.instruction("cmp x0, x10");                                         // is the pointer at or beyond the heap end?
    emitter.instruction("b.hs __rt_ref_cell_decref_skip");                      // invalid heap pointers must be ignored here

    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_heap_debug_enabled");
    emitter.instruction("ldr x9, [x9]");                                        // load the heap-debug enabled flag
    emitter.instruction("cbz x9, __rt_ref_cell_decref_checked");                // skip debug validation when heap-debug mode is disabled
    emitter.instruction("str x30, [sp, #-16]!");                                // preserve the return address before nested validation
    emitter.instruction("bl __rt_heap_debug_check_live");                       // ensure the reference cell is still live
    emitter.instruction("ldr x30, [sp], #16");                                  // restore the return address after validation
    emitter.label("__rt_ref_cell_decref_checked");

    emitter.instruction("ldr w9, [x0, #-12]");                                  // load the reference-cell refcount from the uniform header
    emitter.instruction("subs w9, w9, #1");                                     // decrement the reference-cell refcount and set flags
    emitter.instruction("str w9, [x0, #-12]");                                  // store the decremented reference-cell refcount
    emitter.instruction("b.eq __rt_ref_cell_decref_free");                      // zero refcount means the inner value and cell can be released now

    emitter.instruction("b __rt_ref_cell_decref_skip");                         // non-zero refcount keeps the shared cell alive

    emitter.label("__rt_ref_cell_decref_free");
    emitter.instruction("b __rt_ref_cell_free_deep");                           // tail-call to release the inner value and free the cell

    emitter.label("__rt_ref_cell_decref_skip");
    emitter.instruction("ret");                                                 // nothing to release
}

/// Emits the x86_64 Linux variant of `__rt_ref_cell_decref`.
fn emit_ref_cell_decref_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ref_cell_decref ---");
    emitter.label_global("__rt_ref_cell_decref");

    emitter.instruction("test rax, rax");                                       // skip null reference-cell pointers immediately
    emitter.instruction("jz __rt_ref_cell_decref_skip");                        // null reference cells own no heap storage
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_heap_buf");
    emitter.instruction("lea r10, [r10 + 16]");                                 // first valid user payload begins after the initial heap header
    emitter.instruction("cmp rax, r10");                                        // reject scalars and static pointers before reading a heap header
    emitter.instruction("jb __rt_ref_cell_decref_skip");                        // non-heap values below the managed heap own no reference-cell storage
    crate::codegen::abi::emit_symbol_address(emitter, "r11", "_heap_off");
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // load the current x86_64 heap bump extent
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_heap_buf");
    emitter.instruction("add r11, r10");                                        // compute the managed heap end address
    emitter.instruction("cmp rax, r11");                                        // is the candidate pointer outside the live heap window?
    emitter.instruction("jae __rt_ref_cell_decref_skip");                       // pointers above the live heap end are not reference cells
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the stamped x86_64 heap kind word from the uniform header
    emitter.instruction("shr r10, 32");                                         // isolate the high-word heap marker used by the x86_64 heap wrapper
    emitter.instruction(&format!("cmp r10d, 0x{:x}", crate::codegen_support::sentinels::X86_64_HEAP_MAGIC_HI32)); // ignore foreign pointers that do not carry the elephc x86_64 heap marker
    emitter.instruction("jne __rt_ref_cell_decref_skip");                       // only elephc-owned reference cells participate in x86_64 decref bookkeeping
    emitter.instruction("mov r10d, DWORD PTR [rax - 12]");                      // load the 32-bit reference-cell refcount from the uniform heap header
    emitter.instruction("sub r10d, 1");                                         // decrement the reference-cell refcount for the releasing owner
    emitter.instruction("mov DWORD PTR [rax - 12], r10d");                      // store the decremented reference-cell refcount back into the header
    emitter.instruction("jz __rt_ref_cell_decref_free");                        // zero refcount means the inner value and cell can be released now
    emitter.instruction("jmp __rt_ref_cell_decref_skip");                       // non-zero refcount keeps the shared cell alive
    emitter.label("__rt_ref_cell_decref_skip");
    emitter.instruction("ret");                                                 // nothing else to do for non-zero refcounts or foreign pointers

    emitter.label("__rt_ref_cell_decref_free");
    emitter.instruction("jmp __rt_ref_cell_free_deep");                         // tail-call to release the inner value and free the cell
}

/// Emits `__rt_ref_cell_free_deep`: unconditionally release a reference cell's inner value and free it.
///
/// Releases the single inner value the cell owns (callable descriptors through the descriptor helper;
/// every other heap-backed inner value through the refcount-aware `__rt_decref_any`, which self-skips
/// scalars via its heap-range guard) and then frees the cell block. Used both by `__rt_ref_cell_decref`
/// on the last owner and by the cycle collector when reclaiming an unreachable cell.
///
/// # ABI
/// - ARM64: `x0` = cell pointer. x86_64: `rax` = cell pointer.
pub fn emit_ref_cell_free_deep(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ref_cell_free_deep_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: ref_cell_free_deep ---");
    emitter.label_global("__rt_ref_cell_free_deep");

    emitter.instruction("cbz x0, __rt_ref_cell_free_deep_done");                // skip null reference-cell pointers immediately
    emitter.instruction("sub sp, sp, #32");                                     // allocate a small frame to preserve the cell pointer
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // set up the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the cell pointer across the inner-value release
    emitter.instruction("ldr x9, [x0, #8]");                                    // load the inner value-tag stored at cell+8
    emitter.instruction("cmp x9, #10");                                         // does the cell hold a callable descriptor?
    emitter.instruction("b.eq __rt_ref_cell_free_deep_callable");               // callable descriptors release through the descriptor helper
    emitter.instruction("cmp x9, #1");                                          // does the cell hold a persisted string?
    emitter.instruction("b.eq __rt_ref_cell_free_deep_release");                // strings release through the uniform dispatcher
    emitter.instruction("cmp x9, #4");                                          // is the inner value below the heap-backed value-tag range?
    emitter.instruction("b.lo __rt_ref_cell_free_deep_free");                   // scalar inner values (0/2/3) own no heap storage
    emitter.instruction("cmp x9, #7");                                          // is the inner value above the heap-backed value-tag range?
    emitter.instruction("b.hi __rt_ref_cell_free_deep_free");                   // non-heap inner tags (8/9/...) own no releasable storage here
    emitter.label("__rt_ref_cell_free_deep_release");
    emitter.instruction("ldr x0, [x0]");                                        // load the inner value pointer from cell+0
    emitter.instruction("bl __rt_decref_any");                                  // refcount-aware release of the heap-backed inner value
    emitter.instruction("b __rt_ref_cell_free_deep_free");                      // free the cell storage after releasing the inner value

    emitter.label("__rt_ref_cell_free_deep_callable");
    emitter.instruction("ldr x0, [x0]");                                        // load the inner callable descriptor pointer from cell+0
    emitter.instruction("bl __rt_callable_descriptor_release");                 // release the callable descriptor owned by the cell

    emitter.label("__rt_ref_cell_free_deep_free");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the cell pointer after the inner-value release
    emitter.instruction("bl __rt_heap_free");                                   // free the reference-cell storage itself
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate the frame

    emitter.label("__rt_ref_cell_free_deep_done");
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the x86_64 Linux variant of `__rt_ref_cell_free_deep`.
fn emit_ref_cell_free_deep_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ref_cell_free_deep ---");
    emitter.label_global("__rt_ref_cell_free_deep");

    emitter.instruction("test rax, rax");                                       // skip null reference-cell pointers immediately
    emitter.instruction("jz __rt_ref_cell_free_deep_done");                     // null reference cells own no heap storage
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before spilling the cell pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved cell pointer
    emitter.instruction("sub rsp, 16");                                         // reserve local storage for the cell pointer across the release call
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the cell pointer across the inner-value release helper call
    emitter.instruction("mov r10, QWORD PTR [rax + 8]");                        // load the inner value-tag stored at cell+8
    emitter.instruction("cmp r10, 10");                                         // does the cell hold a callable descriptor?
    emitter.instruction("je __rt_ref_cell_free_deep_callable");                 // callable descriptors release through the descriptor helper
    emitter.instruction("cmp r10, 1");                                          // does the cell hold a persisted string?
    emitter.instruction("je __rt_ref_cell_free_deep_release");                  // strings release through the uniform dispatcher
    emitter.instruction("cmp r10, 4");                                          // is the inner value below the heap-backed value-tag range?
    emitter.instruction("jb __rt_ref_cell_free_deep_free");                     // scalar inner values (0/2/3) own no heap storage
    emitter.instruction("cmp r10, 7");                                          // is the inner value above the heap-backed value-tag range?
    emitter.instruction("ja __rt_ref_cell_free_deep_free");                     // non-heap inner tags (8/9/...) own no releasable storage here
    emitter.label("__rt_ref_cell_free_deep_release");
    emitter.instruction("mov rax, QWORD PTR [rax]");                            // load the inner value pointer from cell+0
    emitter.instruction("call __rt_decref_any");                                // refcount-aware release of the heap-backed inner value
    emitter.instruction("jmp __rt_ref_cell_free_deep_free");                    // free the cell storage after releasing the inner value

    emitter.label("__rt_ref_cell_free_deep_callable");
    emitter.instruction("mov rax, QWORD PTR [rax]");                            // load the inner callable descriptor pointer from cell+0
    emitter.instruction("call __rt_callable_descriptor_release");               // release the callable descriptor owned by the cell

    emitter.label("__rt_ref_cell_free_deep_free");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the cell pointer after the inner-value release
    emitter.instruction("call __rt_heap_free");                                 // free the reference-cell storage itself
    emitter.instruction("add rsp, 16");                                         // release the spill slot reserved for the cell pointer
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.label("__rt_ref_cell_free_deep_done");
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits `__rt_ref_cell_store`: whole-value store through a shared kind-7 reference cell.
///
/// Releases the cell's PRIOR inner value (tag-gated on the old `[cell+8]`, mirroring
/// `__rt_ref_cell_free_deep`'s inner-release: callable descriptors release through the
/// descriptor helper, strings and heap-backed tags 4..=7 through `__rt_decref_any`, and
/// scalar tags 0/2/3/8/9 own nothing), then writes the new `value` word to `[cell+0]` and
/// the new inner value-tag to `[cell+8]`. Used by the adopted kind-7 owner store path
/// (`store_value_to_ref_cell_as`) so a whole reassign of a reference-bound local that
/// CHANGES the inner type (int→array, int→string) stamps the new tag, letting
/// `__rt_deref_if_reference` read the value back with the correct tag. Null cells are a
/// defensive no-op.
///
/// # ABI
/// - ARM64: `x0` = cell pointer, `x1` = new inner value word, `x2` = new inner value-tag.
/// - x86_64: `rdi` = cell pointer, `rsi` = new inner value word, `rdx` = new inner value-tag.
pub fn emit_ref_cell_store(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ref_cell_store_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: ref_cell_store ---");
    emitter.label_global("__rt_ref_cell_store");

    emitter.instruction("cbz x0, __rt_ref_cell_store_done");                    // defensive no-op for a null reference cell
    emitter.instruction("sub sp, sp, #48");                                     // allocate a frame to spill cell, value, and tag across the release call
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up the new frame pointer
    emitter.instruction("stp x0, x1, [sp, #0]");                                // save the cell pointer and the new inner value word
    emitter.instruction("str x2, [sp, #16]");                                   // save the new inner value-tag

    // -- release the cell's prior inner value (tag-gated on the old [cell+8]) --
    emitter.instruction("ldr x9, [x0, #8]");                                    // load the prior inner value-tag stored at cell+8
    emitter.instruction("cmp x9, #10");                                         // does the cell hold a callable descriptor?
    emitter.instruction("b.eq __rt_ref_cell_store_callable");                   // callable descriptors release through the descriptor helper
    emitter.instruction("cmp x9, #1");                                          // does the cell hold a persisted string?
    emitter.instruction("b.eq __rt_ref_cell_store_release");                    // strings release through the uniform dispatcher
    emitter.instruction("cmp x9, #4");                                          // is the prior value below the heap-backed value-tag range?
    emitter.instruction("b.lo __rt_ref_cell_store_write");                      // scalar prior values (0/2/3) own no heap storage
    emitter.instruction("cmp x9, #7");                                          // is the prior value above the heap-backed value-tag range?
    emitter.instruction("b.hi __rt_ref_cell_store_write");                      // non-heap prior tags (8/9) own no releasable storage here
    emitter.label("__rt_ref_cell_store_release");
    emitter.instruction("ldr x0, [x0]");                                        // load the prior inner value pointer from cell+0
    emitter.instruction("bl __rt_decref_any");                                  // refcount-aware release of the heap-backed prior inner value
    emitter.instruction("b __rt_ref_cell_store_write");                         // proceed to install the new value and tag

    emitter.label("__rt_ref_cell_store_callable");
    emitter.instruction("ldr x0, [x0]");                                        // load the prior callable descriptor pointer from cell+0
    emitter.instruction("bl __rt_callable_descriptor_release");                 // release the callable descriptor previously owned by the cell

    // -- install the new inner value word and its value-tag --
    emitter.label("__rt_ref_cell_store_write");
    emitter.instruction("ldp x0, x1, [sp, #0]");                                // reload the cell pointer and the new inner value word
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the new inner value-tag
    emitter.instruction("str x1, [x0]");                                        // store the new inner value word at cell+0
    emitter.instruction("str x2, [x0, #8]");                                    // store the new inner value-tag at cell+8
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load the header that also describes the payload
    emitter.instruction("bfi x9, x2, #8, #7");                                  // keep the header payload descriptor equal to the new inner tag
    emitter.instruction("str x9, [x0, #-8]");                                   // publish the descriptor the generic cell release reads
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate the frame

    emitter.label("__rt_ref_cell_store_done");
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the x86_64 Linux variant of `__rt_ref_cell_store`.
fn emit_ref_cell_store_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ref_cell_store ---");
    emitter.label_global("__rt_ref_cell_store");

    emitter.instruction("test rdi, rdi");                                       // defensive check for a null reference cell
    emitter.instruction("jz __rt_ref_cell_store_done");                         // null cells own no inner storage to replace
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before spilling the cell pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the spill slots
    emitter.instruction("sub rsp, 32");                                         // reserve local storage for the cell pointer, value word, and tag
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the cell pointer across the inner-value release call
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the new inner value word across the release call
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the new inner value-tag across the release call

    // -- release the cell's prior inner value (tag-gated on the old [cell+8]) --
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // load the prior inner value-tag stored at cell+8
    emitter.instruction("cmp r10, 10");                                         // does the cell hold a callable descriptor?
    emitter.instruction("je __rt_ref_cell_store_callable");                     // callable descriptors release through the descriptor helper
    emitter.instruction("cmp r10, 1");                                          // does the cell hold a persisted string?
    emitter.instruction("je __rt_ref_cell_store_release");                      // strings release through the uniform dispatcher
    emitter.instruction("cmp r10, 4");                                          // is the prior value below the heap-backed value-tag range?
    emitter.instruction("jb __rt_ref_cell_store_write");                        // scalar prior values (0/2/3) own no heap storage
    emitter.instruction("cmp r10, 7");                                          // is the prior value above the heap-backed value-tag range?
    emitter.instruction("ja __rt_ref_cell_store_write");                        // non-heap prior tags (8/9) own no releasable storage here
    emitter.label("__rt_ref_cell_store_release");
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // load the prior inner value pointer from cell+0
    emitter.instruction("call __rt_decref_any");                                // refcount-aware release of the heap-backed prior inner value
    emitter.instruction("jmp __rt_ref_cell_store_write");                       // proceed to install the new value and tag

    emitter.label("__rt_ref_cell_store_callable");
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // load the prior callable descriptor pointer from cell+0
    emitter.instruction("call __rt_callable_descriptor_release");               // release the callable descriptor previously owned by the cell

    // -- install the new inner value word and its value-tag --
    emitter.label("__rt_ref_cell_store_write");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the cell pointer after the inner-value release
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the new inner value word after the release
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload the new inner value-tag after the release
    emitter.instruction("mov QWORD PTR [rdi], rsi");                            // store the new inner value word at cell+0
    emitter.instruction("mov QWORD PTR [rdi + 8], rdx");                        // store the new inner value-tag at cell+8
    emitter.instruction("mov r10, QWORD PTR [rdi - 8]");                        // load the header that also describes the payload
    emitter.instruction("and r10, -32513");                                     // clear header bits 8-14, the payload descriptor
    emitter.instruction("and rdx, 0x7f");                                       // keep the descriptor within its seven header bits
    emitter.instruction("shl rdx, 8");                                          // position the new inner tag as the payload descriptor
    emitter.instruction("or r10, rdx");                                         // keep the header payload descriptor equal to the new inner tag
    emitter.instruction("mov QWORD PTR [rdi - 8], r10");                        // publish the descriptor the generic cell release reads
    emitter.instruction("add rsp, 32");                                         // release the spill slots reserved for the cell, value, and tag
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning

    emitter.label("__rt_ref_cell_store_done");
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits `__rt_deref_if_reference`: normalize a fetched hash element that may be a reference.
///
/// If the runtime value-tag equals 11 (Reference), the value word is a kind-7 cell pointer; this
/// helper replaces the value with the cell's inner value (`cell+0`) and the tag with the inner
/// value-tag (`cell+8`). Otherwise it is a no-op. It is a leaf routine (no nested calls) so the
/// read sites can call it right after `__rt_hash_get` without extra register preservation.
///
/// # ABI (matches `__rt_hash_get`'s value/tag output)
/// - ARM64: in/out `x1` = value_lo, `x3` = value_tag; clobbers `x9`. `x2` (value_hi) is left as-is
///   because reference cells hold single-word inner values.
/// - x86_64: in/out `rdi` = value_lo, `rcx` = value_tag; clobbers `r10`. `rsi` left as-is.
pub fn emit_deref_if_reference(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emitter.blank();
        emitter.comment("--- runtime: deref_if_reference ---");
        emitter.label_global("__rt_deref_if_reference");
        emitter.instruction("cmp rcx, 11");                                     // is this fetched element a reference (value-tag 11)?
        emitter.instruction("jne __rt_deref_if_reference_done");                // ordinary values pass through unchanged
        emitter.instruction("mov r10, rdi");                                    // r10 = kind-7 reference cell pointer
        emitter.instruction("mov rdi, QWORD PTR [r10]");                        // replace value_lo with the cell's inner value at cell+0
        emitter.instruction("mov rcx, QWORD PTR [r10 + 8]");                    // replace value_tag with the cell's inner value-tag at cell+8
        emitter.label("__rt_deref_if_reference_done");
        emitter.instruction("ret");                                             // return the normalized (value, tag) pair
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: deref_if_reference ---");
    emitter.label_global("__rt_deref_if_reference");
    emitter.instruction("cmp x3, #11");                                         // is this fetched element a reference (value-tag 11)?
    emitter.instruction("b.ne __rt_deref_if_reference_done");                   // ordinary values pass through unchanged
    emitter.instruction("mov x9, x1");                                          // x9 = kind-7 reference cell pointer
    emitter.instruction("ldr x1, [x9]");                                        // replace value_lo with the cell's inner value at cell+0
    emitter.instruction("ldr x3, [x9, #8]");                                    // replace value_tag with the cell's inner value-tag at cell+8
    emitter.label("__rt_deref_if_reference_done");
    emitter.instruction("ret");                                                 // return the normalized (value, tag) pair
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::{Platform, Target};

    /// Emits the whole reference-cell surface for a target and returns the assembly text.
    fn emit_all(target: Target) -> String {
        let mut emitter = Emitter::new(target);
        emit_ref_cell_alloc(&mut emitter);
        emit_ref_cell_incref(&mut emitter);
        emit_ref_cell_decref(&mut emitter);
        emit_ref_cell_free_deep(&mut emitter);
        emit_ref_cell_store(&mut emitter);
        emitter.output()
    }

    /// Verifies the ARM64 reference-cell helpers export every global symbol and stamp heap kind 7.
    #[test]
    fn test_arm64_ref_cell_globals_and_kind() {
        let asm = emit_all(Target::new(Platform::MacOS, Arch::AArch64));
        for sym in [
            "__rt_ref_cell_alloc",
            "__rt_ref_cell_incref",
            "__rt_ref_cell_decref",
            "__rt_ref_cell_free_deep",
        ] {
            assert!(asm.contains(&format!(".globl {}\n", sym)), "missing {sym}");
        }
        // The managed-cell kind and the payload descriptor are stamped into the header, and
        // incref/decref reuse the generic + free paths.
        assert!(asm.contains("lsl x9, x9, #8"));
        assert!(asm.contains(&format!("orr x9, x9, #{}", REFERENCE_CELL_HEAP_KIND)));
        assert!(asm.contains("str x9, [x0, #-8]"));
        assert!(asm.contains("b __rt_incref"));
        assert!(asm.contains("b __rt_ref_cell_free_deep"));
        // Array-element cells carry the address canary `__rt_reference_cell_owner` proves.
        assert!(asm.contains("str x9, [x0, #16]"), "array-element cells stamp their canary");
    }

    /// Verifies the x86_64 reference-cell helpers export every global symbol and carry the heap marker.
    #[test]
    fn test_x86_64_ref_cell_globals_and_kind() {
        let asm = emit_all(Target::new(Platform::Linux, Arch::X86_64));
        for sym in [
            "__rt_ref_cell_alloc",
            "__rt_ref_cell_incref",
            "__rt_ref_cell_decref",
            "__rt_ref_cell_free_deep",
        ] {
            assert!(asm.contains(&format!(".globl {}\n", sym)), "missing {sym}");
        }
        // The x86_64 kind word combines the heap marker with the managed reference-cell kind, the
        // same kind the AArch64 emitter stamps above. Built through the canonical helper, never hand-typed:
        // a transposed marker assembles fine and makes every refcount a silent no-op (#482).
        assert!(asm.contains(&format!(
            "mov r10, 0x{:x}",
            x86_64_heap_kind_word(REFERENCE_CELL_HEAP_KIND)
        )));
        assert!(asm.contains("jmp __rt_incref"));
        assert!(asm.contains("jmp __rt_ref_cell_free_deep"));
        assert!(asm.contains("mov QWORD PTR [rax + 16], r10"), "array-element cells stamp their canary");
    }
}
