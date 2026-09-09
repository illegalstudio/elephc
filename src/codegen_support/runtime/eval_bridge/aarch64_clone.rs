//! Purpose:
//! Emits the AArch64 boxed-object shallow-clone wrapper.
//!
//! Called from:
//! - The eval bridge runtime facade and sibling bridge emitters.
//!
//! Key details:
//! - Runtime-managed payload rejection and Mixed boxing preserve ownership.

use super::*;

/// Emits the ARM64 eval bridge wrapper for cloning boxed object cells.
pub(super) fn emit_aarch64_object_clone_shallow_wrapper(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_value_object_clone_shallow");
    emitter.instruction("sub sp, sp, #80");                                     // reserve source, clone, descriptor, counters, and wrapper frame slots
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address across clone helper calls
    emitter.instruction("add x29, sp, #64");                                    // establish a stable clone wrapper frame pointer
    emitter.instruction("cbz x0, __elephc_eval_value_object_clone_shallow_null"); // null handles cannot be cloned as objects
    emitter.instruction("ldr x9, [x0]");                                        // load the boxed Mixed runtime tag
    emitter.instruction("cmp x9, #6");                                          // tag 6 = object
    emitter.instruction("b.ne __elephc_eval_value_object_clone_shallow_null");  // non-object values cannot be cloned by this bridge
    emitter.instruction("ldr x9, [x0, #8]");                                    // load the object payload pointer
    emit_branch_if_null_container(
        emitter,
        "x9",
        "x10",
        "__elephc_eval_value_object_clone_shallow_null",
    );
    emitter.instruction("str x9, [sp, #0]");                                    // save the source object payload pointer
    emitter.instruction("ldr x11, [x9]");                                       // load the object's runtime class id
    emitter.instruction("str x11, [sp, #56]");                                  // save class id across allocation and ownership calls
    emit_aarch64_reject_runtime_managed_clone_classes(emitter, "x11", "__elephc_eval_value_object_clone_shallow_null");
    abi::emit_symbol_address(emitter, "x10", "_class_gc_desc_count");
    emitter.instruction("ldr x10, [x10]");                                      // load the number of emitted class descriptors
    emitter.instruction("cmp x11, x10");                                        // is this class id inside the descriptor table?
    emitter.instruction("b.hs __elephc_eval_value_object_clone_shallow_null");  // unknown class layouts cannot be cloned by the eval bridge
    abi::emit_symbol_address(emitter, "x10", "_class_gc_desc_ptrs");
    emitter.instruction("lsl x12, x11, #3");                                    // scale class id to an 8-byte descriptor pointer slot
    emitter.instruction("ldr x10, [x10, x12]");                                 // load the class property-tag descriptor pointer
    emitter.instruction("str x10, [sp, #16]");                                  // save descriptor pointer for the property-copy loop
    abi::emit_symbol_address(emitter, "x10", "_class_object_payload_sizes");
    emitter.instruction("ldr x12, [x10, x12]");                                 // load the class-declared object payload size
    emitter.instruction("str x12, [sp, #48]");                                  // save payload size for allocation and dyn-prop offsets
    emitter.instruction("mov x0, x12");                                         // pass the source payload size to the heap allocator
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate a clone object payload with the same byte size
    emitter.instruction("mov x9, #4");                                          // heap kind 4 marks object instances for ownership helpers
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the uniform object heap header
    emitter.instruction("bl __rt_object_handle_acquire");                       // bind the new object to its PHP object handle
    emitter.instruction("ldr x11, [sp, #56]");                                  // reload the source class id
    emitter.instruction("str x11, [x0]");                                       // store the class id at the clone payload head
    emitter.instruction("str x0, [sp, #8]");                                    // save the clone object payload pointer
    emitter.instruction("ldr x12, [sp, #48]");                                  // reload the payload size
    emitter.instruction("sub x12, x12, #8");                                    // remove the leading class id field from the clone layout
    emitter.instruction("ldr x13, [sp, #56]");                                  // reload the source class id for dynamic-property metadata
    emitter.instruction("lsl x13, x13, #3");                                    // scale class id to an 8-byte dynamic-flag slot
    abi::emit_symbol_address(emitter, "x10", "_class_object_dynamic_prop_flags");
    emitter.instruction("ldr x13, [x10, x13]");                                 // load whether this class layout has a dyn-props tail
    emitter.instruction("cbz x13, __elephc_eval_value_object_clone_shallow_count_ready"); // no dyn-props tail contributes to property count
    emitter.instruction("sub x12, x12, #8");                                    // remove the dyn-props tail before counting declared slots
    emitter.label("__elephc_eval_value_object_clone_shallow_count_ready");
    emitter.instruction("lsr x12, x12, #4");                                    // derive declared-property slot count from the payload size
    emitter.instruction("str x12, [sp, #24]");                                  // save property count for the copy loop
    emitter.instruction("str xzr, [sp, #32]");                                  // initialize property-copy index to zero

    emitter.label("__elephc_eval_value_object_clone_shallow_prop_loop");
    emitter.instruction("ldr x12, [sp, #32]");                                  // reload the current property index
    emitter.instruction("ldr x13, [sp, #24]");                                  // reload the declared-property slot count
    emitter.instruction("cmp x12, x13");                                        // has every declared property slot been copied?
    emitter.instruction("b.ge __elephc_eval_value_object_clone_shallow_dyn");   // move on to the optional dynamic-property hash
    emitter.instruction("mov x10, #16");                                        // each declared-property slot is two 8-byte words
    emitter.instruction("mul x10, x12, x10");                                   // compute the byte offset inside the property region
    emitter.instruction("add x10, x10, #8");                                    // skip the leading class id to reach this slot
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the source object pointer
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the clone object pointer
    emitter.instruction("ldr x15, [x9, x10]");                                  // copy the source property low word and keep it for retains
    emitter.instruction("str x15, [x11, x10]");                                 // store the property low word on the clone
    emitter.instruction("add x10, x10, #8");                                    // advance to the high word of the property slot
    emitter.instruction("ldr x14, [x9, x10]");                                  // copy the source property high word
    emitter.instruction("str x14, [x11, x10]");                                 // store the property high word on the clone
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the property-tag descriptor pointer
    emitter.instruction("ldrb w14, [x11, x12]");                                // load the compile-time ownership tag for this slot
    emitter.instruction("cmp x14, #11");                                        // owned property references require PHP reference-aware cloning
    emitter.instruction("b.eq __elephc_eval_value_object_clone_shallow_reference"); // share live aliases and separate singleton cells
    emitter.instruction("cmp x14, #1");                                         // does the slot hold an owned string payload?
    emitter.instruction("b.eq __elephc_eval_value_object_clone_shallow_string"); // string slots need an independent payload copy
    emitter.instruction("cmp x14, #4");                                         // does the slot hold a retained indexed-array payload?
    emitter.instruction("b.eq __elephc_eval_value_object_clone_shallow_retain"); // retained array slots need an extra owner reference
    emitter.instruction("cmp x14, #5");                                         // does the slot hold a retained associative-array payload?
    emitter.instruction("b.eq __elephc_eval_value_object_clone_shallow_retain"); // retained hash slots need an extra owner reference
    emitter.instruction("cmp x14, #6");                                         // does the slot hold a retained object payload?
    emitter.instruction("b.eq __elephc_eval_value_object_clone_shallow_retain"); // retained object slots need an extra owner reference
    emitter.instruction("cmp x14, #7");                                         // does the slot hold a retained boxed Mixed payload?
    emitter.instruction("b.eq __elephc_eval_value_object_clone_shallow_retain"); // retained Mixed slots need an extra owner reference
    emitter.instruction("b __elephc_eval_value_object_clone_shallow_next");     // scalar slots are copied without ownership changes
    emitter.label("__elephc_eval_value_object_clone_shallow_reference");
    emitter.instruction("str x12, [sp, #32]");                                  // preserve the property index across reference-cell cloning
    emitter.instruction("str x10, [sp, #40]");                                  // preserve the high-word property offset
    emitter.instruction("mov x0, x15");                                         // pass the owned reference cell to its clone helper
    emitter.instruction("bl __rt_reference_cell_clone");                        // retain shared aliases or copy singleton cell storage
    emitter.instruction("ldr x10, [sp, #40]");                                  // restore the high-word property offset
    emitter.instruction("sub x10, x10, #8");                                    // locate the clone's cell pointer word
    emitter.instruction("ldr x11, [sp, #8]");                                   // restore the cloned object pointer
    emitter.instruction("str x0, [x11, x10]");                                  // replace the copied slot with its correctly owned cell
    emitter.instruction("ldr x12, [sp, #32]");                                  // restore the property traversal index
    emitter.instruction("b __elephc_eval_value_object_clone_shallow_next");     // continue after reference-aware slot cloning

    emitter.label("__elephc_eval_value_object_clone_shallow_string");
    emitter.instruction("str x12, [sp, #32]");                                  // preserve property index across string persistence
    emitter.instruction("str x10, [sp, #40]");                                  // preserve the high-word slot offset across the helper
    emitter.instruction("mov x1, x15");                                         // pass the source string pointer to the persistence helper
    emitter.instruction("ldr x2, [x9, x10]");                                   // pass the source string length to the persistence helper
    emitter.instruction("bl __rt_str_persist");                                 // duplicate the string payload for clone ownership
    emitter.instruction("ldr x10, [sp, #40]");                                  // restore the high-word slot offset after persistence
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the clone object pointer after persistence
    emitter.instruction("sub x10, x10, #8");                                    // move back to the low-word string pointer slot
    emitter.instruction("str x1, [x11, x10]");                                  // install the persisted string pointer on the clone
    emitter.instruction("add x10, x10, #8");                                    // move to the high-word string length slot
    emitter.instruction("str x2, [x11, x10]");                                  // install the persisted string length on the clone
    emitter.instruction("ldr x12, [sp, #32]");                                  // restore property index after string persistence
    emitter.instruction("b __elephc_eval_value_object_clone_shallow_next");     // continue with the next declared property

    emitter.label("__elephc_eval_value_object_clone_shallow_retain");
    emitter.instruction("str x12, [sp, #32]");                                  // preserve property index across the retain helper
    emitter.instruction("mov x0, x15");                                         // pass the copied heap payload to the retain helper
    emitter.instruction("bl __rt_incref");                                      // retain the shared property payload for the cloned object
    emitter.instruction("ldr x12, [sp, #32]");                                  // restore property index after the retain helper

    emitter.label("__elephc_eval_value_object_clone_shallow_next");
    emitter.instruction("add x12, x12, #1");                                    // advance to the next declared-property slot
    emitter.instruction("str x12, [sp, #32]");                                  // save the advanced property-copy index
    emitter.instruction("b __elephc_eval_value_object_clone_shallow_prop_loop"); // continue copying declared properties

    emitter.label("__elephc_eval_value_object_clone_shallow_dyn");
    emitter.instruction("ldr x12, [sp, #56]");                                  // reload the source class id for dynamic-property metadata
    emitter.instruction("lsl x12, x12, #3");                                    // scale class id to an 8-byte dynamic-flag slot
    abi::emit_symbol_address(emitter, "x10", "_class_object_dynamic_prop_flags");
    emitter.instruction("ldr x12, [x10, x12]");                                 // load whether this class layout has a dyn-props tail
    emitter.instruction("cbz x12, __elephc_eval_value_object_clone_shallow_box"); // no dynamic hash slot: box the copied clone
    emitter.instruction("ldr x13, [sp, #48]");                                  // reload the class-declared payload size
    emitter.instruction("sub x13, x13, #8");                                    // compute dyn-props slot offset as payload_size - 8
    emitter.instruction("str x13, [sp, #40]");                                  // save dyn-props slot offset across hash cloning
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the source object pointer
    emitter.instruction("ldr x10, [x9, x13]");                                  // load the source dynamic-property hash pointer
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the clone object pointer
    emitter.instruction("cbz x10, __elephc_eval_value_object_clone_shallow_dyn_null"); // null source hash stays null on the clone
    emitter.instruction("mov x0, x10");                                         // pass the source dynamic hash to the clone helper
    emitter.instruction("bl __rt_hash_clone_shallow");                          // clone dynamic properties and retain nested values
    emitter.instruction("ldr x13, [sp, #40]");                                  // restore the dynamic-property slot offset
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the clone object pointer after hash cloning
    emitter.instruction("str x0, [x11, x13]");                                  // install the cloned dynamic-property hash
    emitter.instruction("b __elephc_eval_value_object_clone_shallow_box");      // box the clone after dynamic properties are installed

    emitter.label("__elephc_eval_value_object_clone_shallow_dyn_null");
    emitter.instruction("str xzr, [x11, x13]");                                 // clear the clone's dynamic-property hash slot

    emitter.label("__elephc_eval_value_object_clone_shallow_box");
    emitter.instruction("ldr x1, [sp, #8]");                                    // move the cloned object pointer into the Mixed payload
    emitter.instruction("mov x0, #6");                                          // runtime tag 6 = object
    emitter.instruction("mov x2, xzr");                                         // object payloads do not use a high word
    emitter.instruction("bl __rt_mixed_from_value");                            // box the cloned object for Rust
    emitter.instruction("b __elephc_eval_value_object_clone_shallow_done");     // skip the null sentinel after a successful clone

    emitter.label("__elephc_eval_value_object_clone_shallow_null");
    emitter.instruction("mov x0, xzr");                                         // return a null C pointer for unsupported clone inputs
    emitter.label("__elephc_eval_value_object_clone_shallow_done");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the clone wrapper frame
    emitter.instruction("ret");                                                 // return the boxed clone or null failure sentinel
}
