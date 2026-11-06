//! Purpose:
//! Emits the aarch64 consuming string append implementation.
//!
//! Called from:
//! - `super::emit_str_append` for matching supported targets.
//!
//! Key details:
//! - Header size is capacity, logical length stays in the returned string pair.
//! - The old owner survives all allocation and copying until the replacement is ready.

use crate::codegen_support::emit::Emitter;

/// Emits the target ABI helper while preserving frame alignment across nested calls.
pub(super) fn emit(emitter: &mut Emitter, mbstring: bool) {
    emitter.blank();
    emitter.comment("--- runtime: consuming string append ---");
    // -- set up the frame and preserve the input owner --
    emitter.label_global("__rt_str_append");
    emitter.instruction("sub sp, sp, #96");                                     // reserve an aligned frame for arguments, ownership and capacity
    emitter.instruction("stp x29, x30, [sp, #80]");                             // preserve frame pointer and link register
    emitter.instruction("add x29, sp, #80");                                    // establish the helper frame pointer
    emitter.instruction("stp x1, x2, [sp, #0]");                                // preserve the old owner and logical length
    emitter.instruction("stp x3, x4, [sp, #16]");                               // preserve the borrowed RHS string pair
    emitter.instruction("str x1, [sp, #40]");                                   // initially return the original payload
    // -- validate the new length before reading payload bytes --
    emitter.instruction("adds x5, x2, x4");                                     // compute the replacement length and record unsigned wrap
    emitter.instruction("b.cs __rt_str_append_overflow");                       // reject length wrap before any payload access
    emitter.instruction("mov x9, #0xfffffff8");                                 // largest aligned payload representable by the heap header
    emitter.instruction("cmp x5, x9");                                          // enforce the allocator's size representation
    emitter.instruction("b.hi __rt_str_append_overflow");                       // reject an unrepresentable replacement length
    emitter.instruction("str x5, [sp, #32]");                                   // preserve the replacement logical length
    emitter.instruction("cbz x4, __rt_str_append_done");                        // empty append preserves both ownership and origin metadata
    emitter.instruction("str xzr, [sp, #56]");                                  // nonowned inputs have no reusable capacity
    emitter.instruction("mov x0, x1");                                          // classify the old pointer through the validating heap helper
    // -- check ownership, sharing and RHS overlap --
    emitter.instruction("bl __rt_heap_kind");                                   // return the uniform heap kind
    emitter.instruction("str x0, [sp, #48]");                                   // remember whether an old owned string must be retired
    emitter.instruction("cmp x0, #1");                                          // mutation is restricted to ordinary owned strings
    emitter.instruction("b.ne __rt_str_append_allocate");                       // copy literals, scratch and borrowed values
    emitter.instruction("ldr x1, [sp, #0]");                                    // recover the old payload pointer
    emitter.instruction("ldr w9, [x1, #-16]");                                  // load its allocator capacity
    emitter.instruction("str x9, [sp, #56]");                                   // preserve capacity for geometric growth
    emitter.instruction("ldr w10, [x1, #-12]");                                 // inspect the runtime reference count
    emitter.instruction("cmp w10, #1");                                         // require unique ownership before mutation
    emitter.instruction("b.ne __rt_str_append_copy");                           // shared copies must base growth on logical length
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // recover the RHS interval
    emitter.instruction("adds x7, x3, x4");                                     // compute its exclusive end
    emitter.instruction("b.cs __rt_str_append_overflow");                       // reject an input interval that wraps the address space
    emitter.instruction("add x6, x1, x9");                                      // compute the old allocation's exclusive end
    emitter.instruction("cmp x3, x6");                                          // does RHS start beyond that allocation?
    emitter.instruction("b.hs __rt_str_append_capacity");                       // a disjoint RHS permits in-place append
    emitter.instruction("cmp x7, x1");                                          // does RHS end before the old allocation starts?
    emitter.instruction("b.hi __rt_str_append_copy");                           // overlapping inputs need independent copying
    // -- reuse spare capacity only for a unique disjoint owner --
    emitter.label("__rt_str_append_capacity");
    emitter.instruction("ldr x5, [sp, #32]");                                   // recover the replacement logical length
    emitter.instruction("cmp x5, x9");                                          // test the available spare capacity
    emitter.instruction("b.hi __rt_str_append_allocate");                       // grow when the unique buffer is too small
    emitter.instruction("str xzr, [sp, #48]");                                  // reused unique ownership must not be decremented
    emitter.instruction("b __rt_str_append_prepare");                           // detach metadata before changing the existing payload
    // -- choose geometric capacity and allocate before retiring the owner --
    emitter.label("__rt_str_append_copy");
    emitter.instruction("ldr x9, [sp, #8]");                                    // bound copied storage by used bytes rather than old spare capacity
    emitter.instruction("str x9, [sp, #56]");                                   // avoid exponential reservation under repeated sharing
    emitter.label("__rt_str_append_allocate");
    emitter.instruction("ldr x0, [sp, #56]");                                   // seed growth from prior capacity
    emitter.instruction("lsl x0, x0, #1");                                      // double capacity for amortized linear copying
    emitter.instruction("mov x9, #0xfffffff8");                                 // materialize the maximum aligned heap payload
    emitter.instruction("cmp x0, x9");                                          // check whether geometric growth fits the size header
    emitter.instruction("b.ls __rt_str_append_choose_capacity");                // retain representable doubled capacity
    emitter.instruction("ldr x0, [sp, #32]");                                   // near the size limit reserve the exact checked requirement
    emitter.label("__rt_str_append_choose_capacity");
    emitter.instruction("ldr x5, [sp, #32]");                                   // recover the required replacement size
    emitter.instruction("cmp x0, x5");                                          // compare growth with the actual requirement
    emitter.instruction("csel x0, x0, x5, hs");                                 // reserve the larger checked capacity
    emitter.instruction("mov x9, #16");                                         // establish the minimum geometric capacity
    emitter.instruction("cmp x0, x9");                                          // avoid repeated tiny initial allocations
    emitter.instruction("csel x0, x0, x9, hs");                                 // reserve at least sixteen bytes
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate before consuming the old local owner
    emitter.instruction("mov x9, #1");                                          // identify an ordinary owned PHP string
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the new allocation's uniform heap kind
    emitter.instruction("str x0, [sp, #40]");                                   // preserve the replacement payload across helper calls
    // -- detach lazy metadata before mutating the result bytes --
    emitter.label("__rt_str_append_prepare");
    if mbstring {
        emitter.instruction("ldr x1, [sp, #40]");                               // select the changed string identity before any byte store
        emitter.instruction("ldr x2, [sp, #32]");                               // provide its complete new logical length without reading bytes
        emitter.instruction("bl __rt_mbstring_ini_fresh");                      // detach the old lazy origin before payload mutation
    }
    emitter.instruction("ldr x10, [sp, #40]");                                  // recover the destination pointer after optional metadata calls
    emitter.instruction("ldr x1, [sp, #0]");                                    // recover the old payload pointer for the reuse test
    emitter.instruction("cmp x10, x1");                                         // distinguish unique reuse from independent replacement
    emitter.instruction("b.ne __rt_str_append_prefix_setup");                   // new storage needs the old prefix copied first
    emitter.instruction("ldr x2, [sp, #8]");                                    // recover the preserved prefix length for unique reuse
    emitter.instruction("add x10, x10, x2");                                    // start mutation after the preserved prefix
    emitter.instruction("b __rt_str_append_right_setup");                       // only RHS bytes need writing for unique reuse
    // -- copy the prefix when a replacement allocation is required --
    emitter.label("__rt_str_append_prefix_setup");
    emitter.instruction("ldp x1, x2, [sp, #0]");                                // recover the old prefix source and logical length
    emitter.label("__rt_str_append_left");
    emitter.instruction("cbz x2, __rt_str_append_right_setup");                 // move to RHS after the old prefix is complete
    emitter.instruction("ldrb w11, [x1], #1");                                  // load the next prefix byte and advance its source
    emitter.instruction("strb w11, [x10], #1");                                 // initialize that replacement byte and advance destination
    emitter.instruction("sub x2, x2, #1");                                      // consume one prefix byte
    emitter.instruction("b __rt_str_append_left");                              // continue copying the old prefix
    // -- append the complete borrowed RHS including embedded NUL bytes --
    emitter.label("__rt_str_append_right_setup");
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // recover the borrowed RHS source and length
    emitter.label("__rt_str_append_right");
    emitter.instruction("cbz x4, __rt_str_append_publish");                     // publish after the entire result is initialized
    emitter.instruction("ldrb w11, [x3], #1");                                  // load the next RHS byte including embedded NULs
    emitter.instruction("strb w11, [x10], #1");                                 // append that byte and advance destination
    emitter.instruction("sub x4, x4, #1");                                      // consume one RHS byte
    emitter.instruction("b __rt_str_append_right");                             // continue copying new bytes
    // -- retire the consumed owner only after the replacement is complete --
    emitter.label("__rt_str_append_publish");
    emitter.instruction("ldr x9, [sp, #48]");                                   // determine whether an old managed owner was replaced
    emitter.instruction("cmp x9, #1");                                          // only a copied owned string contributes a reference to retire
    emitter.instruction("b.ne __rt_str_append_done");                           // skip release for reused or nonowned inputs
    emitter.instruction("ldr x0, [sp, #0]");                                    // recover the original managed string owner
    emitter.instruction("bl __rt_decref_any");                                  // retire exactly that reference after replacement succeeds
    // -- return the replacement pair and restore the caller frame --
    emitter.label("__rt_str_append_done");
    emitter.instruction("ldr x1, [sp, #40]");                                   // return the reused or replacement pointer
    emitter.instruction("ldr x2, [sp, #32]");                                   // return the checked logical byte length
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore the caller frame and link register
    emitter.instruction("add sp, sp, #96");                                     // release aligned helper spill storage
    emitter.instruction("ret");                                                 // return the replacement owner for its adjacent local store
    // -- forward invalid lengths to the allocation failure boundary --
    emitter.label("__rt_str_append_overflow");
    emitter.instruction("b __rt_alloc_overflow");                               // enter the existing fatal or recoverable allocation boundary
}
