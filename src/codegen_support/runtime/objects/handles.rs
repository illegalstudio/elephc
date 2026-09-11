//! Purpose:
//! Emits the PHP OBJECT HANDLE pool: `__rt_object_handle_acquire`,
//! `__rt_object_handle_of` and `__rt_object_handle_release`. These give every PHP
//! object the small dense integer that `spl_object_id()` returns and that
//! `var_dump()` prints as `object(C)#N (n) {`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::objects`.
//! - Every object-allocation site (`emit_object_allocation`, `__rt_stdclass_new`,
//!   `__rt_new_by_name`, the enum/exception/SPL/reflection allocators, …) calls
//!   `__rt_object_handle_acquire` immediately after stamping the heap kind.
//! - `__rt_heap_free` calls `__rt_object_handle_release` on every block it
//!   reclaims, which is the single release chokepoint.
//!
//! Key details:
//! - STORAGE IS A SIDE TABLE, NOT THE HEADER. The 16-byte allocator header
//!   `[size:4][refcount:4][kind:8]` has no spare field, and its spare BITS are
//!   both too few and not durable (AArch64 cycle collection masks the kind word
//!   to its low 16 bits every pass, and bits 32..63 carry the x86_64 `ELPH`
//!   magic), which would cap handles at 255. Widening the header would mean
//!   auditing ~1727 hand-written `[ptr-8]` / `[ptr-12]` / `+16` references, and
//!   moving the handle into the payload would shift the `8 + index * 16` property
//!   layout baked into every class descriptor. The side table touches neither.
//! - THE SIDE TABLE IS DIRECT-MAPPED, NOT A HASH. `_obj_handle_index` holds one
//!   `u32` handle per 16-byte granule of `_heap_buf`. Two distinct heap blocks can
//!   never share a granule: the smallest possible block is 16 header bytes plus
//!   the allocator's 8-byte minimum payload = 24 bytes, so any two live block
//!   payload addresses differ by at least 24 > 16. That makes the mapping exact
//!   with no probing, no collisions, no capacity failure mode and no rehash — and
//!   it needs no allocation of its own, so it cannot recurse into the allocator.
//! - IT HOLDS NO OWNERSHIP. The table stores a handle keyed BY POSITION; it never
//!   stores an object pointer, never increments a refcount and is never scanned as
//!   a GC root, so it cannot keep an object alive or resurrect a freed one. A slot
//!   is cleared the moment its block is freed, and `acquire` unconditionally
//!   overwrites the slot it targets, so a granule reused by a later allocation can
//!   never report a previous tenant's handle.
//! - REUSE IS LIFO, LIKE php-src. Released handles are pushed onto
//!   `_obj_handle_free` and popped before a fresh handle is minted, so
//!   `$a = new P(); $b = new P(); unset($a); unset($b);` hands the next two objects
//!   `#2` then `#1`, exactly as PHP 8.5 does.
//! - FREE-STACK CAPACITY CANNOT OVERFLOW. The number of distinct handles ever
//!   minted equals the peak number of simultaneously live objects (reuse pops
//!   before minting), and each live object owns at least 24 heap bytes, so at most
//!   `heap_size / 24` handles can exist. `_obj_handle_free` is sized
//!   `heap_size / 24 + 16` entries, which is that bound plus slack.
//! - HANDLE 0 MEANS "NO HANDLE". A block that never went through
//!   `__rt_object_handle_acquire` reads back 0, which renders as `#0` rather than
//!   as a plausible-looking small integer — a missed allocation site is meant to
//!   be glaring, not silently absorbed.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Returns the number of `u32` slots in `_obj_handle_index` for a heap of
/// `heap_size` bytes: one slot per 16-byte granule, plus two slots of slack so the
/// last granule of a full heap is always addressable.
pub(crate) fn object_handle_index_slots(heap_size: usize) -> usize {
    heap_size / 16 + 2
}

