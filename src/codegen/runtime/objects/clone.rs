//! Purpose:
//! Emits the `__rt_object_clone`, `__rt_object_clone_payload`, and
//! `__rt_call_object_clone_method` runtime helpers that back the PHP `clone`
//! expression for the active target.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via
//!   `crate::codegen::runtime::objects`.
//!
//! Key details:
//! - `__rt_object_clone` accepts either a raw object pointer (heap kind 4) or a
//!   boxed Mixed-cell pointer (heap kind 5) and returns the same representation
//!   it was given: a fresh object for an `Object` operand, or a fresh Mixed box
//!   wrapping the clone for a boxed operand. This lets the single EIR
//!   `Op::ObjectClone` lowering handle both static object and mixed/union
//!   receivers (Symfony clones values read out of untyped arrays, which are
//!   `Mixed`-typed element cells).
//! - The payload clone is the inverse of `__rt_object_free_deep`: strings are
//!   re-persisted (single-owner model — never incref'd), refcounted property
//!   payloads (arrays, hashes, objects, mixed cells) are retained via
//!   `__rt_incref`, and scalar/float/reference slots are byte-copied. The
//!   per-property disposition is read from the class `_class_gc_desc_<id>`
//!   descriptor, exactly as the deep-free walk does.
//! - An optional `#[\AllowDynamicProperties]` dynamic-property hash at the
//!   payload tail is cloned via `__rt_hash_clone_shallow` so the clone owns an
//!   independent dynamic-properties container.
//! - `__clone()` dispatch mirrors `__rt_call_object_destructor`: the implementing
//!   class is resolved through `method_impl_classes` so an inherited `__clone`
//!   dispatches to the ancestor's emitted symbol; a null table entry means the
//!   class and its ancestors declare no `__clone`, so no call is made. Unlike
//!   `__destruct`, no re-entrancy guard is needed: `__clone` runs on the freshly
//!   allocated copy (refcount 1) and cannot re-enter a free path.

use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the object-clone runtime helpers for the active target.
pub(crate) fn emit_object_clone(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_object_clone_linux_x86_64(emitter);
        return;
    }
    emit_object_clone_aarch64(emitter);
}

/// Emits the ARM64 `__rt_object_clone`, `__rt_object_clone_payload`, and
/// `__rt_call_object_clone_method` helpers.
fn emit_object_clone_aarch64(emitter: &mut Emitter) {
    emit_object_clone_payload_aarch64(emitter);
    emit_call_object_clone_method_aarch64(emitter);
    emit_object_clone_entry_aarch64(emitter);
}

