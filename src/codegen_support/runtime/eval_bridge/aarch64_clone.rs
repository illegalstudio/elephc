//! Purpose:
//! Emits the AArch64 boxed-object shallow-clone wrapper.
//!
//! Called from:
//! - The eval bridge runtime facade and sibling bridge emitters.
//!
//! Key details:
//! - Runtime-managed payload rejection and Mixed boxing preserve ownership.
//! - The Mixed cell is built by hand so the allocator's single clone reference is
//!   TRANSFERRED into it. Boxing through `__rt_mixed_from_value` would retain a second
//!   owner that nothing releases, and compensating for that with a refcount decrement
//!   would mutate an object refcount by hand. Neither is needed: the cell simply adopts
//!   the reference the allocation already produced.

use super::*;

/// Emits the ARM64 native wrapper for cloning boxed object cells.
pub(super) fn emit_aarch64_object_clone_shallow_wrapper(emitter: &mut Emitter, native_validation: bool) {
    emitter.label_global("__rt_object_clone_shallow_boxed");
    emitter.instruction("sub sp, sp, #80");                                     // reserve source, clone, descriptor, counters, and wrapper frame slots
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address across clone helper calls
    emitter.instruction("add x29, sp, #64");                                    // establish a stable clone wrapper frame pointer
    emitter.instruction("cbz x0, __rt_object_clone_shallow_boxed_null");        // null handles cannot be cloned as objects
    emitter.instruction("ldr x9, [x0]");                                        // load the boxed Mixed runtime tag
    emitter.instruction("cmp x9, #6");                                          // tag 6 = object
    emitter.instruction("b.ne __rt_object_clone_shallow_boxed_null");           // non-object values cannot be cloned by this bridge
    emitter.instruction("ldr x9, [x0, #8]");                                    // load the object payload pointer
    emit_branch_if_null_container(
        emitter,
        "x9",
        "x10",
        "__rt_object_clone_shallow_boxed_null",
    );
    emitter.instruction("str x9, [sp, #0]");                                    // save the source object payload pointer
    emitter.instruction("ldr x11, [x9]");                                       // load the object's runtime class id
    emitter.instruction("str x11, [sp, #56]");                                  // save class id across allocation and ownership calls
    emit_aarch64_reject_runtime_managed_clone_classes(emitter, "x11", "__rt_object_clone_shallow_boxed_null");
    abi::emit_symbol_address(emitter, "x10", "_class_gc_desc_count");
    emitter.instruction("ldr x10, [x10]");                                      // load the number of emitted class descriptors
    emitter.instruction("cmp x11, x10");                                        // is this class id inside the descriptor table?
    emitter.instruction("b.hs __rt_object_clone_shallow_boxed_null");           // unknown class layouts cannot be cloned by the eval bridge
    abi::emit_symbol_address(emitter, "x10", "_class_gc_desc_ptrs");
    emitter.instruction("lsl x12, x11, #3");                                    // scale class id to an 8-byte descriptor pointer slot
    emitter.instruction("ldr x10, [x10, x12]");                                 // load the class property-tag descriptor pointer
    emitter.instruction("str x10, [sp, #16]");                                  // save descriptor pointer for the property-copy loop
    abi::emit_symbol_address(emitter, "x10", "_class_object_payload_sizes");
    emitter.instruction("ldr x12, [x10, x12]");                                 // load the class-declared object payload size
    emitter.instruction("str x12, [sp, #48]");                                  // save payload size for allocation and dyn-prop offsets
    abi::emit_symbol_address(emitter, "x10", "_class_object_allocation_sizes");
    emitter.instruction("ldr x0, [x10, x11, lsl #3]");                          // include ordinary Throwable construction metadata in the allocation
    elephc_memoir::values::object_storage::emit_allocate_object(emitter);
    emitter.instruction("ldr x11, [sp, #56]");                                  // reload the source class id
    emitter.instruction("str x11, [x0]");                                       // store the class id at the clone payload head
    emitter.instruction("str x0, [sp, #8]");                                    // save the clone object payload pointer
    abi::emit_symbol_address(emitter, "x10", "_class_throwable_line_offsets");
    emitter.instruction("ldr x13, [x10, x11, lsl #3]");                         // locate optional ordinary Throwable construction metadata
    emitter.instruction("cbz x13, __rt_clone_line_copied");                     // skip the metadata copy for non-Throwable classes
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the source object independently of its physical layout
    super::super::exceptions::emit_creation_line_load(emitter, "x9", "x0", "x10", "__rt_clone_source");
    emitter.instruction("ldr x11, [sp, #8]");                                   // recover the clone pointer after reading the source line
    emitter.instruction("str x0, [x11, x13]");                                  // copy metadata outside the clone's PHP property region
    emitter.label("__rt_clone_line_copied");
    emitter.instruction("ldr x12, [sp, #48]");                                  // reload the payload size
    emitter.instruction("sub x12, x12, #8");                                    // remove the leading class id field from the clone layout
    emitter.instruction("ldr x13, [sp, #56]");                                  // reload the source class id for dynamic-property metadata
    emitter.instruction("lsl x13, x13, #3");                                    // scale class id to an 8-byte dynamic-flag slot
    abi::emit_symbol_address(emitter, "x10", "_class_object_dynamic_prop_flags");
    emitter.instruction("ldr x13, [x10, x13]");                                 // load whether this class layout has a dyn-props tail
    emitter.instruction("cbz x13, __rt_object_clone_shallow_boxed_count_ready"); // no dyn-props tail contributes to property count
    emitter.instruction("sub x12, x12, #8");                                    // remove the dyn-props tail before counting declared slots
    emitter.label("__rt_object_clone_shallow_boxed_count_ready");
    emitter.instruction("lsr x12, x12, #4");                                    // derive declared-property slot count from the payload size
    emitter.instruction("str x12, [sp, #24]");                                  // save property count for the copy loop
    emitter.instruction("str xzr, [sp, #32]");                                  // initialize property-copy index to zero

    elephc_memoir::values::object_clone_slots::emit_tagged_slots(emitter);

    emitter.label("__rt_object_clone_shallow_boxed_dyn");
    emitter.instruction("ldr x12, [sp, #56]");                                  // reload the source class id for dynamic-property metadata
    emitter.instruction("lsl x12, x12, #3");                                    // scale class id to an 8-byte dynamic-flag slot
    abi::emit_symbol_address(emitter, "x10", "_class_object_dynamic_prop_flags");
    emitter.instruction("ldr x12, [x10, x12]");                                 // load whether this class layout has a dyn-props tail
    emitter.instruction("cbz x12, __rt_object_clone_shallow_boxed_box");        // no dynamic hash slot: box the copied clone
    emitter.instruction("ldr x13, [sp, #48]");                                  // reload the class-declared payload size
    emitter.instruction("sub x13, x13, #8");                                    // compute dyn-props slot offset as payload_size - 8
    emitter.instruction("str x13, [sp, #40]");                                  // save dyn-props slot offset across hash cloning
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the source object pointer
    emitter.instruction("ldr x10, [x9, x13]");                                  // load the source dynamic-property hash pointer
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the clone object pointer
    emitter.instruction("cbz x10, __rt_object_clone_shallow_boxed_dyn_null");   // null source hash stays null on the clone
    emitter.instruction("mov x0, x10");                                         // pass the source dynamic hash to the clone helper
    emitter.instruction("bl __rt_hash_clone_shallow");                          // clone dynamic properties and retain nested values
    emitter.instruction("ldr x13, [sp, #40]");                                  // restore the dynamic-property slot offset
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the clone object pointer after hash cloning
    emitter.instruction("str x0, [x11, x13]");                                  // install the cloned dynamic-property hash
    emitter.instruction("b __rt_object_clone_shallow_boxed_box");               // box the clone after dynamic properties are installed

    emitter.label("__rt_object_clone_shallow_boxed_dyn_null");
    emitter.instruction("str xzr, [x11, x13]");                                 // clear the clone's dynamic-property hash slot

    emitter.label("__rt_object_clone_shallow_boxed_box");
    if native_validation {
        emitter.instruction("ldr x0, [sp, #8]");                                // register the completed clone before a PHP-visible box owns it
        abi::emit_call_label(emitter, "__rt_reference_sources_bind_object");
    }
    emitter.instruction("mov x0, #24");                                         // a Mixed cell stores a runtime tag and two payload words
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the Mixed cell that will own the fresh clone
    emitter.instruction("mov x9, #5");                                          // heap kind 5 marks Mixed cells for the ownership helpers
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the uniform Mixed-cell heap header
    emitter.instruction("mov x9, #6");                                          // runtime tag 6 = object
    emitter.instruction("str x9, [x0]");                                        // store the runtime tag at mixed[0]
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the clone, which still holds the allocator's only reference
    emitter.instruction("str x9, [x0, #8]");                                    // move that reference into the Mixed cell rather than retaining a second one
    emitter.instruction("str xzr, [x0, #16]");                                  // object payloads do not use a high word
    emitter.instruction("b __rt_object_clone_shallow_boxed_done");              // skip the null sentinel after a successful clone

    emitter.label("__rt_object_clone_shallow_boxed_null");
    emitter.instruction("mov x0, xzr");                                         // return a null C pointer for unsupported clone inputs
    emitter.label("__rt_object_clone_shallow_boxed_done");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the clone wrapper frame
    emitter.instruction("ret");                                                 // return the boxed clone or null failure sentinel
}