/// Returns the number of `u32` slots in `_obj_handle_free` for a heap of
/// `heap_size` bytes. See the module preamble: the live-object count — and hence
/// the number of distinct handles and the deepest possible free stack — is bounded
/// by `heap_size / 24` because the smallest object block is 24 bytes.
pub(crate) fn object_handle_free_slots(heap_size: usize) -> usize {
    heap_size / 24 + 16
}

/// Emits `__rt_object_handle_acquire`, `__rt_object_handle_of` and
/// `__rt_object_handle_release` for the active target.
pub(crate) fn emit_object_handles(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emit_object_handle_acquire_arm64(emitter);
            emit_object_handle_of_arm64(emitter);
            emit_object_handle_release_arm64(emitter);
            emit_spl_object_hash_arm64(emitter);
        }
        Arch::X86_64 => {
            emit_object_handle_acquire_x86_64(emitter);
            emit_object_handle_of_x86_64(emitter);
            emit_object_handle_release_x86_64(emitter);
            emit_spl_object_hash_x86_64(emitter);
        }
    }
}

/// Emits the AArch64 `__rt_object_handle_acquire`.
///
/// Input: `x0` = freshly allocated object payload pointer.
/// Output: none — `x0` and every other register are preserved, so the helper can be
/// dropped straight after a kind stamp without disturbing the allocation site.
/// Contains no nested `bl`, so it returns with `ret` and never clobbers `x30`.
fn emit_object_handle_acquire_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: object_handle_acquire ---");
    emitter.label_global("__rt_object_handle_acquire");

    emitter.instruction("stp x9, x10, [sp, #-64]!");                            // preserve the scratch registers this helper reuses
    emitter.instruction("stp x11, x12, [sp, #16]");                             // preserve the free-stack scratch pair
    emitter.instruction("stp x13, x14, [sp, #32]");                             // preserve the table-address and handle scratch pair
    emitter.instruction("str x15, [sp, #48]");                                  // preserve the fresh-handle increment scratch

    emitter.instruction("cbz x0, __rt_object_handle_acquire_done");             // a null allocation carries no identity
    crate::codegen_support::runtime::ctx::emit_heap_base_address(emitter, "x9");
    emitter.instruction("subs x10, x0, x9");                                    // x10 = payload offset from the heap base
    emitter.instruction("b.lo __rt_object_handle_acquire_done");                // reject pointers that do not belong to the managed heap
    emitter.instruction("lsr x10, x10, #4");                                    // x10 = 16-byte granule index of this block

    abi::emit_symbol_address(emitter, "x11", "_obj_handle_free_top");
    emitter.instruction("ldr x12, [x11]");                                      // x12 = number of released handles waiting for reuse
    emitter.instruction("cbz x12, __rt_object_handle_acquire_fresh");           // no released handle to reuse — mint the next fresh one
    emitter.instruction("sub x12, x12, #1");                                    // pop the most recently released handle (LIFO, like php-src)
    emitter.instruction("str x12, [x11]");                                      // publish the shortened released-handle stack
    abi::emit_symbol_address(emitter, "x13", "_obj_handle_free");
    emitter.instruction("ldr w14, [x13, x12, lsl #2]");                         // x14 = the reused handle value
    emitter.instruction("b __rt_object_handle_acquire_store");                  // record the reused handle for this block

    emitter.label("__rt_object_handle_acquire_fresh");
    abi::emit_symbol_address(emitter, "x13", "_obj_handle_next");
    emitter.instruction("ldr x14, [x13]");                                      // x14 = next never-used handle (the pool starts at 1)
    emitter.instruction("add x15, x14, #1");                                    // advance the never-used handle cursor
    emitter.instruction("str x15, [x13]");                                      // publish the advanced cursor

    emitter.label("__rt_object_handle_acquire_store");
    abi::emit_symbol_address(emitter, "x13", "_obj_handle_index");
    emitter.instruction("str w14, [x13, x10, lsl #2]");                         // bind this heap granule to the acquired handle

    emitter.label("__rt_object_handle_acquire_done");
    emitter.instruction("ldr x15, [sp, #48]");                                  // restore the fresh-handle increment scratch
    emitter.instruction("ldp x13, x14, [sp, #32]");                             // restore the table-address and handle scratch pair
    emitter.instruction("ldp x11, x12, [sp, #16]");                             // restore the free-stack scratch pair
    emitter.instruction("ldp x9, x10, [sp], #64");                              // restore the remaining scratch registers
    emitter.instruction("ret");                                                 // return with x0 and every scratch register untouched
}

