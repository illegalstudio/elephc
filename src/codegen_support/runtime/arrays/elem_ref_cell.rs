//! Purpose:
//! Emits `__rt_elem_ref_cell` and `__rt_ref_slot_cell`: make an array element a PHP reference
//! IN PLACE and hand back the shared cell, so `$x = &$a[$k]` binds `$x` to the very reference the
//! element now is.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//! - `Op::ElemRefCell`, for containers whose elements are boxed Mixed values.
//!
//! Key details:
//! - The element becomes the marker every reference element uses: a box `{13, cell, 7}` whose
//!   kind-7 cell holds the element's value as a Mixed box. The marker owns one share of the cell;
//!   the caller receives a second one, which the bound local releases at scope exit. A later
//!   `$a[$k] = v` writes through the marker (`__rt_ref_marker_store`), a copy of the array shares
//!   it, and destroying the array drops the marker's share.
//! - An element that already is a reference hands back its existing cell, so every binder of the
//!   same element shares one reference, as in PHP.
//! - The container is split before the slot changes (copy-on-write) and a missing key is created
//!   as null first, both through the ordinary setters, so the returned container is the one the
//!   caller must store back.
//! - Indexed containers are widened to boxed slots first: a marker needs a pointer-sized slot.

use crate::codegen_support::callable_invoker_args::{
    ARRAY_GLOBAL_REF_CELL_TAG, ARRAY_LOCAL_REF_CELL_TAG,
};
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits both helpers for the current target.
///
/// `__rt_elem_ref_cell`
/// - ARM64: in `x0` = container, `x1` = key_lo, `x2` = key_hi (-1 for an integer key);
///   out `x0` = the container to store back, `x1` = the cell, retained for the caller.
/// - x86_64: in `rdi`, `rsi`, `rdx`; out `rax` = container, `rdx` = cell.
///
/// `__rt_ref_slot_cell`
/// - in `x0`/`rdi` = address of a boxed-Mixed slot in a unique container;
///   out `x0`/`rax` = the slot's cell, retained for the caller.
/// Emits only `__rt_ref_copy_slot`, for runtime subsets that clone containers without the rest of
/// the element-reference helpers.
pub fn emit_ref_copy_slot(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ref_copy_slot_x86_64(emitter);
    } else {
        emit_ref_copy_slot_aarch64(emitter);
    }
}

pub fn emit_elem_ref_cell(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ref_slot_cell_x86_64(emitter);
        emit_elem_ref_cell_x86_64(emitter);
        emit_ref_copy_slot_x86_64(emitter);
        emit_mixed_elem_ref_cell_x86_64(emitter);
        return;
    }
    emit_ref_slot_cell_aarch64(emitter);
    emit_elem_ref_cell_aarch64(emitter);
    emit_ref_copy_slot_aarch64(emitter);
    emit_mixed_elem_ref_cell_aarch64(emitter);
}