/// Emits the ARM64 `__rt_object_clone_payload` helper.
///
/// Shallow-copies an object payload into a fresh heap allocation: stamps the
/// object heap kind, byte-copies the whole payload (class id + property slots +
/// optional dynamic-properties tail), then walks the class gc descriptor to
/// re-persist string properties and retain refcounted property children. The
/// dynamic-properties hash, when present, is replaced with an independent
/// `__rt_hash_clone_shallow` clone so the new object does not alias the source's
/// dynamic-property container.
///
/// Input:  x0 = source object pointer (heap-backed, non-null, heap kind 4).
/// Output: x0 = new object pointer (refcount 1, same class id and properties).
fn emit_object_clone_payload_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: object_clone_payload ---");
    emitter.label_global("__rt_object_clone_payload");

    // -- set up the clone frame: src, new, count, desc, index, size --
    emitter.instruction("sub sp, sp, #64");                                     // allocate a 64-byte frame for clone state
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the source object pointer

    // -- read the payload size and allocate a same-sized clone --
    emitter.instruction("ldr w9, [x0, #-16]");                                  // load the source payload size from the heap header
    emitter.instruction("str x9, [sp, #40]");                                   // save the payload size for the copy loop and dyn-props check
    emitter.instruction("mov x0, x9");                                          // pass the payload size to the heap allocator
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate a fresh object payload (refcount 1, kind raw)
    emitter.instruction("str x0, [sp, #8]");                                    // save the new object pointer
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the source object pointer
    emitter.instruction("ldr x9, [x10, #-8]");                                  // load the source heap kind word (preserves packed GC bits)
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the new object's heap header kind from the source

    // -- byte-copy the whole payload so class id, slots, and the dyn-props slot land intact --
    emitter.instruction("ldr x1, [sp, #0]");                                    // x1 = source payload base
    emitter.instruction("ldr x2, [sp, #8]");                                    // x2 = new payload base
    emitter.instruction("ldr x3, [sp, #40]");                                   // x3 = number of payload bytes to copy
    emitter.label("__rt_object_clone_payload_copy");
    emitter.instruction("cbz x3, __rt_object_clone_payload_walk");              // stop once every payload byte has been copied
    emitter.instruction("ldrb w4, [x1], #1");                                   // load one payload byte from the source object
    emitter.instruction("strb w4, [x2], #1");                                   // store the copied byte into the new object
    emitter.instruction("sub x3, x3, #1");                                      // decrement the remaining byte count
    emitter.instruction("b __rt_object_clone_payload_copy");                    // continue copying the object payload

    // -- derive the declared-property count and resolve the gc descriptor --
    emitter.label("__rt_object_clone_payload_walk");
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the payload size
    emitter.instruction("sub x9, x9, #8");                                      // drop the leading class_id field
    emitter.instruction("lsr x9, x9, #4");                                      // divide by 16 to get the declared-property count
    emitter.instruction("str x9, [sp, #16]");                                   // save the property count for the retain loop
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the source object pointer
    emitter.instruction("ldr x10, [x10]");                                      // x10 = runtime class_id
    abi::emit_load_symbol_to_reg(emitter, "x11", "_class_gc_desc_count", 0);    // x11 = number of emitted class descriptors
    emitter.instruction("cmp x10, x11");                                        // is the class_id within the descriptor table?
    emitter.instruction("b.hs __rt_object_clone_payload_dyn");                  // out-of-range class ids have no descriptor → skip retain fixups
    abi::emit_symbol_address(emitter, "x11", "_class_gc_desc_ptrs");            // x11 = base of the per-class descriptor pointer table
    emitter.instruction("lsl x12, x10, #3");                                    // x12 = class_id * 8 bytes per descriptor pointer
    emitter.instruction("ldr x11, [x11, x12]");                                 // x11 = gc descriptor pointer for this class
    emitter.instruction("str x11, [sp, #24]");                                  // save the descriptor pointer for the retain loop
    emitter.instruction("str xzr, [sp, #32]");                                  // initialize the property index = 0

    // -- walk each property and apply the ownership fixup for its descriptor tag --
    emitter.label("__rt_object_clone_payload_loop");
    emitter.instruction("ldr x12, [sp, #32]");                                  // reload the current property index
    emitter.instruction("ldr x13, [sp, #16]");                                  // reload the total property count
    emitter.instruction("cmp x12, x13");                                        // have we visited every declared property?
    emitter.instruction("b.ge __rt_object_clone_payload_dyn");                  // finish the retain walk once every property is scanned

    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the source object pointer
    emitter.instruction("mov x10, #16");                                        // each property slot occupies 16 bytes
    emitter.instruction("mul x10, x12, x10");                                   // compute the property slot byte offset
    emitter.instruction("add x10, x10, #8");                                    // skip the leading class_id field
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload the descriptor pointer
    emitter.instruction("ldrb w15, [x11, x12]");                                // load the compile-time property tag
    emitter.instruction("cmp x15, #1");                                         // is this a string property?
    emitter.instruction("b.eq __rt_object_clone_payload_str");                  // strings need a fresh persisted copy
    emitter.instruction("cmp x15, #4");                                         // is this an indexed-array property?
    emitter.instruction("b.eq __rt_object_clone_payload_ref");                  // refcounted children need a retain
    emitter.instruction("cmp x15, #5");                                         // is this an associative-array property?
    emitter.instruction("b.eq __rt_object_clone_payload_ref");                  // refcounted children need a retain
    emitter.instruction("cmp x15, #6");                                         // is this an object property?
    emitter.instruction("b.eq __rt_object_clone_payload_ref");                  // refcounted children need a retain
    emitter.instruction("cmp x15, #7");                                         // is this a mixed/union property?
    emitter.instruction("b.eq __rt_object_clone_payload_ref");                  // refcounted children need a retain
    emitter.instruction("b __rt_object_clone_payload_next");                    // scalars, floats, bools, and references are already correct

    // -- string properties: re-persist so the clone owns an independent string payload --
    emitter.label("__rt_object_clone_payload_str");
    emitter.instruction("add x13, x9, x10");                                    // x13 = source slot base
    emitter.instruction("ldr x1, [x13]");                                       // x1 = source string pointer
    emitter.instruction("ldr x2, [x13, #8]");                                   // x2 = source string length
    emitter.instruction("str x12, [sp, #32]");                                  // preserve the property index across str_persist
    emitter.instruction("bl __rt_str_persist");                                 // duplicate the string payload for the clone (x1=new ptr, x2=len)
    emitter.instruction("ldr x12, [sp, #32]");                                  // restore the property index
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the new object pointer
    emitter.instruction("mov x10, #16");                                        // each property slot occupies 16 bytes
    emitter.instruction("mul x10, x12, x10");                                   // recompute the property slot byte offset
    emitter.instruction("add x10, x10, #8");                                    // skip the leading class_id field
    emitter.instruction("add x13, x9, x10");                                    // x13 = new slot base
    emitter.instruction("str x1, [x13]");                                       // install the persisted string pointer into the clone
    emitter.instruction("str x2, [x13, #8]");                                   // install the preserved string length into the clone
    emitter.instruction("b __rt_object_clone_payload_next");                    // advance to the next property

    // -- refcounted properties: retain the byte-copied child pointer for the clone --
    emitter.label("__rt_object_clone_payload_ref");
    emitter.instruction("add x13, x9, x10");                                    // x13 = source slot base
    emitter.instruction("ldr x0, [x13]");                                       // x0 = byte-copied child pointer
    emitter.instruction("str x12, [sp, #32]");                                  // preserve the property index across incref
    emitter.instruction("bl __rt_incref");                                      // retain the shared child for the clone owner
    emitter.instruction("ldr x12, [sp, #32]");                                  // restore the property index
    emitter.instruction("b __rt_object_clone_payload_next");                    // advance to the next property

    emitter.label("__rt_object_clone_payload_next");
    emitter.instruction("ldr x12, [sp, #32]");                                  // reload the property index
    emitter.instruction("add x12, x12, #1");                                    // advance to the next property slot
    emitter.instruction("str x12, [sp, #32]");                                  // save the updated property index
    emitter.instruction("b __rt_object_clone_payload_loop");                    // continue the retain walk

    // -- clone the dynamic-properties hash tail when the payload carries one --
    emitter.label("__rt_object_clone_payload_dyn");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the new object pointer
    emitter.instruction("ldr w9, [x0, #-16]");                                  // load the new payload size
    emitter.instruction("sub x9, x9, #8");                                      // drop the leading class_id field
    emitter.instruction("and x10, x9, #15");                                    // isolate the low 4 bits of the property region size
    emitter.instruction("cmp x10, #8");                                         // 8 leftover bytes signal a dyn-props pointer slot
    emitter.instruction("b.ne __rt_object_clone_payload_ret");                  // no dyn-props tail → finish
    emitter.instruction("sub x9, x9, #8");                                      // back out the dyn-props slot from the property region size
    emitter.instruction("add x9, x9, #8");                                      // re-add the leading class_id offset to land on the dyn-props slot
    emitter.instruction("ldr x11, [x0, x9]");                                   // load the byte-copied dyn-props hash pointer (aliases the source)
    emitter.instruction("cbz x11, __rt_object_clone_payload_ret");              // a null hash (lazy-init never happened) needs no clone
    emitter.instruction("mov x0, x11");                                         // pass the hash pointer to the clone helper
    emitter.instruction("str x9, [sp, #32]");                                   // preserve the dyn-props offset across hash_clone_shallow
    emitter.instruction("bl __rt_hash_clone_shallow");                          // clone the dyn-props hash (x0 = new hash)
    emitter.instruction("ldr x9, [sp, #32]");                                   // restore the dyn-props offset
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the new object pointer
    emitter.instruction("str x0, [x10, x9]");                                   // install the cloned dyn-props hash into the new object

    emitter.label("__rt_object_clone_payload_ret");
    emitter.instruction("ldr x0, [sp, #8]");                                    // return the new object pointer
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate the clone frame
    emitter.instruction("ret");                                                 // return with x0 = new object pointer
}