/// Emits the AArch64 `__rt_object_handle_of`.
///
/// Input: `x0` = object payload pointer. Output: `x0` = the PHP object handle, or
/// `0` when the pointer is null, outside the managed heap, or belongs to a block
/// that never acquired a handle.
fn emit_object_handle_of_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: object_handle_of ---");
    emitter.label_global("__rt_object_handle_of");

    emitter.instruction("cbz x0, __rt_object_handle_of_zero");                  // null objects have no handle
    crate::codegen_support::runtime::ctx::emit_heap_base_address(emitter, "x9");
    emitter.instruction("cmp x0, x9");                                          // is the pointer below the managed heap?
    emitter.instruction("b.lo __rt_object_handle_of_zero");                     // static and foreign pointers carry no handle
    crate::codegen_support::runtime::ctx::emit_heap_off_load(emitter, "x10"); // x10 = current heap bump offset (ctx-relative in ctx mode)
    emitter.instruction("add x10, x9, x10");                                    // compute the live heap end
    emitter.instruction("cmp x0, x10");                                         // is the pointer at or beyond the live heap end?
    emitter.instruction("b.hs __rt_object_handle_of_zero");                     // pointers past the live heap carry no handle
    emitter.instruction("sub x10, x0, x9");                                     // x10 = payload offset from the heap base
    emitter.instruction("lsr x10, x10, #4");                                    // x10 = 16-byte granule index of this block
    abi::emit_symbol_address(emitter, "x11", "_obj_handle_index");
    emitter.instruction("ldr w0, [x11, x10, lsl #2]");                          // load the handle bound to this granule
    emitter.instruction("ret");                                                 // return the handle to the caller

    emitter.label("__rt_object_handle_of_zero");
    emitter.instruction("mov x0, #0");                                          // report "no handle" for anything that never acquired one
    emitter.instruction("ret");                                                 // return zero to the caller
}