/// `__rt_mixed_elem_ref_cell`: `__rt_elem_ref_cell` for a container held in a Mixed box.
///
/// The box is the local's value, so the promoted (split, vivified, maybe hash-promoted) container
/// is republished INSIDE it. A box another holder shares is first replaced by a private one --
/// mutating a shared box would write through every copy -- and a null (or absent) box becomes a
/// new box holding an empty array, as PHP vivifies `$null[$k]`. Any other payload cannot hold an
/// element: the caller gets a fresh detached cell and the box is left alone.
/// - in `x0`/`rdi` = box (may be null), `x1`/`rsi` = key_lo, `x2`/`rdx` = key_hi;
///   out `x0`/`rax` = the box to store back, `x1`/`rdx` = the cell, retained for the caller.
fn emit_mixed_elem_ref_cell_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_elem_ref_cell ---");
    emitter.label_global("__rt_mixed_elem_ref_cell");

    // Frame: [sp,#0]=box, [sp,#8]=key_lo, [sp,#16]=key_hi, [sp,#24]=cell, [sp,#32]=scratch.
    emitter.instruction("sub sp, sp, #64");                                     // reserve the helper frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // keep the box
    emitter.instruction("str x1, [sp, #8]");                                    // keep key_lo
    emitter.instruction("str x2, [sp, #16]");                                   // keep key_hi
    emitter.instruction("cbz x0, __rt_mixed_elem_ref_cell_vivify");             // an absent value vivifies to an empty array
    emitter.instruction("ldr x9, [x0]");                                        // the box's tag
    emitter.instruction("cmp x9, #4");                                          // an indexed array?
    emitter.instruction("b.eq __rt_mixed_elem_ref_cell_container");             // promote its element
    emitter.instruction("cmp x9, #5");                                          // a hash?
    emitter.instruction("b.eq __rt_mixed_elem_ref_cell_container");             // promote its element
    emitter.instruction("cmp x9, #8");                                          // null vivifies like PHP
    emitter.instruction("b.eq __rt_mixed_elem_ref_cell_vivify");                // build an empty array for it
    // -- a scalar or object payload holds no element: hand back a detached cell --
    emitter.instruction("mov x0, #8");                                          // runtime tag 8 = null
    emitter.instruction("mov x1, #0");                                          // null has no payload
    emitter.instruction("mov x2, #0");                                          // null has no high word
    emitter.instruction("bl __rt_mixed_from_value");                            // x0 = a null box
    emitter.instruction("mov x1, #7");                                          // the cell holds a boxed Mixed handle
    emitter.instruction("bl __rt_ref_cell_alloc");                              // x0 = a cell only the caller holds
    emitter.instruction("mov x1, x0");                                          // return it as the cell
    emitter.instruction("ldr x0, [sp, #0]");                                    // the box is unchanged
    emitter.instruction("b __rt_mixed_elem_ref_cell_return");                   // done

    emitter.label("__rt_mixed_elem_ref_cell_vivify");
    emitter.instruction("mov x0, #0");                                          // zero initial capacity
    emitter.instruction("mov x1, #8");                                          // boxed slots are one pointer wide
    emitter.instruction("bl __rt_array_new");                                   // x0 = the empty array PHP creates
    emitter.instruction("str x0, [sp, #32]");                                   // keep it across the boxing
    emitter.instruction("mov x1, x0");                                          // the new box's payload
    emitter.instruction("mov x0, #4");                                          // runtime tag 4 = indexed array
    emitter.instruction("mov x2, #0");                                          // arrays have no high word
    emitter.instruction("bl __rt_mixed_from_value");                            // x0 = a box retaining the array
    emitter.instruction("str x0, [sp, #24]");                                   // keep the new box
    emitter.instruction("ldr x0, [sp, #32]");                                   // the array's creation reference
    emitter.instruction("bl __rt_decref_array");                                // the box is its only owner now
    emitter.instruction("ldr x0, [sp, #0]");                                    // the replaced box (may be null)
    emitter.instruction("bl __rt_decref_mixed");                                // the slot's reference moves to the new box
    emitter.instruction("ldr x0, [sp, #24]");                                   // the new box
    emitter.instruction("str x0, [sp, #0]");                                    // is the one to store back
    emitter.instruction("b __rt_mixed_elem_ref_cell_promote");                  // promote the element inside it

    emitter.label("__rt_mixed_elem_ref_cell_container");
    emitter.instruction("ldr w9, [x0, #-12]");                                  // how many holders share this box
    emitter.instruction("cmp w9, #1");                                          // only this slot?
    emitter.instruction("b.ls __rt_mixed_elem_ref_cell_promote");               // then it may be mutated in place
    emitter.instruction("ldr x1, [x0, #8]");                                    // the container payload
    emitter.instruction("ldr x0, [x0]");                                        // under its own tag
    emitter.instruction("mov x2, #0");                                          // containers have no high word
    emitter.instruction("bl __rt_mixed_from_value");                            // x0 = a private box retaining the container
    emitter.instruction("str x0, [sp, #24]");                                   // keep it
    emitter.instruction("ldr x0, [sp, #0]");                                    // the shared box
    emitter.instruction("bl __rt_decref_mixed");                                // this slot no longer holds it
    emitter.instruction("ldr x0, [sp, #24]");                                   // the private box
    emitter.instruction("str x0, [sp, #0]");                                    // is the one to store back

    emitter.label("__rt_mixed_elem_ref_cell_promote");
    emitter.instruction("ldr x9, [sp, #0]");                                    // the box to mutate
    emitter.instruction("ldr x0, [x9, #8]");                                    // its container
    emitter.instruction("ldr x1, [sp, #8]");                                    // key_lo
    emitter.instruction("ldr x2, [sp, #16]");                                   // key_hi
    emitter.instruction("bl __rt_elem_ref_cell");                               // x0 = container to store back, x1 = cell
    emitter.instruction("str x1, [sp, #24]");                                   // keep the cell
    emitter.instruction("ldr x9, [sp, #0]");                                    // the box
    emitter.instruction("str x0, [x9, #8]");                                    // republish the (possibly relocated) container inside it
    emitter.instruction("bl __rt_heap_kind");                                   // x0 = the container's storage kind
    emitter.instruction("cmp x0, #3");                                          // promoted to a hash?
    emitter.instruction("mov x10, #4");                                         // runtime tag 4 = indexed array
    emitter.instruction("mov x11, #5");                                         // runtime tag 5 = hash
    emitter.instruction("csel x10, x11, x10, eq");                              // the box's tag follows the storage
    emitter.instruction("ldr x9, [sp, #0]");                                    // the box
    emitter.instruction("str x10, [x9]");                                       // retag it
    emitter.instruction("ldr x0, [sp, #0]");                                    // return the box to store back
    emitter.instruction("ldr x1, [sp, #24]");                                   // and the cell

    emitter.label("__rt_mixed_elem_ref_cell_return");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return (x0 = box, x1 = cell)
}