/// Emits the ARM64 `__rt_call_object_clone_method` helper.
///
/// Looks up the class's `__clone` in the `_class_clone_ptrs` table (indexed by
/// runtime class_id) and invokes it on the receiver with no arguments. A null
/// entry means the class and its ancestors declare no `__clone`, so no call is
/// made. The receiver is borrowed (no incref/decref), matching normal method ABI.
///
/// Input:  x0 = object pointer (the freshly cloned instance).
/// Output: none. Clobbers scratch registers; the object pointer's memory is
/// preserved so the caller can return it after the call.
fn emit_call_object_clone_method_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: call_object_clone_method ---");
    emitter.label_global("__rt_call_object_clone_method");

    emitter.instruction("cbz x0, __rt_call_object_clone_method_ret");           // null receiver → nothing to clone-method
    emitter.instruction("ldr x11, [x0]");                                       // x11 = runtime class_id (object payload offset 0)
    abi::emit_load_symbol_to_reg(emitter, "x10", "_class_clone_count", 0);     // x10 = number of emitted clone-method entries
    emitter.instruction("cmp x11, x10");                                        // is class_id within the clone-method table?
    emitter.instruction("b.hs __rt_call_object_clone_method_ret");              // out-of-range class ids have no __clone
    abi::emit_symbol_address(emitter, "x10", "_class_clone_ptrs");             // x10 = base of the per-class __clone symbol table
    emitter.instruction("ldr x10, [x10, x11, lsl #3]");                         // x10 = __clone symbol for this class (or 0)
    emitter.instruction("cbz x10, __rt_call_object_clone_method_ret");          // class defines no __clone → done
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // save frame pointer and return address before the user call
    emitter.instruction("mov x29, sp");                                         // establish the helper frame
    emitter.instruction("blr x10");                                             // invoke <class>::__clone with x0 = $this (borrowed)
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore frame pointer and return address

    emitter.label("__rt_call_object_clone_method_ret");
    emitter.instruction("ret");                                                 // return to the clone entry helper
}