/// Emits the AArch64 `__rt_object_handle_release`.
///
/// Input: `x0` = payload pointer of a block being returned to the allocator.
/// Output: none — `x0` and every other register are preserved. Blocks that never
/// held a handle (strings, arrays, hashes, Mixed cells, descriptors) fall out after
/// a single load. Contains no nested `bl`, so `x30` is never clobbered.
fn emit_object_handle_release_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: object_handle_release ---");
    emitter.label_global("__rt_object_handle_release");

    emitter.instruction("stp x9, x10, [sp, #-48]!");                            // preserve the scratch registers this helper reuses
    emitter.instruction("stp x11, x12, [sp, #16]");                             // preserve the free-stack scratch pair
    emitter.instruction("stp x13, x14, [sp, #32]");                             // preserve the table-address and handle scratch pair

    emitter.instruction("cbz x0, __rt_object_handle_release_done");             // null payloads never held a handle
    crate::codegen_support::runtime::ctx::emit_heap_base_address(emitter, "x9");
    emitter.instruction("subs x10, x0, x9");                                    // x10 = payload offset from the heap base
    emitter.instruction("b.lo __rt_object_handle_release_done");                // pointers below the heap never held a handle
    emitter.instruction("lsr x10, x10, #4");                                    // x10 = 16-byte granule index of this block
    abi::emit_symbol_address(emitter, "x13", "_obj_handle_index");
    emitter.instruction("ldr w14, [x13, x10, lsl #2]");                         // load the handle bound to this granule
    emitter.instruction("cbz w14, __rt_object_handle_release_done");            // non-object blocks leave the pool untouched
    emitter.instruction("str wzr, [x13, x10, lsl #2]");                         // unbind the granule so a reused block starts handle-less

    abi::emit_symbol_address(emitter, "x11", "_obj_handle_free_top");
    emitter.instruction("ldr x12, [x11]");                                      // x12 = current released-handle stack depth
    abi::emit_symbol_address(emitter, "x13", "_obj_handle_free");
    emitter.instruction("str w14, [x13, x12, lsl #2]");                         // push the released handle for LIFO reuse
    emitter.instruction("add x12, x12, #1");                                    // account for the pushed handle
    emitter.instruction("str x12, [x11]");                                      // publish the deepened released-handle stack

    emitter.label("__rt_object_handle_release_done");
    emitter.instruction("ldp x13, x14, [sp, #32]");                             // restore the table-address and handle scratch pair
    emitter.instruction("ldp x11, x12, [sp, #16]");                             // restore the free-stack scratch pair
    emitter.instruction("ldp x9, x10, [sp], #48");                              // restore the remaining scratch registers
    emitter.instruction("ret");                                                 // return with x0 and every scratch register untouched
}

/// Emits the x86_64 `__rt_object_handle_acquire`.
///
/// Input: `rax` = freshly allocated object payload pointer. Every register,
/// including `rax`, is preserved so the helper can be dropped straight after a kind
/// stamp.
fn emit_object_handle_acquire_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: object_handle_acquire ---");
    emitter.label_global("__rt_object_handle_acquire");

    emitter.instruction("push rcx");                                            // preserve the scratch registers this helper reuses
    emitter.instruction("push rdx");                                            // preserve the granule-index scratch
    emitter.instruction("push r8");                                             // preserve the table-address scratch
    emitter.instruction("push r9");                                             // preserve the free-stack scratch
    emitter.instruction("push r10");                                            // preserve the handle scratch
    emitter.instruction("push r11");                                            // preserve the heap-base scratch

    emitter.instruction("test rax, rax");                                       // is this a null allocation?
    emitter.instruction("jz __rt_object_handle_acquire_done");                  // a null allocation carries no identity
    crate::codegen_support::runtime::ctx::emit_heap_base_address(emitter, "r11");
    emitter.instruction("cmp rax, r11");                                        // is the pointer below the managed heap?
    emitter.instruction("jb __rt_object_handle_acquire_done");                  // reject pointers that do not belong to the managed heap
    emitter.instruction("mov rdx, rax");                                        // copy the payload pointer before deriving its granule
    emitter.instruction("sub rdx, r11");                                        // rdx = payload offset from the heap base
    emitter.instruction("shr rdx, 4");                                          // rdx = 16-byte granule index of this block

    abi::emit_symbol_address(emitter, "r8", "_obj_handle_free_top");
    emitter.instruction("mov rcx, QWORD PTR [r8]");                             // rcx = number of released handles waiting for reuse
    emitter.instruction("test rcx, rcx");                                       // is the released-handle stack empty?
    emitter.instruction("jz __rt_object_handle_acquire_fresh");                 // no released handle to reuse — mint the next fresh one
    emitter.instruction("sub rcx, 1");                                          // pop the most recently released handle (LIFO, like php-src)
    emitter.instruction("mov QWORD PTR [r8], rcx");                             // publish the shortened released-handle stack
    abi::emit_symbol_address(emitter, "r9", "_obj_handle_free");
    emitter.instruction("mov r10d, DWORD PTR [r9 + rcx * 4]");                  // r10 = the reused handle value
    emitter.instruction("jmp __rt_object_handle_acquire_store");                // record the reused handle for this block

    emitter.label("__rt_object_handle_acquire_fresh");
    abi::emit_symbol_address(emitter, "r9", "_obj_handle_next");
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // r10 = next never-used handle (the pool starts at 1)
    emitter.instruction("mov rcx, r10");                                        // copy it before advancing the never-used cursor
    emitter.instruction("add rcx, 1");                                          // advance the never-used handle cursor
    emitter.instruction("mov QWORD PTR [r9], rcx");                             // publish the advanced cursor

    emitter.label("__rt_object_handle_acquire_store");
    abi::emit_symbol_address(emitter, "r8", "_obj_handle_index");
    emitter.instruction("mov DWORD PTR [r8 + rdx * 4], r10d");                  // bind this heap granule to the acquired handle

    emitter.label("__rt_object_handle_acquire_done");
    emitter.instruction("pop r11");                                             // restore the heap-base scratch
    emitter.instruction("pop r10");                                             // restore the handle scratch
    emitter.instruction("pop r9");                                              // restore the free-stack scratch
    emitter.instruction("pop r8");                                              // restore the table-address scratch
    emitter.instruction("pop rdx");                                             // restore the granule-index scratch
    emitter.instruction("pop rcx");                                             // restore the remaining scratch register
    emitter.instruction("ret");                                                 // return with rax and every scratch register untouched
}