fn emit_mixed_elem_ref_cell_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_elem_ref_cell ---");
    emitter.label_global("__rt_mixed_elem_ref_cell");

    // Frame: [rbp-8]=box, [rbp-16]=key_lo, [rbp-24]=key_hi, [rbp-32]=cell, [rbp-40]=scratch.
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // reserve the helper frame, keeping calls aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // keep the box
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // keep key_lo
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // keep key_hi
    emitter.instruction("test rdi, rdi");                                       // an absent value vivifies to an empty array
    emitter.instruction("jz __rt_mixed_elem_ref_cell_vivify");                  // build one
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // the box's tag
    emitter.instruction("cmp r10, 4");                                          // an indexed array?
    emitter.instruction("je __rt_mixed_elem_ref_cell_container");               // promote its element
    emitter.instruction("cmp r10, 5");                                          // a hash?
    emitter.instruction("je __rt_mixed_elem_ref_cell_container");               // promote its element
    emitter.instruction("cmp r10, 8");                                          // null vivifies like PHP
    emitter.instruction("je __rt_mixed_elem_ref_cell_vivify");                  // build an empty array for it
    emitter.instruction("mov rax, 8");                                          // runtime tag 8 = null
    emitter.instruction("xor edi, edi");                                        // null has no payload
    emitter.instruction("xor esi, esi");                                        // null has no high word
    emitter.instruction("call __rt_mixed_from_value");                          // rax = a null box
    emitter.instruction("mov rdi, rax");                                        // the cell's value
    emitter.instruction("mov esi, 7");                                          // the cell holds a boxed Mixed handle
    emitter.instruction("call __rt_ref_cell_alloc");                            // rax = a cell only the caller holds
    emitter.instruction("mov rdx, rax");                                        // return it as the cell
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // the box is unchanged
    emitter.instruction("jmp __rt_mixed_elem_ref_cell_return");                 // done

    emitter.label("__rt_mixed_elem_ref_cell_vivify");
    emitter.instruction("xor eax, eax");                                        // zero initial capacity
    emitter.instruction("mov rdi, 0");                                          // zero initial capacity (SysV arg)
    emitter.instruction("mov rsi, 8");                                          // boxed slots are one pointer wide
    emitter.instruction("call __rt_array_new");                                 // rax = the empty array PHP creates
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // keep it across the boxing
    emitter.instruction("mov rdi, rax");                                        // the new box's payload
    emitter.instruction("mov eax, 4");                                          // runtime tag 4 = indexed array
    emitter.instruction("xor esi, esi");                                        // arrays have no high word
    emitter.instruction("call __rt_mixed_from_value");                          // rax = a box retaining the array
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // keep the new box
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // the array's creation reference
    emitter.instruction("call __rt_decref_array");                              // the box is its only owner now
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // the replaced box (may be null)
    emitter.instruction("call __rt_decref_mixed");                              // the slot's reference moves to the new box
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // the new box
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // is the one to store back
    emitter.instruction("jmp __rt_mixed_elem_ref_cell_promote");                // promote the element inside it

    emitter.label("__rt_mixed_elem_ref_cell_container");
    emitter.instruction("cmp DWORD PTR [rdi - 12], 1");                         // is this slot the box's only holder?
    emitter.instruction("jbe __rt_mixed_elem_ref_cell_promote");                // then it may be mutated in place
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // the container's tag
    emitter.instruction("mov rdi, QWORD PTR [rdi + 8]");                        // the container payload
    emitter.instruction("xor esi, esi");                                        // containers have no high word
    emitter.instruction("call __rt_mixed_from_value");                          // rax = a private box retaining the container
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // keep it
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // the shared box
    emitter.instruction("call __rt_decref_mixed");                              // this slot no longer holds it
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // the private box
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // is the one to store back

    emitter.label("__rt_mixed_elem_ref_cell_promote");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // the box to mutate
    emitter.instruction("mov rdi, QWORD PTR [r10 + 8]");                        // its container
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // key_lo
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // key_hi
    emitter.instruction("call __rt_elem_ref_cell");                             // rax = container to store back, rdx = cell
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // keep the cell
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // the box
    emitter.instruction("mov QWORD PTR [r10 + 8], rax");                        // republish the (possibly relocated) container inside it
    emitter.instruction("call __rt_heap_kind");                                 // rax = the container's storage kind
    emitter.instruction("mov r11, 4");                                          // runtime tag 4 = indexed array
    emitter.instruction("mov r10, 5");                                          // runtime tag 5 = hash
    emitter.instruction("cmp rax, 3");                                          // promoted to a hash?
    emitter.instruction("cmove r11, r10");                                      // the box's tag follows the storage
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // the box
    emitter.instruction("mov QWORD PTR [r10], r11");                            // retag it
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the box to store back
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // and the cell

    emitter.label("__rt_mixed_elem_ref_cell_return");
    emitter.instruction("leave");                                               // release the helper frame
    emitter.instruction("ret");                                                 // return (rax = box, rdx = cell)
}