/// Emits the ARM64 `__rt_object_clone` entry helper.
///
/// Dispatches on the operand's heap kind: a raw object (kind 4) is cloned
/// directly and its `__clone` is run; a boxed Mixed cell (kind 5) is unboxed
/// when it holds an object payload (tag 6), the inner object is cloned and
/// `__clone`'d, and the result is reboxed into a fresh Mixed cell; null and
/// non-object operands yield null (PHP's runtime non-object clone error is
/// deferred — not on the Symfony path, and the checker rejects definite
/// non-object operands at compile time).
///
/// Input:  x0 = object pointer or boxed Mixed-cell pointer.
/// Output: x0 = new object pointer, or a new Mixed box wrapping it.
fn emit_object_clone_entry_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: object_clone ---");
    emitter.label_global("__rt_object_clone");

    emitter.instruction("cbz x0, __rt_object_clone_null");                      // clone null → null
    emitter.instruction("stp x29, x30, [sp, #-32]!");                           // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame
    emitter.instruction("str x0, [sp, #16]");                                   // save the source pointer across helper calls
    emitter.instruction("ldr w9, [x0, #-8]");                                   // load the operand heap kind word
    emitter.instruction("and w9, w9, #0xff");                                   // mask to the kind byte (strip packed value_type/GC bits)
    emitter.instruction("cmp w9, #4");                                          // is the operand a raw object?
    emitter.instruction("b.eq __rt_object_clone_obj");                          // clone a raw object directly
    emitter.instruction("cmp w9, #5");                                          // is the operand a boxed Mixed cell?
    emitter.instruction("b.eq __rt_object_clone_mixed");                        // unbox, clone, and rebox
    emitter.instruction("mov x0, #0");                                          // unknown kind → null
    emitter.instruction("b __rt_object_clone_done");                            // finish with a null result

    // -- raw object: clone the payload, run __clone, return the new object --
    emitter.label("__rt_object_clone_obj");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the source object pointer
    emitter.instruction("bl __rt_object_clone_payload");                        // x0 = new object pointer
    emitter.instruction("str x0, [sp, #24]");                                   // save the new object across __clone
    emitter.instruction("bl __rt_call_object_clone_method");                    // run __clone on the new object (x0 = $this)
    emitter.instruction("ldr x0, [sp, #24]");                                   // restore the new object pointer
    emitter.instruction("b __rt_object_clone_done");                            // finish with the new object

    // -- boxed Mixed: unbox an object payload, clone it, run __clone, rebox --
    emitter.label("__rt_object_clone_mixed");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the Mixed-cell pointer
    emitter.instruction("ldr x9, [x0]");                                        // x9 = boxed value tag
    emitter.instruction("cmp x9, #6");                                          // does the box hold an object payload?
    emitter.instruction("b.ne __rt_object_clone_nonobj");                       // non-object payload → null
    emitter.instruction("ldr x0, [x0, #8]");                                    // x0 = boxed object pointer
    emitter.instruction("bl __rt_object_clone_payload");                        // x0 = new object pointer
    emitter.instruction("str x0, [sp, #24]");                                   // save the new object across __clone and rebox
    emitter.instruction("bl __rt_call_object_clone_method");                    // run __clone on the new object
    emitter.instruction("mov x0, #24");                                         // Mixed cells store tag plus two payload words
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate a fresh Mixed cell (refcount 1, kind raw)
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the source Mixed-cell pointer
    emitter.instruction("ldr x9, [x10, #-8]");                                  // load the source Mixed-cell heap kind word (preserves packed GC bits)
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the Mixed-cell heap kind from the source
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the new object pointer
    emitter.instruction("mov x9, #6");                                          // runtime value tag 6 = object payload
    emitter.instruction("str x9, [x0]");                                        // store the object tag at mixed[0]
    emitter.instruction("str x10, [x0, #8]");                                   // store the new object pointer at mixed[8]
    emitter.instruction("str xzr, [x0, #16]");                                  // clear the high payload word at mixed[16]
    emitter.instruction("b __rt_object_clone_done");                            // finish with the new Mixed box

    emitter.label("__rt_object_clone_nonobj");
    emitter.instruction("mov x0, #0");                                          // non-object Mixed payload → null
    emitter.instruction("b __rt_object_clone_done");                            // finish with a null result

    emitter.label("__rt_object_clone_done");
    emitter.instruction("ldp x29, x30, [sp], #32");                             // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return with x0 = clone result

    emitter.label("__rt_object_clone_null");
    emitter.instruction("mov x0, #0");                                          // clone null → null
    emitter.instruction("ret");                                                 // return null without touching the frame
}