/// Emits the x86_64 `__rt_object_handle_of`.
///
/// Input: `rax` = object payload pointer. Output: `rax` = the PHP object handle, or
/// `0` when the pointer never acquired one.
fn emit_object_handle_of_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: object_handle_of ---");
    emitter.label_global("__rt_object_handle_of");

    emitter.instruction("test rax, rax");                                       // is the candidate pointer null?
    emitter.instruction("jz __rt_object_handle_of_zero");                       // null objects have no handle
    crate::codegen_support::runtime::ctx::emit_heap_base_address(emitter, "r10");
    emitter.instruction("cmp rax, r10");                                        // is the pointer below the managed heap?
    emitter.instruction("jb __rt_object_handle_of_zero");                       // static and foreign pointers carry no handle
    crate::codegen_support::runtime::ctx::emit_heap_off_load(emitter, "r11"); // r11 = current heap offset (ctx-relative in ctx mode)
    emitter.instruction("add r11, r10");                                        // compute the live heap end
    emitter.instruction("cmp rax, r11");                                        // is the pointer at or beyond the live heap end?
    emitter.instruction("jae __rt_object_handle_of_zero");                      // pointers past the live heap carry no handle
    emitter.instruction("mov r11, rax");                                        // copy the payload pointer before deriving its granule
    emitter.instruction("sub r11, r10");                                        // r11 = payload offset from the heap base
    emitter.instruction("shr r11, 4");                                          // r11 = 16-byte granule index of this block
    abi::emit_symbol_address(emitter, "r10", "_obj_handle_index");
    emitter.instruction("mov eax, DWORD PTR [r10 + r11 * 4]");                  // load the handle bound to this granule
    emitter.instruction("ret");                                                 // return the handle to the caller

    emitter.label("__rt_object_handle_of_zero");
    emitter.instruction("xor eax, eax");                                        // report "no handle" for anything that never acquired one
    emitter.instruction("ret");                                                 // return zero to the caller
}