/// `__rt_ref_copy_slot`: what a COPY of a container holds for one boxed element.
///
/// A copy shares a reference with its source -- both arrays hold the same marker -- unless the
/// reference has no other holder: a marker only this container holds, over a cell only that
/// marker holds, is a reference in name only, and PHP's `zend_array_dup` copies its VALUE
/// instead (`$x = &$a[0]; unset($x); $b = $a; $b[0] = 2;` leaves `$a[0]` alone). Every other
/// box is shared as it stands.
/// - in `x0`/`rax` = the source element's box (may be null); out `x0`/`rax` = the pointer the copy
///   stores, already retained for it.
fn emit_ref_copy_slot_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ref_copy_slot ---");
    emitter.label_global("__rt_ref_copy_slot");
    emitter.instruction("cbz x0, __rt_ref_copy_slot_retain");                   // a null gap copies as null
    emitter.instruction("ldr w9, [x0, #-12]");                                  // a box mid-release is still visible to a destructor that copies its container
    emitter.instruction("cbz w9, __rt_ref_copy_slot_clone");                    // copy the PHP value without reviving the dying Mixed allocation
    emitter.instruction("ldr x9, [x0]");                                        // inspect the box's tag
    emitter.instruction(&format!("cmp x9, #{}", ARRAY_LOCAL_REF_CELL_TAG));     // is the element a reference?
    emitter.instruction("b.ne __rt_ref_copy_slot_retain");                      // share anything else as it stands
    emitter.instruction("ldr w9, [x0, #-12]");                                  // how many containers hold this marker
    emitter.instruction("cmp w9, #1");                                          // only the source?
    emitter.instruction("b.ne __rt_ref_copy_slot_retain");                      // a shared marker stays a reference in the copy
    emitter.instruction("ldr x10, [x0, #8]");                                   // the marker's cell
    emitter.instruction("ldr w9, [x10, #-12]");                                 // how many holders the reference has
    emitter.instruction("cmp w9, #1");                                          // only this marker?
    emitter.instruction("b.ne __rt_ref_copy_slot_retain");                      // a bound reference stays a reference in the copy
    emitter.instruction("ldr x0, [x10]");                                       // copy the referenced value instead
    emitter.label("__rt_ref_copy_slot_retain");
    emitter.instruction("sub sp, sp, #32");                                     // reserve the helper frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // keep the pointer across the retain
    emitter.instruction("bl __rt_incref");                                      // the copy's own reference (null-safe)
    emitter.instruction("ldr x0, [sp, #0]");                                    // return the pointer the copy stores
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return it in x0
    emitter.label("__rt_ref_copy_slot_clone");
    emitter.instruction("b __rt_mixed_clone");                                  // tail-call: the clone is already owned by the copy
}

fn emit_ref_copy_slot_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ref_copy_slot ---");
    emitter.label_global("__rt_ref_copy_slot");
    emitter.instruction("test rax, rax");                                       // a null gap copies as null
    emitter.instruction("jz __rt_ref_copy_slot_retain");                        // retain nothing for it
    emitter.instruction("cmp DWORD PTR [rax - 12], 0");                         // a box mid-release is still visible to a destructor that copies its container
    emitter.instruction("je __rt_mixed_clone");                                 // copy the PHP value without reviving the dying Mixed allocation
    emitter.instruction(&format!("cmp QWORD PTR [rax], {}", ARRAY_LOCAL_REF_CELL_TAG)); // is the element a reference?
    emitter.instruction("jne __rt_ref_copy_slot_retain");                       // share anything else as it stands
    emitter.instruction("cmp DWORD PTR [rax - 12], 1");                         // is the source the only container holding this marker?
    emitter.instruction("jne __rt_ref_copy_slot_retain");                       // a shared marker stays a reference in the copy
    emitter.instruction("mov r10, QWORD PTR [rax + 8]");                        // the marker's cell
    emitter.instruction("cmp DWORD PTR [r10 - 12], 1");                         // is this marker the reference's only holder?
    emitter.instruction("jne __rt_ref_copy_slot_retain");                       // a bound reference stays a reference in the copy
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // copy the referenced value instead
    emitter.label("__rt_ref_copy_slot_retain");
    emitter.instruction("push rax");                                            // keep the pointer across the retain (aligns the stack)
    emitter.instruction("call __rt_incref");                                    // the copy's own reference (null-safe)
    emitter.instruction("pop rax");                                             // return the pointer the copy stores
    emitter.instruction("ret");                                                 // return it in rax
}

fn emit_ref_slot_cell_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ref_slot_cell ---");
    emitter.label_global("__rt_ref_slot_cell");

    // Frame: [sp,#0]=slot address, [sp,#8]=cell.
    emitter.instruction("sub sp, sp, #32");                                     // reserve the helper frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // keep the slot address across helper calls
    emitter.instruction("ldr x9, [x0]");                                        // the slot's current box
    emitter.instruction("cbz x9, __rt_ref_slot_cell_null");                     // a null gap becomes a reference to null
    emitter.instruction("ldr x10, [x9]");                                       // inspect the box's tag
    emitter.instruction(&format!("cmp x10, #{}", ARRAY_LOCAL_REF_CELL_TAG));    // is the element already a reference?
    emitter.instruction("b.eq __rt_ref_slot_cell_existing");                    // share its cell
    emitter.instruction(&format!("cmp x10, #{}", ARRAY_GLOBAL_REF_CELL_TAG));   // is it a reference to a global?
    emitter.instruction("b.eq __rt_ref_slot_cell_existing");                    // share its cell
    emitter.instruction("mov x0, x9");                                          // the cell takes over the slot's box
    emitter.instruction("b __rt_ref_slot_cell_make");                           // wrap it
    emitter.label("__rt_ref_slot_cell_null");
    emitter.instruction("mov x0, #8");                                          // runtime tag 8 = null
    emitter.instruction("mov x1, #0");                                          // null has no payload
    emitter.instruction("mov x2, #0");                                          // null has no high word
    emitter.instruction("bl __rt_mixed_from_value");                            // x0 = a null box for the cell
    emitter.label("__rt_ref_slot_cell_make");
    emitter.instruction("mov x1, #7");                                          // the cell holds a boxed Mixed handle
    emitter.instruction("bl __rt_ref_cell_alloc");                              // x0 = new cell; its one share is the marker's
    emitter.instruction("str x0, [sp, #8]");                                    // keep the cell across the marker allocation
    emitter.instruction("mov x1, x0");                                          // the marker's payload is the cell
    emitter.instruction(&format!("mov x0, #{}", ARRAY_LOCAL_REF_CELL_TAG));     // marker tag: the element owns a share of a cell
    emitter.instruction("mov x2, #7");                                          // the cell's value is a boxed Mixed handle
    emitter.instruction("bl __rt_mixed_from_value");                            // x0 = the marker box (tag 13 retains nothing)
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the slot address
    emitter.instruction("str x0, [x10]");                                       // the element IS the reference from now on
    emitter.instruction("b __rt_ref_slot_cell_retain");                         // hand the caller its own share
    emitter.label("__rt_ref_slot_cell_existing");
    emitter.instruction("ldr x0, [x9, #8]");                                    // the existing marker's cell
    emitter.instruction("str x0, [sp, #8]");                                    // keep it across the retain
    emitter.label("__rt_ref_slot_cell_retain");
    emitter.instruction("ldr x0, [sp, #8]");                                    // the cell to share
    emitter.instruction("bl __rt_incref");                                      // the caller's share
    emitter.instruction("ldr x0, [sp, #8]");                                    // return the cell
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the cell in x0
}