/// Emits the x86_64 `__rt_object_clone`, `__rt_object_clone_payload`, and
/// `__rt_call_object_clone_method` helpers.
fn emit_object_clone_linux_x86_64(emitter: &mut Emitter) {
    emit_object_clone_payload_linux_x86_64(emitter);
    emit_call_object_clone_method_linux_x86_64(emitter);
    emit_object_clone_entry_linux_x86_64(emitter);
}

/// Emits the x86_64 `__rt_object_clone_payload` helper (mirrors the ARM64 logic).
fn emit_object_clone_payload_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: object_clone_payload ---");
    emitter.label_global("__rt_object_clone_payload");

    // -- set up the clone frame below the saved callee-saved registers --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("push r12");                                            // preserve r12 (holds the new object across helper calls)
    emitter.instruction("push r13");                                            // preserve r13 (holds the source object across helper calls)
    emitter.instruction("push r14");                                            // preserve r14 (holds the descriptor pointer across the retain loop)
    emitter.instruction("push r15");                                            // preserve r15 (holds the property count across the retain loop)
    emitter.instruction("sub rsp, 48");                                         // reserve local slots (16-aligned so helper calls stay ABI-aligned)
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the source object pointer
    emitter.instruction("mov eax, DWORD PTR [rax - 16]");                       // load the source payload size from the heap header
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // save the payload size for the copy loop and dyn-props check
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // pass the payload size to the heap allocator
    emitter.instruction("call __rt_heap_alloc");                                // allocate a fresh object payload (refcount 1, kind raw)
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the new object pointer
    emitter.instruction("mov r12, rax");                                        // keep the new object pointer in a callee-saved register
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the source object pointer
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the source heap kind word (preserves magic + packed bits)
    emitter.instruction("mov QWORD PTR [r12 - 8], r10");                        // stamp the new object's heap header kind from the source

    // -- byte-copy the whole payload so class id, slots, and the dyn-props slot land intact --
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // rsi = source payload base
    emitter.instruction("mov rdi, r12");                                        // rdi = new payload base
    emitter.instruction("mov rcx, QWORD PTR [rbp - 80]");                       // rcx = number of payload bytes to copy
    emitter.label("__rt_object_clone_payload_copy");
    emitter.instruction("test rcx, rcx");                                       // have we copied every payload byte?
    emitter.instruction("jz __rt_object_clone_payload_walk");                   // stop once the payload region is exhausted
    emitter.instruction("mov r8b, BYTE PTR [rsi]");                             // load one payload byte from the source object
    emitter.instruction("mov BYTE PTR [rdi], r8b");                             // store the copied byte into the new object
    emitter.instruction("add rsi, 1");                                          // advance the source cursor
    emitter.instruction("add rdi, 1");                                          // advance the new cursor
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining byte count
    emitter.instruction("jmp __rt_object_clone_payload_copy");                  // continue copying the object payload

    // -- derive the declared-property count and resolve the gc descriptor --
    emitter.label("__rt_object_clone_payload_walk");
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // reload the payload size
    emitter.instruction("sub rax, 8");                                          // drop the leading class_id field
    emitter.instruction("shr rax, 4");                                          // divide by 16 to get the declared-property count
    emitter.instruction("mov r15, rax");                                        // r15 = property count (callee-saved across the loop)
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the source object pointer
    emitter.instruction("mov rax, QWORD PTR [rax]");                            // rax = runtime class_id
    emitter.instruction("mov r13, rax");                                        // r13 = class_id (callee-saved across helper calls)
    abi::emit_cmp_reg_to_symbol(emitter, "r13", "_class_gc_desc_count");      // is the class_id within the descriptor table? (RIP-relative for PIE)
    emitter.instruction("jae __rt_object_clone_payload_dyn");                   // out-of-range class ids have no descriptor → skip retain fixups
    abi::emit_symbol_address(emitter, "r14", "_class_gc_desc_ptrs");           // r14 = base of the per-class descriptor pointer table (RIP-relative for PIE)
    emitter.instruction("mov r14, QWORD PTR [r14 + r13 * 8]");                  // r14 = gc descriptor pointer for this class
    emitter.instruction("mov QWORD PTR [rbp - 72], 0");                         // initialize the property index = 0

    // -- walk each property and apply the ownership fixup for its descriptor tag --
    emitter.label("__rt_object_clone_payload_loop");
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // reload the current property index
    emitter.instruction("cmp rax, r15");                                        // have we visited every declared property?
    emitter.instruction("jae __rt_object_clone_payload_dyn");                   // finish the retain walk once every property is scanned

    emitter.instruction("mov r10, rax");                                        // r10 = property index
    emitter.instruction("shl r10, 4");                                          // r10 = property slot byte offset (16 bytes each)
    emitter.instruction("add r10, 8");                                          // skip the leading class_id field
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // reload the source object pointer
    emitter.instruction("lea r8, [r8 + r10]");                                  // r8 = source slot base
    emitter.instruction("movzx r9, BYTE PTR [r14 + rax]");                      // r9 = compile-time property tag
    emitter.instruction("cmp r9, 1");                                           // is this a string property?
    emitter.instruction("je __rt_object_clone_payload_str");                    // strings need a fresh persisted copy
    emitter.instruction("cmp r9, 4");                                           // is this an indexed-array property?
    emitter.instruction("je __rt_object_clone_payload_ref");                    // refcounted children need a retain
    emitter.instruction("cmp r9, 5");                                           // is this an associative-array property?
    emitter.instruction("je __rt_object_clone_payload_ref");                    // refcounted children need a retain
    emitter.instruction("cmp r9, 6");                                           // is this an object property?
    emitter.instruction("je __rt_object_clone_payload_ref");                    // refcounted children need a retain
    emitter.instruction("cmp r9, 7");                                           // is this a mixed/union property?
    emitter.instruction("je __rt_object_clone_payload_ref");                    // refcounted children need a retain
    emitter.instruction("jmp __rt_object_clone_payload_next");                  // scalars, floats, bools, and references are already correct

    // -- string properties: re-persist so the clone owns an independent string payload --
    emitter.label("__rt_object_clone_payload_str");
    emitter.instruction("mov rax, QWORD PTR [r8]");                             // rax = source string pointer
    emitter.instruction("mov rdx, QWORD PTR [r8 + 8]");                         // rdx = source string length
    emitter.instruction("call __rt_str_persist");                               // duplicate the string payload (rax=new ptr, rdx=len)
    emitter.instruction("mov r10, QWORD PTR [rbp - 72]");                       // restore the property index after str_persist
    emitter.instruction("shl r10, 4");                                          // recompute the property slot byte offset
    emitter.instruction("add r10, 8");                                          // skip the leading class_id field
    emitter.instruction("lea r11, [r12 + r10]");                                // r11 = new slot base (r12 = new object)
    emitter.instruction("mov QWORD PTR [r11], rax");                            // install the persisted string pointer into the clone
    emitter.instruction("mov QWORD PTR [r11 + 8], rdx");                        // install the preserved string length into the clone
    emitter.instruction("jmp __rt_object_clone_payload_next");                  // advance to the next property

    // -- refcounted properties: retain the byte-copied child pointer for the clone --
    emitter.label("__rt_object_clone_payload_ref");
    emitter.instruction("mov rax, QWORD PTR [r8]");                             // rax = byte-copied child pointer
    emitter.instruction("call __rt_incref");                                    // retain the shared child for the clone owner
    emitter.instruction("jmp __rt_object_clone_payload_next");                  // advance to the next property

    emitter.label("__rt_object_clone_payload_next");
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // reload the property index
    emitter.instruction("add rax, 1");                                          // advance to the next property slot
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // save the updated property index
    emitter.instruction("jmp __rt_object_clone_payload_loop");                  // continue the retain walk

    // -- clone the dynamic-properties hash tail when the payload carries one --
    emitter.label("__rt_object_clone_payload_dyn");
    emitter.instruction("mov rax, r12");                                        // rax = new object pointer
    emitter.instruction("mov eax, DWORD PTR [rax - 16]");                       // load the new payload size
    emitter.instruction("sub rax, 8");                                          // drop the leading class_id field
    emitter.instruction("mov r10, rax");                                        // copy the property-region size before masking (rax is reused below)
    emitter.instruction("and r10, 15");                                         // isolate the low 4 bits of the property region size
    emitter.instruction("cmp r10, 8");                                          // 8 leftover bytes signal a dyn-props pointer slot
    emitter.instruction("jne __rt_object_clone_payload_ret");                   // no dyn-props tail → finish
    emitter.instruction("sub rax, 8");                                          // back out the dyn-props slot from the property region size
    emitter.instruction("add rax, 8");                                          // re-add the leading class_id offset to land on the dyn-props slot
    emitter.instruction("mov r9, QWORD PTR [r12 + rax]");                       // load the byte-copied dyn-props hash pointer (aliases the source)
    emitter.instruction("test r9, r9");                                         // is the dyn-props hash present?
    emitter.instruction("jz __rt_object_clone_payload_ret");                    // a null hash (lazy-init never happened) needs no clone
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // preserve the dyn-props offset across hash_clone_shallow
    emitter.instruction("mov rdi, r9");                                         // pass the hash pointer to the clone helper
    emitter.instruction("call __rt_hash_clone_shallow");                        // clone the dyn-props hash (rax = new hash)
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // restore the dyn-props offset
    emitter.instruction("mov QWORD PTR [r12 + rdx], rax");                      // install the cloned dyn-props hash into the new object

    emitter.label("__rt_object_clone_payload_ret");
    emitter.instruction("mov rax, r12");                                        // return the new object pointer
    emitter.instruction("add rsp, 48");                                         // release the local spill slots
    emitter.instruction("pop r15");                                             // restore r15
    emitter.instruction("pop r14");                                             // restore r14
    emitter.instruction("pop r13");                                             // restore r13
    emitter.instruction("pop r12");                                             // restore r12
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return with rax = new object pointer
}