/// Emits the x86_64 `__rt_object_handle_release`.
///
/// Input: `rax` = payload pointer of a block being returned to the allocator.
/// Every register, including `rax`, is preserved.
fn emit_object_handle_release_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: object_handle_release ---");
    emitter.label_global("__rt_object_handle_release");

    emitter.instruction("push rcx");                                            // preserve the scratch registers this helper reuses
    emitter.instruction("push rdx");                                            // preserve the granule-index scratch
    emitter.instruction("push r8");                                             // preserve the table-address scratch
    emitter.instruction("push r9");                                             // preserve the free-stack scratch
    emitter.instruction("push r10");                                            // preserve the handle scratch
    emitter.instruction("push r11");                                            // preserve the heap-base scratch

    emitter.instruction("test rax, rax");                                       // is the released payload pointer null?
    emitter.instruction("jz __rt_object_handle_release_done");                  // null payloads never held a handle
    crate::codegen_support::runtime::ctx::emit_heap_base_address(emitter, "r11");
    emitter.instruction("cmp rax, r11");                                        // is the pointer below the managed heap?
    emitter.instruction("jb __rt_object_handle_release_done");                  // pointers below the heap never held a handle
    emitter.instruction("mov rdx, rax");                                        // copy the payload pointer before deriving its granule
    emitter.instruction("sub rdx, r11");                                        // rdx = payload offset from the heap base
    emitter.instruction("shr rdx, 4");                                          // rdx = 16-byte granule index of this block
    abi::emit_symbol_address(emitter, "r8", "_obj_handle_index");
    emitter.instruction("mov r10d, DWORD PTR [r8 + rdx * 4]");                  // load the handle bound to this granule
    emitter.instruction("test r10d, r10d");                                     // did this block ever acquire a handle?
    emitter.instruction("jz __rt_object_handle_release_done");                  // non-object blocks leave the pool untouched
    emitter.instruction("mov DWORD PTR [r8 + rdx * 4], 0");                     // unbind the granule so a reused block starts handle-less

    abi::emit_symbol_address(emitter, "r8", "_obj_handle_free_top");
    emitter.instruction("mov rcx, QWORD PTR [r8]");                             // rcx = current released-handle stack depth
    abi::emit_symbol_address(emitter, "r9", "_obj_handle_free");
    emitter.instruction("mov DWORD PTR [r9 + rcx * 4], r10d");                  // push the released handle for LIFO reuse
    emitter.instruction("add rcx, 1");                                          // account for the pushed handle
    emitter.instruction("mov QWORD PTR [r8], rcx");                             // publish the deepened released-handle stack

    emitter.label("__rt_object_handle_release_done");
    emitter.instruction("pop r11");                                             // restore the heap-base scratch
    emitter.instruction("pop r10");                                             // restore the handle scratch
    emitter.instruction("pop r9");                                              // restore the free-stack scratch
    emitter.instruction("pop r8");                                              // restore the table-address scratch
    emitter.instruction("pop rdx");                                             // restore the granule-index scratch
    emitter.instruction("pop rcx");                                             // restore the remaining scratch register
    emitter.instruction("ret");                                                 // return with rax and every scratch register untouched
}