fn emit_elem_ref_cell_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: elem_ref_cell ---");
    emitter.label_global("__rt_elem_ref_cell");

    // Frame: [sp,#0]=container, [sp,#8]=key_lo, [sp,#16]=key_hi, [sp,#24]=box, [sp,#32]=cell.
    emitter.instruction("sub sp, sp, #64");                                     // reserve the helper frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // keep the container across helper calls
    emitter.instruction("str x1, [sp, #8]");                                    // keep key_lo across helper calls
    emitter.instruction("str x2, [sp, #16]");                                   // keep key_hi across helper calls

    emitter.label("__rt_elem_ref_cell_dispatch");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the current container
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load its packed kind word
    emitter.instruction("and x10, x9, #0xff");                                  // isolate the storage kind
    emitter.instruction("cmp x10, #3");                                         // kind 3 = hash storage
    emitter.instruction("b.eq __rt_elem_ref_cell_hash");                        // bind a hash bucket

    // -- indexed storage: widen to boxed slots, then bind the slot in place --
    emitter.instruction("ubfx x1, x9, #8, #7");                                 // the element value tag
    emitter.instruction("cmp x1, #7");                                          // are the slots already boxed?
    emitter.instruction("b.eq __rt_elem_ref_cell_indexed_boxed");               // a marker fits them as they are
    emitter.instruction("bl __rt_array_to_mixed");                              // x0 = a unique copy with boxed slots
    emitter.instruction("str x0, [sp, #0]");                                    // the container to store back
    emitter.label("__rt_elem_ref_cell_indexed_boxed");
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload key_hi
    emitter.instruction("cmn x2, #1");                                          // is the key an integer?
    emitter.instruction("b.ne __rt_elem_ref_cell_to_hash");                     // a string key needs hash storage
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the container
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the integer key
    emitter.instruction("cmp x1, #0");                                          // a negative key is not a slot
    emitter.instruction("b.lt __rt_elem_ref_cell_indexed_vivify");              // create it as null first
    emitter.instruction("ldr x10, [x0]");                                       // the logical length
    emitter.instruction("cmp x1, x10");                                         // is the key past the last element?
    emitter.instruction("b.ge __rt_elem_ref_cell_indexed_vivify");              // create it as null first
    emitter.instruction("bl __rt_array_ensure_unique");                         // split a shared container before its slot changes
    emitter.instruction("str x0, [sp, #0]");                                    // the container to store back
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the integer key
    emitter.instruction("add x0, x0, #24");                                     // skip the indexed-array header
    emitter.instruction("add x0, x0, x1, lsl #3");                              // the element's boxed slot
    emitter.instruction("bl __rt_ref_slot_cell");                               // x0 = the element's cell, retained for the caller
    emitter.instruction("str x0, [sp, #32]");                                   // keep it for the return
    emitter.instruction("b __rt_elem_ref_cell_return");                         // done

    emitter.label("__rt_elem_ref_cell_indexed_vivify");
    emitter.instruction("mov x0, #8");                                          // runtime tag 8 = null
    emitter.instruction("mov x1, #0");                                          // null has no payload
    emitter.instruction("mov x2, #0");                                          // null has no high word
    emitter.instruction("bl __rt_mixed_from_value");                            // x0 = the null the missing element starts as
    emitter.instruction("mov x2, x0");                                          // the setter consumes it
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the container
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the integer key
    emitter.instruction("bl __rt_array_set_mixed");                             // append, or promote to a hash for a sparse key
    emitter.instruction("str x0, [sp, #0]");                                    // the container to store back
    emitter.instruction("b __rt_elem_ref_cell_dispatch");                       // the element exists now

    emitter.label("__rt_elem_ref_cell_to_hash");
    emitter.instruction("ldr x0, [sp, #0]");                                    // the boxed indexed container
    emitter.instruction("bl __rt_array_to_hash");                               // x0 = an owned hash with the same entries
    emitter.instruction("str x0, [sp, #24]");                                   // keep it across the release
    emitter.instruction("ldr x0, [sp, #0]");                                    // the superseded indexed container
    emitter.instruction("bl __rt_decref_array");                                // its entries are retained by the hash
    emitter.instruction("ldr x0, [sp, #24]");                                   // the hash replaces it
    emitter.instruction("str x0, [sp, #0]");                                    // the container to store back
    emitter.instruction("b __rt_elem_ref_cell_dispatch");                       // bind the hash bucket

    // -- hash storage: replace the bucket with a marker through the ordinary setter --
    emitter.label("__rt_elem_ref_cell_hash");
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload key_lo
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload key_hi
    emitter.instruction("bl __rt_hash_get");                                    // x0=found, x1=value_lo, x2=value_hi, x3=value_tag
    emitter.instruction("cbz x0, __rt_elem_ref_cell_hash_null");                // a missing key becomes a reference to null
    emitter.instruction("cmp x3, #7");                                          // is the entry a box?
    emitter.instruction("b.ne __rt_elem_ref_cell_hash_typed");                  // a raw entry is boxed first
    emitter.instruction("cbz x1, __rt_elem_ref_cell_hash_null");                // a legacy null box pointer is null
    emitter.instruction("ldr x9, [x1]");                                        // inspect the box's tag
    emitter.instruction(&format!("cmp x9, #{}", ARRAY_LOCAL_REF_CELL_TAG));     // is the element already a reference?
    emitter.instruction("b.eq __rt_elem_ref_cell_hash_existing");               // share its cell
    emitter.instruction(&format!("cmp x9, #{}", ARRAY_GLOBAL_REF_CELL_TAG));    // is it a reference to a global?
    emitter.instruction("b.eq __rt_elem_ref_cell_hash_existing");               // share its cell
    emitter.instruction("str x1, [sp, #24]");                                   // the cell takes a reference to the entry's box
    emitter.instruction("mov x0, x1");                                          // the setter below releases the bucket's own reference
    emitter.instruction("bl __rt_incref");                                      // so the cell needs one of its own
    emitter.instruction("ldr x0, [sp, #24]");                                   // the box the cell will hold
    emitter.instruction("b __rt_elem_ref_cell_hash_make");                      // wrap it
    emitter.label("__rt_elem_ref_cell_hash_typed");
    emitter.instruction("mov x0, x3");                                          // box the raw entry under its own tag
    emitter.instruction("bl __rt_mixed_from_value");                            // x0 = a box retaining the payload
    emitter.instruction("b __rt_elem_ref_cell_hash_make");                      // wrap it
    emitter.label("__rt_elem_ref_cell_hash_null");
    emitter.instruction("mov x0, #8");                                          // runtime tag 8 = null
    emitter.instruction("mov x1, #0");                                          // null has no payload
    emitter.instruction("mov x2, #0");                                          // null has no high word
    emitter.instruction("bl __rt_mixed_from_value");                            // x0 = a null box for the cell
    emitter.label("__rt_elem_ref_cell_hash_make");
    emitter.instruction("mov x1, #7");                                          // the cell holds a boxed Mixed handle
    emitter.instruction("bl __rt_ref_cell_alloc");                              // x0 = new cell; its one share is the marker's
    emitter.instruction("str x0, [sp, #32]");                                   // keep the cell
    emitter.instruction("mov x1, x0");                                          // the marker's payload is the cell
    emitter.instruction(&format!("mov x0, #{}", ARRAY_LOCAL_REF_CELL_TAG));     // marker tag: the element owns a share of a cell
    emitter.instruction("mov x2, #7");                                          // the cell's value is a boxed Mixed handle
    emitter.instruction("bl __rt_mixed_from_value");                            // x0 = the marker box (tag 13 retains nothing)
    emitter.instruction("mov x3, x0");                                          // the bucket's new value
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the hash
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload key_lo
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload key_hi
    emitter.instruction("mov x4, #0");                                          // a box has no high word
    emitter.instruction("mov x5, #7");                                          // runtime tag 7 = boxed Mixed
    emitter.instruction("bl __rt_hash_set");                                    // splits a shared hash, releases the replaced value
    emitter.instruction("str x0, [sp, #0]");                                    // the container to store back
    emitter.instruction("ldr x0, [sp, #32]");                                   // the cell
    emitter.instruction("bl __rt_incref");                                      // the caller's share
    emitter.instruction("b __rt_elem_ref_cell_return");                         // done
    emitter.label("__rt_elem_ref_cell_hash_existing");
    emitter.instruction("ldr x0, [x1, #8]");                                    // the existing marker's cell
    emitter.instruction("str x0, [sp, #32]");                                   // keep it
    emitter.instruction("bl __rt_incref");                                      // the caller's share

    emitter.label("__rt_elem_ref_cell_return");
    emitter.instruction("ldr x0, [sp, #0]");                                    // return the container to store back
    emitter.instruction("ldr x1, [sp, #32]");                                   // return the cell
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return (x0 = container, x1 = cell)
}