/// Emits the x86_64 `__rt_call_object_clone_method` helper (mirrors the ARM64 logic).
fn emit_call_object_clone_method_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: call_object_clone_method ---");
    emitter.label_global("__rt_call_object_clone_method");

    emitter.instruction("test rdi, rdi");                                       // null receiver → nothing to clone-method
    emitter.instruction("jz __rt_call_object_clone_method_ret");                // skip the lookup for a null object
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // rax = runtime class_id (object payload offset 0)
    abi::emit_cmp_reg_to_symbol(emitter, "rax", "_class_clone_count");        // is class_id within the clone-method table? (RIP-relative for PIE)
    emitter.instruction("jae __rt_call_object_clone_method_ret");               // out-of-range class ids have no __clone
    abi::emit_symbol_address(emitter, "r10", "_class_clone_ptrs");            // r10 = base of the per-class __clone symbol table (RIP-relative for PIE)
    emitter.instruction("mov r10, QWORD PTR [r10 + rax * 8]");                  // r10 = __clone symbol for this class (or 0)
    emitter.instruction("test r10, r10");                                       // class defines no __clone?
    emitter.instruction("jz __rt_call_object_clone_method_ret");                // nothing to call → done
    emitter.instruction("push rbp");                                            // align the stack and save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction("call r10");                                            // invoke <class>::__clone with rdi = $this (borrowed)
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer

    emitter.label("__rt_call_object_clone_method_ret");
    emitter.instruction("ret");                                                 // return to the clone entry helper
}