/// Emits the AArch64 `__rt_spl_object_hash`.
///
/// PHP's `spl_object_hash()` is the object handle rendered as 16 zero-padded hex
/// digits followed by 16 more zeros — `spl_object_id($o) === 1` gives
/// `"00000000000000010000000000000000"` (verified against 8.5.6). Deriving it from
/// the same handle keeps `spl_object_hash`, `spl_object_id` and `var_dump`'s `#N`
/// mutually consistent.
///
/// Input: `x0` = object pointer. Output: `x1` = byte pointer, `x2` = 32, matching
/// `__rt_itoa`'s string-return convention. Writes into the `_concat_buf` scratch.
fn emit_spl_object_hash_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: spl_object_hash ---");
    emitter.label_global("__rt_spl_object_hash");

    emitter.instruction("sub sp, sp, #16");                                     // allocate the helper frame
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer

    emitter.instruction("bl __rt_object_handle_of");                            // x0 = this object's PHP handle
    crate::codegen_support::runtime::ctx::emit_concat_off_load(emitter, "x8");
    crate::codegen_support::runtime::ctx::emit_concat_buf_address(emitter, "x7");
    emitter.instruction("add x9, x7, x8");                                      // x9 = write cursor for the 32 hash bytes
    emitter.instruction("add x10, x8, #32");                                    // reserve exactly 32 scratch bytes
    crate::codegen_support::runtime::ctx::emit_concat_off_store(emitter, "x10"); // publish the advanced scratch offset (ctx-relative in ctx mode)

    emitter.instruction("mov x11, #15");                                        // start at the most significant of the 16 nibbles
    emitter.label("__rt_spl_object_hash_nibble");
    emitter.instruction("lsl x12, x11, #2");                                    // x12 = bit shift for this nibble
    emitter.instruction("lsr x13, x0, x12");                                    // move the selected nibble into the low bits
    emitter.instruction("and x13, x13, #0xf");                                  // isolate the nibble value
    emitter.instruction("cmp x13, #10");                                        // is this nibble a decimal digit?
    emitter.instruction("b.lo __rt_spl_object_hash_digit");                     // yes — render it as '0'..'9'
    emitter.instruction("add x13, x13, #87");                                   // render 10..15 as lowercase 'a'..'f'
    emitter.instruction("b __rt_spl_object_hash_store");                        // store the rendered hex character
    emitter.label("__rt_spl_object_hash_digit");
    emitter.instruction("add x13, x13, #48");                                   // render 0..9 as '0'..'9'
    emitter.label("__rt_spl_object_hash_store");
    emitter.instruction("strb w13, [x9], #1");                                  // write the hex character and advance the cursor
    emitter.instruction("subs x11, x11, #1");                                   // step down to the next nibble
    emitter.instruction("b.ge __rt_spl_object_hash_nibble");                    // keep rendering while nibbles remain

    emitter.instruction("mov w13, #48");                                        // PHP pads the second half with 16 literal '0' bytes
    emitter.instruction("mov x11, #16");                                        // 16 padding bytes remain
    emitter.label("__rt_spl_object_hash_pad");
    emitter.instruction("strb w13, [x9], #1");                                  // write one padding byte
    emitter.instruction("subs x11, x11, #1");                                   // count the padding byte
    emitter.instruction("b.ne __rt_spl_object_hash_pad");                       // keep padding to the full 32 bytes

    emitter.instruction("add x1, x7, x8");                                      // x1 = pointer to the first hash byte
    emitter.instruction("mov x2, #32");                                         // x2 = PHP's fixed spl_object_hash length
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the (ptr, len) string pair
}