fn emit_ref_slot_cell_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ref_slot_cell ---");
    emitter.label_global("__rt_ref_slot_cell");

    // Frame: [rbp-8]=slot address, [rbp-16]=cell.
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 16");                                         // reserve the helper frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // keep the slot address across helper calls
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // the slot's current box
    emitter.instruction("test r10, r10");                                       // a null gap becomes a reference to null
    emitter.instruction("jz __rt_ref_slot_cell_null");                          // make a null box for it
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // inspect the box's tag
    emitter.instruction(&format!("cmp r11, {}", ARRAY_LOCAL_REF_CELL_TAG));     // is the element already a reference?
    emitter.instruction("je __rt_ref_slot_cell_existing");                      // share its cell
    emitter.instruction(&format!("cmp r11, {}", ARRAY_GLOBAL_REF_CELL_TAG));    // is it a reference to a global?
    emitter.instruction("je __rt_ref_slot_cell_existing");                      // share its cell
    emitter.instruction("mov rdi, r10");                                        // the cell takes over the slot's box
    emitter.instruction("jmp __rt_ref_slot_cell_make");                         // wrap it
    emitter.label("__rt_ref_slot_cell_null");
    emitter.instruction("mov rax, 8");                                          // runtime tag 8 = null
    emitter.instruction("xor edi, edi");                                        // null has no payload
    emitter.instruction("xor esi, esi");                                        // null has no high word
    emitter.instruction("call __rt_mixed_from_value");                          // rax = a null box for the cell
    emitter.instruction("mov rdi, rax");                                        // the cell's value
    emitter.label("__rt_ref_slot_cell_make");
    emitter.instruction("mov esi, 7");                                          // the cell holds a boxed Mixed handle
    emitter.instruction("call __rt_ref_cell_alloc");                            // rax = new cell; its one share is the marker's
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // keep the cell across the marker allocation
    emitter.instruction("mov rdi, rax");                                        // the marker's payload is the cell
    emitter.instruction(&format!("mov eax, {}", ARRAY_LOCAL_REF_CELL_TAG));     // marker tag: the element owns a share of a cell
    emitter.instruction("mov esi, 7");                                          // the cell's value is a boxed Mixed handle
    emitter.instruction("call __rt_mixed_from_value");                          // rax = the marker box (tag 13 retains nothing)
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the slot address
    emitter.instruction("mov QWORD PTR [r10], rax");                            // the element IS the reference from now on
    emitter.instruction("jmp __rt_ref_slot_cell_retain");                       // hand the caller its own share
    emitter.label("__rt_ref_slot_cell_existing");
    emitter.instruction("mov r10, QWORD PTR [r10 + 8]");                        // the existing marker's cell
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // keep it across the retain
    emitter.label("__rt_ref_slot_cell_retain");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // the cell to share
    emitter.instruction("call __rt_incref");                                    // the caller's share
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the cell
    emitter.instruction("leave");                                               // release the helper frame
    emitter.instruction("ret");                                                 // return the cell in rax
}