/// Emits the x86_64 `__rt_object_clone` entry helper (mirrors the ARM64 logic).
fn emit_object_clone_entry_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: object_clone ---");
    emitter.label_global("__rt_object_clone");

    emitter.instruction("test rax, rax");                                       // clone null → null
    emitter.instruction("jz __rt_object_clone_null");                           // skip the frame setup for a null operand
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 16");                                         // reserve local slots for the source and new pointers
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the source pointer across helper calls
    emitter.instruction("mov eax, DWORD PTR [rax - 8]");                        // load the operand heap kind low word
    emitter.instruction("and eax, 0xff");                                       // mask to the kind byte (strip magic + packed bits)
    emitter.instruction("cmp eax, 4");                                          // is the operand a raw object?
    emitter.instruction("je __rt_object_clone_obj");                            // clone a raw object directly
    emitter.instruction("cmp eax, 5");                                          // is the operand a boxed Mixed cell?
    emitter.instruction("je __rt_object_clone_mixed");                          // unbox, clone, and rebox
    emitter.instruction("xor eax, eax");                                        // unknown kind → null
    emitter.instruction("jmp __rt_object_clone_done");                          // finish with a null result

    // -- raw object: clone the payload, run __clone, return the new object --
    emitter.label("__rt_object_clone_obj");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source object pointer
    emitter.instruction("call __rt_object_clone_payload");                      // rax = new object pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the new object across __clone
    emitter.instruction("mov rdi, rax");                                        // pass the new object as $this to __clone
    emitter.instruction("call __rt_call_object_clone_method");                  // run __clone on the new object
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // restore the new object pointer
    emitter.instruction("jmp __rt_object_clone_done");                          // finish with the new object

    // -- boxed Mixed: unbox an object payload, clone it, run __clone, rebox --
    emitter.label("__rt_object_clone_mixed");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the Mixed-cell pointer
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // r9 = boxed value tag
    emitter.instruction("cmp r9, 6");                                           // does the box hold an object payload?
    emitter.instruction("jne __rt_object_clone_nonobj");                        // non-object payload → null
    emitter.instruction("mov rax, QWORD PTR [rax + 8]");                        // rax = boxed object pointer
    emitter.instruction("call __rt_object_clone_payload");                      // rax = new object pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the new object across __clone and rebox
    emitter.instruction("mov rdi, rax");                                        // pass the new object as $this to __clone
    emitter.instruction("call __rt_call_object_clone_method");                  // run __clone on the new object
    emitter.instruction("mov rax, 24");                                         // Mixed cells store tag plus two payload words
    emitter.instruction("call __rt_heap_alloc");                                // allocate a fresh Mixed cell (refcount 1, kind raw)
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the source Mixed-cell pointer
    emitter.instruction("mov r10, QWORD PTR [r9 - 8]");                         // load the source Mixed-cell heap kind word (preserves magic + packed bits)
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the Mixed-cell heap kind from the source
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the new object pointer (preserved across __clone)
    emitter.instruction("mov QWORD PTR [rax], 6");                              // store the object tag at mixed[0]
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // store the new object pointer at mixed[8]
    emitter.instruction("mov QWORD PTR [rax + 16], 0");                         // clear the high payload word at mixed[16]
    emitter.instruction("jmp __rt_object_clone_done");                          // finish with the new Mixed box (rax = box)

    emitter.label("__rt_object_clone_nonobj");
    emitter.instruction("xor eax, eax");                                        // non-object Mixed payload → null
    emitter.instruction("jmp __rt_object_clone_done");                          // finish with a null result

    emitter.label("__rt_object_clone_done");
    emitter.instruction("add rsp, 16");                                         // release the local slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return with rax = clone result

    emitter.label("__rt_object_clone_null");
    emitter.instruction("xor eax, eax");                                        // clone null → null
    emitter.instruction("ret");                                                 // return null without touching the frame
}