/// Emits the x86_64 `__rt_spl_object_hash`.
///
/// Input: `rax` = object pointer. Output: `rax` = byte pointer, `rdx` = 32, matching
/// `__rt_itoa`'s string-return convention on this target.
fn emit_spl_object_hash_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: spl_object_hash ---");
    emitter.label_global("__rt_spl_object_hash");

    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("push rbx");                                            // preserve the callee-saved cursor register
    emitter.instruction("sub rsp, 8");                                          // restore the SysV 16-byte call alignment the odd push broke

    emitter.instruction("call __rt_object_handle_of");                          // rax = this object's PHP handle
    crate::codegen_support::runtime::ctx::emit_concat_off_load(emitter, "r9");
    crate::codegen_support::runtime::ctx::emit_concat_buf_address(emitter, "r10");
    emitter.instruction("mov rbx, r10");                                        // rbx = write cursor for the 32 hash bytes
    emitter.instruction("add rbx, r9");                                         // advance the cursor to the reserved scratch slot
    emitter.instruction("mov r11, r9");                                         // copy the offset before reserving space
    emitter.instruction("add r11, 32");                                         // reserve exactly 32 scratch bytes
    crate::codegen_support::runtime::ctx::emit_concat_off_store(emitter, "r11"); // publish the advanced scratch offset

    emitter.instruction("mov r8, 15");                                          // start at the most significant of the 16 nibbles
    emitter.label("__rt_spl_object_hash_nibble");
    emitter.instruction("mov rcx, r8");                                         // variable shifts on x86_64 read the count from cl
    emitter.instruction("shl rcx, 2");                                          // rcx = bit shift for this nibble
    emitter.instruction("mov r11, rax");                                        // copy the handle before shifting it
    emitter.instruction("shr r11, cl");                                         // move the selected nibble into the low bits
    emitter.instruction("and r11, 0xf");                                        // isolate the nibble value
    emitter.instruction("cmp r11, 10");                                         // is this nibble a decimal digit?
    emitter.instruction("jb __rt_spl_object_hash_digit");                       // yes — render it as '0'..'9'
    emitter.instruction("add r11, 87");                                         // render 10..15 as lowercase 'a'..'f'
    emitter.instruction("jmp __rt_spl_object_hash_store");                      // store the rendered hex character
    emitter.label("__rt_spl_object_hash_digit");
    emitter.instruction("add r11, 48");                                         // render 0..9 as '0'..'9'
    emitter.label("__rt_spl_object_hash_store");
    emitter.instruction("mov BYTE PTR [rbx], r11b");                            // write the hex character
    emitter.instruction("add rbx, 1");                                          // advance the write cursor
    emitter.instruction("sub r8, 1");                                           // step down to the next nibble
    emitter.instruction("jns __rt_spl_object_hash_nibble");                     // keep rendering while nibbles remain

    emitter.instruction("mov r8, 16");                                          // PHP pads the second half with 16 literal '0' bytes
    emitter.label("__rt_spl_object_hash_pad");
    emitter.instruction("mov BYTE PTR [rbx], 48");                              // write one padding byte
    emitter.instruction("add rbx, 1");                                          // advance the write cursor
    emitter.instruction("sub r8, 1");                                           // count the padding byte
    emitter.instruction("jnz __rt_spl_object_hash_pad");                        // keep padding to the full 32 bytes

    crate::codegen_support::runtime::ctx::emit_concat_buf_address(emitter, "rax");
    emitter.instruction("add rax, r9");                                         // rax = pointer to the first hash byte
    emitter.instruction("mov rdx, 32");                                         // rdx = PHP's fixed spl_object_hash length
    emitter.instruction("add rsp, 8");                                          // release the alignment pad
    emitter.instruction("pop rbx");                                             // restore the callee-saved cursor register
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the (ptr, len) string pair
}

/// Emits the call sequence that binds a PHP object handle to the object pointer
/// currently sitting in the integer result register.
///
/// Every object-allocation site uses this immediately after stamping the heap kind,
/// which is what keeps handle numbering in PHP's ALLOCATION order. The helper
/// preserves all registers, so the only cost at the call site is the branch itself.
pub(crate) fn emit_acquire_object_handle(emitter: &mut Emitter) {
    abi::emit_call_label(emitter, "__rt_object_handle_acquire");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Locks the direct-mapped side-table sizing: one `u32` per 16-byte granule for
    /// the index, and the proven `heap_size / 24` live-object bound (plus slack) for
    /// the released-handle stack.
    #[test]
    fn side_table_sizes_follow_the_heap() {
        assert_eq!(object_handle_index_slots(8 * 1024 * 1024), 524_290);
        assert_eq!(object_handle_free_slots(8 * 1024 * 1024), 349_541);
        // The free stack must always outrank the largest possible live-object count.
        for heap in [1024usize, 65_536, 8 * 1024 * 1024, 512 * 1024 * 1024] {
            assert!(object_handle_free_slots(heap) > heap / 24);
            assert!(object_handle_index_slots(heap) > heap / 16);
        }
    }

    /// Verifies both architectures emit all three pool entry points.
    #[test]
    fn emits_the_pool_entry_points_on_both_targets() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_object_handles(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_object_handle_acquire:"), "{target:?}");
            assert!(asm.contains("__rt_object_handle_of:"), "{target:?}");
            assert!(asm.contains("__rt_object_handle_release:"), "{target:?}");
        }
    }
}