fn emit_elem_ref_cell_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: elem_ref_cell ---");
    emitter.label_global("__rt_elem_ref_cell");

    // Frame: [rbp-8]=container, [rbp-16]=key_lo, [rbp-24]=key_hi, [rbp-32]=box, [rbp-40]=cell.
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // reserve the helper frame, keeping calls 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // keep the container across helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // keep key_lo across helper calls
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // keep key_hi across helper calls

    emitter.label("__rt_elem_ref_cell_dispatch");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the current container
    emitter.instruction("mov r10, QWORD PTR [rdi - 8]");                        // load its packed kind word
    emitter.instruction("mov r11, r10");                                        // copy it to isolate the storage kind
    emitter.instruction("and r11, 0xff");                                       // the storage kind
    emitter.instruction("cmp r11, 3");                                          // kind 3 = hash storage
    emitter.instruction("je __rt_elem_ref_cell_hash");                          // bind a hash bucket

    emitter.instruction("shr r10, 8");                                          // move the element value tag down
    emitter.instruction("and r10, 0x7f");                                       // the element value tag
    emitter.instruction("cmp r10, 7");                                          // are the slots already boxed?
    emitter.instruction("je __rt_elem_ref_cell_indexed_boxed");                 // a marker fits them as they are
    emitter.instruction("mov rsi, r10");                                        // the current element value tag
    emitter.instruction("call __rt_array_to_mixed");                            // rax = a unique copy with boxed slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // the container to store back
    emitter.label("__rt_elem_ref_cell_indexed_boxed");
    emitter.instruction("cmp QWORD PTR [rbp - 24], -1");                        // is the key an integer?
    emitter.instruction("jne __rt_elem_ref_cell_to_hash");                      // a string key needs hash storage
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the container
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the integer key
    emitter.instruction("cmp rsi, 0");                                          // a negative key is not a slot
    emitter.instruction("jl __rt_elem_ref_cell_indexed_vivify");                // create it as null first
    emitter.instruction("cmp rsi, QWORD PTR [rdi]");                            // is the key past the last element?
    emitter.instruction("jge __rt_elem_ref_cell_indexed_vivify");               // create it as null first
    emitter.instruction("call __rt_array_ensure_unique");                       // split a shared container before its slot changes
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // the container to store back
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the integer key
    emitter.instruction("lea rdi, [rax + 24 + rsi * 8]");                       // the element's boxed slot
    emitter.instruction("call __rt_ref_slot_cell");                             // rax = the element's cell, retained for the caller
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // keep it for the return
    emitter.instruction("jmp __rt_elem_ref_cell_return");                       // done

    emitter.label("__rt_elem_ref_cell_indexed_vivify");
    emitter.instruction("mov rax, 8");                                          // runtime tag 8 = null
    emitter.instruction("xor edi, edi");                                        // null has no payload
    emitter.instruction("xor esi, esi");                                        // null has no high word
    emitter.instruction("call __rt_mixed_from_value");                          // rax = the null the missing element starts as
    emitter.instruction("mov rdx, rax");                                        // the setter consumes it
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the container
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the integer key
    emitter.instruction("call __rt_array_set_mixed");                           // append, or promote to a hash for a sparse key
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // the container to store back
    emitter.instruction("jmp __rt_elem_ref_cell_dispatch");                     // the element exists now

    emitter.label("__rt_elem_ref_cell_to_hash");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the boxed indexed container
    emitter.instruction("call __rt_array_to_hash");                             // rax = an owned hash with the same entries
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // keep it across the release
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // the superseded indexed container
    emitter.instruction("call __rt_decref_array");                              // its entries are retained by the hash
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // the hash replaces it
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // the container to store back
    emitter.instruction("jmp __rt_elem_ref_cell_dispatch");                     // bind the hash bucket

    emitter.label("__rt_elem_ref_cell_hash");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload key_lo
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload key_hi
    emitter.instruction("call __rt_hash_get");                                  // rax=found, rdi=value_lo, rsi=value_hi, rcx=value_tag
    emitter.instruction("test rax, rax");                                       // is the key present?
    emitter.instruction("jz __rt_elem_ref_cell_hash_null");                     // a missing key becomes a reference to null
    emitter.instruction("cmp rcx, 7");                                          // is the entry a box?
    emitter.instruction("jne __rt_elem_ref_cell_hash_typed");                   // a raw entry is boxed first
    emitter.instruction("test rdi, rdi");                                       // a legacy null box pointer is null
    emitter.instruction("jz __rt_elem_ref_cell_hash_null");                     // make a null box for it
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // inspect the box's tag
    emitter.instruction(&format!("cmp r10, {}", ARRAY_LOCAL_REF_CELL_TAG));     // is the element already a reference?
    emitter.instruction("je __rt_elem_ref_cell_hash_existing");                 // share its cell
    emitter.instruction(&format!("cmp r10, {}", ARRAY_GLOBAL_REF_CELL_TAG));    // is it a reference to a global?
    emitter.instruction("je __rt_elem_ref_cell_hash_existing");                 // share its cell
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // the cell takes a reference to the entry's box
    emitter.instruction("mov rax, rdi");                                        // the setter below releases the bucket's own reference
    emitter.instruction("call __rt_incref");                                    // so the cell needs one of its own
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // the box the cell will hold
    emitter.instruction("jmp __rt_elem_ref_cell_hash_make");                    // wrap it
    emitter.label("__rt_elem_ref_cell_hash_typed");
    emitter.instruction("mov rax, rcx");                                        // box the raw entry under its own tag
    emitter.instruction("call __rt_mixed_from_value");                          // rax = a box retaining the payload
    emitter.instruction("mov rdi, rax");                                        // the box the cell will hold
    emitter.instruction("jmp __rt_elem_ref_cell_hash_make");                    // wrap it
    emitter.label("__rt_elem_ref_cell_hash_null");
    emitter.instruction("mov rax, 8");                                          // runtime tag 8 = null
    emitter.instruction("xor edi, edi");                                        // null has no payload
    emitter.instruction("xor esi, esi");                                        // null has no high word
    emitter.instruction("call __rt_mixed_from_value");                          // rax = a null box for the cell
    emitter.instruction("mov rdi, rax");                                        // the box the cell will hold
    emitter.label("__rt_elem_ref_cell_hash_make");
    emitter.instruction("mov esi, 7");                                          // the cell holds a boxed Mixed handle
    emitter.instruction("call __rt_ref_cell_alloc");                            // rax = new cell; its one share is the marker's
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // keep the cell
    emitter.instruction("mov rdi, rax");                                        // the marker's payload is the cell
    emitter.instruction(&format!("mov eax, {}", ARRAY_LOCAL_REF_CELL_TAG));     // marker tag: the element owns a share of a cell
    emitter.instruction("mov esi, 7");                                          // the cell's value is a boxed Mixed handle
    emitter.instruction("call __rt_mixed_from_value");                          // rax = the marker box (tag 13 retains nothing)
    emitter.instruction("mov rcx, rax");                                        // the bucket's new value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the hash
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload key_lo
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload key_hi
    emitter.instruction("xor r8d, r8d");                                        // a box has no high word
    emitter.instruction("mov r9d, 7");                                          // runtime tag 7 = boxed Mixed
    emitter.instruction("call __rt_hash_set");                                  // splits a shared hash, releases the replaced value
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // the container to store back
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // the cell
    emitter.instruction("call __rt_incref");                                    // the caller's share
    emitter.instruction("jmp __rt_elem_ref_cell_return");                       // done
    emitter.label("__rt_elem_ref_cell_hash_existing");
    emitter.instruction("mov rax, QWORD PTR [rdi + 8]");                        // the existing marker's cell
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // keep it
    emitter.instruction("call __rt_incref");                                    // the caller's share

    emitter.label("__rt_elem_ref_cell_return");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the container to store back
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // return the cell
    emitter.instruction("leave");                                               // release the helper frame
    emitter.instruction("ret");                                                 // return (rax = container, rdx = cell)
}
